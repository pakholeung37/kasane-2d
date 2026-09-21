use std::collections::HashMap;
use std::fs;
use std::path::Path;

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    Appearance, BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable,
    BlendShapeTargetKind, Canvas, DeltaGlueKeyform, DeltaKeyforms, Glue, GlueVertexPair,
    ImageAsset, Mesh, MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, Parameter,
    ParameterKind, Part, RotationPose, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_moc3::encode_moc3;
use kasane_project::{
    content_sha256, decode_png, decode_project, encode_project, DocumentSession, DocumentStore,
};

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn sid(n: i32) -> String {
    format!("{n:08x}-2222-4222-8222-222222222222")
}

fn create_test_png(path: &Path) -> (Vec<u8>, String) {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, 8, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data = vec![123u8; 8 * 8 * 4];
        writer.write_image_data(&data).unwrap();
    }
    let sha = content_sha256(&buf);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, &buf).unwrap();
    (buf, sha)
}

fn fixture_doc(sha1: &str, sha2: &str) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(271.0, 193.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "A".to_string(),
            source: "assets/0.png".to_string(),
            width: 8,
            height: 8,
            sha256: sha1.to_string(),
        })
        .status
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(3),
            name: "B".to_string(),
            source: "assets/1.png".to_string(),
            width: 8,
            height: 8,
            sha256: sha2.to_string(),
        })
        .status
        .is_ok());

    assert!(doc
        .create_parameter(Parameter {
            id: id(6),
            runtime_id: "ParamAngle".to_string(),
            name: "Angle".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_part(Part {
            id: sid(1),
            runtime_id: "PartRoot".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 10.0,
        })
        .status
        .is_ok());

    let rot = Transform {
        id: sid(2),
        runtime_id: "RootRotation".to_string(),
        name: "Root Rotation".to_string(),
        part_id: sid(1),
        kind: TransformKind::Rotation,
        rotation: RotationPose {
            origin: Vec2::new(320.0, 240.0).into(),
            angle: 0.0,
            scale: 1.0,
            reflect_x: false,
            reflect_y: false,
        },
        ..Default::default()
    };
    assert!(doc.create_transform(rot).status.is_ok());

    let m1 = Mesh {
        id: id(4),
        name: "Quad".to_string(),
        runtime_id: "ArtMeshA".to_string(),
        texture_asset_id: id(2),
        part_id: sid(1),
        deformer_id: sid(2),
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(100.0, 10.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(10.0, 100.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        ..Default::default()
    };
    assert!(doc.create_mesh(m1).status.is_ok());

    let mut b = MeshBinding {
        id: id(7),
        mesh_id: id(4),
        axes: vec![BindingAxis {
            parameter_id: id(6),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: Vec::new(),
    };
    for &k in &[-1.0f32, 0.0, 1.0] {
        let mut kf = MeshKeyform {
            keys: vec![k],
            positions: doc.get_mesh(&id(4)).unwrap().base_positions.clone(),
            appearance: Appearance::default(),
            draw_order: None,
        };
        kf.positions[0].x += k * 5.0;
        b.keyforms.push(kf);
    }
    assert!(doc.create_binding(b).status.is_ok());

    doc
}

#[test]
fn test_png_decode_and_sha256() {
    let temp_dir = std::env::temp_dir().join(format!("kasane-test-png-{}", std::process::id()));
    let png_path = temp_dir.join("test.png");
    let (bytes, sha) = create_test_png(&png_path);

    assert_eq!(sha.len(), 64);
    assert_eq!(content_sha256(&bytes), sha);

    let data = decode_png(&bytes).expect("decode_png failed");
    assert_eq!(data.width, 8);
    assert_eq!(data.height, 8);
    assert_eq!(data.sha256, sha);
    assert_eq!(data.rgba.len(), 8 * 8 * 4);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_project_encode_decode_roundtrip() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let before = fixture_doc(sha1, sha2);

    let encoded = encode_project(&before).expect("encode_project failed");
    assert!(encoded.contains("\"format\": \"kasane-directory-project\""));
    assert!(encoded.contains("\"format_version\": 4"));

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert!(before.same_content(&decoded));

    // Both documents evaluate to identical frames
    let mut p = HashMap::new();
    p.insert(id(6), 0.5);

    let mut frame_before = DrawableFrame::default();
    let mut frame_after = DrawableFrame::default();
    assert!(evaluate_frame(&before, &p, &mut frame_before).is_ok());
    assert!(evaluate_frame(&decoded, &p, &mut frame_after).is_ok());

    assert_eq!(frame_before.drawables.len(), frame_after.drawables.len());
    for i in 0..frame_before.drawables.len() {
        let d1 = &frame_before.drawables[i];
        let d2 = &frame_after.drawables[i];
        assert_eq!(d1.positions, d2.positions);
        assert_eq!(d1.uvs, d2.uvs);
        assert_eq!(d1.indices, d2.indices);
        assert_eq!(d1.draw_order, d2.draw_order);
    }

    // Both encode to identical MOC3 bytes
    let moc1 = encode_moc3(&before).unwrap();
    let moc2 = encode_moc3(&decoded).unwrap();
    assert_eq!(moc1.bytes, moc2.bytes);
}

#[test]
fn test_reject_duplicate_keys_and_malformed() {
    // Duplicate JSON key
    let json_dup = r#"{
      "format": "kasane-directory-project",
      "format": "duplicate-key",
      "format_version": 1,
      "document": {}
    }"#;
    let err = decode_project(json_dup).unwrap_err();
    assert_eq!(err.code, "INVALID_PROJECT");

    // Legacy format
    let legacy_json = r#"{
      "format": "kasane-project",
      "format_version": 5,
      "document": {}
    }"#;
    let err = decode_project(legacy_json).unwrap_err();
    assert_eq!(err.code, "LEGACY_PROJECT");

    // Unsupported version
    let bad_version = r#"{
      "format": "kasane-directory-project",
      "format_version": 99,
      "document": {
        "id": "00000001-1111-4111-8111-111111111111",
        "canvas": [640.0, 480.0],
        "canvas_origin": [0.0, 0.0],
        "pixels_per_unit": 100.0,
        "assets": [],
        "meshes": [],
        "parts": [],
        "transforms": [],
        "parameters": [],
        "bindings": [],
        "scene_bindings": []
      }
    }"#;
    let err = decode_project(bad_version).unwrap_err();
    assert_eq!(err.code, "UNSUPPORTED_VERSION");

    // Invalid asset path (directory traversal)
    let traversal = r#"{
      "format": "kasane-directory-project",
      "format_version": 1,
      "document": {
        "id": "00000001-1111-4111-8111-111111111111",
        "canvas": [640.0, 480.0],
        "canvas_origin": [0.0, 0.0],
        "pixels_per_unit": 100.0,
        "assets": [{
          "id": "00000002-1111-4111-8111-111111111111",
          "name": "escape",
          "source": "../escape.png",
          "width": 8,
          "height": 8,
          "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        }],
        "meshes": [],
        "parts": [],
        "transforms": [],
        "parameters": [],
        "bindings": [],
        "scene_bindings": []
      }
    }"#;
    let err = decode_project(traversal).unwrap_err();
    assert_eq!(err.code, "INVALID_PROJECT");
}

#[test]
fn test_document_session_lifecycle() {
    let base = std::env::temp_dir().join(format!("kasane-session-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let assets_dir = base.join("source_assets");
    let (_, sha1) = create_test_png(&assets_dir.join("assets/0.png"));
    let (_, sha2) = create_test_png(&assets_dir.join("assets/1.png"));

    let mut doc = fixture_doc(&sha1, &sha2);
    // Update asset sources to relative to source_assets
    let mut a1 = doc.get_asset(&id(2)).unwrap().clone();
    a1.source = "assets/0.png".to_string();
    doc.replace_asset(a1);
    let mut a2 = doc.get_asset(&id(3)).unwrap().clone();
    a2.source = "assets/1.png".to_string();
    doc.replace_asset(a2);

    // Save project
    let project_dir = base.join("project");
    let store = DocumentStore::new();
    let (res, snapshot) = store.save(&doc, &assets_dir, &project_dir, None);
    assert!(res.status.is_ok(), "Save failed: {:?}", res.status);
    let snapshot = snapshot.unwrap();
    assert!(snapshot.manifest.exists());

    // Open project in session
    let mut session = DocumentSession::new();
    let open_res = session.open(&snapshot.manifest);
    assert!(
        open_res.status.is_ok(),
        "Open failed: {:?}",
        open_res.status
    );
    assert!(session.document().same_content(&snapshot.document));

    // Edit and save
    assert!(session
        .document_mut()
        .rename_mesh(&id(4), "RenamedQuad".to_string())
        .status
        .is_ok());
    assert!(session.document().modified());

    let save_res = session.save(&snapshot.manifest);
    assert!(save_res.status.is_ok());
    assert!(!session.document().modified());

    // Second session attempting concurrent overwrite with stale sha
    let (conf_res, _) = store.save(
        &doc,
        &project_dir.join("assets"),
        &snapshot.manifest,
        Some("stale_hash_that_does_not_match_current"),
    );
    assert_eq!(conf_res.status.code, "PROJECT_CONFLICT");

    // Package publication
    let package_dir = base.join("published_package");
    let pub_res = session.export_package(&package_dir);
    assert!(
        pub_res.status.is_ok(),
        "export_package failed: {:?}",
        pub_res
    );
    assert!(package_dir.join("model.moc3").exists());
    assert!(package_dir.join("model.model3.json").exists());
    assert!(package_dir.join("textures/0.png").exists());

    let _ = fs::remove_dir_all(base);
}

// Regression coverage for the C++ -> Rust persistence and editing contracts.
struct TestDirectory(std::path::PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "kasane-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn fixture(&self) -> (Document, std::path::PathBuf) {
        let input = self.0.join("input");
        let (_, sha1) = create_test_png(&input.join("assets/0.png"));
        let (_, sha2) = create_test_png(&input.join("assets/1.png"));
        (fixture_doc(&sha1, &sha2), input)
    }
    fn saved_session(&self) -> DocumentSession {
        let (doc, input) = self.fixture();
        let path = self.0.join("project");
        let result = DocumentStore::new().save(&doc, &input, &path, None).0;
        assert!(result.status.is_ok(), "{:?}", result);
        let mut session = DocumentSession::new();
        assert!(session.open(&path).status.is_ok());
        session
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn published_format_preserves_cpp_transform_tags_and_omits_legacy_fields() {
    let text = include_str!("../../../samples/m2-complete/project.kasane.json");
    let doc = decode_project(text).unwrap();
    let wire: serde_json::Value = serde_json::from_str(text).unwrap();
    for t in wire["document"]["transforms"].as_array().unwrap() {
        let expected = if t["kind"] == 0 {
            TransformKind::Warp
        } else {
            TransformKind::Rotation
        };
        assert_eq!(
            doc.get_transform(t["id"].as_str().unwrap()).unwrap().kind,
            expected
        );
    }
    let encoded = encode_project(&doc).unwrap();
    let output: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    let tags = |value: &serde_json::Value| {
        value["document"]["transforms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t["id"].clone(), t["kind"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(tags(&wire), tags(&output));
    for key in ["deformers", "deformation_links", "organization_links"] {
        assert!(output["document"].get(key).is_none());
    }
    assert!(decode_project(&encoded).unwrap().same_content(&doc));
}

fn scene_binding(doc: &mut Document) {
    let transform = doc.get_transform(&sid(2)).unwrap();
    let binding = kasane_core::SceneBinding {
        id: sid(99),
        target_id: sid(2),
        axes: vec![BindingAxis {
            parameter_id: id(6),
            keys: vec![-1.0, 1.0],
        }],
        keyforms: [-1.0, 1.0]
            .into_iter()
            .map(|key| kasane_core::SceneKeyform {
                keys: vec![key],
                rotation: transform.rotation,
                positions: transform.points.clone(),
                ..Default::default()
            })
            .collect(),
    };
    assert!(doc.create_scene_binding(binding).status.is_ok());
}

#[test]
fn bound_transform_type_and_grid_changes_are_atomic_failures() {
    let mut doc = fixture_doc(&"0".repeat(64), &"0".repeat(64));
    scene_binding(&mut doc);
    doc.mark_saved();
    let before = doc.clone();
    let mut transform = doc.get_transform(&sid(2)).unwrap().clone();
    transform.kind = TransformKind::Warp;
    transform.rows = 1;
    transform.columns = 1;
    transform.points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 1.0),
    ];
    assert_eq!(
        doc.replace_transform(transform.clone()).status.code,
        "KEYFORMS_REQUIRED"
    );
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
    assert!(!doc.modified());
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut DrawableFrame::default()).is_ok());
    assert!(doc.erase_object(&sid(99)).status.is_ok());
    assert!(doc.replace_transform(transform.clone()).status.is_ok());
    scene_binding(&mut doc);
    let before = doc.clone();
    transform.rows = 2;
    transform
        .points
        .extend([Vec2::new(0.0, 2.0), Vec2::new(1.0, 2.0)]);
    assert_eq!(
        doc.replace_transform(transform).status.code,
        "KEYFORMS_REQUIRED"
    );
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
}

#[test]
fn parameter_range_changes_validate_mesh_and_scene_bindings() {
    for scene_only in [false, true] {
        let mut doc = fixture_doc(&"0".repeat(64), &"0".repeat(64));
        if scene_only {
            assert!(doc.erase_object(&id(7)).status.is_ok());
            scene_binding(&mut doc);
        }
        doc.mark_saved();
        let before = doc.clone();
        let mut parameter = doc.get_parameter(&id(6)).unwrap().clone();
        parameter.minimum = -0.5;
        parameter.maximum = 0.5;
        assert_eq!(
            doc.replace_parameter(parameter.clone()).status.code,
            "INVALID_KEYS"
        );
        assert!(doc.same_content(&before));
        assert_eq!(doc.revision(), before.revision());
        assert!(!doc.modified());
        parameter.minimum = -2.0;
        parameter.maximum = 2.0;
        let edit = doc.replace_parameter(parameter);
        assert!(edit.status.is_ok());
        assert!(edit.changes.mesh_ids.contains(&id(4)));
        assert!(decode_project(&encode_project(&doc).unwrap())
            .unwrap()
            .same_content(&doc));
    }
}

#[test]
fn save_as_refuses_existing_project_and_preserves_session() {
    let tmp = TestDirectory::new();
    let original = tmp.saved_session();
    let manifest = original.manifest();
    let before = fs::read(manifest).unwrap();
    let mut other = DocumentSession::new();
    assert!(other
        .document_mut()
        .initialize(id(999), original.document().canvas())
        .is_ok());
    let result = other.save(manifest);
    assert_eq!(result.status.code, "DESTINATION_EXISTS");
    assert!(!result.published);
    assert_eq!(before, fs::read(manifest).unwrap());
    assert!(other.manifest().as_os_str().is_empty());
    assert!(other.document().modified());
}

#[test]
fn concurrent_writers_cannot_both_publish_the_same_baseline() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let manifest = session.manifest().to_path_buf();
    let hash = content_sha256(&fs::read(&manifest).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let mut doc = session.document().clone();
            assert!(doc
                .rename_mesh(&id(4), format!("writer-{i}"))
                .status
                .is_ok());
            let barrier = barrier.clone();
            let manifest = manifest.clone();
            let hash = hash.clone();
            let root = session.root();
            std::thread::spawn(move || {
                barrier.wait();
                DocumentStore::new()
                    .save(&doc, &root, &manifest, Some(&hash))
                    .0
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.status.is_ok()).count(), 1);
    assert!(results.iter().all(|r| r.status.is_ok()
        || ["PROJECT_BUSY", "PROJECT_CONFLICT"].contains(&r.status.code.as_str())));
    let mut stale = session;
    assert_eq!(stale.save(&manifest).status.code, "PROJECT_CONFLICT");
}

#[test]
fn save_repairs_corrupt_asset_name_without_overwriting_old_bytes() {
    let tmp = TestDirectory::new();
    let (doc, input) = tmp.fixture();
    let destination = tmp.0.join("destination");
    let sha = &doc.get_asset(&id(2)).unwrap().sha256;
    fs::create_dir_all(destination.join("assets")).unwrap();
    let corrupted = destination.join(format!("assets/{sha}.png"));
    fs::write(&corrupted, b"partial old file").unwrap();
    let (result, snapshot) = DocumentStore::new().save(&doc, &input, &destination, None);
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(fs::read(corrupted).unwrap(), b"partial old file");
    let snapshot = snapshot.unwrap();
    assert_ne!(
        snapshot.document.get_asset(&id(2)).unwrap().source,
        format!("assets/{sha}.png")
    );
    assert!(DocumentStore::new()
        .open(&destination)
        .0
        .resources_complete());
}

#[test]
fn export_refuses_source_project_ancestor_and_source_asset_directories() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let manifest = fs::read(session.manifest()).unwrap();
    for target in [session.root(), tmp.0.clone(), session.root().join("assets")] {
        let result = session.export_package(&target);
        assert_eq!(result.status.code, "INVALID_DESTINATION", "{result:?}");
        assert_eq!(fs::read(session.manifest()).unwrap(), manifest);
    }
    assert!(session
        .export_package(&session.root().join("exports/model"))
        .status
        .is_ok());
}

#[cfg(unix)]
#[test]
fn path_aliases_do_not_bypass_export_guard_or_save_baseline() {
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let alias = tmp.0.join("alias");
    std::os::unix::fs::symlink(session.root(), &alias).unwrap();
    assert!(session
        .save(&alias.join("project.kasane.json"))
        .status
        .is_ok());
    assert_eq!(session.export_package(&alias).status.code, "INVALID_PATH");
    assert_eq!(
        session.export_package(&alias.join("assets/..")).status.code,
        "INVALID_DESTINATION"
    );
    let external = tmp.0.join("external");
    fs::create_dir(&external).unwrap();
    let assets = session.root().join("assets");
    fs::rename(&assets, external.join("assets")).unwrap();
    std::os::unix::fs::symlink(external.join("assets"), &assets).unwrap();
    assert_eq!(
        session
            .save(session.manifest().to_path_buf().as_path())
            .status
            .code,
        "INVALID_PATH"
    );
}

#[test]
fn export_rejects_hash_mismatch_and_keeps_existing_package() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let destination = tmp.0.join("package");
    assert!(session.export_package(&destination).status.is_ok());
    fs::write(destination.join("old-marker"), b"keep").unwrap();
    let asset = session.document().get_asset(&id(2)).unwrap();
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, 8, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; 8 * 8 * 4])
            .unwrap();
    }
    fs::write(session.root().join(&asset.source), buf).unwrap();
    assert_eq!(
        session.export_package(&destination).status.code,
        "RESOURCE_HASH"
    );
    assert_eq!(fs::read(destination.join("old-marker")).unwrap(), b"keep");
}

#[test]
fn png_normalization_preserves_palette_low_depth_16bit_and_transparency() {
    let cases = [
        (
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            vec![0],
            Some(vec![0]),
            [255, 0, 0, 0],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::One,
            vec![0x80],
            None,
            [255, 255, 255, 255],
        ),
        (
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            vec![0xff, 0xff, 0, 0, 0x80, 0x80, 0x80, 0x80],
            None,
            [255, 0, 128, 128],
        ),
        (
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            vec![255, 0, 0],
            Some(vec![0, 255, 0, 0, 0, 0]),
            [255, 0, 0, 0],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            vec![123],
            Some(vec![0, 123]),
            [123, 123, 123, 0],
        ),
    ];
    for (color, depth, bytes, transparency, expected) in cases {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                encoder.set_palette(vec![255, 0, 0]);
            }
            if let Some(trns) = transparency {
                encoder.set_trns(trns);
            }
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&bytes)
                .unwrap();
        }
        assert_eq!(
            decode_png(&encoded).unwrap().rgba,
            expected,
            "{color:?} {depth:?}"
        );
    }
}

#[derive(Clone)]
enum Failure {
    ShortWrite,
    FileSync,
    DirectorySync(usize),
    Rename(Vec<usize>),
    ExternalChange(std::path::PathBuf),
}
struct FailingFileSystem {
    failure: Failure,
    syncs: std::sync::atomic::AtomicUsize,
    renames: std::sync::atomic::AtomicUsize,
}
impl FailingFileSystem {
    fn new(failure: Failure) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            failure,
            syncs: 0.into(),
            renames: 0.into(),
        })
    }
}
impl kasane_project::FileSystem for FailingFileSystem {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match &self.failure {
            Failure::ShortWrite => {
                kasane_project::NativeFileSystem.write_new(path, &bytes[..bytes.len() / 2])?;
                Err(std::io::Error::other("injected short write"))
            }
            Failure::FileSync => {
                fs::write(path, bytes)?;
                Err(std::io::Error::other("injected file sync failure"))
            }
            Failure::ExternalChange(manifest)
                if path.file_name().is_some_and(|s| s == "manifest.json") =>
            {
                kasane_project::NativeFileSystem.write_new(path, bytes)?;
                fs::write(manifest, b"external change")
            }
            _ => kasane_project::NativeFileSystem.write_new(path, bytes),
        }
    }
    fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
        let n = self.syncs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if matches!(self.failure, Failure::DirectorySync(at) if at == n) {
            return Err(std::io::Error::other("injected directory sync failure"));
        }
        kasane_project::NativeFileSystem.sync_directory(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        let n = self
            .renames
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if matches!(&self.failure, Failure::Rename(at) if at.contains(&n)) {
            return Err(std::io::Error::other("injected rename failure"));
        }
        kasane_project::NativeFileSystem.rename(from, to)
    }
}

#[test]
fn precommit_failures_preserve_manifest_document_revision_and_saved_baseline() {
    for fault in [
        Failure::ShortWrite,
        Failure::FileSync,
        Failure::DirectorySync(1),
        Failure::DirectorySync(2),
        Failure::Rename(vec![1]),
    ] {
        let tmp = TestDirectory::new();
        let initial = tmp.saved_session();
        let path = initial.manifest().to_path_buf();
        let original_bytes = fs::read(&path).unwrap();
        let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(fault));
        assert!(session.open(&path).status.is_ok());
        assert!(session
            .document_mut()
            .rename_mesh(&id(4), "Edited".into())
            .status
            .is_ok());
        let before = session.document().clone();
        let result = session.save(&path);
        assert_eq!(result.status.code, "PROJECT_IO", "{result:?}");
        assert!(!result.published);
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert!(session.document().same_content(&before));
        assert_eq!(session.document().revision(), before.revision());
        assert!(session.document().modified());
        assert!(!fs::read_dir(session.root()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".kasane-stage-")));
    }
}

#[test]
fn postcommit_sync_failure_reports_publication_and_advances_baseline() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let path = initial.manifest().to_path_buf();
    let mut session =
        DocumentSession::with_filesystem(FailingFileSystem::new(Failure::DirectorySync(3)));
    assert!(session.open(&path).status.is_ok());
    assert!(session
        .document_mut()
        .rename_mesh(&id(4), "Published".into())
        .status
        .is_ok());
    let result = session.save(&path);
    assert!(result.status.is_ok());
    assert!(result.published);
    assert!(!result.durable);
    assert!(!result.warnings.is_empty());
    assert!(!session.document().modified());
    assert_eq!(
        decode_project(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .get_mesh(&id(4))
            .unwrap()
            .name,
        "Published"
    );
    // This would conflict if a published result failed to update the session baseline.
    assert!(session.save(&path).status.is_ok());
}

#[test]
fn save_rechecks_baseline_after_staging() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let path = initial.manifest().to_path_buf();
    let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(
        Failure::ExternalChange(path.clone()),
    ));
    assert!(session.open(&path).status.is_ok());
    let before = session.document().clone();
    let result = session.save(&path);
    assert_eq!(result.status.code, "PROJECT_CONFLICT");
    assert_eq!(fs::read(&path).unwrap(), b"external change");
    assert!(session.document().same_content(&before));
    assert_eq!(session.document().revision(), before.revision());
}

#[test]
fn package_failures_rollback_and_report_recovery_copy_when_rollback_fails() {
    for fault in [
        Failure::ShortWrite,
        Failure::FileSync,
        Failure::DirectorySync(1),
        Failure::DirectorySync(2),
        Failure::Rename(vec![1]),
        Failure::Rename(vec![2]),
        Failure::Rename(vec![2, 3]),
    ] {
        let rollback_fails = matches!(&fault, Failure::Rename(v) if v.len() == 2);
        let tmp = TestDirectory::new();
        let initial = tmp.saved_session();
        let destination = tmp.0.join("package");
        assert!(initial.export_package(&destination).status.is_ok());
        fs::write(destination.join("old-marker"), b"original").unwrap();
        let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(fault));
        assert!(session.open(initial.manifest()).status.is_ok());
        let result = session.export_package(&destination);
        assert!(!result.status.is_ok());
        assert!(!result.published);
        if rollback_fails {
            assert_eq!(result.status.code, "ROLLBACK_FAILED");
            let recovery = result.status.message.split("recover from ").nth(1).unwrap();
            assert_eq!(
                fs::read(Path::new(recovery).join("old-marker")).unwrap(),
                b"original"
            );
        } else {
            assert_eq!(
                fs::read(destination.join("old-marker")).unwrap(),
                b"original"
            );
        }
        assert!(initial.manifest().exists());
    }
}

#[test]
fn package_postcommit_sync_error_is_a_warning_not_a_failed_export() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let mut session =
        DocumentSession::with_filesystem(FailingFileSystem::new(Failure::DirectorySync(3)));
    assert!(session.open(initial.manifest()).status.is_ok());
    let destination = tmp.0.join("package");
    let result = session.export_package(&destination);
    assert!(result.status.is_ok() && result.published);
    assert!(!result.durable && !result.warnings.is_empty());
    assert!(destination.join("model.moc3").exists());
}

#[test]
fn package_writes_the_same_bytes_that_were_verified() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let asset = session.document().get_asset(&id(2)).unwrap();
    let source = session.root().join(&asset.source);
    let before = fs::read(&source).unwrap();
    let destination = tmp.0.join("package");
    let options = kasane_project::PackageOptions {
        asset_root: session.root(),
        destination: destination.clone(),
        validate: Some(Box::new(move |_| {
            fs::write(&source, b"changed after verification").unwrap();
            Ok(())
        })),
    };
    kasane_project::publish_package(session.document(), &options).unwrap();
    assert_eq!(
        fs::read(destination.join("textures/0.png")).unwrap(),
        before
    );
}

#[test]
fn external_source_asset_directory_cannot_be_an_export_destination() {
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let external = tmp.0.join("external");
    let path = external.join("source.png");
    let (bytes, _) = create_test_png(&path);
    assert!(session.replace_asset(&id(2), &path).status.is_ok());
    assert_eq!(
        session.export_package(&external).status.code,
        "INVALID_DESTINATION"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn a_failed_new_asset_write_can_be_retried_without_reusing_partial_bytes() {
    let tmp = TestDirectory::new();
    let (doc, input) = tmp.fixture();
    let destination = tmp.0.join("new-project");
    let failed = DocumentStore::with_filesystem(FailingFileSystem::new(Failure::ShortWrite));
    let result = failed.save(&doc, &input, &destination, None).0;
    assert_eq!(result.status.code, "PROJECT_IO");
    assert!(!destination.join("project.kasane.json").exists());
    let result = DocumentStore::new()
        .save(&doc, &input, &destination, None)
        .0;
    assert!(result.status.is_ok());
    assert!(DocumentStore::new()
        .open(&destination)
        .0
        .resources_complete());
}

#[test]
fn test_project_detachment_lifecycle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src_fixture = root.join("tests/fixtures/external_v50");

    let tmp = TestDirectory::new();
    let source_dir = tmp.0.join("external_source");
    fs::create_dir_all(&source_dir).unwrap();

    // 1. Copy fixture into source_dir
    let src_model3 = source_dir.join("model.model3.json");
    fs::copy(src_fixture.join("model.model3.json"), &src_model3).unwrap();
    fs::copy(
        src_fixture.join("model.moc3"),
        source_dir.join("model.moc3"),
    )
    .unwrap();
    fs::copy(
        src_fixture.join("texture_00.png"),
        source_dir.join("texture_00.png"),
    )
    .unwrap();

    // 2. Import into a session
    let mut session = DocumentSession::new();
    let (imp_res, report) = session.import_model3(&src_model3);
    assert!(imp_res.status.is_ok(), "{:?}", imp_res);
    assert!(report.is_some());
    let rep = report.unwrap();
    assert_eq!(rep.moc_version, 5);

    // 3. Save as project.kasane
    let project_path = tmp.0.join("detached_project");
    let save_res = session.save(&project_path);
    assert!(save_res.status.is_ok(), "{:?}", save_res);

    // 4. Detachment: completely remove source_dir
    fs::remove_dir_all(&source_dir).unwrap();
    assert!(!source_dir.exists());

    // 5. Reopen the project from project_path without source files
    let mut reopened_session = DocumentSession::new();
    let open_res = reopened_session.open(&project_path);
    assert!(open_res.status.is_ok(), "{:?}", open_res);
    assert!(open_res.resources_complete());

    // Verify document structure intact
    let doc = reopened_session.document();
    assert!(!doc.mesh_order().is_empty());
    assert!(!doc.parameter_order().is_empty());
    assert!(!doc.asset_order().is_empty());

    // 6. Edit the reopened project
    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.get_parameter(&param_id).unwrap().clone();
    p.default_value = (p.minimum + p.maximum) / 2.0;
    let edit_res = reopened_session.document_mut().replace_parameter(p);
    assert!(edit_res.status.is_ok(), "{:?}", edit_res);

    // 7. Re-export MOC3 package from reopened session
    let export_dir = tmp.0.join("re_exported_package");
    let exp_res = reopened_session.export_package(&export_dir);
    assert!(exp_res.status.is_ok(), "{:?}", exp_res);

    let exported_moc3 = export_dir.join("model.moc3");
    assert!(exported_moc3.exists());
    let moc3_bytes = fs::read(&exported_moc3).unwrap();
    let inspection = kasane_moc3::inspect_moc3(&moc3_bytes).expect("Exported MOC3 inspect failed");
    assert_eq!(inspection.version, kasane_moc3::Moc3Version::Version50);

    let exported_tex = export_dir.join("textures/0.png");
    assert!(exported_tex.exists());
    let tex_bytes = fs::read(&exported_tex).unwrap();
    assert!(!tex_bytes.is_empty());
}

#[test]
fn test_v42_project_lifecycle_detachment_and_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src_fixture = root.join("tests/fixtures/external_v42");

    let tmp = TestDirectory::new();
    let source_dir = tmp.0.join("v42_external_source");
    fs::create_dir_all(&source_dir).unwrap();

    let src_model3 = source_dir.join("model.model3.json");
    fs::copy(src_fixture.join("model.model3.json"), &src_model3).unwrap();
    fs::copy(
        src_fixture.join("model.moc3"),
        source_dir.join("model.moc3"),
    )
    .unwrap();
    fs::copy(
        src_fixture.join("texture_00.png"),
        source_dir.join("texture_00.png"),
    )
    .unwrap();

    // Import into session
    let mut session = DocumentSession::new();
    let (imp_res, report) = session.import_model3(&src_model3);
    assert!(imp_res.status.is_ok(), "{:?}", imp_res);
    let rep = report.expect("Report should exist");
    assert_eq!(rep.moc_version, 4);

    // Verify V42 properties
    assert_eq!(session.document().blend_binding_order().len(), 2);
    assert_eq!(session.document().blend_constraint_order().len(), 1);

    // Save as project.kasane
    let project_path = tmp.0.join("v42_detached_project");
    let save_res = session.save(&project_path);
    assert!(save_res.status.is_ok(), "{:?}", save_res);

    // Detach: remove original files
    fs::remove_dir_all(&source_dir).unwrap();
    assert!(!source_dir.exists());

    // Reopen without original files
    let mut reopened_session = DocumentSession::new();
    let open_res = reopened_session.open(&project_path);
    assert!(open_res.status.is_ok(), "{:?}", open_res);
    assert!(open_res.resources_complete());

    // Verify document contents preserved
    let doc = reopened_session.document();
    assert_eq!(doc.blend_binding_order().len(), 2);
    assert_eq!(doc.blend_constraint_order().len(), 1);
    assert_eq!(doc.mesh_order().len(), 1);

    // Export upgraded 5.0 package
    let export_dir = tmp.0.join("v42_exported_package");
    let exp_res = reopened_session.export_package(&export_dir);
    assert!(exp_res.status.is_ok(), "{:?}", exp_res);

    let exported_moc3 = export_dir.join("model.moc3");
    assert!(exported_moc3.exists());
    let moc3_bytes = fs::read(&exported_moc3).unwrap();
    let inspection = kasane_moc3::inspect_moc3(&moc3_bytes).expect("Exported MOC3 inspect failed");
    assert_eq!(inspection.version, kasane_moc3::Moc3Version::Version50);
    assert_eq!(inspection.counts.blend_bindings, 2);
    assert_eq!(inspection.counts.bs_constraints, 1);
}

#[test]
fn import_rejects_cycles_without_replacing_session() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = fs::read(root.join("tests/fixtures/external_v50/model.moc3")).unwrap();
    let offsets = kasane_moc3::inspect_moc3(&source).unwrap().section_offsets;
    let tmp = TestDirectory::new();
    let path = tmp.0.join("bad.moc3");
    let mut session = DocumentSession::new();
    let (result, _) =
        session.import_model3(&root.join("tests/fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok());
    let baseline = encode_project(session.document()).unwrap();
    let revision = session.document().revision();
    for (section, parent) in [(9, 0i32), (16, 0), (16, 1)] {
        let mut bytes = source.clone();
        let offset = offsets[section] as usize;
        bytes[offset..offset + 4].copy_from_slice(&parent.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        let (result, report) = session.import_bare_moc3(&path, &std::collections::HashMap::new());
        assert_eq!(result.status.code, "RELATIONSHIP_CYCLE");
        assert!(report.is_none());
        assert_eq!(encode_project(session.document()).unwrap(), baseline);
        assert_eq!(session.document().revision(), revision);
    }
}

#[test]
fn import_reports_truncated_pngs_for_both_entry_points() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let fixture = root.join("tests/fixtures/external_v50");
    let tmp = TestDirectory::new();
    let png = fs::read(fixture.join("texture_00.png")).unwrap();
    fs::copy(fixture.join("model.moc3"), tmp.0.join("model.moc3")).unwrap();
    fs::copy(
        fixture.join("model.model3.json"),
        tmp.0.join("model.model3.json"),
    )
    .unwrap();
    for length in [24, png.len() - 12] {
        let path = tmp.0.join("texture_00.png");
        fs::write(&path, &png[..length]).unwrap();
        assert!(decode_png(&png[..length]).is_err());
        let map = std::collections::HashMap::from([(0, path)]);
        for result in [
            kasane_moc3::import_from_bare_moc3(&fs::read(tmp.0.join("model.moc3")).unwrap(), &map)
                .unwrap(),
            kasane_moc3::import_from_model3_file(&tmp.0.join("model.model3.json")).unwrap(),
        ] {
            assert!(!result.textures_complete);
            assert!(result
                .diagnostics
                .iter()
                .any(|d| d.code == "CORRUPT_TEXTURE"));
        }
        let mut session = DocumentSession::new();
        let (result, _) = session.import_model3(&tmp.0.join("model.model3.json"));
        assert!(result.status.is_ok());
        assert!(!result.resources_complete());
        let (result, _) = session.import_bare_moc3(&tmp.0.join("model.moc3"), &map);
        assert!(result.status.is_ok());
        assert!(!result.resources_complete());
    }
}

#[test]
fn imported_canvas_and_drawing_groups_survive_save_reopen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let tmp = TestDirectory::new();
    let path = tmp.0.join("original.moc3");
    fs::copy(
        root.join("modules/purism-core/testdata/moc3/3d8e869a678a1dac.moc3"),
        &path,
    )
    .unwrap();
    let texture = tmp.0.join("original.png");
    create_test_png(&texture);
    let mut session = DocumentSession::new();
    let (result, _) = session.import_bare_moc3(&path, &HashMap::from([(0, texture.clone())]));
    assert!(result.status.is_ok());
    assert_eq!(session.document().canvas().flag, 0);
    assert_eq!(session.document().draw_order_groups().unwrap().len(), 1);
    assert!(!session.document().part_order().is_empty());
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(session.document(), &HashMap::new(), &mut before).is_ok());
    let before_bytes = encode_moc3(session.document()).unwrap().bytes;
    let destination = tmp.0.join("saved");
    assert!(session.save(&destination).status.is_ok());
    fs::remove_file(&path).unwrap();
    fs::remove_file(&texture).unwrap();
    let mut reopened = DocumentSession::new();
    assert!(reopened.open(&destination).resources_complete());
    assert_eq!(
        reopened.document().draw_order_groups(),
        session.document().draw_order_groups()
    );
    assert_eq!(reopened.document().canvas().flag, 0);
    assert!(!reopened.document().modified());
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(reopened.document(), &HashMap::new(), &mut after).is_ok());
    assert_eq!(before.drawables, after.drawables);
    assert_eq!(
        before_bytes,
        encode_moc3(reopened.document()).unwrap().bytes
    );

    // Drawing-group edits participate in saved-content tracking and validate atomically.
    let mut groups = reopened.document().draw_order_groups().unwrap().to_vec();
    groups[0].items.swap(0, 1);
    assert!(reopened
        .document_mut()
        .replace_draw_order_groups(groups.clone())
        .status
        .is_ok());
    assert!(reopened.document().modified());
    let baseline = encode_project(reopened.document()).unwrap();
    let duplicate = groups[0].items[0].clone();
    groups[0].items.push(duplicate);
    assert!(!reopened
        .document_mut()
        .replace_draw_order_groups(groups)
        .status
        .is_ok());
    assert_eq!(baseline, encode_project(reopened.document()).unwrap());
}

#[test]
fn test_project_v2_blendshape_and_glue_roundtrip() {
    use kasane_core::types::{
        BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
        DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ParameterKind,
    };

    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let bs_param_id = id(100);
    assert!(doc
        .create_parameter(kasane_core::types::Parameter {
            id: bs_param_id.clone(),
            runtime_id: "ParamMouthOpenBS".to_string(),
            name: "Mouth Open BS".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
            repeat: false,
        })
        .status
        .is_ok());

    let kt_id = id(101);
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            id: kt_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());

    let c_id = id(102);
    assert!(doc
        .create_blend_constraint(BlendShapeConstraint {
            id: c_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            weights: vec![0.0, 1.0],
        })
        .status
        .is_ok());

    let mesh_id = id(4); // from fixture_doc
    let mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    let v_count = mesh.vertex_ids.len();

    let mesh2_id = id(5);
    let mut m2 = mesh.clone();
    m2.id = mesh2_id.clone();
    m2.runtime_id = "ArtMeshB".to_string();
    assert!(doc.create_mesh(m2).status.is_ok());

    let bb_id = id(103);
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: bb_id.clone(),
            target_id: mesh_id.clone(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: kt_id.clone(),
            constraint_ids: vec![c_id.clone()],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(0.0, 0.0); v_count],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(1.0, 2.0); v_count],
                    ..Default::default()
                },
            ]),
        })
        .status
        .is_ok());

    let glue_parameter = id(105);
    assert!(doc
        .create_parameter(Parameter {
            id: glue_parameter.clone(),
            runtime_id: "GlueStrength".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let glue_id = id(104);
    assert!(doc
        .create_glue(Glue {
            id: glue_id.clone(),
            runtime_id: "Glue0".to_string(),
            name: "Glue 0".to_string(),
            mesh_a_id: mesh_id.clone(),
            mesh_b_id: mesh2_id.clone(),
            pairs: vec![GlueVertexPair {
                vertex_a: mesh.vertex_ids[0],
                vertex_b: doc.get_mesh(&mesh2_id).unwrap().vertex_ids[0],
                weight_a: 0.5,
                weight_b: 0.5,
            }],
            intensity: 1.0,
            binding: Some(kasane_core::types::GlueBinding {
                axes: vec![BindingAxis {
                    parameter_id: glue_parameter,
                    keys: vec![0.0, 1.0]
                }],
                keyforms: vec![
                    kasane_core::types::GlueKeyform { intensity: 0.0 },
                    kasane_core::types::GlueKeyform { intensity: 1.0 }
                ],
            }),
        })
        .status
        .is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    assert!(encoded.contains("\"format_version\": 4"));
    assert!(encoded.contains("blend_key_tables"));
    assert!(encoded.contains("blend_constraints"));
    assert!(encoded.contains("blend_bindings"));
    assert!(encoded.contains("glues"));

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert!(doc.same_content(&decoded));
    assert_eq!(decoded.blend_key_table_order(), &[kt_id]);
    assert_eq!(decoded.blend_constraint_order(), &[c_id]);
    assert_eq!(decoded.blend_binding_order(), &[bb_id]);
    assert_eq!(decoded.glue_order(), &[glue_id]);
}

#[test]
fn test_project_v1_v2_v3_migration_to_v4() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let doc = fixture_doc(sha1, sha2);

    for old_ver in [1, 2, 3] {
        let mut encoded = encode_project(&doc).unwrap();
        encoded = encoded.replace(
            "\"format_version\": 4",
            &format!("\"format_version\": {}", old_ver),
        );
        if old_ver == 1 {
            // v1 had no blend or glue fields
            encoded = encoded.replace("\"blend_key_tables\": [],\n", "");
            encoded = encoded.replace("\"blend_constraints\": [],\n", "");
            encoded = encoded.replace("\"blend_bindings\": [],\n", "");
            encoded = encoded.replace("\"glues\": [],\n", "");
        }

        let decoded = decode_project(&encoded)
            .unwrap_or_else(|e| panic!("Failed to decode v{} project: {:?}", old_ver, e));
        // Verify default value populated for repeat
        for p_id in decoded.parameter_order() {
            let p = decoded.get_parameter(p_id).unwrap();
            assert!(
                !p.repeat,
                "Migrated v{} parameter must have repeat: false by default",
                old_ver
            );
        }

        // Saving automatically upgrades to v4
        let re_encoded = encode_project(&decoded).expect("Failed to re-encode project");
        assert!(re_encoded.contains("\"format_version\": 4"));
    }
}

#[test]
fn test_legacy_reader_rejects_v4() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let doc = fixture_doc(sha1, sha2);
    let encoded_v4 = encode_project(&doc).unwrap();

    // Simulate an older reader that only accepts 1..=3
    let v: serde_json::Value = serde_json::from_str(&encoded_v4).unwrap();
    let ver = v["format_version"].as_u64().unwrap() as u32;
    let accepted_by_legacy = (1..=3).contains(&ver);
    assert!(!accepted_by_legacy, "Legacy reader (1..=3) must reject v4");
}

#[test]
fn test_project_v4_rejects_unimplemented_collections() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let doc = fixture_doc(sha1, sha2);
    let encoded_v4 = encode_project(&doc).unwrap();

    // Inject non-empty deformers collection (unsupported prototype relationships)
    let bad_project = encoded_v4.replace(
        "\"document\": {",
        "\"document\": {\n    \"deformers\": [{\"id\": \"def1\"}],",
    );
    let err = decode_project(&bad_project).expect_err("Non-empty deformers must be rejected");
    assert_eq!(err.code, "LEGACY_PROJECT");
    assert!(err.message.contains("Prototype relationships"));
}

#[test]
fn test_project_v4_preserves_offscreen() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let mesh_id = doc.mesh_order()[0].clone();
    let mut mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    mesh.raw_blend_mode = Some(262);
    assert!(doc.replace_mesh(mesh).status.is_ok());

    let os = Offscreen {
        id: id(10),
        runtime_id: "Offscreen0".to_string(),
        name: "Offscreen 0".to_string(),
        part_id: sid(1),
        blend_mode: 262,
        flags: 4,
        masks: vec![mesh_id.clone()],
        part_keyform_indices: vec![0],
        keyforms: vec![OffscreenKeyform {
            opacity: 0.8,
            multiply: Some([1.0, 0.5, 0.5]),
            screen: Some([0.1, 0.1, 0.1]),
        }],
    };
    assert!(doc.create_offscreen(os.clone()).status.is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    assert!(encoded.contains("\"offscreens\":"));
    assert!(encoded.contains("\"Offscreen0\""));
    assert!(encoded.contains("\"raw_blend_mode\": 262"));

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert_eq!(decoded.offscreen_count(), 1);
    let decoded_os = decoded
        .get_offscreen(&id(10))
        .expect("offscreen must exist");
    assert_eq!(decoded_os.runtime_id, "Offscreen0");
    assert_eq!(decoded_os.name, "Offscreen 0");
    assert_eq!(decoded_os.part_id, sid(1));
    assert_eq!(decoded_os.blend_mode, 262);
    assert_eq!(decoded_os.flags, 4);
    assert_eq!(decoded_os.masks, vec![mesh_id.clone()]);
    assert_eq!(decoded_os.part_keyform_indices, vec![0]);
    assert_eq!(decoded_os.keyforms.len(), 1);
    assert_eq!(decoded_os.keyforms[0].opacity, 0.8);
    assert_eq!(decoded_os.keyforms[0].multiply, Some([1.0, 0.5, 0.5]));
    assert_eq!(decoded_os.keyforms[0].screen, Some([0.1, 0.1, 0.1]));

    let decoded_mesh = decoded.get_mesh(&mesh_id).expect("mesh must exist");
    assert_eq!(decoded_mesh.raw_blend_mode, Some(262));
}

#[test]
fn test_project_v4_preserves_repeat_parameter() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.get_parameter(&param_id).unwrap().clone();
    p.repeat = true;
    assert!(doc.replace_parameter(p).status.is_ok());

    let encoded = encode_project(&doc).expect("encode_project failed");
    assert!(encoded.contains("\"format_version\": 4"));
    assert!(encoded.contains("\"repeat\": true"));

    let decoded = decode_project(&encoded).expect("decode_project failed");
    let decoded_param = decoded.get_parameter(&param_id).unwrap();
    assert!(
        decoded_param.repeat,
        "repeat: true must be preserved after project decode"
    );
}

#[test]
fn test_project_failure_preserves_document() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);
    let snap = doc.clone();

    // Attempt invalid parameter edit
    let mut bad_p = doc
        .get_parameter(&doc.parameter_order()[0])
        .unwrap()
        .clone();
    bad_p.maximum = bad_p.minimum - 1.0; // invalid range
    let res = doc.replace_parameter(bad_p);
    assert!(!res.status.is_ok());
    assert_eq!(res.status.code, "INVALID_PARAMETER");

    // Document state remains completely unaltered
    assert!(doc.same_content(&snap));
}

#[test]
fn test_project_v4_blendshape_glue_roundtrip() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);

    // Create a second mesh
    let mesh2 = Mesh {
        id: id(20),
        runtime_id: "Mesh2".to_string(),
        name: "Mesh 2".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![10, 20, 30],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(20.0, 10.0),
            Vec2::new(10.0, 20.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[10, 20, 30]],
        ..Default::default()
    };
    assert!(doc.create_mesh(mesh2).status.is_ok());

    let glue = Glue {
        id: id(21),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: id(4),
        mesh_b_id: id(20),
        pairs: vec![GlueVertexPair {
            vertex_a: 1,
            vertex_b: 10,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.3,
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let param_bs = Parameter {
        id: id(22),
        runtime_id: "ParamBS".to_string(),
        name: "Param BS".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::BlendShape,
        repeat: false,
    };
    assert!(doc.create_parameter(param_bs).status.is_ok());

    let bkt = BlendShapeKeyTable {
        id: id(23),
        parameter_id: id(22),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let constraint = BlendShapeConstraint {
        id: id(24),
        parameter_id: id(6),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.5],
    };
    assert!(doc.create_blend_constraint(constraint).status.is_ok());

    let binding = BlendShapeBinding {
        id: id(25),
        target_id: id(21),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: id(23),
        constraint_ids: vec![id(24)],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.5 },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    // Encode to project JSON
    let encoded = encode_project(&doc).expect("encode_project failed");
    assert!(encoded.contains("\"glue\""));
    assert!(encoded.contains("\"intensity\": 0.5"));

    // Decode and verify
    let decoded = decode_project(&encoded).expect("decode_project failed");
    let re_glue = decoded.get_glue(&id(21)).unwrap();
    assert_eq!(re_glue.intensity, 0.3);

    let re_binding = decoded.get_blend_binding(&id(25)).unwrap();
    assert_eq!(re_binding.target_kind, BlendShapeTargetKind::Glue);
    assert_eq!(re_binding.target_id, id(21));
    match &re_binding.keyforms {
        DeltaKeyforms::Glue(forms) => {
            assert_eq!(forms.len(), 2);
            assert_eq!(forms[0].intensity, 0.0);
            assert_eq!(forms[1].intensity, 0.5);
        }
        _ => panic!("Expected DeltaKeyforms::Glue"),
    }

    // Compare evaluation
    let mut preview = HashMap::new();
    preview.insert(id(6), 0.5);
    preview.insert(id(22), 0.8);

    let mut frame_orig = DrawableFrame::default();
    let mut frame_re = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame_orig).is_ok());
    assert!(evaluate_frame(&decoded, &preview, &mut frame_re).is_ok());

    assert_eq!(frame_orig.drawables.len(), frame_re.drawables.len());
    for (d1, d2) in frame_orig.drawables.iter().zip(&frame_re.drawables) {
        assert_eq!(d1.positions, d2.positions);
    }
}
