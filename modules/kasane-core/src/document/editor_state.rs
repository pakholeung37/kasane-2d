//! Persistent authoring preferences. Never read by runtime evaluation or export.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectEditorState {
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden_in_editor: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorState {
    /// Explicit flags only, keyed by globally unique object ID. Defaults are omitted.
    #[serde(default)]
    pub objects: BTreeMap<String, ObjectEditorState>,
}
impl EditorState {
    pub fn hidden_objects(&self) -> impl Iterator<Item = &str> {
        self.objects
            .iter()
            .filter(|(_, state)| state.hidden_in_editor)
            .map(|(id, _)| id.as_str())
    }
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
    pub fn get(&self, id: &str) -> ObjectEditorState {
        self.objects.get(id).copied().unwrap_or_default()
    }
}
impl Document {
    pub fn editor_state(&self) -> &EditorState {
        &self.editor_state
    }

    pub fn replace_editor_state(&mut self, mut state: EditorState) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.editor_state_issues(&state).into_iter().next() {
            return self.failed(issue.status);
        }
        state
            .objects
            .retain(|_, flags| flags.hidden_in_editor || flags.locked);
        if self.editor_state == state {
            return self.failed(Status::ok());
        }
        let mut changed: Vec<_> = self
            .editor_state
            .objects
            .keys()
            .chain(state.objects.keys())
            .cloned()
            .collect();
        changed.sort();
        changed.dedup();
        self.editor_state = state;
        self.changed(ChangeKind::Metadata, Vec::new(), changed)
    }

    pub(super) fn editor_state_issues(&self, state: &EditorState) -> Vec<StructureIssue> {
        state
            .objects
            .keys()
            .filter_map(|id| {
                let valid = self.meshes.contains_key(id)
                    || self.transforms.contains_key(id)
                    || self.parts.contains_key(id)
                    || self.offscreens.contains_key(id)
                    || self.glues.contains_key(id);
                (!valid).then(|| StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_EDITOR_STATE",
                        "Editor state references a missing or incompatible object",
                    ),
                })
            })
            .collect()
    }
}
