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
            let idx = *param_map
                .get(k)
                .unwrap_or_else(|| panic!("Unknown sample parameter: {k}"));
            s[idx] = v.clamp(params[idx].minimum, params[idx].maximum);
        }
        s
    };

    let mut samples = vec![
        // 0: defaults
        make_sample(&[]),
        // 1: vowel_A (constraint 0)
        make_sample(&[("ParamA", 1.0)]),
        // 2: vowel_I (constraint 1)
        make_sample(&[("ParamI", 1.0)]),
        // 3: vowel_U (constraint 2)
        make_sample(&[("ParamU", 1.0)]),
        // 4: vowel_E (constraint 3)
        make_sample(&[("ParamE", 1.0)]),
        // 5: vowel_O (constraint 4)
        make_sample(&[("ParamO", 1.0)]),
        // 6: mouth_down (constraint 5)
        make_sample(&[("ParamMouthDown", 1.0)]),
        // 7: mouth_angry (constraint 6)
        make_sample(&[("ParamMouthAngry", 1.0)]),
        // 8: mouth_combo (multi-constraint interaction)
        make_sample(&[("ParamA", 0.5), ("ParamMouthDown", 0.5)]),
        // 9: head_angles (multi-axis deformer)
        make_sample(&[
            ("ParamAngleX", 30.0),
            ("ParamAngleY", 30.0),
            ("ParamAngleZ", 30.0),
        ]),
        // 10: head_angles_neg (negative range)
        make_sample(&[
            ("ParamAngleX", -30.0),
            ("ParamAngleY", -30.0),
            ("ParamAngleZ", -30.0),
        ]),
        // 11: body_angles
        make_sample(&[
            ("ParamBodyAngleX", 10.0),
            ("ParamBodyAngleY", 10.0),
            ("ParamBodyAngleZ", 10.0),
        ]),
        // 12: eyes_and_rabbit (rotation deformer blend shape + colors)
        make_sample(&[
            ("ParamEyeLOpen", 0.0),
            ("ParamEyeLSmile", 1.0),
            ("ParamEyeROpen", 0.0),
            ("ParamEyeRSmile", 1.0),
            ("ParamRabbitSize", 1.0),
            ("ParamRabbitRotate", 30.0),
            ("ParamAuraColor2", 1.0),
        ]),
        // 13: hair_front
        make_sample(&[("ParamHairFront", 1.0)]),
        // 14: hair_back
        make_sample(&[("ParamHairBackL", 1.0), ("ParamHairBackR", 1.0)]),
        // 15: breath
        make_sample(&[("ParamBreath", 1.0)]),
        // 16: light_strengthen
        make_sample(&[
            ("ParamStrengthenLightOn", 1.0),
            ("ParamStrengthenLight", 1.0),
            ("ParamStrengthenLightMove", 1.0),
        ]),
    ];

    // Exercise every parameter and every authored curve, not just named poses.
    for (index, p) in params.iter().enumerate() {
        let mut keys = vec![p.minimum, p.maximum, p.default_value];
        for id in doc.binding_order() {
            for axis in &doc.get_binding(id).unwrap().axes {
                if axis.parameter_id == p.id {
                    keys.extend(&axis.keys);
                }
            }
        }
        for id in doc.scene_binding_order() {
            for axis in &doc.get_scene_binding(id).unwrap().axes {
                if axis.parameter_id == p.id {
                    keys.extend(&axis.keys);
                }
            }
        }
        for id in doc.blend_key_table_order() {
            let table = doc.get_blend_key_table(id).unwrap();
            if table.parameter_id == p.id {
                keys.extend(&table.keys);
            }
        }
        for id in doc.blend_constraint_order() {
            let constraint = doc.get_blend_constraint(id).unwrap();
            if constraint.parameter_id == p.id {
                keys.extend(&constraint.keys);
            }
        }
        keys.sort_by(f32::total_cmp);
        keys.dedup();
        let mids: Vec<_> = keys
            .windows(2)
            .map(|w| w[0] + (w[1] - w[0]) * 0.5)
            .collect();
        keys.extend(mids);
        for value in keys {
            let mut sample = defaults.clone();
            sample[index] = value;
            samples.push(sample);
        }
    }
    // Pairwise shared-target and constraint interactions, plus a normal-deformer
    // driver in the same dependency chain. Every combination is deterministic.
    let internal_index: HashMap<&str, usize> = params
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    for id in doc.blend_binding_order() {
        let binding = doc.get_blend_binding(id).unwrap();
        let table = doc.get_blend_key_table(&binding.key_table_id).unwrap();
        let driver = internal_index[table.parameter_id.as_str()];
        let mut dependencies = Vec::new();
        for constraint_id in &binding.constraint_ids {
            dependencies.push(
                doc.get_blend_constraint(constraint_id)
                    .unwrap()
                    .parameter_id
                    .as_str(),
            );
        }
        for other in doc.blend_bindings_for_target(&binding.target_id) {
            dependencies.push(
                doc.get_blend_key_table(&other.key_table_id)
                    .unwrap()
                    .parameter_id
                    .as_str(),
            );
        }
        let mut normal = Vec::new();
        if let Some(b) = doc.binding_for_mesh(&binding.target_id) {
            normal.extend(b.axes.iter().map(|a| a.parameter_id.as_str()));
        }
        let mut target = binding.target_id.as_str();
        if let Some(mesh) = doc.get_mesh(target) {
            target = &mesh.deformer_id;
        }
        while let Some(transform) = doc.get_transform(target) {
            if let Some(b) = doc.binding_for_scene(target) {
                normal.extend(b.axes.iter().map(|a| a.parameter_id.as_str()));
            }
            target = transform.parent();
        }
        dependencies.sort();
        dependencies.dedup();
        normal.sort();
        normal.dedup();
        dependencies.extend(normal.iter().copied());
        for dependency in dependencies {
            let index = internal_index[dependency];
            if index == driver {
                continue;
            }
            for dv in [
                params[driver].minimum,
                (params[driver].minimum + params[driver].maximum) * 0.5,
                params[driver].maximum,
            ] {
                for cv in [
                    params[index].minimum,
                    (params[index].minimum + params[index].maximum) * 0.5,
                    params[index].maximum,
                ] {
                    let mut sample = defaults.clone();
                    sample[driver] = dv;
                    sample[index] = cv;
                    samples.push(sample.clone());
                    if let Some(&normal_id) = normal.first() {
                        let ni = internal_index[normal_id];
                        if ni != index && ni != driver {
                            sample[ni] = params[ni].maximum;
                            samples.push(sample);
                        }
                    }
                }
            }
        }
    }
    samples.push(params.iter().map(|p| p.minimum).collect());
    samples.push(params.iter().map(|p| p.maximum).collect());
    let mut seen = std::collections::HashSet::new();
    samples.retain(|s| seen.insert(s.iter().map(|v| v.to_bits()).collect::<Vec<_>>()));
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
                        "indices": d.indices.as_ref(),
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
    let mao_path = candidates
        .iter()
        .find(|p| p.exists())
        .expect("mao_pro.moc3 not found");

    let mao_bytes = fs::read(mao_path).expect("Failed to read mao_pro.moc3");
    let decoded = import_from_bare_moc3(&mao_bytes, &HashMap::new()).expect("Import failed");
    let orig_doc = &decoded.document;

    // Serialize to Project v3 format
    let project_json = kasane_project::encode_project(orig_doc).expect("encode_project failed");
    if env::args().any(|a| a == "--focus-inkdrop") {
        fs::write(out_dir.join("project.json"), &project_json).unwrap();
    }

    // Deserialize to fresh, completely detached Document
    let detached_doc =
        kasane_project::decode_project(&project_json).expect("decode_project failed");

    let all_samples = if env::args().any(|a| a == "--focus-inkdrop") {
        vec![detached_doc
            .parameter_order()
            .iter()
            .map(|id| {
                let p = detached_doc.get_parameter(id).unwrap();
                if p.runtime_id == "ParamInkDrop" {
                    30.0
                } else {
                    p.default_value
                }
            })
            .collect()]
    } else if env::args().any(|a| a == "--focus-extremes") {
        vec![
            detached_doc
                .parameter_order()
                .iter()
                .map(|id| detached_doc.get_parameter(id).unwrap().minimum)
                .collect(),
            detached_doc
                .parameter_order()
                .iter()
                .map(|id| detached_doc.get_parameter(id).unwrap().maximum)
                .collect(),
        ]
    } else {
        sample_parameters_mao(&detached_doc)
    };
    let re_export_artifact = encode_moc3(&detached_doc).expect("encode_moc3 failed");
    let mut case_names = Vec::new();
    for (batch, samples) in all_samples.chunks(24).enumerate() {
        let name = format!("case_mao_pro_{batch:03}");
        let case_dir = out_dir.join(&name);
        case_names.push(name);
        fs::create_dir_all(&case_dir).unwrap();
        fs::write(case_dir.join("orig.moc3"), &mao_bytes).unwrap();
        fs::write(case_dir.join("re_export.moc3"), &re_export_artifact.bytes).unwrap();
        fs::write(
            case_dir.join("samples.json"),
            serde_json::to_vec(&samples).unwrap(),
        )
        .unwrap();
        fs::write(
            case_dir.join("document.json"),
            serde_json::to_vec(&document_samples(&detached_doc, samples)).unwrap(),
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
    }
    fs::write(
        out_dir.join("cases.json"),
        serde_json::to_vec(&case_names).unwrap(),
    )
    .unwrap();

    println!(
        "export_m3b_cases: {} batches generated with {} samples",
        case_names.len(),
        all_samples.len()
    );
}
