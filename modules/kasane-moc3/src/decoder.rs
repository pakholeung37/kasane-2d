use std::collections::HashMap;

use kasane_core::types::Status;
use kasane_core::Document;

use crate::inspector::{CanvasInfo, Moc3InspectionReport, ModelCounts};

macro_rules! check_status {
    ($expr:expr) => {
        let s = $expr;
        if !s.is_ok() {
            return Err(s);
        }
    };
}

mod blendshapes;
mod context;
mod drawing_groups;
mod glues;
mod helpers;
mod meshes;
mod offscreens;
mod parameters;
mod parts;
mod transforms;

use context::Moc3DecoderContext;
pub use helpers::texture_slot_uuid;

#[derive(Debug, Clone)]
pub struct ImportIdMapping {
    pub parts: HashMap<String, String>, // runtime_id -> internal_id
    pub deformers: HashMap<String, String>, // runtime_id -> internal_id
    pub meshes: HashMap<String, String>, // runtime_id -> internal_id
    pub parameters: HashMap<String, String>, // runtime_id -> internal_id
    pub part_by_index: Vec<String>,     // index -> internal_id
    pub deformer_by_index: Vec<String>, // index -> internal_id
    pub mesh_by_index: Vec<String>,     // index -> internal_id
    pub parameter_by_index: Vec<String>, // index -> internal_id
    pub blend_key_table_by_index: Vec<String>, // index -> internal_id
    pub blend_constraint_by_index: Vec<String>, // index -> internal_id
    pub blend_binding_by_index: Vec<String>, // index -> internal_id
    pub glue_by_index: Vec<String>,     // index -> internal_id
    pub offscreen_by_index: Vec<String>, // index -> internal_id
}

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub moc_version: u8,
    pub canvas: CanvasInfo,
    pub counts: ModelCounts,
    pub generated_runtime_ids: Vec<String>,
    pub unimported_attachments: Vec<String>,
    pub warnings: Vec<String>,
    pub id_mapping: ImportIdMapping,
}

#[derive(Debug, Clone)]
pub struct DecodedMoc3 {
    pub document: Document,
    pub report: ImportReport,
}

#[derive(Debug, Clone)]
pub struct TextureSlotInfo {
    pub slot: usize,
    pub asset_id: String,
    pub name: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

pub fn decode_moc3(
    bytes: &[u8],
    _inspection: &Moc3InspectionReport,
    textures: &[TextureSlotInfo],
) -> Result<DecodedMoc3, Status> {
    let mut ctx = Moc3DecoderContext::new(bytes, textures)?;

    ctx.decode_parameters()?;
    let part_scene_bindings = ctx.decode_parts()?;
    let (deformer_scene_bindings, warp_by_local_idx, rotation_by_local_idx) =
        ctx.decode_transforms()?;
    let mesh_bindings = ctx.decode_art_meshes()?;

    for b in part_scene_bindings {
        check_status!(ctx.doc.create_scene_binding(b).status);
    }
    for b in deformer_scene_bindings {
        check_status!(ctx.doc.create_scene_binding(b).status);
    }
    for b in mesh_bindings {
        check_status!(ctx.doc.create_binding(b).status);
    }

    ctx.decode_glues()?;
    ctx.decode_blend_key_tables()?;
    ctx.decode_blend_constraints()?;
    ctx.decode_offscreens()?;
    ctx.decode_blend_bindings(&warp_by_local_idx, &rotation_by_local_idx)?;
    ctx.decode_drawing_groups()?;

    let ver = ctx.inspection.version_number;
    let counts = ctx.inspection.counts.clone();
    let canvas = ctx.inspection.canvas.clone();

    Ok(DecodedMoc3 {
        document: ctx.doc,
        report: ImportReport {
            moc_version: ver,
            canvas,
            counts,
            generated_runtime_ids: ctx.generated_ids,
            unimported_attachments: Vec::new(),
            warnings: ctx.warnings,
            id_mapping: ctx.mapping,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::helpers::parent_order;

    #[test]
    fn orders_deep_parent_chains_without_recursion() {
        let mut parents: Vec<i32> = (1..=100_000).collect();
        *parents.last_mut().unwrap() = -1;
        let order = parent_order(&parents, "Part").unwrap();
        assert_eq!(order.len(), parents.len());
        assert_eq!(order[0], parents.len() - 1);
        assert_eq!(*order.last().unwrap(), 0);
        assert!(parent_order(&[-2], "Part").is_err());
    }
}
