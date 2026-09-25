use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::filesystem::{self as io, FileSystem, NativeFileSystem, Publication};

use kasane_core::document::{valid_attachment_path, PackageAttachment};
use kasane_core::types::{ImageAsset, Status};
use kasane_core::Document;
use kasane_moc3::{import_from_bare_moc3, import_from_model3_json, ImportReport};
use kasane_psd::{import_psd, ImportReport as PsdImportReport};
use sha2::{Digest, Sha256};

use crate::codec::{decode_project, encode_project};
#[cfg(feature = "binary-prototype")]
use crate::codec::{decode_project_cbor, encode_project_cbor};
use crate::package::{
    publish_with_filesystem, PackageOptions, PackageValidation, RuntimeValidation,
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

#[derive(Clone, Copy)]
enum ProjectEncoding {
    Json,
    #[cfg(feature = "binary-prototype")]
    Cbor,
}

impl ProjectEncoding {
    fn manifest(self, path: &Path) -> PathBuf {
        match self {
            Self::Json => project_manifest(path),
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => {
                if path.extension().is_some_and(|ext| ext == "cbor") {
                    path.to_path_buf()
                } else {
                    path.join("project.kasane.cbor")
                }
            }
        }
    }

    fn decode(self, bytes: &[u8]) -> Result<Document, Status> {
        match self {
            Self::Json => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| Status::error("INVALID_PROJECT", "Manifest is not valid UTF-8"))?;
                decode_project(text)
            }
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => decode_project_cbor(bytes),
        }
    }

    fn encode(self, document: &Document) -> Result<Vec<u8>, Status> {
        match self {
            Self::Json => encode_project(document).map(String::into_bytes),
            #[cfg(feature = "binary-prototype")]
            Self::Cbor => encode_project_cbor(document),
        }
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
    verified_pngs: Mutex<HashMap<String, (u32, u32)>>,
}

struct VerifiedAsset {
    bytes: Vec<u8>,
    sha256: String,
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
        Self {
            filesystem,
            verified_pngs: Mutex::new(HashMap::new()),
        }
    }

    /// A cached entry proves that these exact PNG bytes decoded successfully.
    /// Every call still reads and hashes current bytes, so external edits are detected.
    fn read_verified_asset(
        &self,
        root: &Path,
        asset: &ImageAsset,
    ) -> Result<VerifiedAsset, Status> {
        let source_path = asset_path(root, asset)?;
        let bytes = fs::read(&source_path)
            .map_err(|e| Status::error("PROJECT_IO", format!("{}: {}", asset.id, e)))?;
        let sha256 = content_sha256(&bytes);
        let cached_dimensions = self
            .verified_pngs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&sha256)
            .copied();
        let (width, height) = match cached_dimensions {
            Some(dimensions) => dimensions,
            None => {
                let image = kasane_core::image::decode_png(&bytes).map_err(|mut status| {
                    status.message = format!("{}: {}", asset.id, status.message);
                    status
                })?;
                let dimensions = (image.width, image.height);
                self.verified_pngs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(sha256.clone(), dimensions);
                dimensions
            }
        };
        if width != asset.width || height != asset.height {
            return Err(Status::error(
                "RESOURCE_DIMENSIONS",
                format!("{}: PNG dimensions differ from metadata", asset.id),
            ));
        }
        if !asset.sha256.is_empty() && sha256 != asset.sha256 {
            return Err(Status::error(
                "RESOURCE_HASH",
                format!("{}: PNG differs from saved SHA-256", asset.id),
            ));
        }
        Ok(VerifiedAsset { bytes, sha256 })
    }

    fn remember_imported_assets(&self, document: &Document) {
        let mut verified = self
            .verified_pngs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in document.asset_order() {
            let asset = document.get_asset(id).unwrap();
            // MOC3 import assigns a hash only after successfully decoding the PNG.
            if !asset.sha256.is_empty() {
                verified.insert(asset.sha256.clone(), (asset.width, asset.height));
            }
        }
    }

    pub fn diagnose(&self, document: &Document, root: &Path) -> Vec<ResourceDiagnostic> {
        let mut result = Vec::new();
        for id in document.asset_order() {
            let asset = document.get_asset(id).unwrap();
            if let Err(s) = self.read_verified_asset(root, asset) {
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
        self.open_inner(path, ProjectEncoding::Json)
    }

    #[cfg(feature = "binary-prototype")]
    pub fn open_cbor(&self, path: &Path) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.open_inner(path, ProjectEncoding::Cbor)
    }

    fn open_inner(
        &self,
        path: &Path,
        encoding: ProjectEncoding,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        let manifest = match resolved_manifest_for(path, encoding) {
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
        let document = match encoding.decode(&bytes) {
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
        self.save_with_encoding(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            ProjectEncoding::Json,
        )
    }

    #[cfg(feature = "binary-prototype")]
    pub fn save_cbor(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        self.save_with_encoding(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            ProjectEncoding::Cbor,
        )
    }

    fn save_with_encoding(
        &self,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected_manifest_sha256: Option<&str>,
        encoding: ProjectEncoding,
    ) -> (ProjectResult, Option<DocumentSnapshot>) {
        match self.save_inner(
            document,
            source_root,
            path,
            expected_manifest_sha256,
            encoding,
        ) {
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
        encoding: ProjectEncoding,
    ) -> Result<(ProjectResult, DocumentSnapshot), Status> {
        if document.transaction_active() {
            return Err(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel edits first",
            ));
        }
        if !document.initialized() {
            return Err(Status::error(
                "NOT_INITIALIZED",
                "Initialize Document first",
            ));
        }
        let manifest = resolved_manifest_for(path, encoding)?;
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
        let mut candidate = document.fork_candidate();
        for id in document.asset_order() {
            let mut asset = document.get_asset(id).unwrap().clone();
            let data = self.read_verified_asset(source_root, &asset)?;
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
        let manifest_bytes = encoding.encode(&candidate)?;
        let temporary = stage.0.join("manifest.json");
        files
            .write_new(&temporary, &manifest_bytes)
            .map_err(io::io_error)?;
        check_target(&manifest, expected)?;
        let manifest_sha256 = content_sha256(&manifest_bytes);
        drop(manifest_bytes);
        // Prepare committed state before publishing. A post-commit sync failure is a warning.
        candidate.mark_saved();
        let snapshot = DocumentSnapshot {
            document: candidate,
            manifest: manifest.clone(),
            manifest_sha256,
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
        let mut res = match import_from_model3_json(&content, base_dir) {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None, None),
        };

        let model3 = match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(root) => root,
            Err(error) => {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", &error.to_string()),
                    None,
                    None,
                );
            }
        };
        let file_refs = &model3["FileReferences"];
        let cdi_ref = file_refs.get("DisplayInfo").cloned();
        let cdi_diagnostics = if let Some(reference) = cdi_ref {
            let Some(relative) = reference.as_str() else {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must be a string",
                    ),
                    None,
                    None,
                );
            };
            let path = Path::new(relative);
            if relative.is_empty()
                || path.components().any(|component| {
                    !matches!(
                        component,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must stay inside the model directory",
                    ),
                    None,
                    None,
                );
            }
            let source = base_dir.join(path);
            let source = match source.canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            "CDI_IO_ERROR",
                            &format!("{}: {error}", source.display()),
                        ),
                        None,
                        None,
                    );
                }
            };
            if !base_dir
                .canonicalize()
                .is_ok_and(|base| source.starts_with(base))
            {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must resolve inside the model directory",
                    ),
                    None,
                    None,
                );
            }
            let text = match fs::read_to_string(&source) {
                Ok(text) => text,
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            "CDI_IO_ERROR",
                            &format!("{}: {error}", source.display()),
                        ),
                        None,
                        None,
                    );
                }
            };
            match crate::import_cdi3(&res.document, &text) {
                Ok(imported) => {
                    res.document = imported.candidate;
                    res.report
                        .unimported_attachments
                        .retain(|item| !item.starts_with("DisplayInfo:"));
                    imported.diagnostics
                }
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            &error.code,
                            &format!("{}: {}", error.path, error.message),
                        ),
                        None,
                        None,
                    );
                }
            }
        } else {
            Vec::new()
        };

        let pose_diagnostics = if let Some(reference) = file_refs.get("Pose") {
            let Some(relative) = reference.as_str() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Pose path must be a string"),
                    None,
                    None,
                );
            };
            let text = match read_model_attachment(base_dir, relative, "Pose") {
                Ok(text) => text,
                Err(status) => return (ProjectResult::from_status(status), None, None),
            };
            let id = stable_pose_id(&res.document);
            match crate::import_pose3(&res.document, &id, &text) {
                Ok(imported) => {
                    res.document = imported.candidate;
                    res.report
                        .unimported_attachments
                        .retain(|item| !item.starts_with("Pose:"));
                    imported.diagnostics
                }
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            &error.code,
                            &format!("Pose {}: {}", error.path, error.message),
                        ),
                        None,
                        None,
                    )
                }
            }
        } else {
            Vec::new()
        };

        let physics_diagnostics = if let Some(reference) = file_refs.get("Physics") {
            let Some(relative) = reference.as_str() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Physics path must be a string"),
                    None,
                    None,
                );
            };
            if !valid_attachment_path(relative) {
                return (
                    ProjectResult::failed("INVALID_ATTACHMENT_PATH", relative),
                    None,
                    None,
                );
            }
            if !base_dir.join(relative).is_file() {
                vec![crate::PhysicsDiagnostic {
                    code: "MISSING_PHYSICS_ATTACHMENT".into(),
                    path: "$.FileReferences.Physics".into(),
                    message: format!("{relative} is absent"),
                }]
            } else {
                let text = match read_model_attachment(base_dir, relative, "Physics") {
                    Ok(text) => text,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                let id = stable_attachment_id(&res.document, b"physics");
                match crate::import_physics3(&res.document, &id, &text) {
                    Ok(imported) => {
                        res.document = imported.candidate;
                        res.report
                            .unimported_attachments
                            .retain(|item| !item.starts_with("Physics:"));
                        imported.diagnostics
                    }
                    Err(error) => {
                        return (
                            ProjectResult::failed(
                                &error.code,
                                &format!("Physics {}: {}", error.path, error.message),
                            ),
                            None,
                            None,
                        )
                    }
                }
            }
        } else {
            Vec::new()
        };

        let mut expression_diagnostics = Vec::new();
        if let Some(references) = file_refs.get("Expressions") {
            let Some(references) = references.as_array() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Expressions must be an array"),
                    None,
                    None,
                );
            };
            for (index, registration) in references.iter().enumerate() {
                let Some(name) = registration.get("Name").and_then(|value| value.as_str()) else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Expressions[{index}].Name must be a string"),
                        ),
                        None,
                        None,
                    );
                };
                let Some(relative) = registration.get("File").and_then(|value| value.as_str())
                else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Expressions[{index}].File must be a string"),
                        ),
                        None,
                        None,
                    );
                };
                let text = match read_model_attachment(base_dir, relative, "Expression") {
                    Ok(text) => text,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                let id = stable_expression_id(&res.document, index, name);
                match crate::import_expression3(&res.document, &id, name, &text) {
                    Ok(imported) => {
                        res.document = imported.candidate;
                        expression_diagnostics.extend(imported.diagnostics.into_iter().map(
                            |diagnostic| ResourceDiagnostic {
                                asset_id: name.into(),
                                code: diagnostic.code,
                                message: format!("{}: {}", diagnostic.path, diagnostic.message),
                            },
                        ));
                    }
                    Err(error) => {
                        return (
                            ProjectResult::failed(
                                &error.code,
                                &format!("Expressions[{index}] {}: {}", error.path, error.message),
                            ),
                            None,
                            None,
                        );
                    }
                }
            }
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("Expressions:"));
        }

        let mut motion_diagnostics = Vec::new();
        if let Some(references) = file_refs.get("Motions") {
            let Some(groups) = references.as_object() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Motions must be an object"),
                    None,
                    None,
                );
            };
            let mut imported_paths: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut motion_groups = Vec::new();
            for (group_name, entries) in groups {
                let Some(entries) = entries.as_array() else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Motions.{group_name} must be an array"),
                        ),
                        None,
                        None,
                    );
                };
                let mut registrations = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let Some(object) = entry.as_object() else {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!("Motions.{group_name}[{index}] must be an object"),
                            ),
                            None,
                            None,
                        );
                    };
                    let Some(relative) = object.get("File").and_then(|value| value.as_str()) else {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!("Motions.{group_name}[{index}].File must be a string"),
                            ),
                            None,
                            None,
                        );
                    };
                    let valid_path = !relative.is_empty()
                        && std::path::Path::new(relative)
                            .components()
                            .all(|component| {
                                matches!(
                                    component,
                                    std::path::Component::Normal(_) | std::path::Component::CurDir
                                )
                            });
                    if !valid_path {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!(
                                    "Motions.{group_name}[{index}].File escapes model directory"
                                ),
                            ),
                            None,
                            None,
                        );
                    }
                    if !base_dir.join(relative).is_file() {
                        motion_diagnostics.push(ResourceDiagnostic {
                            asset_id: relative.into(),
                            code: "MISSING_MOTION_ATTACHMENT".into(),
                            message: format!("Motions.{group_name}[{index}].File is absent"),
                        });
                        continue;
                    }
                    let fade = |key: &str| -> Result<Option<f32>, String> {
                        match object.get(key) {
                            None => Ok(None),
                            Some(value) => value.as_f64().map(|number| number as f32).filter(|number| number.is_finite() && *number >= 0.0).map(Some)
                                .ok_or_else(|| format!("Motions.{group_name}[{index}].{key} must be nonnegative finite number")),
                        }
                    };
                    let fade_in = match fade("FadeInTime") {
                        Ok(value) => value,
                        Err(message) => {
                            return (
                                ProjectResult::failed("INVALID_MODEL3_JSON", &message),
                                None,
                                None,
                            )
                        }
                    };
                    let fade_out = match fade("FadeOutTime") {
                        Ok(value) => value,
                        Err(message) => {
                            return (
                                ProjectResult::failed("INVALID_MODEL3_JSON", &message),
                                None,
                                None,
                            )
                        }
                    };
                    let sound = match object.get("Sound") {
                        None => None,
                        Some(value) => match value.as_str() {
                            Some(value) => Some(value.to_string()),
                            None => {
                                return (
                                    ProjectResult::failed(
                                        "INVALID_MODEL3_JSON",
                                        &format!(
                                            "Motions.{group_name}[{index}].Sound must be a string"
                                        ),
                                    ),
                                    None,
                                    None,
                                )
                            }
                        },
                    };
                    let clip_id = if let Some(id) = imported_paths.get(relative) {
                        id.clone()
                    } else {
                        let text = match read_model_attachment(base_dir, relative, "Motion") {
                            Ok(text) => text,
                            Err(status) => return (ProjectResult::from_status(status), None, None),
                        };
                        let id = stable_motion_id(&res.document, relative);
                        let source_file = relative.rsplit('/').next().unwrap_or(relative);
                        let mut name = source_file
                            .strip_suffix(".motion3.json")
                            .filter(|stem| !stem.is_empty())
                            .unwrap_or(source_file)
                            .to_string();
                        if name.is_empty() {
                            name = format!("{group_name}_{index}");
                        }
                        let base_name = name.clone();
                        let mut suffix = 0;
                        while res.document.motion_order().iter().any(|existing| {
                            res.document
                                .get_motion(existing)
                                .is_some_and(|clip| clip.name == name)
                        }) {
                            suffix += 1;
                            name = if suffix == 1 {
                                format!("{base_name}-{}", &id[..8])
                            } else {
                                format!("{base_name}-{}-{suffix}", &id[..8])
                            };
                        }
                        match crate::import_motion3(&res.document, &id, &name, &text) {
                            Ok(imported) => {
                                res.document = imported.candidate;
                                motion_diagnostics.extend(imported.diagnostics.into_iter().map(
                                    |diagnostic| ResourceDiagnostic {
                                        asset_id: relative.into(),
                                        code: diagnostic.code,
                                        message: format!(
                                            "{}: {}",
                                            diagnostic.path, diagnostic.message
                                        ),
                                    },
                                ));
                            }
                            Err(error) => {
                                return (
                                    ProjectResult::failed(
                                        &error.code,
                                        &format!(
                                            "Motions.{group_name}[{index}] {}: {}",
                                            error.path, error.message
                                        ),
                                    ),
                                    None,
                                    None,
                                )
                            }
                        }
                        imported_paths.insert(relative.to_string(), id.clone());
                        id
                    };
                    let extensions = object
                        .iter()
                        .filter(|(key, _)| {
                            !matches!(
                                key.as_str(),
                                "File" | "FadeInTime" | "FadeOutTime" | "Sound"
                            )
                        })
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    registrations.push(kasane_core::document::MotionRegistration {
                        clip_id,
                        fade_in,
                        fade_out,
                        sound,
                        extensions,
                    });
                }
                motion_groups.push(kasane_core::document::MotionGroup {
                    name: group_name.clone(),
                    entries: registrations,
                });
            }
            let result = res.document.set_motion_groups(motion_groups);
            if !result.status.is_ok() {
                return (ProjectResult::from_status(result.status), None, None);
            }
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("Motions:"));
        }

        let settings = match crate::model3::import_settings(&res.document, &model3) {
            Ok(settings) => settings,
            Err(status) => return (ProjectResult::from_status(status), None, None),
        };
        let result = res.document.set_model3_settings(settings);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        res.report.unimported_attachments.retain(|item| {
            !item.starts_with("Groups:")
                && !item.starts_with("Layout:")
                && !item.starts_with("HitAreas:")
        });
        let mut managed_diagnostics = Vec::new();
        let mut managed = Vec::new();
        let mut paths = std::collections::HashSet::new();
        let user_data_value = file_refs.get("UserData");
        if user_data_value.is_some_and(|value| !value.is_string()) {
            return (
                ProjectResult::failed("INVALID_MODEL3_JSON", "UserData must be a string"),
                None,
                None,
            );
        }
        let referenced_files = user_data_value
            .and_then(|value| value.as_str())
            .map(|path| ("UserData", path.to_string()))
            .into_iter()
            .chain(
                res.document
                    .motion_groups()
                    .iter()
                    .flat_map(|group| group.entries.iter())
                    .filter_map(|entry| entry.sound.as_ref().map(|path| ("Sound", path.clone()))),
            );
        for (kind, path) in referenced_files {
            if !valid_attachment_path(&path) {
                return (
                    ProjectResult::failed("INVALID_ATTACHMENT_PATH", &path),
                    None,
                    None,
                );
            }
            if !paths.insert(path.clone()) {
                continue;
            }
            if !base_dir.join(&path).is_file() {
                managed_diagnostics.push(ResourceDiagnostic {
                    asset_id: path.clone(),
                    code: format!("MISSING_{}_ATTACHMENT", kind.to_uppercase()),
                    message: format!("{kind} {path} is absent"),
                });
                continue;
            }
            let bytes = match read_model_attachment_bytes(base_dir, &path, kind) {
                Ok(bytes) => bytes,
                Err(status) => return (ProjectResult::from_status(status), None, None),
            };
            if kind == "UserData" && serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                return (
                    ProjectResult::failed("INVALID_USERDATA_JSON", &path),
                    None,
                    None,
                );
            }
            managed.push(PackageAttachment { path, bytes });
        }
        let result = res.document.set_package_attachments(managed);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        if !managed_diagnostics
            .iter()
            .any(|item| item.code == "MISSING_USERDATA_ATTACHMENT")
        {
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("UserData:"));
        }
        let mut project_result = ProjectResult::ok();
        for d in &res.diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: d.code.clone(),
                code: d.code.clone(),
                message: d.message.clone(),
            });
        }
        for diagnostic in cdi_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "DisplayInfo".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        for diagnostic in pose_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "Pose".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        for diagnostic in physics_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "Physics".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        project_result.diagnostics.extend(expression_diagnostics);
        project_result.diagnostics.extend(motion_diagnostics);
        project_result.diagnostics.extend(managed_diagnostics);
        let mut incomplete = res.report.unimported_attachments.clone();
        incomplete.extend(
            project_result
                .diagnostics
                .iter()
                .filter(|item| {
                    matches!(
                        item.code.as_str(),
                        "MISSING_MOTION_ATTACHMENT"
                            | "MISSING_SOUND_ATTACHMENT"
                            | "MISSING_USERDATA_ATTACHMENT"
                    )
                })
                .map(|item| format!("{}: {}", item.code, item.asset_id)),
        );
        let result = res.document.set_missing_attachments(incomplete);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        self.remember_imported_assets(&res.document);
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

        self.remember_imported_assets(&res.document);
        let snapshot = DocumentSnapshot {
            document: res.document,
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        };

        (project_result, Some(snapshot), Some(res.report))
    }

    /// Publish a new project from a PSD. The destination becomes visible only
    /// after its manifest and every extracted PNG have been written and synced.
    pub fn import_psd(
        &self,
        source: &Path,
        destination: &Path,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<PsdImportReport>,
    ) {
        match self.import_psd_inner(source, destination) {
            Ok((result, snapshot, report)) => (result, Some(snapshot), Some(report)),
            Err(status) => (ProjectResult::from_status(status), None, None),
        }
    }

    fn import_psd_inner(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(ProjectResult, DocumentSnapshot, PsdImportReport), Status> {
        let size = fs::metadata(source).map_err(io::io_error)?.len();
        if size > 512 * 1024 * 1024 {
            return Err(Status::error("PSD_LIMIT", "PSD exceeds 512 MiB"));
        }
        let bytes = fs::read(source).map_err(io::io_error)?;
        let bundle =
            import_psd(&bytes).map_err(|error| Status::error(error.code, error.message))?;
        if let Some(issue) = bundle.document.validate_structure().into_iter().next() {
            return Err(issue.status);
        }
        let manifest_text = encode_project(&bundle.document)?;
        io::reject_symlink(destination)?;
        let destination = io::local_path(destination)?;
        let parent = destination
            .parent()
            .filter(|_| destination.file_name().is_some())
            .ok_or_else(|| {
                Status::error(
                    "INVALID_DESTINATION",
                    "Cannot create a project at a filesystem root",
                )
            })?;
        fs::create_dir_all(parent).map_err(io::io_error)?;
        let _lock = io::lock(parent)?;
        io::reject_symlink(&destination)?;
        if destination.exists() {
            return Err(Status::error(
                "DESTINATION_EXISTS",
                "PSD project destination already exists",
            ));
        }
        let stage = io::Stage::new(parent)?;
        let assets_dir = stage.0.join("assets");
        fs::create_dir(&assets_dir).map_err(io::io_error)?;
        let files = self.filesystem.as_ref();
        for asset in &bundle.assets {
            files
                .write_new(&stage.0.join(&asset.source), &asset.bytes)
                .map_err(io::io_error)?;
        }
        files
            .write_new(
                &stage.0.join("project.kasane.json"),
                manifest_text.as_bytes(),
            )
            .map_err(io::io_error)?;
        files.sync_directory(&assets_dir).map_err(io::io_error)?;
        files.sync_directory(&stage.0).map_err(io::io_error)?;
        io::reject_symlink(&destination)?;
        if destination.exists() {
            return Err(Status::error(
                "DESTINATION_EXISTS",
                "PSD project destination already exists",
            ));
        }
        let manifest = destination.join("project.kasane.json");
        let mut document = bundle.document;
        document.mark_saved();
        let snapshot = DocumentSnapshot {
            document,
            manifest,
            manifest_sha256: content_sha256(manifest_text.as_bytes()),
        };
        files.rename(&stage.0, &destination).map_err(io::io_error)?;
        let mut result = publication_result(Publication::finish(files, parent));
        result
            .warnings
            .extend(bundle.report.warnings.iter().cloned());
        Ok((result, snapshot, bundle.report))
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
        session.reset_authoring_document(document);
        session
    }

    pub fn from_authoring_document_with_filesystem(
        document: Document,
        filesystem: Arc<dyn FileSystem>,
    ) -> Self {
        let mut session = Self::with_filesystem(filesystem);
        session.reset_authoring_document(document);
        session
    }

    /// Replace an SDK document while retaining the configured publication backend.
    pub fn reset_authoring_document(&mut self, document: Document) {
        self.history.clear(document.revision(), None);
        self.document = document;
        self.manifest = PathBuf::new();
        self.manifest_sha256.clear();
    }

    /// The sole SDK publication boundary. It never records a legacy delta.
    pub fn publish_authoring_candidate(
        &mut self,
        candidate: Document,
        kind: kasane_core::ChangeKind,
        identity_changed: bool,
    ) -> Result<bool, Status> {
        if self.history.active() {
            return Err(Status::error(
                "ACTION_ACTIVE",
                "Finish the legacy action first",
            ));
        }
        let changed = self
            .document
            .publish_candidate(candidate, kind, identity_changed)?;
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

    /// Atomically open a document for the SDK, including final graph validation.
    /// Resource diagnostics are returned separately and do not reject an editable project.
    pub fn open_authoring(&mut self, path: &Path) -> ProjectResult {
        if self.history.active() || self.document.transaction_active() {
            return ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first");
        }
        let (result, snapshot) = self.store.open(path);
        if !result.status.is_ok() {
            return result;
        }
        let snapshot = snapshot.expect("successful open has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return ProjectResult::from_status(issue.status);
        }
        self.document = snapshot.document;
        self.history.clear(self.document.revision(), None);
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
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

    /// Import an external model for SDK editing, publishing only a structurally
    /// valid decoded document. Resource diagnostics remain non-fatal.
    pub fn import_model3_authoring(
        &mut self,
        path: &Path,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.history.active() || self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_model3(path);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful import has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return (ProjectResult::from_status(issue.status), None);
        }
        self.document = snapshot.document;
        self.history.clear(self.document.revision(), None);
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
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

    /// Import bare MOC3 for SDK editing with the same atomic validation gate.
    pub fn import_bare_moc3_authoring(
        &mut self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (ProjectResult, Option<ImportReport>) {
        if self.history.active() || self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_bare_moc3(moc3_path, texture_map);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful import has a snapshot");
        if let Some(issue) = snapshot.document.validate_structure().into_iter().next() {
            return (ProjectResult::from_status(issue.status), None);
        }
        self.document = snapshot.document;
        self.history.clear(self.document.revision(), None);
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
        (result, report)
    }

    /// Import a PSD into a newly published project before replacing the
    /// current authoring document. Failures leave the session intact.
    pub fn import_psd_authoring(
        &mut self,
        source: &Path,
        destination: &Path,
    ) -> (ProjectResult, Option<PsdImportReport>) {
        if self.history.active() || self.document.transaction_active() {
            return (
                ProjectResult::failed("EDIT_ACTIVE", "Finish the active edit first"),
                None,
            );
        }
        let (result, snapshot, report) = self.store.import_psd(source, destination);
        if !result.status.is_ok() {
            return (result, None);
        }
        let snapshot = snapshot.expect("successful PSD import has a snapshot");
        self.document = snapshot.document;
        self.history.clear(self.document.revision(), None);
        self.manifest = snapshot.manifest;
        self.manifest_sha256 = snapshot.manifest_sha256;
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
                Ok(PackageValidation::structural(
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

fn read_model_attachment(base_dir: &Path, relative: &str, kind: &str) -> Result<String, Status> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(Status::error(
            "INVALID_MODEL3_JSON",
            format!("{kind} path must stay inside the model directory"),
        ));
    }
    let source = base_dir.join(path);
    let source = source.canonicalize().map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })?;
    if !base_dir
        .canonicalize()
        .is_ok_and(|base| source.starts_with(base))
    {
        return Err(Status::error(
            "INVALID_MODEL3_JSON",
            format!("{kind} path resolves outside the model directory"),
        ));
    }
    fs::read_to_string(&source).map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })
}

fn read_model_attachment_bytes(
    base_dir: &Path,
    relative: &str,
    kind: &str,
) -> Result<Vec<u8>, Status> {
    if !valid_attachment_path(relative) {
        return Err(Status::error("INVALID_ATTACHMENT_PATH", relative));
    }
    let source = base_dir.join(relative);
    let source = source.canonicalize().map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })?;
    if !base_dir
        .canonicalize()
        .is_ok_and(|base| source.starts_with(base))
    {
        return Err(Status::error(
            "INVALID_ATTACHMENT_PATH",
            format!("{kind} resolves outside model directory"),
        ));
    }
    fs::read(&source).map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })
}

fn stable_expression_id(document: &Document, index: usize, name: &str) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(b"expression");
        digest.update(index.to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn stable_motion_id(document: &Document, relative: &str) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(b"motion");
        digest.update(relative.as_bytes());
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn stable_pose_id(document: &Document) -> String {
    stable_attachment_id(document, b"pose")
}

fn stable_attachment_id(document: &Document, marker: &[u8]) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(marker);
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn resolved_manifest(path: &Path) -> Result<PathBuf, Status> {
    resolved_manifest_for(path, ProjectEncoding::Json)
}

fn resolved_manifest_for(path: &Path, encoding: ProjectEncoding) -> Result<PathBuf, Status> {
    let requested = encoding.manifest(path);
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
