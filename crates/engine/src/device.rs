use crate::{Result, Triangle, TriangleMesh, Vertex};
use bytemuck::Pod;
use std::marker::PhantomData;
use wgpu::util::DeviceExt;

#[derive(Clone)]
pub struct DeviceContext {
  pub instance: wgpu::Instance,
  pub adapter:  wgpu::Adapter,
  pub device:   wgpu::Device,
  pub queue:    wgpu::Queue,
}

/// Converts host-side data into device-side data using this device context.
pub trait DeviceFromHost<Host, Device> {
  fn from(&self, host: &Host) -> Result<Device>;
}

impl DeviceContext {
  pub async fn new() -> Result<Self> { Self::with_adapter(None).await }
  pub async fn with_adapter(name: Option<&str>) -> Result<Self> {
    let instance = wgpu::Instance::default();
    let adapter = if let Some(name) = name {
      #[cfg(not(target_arch = "wasm32"))]
      {
        instance
          .enumerate_adapters(wgpu::Backends::all())
          .await
          .into_iter()
          .find(|a| {
            a.get_info()
              .name
              .to_lowercase()
              .contains(&name.to_lowercase())
          })
          .ok_or_else(|| format!("no adapter matching {name:?}"))?
      }
      #[cfg(target_arch = "wasm32")]
      {
        return Err(format!(
          "named adapter selection is unavailable in browsers: {name}"
        ));
      }
    } else {
      instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .map_err(|e| e.to_string())?
    };
    let (device, queue) = adapter
      .request_device(&wgpu::DeviceDescriptor::default())
      .await
      .map_err(|e| e.to_string())?;
    Ok(Self {
      instance,
      adapter,
      device,
      queue,
    })
  }
  pub fn adapter_info(&self) -> wgpu::AdapterInfo { self.adapter.get_info() }
  pub fn limits(&self) -> wgpu::Limits { self.device.limits() }
  pub fn features(&self) -> wgpu::Features { self.device.features() }
  pub fn device(&self) -> &wgpu::Device { &self.device }
  pub fn queue(&self) -> &wgpu::Queue { &self.queue }
  pub fn instance(&self) -> &wgpu::Instance { &self.instance }

  pub fn upload<Host, Device>(&self, host: &Host) -> Result<Device>
  where
    Self: DeviceFromHost<Host, Device>, {
    <Self as DeviceFromHost<Host, Device>>::from(self, host)
  }
}

pub struct DeviceVec<T> {
  raw:    wgpu::Buffer,
  marker: PhantomData<fn() -> T>,
}
impl<T> DeviceVec<T> {
  pub fn buffer(&self) -> &wgpu::Buffer { &self.raw }
  pub fn len(&self) -> usize { (self.raw.size() / std::mem::size_of::<T>() as u64) as usize }
  pub fn size(&self) -> usize { self.len() }
  pub fn is_empty(&self) -> bool { self.len() == 0 }
  pub fn byte_size(&self) -> u64 { self.raw.size() }
}

/// GPU-side triangle data with compile-time vertex and element types. It owns
/// only its two typed buffers; element counts are derived from buffer size.
pub struct DeviceTriangleMesh<V, E> {
  vertex_buffer:  DeviceVec<V>,
  element_buffer: DeviceVec<E>,
}
impl<V: Pod, E: Pod> DeviceTriangleMesh<V, E> {
  pub fn vertex_buffer(&self) -> &DeviceVec<V> { &self.vertex_buffer }
  pub fn element_buffer(&self) -> &DeviceVec<E> { &self.element_buffer }
  pub fn element_count(&self) -> u32 { self.element_buffer.len() as u32 }
}
impl DeviceFromHost<TriangleMesh, DeviceTriangleMesh<Vertex, Triangle>> for DeviceContext {
  fn from(&self, mesh: &TriangleMesh) -> Result<DeviceTriangleMesh<Vertex, Triangle>> {
    let vertex_buffer = self
      .device()
      .create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("triangle mesh vertices"),
        contents: bytemuck::cast_slice(&mesh.geometry.0),
        usage:    wgpu::BufferUsages::VERTEX,
      });
    let element_buffer = self
      .device()
      .create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("triangle mesh elements"),
        contents: bytemuck::cast_slice(&mesh.topology.0),
        usage:    wgpu::BufferUsages::INDEX,
      });
    Ok(DeviceTriangleMesh {
      vertex_buffer:  DeviceVec {
        raw:    vertex_buffer,
        marker: PhantomData,
      },
      element_buffer: DeviceVec {
        raw:    element_buffer,
        marker: PhantomData,
      },
    })
  }
}
impl Vertex {
  pub fn layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
      wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
    wgpu::VertexBufferLayout {
      array_stride: std::mem::size_of::<Self>() as u64,
      step_mode:    wgpu::VertexStepMode::Vertex,
      attributes:   &ATTRIBUTES,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::{DeviceContext, DeviceTriangleMesh, Scene, Triangle, Vertex};

  #[test]
  fn gpu_upload_creates_typed_triangle_buffers() {
    let Ok(context) = pollster::block_on(DeviceContext::new()) else {
      return;
    };
    let scene = Scene::load_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let mesh: DeviceTriangleMesh<Vertex, Triangle> = context.upload(&scene.meshes()[0]).unwrap();
    assert_eq!(mesh.element_count(), 1);
    assert_eq!(
      mesh.vertex_buffer().byte_size(),
      3 * std::mem::size_of::<Vertex>() as u64
    );
    assert_eq!(mesh.vertex_buffer().size(), 3);
    assert_eq!(mesh.vertex_buffer().len(), 3);
  }
}
