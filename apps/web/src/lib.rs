//! Static browser host. The WebGPU renderer and JavaScript API live in
//! `script-js`; the pages under `examples/web` own canvas events and frames.
#[cfg(target_arch = "wasm32")]
pub use script_js::*;
