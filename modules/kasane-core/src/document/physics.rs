//! Persistent physics rig data with references bound to document parameter IDs.
use std::collections::{BTreeMap, HashSet};

use crate::physics::{validate_physics_definition, PhysicsDefinition};
use serde::{Deserialize, Serialize};

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsAsset {
    pub id: String,
    pub data: PhysicsDefinition,
    /// Original runtime ID to stable document parameter UUID. Absent entries
    /// remain unresolved and are diagnosed by import and strict export.
    pub parameter_bindings: BTreeMap<String, String>,
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

impl PhysicsAsset {
    pub fn referenced_runtime_ids(&self) -> HashSet<&str> {
        self.data
            .settings
            .iter()
            .flat_map(|rig| {
                rig.inputs
                    .iter()
                    .map(|input| input.source.id.as_str())
                    .chain(
                        rig.outputs
                            .iter()
                            .map(|output| output.destination.id.as_str()),
                    )
            })
            .collect()
    }
}

impl Document {
    pub fn physics(&self) -> Option<&PhysicsAsset> {
        self.physics.as_ref()
    }

    pub fn set_physics(&mut self, asset: PhysicsAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.physics.as_ref().is_none_or(|old| old.id != asset.id) && self.contains_id(&asset.id)
        {
            return self.failed(Status::error("DUPLICATE_ID", &asset.id));
        }
        let status = self.validate_physics(&asset);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.physics.as_ref() == Some(&asset) {
            return self.failed(Status::ok());
        }
        let id = asset.id.clone();
        self.physics = Some(asset);
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    fn validate_physics(&self, asset: &PhysicsAsset) -> Status {
        if !super::valid_uuid(&asset.id) {
            return Status::error("INVALID_PHYSICS_ID", &asset.id);
        }
        if let Err(error) = validate_physics_definition(&asset.data) {
            return Status::error(error.code, format!("{}: {}", error.path, error.message));
        }
        let referenced = asset.referenced_runtime_ids();
        for (runtime_id, parameter_id) in &asset.parameter_bindings {
            if !referenced.contains(runtime_id.as_str()) {
                return Status::error("UNUSED_PHYSICS_BINDING", runtime_id);
            }
            if self.get_parameter(parameter_id).is_none() {
                return Status::error("MISSING_PARAMETER", parameter_id);
            }
        }
        Status::ok()
    }

    pub(super) fn validate_physics_asset(&self) -> Vec<StructureIssue> {
        self.physics
            .as_ref()
            .and_then(|asset| {
                let status = self.validate_physics(asset);
                (!status.is_ok()).then(|| StructureIssue {
                    object_id: asset.id.clone(),
                    status,
                })
            })
            .into_iter()
            .collect()
    }
}
