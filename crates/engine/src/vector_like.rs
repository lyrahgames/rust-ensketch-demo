use std::ops::{
  Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign,
};

/// A fixed-size vector supporting the operations required by AABB geometry.
///
/// The supported vectors use `f32` components. Implement this trait for other
/// vector types that provide the required indexing and vector-space operations.
pub trait VectorLike<Real = f32>:
  Copy
  + Index<usize, Output = Real>
  + IndexMut<usize>
  + Add<Output = Self>
  + AddAssign
  + Sub<Output = Self>
  + SubAssign
  + Mul<Real, Output = Self>
  + MulAssign<Real>
  + Div<Real, Output = Self>
  + DivAssign<Real>
  + Neg<Output = Self> {
  const DIM: usize;

  fn zero() -> Self;
}

macro_rules! impl_vector_like {
  ($vector:ty, $dimension:expr) => {
    impl VectorLike<f32> for $vector {
      const DIM: usize = $dimension;

      fn zero() -> Self { Self::ZERO }
    }
  };
}

impl_vector_like!(glam::Vec2, 2);
impl_vector_like!(glam::Vec3, 3);
impl_vector_like!(glam::Vec4, 4);

#[cfg(test)]
mod tests {
  use crate::VectorLike;

  fn assert_f32_vector<V: VectorLike<f32>>() {}

  #[test]
  fn glam_vectors_use_expected_dimensions_and_scalar() {
    assert_f32_vector::<glam::Vec2>();
    assert_f32_vector::<glam::Vec3>();
    assert_f32_vector::<glam::Vec4>();

    assert_eq!(<glam::Vec2 as VectorLike<f32>>::DIM, 2);
    assert_eq!(<glam::Vec3 as VectorLike<f32>>::DIM, 3);
    assert_eq!(<glam::Vec4 as VectorLike<f32>>::DIM, 4);
  }
}
