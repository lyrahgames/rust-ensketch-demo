use crate::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use std::io::BufReader;

/// A vertex record containing position, normal, and texture coordinates.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct Vertex {
  pub position: Vec3,
  pub normal:   Vec3,
  pub uv:       Vec2,
}

/// A range of vertices.
#[derive(Clone, Debug)]
pub struct VertexGeometry(pub Vec<Vertex>);
impl VertexGeometry {
  pub fn new(vertices: Vec<Vertex>) -> Self { Self(vertices) }
  pub fn iter(&self) -> std::slice::Iter<'_, Vertex> { self.0.iter() }
  pub fn len(&self) -> usize { self.0.len() }
  pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq, Hash)]
pub struct VertexIndex(pub u32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq)]
pub struct Triangle(pub [VertexIndex; 3]);

/// A range of triangles. The container is generic so arrays, slices, and
/// application-specific iterable collections can be used without changing the
/// triangle representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriangleTopology<T = Vec<Triangle>>(pub T);
impl<T> TriangleTopology<T> {
  pub fn iter(&self) -> impl Iterator<Item = &Triangle>
  where
    for<'a> &'a T: IntoIterator<Item = &'a Triangle>, {
    (&self.0).into_iter()
  }
}

/// CPU triangle data: topology refers to vertices in its geometry range.
#[derive(Clone, Debug)]
pub struct TriangleMesh<G = VertexGeometry, T = TriangleTopology> {
  pub topology: T,
  pub geometry: G,
}

/// CPU-only scene data. It has no GPU resources, upload cache, or stored AABB.
#[derive(Clone, Debug)]
pub struct Scene {
  meshes: Vec<TriangleMesh>,
}
impl Scene {
  pub fn new(meshes: Vec<TriangleMesh>) -> Result<Self> {
    if meshes.is_empty() {
      return Err("scene contains no triangle meshes".into());
    }
    for (mesh_index, mesh) in meshes.iter().enumerate() {
      let vertex_count = mesh.geometry.len();
      if vertex_count == 0 || mesh.topology.0.is_empty() {
        return Err(format!("triangle mesh {mesh_index} is empty"));
      }
      if mesh.geometry.iter().any(|vertex| {
        !vertex.position.is_finite() || !vertex.normal.is_finite() || !vertex.uv.is_finite()
      }) {
        return Err(format!(
          "triangle mesh {mesh_index} has non-finite vertex data"
        ));
      }
      for triangle in &mesh.topology.0 {
        if triangle
          .0
          .iter()
          .any(|index| index.0 as usize >= vertex_count)
        {
          return Err(format!(
            "triangle mesh {mesh_index} contains an out-of-range vertex index"
          ));
        }
      }
    }
    Ok(Self { meshes })
  }

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
    for model in models {
      let source = model.mesh;
      if source.positions.len() % 3 != 0 || source.indices.len() % 3 != 0 {
        return Err(format!("mesh {} has malformed triangles", model.name));
      }
      let positions: Vec<Vec3> = source
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
          let normal = (b - a).cross(c - a);
          for &index in face {
            normals[index as usize] += normal;
          }
        }
        for normal in &mut normals {
          *normal = if normal.length_squared() > 0.0 {
            normal.normalize()
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
          .collect::<Vec<_>>()
      });
      let geometry = VertexGeometry(
        positions
          .iter()
          .zip(normals)
          .enumerate()
          .map(|(i, (position, normal))| Vertex {
            position: *position,
            normal,
            uv: texcoords
              .as_ref()
              .map_or(Vec2::ZERO, |uv| Vec2::from_array(uv[i])),
          })
          .collect(),
      );
      let topology = source
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| Triangle(triangle.map(VertexIndex)))
        .collect();
      meshes.push(TriangleMesh {
        topology: TriangleTopology(topology),
        geometry,
      });
    }
    Self::new(meshes)
  }

  pub fn meshes(&self) -> &[TriangleMesh] { &self.meshes }
}

#[cfg(test)]
mod tests {
  use crate::{Aabb, Scene, Vertex};
  use glam::Vec3;

  #[test]
  fn obj_meshes_normals_and_scene_aabb() {
    let data = b"o first\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no second\nv 0 0 1\nf 1 3 4\n";
    let scene = Scene::load_obj(data).unwrap();
    assert_eq!(scene.meshes().len(), 2);
    assert_eq!(Aabb::from(&scene).max, Vec3::new(1.0, 1.0, 1.0));
    assert_eq!(scene.meshes()[0].topology.0.len(), 1);
    assert!(
      scene.meshes()[0]
        .geometry
        .iter()
        .all(|vertex| vertex.normal.length() > 0.9)
    );
    assert_eq!(Aabb::from(scene.clone()), Aabb::from(&scene));
  }

  #[test]
  fn obj_loader_triangulates_polygon_faces() {
    let scene = Scene::load_obj(b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
    assert_eq!(scene.meshes()[0].topology.0.len(), 2);
  }

  #[test]
  fn bundled_models_triangulate_and_face_outward() {
    let pyramid = Scene::load_obj(include_bytes!("../../../assets/models/pyramid.obj")).unwrap();
    let cube = Scene::load_obj(include_bytes!("../../../assets/models/cube.obj")).unwrap();
    assert_eq!(pyramid.meshes()[0].topology.0.len(), 6);
    assert_eq!(cube.meshes()[0].topology.0.len(), 12);
    let mesh = &pyramid.meshes()[0];
    let front = mesh
      .topology
      .0
      .iter()
      .find(|triangle| {
        let center = triangle
          .0
          .iter()
          .map(|index| mesh.geometry.0[index.0 as usize].position)
          .sum::<Vec3>()
          / 3.0;
        center.z > 0.5 && center.y > -0.5
      })
      .unwrap();
    let [a, b, c] = front
      .0
      .map(|index| mesh.geometry.0[index.0 as usize].position);
    assert!((b - a).cross(c - a).z > 0.0);
  }

  #[test]
  fn vertex_has_tightly_packed_wgpu_layout() {
    assert_eq!(std::mem::size_of::<Vertex>(), 32);
    assert_eq!(std::mem::offset_of!(Vertex, position), 0);
    assert_eq!(std::mem::offset_of!(Vertex, normal), 12);
    assert_eq!(std::mem::offset_of!(Vertex, uv), 24);
  }
}
