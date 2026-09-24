//! Project lifecycle, import/export and save publication.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::assets::{relocate_history_assets, validate_asset_root};
use crate::session::AuthoringSession;
use crate::types::{ImportReceipt, PsdImportReceipt, SaveReceipt, SdkError, Version};
use kasane_core::{Canvas, Document};
use kasane_project::{ProjectResult, ResourceDiagnostic};

impl AuthoringSession {
    /// Atomically replace the in-memory document. A failed request leaves the
    /// current document, history, preview and handles intact.
    pub fn new_project(
        &mut self,
        document_id: &str,
        canvas: Canvas,
        expected: Option<Version>,
    ) -> Result<Version, SdkError> {
        let current = self.version();
        if let Some(value) = expected.filter(|value| *value != current) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "new_project");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(current));
            return Err(error);
        }
        let mut document = Document::new();
        let status = document.initialize(document_id, canvas);
        if !status.is_ok() {
            return Err(SdkError::from_status(
                status,
                "new_project",
                vec![document_id.into()],
            ));
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "new_project",
            )
        })?;
        self.project.reset_authoring_document(document);
        self.generation = generation;
        self.done.clear();
        self.redo.clear();
        self.preview.reset();
        self.incarnations.clear();
        self.events.clear();
        Ok(self.version())
    }

    /// Open a saved project. Missing or corrupt texture files are reported in
    /// the successful ProjectResult diagnostics without discarding the document.
    pub fn open_project(
        &mut self,
        path: &Path,
        expected: Option<Version>,
    ) -> Result<ProjectResult, SdkError> {
        if !path.is_absolute() {
            return Err(SdkError::new(
                "INVALID_PATH",
                "Project path must be absolute",
                "open_project",
            ));
        }
        let current = self.version();
        if let Some(value) = expected.filter(|value| *value != current) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "open_project");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(current));
            return Err(error);
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "open_project",
            )
        })?;
        let result = self.project.open_authoring(path);
        if !result.status.is_ok() {
            return Err(SdkError::from_status(
                result.status,
                "open_project",
                vec![path.display().to_string()],
            ));
        }
        self.reset_after_open(generation);
        Ok(result)
    }

    pub fn document_id(&self) -> &str {
        self.project.document().id()
    }
    pub fn project_path(&self) -> Option<&Path> {
        let path = self.project.manifest();
        (!path.as_os_str().is_empty()).then_some(path)
    }
    pub fn diagnose_resources(&self) -> Vec<ResourceDiagnostic> {
        self.project.diagnose()
    }
    /// Save through the project's publication path and retain SDK undo/redo.
    /// Project paths must be absolute; unsaved relative asset sources require an
    /// explicit base and are rejected instead of being interpreted from cwd.
    pub fn save_project(
        &mut self,
        path: &Path,
        expected: Option<Version>,
    ) -> Result<SaveReceipt, SdkError> {
        if !path.is_absolute() {
            return Err(SdkError::new(
                "INVALID_PATH",
                "Project path must be absolute",
                "save_project",
            ));
        }
        let before = self.version();
        if let Some(value) = expected.filter(|value| *value != before) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "save_project");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(before));
            return Err(error);
        }
        let old_root = self.project.root();
        let old_assets: HashMap<_, _> = self
            .asset_ids()
            .iter()
            .filter_map(|id| self.asset(id).map(|asset| (id.clone(), asset)))
            .collect();
        for asset in old_assets.values() {
            validate_asset_root(&old_root, asset, "save_project")?;
        }
        let mut unverified_history_ids = HashSet::new();
        for entry in self.done.iter().chain(&self.redo) {
            for asset in entry.checkpoint.assets() {
                validate_asset_root(&entry.root, &asset, "save_project")?;
                if asset.sha256.is_empty() {
                    unverified_history_ids.insert(asset.id);
                }
            }
        }
        let result = self.project.save(path);
        if !result.status.is_ok() {
            return Err(SdkError::from_status(
                result.status,
                "save_project",
                vec![path.display().to_string()],
            ));
        }
        let new_root = self.project.root();
        let new_assets: HashMap<_, _> = self
            .asset_ids()
            .iter()
            .filter_map(|id| self.asset(id).map(|asset| (id.clone(), asset)))
            .collect();
        for entry in self.done.iter_mut().chain(&mut self.redo) {
            relocate_history_assets(entry, &old_root, &old_assets, &new_root, &new_assets);
        }
        let mut history_warnings: Vec<_> = unverified_history_ids
            .into_iter()
            .map(|id| format!("Historical resource {id} has no prior SHA-256; its old bytes were not verified"))
            .collect();
        history_warnings.sort();
        Ok(SaveReceipt {
            before,
            after: self.version(),
            manifest: self.project.manifest().to_path_buf(),
            warnings: result.warnings,
            history_warnings,
            durable: result.durable,
        })
    }

    /// Import a model3 file as a new editable document. Texture diagnostics
    /// are returned with the report, while malformed structure is rejected.
    pub fn import_model3(
        &mut self,
        path: &Path,
        expected: Option<Version>,
    ) -> Result<ImportReceipt, SdkError> {
        if !path.is_absolute() {
            return Err(SdkError::new(
                "INVALID_PATH",
                "Model3 path must be absolute",
                "import_model3",
            ));
        }
        let before = self.version();
        if let Some(value) = expected.filter(|value| *value != before) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "import_model3");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(before));
            return Err(error);
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "import_model3",
            )
        })?;
        let (project, report) = self.project.import_model3_authoring(path);
        if !project.status.is_ok() {
            return Err(SdkError::from_status(
                project.status,
                "import_model3",
                vec![path.display().to_string()],
            ));
        }
        self.reset_after_open(generation);
        Ok(ImportReceipt {
            before,
            after: self.version(),
            project,
            report: report.expect("successful import has a report"),
        })
    }

    /// Import bare MOC3 using an explicit texture slot to absolute path map.
    pub fn import_bare_moc3(
        &mut self,
        path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
        expected: Option<Version>,
    ) -> Result<ImportReceipt, SdkError> {
        if !path.is_absolute() || texture_map.values().any(|path| !path.is_absolute()) {
            return Err(SdkError::new(
                "INVALID_PATH",
                "MOC3 and texture paths must be absolute",
                "import_bare_moc3",
            ));
        }
        let before = self.version();
        if let Some(value) = expected.filter(|value| *value != before) {
            let mut error = SdkError::new(
                "STALE_VERSION",
                "Document version changed",
                "import_bare_moc3",
            );
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(before));
            return Err(error);
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "import_bare_moc3",
            )
        })?;
        let (project, report) = self.project.import_bare_moc3_authoring(path, texture_map);
        if !project.status.is_ok() {
            return Err(SdkError::from_status(
                project.status,
                "import_bare_moc3",
                vec![path.display().to_string()],
            ));
        }
        self.reset_after_open(generation);
        Ok(ImportReceipt {
            before,
            after: self.version(),
            project,
            report: report.expect("successful import has a report"),
        })
    }

    /// Import layered PSD artwork into a new project directory and replace
    /// this session only after the complete project has been published.
    pub fn import_psd(
        &mut self,
        source: &Path,
        destination: &Path,
        expected: Option<Version>,
    ) -> Result<PsdImportReceipt, SdkError> {
        if !source.is_absolute() || !destination.is_absolute() {
            return Err(SdkError::new(
                "INVALID_PATH",
                "PSD and destination paths must be absolute",
                "import_psd",
            ));
        }
        let before = self.version();
        if let Some(value) = expected.filter(|value| *value != before) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "import_psd");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(before));
            return Err(error);
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "import_psd",
            )
        })?;
        let (project, report) = self.project.import_psd_authoring(source, destination);
        if !project.status.is_ok() {
            return Err(SdkError::from_status(
                project.status,
                "import_psd",
                vec![source.display().to_string()],
            ));
        }
        self.reset_after_open(generation);
        Ok(PsdImportReceipt {
            before,
            after: self.version(),
            project,
            report: report.expect("successful PSD import has a report"),
            manifest: self.project.manifest().to_path_buf(),
        })
    }

    /// Publish an export package without changing document content or history.
    pub fn export_package(
        &self,
        destination: &Path,
        expected: Option<Version>,
    ) -> Result<ProjectResult, SdkError> {
        if !destination.is_absolute() {
            return Err(SdkError::new(
                "INVALID_PATH",
                "Export destination must be absolute",
                "export_package",
            ));
        }
        let current = self.version();
        if let Some(value) = expected.filter(|value| *value != current) {
            let mut error = SdkError::new(
                "STALE_VERSION",
                "Document version changed",
                "export_package",
            );
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(current));
            return Err(error);
        }
        let root = self.project.root();
        for id in self.asset_ids() {
            if let Some(asset) = self.asset(id) {
                validate_asset_root(&root, &asset, "export_package")?;
            }
        }
        let result = self.project.export_package(destination);
        if !result.status.is_ok() {
            return Err(SdkError::from_status(
                result.status,
                "export_package",
                vec![destination.display().to_string()],
            ));
        }
        Ok(result)
    }

    fn reset_after_open(&mut self, generation: u64) {
        self.generation = generation;
        self.done.clear();
        self.redo.clear();
        self.preview.reset();
        self.incarnations.clear();
        self.events.clear();
        self.refresh_incarnations(&HashSet::new(), &HashSet::new());
    }
}
