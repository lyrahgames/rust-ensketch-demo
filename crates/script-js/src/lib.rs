//! Browser WebGPU bindings. JavaScript owns callbacks and animation frames.
#![cfg(target_arch = "wasm32")]
use engine::{Aabb, Camera, DeviceContext, DeviceTriangleMesh, ObjViewer, Scene, Triangle, Vertex};
use glam::Vec3;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlCanvasElement, Request, Response};

fn error(message: String) -> JsValue { JsValue::from_str(&message) }

#[wasm_bindgen(js_name = DeviceContext)]
pub struct JsDeviceContext {
  inner: DeviceContext,
}
#[wasm_bindgen(js_class = DeviceContext)]
impl JsDeviceContext {
  #[wasm_bindgen(js_name = create)]
  pub async fn create() -> Result<JsDeviceContext, JsValue> {
    Ok(Self {
      inner: DeviceContext::new().await.map_err(error)?,
    })
  }
  #[wasm_bindgen(js_name = adapterName)]
  pub fn adapter_name(&self) -> String { self.inner.adapter_info().name }
  #[wasm_bindgen(js_name = maxTextureDimension2d)]
  pub fn max_texture_dimension_2d(&self) -> u32 { self.inner.limits().max_texture_dimension_2d }
}

#[wasm_bindgen(js_name = Scene)]
pub struct JsScene {
  inner: Scene,
}
#[wasm_bindgen(js_class = Scene)]
impl JsScene {
  #[wasm_bindgen(js_name = loadObj)]
  pub fn load_obj(bytes: &[u8]) -> Result<JsScene, JsValue> {
    Ok(Self {
      inner: Scene::load_obj(bytes).map_err(error)?,
    })
  }
  #[wasm_bindgen(js_name = loadObjUrl)]
  pub async fn load_obj_url(url: String) -> Result<JsScene, JsValue> {
    let window = web_sys::window().ok_or_else(|| error("no browser window".into()))?;
    let request = Request::new_with_str(&url)?;
    let response = JsFuture::from(window.fetch_with_request(&request)).await?;
    let response: Response = response.dyn_into()?;
    if !response.ok() {
      return Err(error(format!(
        "OBJ fetch failed: HTTP {}",
        response.status()
      )));
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    Self::load_obj(&bytes)
  }
}

#[wasm_bindgen(js_name = DeviceTriangleMesh)]
pub struct JsDeviceTriangleMesh {
  inner: Arc<DeviceTriangleMesh<Vertex, Triangle>>,
}
#[wasm_bindgen(js_class = DeviceTriangleMesh)]
impl JsDeviceTriangleMesh {
  #[wasm_bindgen(js_name = fromScene)]
  pub fn from_scene(
    context: &JsDeviceContext,
    scene: &JsScene,
    mesh_index: usize,
  ) -> Result<JsDeviceTriangleMesh, JsValue> {
    let mesh = scene
      .inner
      .meshes()
      .get(mesh_index)
      .ok_or_else(|| error("triangle mesh index is out of range".into()))?;
    Ok(Self {
      inner: Arc::new(context.inner.upload(mesh).map_err(error)?),
    })
  }
}

#[wasm_bindgen(js_name = Camera)]
pub struct JsCamera {
  inner: Camera,
}
#[wasm_bindgen(js_class = Camera)]
impl JsCamera {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    Self {
      inner: Camera::default(),
    }
  }
  pub fn frame(&mut self, scene: &JsScene) { self.inner.frame_aabb(Aabb::from(&scene.inner)); }
  pub fn orbit(&mut self, dx: f32, dy: f32) { self.inner.orbit(Vec3::ZERO, dx, dy); }
  pub fn pan(&mut self, dx: f32, dy: f32) { self.inner.pan(dx, dy); }
  pub fn zoom(&mut self, amount: f32) { self.inner.zoom(amount); }
  pub fn reset(&mut self) { self.inner.reset(); }
  #[wasm_bindgen(js_name = setPosition)]
  pub fn set_position(&mut self, x: f32, y: f32, z: f32) {
    self.inner.position = Vec3::new(x, y, z);
  }
  #[wasm_bindgen(js_name = setOrientation)]
  pub fn set_orientation(&mut self, x: f32, y: f32, z: f32, w: f32) {
    self.inner.orientation = glam::Quat::from_xyzw(x, y, z, w).normalize();
  }
  #[wasm_bindgen(js_name = setPerspective)]
  pub fn set_perspective(&mut self, vertical_fov: f32, near: f32, far: f32) {
    self.inner.vertical_fov = vertical_fov;
    self.inner.near = near;
    self.inner.far = far;
  }
}
impl Default for JsCamera {
  fn default() -> Self { Self::new() }
}

#[wasm_bindgen(js_name = ObjViewer)]
pub struct JsObjViewer {
  inner:      ObjViewer,
  canvas:     HtmlCanvasElement,
  background: [f32; 4],
}
#[wasm_bindgen(js_class = ObjViewer)]
impl JsObjViewer {
  #[wasm_bindgen(constructor)]
  pub fn new(context: &JsDeviceContext, canvas: HtmlCanvasElement) -> Result<JsObjViewer, JsValue> {
    let width = canvas.width().max(1);
    let height = canvas.height().max(1);
    let surface = context
      .inner
      .instance()
      .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
      .map_err(|e| error(e.to_string()))?;
    let mut inner = ObjViewer::new(context.inner.clone());
    inner
      .attach_surface(surface, width, height)
      .map_err(error)?;
    Ok(Self {
      inner,
      canvas,
      background: [0.08, 0.09, 0.12, 1.0],
    })
  }
  #[wasm_bindgen(js_name = setScene)]
  pub fn set_scene(&mut self, scene: &JsScene) -> Result<(), JsValue> {
    let meshes = scene
      .inner
      .meshes()
      .iter()
      .map(|mesh| self.inner.context().upload(mesh).map(Arc::new))
      .collect::<std::result::Result<Vec<_>, _>>()
      .map_err(error)?;
    self.inner.set_device_meshes(meshes);
    Ok(())
  }
  #[wasm_bindgen(js_name = setDeviceMesh)]
  pub fn set_device_mesh(&mut self, mesh: &JsDeviceTriangleMesh) {
    self.inner.set_device_mesh(mesh.inner.clone());
  }
  #[wasm_bindgen(js_name = setBackgroundColor)]
  pub fn set_background_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
    self.background = [r, g, b, a];
  }
  #[wasm_bindgen(js_name = backgroundColor)]
  pub fn background_color(&self) -> Vec<f32> { self.background.to_vec() }
  #[wasm_bindgen(js_name = setTitle)]
  pub fn set_title(&mut self, title: String) { self.canvas.set_title(&title); }
  pub fn title(&self) -> String { self.canvas.title() }
  pub fn render(&mut self, camera: &JsCamera) -> Result<(), JsValue> {
    if self
      .inner
      .render(&camera.inner, self.background)
      .map_err(error)?
      == engine::RenderStatus::SurfaceLost
    {
      let surface = self
        .inner
        .context()
        .instance()
        .create_surface(wgpu::SurfaceTarget::Canvas(self.canvas.clone()))
        .map_err(|e| error(e.to_string()))?;
      self
        .inner
        .attach_surface(surface, self.canvas.width(), self.canvas.height())
        .map_err(error)?;
    }
    Ok(())
  }
  pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
    self.canvas.set_width(width);
    self.canvas.set_height(height);
    self.inner.resize(width, height).map_err(error)
  }
  pub fn close(&mut self) { self.inner.close(); }
}
