//! Backend-neutral render contracts and resource planning.
//!
//! This crate describes what a backend must render without owning any GPU or
//! host-engine resource. Godot and future backends consume the same plan but
//! are free to choose different physical resource implementations.

mod plan;
mod scene;
mod validation;

use std::collections::{HashMap, HashSet};

use kasane_core::types::Vec2;

pub use plan::{build_plan, prepare_frame};
pub use scene::{
    surface_layout, Bounds2, MaskId, MaskPlan, MeshId, MeshPlan, ScenePlan, TargetId, TargetItem,
    TargetPlan,
};
pub use validation::validate_frame;

/// Default attachment budget for the compatibility planner and Godot adapter.
pub const OFFSCREEN_BUDGET_BYTES: i64 = 512 * 1024 * 1024;

/// A 2D affine transform in target-space coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    pub a: Vec2,
    pub b: Vec2,
    pub origin: Vec2,
}

impl Affine2 {
    pub const IDENTITY: Self = Self {
        a: Vec2::new(1.0, 0.0),
        b: Vec2::new(0.0, 1.0),
        origin: Vec2::new(0.0, 0.0),
    };

    pub fn determinant(self) -> f32 {
        self.a.x * self.b.y - self.a.y * self.b.x
    }

    pub fn is_finite(self) -> bool {
        [
            self.a.x,
            self.a.y,
            self.b.x,
            self.b.y,
            self.origin.x,
            self.origin.y,
        ]
        .into_iter()
        .all(f32::is_finite)
    }

    pub fn transform_point(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.a
                .x
                .mul_add(point.x, self.b.x.mul_add(point.y, self.origin.x)),
            self.a
                .y
                .mul_add(point.x, self.b.y.mul_add(point.y, self.origin.y)),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportConfig {
    pub transform: Affine2,
    pub target_extent: Vec2,
    pub mask_scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size2 {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
}

/// Backend-provided texture metadata lookup used during frame preflight.
///
/// The lookup is borrowed so adapters do not have to clone their native
/// texture map just to validate a frame.
pub trait TextureCatalog {
    fn texture_info(&self, id: &str) -> Option<TextureInfo>;
}

impl TextureCatalog for HashMap<String, TextureInfo> {
    fn texture_info(&self, id: &str) -> Option<TextureInfo> {
        self.get(id).copied()
    }
}

/// Legacy pass-stream attachment key. New backends use logical `MaskId` and
/// choose their own physical consumer instances and resolution policy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MaskKey {
    sources: Vec<String>,
    scale_bits: u64,
    consumer: String,
}

impl MaskKey {
    pub fn new(sources: &[String], scale: f64, consumer: &str) -> Self {
        Self {
            sources: sources.to_vec(),
            scale_bits: scale.to_bits(),
            consumer: consumer.to_owned(),
        }
    }
}

/// One drawable submission in a prepared pass stream.
///
/// IDs borrow the evaluated frame so preparing a frame does not duplicate the
/// model's strings on the hot path. A backend may resolve the IDs against its
/// own resource cache while executing the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawItem<'a> {
    pub drawable_id: &'a str,
    pub texture_id: &'a str,
}

/// Compatibility operations in legacy scene-assembly order, not GPU execution order.
///
/// `Offscreen` begins a target and `EndOffscreen` closes it. `Composite` is
/// emitted immediately after `Offscreen`, before the target's child draws, so
/// a backend can attach the target's texture to its parent before rendering
/// nested content. `Mask` identifies the consumer viewport that must own the
/// mask attachment before the following draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderPass<'a> {
    Main,
    Offscreen {
        id: &'a str,
        parent: Option<&'a str>,
    },
    Mask {
        target: &'a str,
        consumer: Option<&'a str>,
    },
    Composite {
        id: &'a str,
        parent: Option<&'a str>,
    },
    Draw(DrawItem<'a>),
    EndOffscreen {
        id: &'a str,
    },
}

/// The backend-neutral result of validating and preparing one frame.
///
/// IDs borrow from the submitted frame to avoid string duplication. Preparing
/// this compatibility view still allocates containers; use `ScenePlan` for a
/// persistent description without retaining a published frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFrame<'a> {
    pub active_offscreens: HashSet<&'a str>,
    pub mask_consumers: HashMap<String, String>,
    /// Logical nodes that read the current destination before compositing.
    ///
    /// The name is deliberately backend-neutral: a Godot adapter may satisfy
    /// this with `BackBufferCopy`, while a wgpu adapter may use a sampled
    /// destination attachment or an explicit copy.
    pub destination_reads: HashSet<&'a str>,
    pub surface_size: Size2,
    pub surface_transform: Affine2,
    pub passes: Vec<RenderPass<'a>>,
}

/// Compatibility name for callers that only need the resource plan.
pub type RenderPlan<'a> = PreparedFrame<'a>;
