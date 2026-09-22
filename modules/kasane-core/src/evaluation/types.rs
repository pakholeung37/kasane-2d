use std::collections::HashMap;
use std::sync::Arc;

use crate::document::Document;
use crate::geometry::{to_runtime_positions, validate_positions};
use crate::types::{BlendMode, Canvas, Status, Vec2};

pub type PreviewValues = HashMap<String, f32>;

#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    pub id: String,
    pub runtime_id: String,
    pub part_id: String,
    pub raw_blend_mode: Option<u32>,
    pub texture_asset_id: String,
    pub texture_slot: i32,
    pub positions: Vec<Vec2>,
    pub uvs: Arc<[Vec2]>,
    pub indices: Arc<[u32]>,
    pub draw_order: i32,
    pub render_order: i32,
    pub opacity: f32,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub visible: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl Default for Drawable {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            part_id: String::new(),
            raw_blend_mode: None,
            texture_asset_id: String::new(),
            texture_slot: 0,
            positions: Vec::new(),
            uvs: Arc::from([]),
            indices: Arc::from([]),
            draw_order: 0,
            render_order: 0,
            opacity: 1.0,
            multiply_color: [1.0, 1.0, 1.0, 1.0],
            screen_color: [0.0, 0.0, 0.0, 1.0],
            blend_mode: BlendMode::Normal,
            enabled: true,
            visible: true,
            double_sided: true,
            inverted_mask: false,
            masks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvaluatedParameter {
    pub id: String,
    pub requested: f32,
    pub value: f32,
    pub clamped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RenderCommand {
    BeginOffscreen { offscreen_id: String },
    DrawMesh { mesh_id: String },
    EndOffscreen { offscreen_id: String },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OffscreenFrame {
    pub id: String,
    pub runtime_id: String,
    pub owner_part_id: String,
    pub parent_offscreen_id: Option<String>,
    pub render_order: i32,
    pub opacity: f32,
    pub enabled: bool,
    pub blend_mode: u32,
    pub flags: u8,
    pub masks: Vec<String>,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawableFrame {
    /// Revision used to evaluate this immutable snapshot; later name edits may reuse it.
    pub source_revision: u64,
    pub canvas: Canvas,
    pub parameters: Vec<EvaluatedParameter>,
    pub drawables: Vec<Drawable>,
    pub offscreens: Vec<OffscreenFrame>,
    pub render_plan: Vec<RenderCommand>,
}

pub fn to_parent_positions(
    doc: &Document,
    parent: &str,
    positions: &[Vec2],
) -> Result<Vec<Vec2>, Status> {
    if parent.is_empty() {
        to_runtime_positions(doc.canvas(), positions)
    } else {
        if doc.get_transform(parent).is_none() {
            return Err(Status::error("MISSING_TRANSFORM", parent));
        }
        let s = validate_positions(positions);
        if !s.is_ok() {
            return Err(s);
        }
        Ok(positions.to_vec())
    }
}

pub fn to_parent_origin(
    doc: &Document,
    parent: &str,
    origin: crate::types::PreciseVec2,
) -> Result<Vec2, Status> {
    let canvas = doc.canvas();
    let p = if parent.is_empty() {
        Vec2::new(
            ((origin.x - canvas.origin.x as f64) / canvas.pixels_per_unit as f64) as f32,
            ((canvas.origin.y as f64 - origin.y) / canvas.pixels_per_unit as f64) as f32,
        )
    } else {
        Vec2::new(origin.x as f32, origin.y as f32)
    };
    let status = validate_positions(&[p]);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(p)
}
