use std::collections::HashMap;

use godot::prelude::*;
use kasane_core::types::Status;

use crate::conversions::{
    dict_from_parameter, error_dict, is_main_thread, parameter_from_dict, status_to_dict, Array,
    Dictionary,
};

use super::KasaneDocumentBridge;

pub(super) fn create_parameter(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let p = match parameter_from_dict(&description) {
        Ok(p) => p,
        Err(s) => return status_to_dict(&s),
    };
    let edit = bridge.session.document_mut().create_parameter(p);
    bridge.apply(edit)
}

pub(super) fn replace_parameter(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let p = match parameter_from_dict(&description) {
        Ok(p) => p,
        Err(s) => return status_to_dict(&s),
    };
    let edit = bridge.session.document_mut().replace_parameter(p);
    bridge.apply(edit)
}

pub(super) fn get_parameter(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    if let Some(p) = bridge.session.document().get_parameter(&id.to_string()) {
        dict_from_parameter(p)
    } else {
        Dictionary::new()
    }
}

pub(super) fn set_preview_values(
    bridge: &mut KasaneDocumentBridge,
    values: Dictionary,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let mut next = HashMap::new();
    for (k, v) in values.iter_shared() {
        let Ok(k_str) = k.try_to::<GString>() else {
            return error_dict(
                "INVALID_FIELD",
                "Preview requires parameter IDs and numeric values.",
            );
        };
        let val = if let Ok(f) = v.try_to::<f64>() {
            f as f32
        } else if let Ok(i) = v.try_to::<i64>() {
            i as f32
        } else {
            return error_dict(
                "INVALID_FIELD",
                "Preview requires parameter IDs and numeric values.",
            );
        };
        if !val.is_finite() {
            return error_dict(
                "INVALID_FIELD",
                "Preview requires parameter IDs and numeric values.",
            );
        }
        next.insert(k_str.to_string(), val);
    }
    if let Err(status) = commit_preview(bridge, next) {
        return status_to_dict(&status);
    }
    super::inspection::get_frame(bridge)
}

pub(super) fn commit_preview(
    bridge: &mut KasaneDocumentBridge,
    next: HashMap<String, f32>,
) -> Result<(), Status> {
    let changed =
        bridge
            .preview
            .get_mut()
            .replace(bridge.session.document(), bridge.generation, next)?;
    if changed {
        bridge.base_mut().emit_signal("preview_changed", &[]);
    }
    Ok(())
}

pub(super) fn set_preview_parameter(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    value: f64,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let mut next = bridge.preview.borrow().values().clone();
    next.insert(id.to_string(), value as f32);
    match commit_preview(bridge, next) {
        Ok(()) => get_parameter_samples(bridge),
        Err(status) => status_to_dict(&status),
    }
}

pub(super) fn reset_preview_values(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    match commit_preview(bridge, HashMap::new()) {
        Ok(()) => get_parameter_samples(bridge),
        Err(status) => status_to_dict(&status),
    }
}

pub(super) fn get_parameter_samples(bridge: &KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let frame = match bridge.evaluated_frame() {
        Ok(frame) => frame,
        Err(status) => return status_to_dict(&status),
    };
    let mut out = status_to_dict(&Status::ok());
    let mut parameters = Array::new();
    for p in &frame.parameters {
        let mut sample = Dictionary::new();
        sample.set("id", p.id.as_str());
        sample.set("requested", p.requested);
        sample.set("value", p.value);
        sample.set("clamped", p.clamped);
        parameters.push(&sample);
    }
    out.set("parameters", &parameters);
    out.set("generation", bridge.generation as i64);
    out.set("revision", bridge.session.document().revision() as i64);
    out.set("evaluated_revision", frame.source_revision as i64);
    out.set(
        "preview_revision",
        bridge.preview.borrow().revision() as i64,
    );
    out.set(
        "evaluation_count",
        bridge.preview.borrow().evaluation_count() as i64,
    );
    out
}
