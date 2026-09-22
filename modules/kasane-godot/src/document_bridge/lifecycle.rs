use godot::prelude::*;
use kasane_core::types::{Canvas, ImageAsset, Status, Vec2};
use kasane_project::store::DocumentSession;

use crate::conversions::{error_dict, is_main_thread, status_to_dict, Dictionary};

use super::KasaneDocumentBridge;

pub(super) fn initialize(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    canvas_size: Vector2,
    origin: Vector2,
    pixels_per_unit: f64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let canvas = Canvas::new(
        canvas_size.x,
        canvas_size.y,
        Vec2::new(origin.x, origin.y),
        pixels_per_unit as f32,
    );
    let status = bridge
        .session
        .document_mut()
        .initialize(id.to_string(), canvas);
    if status.is_ok() {
        bridge.preview.get_mut().invalidate();
    }
    status_to_dict(&status)
}

/// Validate a replacement before discarding the active project and its path.
pub(super) fn new_project(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    canvas_size: Vector2,
    origin: Vector2,
    pixels_per_unit: f64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let mut next = DocumentSession::new();
    let status = next.document_mut().initialize(
        id.to_string(),
        Canvas::new(
            canvas_size.x,
            canvas_size.y,
            Vec2::new(origin.x, origin.y),
            pixels_per_unit as f32,
        ),
    );
    if !status.is_ok() {
        return status_to_dict(&status);
    }
    bridge.session = next;
    bridge.increment_generation();
    let mut out = status_to_dict(&status);
    out.set("generation", bridge.generation as i64);
    out.set("revision", bridge.session.document().revision() as i64);
    out.set("change_kind", "structure");
    bridge
        .base_mut()
        .emit_signal("changed", &[out.to_variant()]);
    out
}

pub(super) fn add_image_asset(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    name: GString,
    source: GString,
    width: i64,
    height: i64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    if width <= 0 || height <= 0 || width > u32::MAX as i64 || height > u32::MAX as i64 {
        return error_dict(
            "INVALID_ASSET",
            "Dimensions must be positive uint32 values.",
        );
    }
    let asset = ImageAsset {
        id: id.to_string(),
        name: name.to_string(),
        source: source.to_string(),
        width: width as u32,
        height: height as u32,
        sha256: String::new(),
    };
    let edit = bridge.session.document_mut().add_asset(asset);
    bridge.apply(edit)
}

pub(super) fn get_asset_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let Some(asset) = bridge.session.document().get_asset(&id.to_string()) else {
        return error_dict("MISSING_ASSET", "Asset does not exist.");
    };
    let mut out = status_to_dict(&Status::ok());
    out.set("id", asset.id.as_str());
    out.set("name", asset.name.as_str());
    out.set("source", asset.source.as_str());
    out.set("sha256", asset.sha256.as_str());
    out.set("width", asset.width as i64);
    out.set("height", asset.height as i64);
    out.set("revision", bridge.session.document().revision() as i64);
    out
}
