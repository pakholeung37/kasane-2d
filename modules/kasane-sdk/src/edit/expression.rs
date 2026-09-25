use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::ExpressionAsset;
use kasane_core::ChangeKind;
use kasane_project::{import_expression3, ExpressionDiagnostic, ExpressionProjectError};

fn project_error(error: ExpressionProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    pub fn create_expression(&mut self, expression: ExpressionAsset) -> Result<(), SdkError> {
        self.ensure_active("create_expression")?;
        let id = expression.id.clone();
        let result = self.document().create_expression(expression);
        self.record(result, "create_expression", &id)
    }

    pub fn replace_expression(&mut self, expression: ExpressionAsset) -> Result<(), SdkError> {
        self.ensure_active("replace_expression")?;
        let id = expression.id.clone();
        let result = self.document().replace_expression(expression);
        self.record(result, "replace_expression", &id)
    }

    pub fn import_expression3(
        &mut self,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<Vec<ExpressionDiagnostic>, SdkError> {
        self.ensure_active("import_expression3")?;
        let imported = import_expression3(self.candidate_document(), id, name, text)
            .map_err(|error| self.abort_with(project_error(error, "import_expression3")))?;
        self.candidate = Some(imported.candidate);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
