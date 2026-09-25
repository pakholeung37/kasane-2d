use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::{Model3Settings, PackageAttachment};

impl EditSession<'_> {
    pub fn set_package_attachments(
        &mut self,
        attachments: Vec<PackageAttachment>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_package_attachments")?;
        let result = self.document().set_package_attachments(attachments);
        self.record(result, "set_package_attachments", "attachments")
    }
    pub fn set_model3_settings(&mut self, settings: Model3Settings) -> Result<(), SdkError> {
        self.ensure_active("set_model3_settings")?;
        let result = self.document().set_model3_settings(settings);
        self.record(result, "set_model3_settings", "model3")
    }
}
