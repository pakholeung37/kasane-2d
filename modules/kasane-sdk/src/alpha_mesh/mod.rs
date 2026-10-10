//! Alpha-driven authoring geometry. All calculations use source-image pixels;
//! this module neither reads a document nor migrates existing deformation data.
mod classify;
mod domain;
mod islands;
mod occupied;
mod resample;
mod simplify;
mod spatial;
mod support;
mod triangulate;

use crate::{MeshGeometry, SdkError};

/// Tightly packed, top-to-bottom, one-byte alpha samples (not RGBA).
#[derive(Clone, Copy, Debug)]
pub struct AlphaMask<'a> {
    pub width: u32,
    pub height: u32,
    pub alpha: &'a [u8],
}

/// Distances are in source-image pixels, independent of model or atlas size.
/// Spacing is a target: corners, rings and thin-feature support can add points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaMeshOptions {
    pub outside_spacing: f64,
    pub inside_spacing: f64,
    pub outside_margin: f64,
    pub inside_margin: f64,
    /// Conservative simplification must retain this much foreground padding.
    /// Like outside_margin, clipping is controlled by clip_to_image.
    pub minimum_margin: f64,
    /// Applies to each exterior and hole boundary, including tiny components.
    pub minimum_boundary_points: usize,
    /// A retained sample is foreground when alpha > alpha_threshold.
    pub alpha_threshold: u8,
    /// Preserve transparent holes; positive padding may still close small holes.
    /// False (the preset default) allows triangles across holes while interior
    /// support continues to follow the original foreground.
    pub preserve_holes: bool,
    /// Keep margins inside the source rectangle. When false, positions and UVs
    /// may extend outside it; callers must supply transparent texture padding.
    pub clip_to_image: bool,
    /// Ignore isolated faint specks (peak alpha <= 64, total alpha <= four
    /// opaque pixels), only when the image also contains alpha >= 128.
    /// Pixels connected to substantial artwork and opaque small features stay.
    pub remove_faint_islands: bool,
    /// Hard output budget. Exceeding it returns an error, never a partial mesh.
    pub max_vertices: usize,
}

impl Default for AlphaMeshOptions {
    fn default() -> Self {
        Self::standard()
    }
}

impl AlphaMeshOptions {
    /// Balanced modeling density, interpreted directly in source pixels.
    /// Preset spacing targets roughly half the former vertex count on typical
    /// artwork; corners and thin-feature support remain geometry-dependent.
    pub const fn standard() -> Self {
        Self {
            outside_spacing: 85.0,
            inside_spacing: 85.0,
            outside_margin: 14.0,
            inside_margin: 14.0,
            minimum_margin: 5.0,
            minimum_boundary_points: 10,
            alpha_threshold: 0,
            preserve_holes: false,
            clip_to_image: true,
            remove_faint_islands: false,
            max_vertices: 65_536,
        }
    }

    pub const fn deformation_small() -> Self {
        Self {
            outside_spacing: 160.0,
            inside_spacing: 160.0,
            minimum_boundary_points: 5,
            ..Self::standard()
        }
    }

    pub const fn deformation_large() -> Self {
        Self {
            outside_spacing: 40.0,
            inside_spacing: 40.0,
            outside_margin: 3.0,
            inside_margin: 2.0,
            minimum_margin: 2.0,
            minimum_boundary_points: 5,
            ..Self::standard()
        }
    }

    pub fn validate(&self) -> Result<(), SdkError> {
        if [self.outside_spacing, self.inside_spacing]
            .iter()
            .any(|n| !n.is_finite() || *n <= 0.0 || *n > 1_000_000.0)
            || [self.outside_margin, self.inside_margin, self.minimum_margin]
                .iter()
                .any(|n| !n.is_finite() || *n < 0.0 || *n > 1_000_000.0)
            || self.minimum_margin > self.outside_margin
            || !(3..=65_536).contains(&self.max_vertices)
            || !(3..=self.max_vertices).contains(&self.minimum_boundary_points)
        {
            return Err(error("INVALID_ALPHA_MESH_OPTIONS", "Use finite positive spacing, nonnegative margins with minimum <= outside, and 3..=65536 boundary/output limits"));
        }
        Ok(())
    }
}

pub(super) fn error(code: &'static str, message: &str) -> SdkError {
    SdkError::new(code, message, "alpha_mesh_geometry")
}

pub(super) fn limit() -> SdkError {
    error("ALPHA_MESH_LIMIT", "Alpha mesh exceeds its vertex or preprocessing budget; increase spacing or simplify the source image")
}

/// Generate deterministic, counterclockwise triangles covering the foreground.
///
/// Positions use pixel-edge coordinates, origin at the image's bottom left, +Y
/// upward. UVs are (x / width, y / height). By default margins are clipped to the
/// image and UVs stay in [0, 1]. With clip_to_image=false, callers must provide
/// transparent texture padding and remap UVs before rendering. No component is
/// removed unless remove_faint_islands is enabled. Filled pixels represent unit
/// squares, so single-pixel features survive. Geometry IDs are local sequential
/// IDs; replacing an authored mesh still requires explicit dependency handling.
///
/// Limits: at most 16384 pixels per axis, 64 Mi pixels, 262144 merged foreground
/// rectangles, 1048576 attempted samples, and 1048576 candidate polygon pairs
/// per topology validation pass. Empty foreground is an explicit
/// EMPTY_ALPHA_MASK error. No file IO, GPU, model mutation or random seed is used.
pub fn alpha_mesh_geometry(
    mask: AlphaMask<'_>,
    options: &AlphaMeshOptions,
) -> Result<MeshGeometry, SdkError> {
    options.validate()?;
    let count = (mask.width as usize).checked_mul(mask.height as usize);
    if mask.width == 0 || mask.height == 0 || count != Some(mask.alpha.len()) {
        return Err(error(
            "INVALID_ALPHA_MASK",
            "Alpha dimensions must be nonzero and match the tightly packed buffer",
        ));
    }
    if mask.width > 16_384 || mask.height > 16_384 || mask.alpha.len() > 64 * 1024 * 1024 {
        return Err(limit());
    }
    let cleaned;
    let mask = if options.remove_faint_islands {
        cleaned = islands::clean(mask);
        AlphaMask {
            alpha: &cleaned,
            ..mask
        }
    } else {
        mask
    };
    let (foreground, outline) = domain::build(mask, options)?;
    triangulate::generate(mask, options, &foreground, &outline)
}
