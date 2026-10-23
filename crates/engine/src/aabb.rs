use crate::{Scene, VectorLike};
use glam::Vec3;

/// An axis-aligned bounding box parameterized by its vector representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb<V: VectorLike = Vec3> {
  pub min: V,
  pub max: V,
}

impl<V: VectorLike> Aabb<V> {
  pub fn center(self) -> V {
    let mut center = V::zero();
    for i in 0..V::DIM {
      center[i] = (self.min[i] + self.max[i]) / 2.0;
    }
    center
  }

  pub fn radius(self) -> f32 {
    let center = self.center();
    let mut squared_radius = 0.0f32;
    for i in 0..V::DIM {
      let offset = self.max[i] - center[i];
      squared_radius += offset * offset;
    }
    squared_radius.sqrt()
  }
}

impl<V: VectorLike> FromIterator<V> for Aabb<V> {
  fn from_iter<I: IntoIterator<Item = V>>(points: I) -> Self {
    let mut points = points.into_iter();
    let first = points.next().expect("cannot create an AABB from no points");
    let mut bounds = Self {
      min: first,
      max: first,
    };
    for point in points {
      for i in 0..V::DIM {
        if point[i] < bounds.min[i] {
          bounds.min[i] = point[i];
        }
        if point[i] > bounds.max[i] {
          bounds.max[i] = point[i];
        }
      }
    }
    bounds
  }
}

impl From<&Scene> for Aabb<Vec3> {
  fn from(scene: &Scene) -> Self {
    scene
      .meshes()
      .iter()
      .flat_map(|mesh| mesh.geometry.iter().map(|vertex| vertex.position))
      .collect()
  }
}

impl From<Scene> for Aabb<Vec3> {
  fn from(scene: Scene) -> Self { Self::from(&scene) }
}

#[cfg(test)]
mod tests {
  use crate::Aabb;

  #[test]
  fn generic_aabb_collects_points_and_computes_center_and_radius() {
    let bounds: Aabb<glam::Vec2> = [glam::Vec2::new(-1.0, 0.0), glam::Vec2::new(1.0, 2.0)]
      .into_iter()
      .collect();
    assert_eq!(bounds.min, glam::Vec2::new(-1.0, 0.0));
    assert_eq!(bounds.max, glam::Vec2::new(1.0, 2.0));
    assert_eq!(bounds.center(), glam::Vec2::new(0.0, 1.0));
    assert!((bounds.radius() - 2.0_f32.sqrt()).abs() < 1e-6);
  }
}
