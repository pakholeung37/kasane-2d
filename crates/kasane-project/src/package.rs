use std::fs;
use std::path::PathBuf;

use kasane_core::types::Status;
use kasane_core::Document;
use kasane_moc3::{encode_moc3, Moc3Artifact};

use crate::filesystem::{self as io, FileSystem, NativeFileSystem, Publication};
use crate::store::{asset_path, read_project_asset};

pub type ArtifactValidator = Box<dyn Fn(&Moc3Artifact) -> Result<(), Status>>;

pub struct PackageOptions {
    pub asset_root: PathBuf,
    pub destination: PathBuf,
    pub validate: Option<ArtifactValidator>,
}

/// Publish only verified source bytes; callers must inspect publication warnings.
pub fn publish_package(doc: &Document, options: &PackageOptions) -> Result<Publication, Status> {
    publish_with_filesystem(doc, options, &NativeFileSystem)
}

pub(crate) fn publish_with_filesystem(
    doc: &Document,
    options: &PackageOptions,
    files: &dyn FileSystem,
) -> Result<Publication, Status> {
    if doc.transaction_active() {
        return Err(Status::error(
            "TRANSACTION_ACTIVE",
            "Commit or cancel edits first",
        ));
    }
    let validator = options.validate.as_ref().ok_or_else(|| {
        Status::error(
            "MISSING_VALIDATOR",
            "Package publication requires an explicit validation gate",
        )
    })?;
    io::reject_symlink(&options.destination)?;
    let destination = io::local_path(&options.destination)?;
    let parent = destination
        .parent()
        .filter(|_| destination.file_name().is_some())
        .ok_or_else(|| Status::error("INVALID_DESTINATION", "Cannot replace a filesystem root"))?;
    if !options.asset_root.as_os_str().is_empty()
        && io::local_path(&options.asset_root)?.starts_with(&destination)
    {
        return Err(Status::error(
            "INVALID_DESTINATION",
            "Export would replace the source project",
        ));
    }
    for id in doc.asset_order() {
        if asset_path(&options.asset_root, doc.get_asset(id).unwrap())?.starts_with(&destination) {
            return Err(Status::error(
                "INVALID_DESTINATION",
                "Export would replace a source asset",
            ));
        }
    }

    let artifact = encode_moc3(doc)?;
    // Verify even unused project assets, as in the existing DocumentSession contract.
    // The bytes checked here are the exact bytes written below; never reopen after validation.
    let mut verified = std::collections::HashMap::new();
    for id in doc.asset_order() {
        let data = read_project_asset(&options.asset_root, doc.get_asset(id).unwrap())?;
        verified.insert(id.as_str(), data.bytes);
    }
    validator(&artifact)?;
    fs::create_dir_all(parent).map_err(io::io_error)?;
    let _lock = io::lock(parent)?;
    io::reject_symlink(&destination)?;
    if destination.exists() && !destination.is_dir() {
        return Err(Status::error(
            "INVALID_DESTINATION",
            "Destination is not a directory",
        ));
    }
    let stage = io::Stage::new(parent)?;
    fs::create_dir(stage.0.join("textures")).map_err(io::io_error)?;
    files
        .write_new(&stage.0.join("model.moc3"), &artifact.bytes)
        .map_err(io::io_error)?;
    files
        .write_new(
            &stage.0.join("model.model3.json"),
            artifact.model3_json.as_bytes(),
        )
        .map_err(io::io_error)?;
    for slot in &artifact.textures {
        files
            .write_new(
                &stage.0.join(&slot.package_path),
                &verified[slot.asset_id.as_str()],
            )
            .map_err(io::io_error)?;
    }
    let report = serde_json::json!({
        "status": "passed",
        "moc_version": 5,
        "source_revision": doc.revision(),
        "textures": artifact.textures.iter().map(|slot| serde_json::json!({
            "asset_id": slot.asset_id, "source": slot.source,
            "path": slot.package_path, "width": slot.width, "height": slot.height,
        })).collect::<Vec<_>>(),
    });
    files
        .write_new(
            &stage.0.join("export-report.json"),
            report.to_string().as_bytes(),
        )
        .map_err(io::io_error)?;
    files
        .sync_directory(&stage.0.join("textures"))
        .map_err(io::io_error)?;
    files.sync_directory(&stage.0).map_err(io::io_error)?;

    // Reserve a separate directory so a failed rollback leaves a named recovery copy.
    let backup = io::Stage::new(parent)?;
    let previous = backup.0.join("previous");
    let replacing = destination.exists();
    if replacing {
        files
            .rename(&destination, &previous)
            .map_err(io::io_error)?;
    }
    if let Err(error) = files.rename(&stage.0, &destination) {
        if replacing {
            if let Err(rollback) = files.rename(&previous, &destination) {
                let recovery = backup.preserve().join("previous");
                return Err(Status::error(
                    "ROLLBACK_FAILED",
                    format!(
                        "Publication failed: {error}; rollback failed: {rollback}; recover from {}",
                        recovery.display()
                    ),
                ));
            }
        }
        return Err(io::io_error(error));
    }
    let mut publication = Publication::finish(files, parent);
    if let Err(e) = fs::remove_dir_all(&backup.0) {
        let recovery = backup.preserve();
        publication.warnings.push(format!(
            "Published; old package retained at {}: {e}",
            recovery.display()
        ));
    }
    Ok(publication)
}
