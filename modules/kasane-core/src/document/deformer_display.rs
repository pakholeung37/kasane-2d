//! Editor control visibility; never participates in runtime deformation or rendering.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeformerDisplay {
    /// Only these deformers' own controls are hidden. This does not inherit.
    #[serde(default)]
    pub hidden: Vec<String>,
}

impl DeformerDisplay {
    pub fn is_empty(&self) -> bool {
        self.hidden.is_empty()
    }
}

impl Document {
    pub fn deformer_display(&self) -> &DeformerDisplay {
        &self.deformer_display
    }

    pub fn replace_deformer_display(&mut self, mut display: DeformerDisplay) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.deformer_display_issues(&display).into_iter().next() {
            return self.failed(issue.status);
        }
        display.hidden.sort();
        if self.deformer_display == display {
            return self.failed(Status::ok());
        }
        let mut changed: Vec<_> = self
            .deformer_display
            .hidden
            .iter()
            .chain(&display.hidden)
            .cloned()
            .collect();
        changed.sort();
        changed.dedup();
        self.deformer_display = display;
        self.changed(ChangeKind::Metadata, Vec::new(), changed)
    }

    pub(super) fn deformer_display_issues(&self, display: &DeformerDisplay) -> Vec<StructureIssue> {
        let mut seen = HashSet::new();
        display
            .hidden
            .iter()
            .filter_map(|id| {
                (!self.transforms.contains_key(id) || !seen.insert(id)).then(|| StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_DEFORMER_DISPLAY",
                        "Hidden controls contain a missing, incompatible or repeated deformer",
                    ),
                })
            })
            .collect()
    }
}
