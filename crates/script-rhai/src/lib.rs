//! Persistent Rhai bindings for script-owned engine objects.
#![recursion_limit = "1024"]
use engine::{
    Camera, DeviceTriangleMesh, EventKind, GpuContext, InputEvent, ObjViewer, Scene, Triangle,
    Vertex,
};
use glam::{Quat, Vec3};
use rhai::{AST, Array, Dynamic, Engine, EvalAltResult, FnPtr, Scope};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

fn err(message: String) -> Box<EvalAltResult> {
    message.into()
}
#[derive(Clone)]
pub struct RhaiContext(pub GpuContext);
#[derive(Clone)]
pub struct RhaiScene(pub Scene);
#[derive(Clone)]
pub struct RhaiDeviceTriangleMesh(pub Arc<DeviceTriangleMesh<Vertex, Triangle>>);
#[derive(Clone)]
pub struct RhaiCamera(pub Rc<RefCell<Camera>>);
#[derive(Clone)]
pub struct RhaiInputEvent(pub InputEvent);
#[derive(Default)]
struct Callbacks {
    mouse_move: Option<FnPtr>,
    mouse_button: Option<FnPtr>,
    mouse_wheel: Option<FnPtr>,
    key: Option<FnPtr>,
    resize: Option<FnPtr>,
    close_request: Option<FnPtr>,
    frame: Option<FnPtr>,
}
#[derive(Clone)]
pub struct RhaiViewer {
    pub viewer: Rc<RefCell<ObjViewer>>,
    callbacks: Rc<RefCell<Callbacks>>,
    background: Rc<RefCell<[f32; 4]>>,
    title: Rc<RefCell<String>>,
    size: Rc<RefCell<(u32, u32)>>,
}
impl RhaiViewer {
    pub fn dispatch_input(
        &self,
        engine: &Engine,
        ast: &AST,
        event: InputEvent,
    ) -> Result<(), Box<EvalAltResult>> {
        let callback = {
            let callbacks = self.callbacks.borrow();
            match event.kind {
                EventKind::MouseMove => callbacks.mouse_move.clone(),
                EventKind::MouseButton => callbacks.mouse_button.clone(),
                EventKind::MouseWheel => callbacks.mouse_wheel.clone(),
                EventKind::Key => callbacks.key.clone(),
                EventKind::Resize => callbacks.resize.clone(),
                EventKind::CloseRequest => callbacks.close_request.clone(),
            }
        };
        if let Some(callback) = callback {
            let _ = callback.call::<Dynamic>(engine, ast, (RhaiInputEvent(event),))?;
        }
        Ok(())
    }
    pub fn dispatch_frame(
        &self,
        engine: &Engine,
        ast: &AST,
        dt: f32,
    ) -> Result<(), Box<EvalAltResult>> {
        let callback = self.callbacks.borrow().frame.clone();
        if let Some(callback) = callback {
            let _ = callback.call::<Dynamic>(engine, ast, (self.clone(), dt as f64))?;
        }
        Ok(())
    }
}

pub struct RhaiRuntime {
    engine: Engine,
    scope: Scope<'static>,
    ast: AST,
    viewers: Rc<RefCell<Vec<Weak<RefCell<ObjViewer>>>>>,
    bindings: Rc<RefCell<Vec<Weak<RefCell<Callbacks>>>>>,
}
impl RhaiRuntime {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.register_type_with_name::<RhaiContext>("GpuContext");
        engine.register_type_with_name::<RhaiScene>("Scene");
        engine.register_type_with_name::<RhaiDeviceTriangleMesh>("DeviceTriangleMesh");
        engine.register_type_with_name::<RhaiCamera>("Camera");
        engine.register_type_with_name::<RhaiViewer>("ObjViewer");
        engine.register_type_with_name::<RhaiInputEvent>("InputEvent");
        engine.register_fn(
            "GpuContext",
            || -> Result<RhaiContext, Box<EvalAltResult>> {
                Ok(RhaiContext(
                    pollster::block_on(GpuContext::new()).map_err(err)?,
                ))
            },
        );
        engine.register_fn(
            "GpuContext",
            |name: &str| -> Result<RhaiContext, Box<EvalAltResult>> {
                Ok(RhaiContext(
                    pollster::block_on(GpuContext::with_adapter(Some(name))).map_err(err)?,
                ))
            },
        );
        engine.register_get("adapter_name", |context: &mut RhaiContext| {
            context.0.adapter_info().name
        });
        engine.register_get("max_texture_dimension_2d", |context: &mut RhaiContext| {
            context.0.limits().max_texture_dimension_2d as i64
        });
        engine.register_fn(
            "Scene",
            |path: &str| -> Result<RhaiScene, Box<EvalAltResult>> {
                let bytes = std::fs::read(path).map_err(|e| err(e.to_string()))?;
                Ok(RhaiScene(Scene::load_obj(&bytes).map_err(err)?))
            },
        );
        engine.register_fn("Camera", || {
            RhaiCamera(Rc::new(RefCell::new(Camera::default())))
        });
        engine.register_fn("vec3", |x: f64, y: f64, z: f64| -> Array {
            vec![Dynamic::from(x), Dynamic::from(y), Dynamic::from(z)]
        });
        engine.register_fn(
            "DeviceTriangleMesh",
            |context: RhaiContext,
             scene: RhaiScene,
             index: i64|
             -> Result<RhaiDeviceTriangleMesh, Box<EvalAltResult>> {
                let mesh = scene
                    .0
                    .meshes()
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
                    .ok_or_else(|| err("triangle mesh index is out of range".into()))?;
                Ok(RhaiDeviceTriangleMesh(Arc::new(
                    DeviceTriangleMesh::from_triangle_mesh(&context.0, mesh).map_err(err)?,
                )))
            },
        );
        engine.register_fn("frame", |camera: &mut RhaiCamera, scene: RhaiScene| {
            camera.0.borrow_mut().frame_aabb(scene.0.aabb())
        });
        engine.register_fn("orbit", |camera: &mut RhaiCamera, dx: f64, dy: f64| {
            camera
                .0
                .borrow_mut()
                .orbit(Vec3::ZERO, dx as f32, dy as f32)
        });
        engine.register_fn("pan", |camera: &mut RhaiCamera, dx: f64, dy: f64| {
            camera.0.borrow_mut().pan(dx as f32, dy as f32)
        });
        engine.register_fn("zoom", |camera: &mut RhaiCamera, amount: f64| {
            camera.0.borrow_mut().zoom(amount as f32)
        });
        engine.register_fn("reset", |camera: &mut RhaiCamera| {
            camera.0.borrow_mut().reset()
        });
        engine.register_get_set(
            "position",
            |camera: &mut RhaiCamera| -> Array {
                let p = camera.0.borrow().position;
                vec![
                    Dynamic::from(p.x as f64),
                    Dynamic::from(p.y as f64),
                    Dynamic::from(p.z as f64),
                ]
            },
            |camera: &mut RhaiCamera, value: Array| {
                if value.len() == 3 {
                    camera.0.borrow_mut().position = Vec3::new(
                        value[0].as_float().unwrap_or(0.0) as f32,
                        value[1].as_float().unwrap_or(0.0) as f32,
                        value[2].as_float().unwrap_or(0.0) as f32,
                    );
                }
            },
        );
        engine.register_get_set(
            "orientation",
            |camera: &mut RhaiCamera| -> Array {
                let q = camera.0.borrow().orientation;
                vec![
                    Dynamic::from(q.x as f64),
                    Dynamic::from(q.y as f64),
                    Dynamic::from(q.z as f64),
                    Dynamic::from(q.w as f64),
                ]
            },
            |camera: &mut RhaiCamera, value: Array| {
                if value.len() == 4 {
                    camera.0.borrow_mut().orientation = Quat::from_xyzw(
                        value[0].as_float().unwrap_or(0.0) as f32,
                        value[1].as_float().unwrap_or(0.0) as f32,
                        value[2].as_float().unwrap_or(0.0) as f32,
                        value[3].as_float().unwrap_or(1.0) as f32,
                    )
                    .normalize();
                }
            },
        );
        engine.register_get_set(
            "vertical_fov",
            |camera: &mut RhaiCamera| camera.0.borrow().vertical_fov as f64,
            |camera: &mut RhaiCamera, v: f64| camera.0.borrow_mut().vertical_fov = v as f32,
        );
        engine.register_get_set(
            "near",
            |camera: &mut RhaiCamera| camera.0.borrow().near as f64,
            |camera: &mut RhaiCamera, v: f64| camera.0.borrow_mut().near = v as f32,
        );
        engine.register_get_set(
            "far",
            |camera: &mut RhaiCamera| camera.0.borrow().far as f64,
            |camera: &mut RhaiCamera, v: f64| camera.0.borrow_mut().far = v as f32,
        );
        engine.register_get("dx", |event: &mut RhaiInputEvent| event.0.dx as f64);
        engine.register_get("kind", |event: &mut RhaiInputEvent| -> &'static str {
            match event.0.kind {
                EventKind::MouseMove => "mouse_move",
                EventKind::MouseButton => "mouse_button",
                EventKind::MouseWheel => "mouse_wheel",
                EventKind::Key => "key",
                EventKind::Resize => "resize",
                EventKind::CloseRequest => "close_request",
            }
        });
        engine.register_get("dy", |event: &mut RhaiInputEvent| event.0.dy as f64);
        engine.register_get("x", |event: &mut RhaiInputEvent| event.0.x as f64);
        engine.register_get("y", |event: &mut RhaiInputEvent| event.0.y as f64);
        engine.register_get("wheel", |event: &mut RhaiInputEvent| event.0.wheel as f64);
        engine.register_get("left_pressed", |event: &mut RhaiInputEvent| {
            event.0.left_pressed
        });
        engine.register_get("middle_pressed", |event: &mut RhaiInputEvent| {
            event.0.middle_pressed
        });
        engine.register_get("right_pressed", |event: &mut RhaiInputEvent| {
            event.0.right_pressed
        });
        engine.register_get("button", |event: &mut RhaiInputEvent| {
            event.0.button.map(i64::from).unwrap_or(-1)
        });
        engine.register_get("pressed", |event: &mut RhaiInputEvent| event.0.pressed);
        engine.register_get("key", |event: &mut RhaiInputEvent| {
            event.0.key.clone().unwrap_or_default()
        });
        engine.register_get("width", |event: &mut RhaiInputEvent| event.0.width as i64);
        engine.register_get("height", |event: &mut RhaiInputEvent| event.0.height as i64);
        let viewers = Rc::new(RefCell::new(Vec::new()));
        let bindings = Rc::new(RefCell::new(Vec::new()));
        let viewer_refs = viewers.clone();
        let binding_refs = bindings.clone();
        engine.register_fn("ObjViewer", move |context: RhaiContext| {
            let viewer = Rc::new(RefCell::new(ObjViewer::new(context.0)));
            let callbacks = Rc::new(RefCell::new(Callbacks::default()));
            viewer_refs.borrow_mut().push(Rc::downgrade(&viewer));
            binding_refs.borrow_mut().push(Rc::downgrade(&callbacks));
            RhaiViewer {
                viewer,
                callbacks,
                background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
                title: Rc::new(RefCell::new("OBJ viewer".into())),
                size: Rc::new(RefCell::new((640, 480))),
            }
        });
        engine.register_fn(
            "set_scene",
            |viewer: &mut RhaiViewer, scene: RhaiScene| -> Result<(), Box<EvalAltResult>> {
                let context = viewer.viewer.borrow().context().clone();
                let meshes = scene
                    .0
                    .meshes()
                    .iter()
                    .map(|mesh| {
                        DeviceTriangleMesh::from_triangle_mesh(&context, mesh).map(Arc::new)
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(err)?;
                viewer.viewer.borrow_mut().set_device_meshes(meshes);
                Ok(())
            },
        );
        engine.register_fn(
            "set_scene",
            |viewer: &mut RhaiViewer, scene: RhaiDeviceTriangleMesh| {
                viewer.viewer.borrow_mut().set_device_mesh(scene.0);
            },
        );
        engine.register_fn(
            "set_background_color",
            |viewer: &mut RhaiViewer, r: f64, g: f64, b: f64, a: f64| {
                *viewer.background.borrow_mut() = [r as f32, g as f32, b as f32, a as f32];
            },
        );
        engine.register_fn("background_color", |viewer: &mut RhaiViewer| -> Array {
            (*viewer.background.borrow())
                .iter()
                .map(|x| Dynamic::from(*x as f64))
                .collect()
        });
        engine.register_fn("set_title", |viewer: &mut RhaiViewer, title: &str| {
            *viewer.title.borrow_mut() = title.to_owned();
        });
        engine.register_fn("title", |viewer: &mut RhaiViewer| {
            viewer.title.borrow().clone()
        });
        engine.register_fn("set_size", |viewer: &mut RhaiViewer, w: i64, h: i64| {
            *viewer.size.borrow_mut() = (w.max(1) as u32, h.max(1) as u32);
            viewer
                .viewer
                .borrow_mut()
                .resize(w.max(1) as u32, h.max(1) as u32)
        });
        engine.register_fn("size", |viewer: &mut RhaiViewer| -> Array {
            let (w, h) = *viewer.size.borrow();
            vec![Dynamic::from(w as i64), Dynamic::from(h as i64)]
        });
        engine.register_fn(
            "render",
            |viewer: &mut RhaiViewer, camera: RhaiCamera| -> Result<(), Box<EvalAltResult>> {
                viewer
                    .viewer
                    .borrow_mut()
                    .render(&camera.0.borrow(), *viewer.background.borrow())
                    .map(|_| ())
                    .map_err(err)
            },
        );
        engine.register_fn("close", |viewer: &mut RhaiViewer| {
            viewer.viewer.borrow_mut().close()
        });
        engine.register_fn(
            "on_mouse_move",
            |viewer: &mut RhaiViewer, callback: FnPtr| {
                viewer.callbacks.borrow_mut().mouse_move = Some(callback)
            },
        );
        engine.register_fn("off_mouse_move", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().mouse_move = None
        });
        engine.register_fn(
            "on_mouse_button",
            |viewer: &mut RhaiViewer, callback: FnPtr| {
                viewer.callbacks.borrow_mut().mouse_button = Some(callback)
            },
        );
        engine.register_fn("off_mouse_button", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().mouse_button = None
        });
        engine.register_fn(
            "on_mouse_wheel",
            |viewer: &mut RhaiViewer, callback: FnPtr| {
                viewer.callbacks.borrow_mut().mouse_wheel = Some(callback)
            },
        );
        engine.register_fn("off_mouse_wheel", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().mouse_wheel = None
        });
        engine.register_fn("on_key", |viewer: &mut RhaiViewer, callback: FnPtr| {
            viewer.callbacks.borrow_mut().key = Some(callback)
        });
        engine.register_fn("off_key", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().key = None
        });
        engine.register_fn("on_resize", |viewer: &mut RhaiViewer, callback: FnPtr| {
            viewer.callbacks.borrow_mut().resize = Some(callback)
        });
        engine.register_fn("off_resize", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().resize = None
        });
        engine.register_fn(
            "on_close_request",
            |viewer: &mut RhaiViewer, callback: FnPtr| {
                viewer.callbacks.borrow_mut().close_request = Some(callback)
            },
        );
        engine.register_fn("off_close_request", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().close_request = None
        });
        engine.register_fn("on_frame", |viewer: &mut RhaiViewer, callback: FnPtr| {
            viewer.callbacks.borrow_mut().frame = Some(callback)
        });
        engine.register_fn("off_frame", |viewer: &mut RhaiViewer| {
            viewer.callbacks.borrow_mut().frame = None
        });
        Self {
            engine,
            scope: Scope::new(),
            ast: AST::empty(),
            viewers,
            bindings,
        }
    }
    pub fn eval(&mut self, source: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        let ast = self.engine.compile_with_scope(&self.scope, source)?;
        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut self.scope, &ast)?;
        self.ast = self.ast.merge(&ast);
        Ok(result)
    }
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
    pub fn ast(&self) -> &AST {
        &self.ast
    }
    pub fn expose_viewer(&mut self, name: &str, viewer: Rc<RefCell<ObjViewer>>) {
        let callbacks = Rc::new(RefCell::new(Callbacks::default()));
        self.viewers.borrow_mut().push(Rc::downgrade(&viewer));
        self.bindings.borrow_mut().push(Rc::downgrade(&callbacks));
        self.scope.push(
            name,
            RhaiViewer {
                viewer,
                callbacks,
                background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
                title: Rc::new(RefCell::new("OBJ viewer".into())),
                size: Rc::new(RefCell::new((640, 480))),
            },
        );
    }
    pub fn expose_camera(&mut self, name: &str, camera: Rc<RefCell<Camera>>) {
        self.scope.push(name, RhaiCamera(camera));
    }
    pub fn expose_context(&mut self, name: &str, context: GpuContext) {
        self.scope.push(name, RhaiContext(context));
    }
    pub fn expose_scene(&mut self, name: &str, scene: Scene) {
        self.scope.push(name, RhaiScene(scene));
    }
    pub fn viewers(&self) -> Vec<RhaiViewer> {
        self.viewers
            .borrow()
            .iter()
            .zip(self.bindings.borrow().iter())
            .filter_map(|(viewer, callbacks)| {
                Some(RhaiViewer {
                    viewer: viewer.upgrade()?,
                    callbacks: callbacks.upgrade()?,
                    background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
                    title: Rc::new(RefCell::new("OBJ viewer".into())),
                    size: Rc::new(RefCell::new((640, 480))),
                })
            })
            .collect()
    }
}
impl Default for RhaiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_persists() {
        let mut runtime = RhaiRuntime::new();
        let _ = runtime.eval("let camera = Camera(); let n = 1;").unwrap();
        let _ = runtime.eval("n += 1; camera.zoom(1.0);").unwrap();
        assert_eq!(runtime.scope.get_value::<i64>("n"), Some(2));
    }
    #[test]
    fn callback_replacement() {
        if pollster::block_on(GpuContext::new()).is_err() {
            return;
        }
        let mut runtime = RhaiRuntime::new();
        let _ = runtime.eval("let context = GpuContext(); let viewer = ObjViewer(context); let camera = Camera(); viewer.on_mouse_move(|e| { camera.zoom(1.0); });").unwrap();
        let viewer = runtime.viewers().pop().unwrap();
        viewer
            .dispatch_input(
                runtime.engine(),
                runtime.ast(),
                InputEvent {
                    dx: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        let first = runtime
            .eval("camera.position[2]")
            .unwrap()
            .as_float()
            .unwrap();
        let _ = runtime
            .eval("viewer.on_mouse_move(|e| { camera.zoom(2.0); });")
            .unwrap();
        viewer
            .dispatch_input(
                runtime.engine(),
                runtime.ast(),
                InputEvent {
                    dx: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        let second = runtime
            .eval("camera.position[2]")
            .unwrap()
            .as_float()
            .unwrap();
        assert!((first - second - 2.0).abs() < 0.001);
    }
}
