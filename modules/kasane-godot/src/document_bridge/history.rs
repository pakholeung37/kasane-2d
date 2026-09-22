use godot::prelude::*;
use kasane_core::types::ChangeKind;

use crate::conversions::{error_dict, is_main_thread, status_to_dict, Dictionary};

use super::KasaneDocumentBridge;

pub(super) fn begin_transaction(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let s = bridge.session.document_mut().begin_transaction();
    status_to_dict(&s)
}

pub(super) fn commit_transaction(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let edit = bridge.session.document_mut().commit_transaction();
    bridge.apply(edit)
}

pub(super) fn cancel_transaction(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let s = bridge.session.document_mut().cancel_transaction();
    status_to_dict(&s)
}

pub(super) fn get_history_state(bridge: &KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let history = bridge.session.history();
    let mut out = Dictionary::new();
    out.set("undo_steps", history.undo_len() as i64);
    out.set("redo_steps", history.redo_len() as i64);
    out.set("estimated_bytes", history.estimated_bytes() as i64);
    out.set("action_active", history.active());
    out.set("warning", history.notice().unwrap_or(""));
    out
}

pub(super) fn begin_action(bridge: &mut KasaneDocumentBridge, label: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let status = bridge.session.begin_action(label.to_string());
    status_to_dict(&status)
}

pub(super) fn end_action(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let status = bridge.session.end_action();
    let mut out = status_to_dict(&status);
    out.set("history", &get_history_state(bridge));
    if let Some(notice) = bridge.session.history().notice() {
        out.set("history_warning", notice);
    }
    out
}

pub(super) fn cancel_action(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let result = bridge.session.cancel_action();
    if result.status.is_ok()
        && matches!(
            result.changes.kind,
            ChangeKind::Positions | ChangeKind::Structure | ChangeKind::Resources
        )
    {
        bridge.preview.get_mut().reset();
    }
    bridge.publish_edit(result)
}

pub(super) fn undo(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let result = bridge.session.undo();
    if result.status.is_ok()
        && matches!(
            result.changes.kind,
            ChangeKind::Positions | ChangeKind::Structure | ChangeKind::Resources
        )
    {
        bridge.preview.get_mut().reset();
    }
    bridge.publish_edit(result)
}

pub(super) fn redo(bridge: &mut KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let result = bridge.session.redo();
    if result.status.is_ok()
        && matches!(
            result.changes.kind,
            ChangeKind::Positions | ChangeKind::Structure | ChangeKind::Resources
        )
    {
        bridge.preview.get_mut().reset();
    }
    bridge.publish_edit(result)
}
