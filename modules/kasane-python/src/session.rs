//! Python-facing authoring session and read/IO operations.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::conversion::*;
use crate::edit::NativeEdit;
use crate::error::{edit_failure, poisoned, sdk_failure};
use crate::handle::NativeHandle;
use kasane_core::{Canvas, Vec2};
use kasane_sdk::{
    AuthoringSession, GeometryBounds, GeometryChecks, GeometryDiagnosticKind, HistoryLimits,
    MeshProperties, SdkError, SourceSpace,
};
use pyo3::prelude::*;

#[pyclass]
pub(crate) struct NativeSession {
    pub(crate) inner: Arc<Mutex<AuthoringSession>>,
    active_edit: Arc<AtomicBool>,
}

impl NativeSession {
    fn ensure_idle(&self, py: Python<'_>, operation: &str) -> PyResult<()> {
        if self.active_edit.load(Ordering::Acquire) {
            Err(edit_failure(
                py,
                "EDIT_ACTIVE",
                operation,
                "An edit is active",
            ))
        } else {
            Ok(())
        }
    }
}

fn active_edit_error(operation: &'static str) -> SdkError {
    SdkError {
        code: "EDIT_ACTIVE".into(),
        message: "An edit is active".into(),
        operation,
        object_ids: Vec::new(),
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    }
}

mod diagnostics;
mod editing;
mod evaluation;
mod ids;
mod lifecycle;
mod objects;
mod project_io;
