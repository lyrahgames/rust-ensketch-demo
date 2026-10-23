use crate::{GpuContext, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::{
    io::BufReader,
    sync::{Arc, Mutex, Weak},
};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}
impl Bounds {
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn radius(self) -> f32 {
        (self.max - self.center()).length()
    }
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub name: String,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub texcoords: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
    pub bounds: Bounds,
}

#[derive(Clone)]
pub struct HostScene(Arc<HostInner>);
struct HostInner {
    meshes: Vec<Mesh>,
    bounds: Bounds,
    uploads: Mutex<Vec<Weak<DeviceInner>>>,
}

impl HostScene {
    pub fn load_obj(bytes: &[u8]) -> Result<Self> {
        let mut reader = BufReader::new(bytes);
        let (models, _) = tobj::load_obj_buf(
            &mut reader,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
            |_| Ok((Vec::new(), Default::default())),
        )
        .map_err(|e| format!("invalid OBJ: {e}"))?;
        let mut meshes = Vec::new();
        let mut all_min = Vec3::splat(f32::INFINITY);
        let mut all_max = Vec3::splat(f32::NEG_INFINITY);
        for model in models {
            let source = model.mesh;
            if source.positions.len() % 3 != 0 || source.indices.len() % 3 != 0 {
                return Err(format!("mesh {} has malformed triangles", model.name));
            }
            let positions: Vec<_> = source
                .positions
                .as_chunks::<3>()
                .0
                .iter()
                .map(|v| Vec3::new(v[0], v[1], v[2]))
                .collect();
            if positions.is_empty() || source.indices.is_empty() {
                continue;
            }
            if positions.iter().any(|p| !p.is_finite()) {
                return Err(format!("mesh {} has non-finite positions", model.name));
            }
            if source
                .indices
                .iter()
                .any(|&i| i as usize >= positions.len())
            {
                return Err(format!(
                    "mesh {} contains an out-of-range index",
                    model.name
                ));
            }
            let mut normals = if source.normals.len() == source.positions.len() {
                source
                    .normals
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|v| Vec3::new(v[0], v[1], v[2]))
                    .collect()
            } else {
                vec![Vec3::ZERO; positions.len()]
            };
            if source.normals.len() != source.positions.len() {
                for face in source.indices.as_chunks::<3>().0 {
                    let a = positions[face[0] as usize];
                    let b = positions[face[1] as usize];
                    let c = positions[face[2] as usize];
                    let n = (b - a).cross(c - a);
                    for &i in face {
                        normals[i as usize] += n;
                    }
                }
                for n in &mut normals {
                    *n = if n.length_squared() > 0.0 {
                        n.normalize()
                    } else {
                        Vec3::Y
                    };
                }
            }
            let texcoords = (source.texcoords.len() == positions.len() * 2).then(|| {
                source
                    .texcoords
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| [v[0], v[1]])
                    .collect()
            });
            let min = positions
                .iter()
                .fold(Vec3::splat(f32::INFINITY), |a, p| a.min(*p));
            let max = positions
                .iter()
                .fold(Vec3::splat(f32::NEG_INFINITY), |a, p| a.max(*p));
            all_min = all_min.min(min);
            all_max = all_max.max(max);
            meshes.push(Mesh {
                name: model.name,
                positions,
                normals,
                texcoords,
                indices: source.indices,
                bounds: Bounds { min, max },
            });
        }
        if meshes.is_empty() {
            return Err("OBJ contains no triangle meshes".into());
        }
        Ok(Self(Arc::new(HostInner {
            meshes,
            bounds: Bounds {
                min: all_min,
                max: all_max,
            },
            uploads: Mutex::new(Vec::new()),
        })))
    }
    pub fn meshes(&self) -> &[Mesh] {
        &self.0.meshes
    }
    pub fn bounds(&self) -> Bounds {
        self.0.bounds
    }
    pub fn upload(&self, context: &GpuContext) -> Result<DeviceScene> {
        let mut uploads = self.0.uploads.lock().map_err(|e| e.to_string())?;
        uploads.retain(|entry| entry.strong_count() > 0);
        if let Some(scene) = uploads.iter().filter_map(Weak::upgrade).find(|scene| {
            scene.context.instance == context.instance && scene.context.device == context.device
        }) {
            return Ok(DeviceScene(scene));
        }
        let mut meshes = Vec::new();
        for mesh in &self.0.meshes {
            let vertices: Vec<_> = mesh
                .positions
                .iter()
                .zip(&mesh.normals)
                .map(|(p, n)| Vertex {
                    position: p.to_array(),
                    normal: n.to_array(),
                })
                .collect();
            let vertex = context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("OBJ vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index = context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("OBJ indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            meshes.push(DeviceMesh {
                vertex,
                index,
                count: mesh.indices.len() as u32,
            });
        }
        let scene = Arc::new(DeviceInner {
            context: context.clone(),
            meshes,
            bounds: self.bounds(),
        });
        uploads.push(Arc::downgrade(&scene));
        Ok(DeviceScene(scene))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

#[derive(Clone)]
pub struct DeviceScene(Arc<DeviceInner>);
struct DeviceInner {
    context: GpuContext,
    meshes: Vec<DeviceMesh>,
    bounds: Bounds,
}
struct DeviceMesh {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    count: u32,
}
impl DeviceScene {
    pub fn bounds(&self) -> Bounds {
        self.0.bounds
    }
    pub fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn compatible(&self, context: &GpuContext) -> bool {
        self.0.context.instance == context.instance && self.0.context.device == context.device
    }
    pub(crate) fn meshes(&self) -> impl Iterator<Item = (&wgpu::Buffer, &wgpu::Buffer, u32)> {
        self.0.meshes.iter().map(|m| (&m.vertex, &m.index, m.count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obj_meshes_normals_bounds() {
        let data = b"o first\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no second\nv 0 0 1\nf 1 3 4\n";
        let scene = HostScene::load_obj(data).unwrap();
        assert_eq!(scene.meshes().len(), 2);
        assert_eq!(scene.bounds().max, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(scene.meshes()[0].indices.len(), 3);
        assert!(scene.meshes()[0].normals.iter().all(|n| n.length() > 0.9));
    }
    #[test]
    fn bundled_models_triangulate_and_face_outward() {
        let pyramid =
            HostScene::load_obj(include_bytes!("../../../assets/models/pyramid.obj")).unwrap();
        let cube = HostScene::load_obj(include_bytes!("../../../assets/models/cube.obj")).unwrap();
        assert_eq!(pyramid.meshes()[0].indices.len(), 18);
        assert_eq!(cube.meshes()[0].indices.len(), 36);
        let mesh = &pyramid.meshes()[0];
        let front = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .find(|face| {
                let center = (mesh.positions[face[0] as usize]
                    + mesh.positions[face[1] as usize]
                    + mesh.positions[face[2] as usize])
                    / 3.0;
                center.z > 0.5 && center.y > -0.5
            })
            .unwrap();
        let a = mesh.positions[front[0] as usize];
        let b = mesh.positions[front[1] as usize];
        let c = mesh.positions[front[2] as usize];
        assert!((b - a).cross(c - a).z > 0.0);
    }
    #[test]
    fn gpu_upload_reuse_and_context_check() {
        let Ok(first_context) = pollster::block_on(GpuContext::new()) else {
            return;
        };
        let Ok(second_context) = pollster::block_on(GpuContext::new()) else {
            return;
        };
        let scene = HostScene::load_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let first = scene.upload(&first_context).unwrap();
        let reused = scene.upload(&first_context).unwrap();
        assert!(first.same_allocation(&reused));
        let second = scene.upload(&second_context).unwrap();
        assert!(!first.same_allocation(&second));
        let mut viewer = crate::ObjViewer::new(first_context);
        assert!(viewer.set_device_scene(first).is_ok());
        let mut other_viewer = crate::ObjViewer::new(viewer.context().clone());
        other_viewer.set_device_scene(reused.clone()).unwrap();
        assert!(
            viewer
                .scene()
                .unwrap()
                .same_allocation(other_viewer.scene().unwrap())
        );
        assert!(viewer.set_device_scene(second).is_err());
        viewer.close();
        assert!(!viewer.is_attached());
        assert!(other_viewer.scene().is_some());
    }
}
