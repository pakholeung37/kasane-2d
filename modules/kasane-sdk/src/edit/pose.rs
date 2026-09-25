use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::PoseAsset;
use kasane_core::ChangeKind;
use kasane_project::{import_pose3, PoseDiagnostic, PoseProjectError};

fn project_error(error: PoseProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn set_pose(&mut self, pose: PoseAsset) -> Result<(), SdkError> {
        self.ensure_active("set_pose")?;
        let id = pose.id.clone();
        let result = self.document().set_pose(pose);
        self.record(result, "set_pose", &id)
    }

    pub fn import_pose3(&mut self, id: &str, text: &str) -> Result<Vec<PoseDiagnostic>, SdkError> {
        self.ensure_active("import_pose3")?;
        let imported = import_pose3(self.candidate_document(), id, text)
            .map_err(|error| self.abort_with(project_error(error, "import_pose3")))?;
        self.candidate = Some(imported.candidate);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
