use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use kasane_core::types::Status;
use kasane_core::Document;

use crate::filesystem::{self as io, FileSystem, NativeFileSystem, Publication};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeValidation {
    Passed,
    Unavailable,
    NotPerformed,
}

impl RuntimeValidation {
    fn report_value(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Unavailable => "unavailable",
            Self::NotPerformed => "not_performed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageValidation {
    pub structural: bool,
    pub runtime: RuntimeValidation,
    pub model3_loader: RuntimeValidation,
    pub animation: RuntimeValidation,
    pub render: RuntimeValidation,
}

impl PackageValidation {
    pub const fn structural(runtime: RuntimeValidation) -> Self {
        Self {
            structural: true,
            runtime,
            model3_loader: RuntimeValidation::NotPerformed,
            animation: RuntimeValidation::NotPerformed,
            render: RuntimeValidation::NotPerformed,
        }
    }
}

pub type PackageValidator = Box<dyn Fn(&ExportPlan) -> Result<PackageValidation, Status>>;

pub struct PackageOptions {
    pub asset_root: PathBuf,
    pub destination: PathBuf,
    pub validate: Option<PackageValidator>,
}

mod plan;
pub use plan::{build_export_plan, ExportFile, ExportFileKind, ExportPlan, PackageReference};

pub fn publish_package(doc: &Document, options: &PackageOptions) -> Result<Publication, Status> {
    publish_with_filesystem(doc, options, &NativeFileSystem)
}

pub(crate) fn publish_with_filesystem(
    doc: &Document,
    options: &PackageOptions,
    files: &dyn FileSystem,
) -> Result<Publication, Status> {
    let validate = options.validate.as_ref().ok_or_else(|| {
        Status::error(
            "MISSING_VALIDATOR",
            "Package publication requires an explicit validation gate",
        )
    })?;
    let plan = build_export_plan(doc, &options.asset_root)?;
    publish_plan_with_filesystem(&plan, &options.destination, validate.as_ref(), files)
}

/// Validate and publish the exact captured bytes; source files are never reopened.
pub fn publish_export_plan(
    plan: &ExportPlan,
    destination: &std::path::Path,
    validate: &dyn Fn(&ExportPlan) -> Result<PackageValidation, Status>,
) -> Result<Publication, Status> {
    publish_plan_with_filesystem(plan, destination, validate, &NativeFileSystem)
}

fn publish_plan_with_filesystem(
    plan: &ExportPlan,
    destination: &std::path::Path,
    validate: &dyn Fn(&ExportPlan) -> Result<PackageValidation, Status>,
    files: &dyn FileSystem,
) -> Result<Publication, Status> {
    io::reject_symlink(destination)?;
    let destination = io::local_path(destination)?;
    let parent = destination
        .parent()
        .filter(|_| destination.file_name().is_some())
        .ok_or_else(|| Status::error("INVALID_DESTINATION", "Cannot replace a filesystem root"))?;
    if plan
        .source_paths
        .iter()
        .any(|source| source.starts_with(&destination))
    {
        return Err(Status::error(
            "INVALID_DESTINATION",
            "Export would replace source project or assets",
        ));
    }
    let validation = validate(plan)?;
    if !validation.structural {
        return Err(Status::error(
            "INVALID_VALIDATION_RESULT",
            "Publication requires successful structural validation",
        ));
    }
    let mut report = plan.report.clone();
    report["runtime_validation"] = validation.runtime.report_value().into();
    report["framework_model3_loader_validation"] = validation.model3_loader.report_value().into();
    report["framework_animation_validation"] = validation.animation.report_value().into();
    report["framework_render_validation"] = validation.render.report_value().into();
    let directories = validate_package_paths(
        plan.files
            .keys()
            .map(String::as_str)
            .chain(["export-report.json"]),
    )?;
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
    for directory in &directories {
        fs::create_dir_all(stage.0.join(directory)).map_err(io::io_error)?;
    }
    for (path, file) in &plan.files {
        files
            .write_new(&stage.0.join(path), file.bytes())
            .map_err(io::io_error)?;
    }
    files
        .write_new(
            &stage.0.join("export-report.json"),
            report.to_string().as_bytes(),
        )
        .map_err(io::io_error)?;
    // Sync every ancestor of managed files too, deepest first. File fsync
    // alone does not make newly created nested directory entries durable.
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.split('/').count()));
    for directory in directories {
        files
            .sync_directory(&stage.0.join(directory))
            .map_err(io::io_error)?;
    }
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

/// Validate the entire file namespace before writing anything. Use folded names
/// for collision checks so a package is portable to case-insensitive volumes.
/// Return the original directory spellings for creation/durability operations.
fn validate_package_paths<'a>(
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<String>, Status> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::from(["textures".to_string()]);
    for path in paths {
        if !kasane_core::document::valid_attachment_path(path) || !files.insert(path.to_lowercase())
        {
            return Err(Status::error("ATTACHMENT_PATH_COLLISION", path));
        }
        let mut parent = path;
        while let Some((directory, _)) = parent.rsplit_once('/') {
            directories.insert(directory.to_string());
            parent = directory;
        }
    }
    for directory in &directories {
        if files.contains(&directory.to_lowercase()) {
            return Err(Status::error("ATTACHMENT_PATH_COLLISION", directory));
        }
    }
    Ok(directories)
}

#[cfg(test)]
mod path_tests {
    use super::validate_package_paths;

    #[test]
    fn rejects_portability_and_file_directory_collisions() {
        for paths in [
            vec!["model.moc3", "MODEL.MOC3"],
            vec!["sounds", "sounds/a.wav"],
            vec!["SOUNDS", "sounds/a.wav"],
            vec!["textures"],
            vec!["sounds/./a.wav"],
            vec!["sounds//a.wav"],
            vec!["sounds/a.wav/"],
        ] {
            assert_eq!(
                validate_package_paths(paths).unwrap_err().code,
                "ATTACHMENT_PATH_COLLISION"
            );
        }
        let directories =
            validate_package_paths(["motions/clip.motion3.json", "motions/audio/a.wav"]).unwrap();
        assert!(directories.contains("motions"));
        assert!(directories.contains("motions/audio"));
    }
}
