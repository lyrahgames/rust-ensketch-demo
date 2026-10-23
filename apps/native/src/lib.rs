//! Winit host for engine viewers. One event loop owns all native windows.
mod examples;
mod scripting;

pub use examples::run_case;
#[cfg(all(feature = "lua", feature = "rhai"))]
pub use scripting::run_both;
#[cfg(feature = "lua")]
pub use scripting::run_lua;
#[cfg(feature = "rhai")]
pub use scripting::run_rhai;

use engine::{
    Camera, EventKind, Executor, InputEvent, ObjViewer, PointerButton, PointerInputState,
    RenderStatus,
};
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

pub(crate) enum HostEvent {
    Wake,
    Exit,
}
pub struct ViewerConfig {
    pub viewer: Rc<RefCell<ObjViewer>>,
    pub camera: Rc<RefCell<Camera>>,
    pub title: String,
    pub size: (u32, u32),
    pub background: [f32; 4],
    pub on_input: Option<Box<dyn FnMut(InputEvent)>>,
    pub on_frame: Option<Box<dyn FnMut(f32)>>,
}
impl ViewerConfig {
    pub fn new(viewer: Rc<RefCell<ObjViewer>>, camera: Rc<RefCell<Camera>>) -> Self {
        let pivot = viewer
            .borrow()
            .scene()
            .map(|scene| scene.aabb().center())
            .unwrap_or(glam::Vec3::ZERO);
        let controlled_camera = camera.clone();
        Self {
            viewer,
            camera,
            title: "OBJ viewer".into(),
            size: (640, 480),
            background: [0.08, 0.09, 0.12, 1.0],
            on_input: Some(Box::new(move |event| {
                controlled_camera
                    .borrow_mut()
                    .apply_pointer_controls(&event, pivot);
            })),
            on_frame: None,
        }
    }
}
struct LiveViewer {
    window: Arc<Window>,
    config: ViewerConfig,
    pointer: PointerInputState,
    last_frame: Instant,
}
pub struct NativeApp {
    pending: Vec<ViewerConfig>,
    live: HashMap<WindowId, LiveViewer>,
    error: Option<String>,
    pub(crate) script_state: Option<scripting::ScriptState>,
    main_executor: Option<Executor<scripting::ScriptState>>,
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
    /// Add a viewer with the standard orbit/pan/dolly controls and render loop.
    pub fn add_obj_viewer(&mut self, viewer: ObjViewer, camera: Camera) {
        self.add_viewer(ViewerConfig::new(
            Rc::new(RefCell::new(viewer)),
            Rc::new(RefCell::new(camera)),
        ));
    }
    pub fn start(mut self) -> Result<NativeSession, String> {
        let event_loop = EventLoop::<HostEvent>::with_user_event()
            .build()
            .map_err(|e| e.to_string())?;
        if let Some(state) = self.script_state.take() {
            self.main_executor = Some(scripting::start_repl(state, event_loop.create_proxy()));
        }
        Ok(NativeSession {
            event_loop,
            app: self,
        })
    }
    pub fn show(self) -> Result<(), String> {
        self.start()?.run()
    }
    /// Run one viewer with standard controls, without manually assembling an
    /// application or callback configuration.
    pub fn show_obj_viewer(viewer: ObjViewer, camera: Camera) -> Result<(), String> {
        let mut app = Self::new();
        app.add_obj_viewer(viewer, camera);
        app.show()
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
            let attributes = Window::default_attributes()
                .with_title(config.title.clone())
                .with_inner_size(PhysicalSize::new(config.size.0, config.size.1));
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
                    pointer: PointerInputState::default(),
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
                    .resize(size.width, size.height)
                    .unwrap_or_else(|error| eprintln!("viewer resize: {error}"));
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
                let button = match button {
                    MouseButton::Left => Some(PointerButton::Left),
                    MouseButton::Middle => Some(PointerButton::Middle),
                    MouseButton::Right => Some(PointerButton::Right),
                    _ => None,
                };
                if let Some(button) = button {
                    let event = live.pointer.button(button, state == ElementState::Pressed);
                    if let Some(callback) = &mut live.config.on_input {
                        callback(event);
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
                if let Some(event) = live.pointer.cursor(position.x as f32, position.y as f32)
                    && let Some(callback) = &mut live.config.on_input
                {
                    callback(event);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let wheel = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Some(callback) = &mut live.config.on_input {
                    callback(live.pointer.wheel(wheel));
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(live.last_frame).as_secs_f32();
                live.last_frame = now;
                let result = if let Some(callback) = &mut live.config.on_frame {
                    callback(dt);
                    Ok(RenderStatus::Skipped)
                } else {
                    live.config
                        .viewer
                        .borrow_mut()
                        .render(&live.config.camera.borrow(), live.config.background)
                };
                if matches!(result, Ok(RenderStatus::SurfaceLost)) {
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
            .retain(|_, live| live.config.viewer.borrow().is_attached());
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
            live.window.set_title(&live.config.title);
            let requested = live.config.size;
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
