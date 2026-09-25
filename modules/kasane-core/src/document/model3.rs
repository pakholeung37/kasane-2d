//! Typed supplemental model metadata. Known references use stable document IDs.
use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelTargetRef {
    Resolved { object_id: String },
    Unresolved { runtime_id: String },
}
impl ModelTargetRef {
    pub fn object_id(&self) -> Option<&str> {
        match self {
            Self::Resolved { object_id } => Some(object_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelParameterGroup {
    pub name: String,
    pub parameters: Vec<ModelTargetRef>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelHitArea {
    pub name: String,
    pub mesh: ModelTargetRef,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model3Settings {
    pub groups: Option<Vec<ModelParameterGroup>>,
    pub layout: Option<BTreeMap<String, f64>>,
    pub hit_areas: Option<Vec<ModelHitArea>>,
    pub user_data: Option<String>,
    pub extensions: BTreeMap<String, Value>,
    /// Provenance is needed only for opaque metadata and unmanaged IDs inside UserData.
    pub source_runtime_ids: Option<BTreeMap<String, String>>,
    pub source_content: Option<String>,
}
impl Model3Settings {
    pub fn references_to(&self, id: &str) -> bool {
        self.groups
            .iter()
            .flatten()
            .flat_map(|group| &group.parameters)
            .chain(self.hit_areas.iter().flatten().map(|area| &area.mesh))
            .any(|target| target.object_id() == Some(id))
    }
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .groups
                .iter()
                .flatten()
                .any(|g| !g.extensions.is_empty())
            || self
                .hit_areas
                .iter()
                .flatten()
                .any(|a| !a.extensions.is_empty())
    }
}
impl Document {
    pub fn model3_settings(&self) -> &Model3Settings {
        &self.model3_settings
    }

    pub fn set_model3_settings(&mut self, settings: Model3Settings) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let status = self.validate_model3_settings(&settings);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.model3_settings == settings {
            return self.failed(Status::ok());
        }
        self.model3_settings = settings;
        self.changed(ChangeKind::Metadata, Vec::new(), Vec::new())
    }

    fn validate_model3_settings(&self, settings: &Model3Settings) -> Status {
        let valid_target = |target: &ModelTargetRef, parameter: bool| match target {
            ModelTargetRef::Resolved { object_id } => {
                if parameter {
                    self.get_parameter(object_id).is_some()
                } else {
                    self.get_mesh(object_id).is_some()
                }
            }
            ModelTargetRef::Unresolved { runtime_id } => {
                !runtime_id.is_empty() && !runtime_id.contains('\0')
            }
        };
        for group in settings.groups.iter().flatten() {
            if group.name.is_empty()
                || group.name.contains('\0')
                || group
                    .parameters
                    .iter()
                    .any(|target| !valid_target(target, true))
            {
                return Status::error("INVALID_MODEL3_GROUPS", &group.name);
            }
        }
        for area in settings.hit_areas.iter().flatten() {
            // Cubism permits unnamed hit areas; the Mao sample uses them.
            if area.name.contains('\0') || !valid_target(&area.mesh, false) {
                return Status::error("INVALID_MODEL3_HIT_AREAS", &area.name);
            }
        }
        if settings
            .layout
            .iter()
            .flat_map(|layout| layout.iter())
            .any(|(key, value)| key.is_empty() || key.contains('\0') || !value.is_finite())
        {
            return Status::error(
                "INVALID_MODEL3_LAYOUT",
                "Layout requires named finite values",
            );
        }
        if settings
            .user_data
            .as_ref()
            .is_some_and(|path| !super::valid_attachment_path(path))
        {
            return Status::error(
                "INVALID_ATTACHMENT_PATH",
                settings.user_data.as_ref().unwrap(),
            );
        }
        Status::ok()
    }
    pub(super) fn validate_model3(&self) -> Vec<StructureIssue> {
        let status = self.validate_model3_settings(&self.model3_settings);
        if status.is_ok() {
            Vec::new()
        } else {
            vec![StructureIssue {
                object_id: self.id.clone(),
                status,
            }]
        }
    }
}
