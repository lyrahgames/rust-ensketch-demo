use crate::scene::Scene;
use glam::{DVec2, DVec3, DVec4, Vec2, Vec3, Vec4};
use std::ops::{Add, Div, Index, IndexMut, Mul, Sub};

/// Scalar operations needed to compute an AABB's center and radius.
pub trait AabbReal:
  Copy
  + Default
  + PartialOrd
  + Add<Output = Self>
  + Sub<Output = Self>
  + Mul<Output = Self>
  + Div<Output = Self> {
  const HALF: Self;

  fn sqrt(self) -> Self;
}

impl AabbReal for f32 {
  const HALF: Self = 0.5;

  fn sqrt(self) -> Self { f32::sqrt(self) }
}

impl AabbReal for f64 {
  const HALF: Self = 0.5;

  fn sqrt(self) -> Self { f64::sqrt(self) }
}

/// A fixed-size vector that can be used as the bounds type of an [`Aabb`].
pub trait VectorLike: Copy + Index<usize, Output = Self::Real> + IndexMut<usize> {
  type Real: AabbReal;

  const DIM: usize;

  fn zero() -> Self;
}

macro_rules! impl_vector_like {
  ($vector:ty, $real:ty, $dimension:expr) => {
    impl VectorLike for $vector {
      type Real = $real;

      const DIM: usize = $dimension;

      fn zero() -> Self { Self::ZERO }
    }
  };
}

impl_vector_like!(Vec2, f32, 2);
impl_vector_like!(Vec3, f32, 3);
impl_vector_like!(Vec4, f32, 4);
impl_vector_like!(DVec2, f64, 2);
impl_vector_like!(DVec3, f64, 3);
impl_vector_like!(DVec4, f64, 4);

impl<const N: usize> VectorLike for [f32; N] {
  type Real = f32;

  const DIM: usize = N;

  fn zero() -> Self { [0.0; N] }
}

impl<const N: usize> VectorLike for [f64; N] {
  type Real = f64;

  const DIM: usize = N;

  fn zero() -> Self { [0.0; N] }
}

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
      center[i] = (self.min[i] + self.max[i]) * V::Real::HALF;
    }
    center
  }

  pub fn radius(self) -> V::Real {
    let center = self.center();
    let mut squared_radius = V::Real::default();
    for i in 0..V::DIM {
      let offset = self.max[i] - center[i];
      squared_radius = squared_radius + offset * offset;
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
  use super::*;

  #[test]
  fn vector_like_dimensions_and_real_types() {
    assert_eq!(<Vec2 as VectorLike>::DIM, 2);
    assert_eq!(<Vec3 as VectorLike>::DIM, 3);
    assert_eq!(<Vec4 as VectorLike>::DIM, 4);
    assert_eq!(<DVec2 as VectorLike>::DIM, 2);
    assert_eq!(<DVec3 as VectorLike>::DIM, 3);
    assert_eq!(<DVec4 as VectorLike>::DIM, 4);
    assert_eq!(<[f32; 7] as VectorLike>::DIM, 7);
    assert_eq!(<[f64; 5] as VectorLike>::DIM, 5);
  }

  #[test]
  fn generic_aabb_collects_points_and_computes_center_and_radius() {
    let bounds: Aabb<[f64; 2]> = [[-1.0, 0.0], [1.0, 2.0]].into_iter().collect();
    assert_eq!(bounds.min, [-1.0, 0.0]);
    assert_eq!(bounds.max, [1.0, 2.0]);
    assert_eq!(bounds.center(), [0.0, 1.0]);
    assert!((bounds.radius() - 2.0_f64.sqrt()).abs() < 1e-12);
  }
}
