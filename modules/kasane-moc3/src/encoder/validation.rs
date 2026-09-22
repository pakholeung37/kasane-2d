use std::collections::HashMap;

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{BlendShapeTargetKind, Status};
use kasane_core::Document;

use super::helpers::is_representable;
use super::Moc3ExportVersion;

pub(super) fn validate_preflight(
    doc: &Document,
    target_version: Moc3ExportVersion,
) -> Result<(u8, DrawableFrame), Status> {
    if doc.transaction_active() {
        return Err(Status::error(
            "TRANSACTION_ACTIVE",
            "Commit or cancel edits before export",
        ));
    }

    let has_v53_features = doc.offscreen_count() > 0
        || doc
            .mesh_order()
            .iter()
            .any(|id| doc.get_mesh(id).and_then(|m| m.raw_blend_mode).is_some())
        || doc.blend_binding_order().iter().any(|id| {
            doc.get_blend_binding(id)
                .map(|b| b.target_kind == BlendShapeTargetKind::Offscreen)
                .unwrap_or(false)
        });

    let export_version = match target_version {
        Moc3ExportVersion::V50 => {
            if has_v53_features {
                return Err(Status::error(
                    "INCOMPATIBLE_EXPORT_VERSION",
                    "Cannot export to MOC3 5.0: document contains Cubism 5.3 features (offscreens or extended blend modes)",
                ));
            }
            5
        }
        Moc3ExportVersion::V53 => 6,
        Moc3ExportVersion::Auto => {
            if has_v53_features {
                6
            } else {
                5
            }
        }
    };

    let mut frame = DrawableFrame::default();
    let s = evaluate_frame(doc, &HashMap::new(), &mut frame);
    if !s.is_ok() {
        return Err(s);
    }

    let drawables = &frame.drawables;
    if drawables.is_empty() {
        return Err(Status::error(
            "EMPTY_MODEL",
            "At least one mesh is required",
        ));
    }

    if !(doc.canvas().height - doc.canvas().origin.y).is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            format!(
                "{}.canvas.origin: runtime origin overflows float32",
                doc.id()
            ),
        ));
    }

    for d in drawables {
        if !is_representable(&d.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", d.id),
            ));
        }
        if d.positions.len() > 65536 {
            return Err(Status::error(
                "CAPACITY",
                format!("{}.vertex_ids: at most 65536 vertices", d.id),
            ));
        }
    }

    for id in doc.parameter_order() {
        let p = doc.get_parameter(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    let parts = doc.sorted_parts();
    let transforms = doc.sorted_transforms();

    for id in &parts {
        let p = doc.get_part(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in &transforms {
        let t = doc.get_transform(id).unwrap();
        if !is_representable(&t.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in doc.glue_order() {
        let g = doc.get_glue(id).unwrap();
        if !is_representable(&g.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in doc.offscreen_order() {
        let os = doc.get_offscreen(id).unwrap();
        if !is_representable(&os.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    Ok((export_version, frame))
}
