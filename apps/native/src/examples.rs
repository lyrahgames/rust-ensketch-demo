//! Runnable Rust examples built on the engine and native host APIs.
use crate::{NativeApp, ViewerConfig};
use engine::{Aabb, Camera, DeviceContext, ObjViewer, Scene};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Run one of the four Rust parity demonstrations.
pub fn run_case(case: usize) -> Result<(), String> {
  if !(1..=4).contains(&case) {
    return Err("case must be 1..=4".into());
  }
  let context = pollster::block_on(DeviceContext::new())?;
  let first = Scene::load_obj(include_bytes!("../../../assets/models/pyramid.obj"))?;
  let second = Scene::load_obj(include_bytes!("../../../assets/models/cube.obj"))?;
  let first_meshes: Vec<_> = first
    .meshes()
    .iter()
    .map(|mesh| context.upload(mesh).map(Arc::new))
    .collect::<engine::Result<_>>()?;
  let second_meshes: Vec<_> = second
    .meshes()
    .iter()
    .map(|mesh| context.upload(mesh).map(Arc::new))
    .collect::<engine::Result<_>>()?;
  let shared_camera = Rc::new(RefCell::new(Camera::from_aabb(Aabb::from(&first))));
  let mut app = NativeApp::new();
  let count = if case == 1 { 1 } else { 2 };
  for i in 0..count {
    let mut viewer = ObjViewer::new(context.clone());
    match case {
      2 if i == 1 => viewer.set_device_meshes(second_meshes.clone()),
      4 => viewer.set_device_mesh(first_meshes[0].clone()),
      _ => viewer.set_device_meshes(first_meshes.clone()),
    }
    let camera = if case == 4 || i == 0 {
      shared_camera.clone()
    } else {
      Rc::new(RefCell::new(Camera::from_aabb(if case == 2 {
        Aabb::from(&second)
      } else {
        Aabb::from(&first)
      })))
    };
    let mut config = ViewerConfig::new(Rc::new(RefCell::new(viewer)), camera.clone());
    config.title = format!("Rust case {case} — viewer {}", i + 1);
    config.size = if case == 4 && i == 1 {
      (900, 400)
    } else {
      (640, 480)
    };
    config.background = if i == 0 {
      [0.08, 0.09, 0.12, 1.0]
    } else {
      [0.17, 0.08, 0.12, 1.0]
    };
    app.add_viewer(config);
  }
  app.show()
}
