//! Persistent editor protection metadata. Runtime evaluation and SDK writes ignore locks.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectLocks {
    /// Explicit locks only; editors derive inheritance from the Part organization hierarchy.
    #[serde(default)]
    pub objects: Vec<String>,
}

impl ObjectLocks {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

impl Document {
    pub fn object_locks(&self) -> &ObjectLocks {
        &self.object_locks
    }

    pub fn replace_object_locks(&mut self, mut locks: ObjectLocks) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.object_lock_issues(&locks).into_iter().next() {
            return self.failed(issue.status);
        }
        locks.objects.sort();
        if self.object_locks == locks {
            return self.failed(Status::ok());
        }
        let mut changed: Vec<_> = self
            .object_locks
            .objects
            .iter()
            .chain(&locks.objects)
            .cloned()
            .collect();
        changed.sort();
        changed.dedup();
        self.object_locks = locks;
        self.changed(ChangeKind::Metadata, Vec::new(), changed)
    }

    pub(super) fn object_lock_issues(&self, locks: &ObjectLocks) -> Vec<StructureIssue> {
        let mut seen = HashSet::new();
        locks
            .objects
            .iter()
            .filter_map(|id| {
                let valid = self.meshes.contains_key(id)
                    || self.transforms.contains_key(id)
                    || self.parts.contains_key(id)
                    || self.offscreens.contains_key(id);
                (!valid || !seen.insert(id)).then(|| StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_OBJECT_LOCKS",
                        "Object locks contain a missing, incompatible or repeated object",
                    ),
                })
            })
            .collect()
    }
}
