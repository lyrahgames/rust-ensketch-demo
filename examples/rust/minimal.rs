use engine::{Camera, GpuContext, HostScene, ObjViewer};
use native::NativeApp;

fn main() -> Result<(), String> {
    let context = pollster::block_on(GpuContext::new())?;
    let scene = HostScene::load_obj(include_bytes!("../../assets/models/pyramid.obj"))?;
    let mut viewer = ObjViewer::new(context);
    viewer.set_host_scene(&scene)?;
    viewer.set_title("Minimal engine viewer");
    NativeApp::show_obj_viewer(viewer, Camera::from_bounds(scene.bounds()))
}
