#[cfg(all(feature = "lua", feature = "rhai"))]
#[test]
fn lua_update_is_visible_to_rhai() {
    use std::{cell::RefCell, rc::Rc};
    let Ok(context) = pollster::block_on(engine::GpuContext::new()) else {
        return;
    };
    let viewer = Rc::new(RefCell::new(engine::ObjViewer::new(context)));
    let lua = script_lua::LuaRuntime::new().unwrap();
    let mut rhai = script_rhai::RhaiRuntime::new();
    lua.expose_viewer("viewer", viewer.clone()).unwrap();
    rhai.expose_viewer("viewer", viewer);
    lua.eval("viewer:set_background_color(0.2, 0.3, 0.4, 1)")
        .unwrap();
    let value = rhai.eval("viewer.background_color()[0]").unwrap();
    assert!((value.as_float().unwrap() - 0.2).abs() < 0.0001);
}
