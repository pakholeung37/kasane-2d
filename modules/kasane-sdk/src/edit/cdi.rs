//! CDI authoring commands. Each command validates the complete candidate
//! before publication, so a failed edit cannot leave partial memberships.
use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::{
    CdiCombinedSet, CdiParameterEntry, CdiParameterGroup, DisplayInfo, DisplayInfoOrigin,
};
use kasane_core::ChangeKind;
use kasane_project::{import_cdi3, CdiProjectDiagnostic, CdiProjectError};
use std::collections::BTreeMap;

fn project_error(error: CdiProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn replace_display_info(&mut self, info: DisplayInfo) -> Result<(), SdkError> {
        self.ensure_active("replace_display_info")?;
        let id = self.document().id().to_owned();
        let result = self.document().replace_display_info(info);
        self.record(result, "replace_display_info", &id)
    }

    pub fn set_parameter_display_name(
        &mut self,
        parameter_id: &str,
        name: String,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_parameter_display_name")?;
        let result = self
            .document()
            .set_parameter_display_name(parameter_id, name);
        self.record(result, "set_parameter_display_name", parameter_id)
    }

    pub fn set_part_display_name(&mut self, part_id: &str, name: String) -> Result<(), SdkError> {
        self.ensure_active("set_part_display_name")?;
        let result = self.document().set_part_display_name(part_id, name);
        self.record(result, "set_part_display_name", part_id)
    }

    pub fn create_parameter_group(&mut self, group: CdiParameterGroup) -> Result<(), SdkError> {
        self.ensure_active("create_parameter_group")?;
        let mut info = self.candidate_document().display_info().clone();
        if info
            .parameter_groups
            .as_ref()
            .is_some_and(|groups| groups.iter().any(|item| item.id == group.id))
        {
            return Err(self.abort_with(SdkError::new(
                "DUPLICATE_ID",
                "CDI parameter group already exists",
                "create_parameter_group",
            )));
        }
        info.parameter_groups
            .get_or_insert_with(Vec::new)
            .push(group);
        self.replace_display_info(info)
    }

    pub fn replace_parameter_group(&mut self, group: CdiParameterGroup) -> Result<(), SdkError> {
        self.ensure_active("replace_parameter_group")?;
        let mut info = self.candidate_document().display_info().clone();
        let Some(existing) = info
            .parameter_groups
            .as_mut()
            .and_then(|groups| groups.iter_mut().find(|item| item.id == group.id))
        else {
            return Err(self.abort_with(SdkError::new(
                "MISSING_CDI_GROUP",
                "CDI parameter group does not exist",
                "replace_parameter_group",
            )));
        };
        *existing = group;
        self.replace_display_info(info)
    }

    /// Materializes generated entries before changing one membership, so
    /// other parameters do not disappear from the exported CDI.
    pub fn set_parameter_group(
        &mut self,
        parameter_id: &str,
        group_id: Option<&str>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_parameter_group")?;
        if self
            .candidate_document()
            .get_parameter(parameter_id)
            .is_none()
        {
            return Err(self.abort_with(SdkError::new(
                "MISSING_PARAMETER",
                "Parameter does not exist",
                "set_parameter_group",
            )));
        }
        let mut info = self.candidate_document().display_info().clone();
        if info.origin == DisplayInfoOrigin::Generated && info.parameters.is_none() {
            info.parameters = Some(
                self.candidate_document()
                    .parameter_order()
                    .iter()
                    .map(|id| CdiParameterEntry::Resolved {
                        parameter_id: id.clone(),
                        group_id: None,
                        extensions: BTreeMap::new(),
                    })
                    .collect(),
            );
        }
        let entries = info.parameters.get_or_insert_with(Vec::new);
        if let Some(CdiParameterEntry::Resolved { group_id: current, .. }) = entries.iter_mut().find(
            |entry| matches!(entry, CdiParameterEntry::Resolved { parameter_id: id, .. } if id == parameter_id),
        ) {
            *current = group_id.map(str::to_owned);
        } else {
            entries.push(CdiParameterEntry::Resolved {
                parameter_id: parameter_id.into(),
                group_id: group_id.map(str::to_owned),
                extensions: BTreeMap::new(),
            });
        }
        self.replace_display_info(info)
    }

    pub fn set_combined_parameters(&mut self, set: CdiCombinedSet) -> Result<(), SdkError> {
        self.ensure_active("set_combined_parameters")?;
        let mut info = self.candidate_document().display_info().clone();
        let sets = info.combined_parameters.get_or_insert_with(Vec::new);
        if let Some(existing) = sets.iter_mut().find(|item| item.id == set.id) {
            *existing = set;
        } else {
            sets.push(set);
        }
        self.replace_display_info(info)
    }

    /// Import CDI into the current edit workspace. The original session is
    /// untouched until the containing edit commits.
    pub fn import_cdi3(&mut self, text: &str) -> Result<Vec<CdiProjectDiagnostic>, SdkError> {
        self.ensure_active("import_cdi3")?;
        let imported = import_cdi3(self.candidate_document(), text)
            .map_err(|error| self.abort_with(project_error(error, "import_cdi3")))?;
        let id = imported.candidate.id().to_owned();
        self.candidate = Some(imported.candidate);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.contains(&id) {
            self.object_ids.push(id);
        }
        Ok(imported.diagnostics)
    }
}
