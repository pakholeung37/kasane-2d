//! Self-contained non-texture files referenced by model3 registrations.
use std::collections::HashSet;
use std::path::Component;

use serde::{Deserialize, Serialize};

use super::Document;
use crate::types::{ChangeKind, EditResult, Status};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageAttachment {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub fn valid_attachment_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !path.contains('\\')
        && !path.contains('\0')
        && std::path::Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

impl Document {
    pub fn package_attachments(&self) -> &[PackageAttachment] {
        &self.package_attachments
    }

    pub fn set_package_attachments(&mut self, attachments: Vec<PackageAttachment>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut paths = HashSet::new();
        for attachment in &attachments {
            if !valid_attachment_path(&attachment.path) || !paths.insert(attachment.path.as_str()) {
                return self.failed(Status::error("INVALID_ATTACHMENT_PATH", &attachment.path));
            }
            if attachment.bytes.len() > 50 * 1024 * 1024 {
                return self.failed(Status::error("ATTACHMENT_TOO_LARGE", &attachment.path));
            }
        }
        if self.package_attachments == attachments {
            return self.failed(Status::ok());
        }
        self.package_attachments = attachments;
        self.changed(ChangeKind::Metadata, Vec::new(), Vec::new())
    }
}
