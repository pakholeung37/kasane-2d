mod common;
use common::purism::*;

use std::collections::HashMap;
use std::fs;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform,
    Parameter, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_moc3::{
    encode_moc3, import_from_bare_moc3, import_from_model3_file, import_from_model3_json,
    inspect_moc3, Moc3Version,
};

fn near(actual: f32, expected: f32, ppu: f32) {
    assert!(actual.is_finite(), "actual is not finite");
    assert!(expected.is_finite(), "expected is not finite");
    let e = (actual as f64 - expected as f64).abs();
    assert!(
        e <= 1e-5 + 1e-5 * (actual as f64).abs().max((expected as f64).abs())
            && e * ppu as f64 <= 0.05,
        "expected {}, actual {}, diff {}, pixel_diff {}",
        expected,
        actual,
        e,
        e * ppu as f64
    );
}

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest.ends_with("kasane-moc3") {
        manifest.parent().unwrap().parent().unwrap().to_path_buf()
    } else {
        manifest
    }
}

struct PurismModelInstance {
    _moc_buffer: AlignedBuffer,
    _model_buffer: AlignedBuffer,
    _moc: *mut c_void,
    model: *mut c_void,
}

impl PurismModelInstance {
    fn new(moc_bytes: &[u8]) -> Self {
        let mut moc_buf = AlignedBuffer::new(moc_bytes.len(), 64);
        unsafe {
            std::ptr::copy_nonoverlapping(
                moc_bytes.as_ptr(),
                moc_buf.as_mut_ptr(),
                moc_bytes.len(),
            );
            assert_eq!(
                csmHasMocConsistency(moc_buf.as_mut_ptr() as *mut c_void, moc_bytes.len() as u32),
                1,
                "Purism consistency check failed"
            );
            let moc =
                csmReviveMocInPlace(moc_buf.as_mut_ptr() as *mut c_void, moc_bytes.len() as u32);
            assert!(!moc.is_null(), "csmReviveMocInPlace returned null");
            let model_size = csmGetSizeofModel(moc);
            assert!(model_size > 0, "csmGetSizeofModel returned 0");
            let mut model_buf = AlignedBuffer::new(model_size as usize, 16);
            let model =
                csmInitializeModelInPlace(moc, model_buf.as_mut_ptr() as *mut c_void, model_size);
            assert!(!model.is_null(), "csmInitializeModelInPlace returned null");

            PurismModelInstance {
                _moc_buffer: moc_buf,
                _model_buffer: model_buf,
                _moc: moc,
                model,
            }
        }
    }

    fn set_parameter(&mut self, param_id: &str, value: f32) {
        unsafe {
            let count = csmGetParameterCount(self.model) as usize;
            let ids = csmGetParameterIds(self.model);
            let values = csmGetParameterValues(self.model);
            for i in 0..count {
                let id_str = c_str_to_str(*ids.add(i));
                if id_str == param_id {
                    *values.add(i) = value;
                    return;
                }
            }
        }
    }

    fn update(&mut self) {
        unsafe {
            csmUpdateModel(self.model);
        }
    }

    fn get_drawable(&self, runtime_id: &str) -> Option<PurismDrawableData> {
        unsafe {
            let count = csmGetDrawableCount(self.model) as usize;
            let ids = csmGetDrawableIds(self.model);
            let opacities = csmGetDrawableOpacities(self.model);
            let draw_orders = csmGetDrawableDrawOrders(self.model);
            let render_orders = csmGetRenderOrders(self.model);
            let positions = csmGetDrawableVertexPositions(self.model);
            let vertex_counts = csmGetDrawableVertexCounts(self.model);
            let mul_colors = csmGetDrawableMultiplyColors(self.model);
            let scr_colors = csmGetDrawableScreenColors(self.model);

            for i in 0..count {
                let id_str = c_str_to_str(*ids.add(i));
                if id_str == runtime_id {
                    let vc = *vertex_counts.add(i) as usize;
                    let pos_ptr = *positions.add(i);
                    let mut pts = Vec::with_capacity(vc);
                    for v in 0..vc {
                        let pt = *pos_ptr.add(v);
                        pts.push(Vec2::new(pt.x, pt.y));
                    }
                    let mul = *mul_colors.add(i);
                    let scr = *scr_colors.add(i);

                    let uv_ptr = *csmGetDrawableVertexUvs(self.model).add(i);
                    let uvs = (0..vc)
                        .map(|v| {
                            let p = *uv_ptr.add(v);
                            Vec2::new(p.x, p.y)
                        })
                        .collect();
                    let index_count = *csmGetDrawableIndexCounts(self.model).add(i) as usize;
                    let index_ptr = *csmGetDrawableIndices(self.model).add(i);
                    let indices = (0..index_count).map(|v| *index_ptr.add(v) as u32).collect();
                    return Some(PurismDrawableData {
                        uvs,
                        indices,
                        opacity: *opacities.add(i),
                        draw_order: *draw_orders.add(i),
                        render_order: *render_orders.add(i),
                        positions: pts,
                        multiply_color: [mul.x, mul.y, mul.z, mul.w],
                        screen_color: [scr.x, scr.y, scr.z, scr.w],
                    });
                }
            }
        }
        None
    }
}

#[allow(dead_code)]
struct PurismDrawableData {
    uvs: Vec<Vec2>,
    indices: Vec<u32>,
    opacity: f32,
    draw_order: i32,
    render_order: i32,
    positions: Vec<Vec2>,
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
}

fn create_m1_fixture_doc() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(320.0, 240.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "Tex0".to_string(),
            source: "textures/0.png".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    let p1 = Parameter {
        id: id(3),
        runtime_id: "ParamAngle".to_string(),
        name: "Angle".to_string(),
        minimum: -30.0,
        maximum: 30.0,
        default_value: 0.0,
        decimal_places: 4,
    };
    assert!(doc.create_parameter(p1).status.is_ok());

    let warp = Transform {
        id: id(4),
        runtime_id: "WarpRoot".to_string(),
        name: "WarpRoot".to_string(),
        kind: TransformKind::Warp,
        rows: 2,
        columns: 2,
        quad: true,
        points: vec![
            Vec2::new(120.0, 40.0),
            Vec2::new(320.0, 40.0),
            Vec2::new(520.0, 40.0),
            Vec2::new(120.0, 240.0),
            Vec2::new(320.0, 240.0),
            Vec2::new(520.0, 240.0),
            Vec2::new(120.0, 440.0),
            Vec2::new(320.0, 440.0),
            Vec2::new(520.0, 440.0),
        ],
        ..Default::default()
    };
    assert!(doc.create_transform(warp).status.is_ok());

    let m1 = Mesh {
        id: id(5),
        name: "QuadMesh".to_string(),
        runtime_id: "ArtMeshQuad".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(220.0, 140.0),
            Vec2::new(420.0, 140.0),
            Vec2::new(420.0, 340.0),
            Vec2::new(220.0, 340.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        deformer_id: id(4),
        appearance: Appearance {
            opacity: 1.0,
            multiply: [0.9, 0.8, 0.7],
            screen: [0.1, 0.2, 0.3],
        },
        draw_order: Some(15.0),
        ..Default::default()
    };
    assert!(doc.create_mesh(m1).status.is_ok());

    let binding = MeshBinding {
        id: id(6),
        mesh_id: id(5),
        axes: vec![BindingAxis {
            parameter_id: id(3),
            keys: vec![-30.0, 0.0, 30.0],
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![-30.0],
                positions: vec![
                    Vec2::new(200.0, 150.0),
                    Vec2::new(400.0, 130.0),
                    Vec2::new(410.0, 330.0),
                    Vec2::new(210.0, 350.0),
                ],
                appearance: Appearance {
                    opacity: 0.8,
                    multiply: [0.8, 0.7, 0.6],
                    screen: [0.0, 0.1, 0.2],
                },
                draw_order: Some(12.0),
            },
            MeshKeyform {
                keys: vec![0.0],
                positions: vec![
                    Vec2::new(220.0, 140.0),
                    Vec2::new(420.0, 140.0),
                    Vec2::new(420.0, 340.0),
                    Vec2::new(220.0, 340.0),
                ],
                appearance: Appearance {
                    opacity: 1.0,
                    multiply: [0.9, 0.8, 0.7],
                    screen: [0.1, 0.2, 0.3],
                },
                draw_order: Some(15.0),
            },
            MeshKeyform {
                keys: vec![30.0],
                positions: vec![
                    Vec2::new(240.0, 130.0),
                    Vec2::new(440.0, 150.0),
                    Vec2::new(430.0, 350.0),
                    Vec2::new(230.0, 330.0),
                ],
                appearance: Appearance {
                    opacity: 0.9,
                    multiply: [1.0, 0.9, 0.8],
                    screen: [0.2, 0.3, 0.4],
                },
                draw_order: Some(18.0),
            },
        ],
    };
    assert!(doc.create_binding(binding).status.is_ok());

    doc
}

#[test]
fn test_import_m1_roundtrip() {
    let doc_orig = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc_orig).expect("Export failed");
    assert!(!encoded.bytes.is_empty());

    // Import back via bare moc3
    let mut tex_map = HashMap::new();
    // Use an existing test texture
    tex_map.insert(
        0,
        PathBuf::from("tests/fixtures/gpu/gpu-package/textures/0.png"),
    );
    let import_res = import_from_bare_moc3(&encoded.bytes, &tex_map).expect("Import failed");
    let doc_imported = &import_res.document;

    // Verify canvas
    assert_eq!(doc_imported.canvas().width, doc_orig.canvas().width);
    assert_eq!(doc_imported.canvas().height, doc_orig.canvas().height);
    near(
        doc_imported.canvas().origin.x,
        doc_orig.canvas().origin.x,
        100.0,
    );
    near(
        doc_imported.canvas().origin.y,
        doc_orig.canvas().origin.y,
        100.0,
    );
    assert_eq!(
        doc_imported.canvas().pixels_per_unit,
        doc_orig.canvas().pixels_per_unit
    );

    // Verify parameters count and values
    assert_eq!(doc_imported.parameter_order().len(), 1);
    let p_imp_id = &doc_imported.parameter_order()[0];
    let p_imp = doc_imported.get_parameter(p_imp_id).unwrap();
    assert_eq!(p_imp.runtime_id, "ParamAngle");
    assert_eq!(p_imp.minimum, -30.0);
    assert_eq!(p_imp.maximum, 30.0);
    assert_eq!(p_imp.default_value, 0.0);

    // Compare evaluation across sample values (-30, -15, 0, 15, 30)
    for &val in &[-30.0, -15.0, 0.0, 15.0, 30.0] {
        let mut orig_frame = DrawableFrame::default();
        let mut imp_frame = DrawableFrame::default();

        let mut p_orig_map = HashMap::new();
        p_orig_map.insert(id(3), val);
        evaluate_frame(&doc_orig, &p_orig_map, &mut orig_frame);

        let mut p_imp_map = HashMap::new();
        p_imp_map.insert(p_imp_id.clone(), val);
        evaluate_frame(doc_imported, &p_imp_map, &mut imp_frame);

        assert_eq!(orig_frame.drawables.len(), imp_frame.drawables.len());
        let d_orig = &orig_frame.drawables[0];
        let d_imp = &imp_frame.drawables[0];

        assert_eq!(d_imp.runtime_id, "ArtMeshQuad");
        near(d_imp.opacity, d_orig.opacity, 100.0);
        near(d_imp.draw_order as f32, d_orig.draw_order as f32, 100.0);

        for (p_i, p_o) in d_imp.positions.iter().zip(d_orig.positions.iter()) {
            near(p_i.x, p_o.x, 100.0);
            near(p_i.y, p_o.y, 100.0);
        }
    }

    // Re-export imported document to MOC3 and verify in PurismCore
    let re_encoded = encode_moc3(doc_imported).expect("Re-export failed");
    let mut purism = PurismModelInstance::new(&re_encoded.bytes);
    purism.set_parameter("ParamAngle", 15.0);
    purism.update();
    let d_purism = purism
        .get_drawable("ArtMeshQuad")
        .expect("Drawable not found");
    assert!(d_purism.positions.len() == 4);
    assert!(d_purism.opacity > 0.0);
}

#[test]
fn test_import_external_v33_3d8e869a() {
    let root = workspace_root();
    let moc_path = root.join("modules/purism-core/testdata/moc3/3d8e869a678a1dac.moc3");
    let bytes = fs::read(&moc_path).expect("Failed to read 3d8e869a moc3");

    // 1. Inspect
    let inspection = inspect_moc3(&bytes).expect("Inspection failed");
    assert_eq!(inspection.version, Moc3Version::Version33);
    assert_eq!(inspection.counts.parts, 10);
    assert_eq!(inspection.counts.deformers, 17);
    assert_eq!(inspection.counts.warps, 15);
    assert_eq!(inspection.counts.rotations, 2);
    assert_eq!(inspection.counts.art_meshes, 29);
    assert_eq!(inspection.counts.parameters, 32);

    // 2. Import
    let mut tex_map = HashMap::new();
    tex_map.insert(
        0,
        root.join("tests/fixtures/gpu/gpu-package/textures/0.png"),
    );
    let res = import_from_bare_moc3(&bytes, &tex_map).expect("Import failed");
    let doc = &res.document;

    assert_eq!(doc.sorted_parts().len(), 10);
    assert_eq!(doc.sorted_transforms().len(), 17);
    assert_eq!(doc.mesh_order().len(), 29);
    assert_eq!(doc.parameter_order().len(), 32);

    let defaults = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap().default_value)
        .collect::<Vec<_>>();
    let mut toggle = defaults.clone();
    let index = doc
        .parameter_order()
        .iter()
        .position(|id| doc.get_parameter(id).unwrap().runtime_id == "MouseToggle")
        .unwrap();
    toggle[index] = 1.0;
    assert_runtime_matches(doc, &bytes, &[defaults, toggle]);

    // 3. Compare with PurismCore running the original model
    let mut purism_orig = PurismModelInstance::new(&bytes);
    purism_orig.update();

    let mut doc_frame = DrawableFrame::default();
    evaluate_frame(doc, &HashMap::new(), &mut doc_frame);

    let ppu = doc.canvas().pixels_per_unit;

    // 4. Re-export imported Document to v50 MOC3 and verify in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export of v33 model to v50 failed");

    let mut purism_re = PurismModelInstance::new(&re_export.bytes);
    purism_re.update();

    for d in &doc_frame.drawables {
        let p_draw_orig = purism_orig.get_drawable(&d.runtime_id).unwrap();
        let p_draw_re = purism_re.get_drawable(&d.runtime_id);
        assert!(
            p_draw_re.is_some(),
            "Drawable {} missing in re-exported model",
            d.runtime_id
        );
        let pd_re = p_draw_re.unwrap();
        near(pd_re.opacity, p_draw_orig.opacity, ppu);

        for (po, pr) in p_draw_orig.positions.iter().zip(pd_re.positions.iter()) {
            near(pr.x, po.x, ppu);
            near(pr.y, po.y, ppu);
        }
    }
}

#[test]
fn test_import_external_v50() {
    let root = workspace_root();
    let model3_path = root.join("tests/fixtures/external_v50/model.model3.json");

    let res = import_from_model3_file(&model3_path).expect("Import external v50 failed");
    let doc = &res.document;

    // Verify unimported attachments collected
    assert!(
        res.report
            .unimported_attachments
            .iter()
            .any(|s| s.contains("Physics")),
        "Physics attachment should be reported"
    );
    assert!(
        res.report
            .unimported_attachments
            .iter()
            .any(|s| s.contains("Motions")),
        "Motions attachment should be reported"
    );

    // Verify topology
    assert_eq!(doc.parameter_order().len(), 2);
    assert_eq!(doc.sorted_transforms().len(), 2); // 1 Warp, 1 Rotation
    assert_eq!(doc.mesh_order().len(), 1);

    // Verify nested deformer hierarchy: RotChild has parent WarpRoot
    let rot_id = &doc.sorted_transforms()[1];
    let rot = doc.get_transform(rot_id).unwrap();
    assert_eq!(rot.runtime_id, "RotChild");
    let warp_id = &doc.sorted_transforms()[0];
    assert_eq!(rot.parent_id, *warp_id);

    // Verify explicit v50 multiply and screen colors are preserved
    let mesh = doc.get_mesh(&doc.mesh_order()[0]).unwrap();
    let b = doc.binding_for_mesh(&mesh.id).unwrap();
    assert_eq!(b.keyforms.len(), 6);
    // Keyform 0 color from create_v50_external_fixture: mul_r = 0.5, scr_r = 0.0
    near(b.keyforms[0].appearance.multiply[0], 0.5, 100.0);
    near(b.keyforms[0].appearance.screen[0], 0.0, 100.0);
    // Keyform 5 color: mul_r = 0.5 + 5 * 0.08 = 0.9, scr_r = 0.05 * 5 = 0.25
    near(b.keyforms[5].appearance.multiply[0], 0.9, 100.0);
    near(b.keyforms[5].appearance.screen[0], 0.25, 100.0);

    // Re-export and verify in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export failed");
    let mut purism = PurismModelInstance::new(&re_export.bytes);
    purism.set_parameter("ParamX", 0.5);
    purism.set_parameter("ParamY", -0.5);
    purism.update();
    let d = purism.get_drawable("MeshQuad").expect("Drawable not found");
    assert_eq!(d.positions.len(), 4);
}

fn exported_drawable(doc: &Document) -> PurismDrawableData {
    let exported = encode_moc3(doc).unwrap();
    let mut runtime = PurismModelInstance::new(&exported.bytes);
    runtime.update();
    runtime.get_drawable("MeshQuad").unwrap()
}

#[test]
fn test_post_import_editing() {
    let source = fs::read(workspace_root().join("tests/fixtures/external_v50/model.moc3")).unwrap();
    let mut doc = import_from_bare_moc3(&source, &HashMap::new())
        .unwrap()
        .document;
    let mesh_id = doc.mesh_order()[0].clone();
    let before = exported_drawable(&doc);

    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    binding.keyforms[1].positions[0].x += 0.17;
    assert!(doc.replace_binding(binding).status.is_ok());
    let mesh_edited = exported_drawable(&doc);
    assert_ne!(
        mesh_edited.positions, before.positions,
        "Mesh edit must affect runtime geometry"
    );

    let warp_id = doc.sorted_transforms()[0].clone();
    let mut warp = doc.get_transform(&warp_id).unwrap().clone();
    warp.points[0].x += 30.0;
    assert!(doc.replace_transform(warp).status.is_ok());
    let warp_edited = exported_drawable(&doc);
    assert_ne!(
        warp_edited.positions, mesh_edited.positions,
        "Warp edit must affect runtime geometry"
    );

    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    for keyform in &mut binding.keyforms {
        keyform.appearance.opacity = 0.42;
        keyform.draw_order = Some(42.0);
    }
    assert!(doc.replace_binding(binding).status.is_ok());
    let mut mesh = doc.get_mesh(&mesh_id).unwrap().clone();
    mesh.blend_mode = BlendMode::Additive;
    assert!(doc.replace_mesh(mesh).status.is_ok());
    let draw_edited = exported_drawable(&doc);
    near(draw_edited.opacity, 0.42, 1.0);
    assert_eq!(draw_edited.draw_order, 42);
    let artifact = encode_moc3(&doc).unwrap();
    let reimported = import_from_bare_moc3(&artifact.bytes, &HashMap::new()).unwrap();
    assert_eq!(
        reimported
            .document
            .get_mesh(&reimported.document.mesh_order()[0])
            .unwrap()
            .blend_mode,
        BlendMode::Additive
    );
    assert_runtime_matches(
        &doc,
        &artifact.bytes,
        &[vec![0.0, 0.0], vec![-1.0, 1.0], vec![0.5, -0.5]],
    );
}

#[test]
fn test_structural_editing() {
    let doc_orig = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc_orig).expect("Export failed");
    let mut tex_map = HashMap::new();
    tex_map.insert(
        0,
        PathBuf::from("tests/fixtures/gpu/gpu-package/textures/0.png"),
    );
    let mut res = import_from_bare_moc3(&encoded.bytes, &tex_map).expect("Import failed");
    let doc = &mut res.document;

    // Add new parameter
    let new_p = Parameter {
        id: id(100),
        runtime_id: "ParamNew".to_string(),
        name: "NewParam".to_string(),
        minimum: 0.0,
        maximum: 10.0,
        default_value: 5.0,
        decimal_places: 2,
    };
    assert!(doc.create_parameter(new_p).status.is_ok());

    // Add new mesh
    let new_m = Mesh {
        id: id(101),
        name: "NewMesh".to_string(),
        runtime_id: "ArtMeshNew".to_string(),
        texture_asset_id: doc.asset_order()[0].clone(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(200.0, 100.0),
            Vec2::new(150.0, 200.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    assert!(doc.create_mesh(new_m).status.is_ok());

    // Re-export and verify both meshes exist in PurismCore
    let re_export = encode_moc3(doc).expect("Re-export with new mesh failed");
    let mut purism = PurismModelInstance::new(&re_export.bytes);
    purism.update();

    assert!(purism.get_drawable("ArtMeshQuad").is_some());
    assert!(purism.get_drawable("ArtMeshNew").is_some());
}

#[test]
fn test_unsupported_features_rejected() {
    // 1. mao_pro.moc3 has Glues (7) and BlendShapes (34 targets/tables), which are valid in M3B
    let root = workspace_root();
    let mao_path = root.join("demos/gd-cubism-demo/assets/live2d/mao/runtime/mao_pro.moc3");
    let mao_path_bench = root.join("benchmarks/cubism-matrix/assets/live2d/mao/runtime/mao_pro.moc3");
    let target_path = if mao_path.exists() {
        mao_path
    } else {
        mao_path_bench
    };
    assert!(target_path.exists(), "mao_pro.moc3 must exist for M3B acceptance");
    let bytes = fs::read(&target_path).expect("Failed to read mao_pro.moc3");
    let report = inspect_moc3(&bytes).expect("mao_pro must pass structural inspection in M3B");
    assert_eq!(report.version, Moc3Version::Version50);
    assert_eq!(report.counts.glues, 7);
    assert_eq!(report.counts.parameters, 128);
    assert_eq!(report.counts.art_meshes, 260);
    assert_eq!(report.counts.bs_glues, 0);
    assert_eq!(report.counts.offscreens, 0);
    assert!(report.unsupported_features.is_empty());

    // 2. Unknown version rejection
    let bad_ver_bytes = create_m1_fixture_doc();
    let mut bytes = encode_moc3(&bad_ver_bytes).unwrap().bytes;
    bytes[4] = 99; // unknown version
    let err = inspect_moc3(&bytes).expect_err("Version 99 must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_VERSION");

    // 3. Cyclic parameter rejection
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();
    let offsets = &inspect_moc3(&encoded.bytes).unwrap().section_offsets;
    let counts_off = offsets[0] as usize;
    let rep_off = offsets[54] as usize;

    let mut cyclic_bytes = encoded.bytes.clone();
    cyclic_bytes[rep_off] = 1; // set repeat = 1
    let err = inspect_moc3(&cyclic_bytes).expect_err("Cyclic parameter must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_FEATURE");
    assert!(err.message.contains("repeat") || err.message.contains("cyclic"));

    // 4. BlendShape Glue rejection (bs_glues > 0)
    let mut bs_glue_bytes = encoded.bytes.clone();
    bs_glue_bytes.resize(bs_glue_bytes.len() + 2048, 0);
    bs_glue_bytes[counts_off + 34 * 4] = 1;
    let err = inspect_moc3(&bs_glue_bytes).expect_err("BlendShape Glue must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_FEATURE");
    assert!(err.message.contains("BlendShape Glue"));

    // 5. Offscreen rejection (offscreens > 0)
    let mut offscreen_bytes = encoded.bytes.clone();
    offscreen_bytes.resize(offscreen_bytes.len() + 2048, 0);
    offscreen_bytes[counts_off + 35 * 4] = 1;
    let err = inspect_moc3(&offscreen_bytes).expect_err("Offscreen must be rejected");
    assert_eq!(err.code, "UNSUPPORTED_FEATURE");
    assert!(err.message.contains("Offscreen"));

    // 6. Multiple unsupported features reported together (F01 requirement)
    let mut multi_bytes = encoded.bytes.clone();
    multi_bytes.resize(multi_bytes.len() + 2048, 0);
    multi_bytes[rep_off] = 1;
    multi_bytes[counts_off + 34 * 4] = 2;
    multi_bytes[counts_off + 35 * 4] = 3;
    let err = inspect_moc3(&multi_bytes).expect_err("All unsupported features must be reported");
    assert_eq!(err.code, "UNSUPPORTED_FEATURE");
    assert!(err.message.contains("cyclic") || err.message.contains("repeat"));
    assert!(err.message.contains("BlendShape Glue"));
    assert!(err.message.contains("Offscreen"));
}

#[test]
fn test_malformed_and_corrupt_files() {
    // 1. Truncated header
    let short = vec![0u8; 32];
    assert_eq!(inspect_moc3(&short).unwrap_err().code, "BUFFER_TOO_SMALL");

    // 2. Invalid magic
    let mut bad_magic = vec![0u8; 100];
    bad_magic[0..4].copy_from_slice(b"NOPE");
    bad_magic[4] = 5;
    assert_eq!(inspect_moc3(&bad_magic).unwrap_err().code, "INVALID_MAGIC");

    // 3. Bad counts offset oob
    let mut bad_offsets = vec![0u8; 1000];
    bad_offsets[0..4].copy_from_slice(b"MOC3");
    bad_offsets[4] = 5;
    bad_offsets[64..68].copy_from_slice(&999999u32.to_le_bytes()); // count_info offset out of bounds
    assert!(inspect_moc3(&bad_offsets).is_err());
}

#[test]
fn test_resource_mapping() {
    let doc = create_m1_fixture_doc();
    let encoded = encode_moc3(&doc).unwrap();

    // Bare moc3 with empty texture map: should succeed with unmapped texture warning
    let empty_map = HashMap::new();
    let res = import_from_bare_moc3(&encoded.bytes, &empty_map).unwrap();
    assert!(!res.textures_complete);
    assert!(res
        .diagnostics
        .iter()
        .any(|d| d.code == "UNMAPPED_TEXTURE_SLOT"));

    // model3.json without textures: reports diagnostic
    let json_str = r#"{
        "Version": 3,
        "FileReferences": {
            "Moc": "model.moc3"
        }
    }"#;
    let res = import_from_model3_json(json_str, Path::new("."));
    assert!(res.is_err() || !res.unwrap().textures_complete);
}

#[path = "common/import_cases.rs"]
mod import_cases;

fn assert_runtime_matches(doc: &Document, bytes: &[u8], samples: &[Vec<f32>]) {
    let re_export = encode_moc3(doc).unwrap();
    let mut original = PurismModelInstance::new(bytes);
    let mut exported = PurismModelInstance::new(&re_export.bytes);
    for sample in samples {
        let mut values = HashMap::new();
        for (id, value) in doc.parameter_order().iter().zip(sample) {
            let parameter = doc.get_parameter(id).unwrap();
            values.insert(id.clone(), *value);
            original.set_parameter(&parameter.runtime_id, *value);
            exported.set_parameter(&parameter.runtime_id, *value);
        }
        original.update();
        exported.update();
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(doc, &values, &mut frame).is_ok());
        for d in frame.drawables {
            for actual in [
                original.get_drawable(&d.runtime_id).unwrap(),
                exported.get_drawable(&d.runtime_id).unwrap(),
            ] {
                assert_eq!(d.indices, actual.indices);
                assert_eq!(d.uvs.len(), actual.uvs.len());
                for (a, b) in d.uvs.iter().zip(&actual.uvs) {
                    near(a.x, b.x, 1.0);
                    near(a.y, b.y, 1.0);
                }
                assert_eq!(d.positions.len(), actual.positions.len());
                for (a, b) in d.positions.iter().zip(&actual.positions) {
                    near(a.x, b.x, doc.canvas().pixels_per_unit);
                    near(a.y, b.y, doc.canvas().pixels_per_unit);
                }
                near(d.opacity, actual.opacity, 1.0);
                assert_eq!(d.draw_order, actual.draw_order);
                assert_eq!(d.render_order, actual.render_order);
                for c in 0..4 {
                    near(d.multiply_color[c], actual.multiply_color[c], 1.0);
                    near(d.screen_color[c], actual.screen_color[c], 1.0);
                }
            }
        }
    }
}

#[test]
fn import_regressions_match_document_and_runtime() {
    let source = fs::read(workspace_root().join("tests/fixtures/external_v50/model.moc3")).unwrap();
    let samples: Vec<Vec<f32>> = [-1.0, -0.5, 0.0, 0.5, 1.0]
        .into_iter()
        .flat_map(|x| [-1.0, 0.0, 1.0].into_iter().map(move |y| vec![x, y]))
        .collect();
    for name in ["binding_zero", "color_window", "canvas_y"] {
        let bytes = import_cases::variant(&source, name);
        let imported = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap();
        assert_eq!(imported.document.binding_order().len(), 1, "{name}");
        assert_eq!(
            imported
                .document
                .get_binding(&imported.document.binding_order()[0])
                .unwrap()
                .keyforms
                .len(),
            6
        );
        assert_runtime_matches(&imported.document, &bytes, &samples);
    }
}

#[test]
fn cyclic_parent_references_are_errors() {
    let source = fs::read(workspace_root().join("tests/fixtures/external_v50/model.moc3")).unwrap();
    for (section, parent, kind) in [(9, 0, "Part"), (16, 0, "Deformer"), (16, 1, "Deformer")] {
        let mut bytes = source.clone();
        import_cases::set_i32(&mut bytes, section, 0, parent);
        let error = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
        assert_eq!(error.code, "RELATIONSHIP_CYCLE");
        assert!(error.message.contains(kind));
    }
}

#[test]
fn invalid_color_windows_are_rejected() {
    let mut bytes =
        fs::read(workspace_root().join("tests/fixtures/external_v50/model.moc3")).unwrap();
    import_cases::set_i32(&mut bytes, 107, 0, 1); // six keyforms no longer fit
    let error = import_from_bare_moc3(&bytes, &HashMap::new()).unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "INVALID_COLOR_REFERENCE" | "FILE_CORRUPT"
    ));
}

#[test]
fn rotation_edit_rebinding_and_deletion_change_export() {
    let bytes = fs::read(workspace_root().join("tests/fixtures/external_v50/model.moc3")).unwrap();
    let mut doc = import_from_bare_moc3(&bytes, &HashMap::new())
        .unwrap()
        .document;
    let rot_id = doc
        .sorted_transforms()
        .into_iter()
        .find(|id| doc.get_transform(id).unwrap().kind == TransformKind::Rotation)
        .unwrap();
    let mut rot = doc.get_transform(&rot_id).unwrap().clone();
    rot.rotation.angle = 23.0;
    rot.rotation.scale = 0.7;
    assert!(doc.replace_transform(rot).status.is_ok());
    let changed = encode_moc3(&doc).unwrap();
    let mut before = PurismModelInstance::new(&bytes);
    let mut after = PurismModelInstance::new(&changed.bytes);
    before.update();
    after.update();
    assert_ne!(
        before.get_drawable("MeshQuad").unwrap().positions,
        after.get_drawable("MeshQuad").unwrap().positions
    );
    assert_runtime_matches(&doc, &changed.bytes, &[vec![0.0, 0.0], vec![0.5, 1.0]]);

    let old_param = doc.parameter_order()[0].clone();
    let mut param = doc.get_parameter(&old_param).unwrap().clone();
    param.id = id(200);
    param.runtime_id = "ReboundParameter".into();
    assert!(doc.create_parameter(param).status.is_ok());
    let mesh_id = doc.mesh_order()[0].clone();
    let mut binding = doc.binding_for_mesh(&mesh_id).unwrap().clone();
    binding.axes[0].parameter_id = id(200);
    assert!(doc.replace_binding(binding).status.is_ok());
    assert!(doc.erase_object(&old_param).status.is_ok());
    let rebound = encode_moc3(&doc).unwrap();
    let reimported = import_from_bare_moc3(&rebound.bytes, &HashMap::new())
        .unwrap()
        .document;
    assert_eq!(reimported.parameter_order().len(), 2);
    assert!(reimported.parameter_order().iter().any(|p| reimported
        .get_parameter(p)
        .unwrap()
        .runtime_id
        == "ReboundParameter"));
    let mut runtime = PurismModelInstance::new(&rebound.bytes);
    runtime.set_parameter("ReboundParameter", -1.0);
    runtime.update();
    let left = runtime.get_drawable("MeshQuad").unwrap().positions;
    runtime.set_parameter("ReboundParameter", 1.0);
    runtime.update();
    assert_ne!(left, runtime.get_drawable("MeshQuad").unwrap().positions);

    let mut survivor = doc.get_mesh(&mesh_id).unwrap().clone();
    survivor.id = id(201);
    survivor.runtime_id = "SurvivingMesh".into();
    assert!(doc.create_mesh(survivor).status.is_ok());
    let binding_id = doc.binding_for_mesh(&mesh_id).unwrap().id.clone();
    assert!(doc.erase_object(&binding_id).status.is_ok());
    assert!(doc.erase_object(&mesh_id).status.is_ok());
    let empty = encode_moc3(&doc).unwrap();
    assert_eq!(inspect_moc3(&empty.bytes).unwrap().counts.art_meshes, 1);
    assert!(PurismModelInstance::new(&empty.bytes)
        .get_drawable("MeshQuad")
        .is_none());
}
