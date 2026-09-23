use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::filesystem::{self as io, FileSystem, NativeFileSystem, Publication};

use kasane_core::types::{ImageAsset, Status};
use kasane_core::Document;
use kasane_moc3::{import_from_bare_moc3, import_from_model3_json, ImportReport};

use crate::codec::{decode_project, encode_project};
use crate::package::{
    publish_with_filesystem, ArtifactValidation, PackageOptions, RuntimeValidation,
};
use crate::resources::{content_sha256, decode_png, AssetData};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDiagnostic {
    pub asset_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectResult {
    pub status: Status,
    pub diagnostics: Vec<ResourceDiagnostic>,
    pub warnings: Vec<String>,
    pub published: bool,
    pub durable: bool,
}

impl ProjectResult {
    pub fn ok() -> Self {
        Self {
            status: Status::ok(),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            published: true,
            durable: true,
        }
    }

    pub fn failed(code: &str, message: &str) -> Self {
        Self {
            status: Status::error(code, message),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            published: false,
            durable: false,
        }
    }

    pub fn from_status(status: Status) -> Self {
        Self {
            status,
            ..Default::default()
        }
    }

    pub fn resources_complete(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    pub document: Document,
    pub manifest: PathBuf,
    pub manifest_sha256: String,
}

pub fn project_manifest(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|ext| ext == "json") {
        path.to_path_buf()
    } else {
        path.join("project.kasane.json")
    }
}

pub fn read_project_asset(root: &Path, asset: &ImageAsset) -> Result<AssetData, Status> {
    read_project_asset_if_changed(root, asset, None).map(|data| data.unwrap())
}

/// Skip PNG decoding only when these exact bytes have already been validated.
/// Cached dimensions are checked against current metadata before skipping decode.
pub fn read_project_asset_if_changed(
    root: &Path,
    asset: &ImageAsset,
    validated_image: Option<(&str, u32, u32)>,
) -> Result<Option<AssetData>, Status> {
    let source_path = asset_path(root, asset)?;

    let bytes = fs::read(&source_path)
        .map_err(|e| Status::error("PROJECT_IO", format!("{}: {}", asset.id, e)))?;

    if let Some((hash, width, height)) = validated_image {
        if width == asset.width
            && height == asset.height
            && (asset.sha256.is_empty() || asset.sha256 == hash)
            && content_sha256(&bytes) == hash
        {
            return Ok(None);
        }
    }

    let data = decode_png(&bytes).map_err(|mut s| {
        s.message = format!("{}: {}", asset.id, s.message);
        s
    })?;

    if data.width != asset.width || data.height != asset.height {
        return Err(Status::error(
            "RESOURCE_DIMENSIONS",
            format!("{}: PNG dimensions differ from metadata", asset.id),
        ));
    }

    if !asset.sha256.is_empty() && data.sha256 != asset.sha256 {
        return Err(Status::error(
            "RESOURCE_HASH",
            format!("{}: PNG differs from saved SHA-256", asset.id),
        ));
    }

    Ok(Some(data))
}

pub struct DocumentStore {
    filesystem: Arc<dyn FileSystem>,
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::with_filesystem(Arc::new(NativeFileSystem))
    }

    pub fn with_filesystem(filesystem: Arc<dyn FileSystem>) -> Self {
        Self { filesystem }
    }

    pub fn diagnose(&self, document: &Document, root: &Path) -> Vec<ResourceDiagnostic> {
        let mut result = Vec::new();
        for id in document.asset_order() {
            let asset = document.get_asset(id).unwrap();
            if let Err(s) = read_project_asset(root, asset) {
                result.push(ResourceDiagnostic {
                    asset_id: id.clone(),
                    code: s.code,
                    message: s.message,
                });
            }
        }
        result
    }

    pub fn open(&self, path: &Path) -> (ProjectResult, Option<DocumentSnapshot>) {
        let manifest = match resolved_manifest(path) {
            Ok(path) => path,
            Err(s) => return (ProjectResult::from_status(s), None),
        };

        let bytes = match fs::read(&manifest) {
            Ok(b) => b,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", manifest.display(), e)),
                    None,
                )
            }
        };

        let sha256 = content_sha256(&bytes);
        let text = match std::str::from_utf8(&bytes) {
            Ok(t) => t,
            Err(_) => {
                return (
                    ProjectResult::failed("INVALID_PROJECT", "Manifest is not valid UTF-8"),
                    None,
                )
            }
        };

        let document = match decode_project(text) {
            Ok(d) => d,
            Err(s) => {
                return (ProjectResult::from_status(s), None);
            }
        };

        let root = manifest.parent().unwrap_or_else(|| Path::new("."));
        let diagnostics = self.diagnose(&document, root);

        let snapshot = DocumentSnapshot {
            document,
            manifest,
            manifest_sha256: sha256,
        };

        let mut res = ProjectResult::ok();
        res.diagnostics = diagnostics;
        (res, Some(snapshot))
    }

    pub fn save(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        match self.save_inner(document, source_root, path, expected_manifest_sha256) {
            Ok((result, snapshot)) => (result, Some(snapshot)),
            Err(status) => (ProjectResult::from_status(status), None),
        }
    }

    fn save_inner(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected: Option<&str>,
    ) -> Result<(ProjectResult, DocumentSnapshot), Status> {
        if document.transaction_active() {
            return Err(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel edits first",
            ));
        }
        encode_project(document)?;
        let manifest = resolved_manifest(path)?;
        let root = manifest
            .parent()
            .ok_or_else(|| Status::error("INVALID_PATH", "Missing project directory"))?;
        fs::create_dir_all(root).map_err(io::io_error)?;
        let _lock = io::lock(root)?;
        check_target(&manifest, expected)?;
        let assets_dir = root.join("assets");
        io::reject_symlink(&assets_dir)?;
        fs::create_dir_all(&assets_dir).map_err(io::io_error)?;
        let stage = io::Stage::new(root)?;
        let files = self.filesystem.as_ref();
        let mut candidate = document.clone();
        for id in document.asset_order() {
            let mut asset = document.get_asset(id).unwrap().clone();
            let data = read_project_asset(source_root, &asset)?;
            let mut name = format!("{}.png", data.sha256);
            let mut target = assets_dir.join(&name);
            let mut reuse = false;
            match fs::symlink_metadata(&target) {
                Ok(info) => {
                    reuse = info.is_file()
                        && !info.file_type().is_symlink()
                        && content_sha256(&fs::read(&target).map_err(io::io_error)?) == data.sha256;
                    if !reuse {
                        name = format!("{}-{}.png", data.sha256, io::unique_name());
                        target = assets_dir.join(&name);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(io::io_error(e)),
            }
            if !reuse {
                files
                    .write_new(&target, &data.bytes)
                    .map_err(io::io_error)?;
            }
            asset.source = format!("assets/{name}");
            asset.sha256 = data.sha256;
            let edit = candidate.replace_asset(asset);
            if !edit.status.is_ok() {
                return Err(edit.status);
            }
        }
        files.sync_directory(&assets_dir).map_err(io::io_error)?;
        files.sync_directory(root).map_err(io::io_error)?;
        let text = encode_project(&candidate)?;
        let temporary = stage.0.join("manifest.json");
        files
            .write_new(&temporary, text.as_bytes())
            .map_err(io::io_error)?;
        check_target(&manifest, expected)?;
        // Prepare committed state before publishing. A post-commit sync failure is a warning.
        candidate.mark_saved();
        let snapshot = DocumentSnapshot {
            document: candidate,
            manifest: manifest.clone(),
            manifest_sha256: content_sha256(text.as_bytes()),
        };
        files.rename(&temporary, &manifest).map_err(io::io_error)?;
        let result = publication_result(Publication::finish(files, root));
        Ok((result, snapshot))
    }

    pub fn import_model3(
        &self,
        path: &Path,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<ImportReport>,
    ) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", path.display(), e)),
                    None,
                    None,
                );
            }
        };
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let res = match import_from_model3_json(&content, base_dir) {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None, None),
        };

        let mut project_result = ProjectResult::ok();
        for d in &res.diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: d.code.clone(),
                code: d.code.clone(),
                message: d.message.clone(),
            });
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        let snapshot = DocumentSnapshot {
            document: res.document,
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        };

        (project_result, Some(snapshot), Some(res.report))
    }

    pub fn import_bare_moc3(
        &self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<ImportReport>,
    ) {
        let bytes = match fs::read(moc3_path) {
            Ok(b) => b,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", moc3_path.display(), e)),
                    None,
                    None,
                );
            }
        };
        let res = match import_from_bare_moc3(&bytes, texture_map) {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None, None),
        };

        let mut project_result = ProjectResult::ok();
        for d in &res.diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: d.code.clone(),
                code: d.code.clone(),
                message: d.message.clone(),
            });
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        let snapshot = DocumentSnapshot {
            document: res.document,
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        };

        (project_result, Some(snapshot), Some(res.report))
    }
}

pub struct DocumentSession {
    history: kasane_core::history::History,
    store: DocumentStore,
    document: Document,
    manifest: PathBuf,
    manifest_sha256: String,
}

impl DocumentSession {
    pub fn new() -> Self {
        Self::with_filesystem(Arc::new(NativeFileSystem))
    }

    /// Start an in-memory authoring session. SDK edits use a separate, complete
    /// checkpoint history; legacy delta history remains empty on this path.
    pub fn from_authoring_document(document: Document) -> Self {
        let mut session = Self::new();
        session.document = document;
        session
    }

    /// The sole SDK publication boundary. It never records a legacy delta.
    pub fn publish_authoring_candidate(
        &mut self,
        candidate: Document,
        kind: kasane_core::ChangeKind,
    ) -> Result<bool, Status> {
        if self.history.active() {
            return Err(Status::error(
                "ACTION_ACTIVE",
                "Finish the legacy action first",
            ));
        }
        let changed = self.document.publish_candidate(candidate, kind)?;
        if changed {
            self.history
                .clear(self.document.revision(), Some("HISTORY_EXTERNAL_EDIT"));
        }
        Ok(changed)
    }

    /// Restore a complete SDK checkpoint, retaining manifest and saved baseline.
    pub fn swap_authoring_checkpoint(
        &mut self,
        checkpoint: &mut kasane_core::document::DocumentCheckpoint,
    ) -> Result<(), Status> {
        self.document.exchange_checkpoint(checkpoint)?;
        self.history
            .clear(self.document.revision(), Some("HISTORY_EXTERNAL_EDIT"));
        Ok(())
    }

    pub fn with_filesystem(filesystem: Arc<dyn FileSystem>) -> Self {
        Self {
            history: kasane_core::history::History::default(),
            store: DocumentStore::with_filesystem(filesystem),
            document: Document::new(),
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        }
    }

    pub fn history(&self) -> &kasane_core::history::History {
        &self.history
    }

    pub fn record_edit(&mut self, edit: &kasane_core::EditResult) {
        self.history.record(&mut self.document, edit);
    }

    /// Engine-independent entry point for a single recorded edit.
    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Document) -> kasane_core::EditResult,
    ) -> kasane_core::EditResult {
        let result = operation(&mut self.document);
        self.record_edit(&result);
        result
    }

    pub fn begin_action(&mut self, label: String) -> Status {
        self.history.begin(&self.document, label)
    }
    pub fn end_action(&mut self) -> Status {
        self.history.end(&self.document)
    }
    pub fn cancel_action(&mut self) -> kasane_core::EditResult {
        self.history.cancel(&mut self.document)
    }
    pub fn undo(&mut self) -> kasane_core::EditResult {
        self.history.undo(&mut self.document)
    }
    pub fn redo(&mut self) -> kasane_core::EditResult {
        self.history.redo(&mut self.document)
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    pub fn root(&self) -> PathBuf {
        self.manifest
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    }

    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    pub fn open(&mut self, path: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let (result, snapshot) = self.store.open(path);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.document = s.document;
            self.history.clear(self.document.revision(), None);
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
    }

    pub fn save(&mut self, path: &Path) -> ProjectResult {
        if self.history.active() {
            return ProjectResult::failed(
                "ACTION_ACTIVE",
                "End or cancel the active action before saving",
            );
        }
        let before_revision = self.document.revision();
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let target = match resolved_manifest(path) {
            Ok(path) => path,
            Err(s) => return ProjectResult::from_status(s),
        };
        let expected = if self.manifest == target {
            Some(self.manifest_sha256.as_str())
        } else {
            None
        };
        let source_root = self.root();
        let (result, snapshot) = self
            .store
            .save(&self.document, &source_root, path, expected);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.document = s.document;
            self.history
                .saved(before_revision, self.document.revision());
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
    }

    pub fn import_model3(&mut self, path: &Path) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel edits first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_model3(path);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.document = s.document;
            self.history.clear(self.document.revision(), None);
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        (result, report)
    }

    pub fn import_bare_moc3(
        &mut self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.document.transaction_active() {
            return (
                ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel edits first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_bare_moc3(moc3_path, texture_map);
        if result.status.is_ok() {
            let s = snapshot.unwrap();
            self.document = s.document;
            self.history.clear(self.document.revision(), None);
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        (result, report)
    }

    pub fn diagnose(&self) -> Vec<ResourceDiagnostic> {
        self.store.diagnose(&self.document, &self.root())
    }

    pub fn read_asset(&self, asset_id: &str) -> Result<AssetData, Status> {
        let asset = self
            .document
            .get_asset(asset_id)
            .ok_or_else(|| Status::error("NOT_FOUND", asset_id))?;
        read_project_asset(&self.root(), asset)
    }

    pub fn read_asset_if_changed(
        &self,
        asset_id: &str,
        validated_image: Option<(&str, u32, u32)>,
    ) -> Result<Option<AssetData>, Status> {
        let asset = self
            .document
            .get_asset(asset_id)
            .ok_or_else(|| Status::error("NOT_FOUND", asset_id))?;
        read_project_asset_if_changed(&self.root(), asset, validated_image)
    }

    pub fn relocate_asset(&mut self, id: &str, path: &Path) -> kasane_core::types::EditResult {
        if self.document.transaction_active() {
            return kasane_core::types::EditResult {
                status: Status::error(
                    "TRANSACTION_ACTIVE",
                    "Commit or cancel the transaction first",
                ),
                ..Default::default()
            };
        }
        let path = match io::local_path(path) {
            Ok(path) => path,
            Err(status) => {
                return kasane_core::types::EditResult {
                    status,
                    ..Default::default()
                }
            }
        };
        let existing = match self.document.get_asset(id) {
            Some(a) => a.clone(),
            None => {
                return kasane_core::types::EditResult {
                    status: Status::error("MISSING_ASSET", id),
                    ..Default::default()
                }
            }
        };
        let mut asset = existing;
        asset.source = path.to_string_lossy().to_string();
        let data = match read_project_asset(Path::new(""), &asset) {
            Ok(d) => d,
            Err(s) => {
                return kasane_core::types::EditResult {
                    status: s,
                    ..Default::default()
                }
            }
        };
        asset.sha256 = data.sha256;
        self.document.replace_asset(asset)
    }

    pub fn replace_asset(&mut self, id: &str, path: &Path) -> kasane_core::types::EditResult {
        if self.document.transaction_active() {
            return kasane_core::types::EditResult {
                status: Status::error(
                    "TRANSACTION_ACTIVE",
                    "Commit or cancel the transaction first",
                ),
                ..Default::default()
            };
        }
        let path = match io::local_path(path) {
            Ok(path) => path,
            Err(status) => {
                return kasane_core::types::EditResult {
                    status,
                    ..Default::default()
                }
            }
        };
        let existing = match self.document.get_asset(id) {
            Some(a) => a.clone(),
            None => {
                return kasane_core::types::EditResult {
                    status: Status::error("MISSING_ASSET", id),
                    ..Default::default()
                }
            }
        };
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                return kasane_core::types::EditResult {
                    status: Status::error("RESOURCE_IO", e.to_string()),
                    ..Default::default()
                }
            }
        };
        let data = match decode_png(&bytes) {
            Ok(d) => d,
            Err(s) => {
                return kasane_core::types::EditResult {
                    status: s,
                    ..Default::default()
                }
            }
        };
        let mut asset = existing;
        asset.source = path.to_string_lossy().to_string();
        asset.sha256 = data.sha256;
        asset.width = data.width;
        asset.height = data.height;
        self.document.replace_asset(asset)
    }

    pub fn export_package(&self, destination: &Path) -> ProjectResult {
        if self.document.transaction_active() {
            return ProjectResult::failed("TRANSACTION_ACTIVE", "Commit or cancel first");
        }
        let options = PackageOptions {
            asset_root: self.root(),
            destination: destination.to_path_buf(),
            validate: Some(Box::new(|_artifact| {
                Ok(ArtifactValidation::structural(
                    if kasane_moc3::HAS_CORE_VALIDATION {
                        RuntimeValidation::Passed
                    } else {
                        RuntimeValidation::Unavailable
                    },
                ))
            })),
        };
        match publish_with_filesystem(&self.document, &options, self.store.filesystem.as_ref()) {
            Ok(publication) => publication_result(publication),
            Err(status) => ProjectResult::from_status(status),
        }
    }
}

impl Default for DocumentSession {
    fn default() -> Self {
        Self::new()
    }
}

fn resolved_manifest(path: &Path) -> Result<PathBuf, Status> {
    let requested = project_manifest(path);
    io::reject_symlink(&requested)?;
    io::local_path(&requested)
}

pub(crate) fn asset_path(root: &Path, asset: &ImageAsset) -> Result<PathBuf, Status> {
    let path = Path::new(&asset.source);
    if path.is_absolute() {
        return io::local_path(path);
    }
    if !crate::resources::is_valid_asset_path(&asset.source) {
        return Err(Status::error("INVALID_PATH", &asset.source));
    }
    let root = io::local_path(root)?;
    let resolved = io::local_path(&root.join(path))?;
    if !resolved.starts_with(&root) {
        return Err(Status::error(
            "INVALID_PATH",
            "Asset escapes the project root",
        ));
    }
    Ok(resolved)
}

fn check_target(manifest: &Path, expected: Option<&str>) -> Result<(), Status> {
    io::reject_symlink(manifest)?;
    match (fs::symlink_metadata(manifest), expected) {
        (Ok(_), None) => Err(Status::error(
            "DESTINATION_EXISTS",
            "Reopen the existing project before saving",
        )),
        (Ok(_), Some(hash)) => {
            let bytes = fs::read(manifest).map_err(io::io_error)?;
            if content_sha256(&bytes) != hash {
                return Err(Status::error(
                    "PROJECT_CONFLICT",
                    "On-disk manifest changed; reopen or save to a new path",
                ));
            }
            Ok(())
        }
        (Err(e), baseline) if e.kind() == std::io::ErrorKind::NotFound => {
            if baseline.is_some() {
                Err(Status::error(
                    "PROJECT_CONFLICT",
                    "Expected on-disk manifest not found",
                ))
            } else {
                Ok(())
            }
        }
        (Err(e), _) => Err(io::io_error(e)),
    }
}

fn publication_result(publication: Publication) -> ProjectResult {
    ProjectResult {
        durable: publication.durable(),
        warnings: publication.warnings,
        ..ProjectResult::ok()
    }
}
