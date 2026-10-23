//! Winit host for engine viewers. One event loop owns all native windows.
use engine::{Camera, EventKind, Executor, GpuContext, HostScene, InputEvent, ObjViewer};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    platform::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    window::{Window, WindowId},
};

enum HostEvent {
    Wake,
    Exit,
}
struct ScriptState {
    default_language: &'static str,
    #[cfg(feature = "lua")]
    lua: Option<Rc<script_lua::LuaRuntime>>,
    #[cfg(feature = "rhai")]
    rhai: Option<Rc<RefCell<script_rhai::RhaiRuntime>>>,
}
impl ScriptState {
    fn eval(&mut self, language: &str, source: &str) -> String {
        #[cfg(not(any(feature = "lua", feature = "rhai")))]
        let _ = source;
        match language {
            #[cfg(feature = "lua")]
            "lua" => self
                .lua
                .as_ref()
                .map(|runtime| {
                    runtime
                        .eval(source)
                        .map(|_| "ok".into())
                        .unwrap_or_else(|e| e.to_string())
                })
                .unwrap_or("Lua is not enabled".into()),
            #[cfg(feature = "rhai")]
            "rhai" => self
                .rhai
                .as_ref()
                .map(|runtime| {
                    runtime
                        .borrow_mut()
                        .eval(source)
                        .map(|v| format!("{v:?}"))
                        .unwrap_or_else(|e| e.to_string())
                })
                .unwrap_or("Rhai is not enabled".into()),
            _ => "unknown language".into(),
        }
    }
}

pub struct ViewerConfig {
    pub viewer: Rc<RefCell<ObjViewer>>,
    pub camera: Rc<RefCell<Camera>>,
    pub on_input: Option<Box<dyn FnMut(InputEvent)>>,
    pub on_frame: Option<Box<dyn FnMut(f32)>>,
}
impl ViewerConfig {
    pub fn new(viewer: Rc<RefCell<ObjViewer>>, camera: Rc<RefCell<Camera>>) -> Self {
        Self {
            viewer,
            camera,
            on_input: None,
            on_frame: None,
        }
    }
}
struct LiveViewer {
    window: Arc<Window>,
    config: ViewerConfig,
    last_cursor: Option<(f32, f32)>,
    buttons: [bool; 3],
    last_frame: Instant,
}
pub struct NativeApp {
    pending: Vec<ViewerConfig>,
    live: HashMap<WindowId, LiveViewer>,
    error: Option<String>,
    script_state: Option<ScriptState>,
    main_executor: Option<Executor<ScriptState>>,
    next_frame: Instant,
}
/// Optional manual event pumping for embedding the native host in another
/// main-thread loop. Events and rendering still run synchronously in winit.
pub struct NativeSession {
    event_loop: EventLoop<HostEvent>,
    app: NativeApp,
}
impl NativeSession {
    /// Returns `false` after the application exits. `None` may block until an
    /// event arrives; `Some(Duration::ZERO)` is nonblocking.
    pub fn pump(&mut self, timeout: Option<Duration>) -> Result<bool, String> {
        let status = self.event_loop.pump_app_events(timeout, &mut self.app);
        if let Some(error) = self.app.error.take() {
            return Err(error);
        }
        Ok(matches!(status, PumpStatus::Continue))
    }
    pub fn run(mut self) -> Result<(), String> {
        self.event_loop
            .run_app(&mut self.app)
            .map_err(|e| e.to_string())?;
        self.app.error.map_or(Ok(()), Err)
    }
}
impl NativeApp {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            live: HashMap::new(),
            error: None,
            script_state: None,
            main_executor: None,
            next_frame: Instant::now(),
        }
    }
    pub fn add_viewer(&mut self, config: ViewerConfig) {
        self.pending.push(config);
    }
    pub fn start(mut self) -> Result<NativeSession, String> {
        let event_loop = EventLoop::<HostEvent>::with_user_event()
            .build()
            .map_err(|e| e.to_string())?;
        if let Some(state) = self.script_state.take() {
            let default_language = state.default_language.to_owned();
            let proxy = event_loop.create_proxy();
            let wake = proxy.clone();
            let executor = Executor::new_main(state).with_wakeup(move || {
                let _ = wake.send_event(HostEvent::Wake);
            });
            let submitter = executor.submitter();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                let mut language = default_language;
                for line in stdin.lock().lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    let source = line.trim();
                    if source == ":quit" {
                        let _ = proxy.send_event(HostEvent::Exit);
                        break;
                    }
                    if source == ":lua" || source == ":rhai" {
                        language = source[1..].into();
                        println!("{language}");
                        continue;
                    }
                    if source == ":main" {
                        println!("main executor");
                        continue;
                    }
                    if source.is_empty() {
                        continue;
                    }
                    let language = language.clone();
                    let source = source.to_owned();
                    match submitter.call(move |state| state.eval(&language, &source)) {
                        Ok(task) => match pollster::block_on(task) {
                            Ok(output) => println!("{output}"),
                            Err(error) => eprintln!("task failed: {error:?}"),
                        },
                        Err(error) => {
                            eprintln!("executor closed: {error:?}");
                            break;
                        }
                    }
                }
            });
            self.main_executor = Some(executor);
        }
        Ok(NativeSession {
            event_loop,
            app: self,
        })
    }
    pub fn show(self) -> Result<(), String> {
        self.start()?.run()
    }
}
impl Default for NativeApp {
    fn default() -> Self {
        Self::new()
    }
}
impl ApplicationHandler<HostEvent> for NativeApp {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: HostEvent) {
        match event {
            HostEvent::Wake => {
                if let Some(executor) = &mut self.main_executor {
                    let _ = executor.drain();
                }
            }
            HostEvent::Exit => event_loop.exit(),
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        for config in self.pending.drain(..) {
            if config.viewer.borrow().is_closed() {
                continue;
            }
            let title = config.viewer.borrow().title().to_owned();
            let size = config.viewer.borrow().size();
            let attributes = Window::default_attributes()
                .with_title(title)
                .with_inner_size(PhysicalSize::new(size.0, size.1));
            let result = (|| -> Result<LiveViewer, String> {
                let window = Arc::new(
                    event_loop
                        .create_window(attributes)
                        .map_err(|e| e.to_string())?,
                );
                let context = config.viewer.borrow().context().clone();
                let surface = context
                    .instance()
                    .create_surface(window.clone())
                    .map_err(|e| e.to_string())?;
                let size = window.inner_size();
                config
                    .viewer
                    .borrow_mut()
                    .attach_surface(surface, size.width, size.height)?;
                Ok(LiveViewer {
                    window,
                    config,
                    last_cursor: None,
                    buttons: [false; 3],
                    last_frame: Instant::now(),
                })
            })();
            match result {
                Ok(live) => {
                    self.live.insert(live.window.id(), live);
                }
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                    break;
                }
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            if let Some(live) = self.live.get_mut(&id)
                && let Some(callback) = &mut live.config.on_input
            {
                callback(InputEvent {
                    kind: EventKind::CloseRequest,
                    ..Default::default()
                });
            }
            if let Some(live) = self.live.remove(&id) {
                live.config.viewer.borrow_mut().close();
            }
            if self.live.is_empty() {
                event_loop.exit();
            }
            return;
        }
        let Some(live) = self.live.get_mut(&id) else {
            return;
        };
        match event {
            WindowEvent::Resized(size) => {
                live.config
                    .viewer
                    .borrow_mut()
                    .resize(size.width, size.height);
                if let Some(callback) = &mut live.config.on_input {
                    callback(InputEvent {
                        kind: EventKind::Resize,
                        width: size.width,
                        height: size.height,
                        ..Default::default()
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
                    live.buttons[index] = state == ElementState::Pressed;
                    let (x, y) = live.last_cursor.unwrap_or((0.0, 0.0));
                    if let Some(callback) = &mut live.config.on_input {
                        callback(InputEvent {
                            kind: EventKind::MouseButton,
                            x,
                            y,
                            button: Some(index as u8),
                            pressed: live.buttons[index],
                            left_pressed: live.buttons[0],
                            middle_pressed: live.buttons[1],
                            right_pressed: live.buttons[2],
                            ..Default::default()
                        });
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(callback) = &mut live.config.on_input {
                    callback(InputEvent {
                        kind: EventKind::Key,
                        key: Some(match event.physical_key {
                            PhysicalKey::Code(code) => format!("{code:?}"),
                            _ => format!("{:?}", event.logical_key),
                        }),
                        pressed: event.state == ElementState::Pressed,
                        ..Default::default()
                    });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let point = (position.x as f32, position.y as f32);
                if let Some(last) = live.last_cursor
                    && let Some(callback) = &mut live.config.on_input
                {
                    callback(InputEvent {
                        kind: EventKind::MouseMove,
                        dx: point.0 - last.0,
                        dy: point.1 - last.1,
                        x: point.0,
                        y: point.1,
                        left_pressed: live.buttons[0],
                        middle_pressed: live.buttons[1],
                        right_pressed: live.buttons[2],
                        ..Default::default()
                    });
                }
                live.last_cursor = Some(point);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let wheel = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                let (x, y) = live.last_cursor.unwrap_or((0.0, 0.0));
                if let Some(callback) = &mut live.config.on_input {
                    callback(InputEvent {
                        kind: EventKind::MouseWheel,
                        wheel,
                        x,
                        y,
                        left_pressed: live.buttons[0],
                        middle_pressed: live.buttons[1],
                        right_pressed: live.buttons[2],
                        ..Default::default()
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(live.last_frame).as_secs_f32();
                live.last_frame = now;
                let result = if let Some(callback) = &mut live.config.on_frame {
                    callback(dt);
                    Ok(())
                } else {
                    live.config
                        .viewer
                        .borrow_mut()
                        .render(&live.config.camera.borrow())
                };
                if live.config.viewer.borrow().needs_surface_recreation() {
                    let context = live.config.viewer.borrow().context().clone();
                    let size = live.window.inner_size();
                    let recovery = context
                        .instance()
                        .create_surface(live.window.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|surface| {
                            live.config.viewer.borrow_mut().attach_surface(
                                surface,
                                size.width,
                                size.height,
                            )
                        });
                    if let Err(error) = recovery {
                        self.error = Some(error);
                        event_loop.exit();
                        return;
                    }
                }
                if let Err(error) = result {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(executor) = &mut self.main_executor {
            let _ = executor.drain();
        }
        self.live
            .retain(|_, live| !live.config.viewer.borrow().is_closed());
        if self.live.is_empty() && self.pending.is_empty() {
            event_loop.exit();
        }
        let now = Instant::now();
        if now < self.next_frame {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            return;
        }
        self.next_frame = now + Duration::from_millis(16);
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        for live in self.live.values() {
            let viewer = live.config.viewer.borrow();
            live.window.set_title(viewer.title());
            let requested = viewer.size();
            let actual = live.window.inner_size();
            if requested != (actual.width, actual.height) {
                let _ = live
                    .window
                    .request_inner_size(PhysicalSize::new(requested.0, requested.1));
            }
            live.window.request_redraw();
        }
    }
}

/// Run one of the four Rust parity demonstrations.
pub fn run_case(case: usize) -> Result<(), String> {
    if !(1..=4).contains(&case) {
        return Err("case must be 1..=4".into());
    }
    let context = pollster::block_on(GpuContext::new())?;
    let first = HostScene::load_obj(include_bytes!("../../../assets/models/pyramid.obj"))?;
    let second = HostScene::load_obj(include_bytes!("../../../assets/models/cube.obj"))?;
    let shared_device = (case == 4).then(|| first.upload(&context)).transpose()?;
    let shared_camera = Rc::new(RefCell::new(Camera::default()));
    shared_camera.borrow_mut().frame(first.bounds());
    let mut app = NativeApp::new();
    let count = if case == 1 { 1 } else { 2 };
    for i in 0..count {
        let mut viewer = ObjViewer::new(context.clone());
        match case {
            2 if i == 1 => viewer.set_host_scene(&second)?,
            4 => viewer.set_device_scene(shared_device.as_ref().unwrap().clone())?,
            _ => viewer.set_host_scene(&first)?,
        }
        let camera = if case == 4 || i == 0 {
            shared_camera.clone()
        } else {
            let c = Rc::new(RefCell::new(Camera::default()));
            c.borrow_mut().frame(if case == 2 {
                second.bounds()
            } else {
                first.bounds()
            });
            c
        };
        viewer.set_title(format!("Rust case {case} — viewer {}", i + 1));
        viewer.resize(
            if case == 4 && i == 1 { 900 } else { 640 },
            if case == 4 && i == 1 { 400 } else { 480 },
        );
        viewer.set_background_color(if i == 0 {
            [0.08, 0.09, 0.12, 1.0]
        } else {
            [0.17, 0.08, 0.12, 1.0]
        });
        let mut config = ViewerConfig::new(Rc::new(RefCell::new(viewer)), camera.clone());
        config.on_input = Some(Box::new(move |event| {
            let mut camera = camera.borrow_mut();
            if event.left_pressed {
                camera.orbit(glam::Vec3::ZERO, event.dx, event.dy);
            }
            if event.middle_pressed || event.right_pressed {
                camera.pan(event.dx, event.dy);
            }
            if event.wheel != 0.0 {
                camera.zoom(event.wheel * 0.2);
            }
        }));
        app.add_viewer(config);
    }
    app.show()
}

#[cfg(feature = "lua")]
pub fn run_lua(path: &str) -> Result<(), String> {
    use script_lua::LuaRuntime;
    let runtime = Rc::new(LuaRuntime::new().map_err(|e| e.to_string())?);
    runtime
        .eval(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut app = NativeApp::new();
    app.script_state = Some(ScriptState {
        default_language: "lua",
        lua: Some(runtime.clone()),
        #[cfg(feature = "rhai")]
        rhai: None,
    });
    for scripted in runtime.viewers() {
        let mut config = ViewerConfig::new(
            scripted.viewer.clone(),
            Rc::new(RefCell::new(Camera::default())),
        );
        let viewer = scripted.clone();
        let runtime_ref = runtime.clone();
        config.on_input = Some(Box::new(move |event| {
            if let Err(error) = viewer.dispatch_input(runtime_ref.lua(), event) {
                eprintln!("Lua callback: {error}");
            }
        }));
        let viewer = scripted.clone();
        config.on_frame = Some(Box::new(move |dt| {
            if let Err(error) = viewer.dispatch_frame(dt) {
                eprintln!("Lua frame: {error}");
            }
        }));
        app.add_viewer(config);
    }
    app.show()
}

#[cfg(feature = "rhai")]
pub fn run_rhai(path: &str) -> Result<(), String> {
    use script_rhai::RhaiRuntime;
    let runtime = Rc::new(RefCell::new(RhaiRuntime::new()));
    let _ = runtime
        .borrow_mut()
        .eval(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut app = NativeApp::new();
    app.script_state = Some(ScriptState {
        default_language: "rhai",
        #[cfg(feature = "lua")]
        lua: None,
        rhai: Some(runtime.clone()),
    });
    for scripted in runtime.borrow().viewers() {
        let mut config = ViewerConfig::new(
            scripted.viewer.clone(),
            Rc::new(RefCell::new(Camera::default())),
        );
        let viewer = scripted.clone();
        let runtime_ref = runtime.clone();
        config.on_input = Some(Box::new(move |event| {
            let runtime = runtime_ref.borrow();
            if let Err(error) = viewer.dispatch_input(runtime.engine(), runtime.ast(), event) {
                eprintln!("Rhai callback: {error}");
            }
        }));
        let viewer = scripted.clone();
        let runtime_ref = runtime.clone();
        config.on_frame = Some(Box::new(move |dt| {
            let runtime = runtime_ref.borrow();
            if let Err(error) = viewer.dispatch_frame(runtime.engine(), runtime.ast(), dt) {
                eprintln!("Rhai frame: {error}");
            }
        }));
        app.add_viewer(config);
    }
    app.show()
}

/// Run Lua-owned windows with the first viewer explicitly shared into Rhai.
#[cfg(all(feature = "lua", feature = "rhai"))]
pub fn run_both(lua_path: &str, rhai_path: &str) -> Result<(), String> {
    let lua = Rc::new(script_lua::LuaRuntime::new().map_err(|e| e.to_string())?);
    lua.eval(&std::fs::read_to_string(lua_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let scripted_viewers = lua.viewers();
    let mut rhai_runtime = script_rhai::RhaiRuntime::new();
    if let Some(first) = scripted_viewers.first() {
        rhai_runtime.expose_viewer("viewer", first.viewer.clone());
    }
    let _ = rhai_runtime
        .eval(&std::fs::read_to_string(rhai_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let rhai = Rc::new(RefCell::new(rhai_runtime));
    let mut app = NativeApp::new();
    app.script_state = Some(ScriptState {
        default_language: "lua",
        lua: Some(lua.clone()),
        rhai: Some(rhai),
    });
    for scripted in scripted_viewers {
        let mut config = ViewerConfig::new(
            scripted.viewer.clone(),
            Rc::new(RefCell::new(Camera::default())),
        );
        let viewer = scripted.clone();
        let runtime = lua.clone();
        config.on_input = Some(Box::new(move |event| {
            if let Err(error) = viewer.dispatch_input(runtime.lua(), event) {
                eprintln!("Lua callback: {error}");
            }
        }));
        let viewer = scripted.clone();
        config.on_frame = Some(Box::new(move |dt| {
            if let Err(error) = viewer.dispatch_frame(dt) {
                eprintln!("Lua frame: {error}");
            }
        }));
        app.add_viewer(config);
    }
    app.show()
}
