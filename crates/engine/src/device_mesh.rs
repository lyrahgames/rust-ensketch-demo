use crate::{GpuContext, Result, Triangle, TriangleMesh, Vertex};
use bytemuck::Pod;
use std::marker::PhantomData;
use wgpu::util::DeviceExt;

pub struct GpuBuffer<T> {
    raw: wgpu::Buffer,
    marker: PhantomData<fn() -> T>,
}
impl<T> GpuBuffer<T> {
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.raw
    }
    pub fn size(&self) -> u64 {
        self.raw.size()
    }
}

/// GPU-side triangle data with compile-time vertex and element types. It owns
/// only its two typed buffers; element counts are derived from buffer size.
pub struct DeviceTriangleMesh<V, E> {
    vertex_buffer: GpuBuffer<V>,
    element_buffer: GpuBuffer<E>,
}
impl<V: Pod, E: Pod> DeviceTriangleMesh<V, E> {
    pub fn upload(context: &GpuContext, vertices: &[V], elements: &[E]) -> Result<Self> {
        let vertex_size = std::mem::size_of::<V>();
        let element_size = std::mem::size_of::<E>();
        if vertex_size == 0 || element_size == 0 {
            return Err("device triangle mesh buffer elements cannot be zero-sized".into());
        }
        if !vertex_size.is_multiple_of(4) || !element_size.is_multiple_of(4) {
            return Err("device triangle mesh element sizes must be four-byte aligned".into());
        }
        if vertices.is_empty() || elements.is_empty() {
            return Err("device triangle mesh buffers cannot be empty".into());
        }
        let vertex_buffer =
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("triangle mesh vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
        let element_buffer =
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("triangle mesh elements"),
                    contents: bytemuck::cast_slice(elements),
                    usage: wgpu::BufferUsages::INDEX,
                });
        Ok(Self {
            vertex_buffer: GpuBuffer {
                raw: vertex_buffer,
                marker: PhantomData,
            },
            element_buffer: GpuBuffer {
                raw: element_buffer,
                marker: PhantomData,
            },
        })
    }
    pub fn vertex_buffer(&self) -> &GpuBuffer<V> {
        &self.vertex_buffer
    }
    pub fn element_buffer(&self) -> &GpuBuffer<E> {
        &self.element_buffer
    }
    pub fn element_count(&self) -> u32 {
        (self.element_buffer.raw.size() / std::mem::size_of::<E>() as u64) as u32
    }
}
impl DeviceTriangleMesh<Vertex, Triangle> {
    pub fn from_triangle_mesh(context: &GpuContext, mesh: &TriangleMesh) -> Result<Self> {
        if mesh.topology.0.iter().any(|triangle| {
            triangle
                .0
                .iter()
                .any(|index| index.0 as usize >= mesh.geometry.len())
        }) {
            return Err("triangle mesh contains an out-of-range vertex index".into());
        }
        Self::upload(context, &mesh.geometry.0, &mesh.topology.0)
    }
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn gpu_upload_creates_typed_triangle_buffers() {
        let Ok(context) = pollster::block_on(GpuContext::new()) else {
            return;
        };
        let scene = Scene::load_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let mesh = DeviceTriangleMesh::from_triangle_mesh(&context, &scene.meshes()[0]).unwrap();
        assert_eq!(mesh.element_count(), 1);
        assert_eq!(
            mesh.vertex_buffer().size(),
            3 * std::mem::size_of::<Vertex>() as u64
        );
    }
}
