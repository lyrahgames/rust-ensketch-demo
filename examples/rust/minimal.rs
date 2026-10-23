use engine::{Aabb, Camera, DeviceTriangleMesh, GpuContext, ObjViewer, Scene};
use native::NativeApp;
use std::sync::Arc;

fn main() -> Result<(), String> {
  let context = pollster::block_on(GpuContext::new())?;
  let scene = Scene::load_obj(include_bytes!("../../assets/models/pyramid.obj"))?;
  let mut viewer = ObjViewer::new(context.clone());
  let meshes = scene
    .meshes()
    .iter()
    .map(|mesh| DeviceTriangleMesh::from_triangle_mesh(&context, mesh).map(Arc::new))
    .collect::<engine::Result<Vec<_>>>()?;
  viewer.set_device_meshes(meshes);
  NativeApp::show_obj_viewer(viewer, Camera::from_aabb(Aabb::from(&scene)))
}
