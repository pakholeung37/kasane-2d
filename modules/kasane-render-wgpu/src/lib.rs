//! wgpu rendering adapter for the backend-neutral `kasane-render` plan.
//!
//! The current execution path supports normal drawable meshes, alpha masks,
//! nested offscreen surfaces, destination snapshots for extended blend modes,
//! and fixed-function additive/multiplicative drawable blends.

use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use kasane_core::evaluation::{Drawable, DrawableFrame};
use kasane_core::types::{BlendMode, Status, Vec2};
use kasane_render::{
    prepare_frame, Affine2, DrawItem, MaskKey, PreparedFrame, RenderPass, Size2, TextureCatalog,
    ViewportConfig,
};
use wgpu::util::DeviceExt;

mod api;
mod compat;
mod encoding;
mod geometry;
mod modern;
mod pipeline;
mod resources;
mod shaders;
#[cfg(test)]
mod tests;

pub use api::*;
pub use modern::{
    WgpuEncodeTarget, WgpuMainBackground, WgpuMaskAttachment, WgpuOutputMode, WgpuRenderStats,
    WgpuRenderer,
};
pub use pipeline::WgpuBasicRenderer;
pub use resources::{
    WgpuDestination, WgpuDestinationPool, WgpuMask, WgpuMaskPool, WgpuSurface, WgpuSurfacePool,
};

use encoding::*;
use geometry::*;
use pipeline::*;
use resources::*;
use shaders::*;
