use godot::prelude::*;
use kasane_core::types::Offscreen;

use crate::conversions::{
    error_dict, is_main_thread, status_to_dict, structured_from_dict, structured_to_dict,
    Dictionary,
};

use super::KasaneDocumentBridge;

pub(super) fn write_offscreen(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let value = match structured_from_dict::<Offscreen>(&description) {
        Ok(value) => value,
        Err(status) => return status_to_dict(&status),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_offscreen(value)
    } else {
        bridge.session.document_mut().create_offscreen(value)
    };
    bridge.apply(edit)
}

pub(super) fn get_offscreen_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    match bridge.session.document().get_offscreen(&id.to_string()) {
        Some(value) => structured_to_dict(value),
        None => error_dict("MISSING_OBJECT", &id.to_string()),
    }
}
