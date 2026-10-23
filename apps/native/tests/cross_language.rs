#[cfg(all(feature = "lua", feature = "rhai"))]
#[test]
fn lua_and_rhai_bind_the_same_engine_viewer() {
    use std::{cell::RefCell, rc::Rc};
    let Ok(context) = pollster::block_on(engine::GpuContext::new()) else {
        return;
    };
    let viewer = Rc::new(RefCell::new(engine::ObjViewer::new(context)));
    let scene = engine::Scene::load_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let lua = script_lua::LuaRuntime::new().unwrap();
    let mut rhai = script_rhai::RhaiRuntime::new();
    lua.expose_viewer("viewer", viewer.clone()).unwrap();
    lua.expose_scene("scene", scene.clone()).unwrap();
    rhai.expose_viewer("viewer", viewer.clone());
    rhai.expose_scene("scene", scene);
    lua.eval("viewer:set_scene(scene)").unwrap();
    assert_eq!(viewer.borrow().mesh_count(), 1);
    let value = rhai.eval("viewer.set_scene(scene)").unwrap();
    assert!(value.is_unit());
    assert_eq!(viewer.borrow().mesh_count(), 1);
}
