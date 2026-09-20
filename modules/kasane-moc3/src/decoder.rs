use sha2::{Digest, Sha256};
use std::collections::HashMap;

use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, Canvas, Mesh, MeshBinding, MeshKeyform, Parameter, Part,
    RotationPose, SceneBinding, SceneKeyform, Status, Transform, TransformKind, Vec2, VertexId,
};
use kasane_core::Document;

use crate::inspector::{CanvasInfo, Moc3InspectionReport, ModelCounts};

macro_rules! check_status {
    ($expr:expr) => {
        let s = $expr;
        if !s.is_ok() {
            return Err(s);
        }
    };
}

#[derive(Debug, Clone)]
pub struct ImportIdMapping {
    pub parts: HashMap<String, String>, // runtime_id -> internal_id
    pub deformers: HashMap<String, String>, // runtime_id -> internal_id
    pub meshes: HashMap<String, String>, // runtime_id -> internal_id
    pub parameters: HashMap<String, String>, // runtime_id -> internal_id
    pub part_by_index: Vec<String>,     // index -> internal_id
    pub deformer_by_index: Vec<String>, // index -> internal_id
    pub mesh_by_index: Vec<String>,     // index -> internal_id
    pub parameter_by_index: Vec<String>, // index -> internal_id
}

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub moc_version: u8,
    pub canvas: CanvasInfo,
    pub counts: ModelCounts,
    pub generated_runtime_ids: Vec<String>,
    pub unimported_attachments: Vec<String>,
    pub warnings: Vec<String>,
    pub id_mapping: ImportIdMapping,
}

#[derive(Debug, Clone)]
pub struct DecodedMoc3 {
    pub document: Document,
    pub report: ImportReport,
}

#[derive(Debug, Clone)]
pub struct TextureSlotInfo {
    pub slot: usize,
    pub asset_id: String,
    pub name: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

pub fn texture_slot_uuid(slot: usize) -> String {
    stable_id("kasane", "asset", slot, &format!("texture_{slot}"))
}

fn stable_id(doc_prefix: &str, kind: &str, index: usize, runtime_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(doc_prefix.as_bytes());
    hasher.update(b":");
    hasher.update(kind.as_bytes());
    hasher.update(b":");
    hasher.update(index.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(runtime_id.as_bytes());
    let hash = hasher.finalize();

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3],
        hash[4], hash[5],
        (hash[6] & 0x0f) | 0x40, hash[7],
        (hash[8] & 0x3f) | 0x80, hash[9],
        hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
    )
}

fn read_string(bytes: &[u8], offset: usize, max_len: usize) -> String {
    if offset >= bytes.len() {
        return String::new();
    }
    let end = (offset + max_len).min(bytes.len());
    let slice = &bytes[offset..end];
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..len]).trim().to_string()
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading i32 at offset {offset}"),
        ));
    }
    Ok(i32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Status> {
    if offset + 2 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading u16 at offset {offset}"),
        ));
    }
    Ok(u16::from_le_bytes(
        bytes[offset..offset + 2].try_into().unwrap(),
    ))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading f32 at offset {offset}"),
        ));
    }
    Ok(f32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

// Iterative three-state traversal: reject cycles before Document mutation and
// avoid using the call stack for externally supplied parent chains.
fn parent_order(parents: &[i32], kind: &str) -> Result<Vec<usize>, Status> {
    let mut state = vec![0u8; parents.len()];
    let mut order = Vec::with_capacity(parents.len());
    let mut chain = Vec::new();
    for start in 0..parents.len() {
        let mut current = start as i32;
        while current != -1 {
            if current < 0 || current as usize >= parents.len() {
                return Err(Status::error(
                    "INVALID_REFERENCE",
                    format!("{kind}[{start}]: invalid parent {current}"),
                ));
            }
            let index = current as usize;
            match state[index] {
                1 => {
                    return Err(Status::error(
                        "RELATIONSHIP_CYCLE",
                        format!("{kind}[{index}]: parent relationship contains a cycle"),
                    ))
                }
                2 => break,
                _ => {}
            }
            state[index] = 1;
            chain.push(index);
            current = parents[index];
        }
        while let Some(index) = chain.pop() {
            state[index] = 2;
            order.push(index);
        }
    }
    Ok(order)
}

pub fn decode_moc3(
    bytes: &[u8],
    inspection: &Moc3InspectionReport,
    textures: &[TextureSlotInfo],
) -> Result<DecodedMoc3, Status> {
    let offsets = &inspection.section_offsets;
    let counts = &inspection.counts;
    let ver = inspection.version_number;

    let doc_id = stable_id("document", "doc", 0, "root");
    let mut doc = Document::new();

    // 1. Canvas
    let ppu = inspection.canvas.pixels_per_unit;
    let canvas = Canvas {
        width: inspection.canvas.width,
        height: inspection.canvas.height,
        origin: Vec2::new(
            inspection.canvas.origin_x,
            inspection.canvas.height - inspection.canvas.origin_y,
        ),
        pixels_per_unit: ppu,
        flag: inspection.canvas.flag,
    };
    check_status!(doc.initialize(doc_id.clone(), canvas));

    let mut generated_ids = Vec::new();
    let warnings = Vec::new();

    // 2. Texture Assets
    let mut texture_by_slot: HashMap<usize, &TextureSlotInfo> = HashMap::new();
    for t in textures {
        texture_by_slot.insert(t.slot, t);
        check_status!(
            doc.add_asset(kasane_core::types::ImageAsset {
                id: t.asset_id.clone(),
                name: t.name.clone(),
                source: t.source.clone(),
                width: t.width,
                height: t.height,
                sha256: t.sha256.clone(),
            })
            .status
        );
    }

    // 3. ID Mappings
    let mut mapping = ImportIdMapping {
        parts: HashMap::new(),
        deformers: HashMap::new(),
        meshes: HashMap::new(),
        parameters: HashMap::new(),
        part_by_index: Vec::with_capacity(counts.parts as usize),
        deformer_by_index: Vec::with_capacity(counts.deformers as usize),
        mesh_by_index: Vec::with_capacity(counts.art_meshes as usize),
        parameter_by_index: Vec::with_capacity(counts.parameters as usize),
    };

    // Pre-calculate stable internal IDs
    for i in 0..counts.parts as usize {
        let mut rid = read_string(bytes, offsets[3] as usize + i * 64, 64);
        if rid.is_empty() {
            rid = format!("Part{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "part", i, &rid);
        mapping.parts.insert(rid, iid.clone());
        mapping.part_by_index.push(iid);
    }

    for i in 0..counts.deformers as usize {
        let mut rid = read_string(bytes, offsets[11] as usize + i * 64, 64);
        if rid.is_empty() {
            rid = format!("Deformer{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "deformer", i, &rid);
        mapping.deformers.insert(rid, iid.clone());
        mapping.deformer_by_index.push(iid);
    }

    for i in 0..counts.art_meshes as usize {
        let mut rid = read_string(bytes, offsets[33] as usize + i * 64, 64);
        if rid.is_empty() {
            rid = format!("ArtMesh{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "mesh", i, &rid);
        mapping.meshes.insert(rid, iid.clone());
        mapping.mesh_by_index.push(iid);
    }

    for i in 0..counts.parameters as usize {
        let mut rid = read_string(bytes, offsets[50] as usize + i * 64, 64);
        if rid.is_empty() {
            rid = format!("Param{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "param", i, &rid);
        mapping.parameters.insert(rid, iid.clone());
        mapping.parameter_by_index.push(iid);
    }

    // Ordinary keyform colors use the object's contiguous color window.
    // Per-keyform color offsets belong to BlendShape data, which we reject.
    let get_colors = |section: usize, object: usize, key: usize| -> Result<Appearance, Status> {
        let mut appearance = Appearance::default();
        if ver < 5 || (counts.keyform_mul_colors == 0 && counts.keyform_scr_colors == 0) {
            return Ok(appearance);
        }
        let base = read_i32(bytes, offsets[section] as usize + object * 4)?;
        let index = (base as i64) + key as i64;
        if base < 0
            || index >= counts.keyform_mul_colors as i64
            || index >= counts.keyform_scr_colors as i64
        {
            return Err(Status::error("INVALID_COLOR_REFERENCE", format!("section {section} object[{object}] keyform[{key}]: color index {index} is outside the color pools")));
        }
        for channel in 0..3 {
            appearance.multiply[channel] =
                read_f32(bytes, offsets[108 + channel] as usize + index as usize * 4)?;
            appearance.screen[channel] =
                read_f32(bytes, offsets[111 + channel] as usize + index as usize * 4)?;
        }
        Ok(appearance)
    };

    // Helper: recover binding axes for binding_idx
    let get_binding_axes = |b_idx: i32| -> Result<Vec<BindingAxis>, Status> {
        if b_idx < 0 || b_idx >= counts.bindings {
            return Err(Status::error(
                "INVALID_BINDING",
                format!("Invalid binding index {b_idx}"),
            ));
        }
        let kt_off = read_i32(bytes, offsets[73] as usize + b_idx as usize * 4)? as usize;
        let kt_len = read_i32(bytes, offsets[74] as usize + b_idx as usize * 4)? as usize;
        let mut axes = Vec::with_capacity(kt_len);

        for a in 0..kt_len {
            let kt = read_i32(bytes, offsets[72] as usize + (kt_off + a) * 4)?;
            // Find which parameter owns kt
            let mut param_idx: Option<usize> = None;
            for p in 0..counts.parameters as usize {
                let p_off = read_i32(bytes, offsets[56] as usize + p * 4)?;
                let p_len = read_i32(bytes, offsets[57] as usize + p * 4)?;
                if kt >= p_off && kt < p_off + p_len {
                    param_idx = Some(p);
                    break;
                }
            }
            let p = param_idx.ok_or_else(|| {
                Status::error(
                    "INVALID_BINDING",
                    format!("Key table {kt} not found in any parameter ranges"),
                )
            })?;

            let keys_off = read_i32(bytes, offsets[75] as usize + kt as usize * 4)? as usize;
            let keys_len = read_i32(bytes, offsets[76] as usize + kt as usize * 4)? as usize;
            let mut keys = Vec::with_capacity(keys_len);
            for k in 0..keys_len {
                keys.push(read_f32(bytes, offsets[77] as usize + (keys_off + k) * 4)?);
            }

            axes.push(BindingAxis {
                parameter_id: mapping.parameter_by_index[p].clone(),
                keys,
            });
        }
        Ok(axes)
    };

    // Helper: calculate combo key values for index in full grid
    let combo_keys = |axes: &[BindingAxis], combo_idx: usize| -> Vec<f32> {
        let mut result = Vec::with_capacity(axes.len());
        let mut stride = 1usize;
        for axis in axes {
            let k_idx = (combo_idx / stride) % axis.keys.len();
            result.push(axis.keys[k_idx]);
            stride *= axis.keys.len();
        }
        result
    };

    // 4. Create Parameters
    for p in 0..counts.parameters as usize {
        let id = mapping.parameter_by_index[p].clone();
        let runtime_id = read_string(bytes, offsets[50] as usize + p * 64, 64);
        let max = read_f32(bytes, offsets[51] as usize + p * 4)?;
        let min = read_f32(bytes, offsets[52] as usize + p * 4)?;
        let default_val = read_f32(bytes, offsets[53] as usize + p * 4)?;
        let dec_places = read_i32(bytes, offsets[55] as usize + p * 4)?;

        check_status!(
            doc.create_parameter(Parameter {
                id,
                runtime_id: if runtime_id.is_empty() {
                    format!("Param{p}")
                } else {
                    runtime_id.clone()
                },
                name: if runtime_id.is_empty() {
                    format!("Param{p}")
                } else {
                    runtime_id
                },
                minimum: min,
                maximum: max,
                default_value: default_val,
                decimal_places: dec_places,
            })
            .status
        );
    }

    // 5. Create Parts (topologically ordered: parents before children)
    let mut part_parent_indices = Vec::with_capacity(counts.parts as usize);
    for p in 0..counts.parts as usize {
        part_parent_indices.push(read_i32(bytes, offsets[9] as usize + p * 4)?);
    }

    let part_creation_order = parent_order(&part_parent_indices, "Part")?;

    let mut part_scene_bindings = Vec::new();
    for &p in &part_creation_order {
        let id = mapping.part_by_index[p].clone();
        let runtime_id = read_string(bytes, offsets[3] as usize + p * 64, 64);
        let b_idx = read_i32(bytes, offsets[4] as usize + p * 4)?;
        let kf_off = read_i32(bytes, offsets[5] as usize + p * 4)? as usize;
        let _kf_len = read_i32(bytes, offsets[6] as usize + p * 4)? as usize;
        let enabled = read_i32(bytes, offsets[8] as usize + p * 4)? != 0;
        let parent_idx = part_parent_indices[p];

        let parent_id = if parent_idx >= 0 && (parent_idx as usize) < counts.parts as usize {
            mapping.part_by_index[parent_idx as usize].clone()
        } else {
            String::new()
        };

        let axes = get_binding_axes(b_idx)?;
        let is_bound = !axes.is_empty();
        let total_combos = if is_bound {
            axes.iter().map(|a| a.keys.len()).product()
        } else {
            1
        };

        let base_draw_order = if counts.part_keyforms > 0 && kf_off < counts.part_keyforms as usize
        {
            read_f32(bytes, offsets[58] as usize + kf_off * 4)?
        } else {
            0.0
        };

        check_status!(
            doc.create_part(Part {
                id: id.clone(),
                runtime_id: if runtime_id.is_empty() {
                    format!("Part{p}")
                } else {
                    runtime_id.clone()
                },
                name: if runtime_id.is_empty() {
                    format!("Part{p}")
                } else {
                    runtime_id
                },
                parent_id,
                enabled,
                draw_order: base_draw_order,
            })
            .status
        );

        if is_bound {
            let mut keyforms = Vec::with_capacity(total_combos);
            for k in 0..total_combos {
                let d_order = if kf_off + k < counts.part_keyforms as usize {
                    read_f32(bytes, offsets[58] as usize + (kf_off + k) * 4)?
                } else {
                    base_draw_order
                };
                keyforms.push(SceneKeyform {
                    keys: combo_keys(&axes, k),
                    positions: Vec::new(),
                    rotation: RotationPose::default(),
                    appearance: Appearance::default(),
                    draw_order: d_order,
                });
            }
            part_scene_bindings.push(SceneBinding {
                id: stable_id(&doc_id, "part_binding", p, &id),
                target_id: id,
                axes,
                keyforms,
            });
        }
    }

    // 6. Create Transforms (Deformers) (topologically ordered: parents before children)
    let mut deformer_parents = Vec::with_capacity(counts.deformers as usize);
    for d in 0..counts.deformers as usize {
        deformer_parents.push(read_i32(bytes, offsets[16] as usize + d * 4)?);
    }

    let deformer_creation_order = parent_order(&deformer_parents, "Deformer")?;

    let mut deformer_scene_bindings = Vec::new();
    for &d in &deformer_creation_order {
        let id = mapping.deformer_by_index[d].clone();
        let runtime_id = read_string(bytes, offsets[11] as usize + d * 64, 64);
        let b_idx = read_i32(bytes, offsets[12] as usize + d * 4)?;
        let enabled = read_i32(bytes, offsets[14] as usize + d * 4)? != 0;
        let part_idx = read_i32(bytes, offsets[15] as usize + d * 4)?;
        let parent_def_idx = deformer_parents[d];
        let dtype = read_i32(bytes, offsets[17] as usize + d * 4)?;
        let local_idx = read_i32(bytes, offsets[18] as usize + d * 4)? as usize;

        let part_id = if part_idx >= 0 && (part_idx as usize) < counts.parts as usize {
            mapping.part_by_index[part_idx as usize].clone()
        } else {
            String::new()
        };

        let parent_id =
            if parent_def_idx >= 0 && (parent_def_idx as usize) < counts.deformers as usize {
                mapping.deformer_by_index[parent_def_idx as usize].clone()
            } else {
                String::new()
            };

        let is_root = parent_id.is_empty();
        let axes = get_binding_axes(b_idx)?;
        let is_bound = !axes.is_empty();
        let total_combos = if is_bound {
            axes.iter().map(|a| a.keys.len()).product()
        } else {
            1
        };

        let is_warp = dtype == 0;
        if is_warp {
            let kf_off = read_i32(bytes, offsets[20] as usize + local_idx * 4)? as usize;
            let rows = read_i32(bytes, offsets[23] as usize + local_idx * 4)? as u32;
            let cols = read_i32(bytes, offsets[24] as usize + local_idx * 4)? as u32;
            let quad = if offsets.len() > 101 && counts.warps > 0 {
                read_i32(bytes, offsets[101] as usize + local_idx * 4)? != 0
            } else {
                true
            };

            let pt_count = ((rows + 1) * (cols + 1)) as usize;
            let mut keyforms = Vec::with_capacity(total_combos);

            for k in 0..total_combos {
                let opacity = if kf_off + k < counts.warp_keyforms as usize {
                    read_f32(bytes, offsets[59] as usize + (kf_off + k) * 4)?
                } else {
                    1.0
                };
                let pos_off = if kf_off + k < counts.warp_keyforms as usize {
                    read_i32(bytes, offsets[60] as usize + (kf_off + k) * 4)? as usize
                } else {
                    0
                };

                let mut points = Vec::with_capacity(pt_count);
                for p_idx in 0..pt_count {
                    let rx = read_f32(bytes, offsets[71] as usize + (pos_off + p_idx * 2) * 4)?;
                    let ry = read_f32(bytes, offsets[71] as usize + (pos_off + p_idx * 2 + 1) * 4)?;
                    if is_root {
                        points.push(Vec2::new(
                            rx * ppu + canvas.origin.x,
                            canvas.origin.y - ry * ppu,
                        ));
                    } else {
                        points.push(Vec2::new(rx, ry));
                    }
                }

                let mut appearance = get_colors(105, local_idx, k)?;
                appearance.opacity = opacity;

                keyforms.push(SceneKeyform {
                    keys: if is_bound {
                        combo_keys(&axes, k)
                    } else {
                        Vec::new()
                    },
                    positions: points,
                    rotation: RotationPose::default(),
                    appearance,
                    draw_order: 0.0,
                });
            }

            let base = &keyforms[0];
            check_status!(
                doc.create_transform(Transform {
                    id: id.clone(),
                    runtime_id: if runtime_id.is_empty() {
                        format!("Warp{local_idx}")
                    } else {
                        runtime_id.clone()
                    },
                    name: if runtime_id.is_empty() {
                        format!("Warp{local_idx}")
                    } else {
                        runtime_id
                    },
                    part_id,
                    parent_id,
                    kind: TransformKind::Warp,
                    base_angle: 0.0,
                    rotation: RotationPose::default(),
                    rows,
                    columns: cols,
                    quad,
                    enabled,
                    points: base.positions.clone(),
                    appearance: base.appearance,
                })
                .status
            );

            if is_bound {
                deformer_scene_bindings.push(SceneBinding {
                    id: stable_id(&doc_id, "deformer_binding", d, &id),
                    target_id: id,
                    axes,
                    keyforms,
                });
            }
        } else {
            // Rotation Deformer
            let kf_off = read_i32(bytes, offsets[26] as usize + local_idx * 4)? as usize;
            let base_angle = read_f32(bytes, offsets[28] as usize + local_idx * 4)?;

            let mut keyforms = Vec::with_capacity(total_combos);
            for k in 0..total_combos {
                let opacity = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(bytes, offsets[61] as usize + (kf_off + k) * 4)?
                } else {
                    1.0
                };
                let angle = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(bytes, offsets[62] as usize + (kf_off + k) * 4)?
                } else {
                    0.0
                };
                let ox = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(bytes, offsets[63] as usize + (kf_off + k) * 4)?
                } else {
                    0.0
                };
                let oy = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(bytes, offsets[64] as usize + (kf_off + k) * 4)?
                } else {
                    0.0
                };
                let scale = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(bytes, offsets[65] as usize + (kf_off + k) * 4)?
                } else {
                    1.0
                };
                let ref_x = if kf_off + k < counts.rotation_keyforms as usize {
                    read_i32(bytes, offsets[66] as usize + (kf_off + k) * 4)? != 0
                } else {
                    false
                };
                let ref_y = if kf_off + k < counts.rotation_keyforms as usize {
                    read_i32(bytes, offsets[67] as usize + (kf_off + k) * 4)? != 0
                } else {
                    false
                };

                let origin = if is_root {
                    Vec2::new(ox * ppu + canvas.origin.x, canvas.origin.y - oy * ppu)
                } else {
                    Vec2::new(ox, oy)
                };

                let mut appearance = get_colors(106, local_idx, k)?;
                appearance.opacity = opacity;

                keyforms.push(SceneKeyform {
                    keys: if is_bound {
                        combo_keys(&axes, k)
                    } else {
                        Vec::new()
                    },
                    positions: Vec::new(),
                    rotation: RotationPose {
                        origin,
                        angle,
                        scale,
                        reflect_x: ref_x,
                        reflect_y: ref_y,
                    },
                    appearance,
                    draw_order: 0.0,
                });
            }

            let base = &keyforms[0];
            check_status!(
                doc.create_transform(Transform {
                    id: id.clone(),
                    runtime_id: if runtime_id.is_empty() {
                        format!("Rotation{local_idx}")
                    } else {
                        runtime_id.clone()
                    },
                    name: if runtime_id.is_empty() {
                        format!("Rotation{local_idx}")
                    } else {
                        runtime_id
                    },
                    part_id,
                    parent_id,
                    kind: TransformKind::Rotation,
                    base_angle,
                    rotation: base.rotation,
                    rows: 1,
                    columns: 1,
                    quad: true,
                    enabled,
                    points: Vec::new(),
                    appearance: base.appearance,
                })
                .status
            );

            if is_bound {
                deformer_scene_bindings.push(SceneBinding {
                    id: stable_id(&doc_id, "deformer_binding", d, &id),
                    target_id: id,
                    axes,
                    keyforms,
                });
            }
        }
    }

    // 7. Create ArtMeshes
    let mut mesh_bindings = Vec::new();
    let mut mesh_masks = Vec::new();
    for m in 0..counts.art_meshes as usize {
        let id = mapping.mesh_by_index[m].clone();
        let runtime_id = read_string(bytes, offsets[33] as usize + m * 64, 64);
        let b_idx = read_i32(bytes, offsets[34] as usize + m * 4)?;
        let kf_off = read_i32(bytes, offsets[35] as usize + m * 4)? as usize;
        let enabled = read_i32(bytes, offsets[38] as usize + m * 4)? != 0;
        let part_idx = read_i32(bytes, offsets[39] as usize + m * 4)?;
        let def_idx = read_i32(bytes, offsets[40] as usize + m * 4)?;
        let tex_no = read_i32(bytes, offsets[41] as usize + m * 4)?;
        let flag = bytes[offsets[42] as usize + m];
        let vc = read_i32(bytes, offsets[43] as usize + m * 4)? as usize;
        let uv_off = read_i32(bytes, offsets[44] as usize + m * 4)? as usize;
        let idx_off = read_i32(bytes, offsets[45] as usize + m * 4)? as usize;
        let idx_len = read_i32(bytes, offsets[46] as usize + m * 4)? as usize;
        let mask_off = read_i32(bytes, offsets[47] as usize + m * 4)? as usize;
        let mask_len = read_i32(bytes, offsets[48] as usize + m * 4)? as usize;

        let part_id = if part_idx >= 0 && (part_idx as usize) < counts.parts as usize {
            mapping.part_by_index[part_idx as usize].clone()
        } else {
            String::new()
        };

        let deformer_id = if def_idx >= 0 && (def_idx as usize) < counts.deformers as usize {
            mapping.deformer_by_index[def_idx as usize].clone()
        } else {
            String::new()
        };

        let is_root = deformer_id.is_empty();

        let slot = if tex_no >= 0 { tex_no as usize } else { 0 };
        let texture_asset_id = if let Some(t) = texture_by_slot.get(&slot) {
            t.asset_id.clone()
        } else {
            let id = texture_slot_uuid(slot);
            if doc.get_asset(&id).is_none() {
                check_status!(
                    doc.add_asset(kasane_core::types::ImageAsset {
                        id: id.clone(),
                        name: format!("Texture {slot} (Unmapped)"),
                        source: format!("unmapped_slot_{slot}.png"),
                        width: 1,
                        height: 1,
                        sha256: String::new(),
                    })
                    .status
                );
            }
            id
        };

        let double_sided = (flag & 4) != 0;
        let inverted_mask = (flag & 8) != 0;
        let blend_mode = match flag & 3 {
            1 => BlendMode::Additive,
            2 => BlendMode::Multiplicative,
            _ => BlendMode::Normal,
        };

        // Read UVs
        let mut uvs = Vec::with_capacity(vc);
        for v in 0..vc {
            let u = read_f32(bytes, offsets[78] as usize + (uv_off + v * 2) * 4)?;
            let v_val = read_f32(bytes, offsets[78] as usize + (uv_off + v * 2 + 1) * 4)?;
            uvs.push(Vec2::new(u, 1.0 - v_val));
        }

        // Generate 1-indexed dense vertex IDs
        let vertex_ids: Vec<VertexId> = (1..=vc as u32).collect();

        // Read triangles (inverting winding by swapping 1 and 2)
        let mut triangles = Vec::with_capacity(idx_len / 3);
        for t in 0..(idx_len / 3) {
            let i0 = read_u16(bytes, offsets[79] as usize + (idx_off + t * 3) * 2)? as usize;
            let i1 = read_u16(bytes, offsets[79] as usize + (idx_off + t * 3 + 1) * 2)? as usize;
            let i2 = read_u16(bytes, offsets[79] as usize + (idx_off + t * 3 + 2) * 2)? as usize;
            if i0 < vc && i1 < vc && i2 < vc {
                // Invert winding swap (render swapped 1 and 2, so swapping 1 and 2 restores source)
                triangles.push([vertex_ids[i0], vertex_ids[i2], vertex_ids[i1]]);
            }
        }

        // Masks
        let mut masks = Vec::with_capacity(mask_len);
        for m_idx in 0..mask_len {
            let target_mesh_idx = read_i32(bytes, offsets[80] as usize + (mask_off + m_idx) * 4)?;
            if target_mesh_idx >= 0 && (target_mesh_idx as usize) < counts.art_meshes as usize {
                masks.push(mapping.mesh_by_index[target_mesh_idx as usize].clone());
            }
        }

        let axes = get_binding_axes(b_idx)?;
        let is_bound = !axes.is_empty();
        let total_combos = if is_bound {
            axes.iter().map(|a| a.keys.len()).product()
        } else {
            1
        };

        let mut keyforms = Vec::with_capacity(total_combos);
        for k in 0..total_combos {
            let opacity = if kf_off + k < counts.art_mesh_keyforms as usize {
                read_f32(bytes, offsets[68] as usize + (kf_off + k) * 4)?
            } else {
                1.0
            };
            let d_order = if kf_off + k < counts.art_mesh_keyforms as usize {
                read_f32(bytes, offsets[69] as usize + (kf_off + k) * 4)?
            } else {
                m as f32
            };
            let pos_off = if kf_off + k < counts.art_mesh_keyforms as usize {
                read_i32(bytes, offsets[70] as usize + (kf_off + k) * 4)? as usize
            } else {
                0
            };

            let mut positions = Vec::with_capacity(vc);
            for v in 0..vc {
                let rx = read_f32(bytes, offsets[71] as usize + (pos_off + v * 2) * 4)?;
                let ry = read_f32(bytes, offsets[71] as usize + (pos_off + v * 2 + 1) * 4)?;
                if is_root {
                    positions.push(Vec2::new(
                        rx * ppu + canvas.origin.x,
                        canvas.origin.y - ry * ppu,
                    ));
                } else {
                    positions.push(Vec2::new(rx, ry));
                }
            }

            let mut appearance = get_colors(107, m, k)?;
            appearance.opacity = opacity;

            keyforms.push(MeshKeyform {
                keys: if is_bound {
                    combo_keys(&axes, k)
                } else {
                    Vec::new()
                },
                positions,
                appearance,
                draw_order: Some(d_order),
            });
        }

        let base = &keyforms[0];
        check_status!(
            doc.create_mesh(Mesh {
                id: id.clone(),
                name: if runtime_id.is_empty() {
                    format!("ArtMesh{m}")
                } else {
                    runtime_id.clone()
                },
                texture_asset_id,
                vertex_ids,
                base_positions: base.positions.clone(),
                uvs,
                triangles,
                runtime_id: if runtime_id.is_empty() {
                    format!("ArtMesh{m}")
                } else {
                    runtime_id
                },
                part_id,
                deformer_id,
                appearance: base.appearance,
                draw_order: base.draw_order,
                blend_mode,
                enabled,
                double_sided,
                inverted_mask,
                masks: Vec::new(),
            })
            .status
        );

        if !masks.is_empty() {
            mesh_masks.push((id.clone(), masks));
        }

        if is_bound {
            mesh_bindings.push(MeshBinding {
                id: stable_id(&doc_id, "mesh_binding", m, &id),
                mesh_id: id,
                axes,
                keyforms,
            });
        }
    }

    // Set mesh masks now that all meshes exist in document
    for (m_id, masks) in mesh_masks {
        let mut mesh = doc.get_mesh(&m_id).unwrap().clone();
        mesh.masks = masks;
        check_status!(doc.replace_mesh(mesh).status);
    }

    // 8. Attach Bindings to Document
    for b in part_scene_bindings {
        check_status!(doc.create_scene_binding(b).status);
    }
    for b in deformer_scene_bindings {
        check_status!(doc.create_scene_binding(b).status);
    }
    for b in mesh_bindings {
        check_status!(doc.create_binding(b).status);
    }

    // Drawing hierarchy and tie order are independent of organization Parts.
    let mut groups = Vec::new();
    let mut owners = vec![None; counts.draw_groups as usize];
    if owners.is_empty() {
        return Err(Status::error(
            "INVALID_DRAW_GROUP",
            "Missing root drawing group",
        ));
    }
    owners[0] = Some(String::new());
    for index in 0..counts.draw_items as usize {
        if read_i32(bytes, offsets[86] as usize + index * 4)? == 1 {
            let part = read_i32(bytes, offsets[87] as usize + index * 4)?;
            let child = read_i32(bytes, offsets[88] as usize + index * 4)?;
            if part < 0
                || part as usize >= mapping.part_by_index.len()
                || child <= 0
                || child as usize >= owners.len()
                || owners[child as usize].is_some()
            {
                return Err(Status::error(
                    "INVALID_DRAW_GROUP",
                    format!("Invalid child group at drawing item {index}"),
                ));
            }
            owners[child as usize] = Some(mapping.part_by_index[part as usize].clone());
        }
    }
    for (index, owner) in owners.into_iter().enumerate() {
        let owner = owner.ok_or_else(|| {
            Status::error(
                "INVALID_DRAW_GROUP",
                format!("Detached drawing group {index}"),
            )
        })?;
        let start = read_i32(bytes, offsets[81] as usize + index * 4)?;
        let length = read_i32(bytes, offsets[82] as usize + index * 4)?;
        if start < 0 || length < 0 || start as i64 + length as i64 > counts.draw_items as i64 {
            return Err(Status::error(
                "INVALID_DRAW_GROUP",
                format!("Invalid item range in group {index}"),
            ));
        }
        let mut items = Vec::new();
        for item in start as usize..(start + length) as usize {
            let kind = read_i32(bytes, offsets[86] as usize + item * 4)?;
            let object = read_i32(bytes, offsets[87] as usize + item * 4)?;
            let table = match kind {
                0 => &mapping.mesh_by_index,
                1 => &mapping.part_by_index,
                _ => {
                    return Err(Status::error(
                        "INVALID_DRAW_GROUP",
                        format!("Unknown item type {kind}"),
                    ))
                }
            };
            let id = table.get(object as usize).ok_or_else(|| {
                Status::error(
                    "INVALID_DRAW_GROUP",
                    format!("Invalid object at item {item}"),
                )
            })?;
            items.push(id.clone());
        }
        groups.push(kasane_core::draw_order::DrawOrderGroup {
            owner,
            items,
            min_order: read_i32(bytes, offsets[85] as usize + index * 4)?,
            max_order: read_i32(bytes, offsets[84] as usize + index * 4)?,
        });
    }
    check_status!(doc.replace_draw_order_groups(groups).status);

    Ok(DecodedMoc3 {
        document: doc,
        report: ImportReport {
            moc_version: ver,
            canvas: inspection.canvas.clone(),
            counts: counts.clone(),
            generated_runtime_ids: generated_ids,
            unimported_attachments: Vec::new(),
            warnings,
            id_mapping: mapping,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::parent_order;

    #[test]
    fn orders_deep_parent_chains_without_recursion() {
        let mut parents: Vec<i32> = (1..=100_000).collect();
        *parents.last_mut().unwrap() = -1;
        let order = parent_order(&parents, "Part").unwrap();
        assert_eq!(order.len(), parents.len());
        assert_eq!(order[0], parents.len() - 1);
        assert_eq!(*order.last().unwrap(), 0);
        assert!(parent_order(&[-2], "Part").is_err());
    }
}
