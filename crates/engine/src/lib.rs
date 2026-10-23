//! Script independent graphics and scheduling primitives.
#![recursion_limit = "512"]
// Browser WebGPU handles are thread-local, while the same ownership path uses
// Arc to share native device resources across viewers and executor threads.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
mod camera;
mod executor;
mod gpu;
mod scene;
mod viewer;

pub use camera::Camera;
pub use executor::{Executor, ExecutorError, Submitter, Task};
pub use gpu::GpuContext;
pub use scene::{Bounds, DeviceScene, HostScene, Mesh};
pub use viewer::ObjViewer;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventKind {
    #[default]
    MouseMove,
    MouseButton,
    MouseWheel,
    Key,
    Resize,
    CloseRequest,
}

#[derive(Clone, Debug, Default)]
pub struct InputEvent {
    pub kind: EventKind,
    pub dx: f32,
    pub dy: f32,
    pub wheel: f32,
    pub x: f32,
    pub y: f32,
    pub left_pressed: bool,
    pub middle_pressed: bool,
    pub right_pressed: bool,
    pub button: Option<u8>,
    pub pressed: bool,
    pub key: Option<String>,
    pub width: u32,
    pub height: u32,
}
