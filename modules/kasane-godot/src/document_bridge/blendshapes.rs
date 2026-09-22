use godot::prelude::*;
use kasane_core::types::{BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable};

use crate::conversions::{
    binding_from_dict, error_dict, is_main_thread, status_to_dict, structured_from_dict,
    structured_to_dict, Dictionary,
};

use super::KasaneDocumentBridge;

pub(super) fn write_binding(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let b = match binding_from_dict(&description) {
        Ok(b) => b,
        Err(s) => return status_to_dict(&s),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_binding(b)
    } else {
        bridge.session.document_mut().create_binding(b)
    };
    bridge.apply(edit)
}

pub(super) fn write_blend_key_table(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let value = match structured_from_dict::<BlendShapeKeyTable>(&description) {
        Ok(value) => value,
        Err(status) => return status_to_dict(&status),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_blend_key_table(value)
    } else {
        bridge.session.document_mut().create_blend_key_table(value)
    };
    bridge.apply(edit)
}

pub(super) fn get_blend_key_table_snapshot(
    bridge: &KasaneDocumentBridge,
    id: GString,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    match bridge
        .session
        .document()
        .get_blend_key_table(&id.to_string())
    {
        Some(value) => structured_to_dict(value),
        None => error_dict("MISSING_OBJECT", &id.to_string()),
    }
}

pub(super) fn write_blend_constraint(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let value = match structured_from_dict::<BlendShapeConstraint>(&description) {
        Ok(value) => value,
        Err(status) => return status_to_dict(&status),
    };
    let edit = if replace {
        bridge
            .session
            .document_mut()
            .replace_blend_constraint(value)
    } else {
        bridge.session.document_mut().create_blend_constraint(value)
    };
    bridge.apply(edit)
}

pub(super) fn get_blend_constraint_snapshot(
    bridge: &KasaneDocumentBridge,
    id: GString,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    match bridge
        .session
        .document()
        .get_blend_constraint(&id.to_string())
    {
        Some(value) => structured_to_dict(value),
        None => error_dict("MISSING_OBJECT", &id.to_string()),
    }
}

pub(super) fn write_blend_binding(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    replace: bool,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let value = match structured_from_dict::<BlendShapeBinding>(&description) {
        Ok(value) => value,
        Err(status) => return status_to_dict(&status),
    };
    let edit = if replace {
        bridge.session.document_mut().replace_blend_binding(value)
    } else {
        bridge.session.document_mut().create_blend_binding(value)
    };
    bridge.apply(edit)
}

pub(super) fn get_blend_binding_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    match bridge.session.document().get_blend_binding(&id.to_string()) {
        Some(value) => structured_to_dict(value),
        None => error_dict("MISSING_OBJECT", &id.to_string()),
    }
}
