//! Native Metal renderer for Kasane scene plans.
//!
//! On macOS this crate uses `MTLDevice`, Metal textures, MSL pipelines, and
//! Metal command buffers directly. Scene validation and CPU geometry come from
//! the backend-neutral `kasane-render` crate.

#[cfg(target_os = "macos")]
mod native;

#[cfg(target_os = "macos")]
pub use native::*;

#[cfg(target_os = "macos")]
pub use metal;
