use crate::conversions::{
    error_dict, is_main_thread, part_from_dict, scene_binding_from_dict, status_to_dict, Dictionary,
};

use super::KasaneDocumentBridge;

pub(super) fn write_part(
    bridge: &mut KasaneDocumentBridge,
    data: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let p = match part_from_dict(&data) {
        Ok(p) => p,
        Err(s) => return status_to_dict(&s),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_part(p)
    } else {
        bridge.session.document_mut().create_part(p)
    };
    bridge.apply(edit)
}

/// Call inside one workspace action for a single undo/redo entry.
pub(super) fn replace_part_binding_with_offscreen(
    bridge: &mut KasaneDocumentBridge,
    binding: Dictionary,
    offscreen: Dictionary,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let binding = match scene_binding_from_dict(&binding, bridge.session.document()) {
        Ok(value) => value,
        Err(status) => return status_to_dict(&status),
    };
    let offscreen =
        match crate::conversions::structured_from_dict::<kasane_core::types::Offscreen>(&offscreen)
        {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
    let edit = bridge
        .session
        .document_mut()
        .replace_part_binding_with_offscreen(binding, offscreen);
    bridge.apply(edit)
}

pub(super) fn write_scene_binding(
    bridge: &mut KasaneDocumentBridge,
    data: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let b = match scene_binding_from_dict(&data, bridge.session.document()) {
        Ok(b) => b,
        Err(s) => return status_to_dict(&s),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_scene_binding(b)
    } else {
        bridge.session.document_mut().create_scene_binding(b)
    };
    bridge.apply(edit)
}
