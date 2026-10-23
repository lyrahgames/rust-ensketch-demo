use crate::Bounds;
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub orientation: Quat,
    pub vertical_fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 4.0),
            orientation: Quat::IDENTITY,
            vertical_fov: 45f32.to_radians(),
            near: 0.01,
            far: 1000.0,
        }
    }
}

impl Camera {
    pub fn is_valid(&self) -> bool {
        self.position.is_finite()
            && self.orientation.is_finite()
            && self.orientation.length_squared() > 0.0
            && self.vertical_fov.is_finite()
            && self.vertical_fov > 0.0
            && self.vertical_fov < std::f32::consts::PI
            && self.near.is_finite()
            && self.near > 0.0
            && self.far.is_finite()
            && self.far > self.near
    }
    pub fn view(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.orientation, self.position).inverse()
    }
    pub fn projection(&self, width: u32, height: u32) -> Mat4 {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        glam::camera::rh::proj::directx::perspective(self.vertical_fov, aspect, self.near, self.far)
    }
    pub fn view_projection(&self, width: u32, height: u32) -> Mat4 {
        self.projection(width, height) * self.view()
    }
    pub fn orbit(&mut self, pivot: Vec3, dx: f32, dy: f32) {
        let yaw = Quat::from_rotation_y(-dx * 0.005);
        let right = self.orientation * Vec3::X;
        let pitch = Quat::from_axis_angle(right, -dy * 0.005);
        let rotation = (yaw * pitch).normalize();
        self.position = pivot + rotation * (self.position - pivot);
        self.orientation = (rotation * self.orientation).normalize();
    }
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.position += self.orientation * Vec3::new(-dx * 0.005, dy * 0.005, 0.0);
    }
    pub fn zoom(&mut self, amount: f32) {
        self.position += self.orientation * Vec3::NEG_Z * amount;
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn frame(&mut self, bounds: Bounds) {
        let radius = bounds.radius().max(0.01);
        self.position =
            bounds.center() + Vec3::Z * (radius / (self.vertical_fov * 0.5).tan() * 1.4);
        self.orientation = Quat::IDENTITY;
        self.near = (radius * 0.001).max(0.001);
        self.far = (radius * 100.0).max(100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_motion_and_depth() {
        let mut c = Camera::default();
        assert!(c.view_projection(800, 600).is_finite());
        assert_ne!(c.projection(800, 600), c.projection(600, 800));
        let near = c.projection(800, 600) * glam::Vec4::new(0.0, 0.0, -c.near, 1.0);
        let far = c.projection(800, 600) * glam::Vec4::new(0.0, 0.0, -c.far, 1.0);
        assert!((near.z / near.w).abs() < 0.0001);
        assert!((far.z / far.w - 1.0).abs() < 0.0001);
        let before = c.position;
        c.orbit(Vec3::ZERO, 20.0, 10.0);
        assert!((c.position.length() - before.length()).abs() < 0.001);
        c.pan(10.0, 10.0);
        c.zoom(1.0);
        assert_ne!(c.position, before);
        c.near = -1.0;
        assert!(!c.is_valid());
    }
}
