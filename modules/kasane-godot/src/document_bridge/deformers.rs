use godot::prelude::*;
use kasane_core::types::{
    RotationPose, RotationTransform, Status, Transform, TransformData, TransformKind, Vec2,
    WarpTransform,
};

use crate::conversions::{
    error_dict, is_main_thread, packed_to_vectors, status_to_dict, transform_from_dict,
    vectors_to_packed, Dictionary,
};
use crate::deformer_data::KasaneDeformerData;

use super::KasaneDocumentBridge;

pub(super) fn create_rotation(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    name: GString,
    center: Vector2,
    angle: f64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let t = Transform {
        id: id.to_string(),
        runtime_id: format!("Rotation_{}", id.to_string().replace('-', "")),
        name: name.to_string(),
        part_id: kasane_core::PartId::optional(String::new()),
        parent_id: kasane_core::TransformId::optional(String::new()),
        enabled: true,
        appearance: Default::default(),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 0.0,
            pose: RotationPose {
                origin: Vec2::new(center.x, center.y).into(),
                angle: angle as f32,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
        }),
    };
    let edit = bridge.session.document_mut().create_transform(t);
    bridge.apply(edit)
}

pub(super) fn create_warp(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    name: GString,
    origin: Vector2,
    size: Vector2,
    columns: i64,
    rows: i64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    if columns < 1 || rows < 1 || columns > 16 || rows > 16 {
        return error_dict("INVALID_WARP", "Warp supports 1–16 cells per axis.");
    }
    let mut points = Vec::new();
    for y in 0..=rows {
        for x in 0..=columns {
            points.push(Vec2::new(
                origin.x + size.x * (x as f32 / columns as f32),
                origin.y + size.y * (y as f32 / rows as f32),
            ));
        }
    }
    let t = Transform {
        id: id.to_string(),
        runtime_id: format!("Warp_{}", id.to_string().replace('-', "")),
        name: name.to_string(),
        part_id: kasane_core::PartId::optional(String::new()),
        parent_id: kasane_core::TransformId::optional(String::new()),
        enabled: true,
        appearance: Default::default(),
        data: TransformData::Warp(WarpTransform {
            rows: rows as u32,
            columns: columns as u32,
            quad: false,
            points,
        }),
    };
    let edit = bridge.session.document_mut().create_transform(t);
    bridge.apply(edit)
}

pub(super) fn set_rotation(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    center: Vector2,
    angle: f64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let Some(old) = bridge.session.document().get_transform(&id.to_string()) else {
        return error_dict("MISSING_TRANSFORM", &id.to_string());
    };
    if old.kind() != TransformKind::Rotation {
        return error_dict("WRONG_TRANSFORM_KIND", "Use a Rotation deformer.");
    }
    let mut t = old.clone();
    t.rotation_mut().unwrap().pose.origin = Vec2::new(center.x, center.y).into();
    t.rotation_mut().unwrap().pose.angle = angle as f32;
    let edit = bridge.session.document_mut().replace_transform(t);
    bridge.apply(edit)
}

pub(super) fn set_warp_points(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    points: PackedVector2Array,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let Some(old) = bridge.session.document().get_transform(&id.to_string()) else {
        return error_dict("MISSING_TRANSFORM", &id.to_string());
    };
    if old.kind() != TransformKind::Warp {
        return error_dict("WRONG_TRANSFORM_KIND", "Use a Warp deformer.");
    }
    let mut t = old.clone();
    t.warp_mut().unwrap().points = packed_to_vectors(&points);
    let edit = bridge.session.document_mut().replace_transform(t);
    bridge.apply(edit)
}

pub(super) fn set_deform_parent(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    parent: GString,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let id_str = id.to_string();
    let parent_str = parent.to_string();
    if let Some(old) = bridge.session.document().get_transform(&id_str) {
        let mut t = old.clone();
        t.parent_id = kasane_core::TransformId::optional(parent_str);
        let edit = bridge.session.document_mut().replace_transform(t);
        return bridge.apply(edit);
    }
    if let Some(old) = bridge.session.document().get_mesh(&id_str) {
        let mut m = old.clone();
        m.deformer_id = parent_str;
        let edit = bridge.session.document_mut().replace_mesh(m);
        return bridge.apply(edit);
    }
    error_dict("MISSING_OBJECT", &id_str)
}

pub(super) fn set_organization_parent(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    parent: GString,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let id_str = id.to_string();
    let parent_str = parent.to_string();
    if let Some(old) = bridge.session.document().get_part(&id_str) {
        let mut p = old.clone();
        p.parent_id = parent_str;
        let edit = bridge.session.document_mut().replace_part(p);
        return bridge.apply(edit);
    }
    if let Some(old) = bridge.session.document().get_transform(&id_str) {
        let mut t = old.clone();
        t.part_id = kasane_core::PartId::optional(parent_str);
        let edit = bridge.session.document_mut().replace_transform(t);
        return bridge.apply(edit);
    }
    if let Some(old) = bridge.session.document().get_mesh(&id_str) {
        let mut m = old.clone();
        m.part_id = parent_str;
        let edit = bridge.session.document_mut().replace_mesh(m);
        return bridge.apply(edit);
    }
    error_dict("MISSING_OBJECT", &id_str)
}

pub(super) fn get_deformer_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let Some(t) = bridge.session.document().get_transform(&id.to_string()) else {
        return error_dict("MISSING_DEFORMER", "Deformer does not exist.");
    };
    let mut out = status_to_dict(&Status::ok());
    out.set("id", &id);
    out.set("name", t.name.as_str());
    let kind_str = match t.kind() {
        TransformKind::Rotation => "rotation",
        TransformKind::Warp => "warp",
    };
    out.set("kind", kind_str);
    out.set("deform_parent", t.parent());
    out.set("organization_parent", t.part());
    out.set("revision", bridge.session.document().revision() as i64);
    if t.kind() == TransformKind::Rotation {
        out.set(
            "center",
            Vector2::new(
                t.rotation().unwrap().pose.origin.x as f32,
                t.rotation().unwrap().pose.origin.y as f32,
            ),
        );
        out.set("angle_degrees", t.rotation().unwrap().pose.angle as f64);
    } else {
        out.set(
            "origin",
            Vector2::new(
                t.warp().unwrap().points.first().map(|p| p.x).unwrap_or(0.0),
                t.warp().unwrap().points.first().map(|p| p.y).unwrap_or(0.0),
            ),
        );
        out.set("size", Vector2::ZERO);
        out.set("columns", t.warp().unwrap().columns as i64);
        out.set("rows", t.warp().unwrap().rows as i64);
        out.set(
            "control_points",
            &vectors_to_packed(&t.warp().unwrap().points),
        );
    }
    out
}

pub(super) fn get_deformer(
    bridge: &KasaneDocumentBridge,
    id: GString,
) -> Option<Gd<KasaneDeformerData>> {
    if !is_main_thread() {
        return None;
    }
    bridge.session.document().get_transform(&id.to_string())?;
    let mut handle = Gd::<KasaneDeformerData>::default();
    handle.bind_mut().attach(
        bridge.base().instance_id().to_i64() as u64,
        bridge.generation,
        bridge.object_epoch(&id.to_string()),
        id,
    );
    Some(handle)
}

pub(super) fn write_transform(
    bridge: &mut KasaneDocumentBridge,
    data: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let t = match transform_from_dict(&data) {
        Ok(t) => t,
        Err(s) => return status_to_dict(&s),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_transform(t)
    } else {
        bridge.session.document_mut().create_transform(t)
    };
    bridge.apply(edit)
}
