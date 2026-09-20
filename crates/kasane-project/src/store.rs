use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::filesystem::{self as io, FileSystem, NativeFileSystem, Publication};

use kasane_core::types::{ImageAsset, Status};
use kasane_core::Document;

use crate::codec::{decode_project, encode_project};
use crate::package::{publish_with_filesystem, PackageOptions};
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
    let source_path = asset_path(root, asset)?;

    let bytes = fs::read(&source_path)
        .map_err(|e| Status::error("PROJECT_IO", format!("{}: {}", asset.id, e)))?;

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

    Ok(data)
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
}

pub struct DocumentSession {
    store: DocumentStore,
    document: Document,
    manifest: PathBuf,
    manifest_sha256: String,
}

impl DocumentSession {
    pub fn new() -> Self {
        Self::with_filesystem(Arc::new(NativeFileSystem))
    }

    pub fn with_filesystem(filesystem: Arc<dyn FileSystem>) -> Self {
        Self {
            store: DocumentStore::with_filesystem(filesystem),
            document: Document::new(),
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        }
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
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
    }

    pub fn save(&mut self, path: &Path) -> ProjectResult {
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
            self.manifest = s.manifest;
            self.manifest_sha256 = s.manifest_sha256;
        }
        result
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
            validate: Some(Box::new(|artifact| {
                if artifact.bytes.is_empty() {
                    Err(Status::error("EMPTY_MOC3", "Encoder returned no bytes"))
                } else {
                    Ok(())
                }
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
