//! Importable Python host. Python owns all wrapper objects and callbacks.
use engine::{Camera, DeviceScene, EventKind, GpuContext, HostScene, InputEvent, ObjViewer};
use glam::{Quat, Vec3};
use native::{NativeApp, NativeSession, ViewerConfig};
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::Duration,
};

fn error(message: String) -> PyErr {
    PyRuntimeError::new_err(message)
}

#[pyclass(name = "GpuContext", unsendable)]
struct PyGpuContext {
    inner: GpuContext,
}
#[pymethods]
impl PyGpuContext {
    #[new]
    #[pyo3(signature = (adapter=None))]
    fn new(adapter: Option<&str>) -> PyResult<Self> {
        Ok(Self {
            inner: pollster::block_on(GpuContext::with_adapter(adapter)).map_err(error)?,
        })
    }
    #[getter]
    fn adapter_name(&self) -> String {
        self.inner.adapter_info().name
    }
    #[getter]
    fn max_texture_dimension_2d(&self) -> u32 {
        self.inner.limits().max_texture_dimension_2d
    }
}

#[pyclass(name = "HostScene", unsendable)]
struct PyHostScene {
    inner: HostScene,
}
#[pymethods]
impl PyHostScene {
    #[staticmethod]
    fn load_obj(path: &str) -> PyResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: HostScene::load_obj(&bytes).map_err(error)?,
        })
    }
    #[staticmethod]
    fn from_bytes(bytes: &[u8]) -> PyResult<Self> {
        Ok(Self {
            inner: HostScene::load_obj(bytes).map_err(error)?,
        })
    }
    fn upload(&self, context: PyRef<'_, PyGpuContext>) -> PyResult<PyDeviceScene> {
        Ok(PyDeviceScene {
            inner: self.inner.upload(&context.inner).map_err(error)?,
        })
    }
    #[getter]
    fn mesh_count(&self) -> usize {
        self.inner.meshes().len()
    }
}

#[pyclass(name = "DeviceScene", unsendable)]
struct PyDeviceScene {
    inner: DeviceScene,
}

#[pyclass(name = "Camera", unsendable)]
struct PyCamera {
    inner: Rc<RefCell<Camera>>,
}
#[pymethods]
impl PyCamera {
    #[new]
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Camera::default())),
        }
    }
    #[getter]
    fn position(&self) -> (f32, f32, f32) {
        let p = self.inner.borrow().position;
        (p.x, p.y, p.z)
    }
    #[setter]
    fn set_position(&self, position: (f32, f32, f32)) {
        self.inner.borrow_mut().position = Vec3::new(position.0, position.1, position.2);
    }
    #[getter]
    fn orientation(&self) -> (f32, f32, f32, f32) {
        let q = self.inner.borrow().orientation;
        (q.x, q.y, q.z, q.w)
    }
    #[setter]
    fn set_orientation(&self, orientation: (f32, f32, f32, f32)) {
        self.inner.borrow_mut().orientation =
            Quat::from_xyzw(orientation.0, orientation.1, orientation.2, orientation.3).normalize();
    }
    #[getter]
    fn vertical_fov(&self) -> f32 {
        self.inner.borrow().vertical_fov
    }
    #[setter]
    fn set_vertical_fov(&self, value: f32) {
        self.inner.borrow_mut().vertical_fov = value;
    }
    #[getter]
    fn near(&self) -> f32 {
        self.inner.borrow().near
    }
    #[setter]
    fn set_near(&self, value: f32) {
        self.inner.borrow_mut().near = value;
    }
    #[getter]
    fn far(&self) -> f32 {
        self.inner.borrow().far
    }
    #[setter]
    fn set_far(&self, value: f32) {
        self.inner.borrow_mut().far = value;
    }
    fn orbit(&self, dx: f32, dy: f32) {
        self.inner.borrow_mut().orbit(Vec3::ZERO, dx, dy);
    }
    fn pan(&self, dx: f32, dy: f32) {
        self.inner.borrow_mut().pan(dx, dy);
    }
    fn zoom(&self, amount: f32) {
        self.inner.borrow_mut().zoom(amount);
    }
    fn reset(&self) {
        self.inner.borrow_mut().reset();
    }
    fn frame(&self, scene: PyRef<'_, PyHostScene>) {
        self.inner.borrow_mut().frame_aabb(scene.inner.aabb());
    }
}

#[pyclass(name = "InputEvent", unsendable)]
struct PyInputEvent {
    event: InputEvent,
}
#[pymethods]
impl PyInputEvent {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.event.kind {
            EventKind::MouseMove => "mouse_move",
            EventKind::MouseButton => "mouse_button",
            EventKind::MouseWheel => "mouse_wheel",
            EventKind::Key => "key",
            EventKind::Resize => "resize",
            EventKind::CloseRequest => "close_request",
        }
    }
    #[getter]
    fn dx(&self) -> f32 {
        self.event.dx
    }
    #[getter]
    fn dy(&self) -> f32 {
        self.event.dy
    }
    #[getter]
    fn x(&self) -> f32 {
        self.event.x
    }
    #[getter]
    fn y(&self) -> f32 {
        self.event.y
    }
    #[getter]
    fn wheel(&self) -> f32 {
        self.event.wheel
    }
    #[getter]
    fn left_pressed(&self) -> bool {
        self.event.left_pressed
    }
    #[getter]
    fn middle_pressed(&self) -> bool {
        self.event.middle_pressed
    }
    #[getter]
    fn right_pressed(&self) -> bool {
        self.event.right_pressed
    }
    #[getter]
    fn button(&self) -> Option<u8> {
        self.event.button
    }
    #[getter]
    fn pressed(&self) -> bool {
        self.event.pressed
    }
    #[getter]
    fn key(&self) -> Option<String> {
        self.event.key.clone()
    }
    #[getter]
    fn width(&self) -> u32 {
        self.event.width
    }
    #[getter]
    fn height(&self) -> u32 {
        self.event.height
    }
}

#[derive(Default)]
struct Callbacks {
    move_fn: Option<Py<PyAny>>,
    button_fn: Option<Py<PyAny>>,
    wheel_fn: Option<Py<PyAny>>,
    key_fn: Option<Py<PyAny>>,
    resize_fn: Option<Py<PyAny>>,
    close_fn: Option<Py<PyAny>>,
    frame_fn: Option<Py<PyAny>>,
}
struct Registration {
    viewer: Rc<RefCell<ObjViewer>>,
    camera: Rc<RefCell<Camera>>,
    callbacks: Rc<RefCell<Callbacks>>,
    background: Rc<RefCell<[f32; 4]>>,
    title: Rc<RefCell<String>>,
    size: Rc<RefCell<(u32, u32)>>,
    closed: Cell<bool>,
    #[cfg(feature = "lua")]
    lua: RefCell<Option<script_lua::LuaRuntime>>,
    #[cfg(feature = "rhai")]
    rhai: RefCell<Option<script_rhai::RhaiRuntime>>,
}
thread_local! {
    static VIEWERS: RefCell<Vec<Weak<Registration>>> = const { RefCell::new(Vec::new()) };
    // A temporary thread-local transfer lets `show` release the GIL while
    // keeping winit's event loop on the Python caller's OS thread.
    static SHOW_APP: RefCell<Option<NativeApp>> = const { RefCell::new(None) };
    static MANUAL_SESSION: RefCell<Option<NativeSession>> = const { RefCell::new(None) };
    static SHOW_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

#[pyclass(name = "ObjViewer", unsendable)]
struct PyObjViewer {
    registration: Rc<Registration>,
}
#[pymethods]
impl PyObjViewer {
    #[new]
    fn new(context: PyRef<'_, PyGpuContext>) -> Self {
        let registration = Rc::new(Registration {
            viewer: Rc::new(RefCell::new(ObjViewer::new(context.inner.clone()))),
            camera: Rc::new(RefCell::new(Camera::default())),
            callbacks: Rc::new(RefCell::new(Callbacks::default())),
            background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
            title: Rc::new(RefCell::new("OBJ viewer".into())),
            size: Rc::new(RefCell::new((640, 480))),
            closed: Cell::new(false),
            #[cfg(feature = "lua")]
            lua: RefCell::new(None),
            #[cfg(feature = "rhai")]
            rhai: RefCell::new(None),
        });
        VIEWERS.with(|viewers| viewers.borrow_mut().push(Rc::downgrade(&registration)));
        Self { registration }
    }
    fn set_scene(&self, scene: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(host) = scene.extract::<PyRef<'_, PyHostScene>>() {
            self.registration
                .camera
                .borrow_mut()
                .frame_aabb(host.inner.aabb());
            self.registration
                .viewer
                .borrow_mut()
                .set_host_scene(&host.inner)
                .map_err(error)
        } else if let Ok(device) = scene.extract::<PyRef<'_, PyDeviceScene>>() {
            self.registration
                .camera
                .borrow_mut()
                .frame_aabb(device.inner.aabb());
            self.registration
                .viewer
                .borrow_mut()
                .set_device_scene(device.inner.clone())
                .map_err(error)
        } else {
            Err(PyValueError::new_err("expected HostScene or DeviceScene"))
        }
    }
    fn set_background_color(&self, r: f32, g: f32, b: f32, a: f32) {
        *self.registration.background.borrow_mut() = [r, g, b, a];
    }
    fn background_color(&self) -> (f32, f32, f32, f32) {
        let c = *self.registration.background.borrow();
        (c[0], c[1], c[2], c[3])
    }
    fn set_title(&self, title: &str) {
        *self.registration.title.borrow_mut() = title.to_owned();
    }
    fn title(&self) -> String {
        self.registration.title.borrow().clone()
    }
    fn set_size(&self, width: u32, height: u32) {
        *self.registration.size.borrow_mut() = (width, height);
        let _ = self.registration.viewer.borrow_mut().resize(width, height);
    }
    fn size(&self) -> (u32, u32) {
        *self.registration.size.borrow()
    }
    fn render(&self, camera: PyRef<'_, PyCamera>) -> PyResult<()> {
        self.registration
            .viewer
            .borrow_mut()
            .render(
                &camera.inner.borrow(),
                *self.registration.background.borrow(),
            )
            .map(|_| ())
            .map_err(error)
    }
    fn close(&self) {
        self.registration.closed.set(true);
        self.registration.viewer.borrow_mut().close();
    }
    fn on_mouse_move(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().move_fn = callback;
    }
    fn on_mouse_button(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().button_fn = callback;
    }
    fn on_mouse_wheel(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().wheel_fn = callback;
    }
    fn on_key(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().key_fn = callback;
    }
    fn on_resize(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().resize_fn = callback;
    }
    fn on_close_request(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().close_fn = callback;
    }
    fn on_frame(&self, callback: Option<Py<PyAny>>) {
        self.registration.callbacks.borrow_mut().frame_fn = callback;
    }
    /// Evaluate in this viewer's persistent Lua environment, when built with `lua`.
    #[cfg(feature = "lua")]
    #[pyo3(signature = (source, camera=None))]
    fn eval_lua(&self, source: &str, camera: Option<PyRef<'_, PyCamera>>) -> PyResult<()> {
        let mut runtime = self.registration.lua.borrow_mut();
        if runtime.is_none() {
            let lua = script_lua::LuaRuntime::new().map_err(|e| error(e.to_string()))?;
            lua.expose_viewer("viewer", self.registration.viewer.clone())
                .map_err(|e| error(e.to_string()))?;
            lua.expose_context(
                "context",
                self.registration.viewer.borrow().context().clone(),
            )
            .map_err(|e| error(e.to_string()))?;
            *runtime = Some(lua);
        }
        let runtime = runtime.as_ref().unwrap();
        if let Some(camera) = camera {
            runtime
                .expose_camera("camera", camera.inner.clone())
                .map_err(|e| error(e.to_string()))?;
        }
        runtime.eval(source).map_err(|e| error(e.to_string()))
    }
    /// Evaluate in this viewer's persistent Rhai environment, when built with `rhai`.
    #[cfg(feature = "rhai")]
    #[pyo3(signature = (source, camera=None))]
    fn eval_rhai(&self, source: &str, camera: Option<PyRef<'_, PyCamera>>) -> PyResult<String> {
        let mut runtime = self.registration.rhai.borrow_mut();
        if runtime.is_none() {
            let mut rhai = script_rhai::RhaiRuntime::new();
            rhai.expose_viewer("viewer", self.registration.viewer.clone());
            rhai.expose_context(
                "context",
                self.registration.viewer.borrow().context().clone(),
            );
            *runtime = Some(rhai);
        }
        let runtime = runtime.as_mut().unwrap();
        if let Some(camera) = camera {
            runtime.expose_camera("camera", camera.inner.clone());
        }
        runtime
            .eval(source)
            .map(|value| value.to_string())
            .map_err(|e| error(e.to_string()))
    }
}

fn invoke_event(callbacks: &Rc<RefCell<Callbacks>>, event: InputEvent) {
    Python::attach(|py| {
        let callback = {
            let callbacks = callbacks.borrow();
            match event.kind {
                EventKind::MouseMove => callbacks.move_fn.as_ref(),
                EventKind::MouseButton => callbacks.button_fn.as_ref(),
                EventKind::MouseWheel => callbacks.wheel_fn.as_ref(),
                EventKind::Key => callbacks.key_fn.as_ref(),
                EventKind::Resize => callbacks.resize_fn.as_ref(),
                EventKind::CloseRequest => callbacks.close_fn.as_ref(),
            }
            .map(|callback| callback.clone_ref(py))
        };
        if let Some(callback) = callback {
            match Py::new(py, PyInputEvent { event }) {
                Ok(event) => {
                    if let Err(error) = callback.call1(py, (event,)) {
                        error.print(py);
                    }
                }
                Err(error) => error.print(py),
            }
        }
    });
}

fn build_app() -> NativeApp {
    let registrations = VIEWERS.with(|viewers| {
        let mut viewers = viewers.borrow_mut();
        viewers.retain(|weak| weak.strong_count() > 0);
        viewers.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
    });
    let mut app = NativeApp::new();
    for registration in registrations {
        if registration.closed.get() {
            continue;
        }
        let mut config =
            ViewerConfig::new(registration.viewer.clone(), registration.camera.clone());
        let callbacks = registration.callbacks.clone();
        config.title = registration.title.borrow().clone();
        config.size = *registration.size.borrow();
        config.background = *registration.background.borrow();
        config.on_input = Some(Box::new(move |event| invoke_event(&callbacks, event)));
        let callbacks = registration.callbacks.clone();
        let viewer = registration.viewer.clone();
        let camera = registration.camera.clone();
        let background = registration.background.clone();
        config.on_frame = Some(Box::new(move |dt| {
            let called = Python::attach(|py| {
                let callback = callbacks
                    .borrow()
                    .frame_fn
                    .as_ref()
                    .map(|cb| cb.clone_ref(py));
                if let Some(callback) = callback {
                    if let Err(error) = callback.call1(py, (dt,)) {
                        error.print(py);
                    }
                    true
                } else {
                    false
                }
            });
            if !called {
                let _ = viewer
                    .borrow_mut()
                    .render(&camera.borrow(), *background.borrow());
            }
        }));
        app.add_viewer(config);
    }
    app
}

#[pyfunction]
fn show(py: Python<'_>) -> PyResult<()> {
    SHOW_ACTIVE.with(|active| {
        if active.replace(true) {
            Err(PyRuntimeError::new_err("show cannot be called recursively"))
        } else {
            Ok(())
        }
    })?;
    let result = if MANUAL_SESSION.with(|slot| slot.borrow().is_some()) {
        py.detach(|| {
            let mut session = MANUAL_SESSION.with(|slot| slot.borrow_mut().take().unwrap());
            loop {
                match session.pump(None) {
                    Ok(true) => continue,
                    Ok(false) => break Ok(()),
                    Err(error) => break Err(error),
                }
            }
        })
    } else {
        let app = build_app();
        SHOW_APP.with(|slot| {
            let mut slot = slot.borrow_mut();
            debug_assert!(slot.is_none());
            *slot = Some(app);
        });
        py.detach(|| {
            let app =
                SHOW_APP.with(|slot| slot.borrow_mut().take().expect("show app was installed"));
            app.show()
        })
    };
    SHOW_ACTIVE.with(|active| active.set(false));
    result.map_err(error)
}

/// Process native events once on the Python caller's OS thread. Returns
/// `False` when all viewers close. It must be called repeatedly by the host.
#[pyfunction]
#[pyo3(signature = (timeout_ms=0))]
fn process_events(py: Python<'_>, timeout_ms: u64) -> PyResult<bool> {
    SHOW_ACTIVE.with(|active| {
        if active.replace(true) {
            Err(PyRuntimeError::new_err("event processing is not reentrant"))
        } else {
            Ok(())
        }
    })?;
    let result = (|| -> PyResult<bool> {
        if MANUAL_SESSION.with(|slot| slot.borrow().is_none()) {
            let session = build_app().start().map_err(error)?;
            MANUAL_SESSION.with(|slot| *slot.borrow_mut() = Some(session));
        }
        py.detach(|| {
            let mut session = MANUAL_SESSION.with(|slot| slot.borrow_mut().take().unwrap());
            let result = session.pump(Some(Duration::from_millis(timeout_ms)));
            if matches!(result, Ok(true)) {
                MANUAL_SESSION.with(|slot| *slot.borrow_mut() = Some(session));
            }
            result
        })
        .map_err(error)
    })();
    SHOW_ACTIVE.with(|active| active.set(false));
    result
}

#[pymodule]
fn graphics(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGpuContext>()?;
    module.add_class::<PyHostScene>()?;
    module.add_class::<PyDeviceScene>()?;
    module.add_class::<PyCamera>()?;
    module.add_class::<PyInputEvent>()?;
    module.add_class::<PyObjViewer>()?;
    module.add_function(wrap_pyfunction!(show, module)?)?;
    module.add_function(wrap_pyfunction!(process_events, module)?)?;
    Ok(())
}
