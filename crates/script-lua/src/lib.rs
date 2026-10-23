//! Persistent Lua bindings for script-owned engine objects.
use engine::{Camera, DeviceScene, EventKind, GpuContext, HostScene, InputEvent, ObjViewer};
use glam::{Quat, Vec3};
use mlua::{AnyUserData, Function, Lua, Result, Table, UserData, UserDataFields, UserDataMethods};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

fn err(message: String) -> mlua::Error {
    mlua::Error::RuntimeError(message)
}

#[derive(Clone)]
pub struct LuaContext(pub GpuContext);
impl UserData for LuaContext {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("adapter_name", |_, this| Ok(this.0.adapter_info().name));
        fields.add_field_method_get("max_texture_dimension_2d", |_, this| {
            Ok(this.0.limits().max_texture_dimension_2d)
        });
    }
}

#[derive(Clone)]
pub struct LuaHostScene(pub HostScene);
impl UserData for LuaHostScene {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("upload", |_, this, context: AnyUserData| {
            let context = context.borrow::<LuaContext>()?;
            Ok(LuaDeviceScene(this.0.upload(&context.0).map_err(err)?))
        });
    }
}
#[derive(Clone)]
pub struct LuaDeviceScene(pub DeviceScene);
impl UserData for LuaDeviceScene {}

#[derive(Clone)]
pub struct LuaCamera(pub Rc<RefCell<Camera>>);
impl UserData for LuaCamera {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("position", |lua, this| {
            let p = this.0.borrow().position;
            let table = lua.create_table()?;
            table.set("x", p.x)?;
            table.set("y", p.y)?;
            table.set("z", p.z)?;
            Ok(table)
        });
        fields.add_field_method_set("position", |_, this, value: Table| {
            this.0.borrow_mut().position =
                Vec3::new(value.get("x")?, value.get("y")?, value.get("z")?);
            Ok(())
        });
        fields.add_field_method_get("orientation", |lua, this| {
            let q = this.0.borrow().orientation;
            let table = lua.create_table()?;
            table.set("x", q.x)?;
            table.set("y", q.y)?;
            table.set("z", q.z)?;
            table.set("w", q.w)?;
            Ok(table)
        });
        fields.add_field_method_set("orientation", |_, this, value: Table| {
            this.0.borrow_mut().orientation = Quat::from_xyzw(
                value.get("x")?,
                value.get("y")?,
                value.get("z")?,
                value.get("w")?,
            )
            .normalize();
            Ok(())
        });
        fields.add_field_method_get("vertical_fov", |_, this| Ok(this.0.borrow().vertical_fov));
        fields.add_field_method_set("vertical_fov", |_, this, value: f32| {
            this.0.borrow_mut().vertical_fov = value;
            Ok(())
        });
        fields.add_field_method_get("near", |_, this| Ok(this.0.borrow().near));
        fields.add_field_method_set("near", |_, this, value: f32| {
            this.0.borrow_mut().near = value;
            Ok(())
        });
        fields.add_field_method_get("far", |_, this| Ok(this.0.borrow().far));
        fields.add_field_method_set("far", |_, this, value: f32| {
            this.0.borrow_mut().far = value;
            Ok(())
        });
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("orbit", |_, this, (dx, dy): (f32, f32)| {
            this.0.borrow_mut().orbit(Vec3::ZERO, dx, dy);
            Ok(())
        });
        methods.add_method("pan", |_, this, (dx, dy): (f32, f32)| {
            this.0.borrow_mut().pan(dx, dy);
            Ok(())
        });
        methods.add_method("zoom", |_, this, amount: f32| {
            this.0.borrow_mut().zoom(amount);
            Ok(())
        });
        methods.add_method("frame", |_, this, scene: AnyUserData| {
            this.0
                .borrow_mut()
                .frame_aabb(scene.borrow::<LuaHostScene>()?.0.aabb());
            Ok(())
        });
        methods.add_method("reset", |_, this, ()| {
            this.0.borrow_mut().reset();
            Ok(())
        });
    }
}

#[derive(Default)]
struct Callbacks {
    mouse_move: Option<Function>,
    mouse_button: Option<Function>,
    mouse_wheel: Option<Function>,
    key: Option<Function>,
    resize: Option<Function>,
    close_request: Option<Function>,
    frame: Option<Function>,
}
#[derive(Clone)]
pub struct LuaViewer {
    pub viewer: Rc<RefCell<ObjViewer>>,
    callbacks: Rc<RefCell<Callbacks>>,
    background: Rc<RefCell<[f32; 4]>>,
    title: Rc<RefCell<String>>,
    size: Rc<RefCell<(u32, u32)>>,
}
impl LuaViewer {
    pub fn dispatch_input(&self, lua: &Lua, event: InputEvent) -> Result<()> {
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
            let table = lua.create_table()?;
            table.set(
                "kind",
                match event.kind {
                    EventKind::MouseMove => "mouse_move",
                    EventKind::MouseButton => "mouse_button",
                    EventKind::MouseWheel => "mouse_wheel",
                    EventKind::Key => "key",
                    EventKind::Resize => "resize",
                    EventKind::CloseRequest => "close_request",
                },
            )?;
            table.set("dx", event.dx)?;
            table.set("dy", event.dy)?;
            table.set("x", event.x)?;
            table.set("y", event.y)?;
            table.set("wheel", event.wheel)?;
            table.set("left_pressed", event.left_pressed)?;
            table.set("middle_pressed", event.middle_pressed)?;
            table.set("right_pressed", event.right_pressed)?;
            table.set("button", event.button)?;
            table.set("pressed", event.pressed)?;
            table.set("key", event.key)?;
            table.set("width", event.width)?;
            table.set("height", event.height)?;
            callback.call::<()>(table)?;
        }
        Ok(())
    }
    pub fn dispatch_frame(&self, dt: f32) -> Result<()> {
        let callback = self.callbacks.borrow().frame.clone();
        if let Some(callback) = callback {
            callback.call::<()>(dt)?;
        }
        Ok(())
    }
}
impl UserData for LuaViewer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_scene", |_, this, scene: AnyUserData| {
            if let Ok(host) = scene.borrow::<LuaHostScene>() {
                this.viewer
                    .borrow_mut()
                    .set_host_scene(&host.0)
                    .map_err(err)
            } else if let Ok(device) = scene.borrow::<LuaDeviceScene>() {
                this.viewer
                    .borrow_mut()
                    .set_device_scene(device.0.clone())
                    .map_err(err)
            } else {
                Err(err("expected HostScene or DeviceScene".into()))
            }
        });
        methods.add_method(
            "set_background_color",
            |_, this, (r, g, b, a): (f32, f32, f32, f32)| {
                *this.background.borrow_mut() = [r, g, b, a];
                Ok(())
            },
        );
        methods.add_method("background_color", |_, this, ()| {
            let c = *this.background.borrow();
            Ok((c[0], c[1], c[2], c[3]))
        });
        methods.add_method("set_title", |_, this, title: String| {
            *this.title.borrow_mut() = title;
            Ok(())
        });
        methods.add_method("title", |_, this, ()| Ok(this.title.borrow().clone()));
        methods.add_method("set_size", |_, this, (w, h): (u32, u32)| {
            *this.size.borrow_mut() = (w, h);
            let _ = this.viewer.borrow_mut().resize(w, h);
            Ok(())
        });
        methods.add_method("size", |_, this, ()| Ok(*this.size.borrow()));
        methods.add_method("render", |_, this, camera: AnyUserData| {
            this.viewer
                .borrow_mut()
                .render(
                    &camera.borrow::<LuaCamera>()?.0.borrow(),
                    *this.background.borrow(),
                )
                .map(|_| ())
                .map_err(err)
        });
        methods.add_method("close", |_, this, ()| {
            this.viewer.borrow_mut().close();
            Ok(())
        });
        methods.add_method("on_mouse_move", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().mouse_move = callback;
            Ok(())
        });
        methods.add_method("on_mouse_button", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().mouse_button = callback;
            Ok(())
        });
        methods.add_method("on_mouse_wheel", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().mouse_wheel = callback;
            Ok(())
        });
        methods.add_method("on_key", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().key = callback;
            Ok(())
        });
        methods.add_method("on_resize", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().resize = callback;
            Ok(())
        });
        methods.add_method("on_close_request", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().close_request = callback;
            Ok(())
        });
        methods.add_method("on_frame", |_, this, callback: Option<Function>| {
            this.callbacks.borrow_mut().frame = callback;
            Ok(())
        });
    }
}

pub struct LuaRuntime {
    lua: Lua,
    viewers: Rc<RefCell<Vec<Weak<RefCell<ObjViewer>>>>>,
    bindings: Rc<RefCell<Vec<Weak<RefCell<Callbacks>>>>>,
}
impl LuaRuntime {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        let viewers = Rc::new(RefCell::new(Vec::new()));
        let bindings = Rc::new(RefCell::new(Vec::new()));
        let globals = lua.globals();
        globals.set(
            "vec3",
            lua.create_function(|lua, (x, y, z): (f32, f32, f32)| {
                let value = lua.create_table()?;
                value.set("x", x)?;
                value.set("y", y)?;
                value.set("z", z)?;
                Ok(value)
            })?,
        )?;
        let context_class = lua.create_table()?;
        context_class.set(
            "new",
            lua.create_function(|_, adapter: Option<String>| {
                Ok(LuaContext(
                    pollster::block_on(GpuContext::with_adapter(adapter.as_deref()))
                        .map_err(err)?,
                ))
            })?,
        )?;
        globals.set("GpuContext", context_class)?;
        let scene_class = lua.create_table()?;
        scene_class.set(
            "load_obj",
            lua.create_function(|_, path: String| {
                let data = std::fs::read(path).map_err(mlua::Error::external)?;
                Ok(LuaHostScene(HostScene::load_obj(&data).map_err(err)?))
            })?,
        )?;
        globals.set("HostScene", scene_class)?;
        let camera_class = lua.create_table()?;
        camera_class.set(
            "new",
            lua.create_function(|_, ()| Ok(LuaCamera(Rc::new(RefCell::new(Camera::default())))))?,
        )?;
        globals.set("Camera", camera_class)?;
        let viewer_class = lua.create_table()?;
        let viewer_refs = viewers.clone();
        let binding_refs = bindings.clone();
        viewer_class.set(
            "new",
            lua.create_function(move |_, context: AnyUserData| {
                let context = context.borrow::<LuaContext>()?.0.clone();
                let viewer = Rc::new(RefCell::new(ObjViewer::new(context)));
                let callbacks = Rc::new(RefCell::new(Callbacks::default()));
                viewer_refs.borrow_mut().push(Rc::downgrade(&viewer));
                binding_refs.borrow_mut().push(Rc::downgrade(&callbacks));
                Ok(LuaViewer {
                    viewer,
                    callbacks,
                    background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
                    title: Rc::new(RefCell::new("OBJ viewer".into())),
                    size: Rc::new(RefCell::new((640, 480))),
                })
            })?,
        )?;
        globals.set("ObjViewer", viewer_class)?;
        drop(globals);
        Ok(Self {
            lua,
            viewers,
            bindings,
        })
    }
    pub fn eval(&self, source: &str) -> Result<()> {
        self.lua.load(source).exec()
    }
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
    pub fn expose_viewer(&self, name: &str, viewer: Rc<RefCell<ObjViewer>>) -> Result<()> {
        let callbacks = Rc::new(RefCell::new(Callbacks::default()));
        self.viewers.borrow_mut().push(Rc::downgrade(&viewer));
        self.bindings.borrow_mut().push(Rc::downgrade(&callbacks));
        self.lua.globals().set(
            name,
            LuaViewer {
                viewer,
                callbacks,
                background: Rc::new(RefCell::new([0.08, 0.09, 0.12, 1.0])),
                title: Rc::new(RefCell::new("OBJ viewer".into())),
                size: Rc::new(RefCell::new((640, 480))),
            },
        )
    }
    pub fn expose_camera(&self, name: &str, camera: Rc<RefCell<Camera>>) -> Result<()> {
        self.lua.globals().set(name, LuaCamera(camera))
    }
    pub fn expose_context(&self, name: &str, context: GpuContext) -> Result<()> {
        self.lua.globals().set(name, LuaContext(context))
    }
    pub fn expose_host_scene(&self, name: &str, scene: HostScene) -> Result<()> {
        self.lua.globals().set(name, LuaHostScene(scene))
    }
    pub fn viewers(&self) -> Vec<LuaViewer> {
        self.viewers
            .borrow()
            .iter()
            .zip(self.bindings.borrow().iter())
            .filter_map(|(viewer, callbacks)| {
                Some(LuaViewer {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn globals_persist() {
        let runtime = LuaRuntime::new().unwrap();
        runtime.eval("camera = Camera.new(); n = 1").unwrap();
        runtime.eval("n = n + 1; camera:zoom(1)").unwrap();
        assert_eq!(runtime.lua.globals().get::<i32>("n").unwrap(), 2);
    }
    #[test]
    fn callback_replacement() {
        if pollster::block_on(GpuContext::new()).is_err() {
            return;
        }
        let runtime = LuaRuntime::new().unwrap();
        runtime.eval("context = GpuContext.new(); viewer = ObjViewer.new(context); hits = 0; viewer:on_mouse_move(function(e) hits = hits + 1 end)").unwrap();
        let viewer = runtime.viewers().pop().unwrap();
        viewer
            .dispatch_input(
                runtime.lua(),
                InputEvent {
                    dx: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        runtime
            .eval("viewer:on_mouse_move(function(e) hits = hits + 10 end)")
            .unwrap();
        viewer
            .dispatch_input(
                runtime.lua(),
                InputEvent {
                    dx: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(runtime.lua.globals().get::<i32>("hits").unwrap(), 11);
    }
}
