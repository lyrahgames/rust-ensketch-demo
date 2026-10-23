//! One winit loop, one main-thread viewer, and one worker-owned viewer.
#![recursion_limit = "512"]
use engine::{Aabb, Camera, Executor, GpuContext, ObjViewer, RenderStatus, Scene};
use script_lua::LuaRuntime;
use script_rhai::RhaiRuntime;
use std::{
  cell::RefCell,
  collections::HashMap,
  io::BufRead,
  rc::Rc,
  sync::Arc,
  sync::mpsc,
  time::{Duration, Instant},
};
use winit::{
  application::ApplicationHandler,
  dpi::PhysicalSize,
  event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
  event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
  window::{Window, WindowId},
};

type MainTask = Box<dyn FnOnce(&mut ThreadedApp, &ActiveEventLoop) + Send>;
struct WorkerState {
  viewer:             Rc<RefCell<ObjViewer>>,
  camera:             Rc<RefCell<Camera>>,
  lua:                LuaRuntime,
  rhai:               RhaiRuntime,
  proxy:              EventLoopProxy<MainTask>,
  recovery_requested: bool,
  background:         [f32; 4],
}
impl WorkerState {
  fn eval(&mut self, language: &str, source: &str) -> String {
    match language {
      "lua" => self
        .lua
        .eval(source)
        .map(|_| "ok".to_owned())
        .unwrap_or_else(|e| e.to_string()),
      "rhai" => self
        .rhai
        .eval(source)
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|e| e.to_string()),
      _ => format!("unknown language: {language}"),
    }
  }
  fn render(&mut self) {
    let status = self
      .viewer
      .borrow_mut()
      .render(&self.camera.borrow(), self.background);
    if let Err(error) = &status {
      eprintln!("worker render: {error}");
    }
    if matches!(status, Ok(RenderStatus::SurfaceLost)) && !self.recovery_requested {
      self.recovery_requested = true;
      let _ = self.proxy.send_event(Box::new(|app, _| {
        let Some(window) = &app.worker_window else {
          return;
        };
        let size = window.inner_size();
        match app.context.instance().create_surface(window.clone()) {
          Ok(surface) => {
            if let Some(worker) = &app.worker {
              let _ = worker.spawn(move |state| {
                if let Err(error) =
                  state
                    .viewer
                    .borrow_mut()
                    .attach_surface(surface, size.width, size.height)
                {
                  eprintln!("worker surface recovery: {error}");
                }
                state.recovery_requested = false;
              });
            }
          }
          Err(error) => eprintln!("worker surface recovery: {error}"),
        }
      }));
    }
  }
}

struct ThreadedApp {
  context:       GpuContext,
  scene:         Scene,
  camera:        Rc<RefCell<Camera>>,
  main_window:   Option<Arc<Window>>,
  worker_window: Option<Arc<Window>>,
  main_viewer:   Option<Rc<RefCell<ObjViewer>>>,
  main_lua:      Option<LuaRuntime>,
  main_rhai:     Option<RhaiRuntime>,
  worker:        Option<Executor<WorkerState>>,
  proxy:         EventLoopProxy<MainTask>,
  last:          HashMap<WindowId, (f32, f32)>,
  buttons:       HashMap<WindowId, [bool; 3]>,
  next_frame:    Instant,
}
impl ThreadedApp {
  fn eval(&mut self, language: &str, source: &str) -> String {
    match language {
      "lua" => self
        .main_lua
        .as_ref()
        .unwrap()
        .eval(source)
        .map(|_| "ok".to_owned())
        .unwrap_or_else(|e| e.to_string()),
      "rhai" => self
        .main_rhai
        .as_mut()
        .unwrap()
        .eval(source)
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|e| e.to_string()),
      _ => format!("unknown language: {language}"),
    }
  }
}
impl ApplicationHandler<MainTask> for ThreadedApp {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.main_window.is_some() {
      return;
    }
    let main_window = Arc::new(
      event_loop
        .create_window(
          Window::default_attributes()
            .with_title("Main executor viewer")
            .with_inner_size(PhysicalSize::new(640, 480)),
        )
        .unwrap(),
    );
    let worker_window = Arc::new(
      event_loop
        .create_window(
          Window::default_attributes()
            .with_title("Worker executor viewer")
            .with_inner_size(PhysicalSize::new(800, 400)),
        )
        .unwrap(),
    );
    let main_surface = self
      .context
      .instance()
      .create_surface(main_window.clone())
      .unwrap();
    let worker_surface = self
      .context
      .instance()
      .create_surface(worker_window.clone())
      .unwrap();
    let mut main_viewer = ObjViewer::new(self.context.clone());
    let meshes = self
      .scene
      .meshes()
      .iter()
      .map(|mesh| engine::DeviceTriangleMesh::from_triangle_mesh(&self.context, mesh).map(Arc::new))
      .collect::<engine::Result<Vec<_>>>()
      .unwrap();
    main_viewer.set_device_meshes(meshes);
    main_viewer.attach_surface(main_surface, 640, 480).unwrap();
    let main_viewer = Rc::new(RefCell::new(main_viewer));
    let main_lua = LuaRuntime::new().unwrap();
    main_lua
      .expose_viewer("viewer", main_viewer.clone())
      .unwrap();
    main_lua
      .expose_camera("camera", self.camera.clone())
      .unwrap();
    main_lua
      .expose_context("context", self.context.clone())
      .unwrap();
    main_lua.expose_scene("scene", self.scene.clone()).unwrap();
    let mut main_rhai = RhaiRuntime::new();
    main_rhai.expose_viewer("viewer", main_viewer.clone());
    main_rhai.expose_camera("camera", self.camera.clone());
    main_rhai.expose_context("context", self.context.clone());
    main_rhai.expose_scene("scene", self.scene.clone());
    self.main_viewer = Some(main_viewer);
    self.main_lua = Some(main_lua);
    self.main_rhai = Some(main_rhai);
    self.main_window = Some(main_window);
    self.worker_window = Some(worker_window);

    let context = self.context.clone();
    let scene = self.scene.clone();
    let proxy = self.proxy.clone();
    let worker = Executor::new_worker_with(move || {
      let mut viewer = ObjViewer::new(context.clone());
      let meshes = scene
        .meshes()
        .iter()
        .map(|mesh| engine::DeviceTriangleMesh::from_triangle_mesh(&context, mesh).map(Arc::new))
        .collect::<engine::Result<Vec<_>>>()
        .unwrap();
      viewer.set_device_meshes(meshes);
      viewer.attach_surface(worker_surface, 800, 400).unwrap();
      let viewer = Rc::new(RefCell::new(viewer));
      let mut camera = Camera::default();
      camera.frame_aabb(Aabb::from(&scene));
      let camera = Rc::new(RefCell::new(camera));
      let lua = LuaRuntime::new().unwrap();
      lua.expose_viewer("viewer", viewer.clone()).unwrap();
      lua.expose_camera("camera", camera.clone()).unwrap();
      lua.expose_context("context", context.clone()).unwrap();
      lua.expose_scene("scene", scene.clone()).unwrap();
      let mut rhai = RhaiRuntime::new();
      rhai.expose_viewer("viewer", viewer.clone());
      rhai.expose_camera("camera", camera.clone());
      rhai.expose_context("context", context);
      rhai.expose_scene("scene", scene);
      let _ = proxy.send_event(Box::new(|app, _| {
        if let Some(window) = &app.main_window {
          window.set_title("Main viewer — title changed by worker closure");
          let _ = window.request_inner_size(PhysicalSize::new(700, 500));
        }
      }));
      println!("worker viewer thread: {:?}", std::thread::current().id());
      WorkerState {
        viewer,
        camera,
        lua,
        rhai,
        proxy,
        recovery_requested: false,
        background: [0.18, 0.07, 0.1, 1.0],
      }
    });
    let submitter = worker.submitter();
    let input_proxy = self.proxy.clone();
    std::thread::spawn(move || {
      println!(
        "REPL: :main or :worker selects executor; :lua or :rhai selects language; :quit exits"
      );
      let mut target = "worker";
      let mut language = "lua";
      for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
          break;
        };
        let source = line.trim().to_owned();
        if source.is_empty() {
          continue;
        }
        match source.as_str() {
          ":main" => {
            target = "main";
            println!("main executor");
            continue;
          }
          ":worker" => {
            target = "worker";
            println!("worker executor");
            continue;
          }
          ":lua" => {
            language = "lua";
            println!("Lua");
            continue;
          }
          ":rhai" => {
            language = "rhai";
            println!("Rhai");
            continue;
          }
          ":quit" => {
            let _ = input_proxy.send_event(Box::new(|_, event_loop| event_loop.exit()));
            break;
          }
          _ => {}
        }
        if target == "worker" {
          let selected = language.to_owned();
          match submitter.call(move |state| state.eval(&selected, &source)) {
            Ok(task) => match pollster::block_on(task) {
              Ok(result) => println!("{result}"),
              Err(error) => eprintln!("task: {error:?}"),
            },
            Err(error) => {
              eprintln!("worker stopped: {error:?}");
              break;
            }
          }
        } else {
          let (sender, receiver) = mpsc::sync_channel(1);
          let selected = language.to_owned();
          if input_proxy
            .send_event(Box::new(move |app, _| {
              let _ = sender.send(app.eval(&selected, &source));
            }))
            .is_err()
          {
            break;
          }
          match receiver.recv() {
            Ok(result) => println!("{result}"),
            Err(error) => {
              eprintln!("main executor stopped: {error}");
              break;
            }
          }
        }
      }
    });
    self.worker = Some(worker);
    println!("main viewer thread: {:?}", std::thread::current().id());
  }
  fn user_event(&mut self, event_loop: &ActiveEventLoop, task: MainTask) { task(self, event_loop); }
  fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
    let is_main = self.main_window.as_ref().is_some_and(|w| w.id() == id);
    let is_worker = self.worker_window.as_ref().is_some_and(|w| w.id() == id);
    if !is_main && !is_worker {
      return;
    }
    match event {
      WindowEvent::CloseRequested => {
        if is_main {
          self.main_viewer.take();
          self.main_window.take();
        }
        if is_worker {
          if let Some(worker) = &self.worker {
            let _ = worker.spawn(|state| state.viewer.borrow_mut().close());
          }
          self.worker_window.take();
        }
        if self.main_window.is_none() && self.worker_window.is_none() {
          if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
          }
          event_loop.exit();
        }
      }
      WindowEvent::Resized(size) => {
        if is_main && let Some(viewer) = &self.main_viewer {
          let _ = viewer.borrow_mut().resize(size.width, size.height);
        }
        if is_worker && let Some(worker) = &self.worker {
          let _ = worker.spawn(move |state| {
            let _ = state.viewer.borrow_mut().resize(size.width, size.height);
          });
        }
      }
      WindowEvent::MouseInput { state, button, .. } => {
        let index = match button {
          MouseButton::Left => Some(0),
          MouseButton::Middle => Some(1),
          MouseButton::Right => Some(2),
          _ => None,
        };
        if let Some(index) = index {
          self.buttons.entry(id).or_default()[index] = state == ElementState::Pressed;
        }
      }
      WindowEvent::CursorMoved { position, .. } => {
        let point = (position.x as f32, position.y as f32);
        if let Some(previous) = self.last.insert(id, point) {
          let (dx, dy) = (point.0 - previous.0, point.1 - previous.1);
          let buttons = *self.buttons.get(&id).unwrap_or(&[false; 3]);
          if is_main {
            if buttons[0] {
              self.camera.borrow_mut().orbit(glam::Vec3::ZERO, dx, dy);
            }
            if buttons[1] || buttons[2] {
              self.camera.borrow_mut().pan(dx, dy);
            }
          } else if let Some(worker) = &self.worker {
            let _ = worker.spawn(move |state| {
              let mut camera = state.camera.borrow_mut();
              if buttons[0] {
                camera.orbit(glam::Vec3::ZERO, dx, dy);
              }
              if buttons[1] || buttons[2] {
                camera.pan(dx, dy);
              }
            });
          }
        }
      }
      WindowEvent::MouseWheel { delta, .. } => {
        let amount = match delta {
          MouseScrollDelta::LineDelta(_, y) => y * 0.2,
          MouseScrollDelta::PixelDelta(p) => p.y as f32 / 200.0,
        };
        if is_main {
          self.camera.borrow_mut().zoom(amount);
        } else if let Some(worker) = &self.worker {
          let _ = worker.spawn(move |state| state.camera.borrow_mut().zoom(amount));
        }
      }
      WindowEvent::RedrawRequested => {
        if is_main && let Some(viewer) = &self.main_viewer {
          let _ = viewer
            .borrow_mut()
            .render(&self.camera.borrow(), [0.08, 0.09, 0.12, 1.0]);
        }
        if is_worker && let Some(worker) = &self.worker {
          let _ = worker.spawn(WorkerState::render);
        }
      }
      _ => {}
    }
  }
  fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    let now = Instant::now();
    if now >= self.next_frame {
      self.next_frame = now + Duration::from_millis(16);
      if let Some(window) = &self.main_window {
        window.request_redraw();
      }
      if let Some(window) = &self.worker_window {
        window.request_redraw();
      }
    }
    event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
  }
}

fn main() {
  let context = pollster::block_on(GpuContext::new()).unwrap();
  let scene = Scene::load_obj(include_bytes!("../../../assets/models/pyramid.obj")).unwrap();
  let mut camera = Camera::default();
  camera.frame_aabb(Aabb::from(&scene));
  let event_loop = EventLoop::<MainTask>::with_user_event().build().unwrap();
  let proxy = event_loop.create_proxy();
  let mut app = ThreadedApp {
    context,
    scene,
    camera: Rc::new(RefCell::new(camera)),
    main_window: None,
    worker_window: None,
    main_viewer: None,
    main_lua: None,
    main_rhai: None,
    worker: None,
    proxy,
    last: HashMap::new(),
    buttons: HashMap::new(),
    next_frame: Instant::now(),
  };
  event_loop.run_app(&mut app).unwrap();
}
