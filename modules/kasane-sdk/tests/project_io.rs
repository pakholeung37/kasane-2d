use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use kasane_core::{Canvas, Vec2};
use kasane_project::{FileSystem, NativeFileSystem};
use kasane_sdk::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
    AuthoringSession, ObjectKind,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kasane-sdk-io-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn png(&self, name: &str, color: u8) -> PathBuf {
        let path = self.path(name);
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[color; 16]).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn session(texture: &Path, with_mesh: bool) -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let asset = prepare_png_asset(ASSET, "texture", texture).unwrap();
    sdk.edit("create", None, |edit| {
        edit.create_asset(asset)?;
        if with_mesh {
            edit.create_mesh(
                rectangle_mesh(
                    MESH,
                    "mesh",
                    ASSET,
                    Vec2::new(0.0, 0.0),
                    Vec2::new(2.0, 2.0),
                )
                .unwrap(),
            )?;
        }
        Ok(())
    })
    .unwrap();
    sdk
}

#[test]
fn save_relocates_unhashed_history_and_redo_reaches_saved_baseline() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    let mut asset = sdk.asset(ASSET).unwrap();
    asset.sha256.clear();
    sdk.edit("legacy resource", None, |edit| edit.replace_asset(asset))
        .unwrap();
    let mesh = sdk.mesh(MESH).unwrap();
    let moved: Vec<_> = mesh
        .base_positions
        .iter()
        .map(|p| Vec2::new(p.x + 5.0, p.y))
        .collect();
    sdk.edit("move", None, |edit| {
        edit.update_positions(MESH, &mesh.vertex_ids, &moved)
    })
    .unwrap();
    let before = sdk.version();
    let receipt = sdk.save_project(&temp.path("first"), Some(before)).unwrap();
    assert_eq!(receipt.before, before);
    assert_eq!(receipt.after, sdk.version());
    assert!(receipt.manifest.exists());
    assert!(receipt.durable);
    assert_eq!(receipt.history_warnings.len(), 1);
    assert!(receipt.history_warnings[0].contains(ASSET));
    assert!(!sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
    let saved_asset = sdk.asset(ASSET).unwrap();
    assert!(saved_asset.source.starts_with("assets/"));
    assert_eq!(saved_asset.sha256.len(), 64);

    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().base_positions, mesh.base_positions);
    assert_eq!(sdk.asset(ASSET).unwrap(), saved_asset);
    assert!(sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().base_positions, moved);
    assert_eq!(sdk.asset(ASSET).unwrap(), saved_asset);
    assert!(!sdk.modified());

    let expected = sdk.evaluate(&Default::default()).unwrap();
    let mut reopened = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    assert!(reopened
        .open_project(&receipt.manifest, None)
        .unwrap()
        .diagnostics
        .is_empty());
    assert_eq!(reopened.asset(ASSET), sdk.asset(ASSET));
    assert_eq!(reopened.mesh(MESH), sdk.mesh(MESH));
    assert_eq!(
        reopened.evaluate(&Default::default()).unwrap().drawables,
        expected.drawables
    );
}

#[test]
fn save_relocates_redo_branch_and_retains_saved_content() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    sdk.save_project(&temp.path("first"), None).unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH, "redo name"))
        .unwrap();
    sdk.undo().unwrap();
    assert_eq!(sdk.history_lengths().1, 1);
    sdk.save_project(&temp.path("second"), None).unwrap();
    let saved = sdk.asset(ASSET).unwrap();
    assert!(!sdk.modified());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().name, "redo name");
    assert_eq!(sdk.asset(ASSET).unwrap(), saved);
    assert!(sdk.diagnose_resources().is_empty());
    assert!(sdk.modified());
    sdk.undo().unwrap();
    assert!(!sdk.modified());
}

#[test]
fn save_as_keeps_old_resource_for_same_id_and_new_resource_for_redo() {
    let temp = TempDir::new();
    let a = temp.png("a.png", 10);
    let b = temp.png("b.png", 200);
    let mut sdk = session(&a, true);
    sdk.save_project(&temp.path("first"), None).unwrap();
    let old = sdk.asset(ASSET).unwrap();
    let replacement = prepare_png_asset(ASSET, "texture", &b).unwrap();
    sdk.edit("replace texture", None, |edit| {
        edit.replace_asset(replacement)
    })
    .unwrap();
    let second = temp.path("second");
    sdk.save_project(&second, None).unwrap();
    let saved = sdk.asset(ASSET).unwrap();
    assert_ne!(old.sha256, saved.sha256);
    assert_eq!(
        sdk.project_path(),
        Some(second.join("project.kasane.json").as_path())
    );
    assert!(!sdk.modified());

    sdk.undo().unwrap();
    let restored = sdk.asset(ASSET).unwrap();
    assert_eq!(restored.sha256, old.sha256);
    assert_eq!(
        restored.source,
        temp.path("first").join(&old.source).to_str().unwrap()
    );
    assert!(sdk.diagnose_resources().is_empty());
    assert!(sdk.modified());
    sdk.redo().unwrap();
    assert_eq!(sdk.asset(ASSET).unwrap(), saved);
    assert!(sdk.diagnose_resources().is_empty());
    assert!(!sdk.modified());
}

#[test]
fn deleted_historical_asset_is_rebased_to_its_original_project() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, false);
    sdk.save_project(&temp.path("first"), None).unwrap();
    let old = sdk.asset(ASSET).unwrap();
    sdk.edit("erase asset", None, |edit| edit.erase_object(ASSET))
        .unwrap();
    sdk.save_project(&temp.path("second"), None).unwrap();
    assert!(sdk.asset_ids().is_empty());
    sdk.undo().unwrap();
    assert_eq!(sdk.asset(ASSET).unwrap().sha256, old.sha256);
    assert_eq!(
        sdk.asset(ASSET).unwrap().source,
        temp.path("first").join(old.source).to_str().unwrap()
    );
    assert!(sdk.diagnose_resources().is_empty());
    sdk.redo().unwrap();
    assert!(sdk.asset_ids().is_empty());
    assert!(!sdk.modified());
}

#[test]
fn failed_save_and_open_preserve_state_successful_open_resets_generation() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    let path = temp.path("first");
    sdk.save_project(&path, None).unwrap();
    let manifest = sdk.project_path().unwrap().to_path_buf();
    let original_manifest = fs::read(&manifest).unwrap();
    let old_handle = sdk.handle(ObjectKind::Mesh, MESH).unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH, "edited"))
        .unwrap();
    let before = sdk.version();
    let history = sdk.history_lengths();
    let frame = sdk.preview_frame().unwrap();

    let existing = temp.path("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("project.kasane.json"), &original_manifest).unwrap();
    assert_eq!(
        sdk.save_project(&existing, None).unwrap_err().code.as_ref(),
        "DESTINATION_EXISTS"
    );
    fs::write(&manifest, [original_manifest.as_slice(), b" "].concat()).unwrap();
    assert_eq!(
        sdk.save_project(&path, None).unwrap_err().code.as_ref(),
        "PROJECT_CONFLICT"
    );
    assert_eq!(
        sdk.open_project(&temp.path("missing"), None)
            .unwrap_err()
            .code
            .as_ref(),
        "PROJECT_IO"
    );
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), history);
    assert!(Arc::ptr_eq(&frame, &sdk.preview_frame().unwrap()));
    sdk.resolve_handle(&old_handle).unwrap();

    fs::write(&manifest, original_manifest).unwrap();
    let result = sdk.open_project(&path, Some(before)).unwrap();
    assert!(result.status.is_ok());
    assert!(result.diagnostics.is_empty());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert_eq!(sdk.mesh(MESH).unwrap().name, "mesh");
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );

    let asset = sdk.asset(ASSET).unwrap();
    fs::remove_file(path.join(&asset.source)).unwrap();
    let opened = sdk.open_project(&path, None).unwrap();
    assert_eq!(opened.diagnostics.len(), 1);
    assert_eq!(opened.diagnostics[0].asset_id, ASSET);
    assert_eq!(sdk.diagnose_resources().len(), 1);
}

fn external_model() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/external_v50")
}

#[test]
fn model3_import_edits_saves_and_exports_without_changing_session_on_export() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("initial.png", 10), true);
    let old_handle = sdk.handle(ObjectKind::Mesh, MESH).unwrap();
    let before = sdk.version();
    let imported = sdk
        .import_model3(&external_model().join("model.model3.json"), Some(before))
        .unwrap();
    assert_eq!(imported.before, before);
    assert_eq!(imported.after, sdk.version());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert_eq!(imported.report.moc_version, 5);
    assert!(imported.project.diagnostics.is_empty());
    assert!(sdk.project_path().is_none());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let mesh_id = sdk.mesh_ids()[0].clone();
    let old_name = sdk.mesh(&mesh_id).unwrap().name;
    sdk.edit("rename imported", None, |edit| {
        edit.rename_mesh(&mesh_id, "edited imported")
    })
    .unwrap();
    sdk.save_project(&temp.path("saved"), None).unwrap();
    let saved_version = sdk.version();
    let history = sdk.history_lengths();
    let package = temp.path("exported");
    let publication = sdk.export_package(&package, Some(saved_version)).unwrap();
    assert!(publication.published);
    assert!(package.join("model.moc3").exists());
    assert_eq!(sdk.version(), saved_version);
    assert_eq!(sdk.history_lengths(), history);
    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(&mesh_id).unwrap().name, old_name);
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn bare_moc3_import_rejects_relative_texture_paths_without_losing_state() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("initial.png", 10), true);
    let before = sdk.version();
    let moc3 = external_model().join("model.moc3");
    let relative_map = HashMap::from([(0, PathBuf::from("texture_00.png"))]);
    assert_eq!(
        sdk.import_bare_moc3(&moc3, &relative_map, None)
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PATH"
    );
    assert_eq!(sdk.version(), before);
    assert!(sdk.mesh(MESH).is_some());
    let texture_map = HashMap::from([(0, external_model().join("texture_00.png"))]);
    let imported = sdk
        .import_bare_moc3(&moc3, &texture_map, Some(before))
        .unwrap();
    assert_eq!(imported.report.moc_version, 5);
    assert!(imported.project.diagnostics.is_empty());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert!(!sdk.mesh_ids().is_empty());
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn relocation_requires_same_bytes_and_replacement_accepts_new_content() {
    let temp = TempDir::new();
    let original = temp.png("a.png", 10);
    let changed = temp.png("b.png", 200);
    let relocated_path = temp.path("same.png");
    fs::copy(&original, &relocated_path).unwrap();
    let mut sdk = session(&original, true);
    let asset = sdk.asset(ASSET).unwrap();
    let before = sdk.version();
    assert_eq!(
        prepare_relocated_asset(&asset, &changed)
            .unwrap_err()
            .code
            .as_ref(),
        "RESOURCE_HASH"
    );
    assert_eq!(sdk.version(), before);
    let relocated = prepare_relocated_asset(&asset, &relocated_path).unwrap();
    assert_eq!(relocated.sha256, asset.sha256);
    assert_ne!(relocated.source, asset.source);
    sdk.edit("relocate", None, |edit| edit.replace_asset(relocated))
        .unwrap();
    assert!(sdk.diagnose_resources().is_empty());
    let replacement = prepare_png_asset(ASSET, "texture", &changed).unwrap();
    sdk.edit("replace", None, |edit| edit.replace_asset(replacement))
        .unwrap();
    assert_ne!(sdk.asset(ASSET).unwrap().sha256, asset.sha256);
    sdk.undo().unwrap();
    assert_eq!(sdk.asset(ASSET).unwrap().sha256, asset.sha256);
}

#[test]
fn relative_png_paths_require_an_explicit_absolute_base() {
    let temp = TempDir::new();
    let absolute = temp.png("texture.png", 10);
    assert_eq!(
        prepare_png_asset(ASSET, "texture", Path::new("texture.png"))
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PATH"
    );
    let from_base =
        prepare_png_asset_from_base(ASSET, "texture", &temp.0, Path::new("texture.png")).unwrap();
    assert_eq!(
        from_base,
        prepare_png_asset(ASSET, "texture", &absolute).unwrap()
    );
    assert_eq!(
        prepare_png_asset_from_base(
            ASSET,
            "texture",
            Path::new("relative-base"),
            Path::new("texture.png"),
        )
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_PATH"
    );
}

#[derive(Default)]
struct PostCommitWarning {
    syncs: AtomicUsize,
}

impl FileSystem for PostCommitWarning {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        NativeFileSystem.write_new(path, bytes)
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        let count = self.syncs.fetch_add(1, Ordering::SeqCst) + 1;
        if count == 3 {
            return Err(io::Error::other("injected post-publication warning"));
        }
        NativeFileSystem.sync_directory(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        NativeFileSystem.rename(from, to)
    }
}

#[test]
fn publication_warning_keeps_saved_baseline_and_undo_history() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let filesystem = Arc::new(PostCommitWarning::default());
    let mut sdk = AuthoringSession::with_filesystem(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
        filesystem,
    )
    .unwrap();
    let asset = prepare_png_asset(ASSET, "texture", &texture).unwrap();
    sdk.edit("create", None, |edit| edit.create_asset(asset))
        .unwrap();
    let project = temp.path("warning-project");
    let warning = sdk.save_project(&project, None).unwrap();
    assert!(!warning.durable);
    assert!(!warning.warnings.is_empty());
    assert!(warning.manifest.exists());
    assert!(!sdk.modified());
    sdk.undo().unwrap();
    assert!(sdk.asset_ids().is_empty());
    sdk.redo().unwrap();
    assert!(sdk.diagnose_resources().is_empty());
    assert!(!sdk.modified());
    let retried = sdk.save_project(&project, None).unwrap();
    #[cfg(unix)]
    assert!(retried.durable);
    #[cfg(unix)]
    assert!(retried.warnings.is_empty());
    #[cfg(not(unix))]
    assert!(!retried.durable);
}
