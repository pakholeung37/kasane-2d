use kasane_core::{TransformData, WarpTransform};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use kasane_core::types::{
    Appearance, BindingAxis, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter,
    Transform, Vec2,
};
use kasane_core::Document;
use kasane_moc3::{encode_moc3, import_from_bare_moc3, import_from_model3_file};

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
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

    let _ = doc.add_asset(ImageAsset {
        id: id(2),
        name: "Texture0".to_string(),
        source: "textures/0.png".to_string(),
        width: 64,
        height: 64,
        sha256: "0".repeat(64),
    });

    let param = Parameter {
        id: id(3),
        runtime_id: "ParamAngleX".to_string(),
        name: "Angle X".to_string(),
        minimum: -30.0,
        maximum: 30.0,
        default_value: 0.0,
        decimal_places: 1,
        ..Default::default()
    };
    assert!(doc.create_parameter(param).status.is_ok());

    let warp = Transform {
        id: id(4),
        name: "HeadWarp".to_string(),
        runtime_id: "WarpHead".to_string(),
        data: TransformData::Warp(WarpTransform {
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
        }),
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
                    screen: [0.2, 0.1, 0.0],
                },
                draw_order: Some(18.0),
            },
        ],
    };
    assert!(doc.create_binding(binding).status.is_ok());
    doc
}

#[path = "../tests/common/import_cases.rs"]
mod import_cases;

// Defaults, each parameter's bounds/keys/midpoints, and full grids for
// two/three-axis bindings. Other parameters retain their actual defaults.
fn sample_parameters(doc: &Document) -> Vec<Vec<f32>> {
    let defaults: Vec<f32> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap().default_value)
        .collect();
    let slots: HashMap<&str, usize> = doc
        .parameter_order()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let bindings: Vec<&[BindingAxis]> = doc
        .binding_order()
        .iter()
        .map(|id| doc.get_binding(id).unwrap().axes.as_slice())
        .chain(
            doc.scene_binding_order()
                .iter()
                .map(|id| doc.get_scene_binding(id).unwrap().axes.as_slice()),
        )
        .collect();
    let mut samples = vec![defaults.clone()];
    for (i, id) in doc.parameter_order().iter().enumerate() {
        let p = doc.get_parameter(id).unwrap();
        let mut keys = vec![p.minimum, p.maximum, p.default_value];
        for axes in &bindings {
            for axis in *axes {
                if &axis.parameter_id == id {
                    keys.extend(&axis.keys);
                }
            }
        }
        keys.sort_by(f32::total_cmp);
        keys.dedup();
        let mids: Vec<f32> = keys
            .windows(2)
            .map(|w| w[0] + (w[1] - w[0]) * 0.5)
            .collect();
        keys.extend(mids);
        for value in keys {
            let mut sample = defaults.clone();
            sample[i] = value;
            samples.push(sample);
        }
    }
    for axes in bindings {
        if !(2..=3).contains(&axes.len()) {
            continue;
        }
        let mut grid = vec![defaults.clone()];
        for axis in axes {
            let mut values = axis.keys.clone();
            values.extend(axis.keys.windows(2).map(|w| w[0] + (w[1] - w[0]) * 0.5));
            let mut next = Vec::new();
            for sample in grid {
                for &value in &values {
                    let mut sample = sample.clone();
                    sample[slots[axis.parameter_id.as_str()]] = value;
                    next.push(sample);
                }
            }
            grid = next;
        }
        samples.extend(grid);
    }
    let mut seen = std::collections::HashSet::new();
    samples.retain(|s| seen.insert(s.iter().map(|v| v.to_bits()).collect::<Vec<_>>()));
    samples
}

fn document_samples(doc: &Document, samples: &[Vec<f32>]) -> serde_json::Value {
    use kasane_core::{evaluate_frame, BlendMode, DrawableFrame};
    let mesh_slots: HashMap<&str, usize> = doc
        .mesh_order()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let frames: Vec<_> = samples.iter().map(|sample| {
        let values = doc.parameter_order().iter().cloned().zip(sample.iter().copied()).collect();
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(doc, &values, &mut frame);
        assert!(status.is_ok(), "{status:?}");
        frame.drawables.iter().map(|d| serde_json::json!({
            "runtime_id": d.runtime_id, "texture_slot": d.texture_slot,
            "draw_order": d.draw_order, "render_order": d.render_order,
            "opacity": d.opacity, "visible": d.visible, "enabled": d.enabled,
            "double_sided": d.double_sided, "inverted_mask": d.inverted_mask,
            "blend_mode": match d.blend_mode { BlendMode::Normal => 0, BlendMode::Additive => 1, BlendMode::Multiplicative => 2 },
            "positions": d.positions.iter().map(|p| [p.x,p.y]).collect::<Vec<_>>(),
            "uvs": d.uvs.iter().map(|p| [p.x,p.y]).collect::<Vec<_>>(),
            "indices": d.indices.as_ref(), "mask_indices": d.masks.iter().map(|id| mesh_slots[id.as_str()]).collect::<Vec<_>>(),
            "multiply_color": d.multiply_color, "screen_color": d.screen_color,
        })).collect::<Vec<_>>()
    }).collect();
    serde_json::json!({"samples": frames})
}

fn write_case(out_dir: &Path, name: &str, bytes: &[u8], doc: &Document) {
    let case_dir = out_dir.join(name);
    fs::create_dir_all(&case_dir).unwrap();
    let samples = sample_parameters(doc);
    fs::write(case_dir.join("orig.moc3"), bytes).unwrap();
    fs::write(
        case_dir.join("re_export.moc3"),
        encode_moc3(doc).unwrap().bytes,
    )
    .unwrap();
    fs::write(
        case_dir.join("samples.json"),
        serde_json::to_vec(&samples).unwrap(),
    )
    .unwrap();
    fs::write(
        case_dir.join("document.json"),
        serde_json::to_vec(&document_samples(doc, &samples)).unwrap(),
    )
    .unwrap();
    fs::write(
        case_dir.join("ppu.txt"),
        doc.canvas().pixels_per_unit.to_string(),
    )
    .unwrap();
    let parameters: Vec<_> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap())
        .collect();
    fs::write(case_dir.join("metadata.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "canvas": doc.canvas(), "parameters": parameters,
        "textures": doc.asset_order().iter().map(|id| doc.get_asset(id).unwrap()).collect::<Vec<_>>()
    })).unwrap()).unwrap();
    println!("{name}: {} samples", samples.len());
}

fn main() {
    let out_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/kasane/m3/fixtures"));
    fs::create_dir_all(&out_dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let tex_map = HashMap::from([(
        0,
        root.join("tests/fixtures/gpu/gpu-package/textures/0.png"),
    )]);
    let original = encode_moc3(&create_m1_fixture_doc()).unwrap();
    let decoded = import_from_bare_moc3(&original.bytes, &tex_map).unwrap();
    write_case(
        &out_dir,
        "case_m1_roundtrip",
        &original.bytes,
        &decoded.document,
    );

    // Required fixtures must fail explicitly if absent; never reuse stale output.
    let v33 = fs::read(root.join("modules/purism-core/testdata/moc3/3d8e869a678a1dac.moc3"))
        .expect("Required external v33 fixture missing");
    let decoded = import_from_bare_moc3(&v33, &tex_map).unwrap();
    write_case(&out_dir, "case_external_v33", &v33, &decoded.document);
    let v50 = fs::read(root.join("tests/fixtures/external_v50/model.moc3"))
        .expect("Required external v50 fixture missing");
    let decoded =
        import_from_model3_file(&root.join("tests/fixtures/external_v50/model.model3.json"))
            .unwrap();
    write_case(&out_dir, "case_external_v50", &v50, &decoded.document);
    for name in ["binding_zero", "color_window", "canvas_y"] {
        let bytes = import_cases::variant(&v50, name);
        let decoded = import_from_bare_moc3(&bytes, &tex_map).unwrap();
        write_case(&out_dir, &format!("case_{name}"), &bytes, &decoded.document);
    }
    fs::write(
        out_dir.join("cases.json"),
        serde_json::to_vec(&[
            "case_m1_roundtrip",
            "case_external_v33",
            "case_external_v50",
            "case_binding_zero",
            "case_color_window",
            "case_canvas_y",
        ])
        .unwrap(),
    )
    .unwrap();
}
