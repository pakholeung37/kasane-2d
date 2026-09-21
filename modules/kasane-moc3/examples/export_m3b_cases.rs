use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{BlendMode, Parameter};
use kasane_core::Document;
use kasane_moc3::{encode_moc3, import_from_bare_moc3};

fn sample_parameters_mao(doc: &Document) -> Vec<Vec<f32>> {
    let params: Vec<_> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap())
        .collect();
    let param_map: HashMap<String, usize> = params
        .iter()
        .enumerate()
        .map(|(i, p)| (p.runtime_id.clone(), i))
        .collect();

    let defaults: Vec<f32> = params.iter().map(|p| p.default_value).collect();

    let make_sample = |overrides: &[(&str, f32)]| -> Vec<f32> {
        let mut s = defaults.clone();
        for &(k, v) in overrides {
            if let Some(&idx) = param_map.get(k) {
                s[idx] = v;
            }
        }
        s
    };

    let mut samples = Vec::new();
    // 0: defaults
    samples.push(make_sample(&[]));
    // 1: vowel_A (constraint 0)
    samples.push(make_sample(&[("ParamA", 1.0)]));
    // 2: vowel_I (constraint 1)
    samples.push(make_sample(&[("ParamI", 1.0)]));
    // 3: vowel_U (constraint 2)
    samples.push(make_sample(&[("ParamU", 1.0)]));
    // 4: vowel_E (constraint 3)
    samples.push(make_sample(&[("ParamE", 1.0)]));
    // 5: vowel_O (constraint 4)
    samples.push(make_sample(&[("ParamO", 1.0)]));
    // 6: mouth_down (constraint 5)
    samples.push(make_sample(&[("ParamMouthDown", 1.0)]));
    // 7: mouth_angry (constraint 6)
    samples.push(make_sample(&[("ParamMouthAngry", 1.0)]));
    // 8: mouth_combo (multi-constraint interaction)
    samples.push(make_sample(&[("ParamA", 0.5), ("ParamMouthDown", 0.5)]));
    // 9: head_angles (multi-axis deformer)
    samples.push(make_sample(&[("ParamAngleX", 30.0), ("ParamAngleY", 30.0), ("ParamAngleZ", 30.0)]));
    // 10: head_angles_neg (negative range)
    samples.push(make_sample(&[("ParamAngleX", -30.0), ("ParamAngleY", -30.0), ("ParamAngleZ", -30.0)]));
    // 11: body_angles
    samples.push(make_sample(&[("ParamBodyAngleX", 10.0), ("ParamBodyAngleY", 10.0), ("ParamBodyAngleZ", 10.0)]));
    // 12: eyes_and_rabbit (rotation deformer blend shape + colors)
    samples.push(make_sample(&[
        ("ParamEyeLOpen", 0.0),
        ("ParamEyeLSmile", 1.0),
        ("ParamEyeROpen", 0.0),
        ("ParamEyeRSmile", 1.0),
        ("ParamRabbitSize", 1.0),
        ("ParamRabbitRotate", 30.0),
        ("ParamAuraColor2", 1.0),
    ]));
    // 13: hair_front
    samples.push(make_sample(&[("ParamHairFront", 1.0)]));
    // 14: hair_back
    samples.push(make_sample(&[("ParamHairBack", 1.0)]));
    // 15: breath
    samples.push(make_sample(&[("ParamBreath", 1.0)]));
    // 16: light_strengthen
    samples.push(make_sample(&[
        ("ParamStrengthenLightOn", 1.0),
        ("ParamStrengthenLight", 1.0),
        ("ParamStrengthenLightMove", 1.0),
    ]));

    samples
}

fn document_samples(doc: &Document, samples: &[Vec<f32>]) -> serde_json::Value {
    let params: Vec<_> = doc
        .parameter_order()
        .iter()
        .map(|id| doc.get_parameter(id).unwrap())
        .collect();
    let mesh_slots: HashMap<&str, usize> = doc
        .mesh_order()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let frames: Vec<_> = samples
        .iter()
        .map(|sample| {
            let values: HashMap<String, f32> = params
                .iter()
                .zip(sample.iter())
                .map(|(p, &v)| (p.id.clone(), v))
                .collect();
            let mut frame = DrawableFrame::default();
            let status = evaluate_frame(doc, &values, &mut frame);
            assert!(status.is_ok(), "{status:?}");
            frame
                .drawables
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "runtime_id": d.runtime_id,
                        "texture_slot": d.texture_slot,
                        "draw_order": d.draw_order,
                        "render_order": d.render_order,
                        "opacity": d.opacity,
                        "visible": d.visible,
                        "enabled": d.enabled,
                        "double_sided": d.double_sided,
                        "inverted_mask": d.inverted_mask,
                        "blend_mode": match d.blend_mode {
                            BlendMode::Normal => 0,
                            BlendMode::Additive => 1,
                            BlendMode::Multiplicative => 2,
                        },
                        "positions": d.positions.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
                        "uvs": d.uvs.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
                        "indices": d.indices,
                        "mask_indices": d.masks.iter().map(|id| mesh_slots[id.as_str()]).collect::<Vec<_>>(),
                        "multiply_color": d.multiply_color,
                        "screen_color": d.screen_color,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    serde_json::json!({ "samples": frames })
}

fn main() {
    let out_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/kasane/m3b/fixtures"));
    fs::create_dir_all(&out_dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let candidates = [
        root.join("demos/gd-cubism-demo/assets/live2d/mao/runtime/mao_pro.moc3"),
        PathBuf::from("demos/gd-cubism-demo/assets/live2d/mao/runtime/mao_pro.moc3"),
    ];
    let mao_path = candidates.iter().find(|p| p.exists()).expect("mao_pro.moc3 not found");

    let mao_bytes = fs::read(mao_path).expect("Failed to read mao_pro.moc3");
    let decoded = import_from_bare_moc3(&mao_bytes, &HashMap::new()).expect("Import failed");
    let orig_doc = &decoded.document;

    // Serialize to Project v2 format
    let project_json = kasane_project::encode_project(orig_doc).expect("encode_project failed");

    // Deserialize to fresh, completely detached Document
    let detached_doc = kasane_project::decode_project(&project_json).expect("decode_project failed");

    let case_dir = out_dir.join("case_mao_pro");
    fs::create_dir_all(&case_dir).unwrap();

    let samples = sample_parameters_mao(&detached_doc);

    fs::write(case_dir.join("orig.moc3"), &mao_bytes).unwrap();
    let re_export_artifact = encode_moc3(&detached_doc).expect("encode_moc3 failed");
    fs::write(case_dir.join("re_export.moc3"), &re_export_artifact.bytes).unwrap();
    fs::write(
        case_dir.join("samples.json"),
        serde_json::to_vec(&samples).unwrap(),
    )
    .unwrap();
    fs::write(
        case_dir.join("document.json"),
        serde_json::to_vec(&document_samples(&detached_doc, &samples)).unwrap(),
    )
    .unwrap();
    fs::write(
        case_dir.join("ppu.txt"),
        detached_doc.canvas().pixels_per_unit.to_string(),
    )
    .unwrap();

    let parameters: Vec<Parameter> = detached_doc
        .parameter_order()
        .iter()
        .map(|id| detached_doc.get_parameter(id).unwrap().clone())
        .collect();

    fs::write(
        case_dir.join("metadata.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "canvas": detached_doc.canvas(),
            "parameters": parameters,
            "meshes_count": detached_doc.mesh_order().len(),
            "parts_count": detached_doc.part_order().len(),
            "glues_count": detached_doc.glue_order().len(),
            "blend_bindings_count": detached_doc.blend_binding_order().len(),
            "blend_key_tables_count": detached_doc.blend_key_table_order().len(),
            "blend_constraints_count": detached_doc.blend_constraint_order().len(),
        }))
        .unwrap(),
    )
    .unwrap();

    fs::write(
        out_dir.join("cases.json"),
        serde_json::to_vec(&["case_mao_pro"]).unwrap(),
    )
    .unwrap();

    println!("export_m3b_cases: case_mao_pro generated successfully with {} samples", samples.len());
}
