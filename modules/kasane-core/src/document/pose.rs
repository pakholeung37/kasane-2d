//! Persistent pose3 authoring asset. Runtime fade state belongs to animation.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PosePartRef {
    Resolved { part_id: String },
    Unresolved { runtime_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseEntry {
    pub part: PosePartRef,
    pub links: Option<Vec<PosePartRef>>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseAsset {
    pub id: String,
    pub file_type: Option<String>,
    pub fade_in: Option<f32>,
    pub groups: Vec<Vec<PoseEntry>>,
    pub extensions: BTreeMap<String, Value>,
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

impl PoseAsset {
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .groups
                .iter()
                .flatten()
                .any(|entry| !entry.extensions.is_empty())
    }
}

impl Document {
    pub fn pose(&self) -> Option<&PoseAsset> {
        self.pose.as_ref()
    }

    pub fn set_pose(&mut self, pose: PoseAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.pose.as_ref().is_none_or(|old| old.id != pose.id) && self.contains_id(&pose.id) {
            return self.failed(Status::error("DUPLICATE_ID", &pose.id));
        }
        let status = self.validate_pose(&pose);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.pose.as_ref() == Some(&pose) {
            return self.failed(Status::ok());
        }
        let id = pose.id.clone();
        self.pose = Some(pose);
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    fn validate_pose(&self, pose: &PoseAsset) -> Status {
        if !super::valid_uuid(&pose.id) {
            return Status::error("INVALID_POSE_ID", &pose.id);
        }
        if pose
            .file_type
            .as_deref()
            .is_some_and(|value| value != "Live2D Pose")
        {
            return Status::error("INVALID_POSE_TYPE", &pose.id);
        }
        if pose
            .fade_in
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Status::error("INVALID_POSE_FADE", &pose.id);
        }
        for (group_index, group) in pose.groups.iter().enumerate() {
            let mut seen = HashSet::new();
            for (entry_index, entry) in group.iter().enumerate() {
                let path = format!("{}.groups[{group_index}][{entry_index}]", pose.id);
                let runtime = match self.validate_pose_ref(&entry.part) {
                    Ok(id) => id,
                    Err(status) => return status,
                };
                if !seen.insert(runtime) {
                    return Status::error("DUPLICATE_POSE_PART", path);
                }
                if let Some(links) = &entry.links {
                    let mut linked = HashSet::new();
                    for target in links {
                        let runtime = match self.validate_pose_ref(target) {
                            Ok(id) => id,
                            Err(status) => return status,
                        };
                        if !linked.insert(runtime) {
                            return Status::error("DUPLICATE_POSE_LINK", path);
                        }
                    }
                }
            }
        }
        Status::ok()
    }

    fn validate_pose_ref(&self, target: &PosePartRef) -> Result<String, Status> {
        match target {
            PosePartRef::Resolved { part_id } => self
                .get_part(part_id)
                .map(|part| part.runtime_id.clone())
                .ok_or_else(|| Status::error("MISSING_PART", part_id)),
            PosePartRef::Unresolved { runtime_id }
                if runtime_id.is_empty() || runtime_id.contains('\0') =>
            {
                Err(Status::error("INVALID_POSE_PART", runtime_id))
            }
            PosePartRef::Unresolved { runtime_id } => Ok(runtime_id.clone()),
        }
    }

    pub(super) fn validate_pose_asset(&self) -> Vec<StructureIssue> {
        self.pose
            .as_ref()
            .and_then(|pose| {
                let status = self.validate_pose(pose);
                (!status.is_ok()).then(|| StructureIssue {
                    object_id: pose.id.clone(),
                    status,
                })
            })
            .into_iter()
            .collect()
    }
}
