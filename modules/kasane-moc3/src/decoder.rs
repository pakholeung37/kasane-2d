use crate::schema::section;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint,
    BlendShapeKeyTable, BlendShapeTargetKind, Canvas, DeltaGlueKeyform, DeltaKeyforms,
    DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform,
    DeltaWarpKeyform, Glue, GlueVertexPair, Mesh, MeshBinding, MeshKeyform, Offscreen,
    OffscreenKeyform, Parameter, ParameterKind, Part, RotationPose, SceneBinding, SceneKeyform,
    Status, Transform, TransformKind, Vec2, VertexId,
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
    pub blend_key_table_by_index: Vec<String>, // index -> internal_id
    pub blend_constraint_by_index: Vec<String>, // index -> internal_id
    pub blend_binding_by_index: Vec<String>, // index -> internal_id
    pub glue_by_index: Vec<String>,     // index -> internal_id
    pub offscreen_by_index: Vec<String>, // index -> internal_id
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

fn checked_reference<'a, T>(table: &'a [T], index: usize, field: &str) -> Result<&'a T, Status> {
    table.get(index).ok_or_else(|| {
        Status::error(
            "INDEX_OUT_OF_BOUNDS",
            format!(
                "{field}: index {index} exceeds table length {}",
                table.len()
            ),
        )
    })
}

fn checked_window(start: usize, len: usize, count: usize, field: &str) -> Result<(), Status> {
    if start > count || len > count - start {
        return Err(Status::error(
            "INDEX_OUT_OF_BOUNDS",
            format!("{field}: window {start}+{len} exceeds table length {count}"),
        ));
    }
    Ok(())
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

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading u32 at offset {offset}"),
        ));
    }
    Ok(u32::from_le_bytes(
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
    _inspection: &Moc3InspectionReport,
    textures: &[TextureSlotInfo],
) -> Result<DecodedMoc3, Status> {
    // Public callers may provide a stale report; never trust it for memory safety.
    let inspection = crate::inspector::inspect_structure(bytes)?;
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
        blend_key_table_by_index: Vec::with_capacity(counts.blend_key_tables as usize),
        blend_constraint_by_index: Vec::with_capacity(counts.bs_constraints as usize),
        blend_binding_by_index: Vec::with_capacity(counts.blend_bindings as usize),
        glue_by_index: Vec::with_capacity(counts.glues as usize),
        offscreen_by_index: Vec::with_capacity(counts.offscreens as usize),
    };

    // Pre-calculate stable internal IDs
    for i in 0..counts.parts as usize {
        let mut rid = read_string(bytes, offsets[section::PART_SRC_ID] as usize + i * 64, 64);
        if rid.is_empty() {
            rid = format!("Part{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "part", i, &rid);
        mapping.parts.insert(rid, iid.clone());
        mapping.part_by_index.push(iid);
    }

    for i in 0..counts.deformers as usize {
        let mut rid = read_string(
            bytes,
            offsets[section::DEFORMER_SRC_ID] as usize + i * 64,
            64,
        );
        if rid.is_empty() {
            rid = format!("Deformer{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "deformer", i, &rid);
        mapping.deformers.insert(rid, iid.clone());
        mapping.deformer_by_index.push(iid);
    }

    for i in 0..counts.art_meshes as usize {
        let mut rid = read_string(
            bytes,
            offsets[section::ART_MESH_SRC_ID] as usize + i * 64,
            64,
        );
        if rid.is_empty() {
            rid = format!("ArtMesh{i}");
            generated_ids.push(rid.clone());
        }
        let iid = stable_id(&doc_id, "mesh", i, &rid);
        mapping.meshes.insert(rid, iid.clone());
        mapping.mesh_by_index.push(iid);
    }

    for i in 0..counts.parameters as usize {
        let mut rid = read_string(bytes, offsets[section::PARAM_SRC_ID] as usize + i * 64, 64);
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
        if ver < 4 || (counts.keyform_mul_colors == 0 && counts.keyform_scr_colors == 0) {
            return Ok(appearance);
        }
        let base = read_i32(bytes, offsets[section] as usize + object * 4)?;
        if base < 0 {
            return Ok(appearance);
        }
        let index = (base as i64) + key as i64;
        if index >= counts.keyform_mul_colors as i64 || index >= counts.keyform_scr_colors as i64 {
            return Err(Status::error("INVALID_COLOR_REFERENCE", format!("section {section} object[{object}] keyform[{key}]: color index {index} is outside the color pools")));
        }
        for channel in 0..3 {
            appearance.multiply[channel] = read_f32(
                bytes,
                offsets[section::KEYFORM_MUL_COLOR_SRC_R + channel] as usize + index as usize * 4,
            )?;
            appearance.screen[channel] = read_f32(
                bytes,
                offsets[section::KEYFORM_SCR_COLOR_SRC_R + channel] as usize + index as usize * 4,
            )?;
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
        let kt_off = read_i32(
            bytes,
            offsets[section::BINDING_SRC_KEY_TABLE_IDX_OFF] as usize + b_idx as usize * 4,
        )? as usize;
        let kt_len = read_i32(
            bytes,
            offsets[section::BINDING_SRC_KEY_TABLE_IDX_LEN] as usize + b_idx as usize * 4,
        )? as usize;
        let mut axes = Vec::with_capacity(kt_len);

        for a in 0..kt_len {
            let kt = read_i32(
                bytes,
                offsets[section::KEY_TABLE_IDX_SRC_IDX] as usize + (kt_off + a) * 4,
            )?;
            // Find which parameter owns kt
            let mut param_idx: Option<usize> = None;
            for p in 0..counts.parameters as usize {
                let p_off = read_i32(
                    bytes,
                    offsets[section::PARAM_SRC_KEY_TABLE_OFF] as usize + p * 4,
                )?;
                let p_len = read_i32(
                    bytes,
                    offsets[section::PARAM_SRC_KEY_TABLE_LEN] as usize + p * 4,
                )?;
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

            let keys_off = read_i32(
                bytes,
                offsets[section::KEY_TABLE_SRC_KEYS_OFF] as usize + kt as usize * 4,
            )? as usize;
            let keys_len = read_i32(
                bytes,
                offsets[section::KEY_TABLE_SRC_KEYS_LEN] as usize + kt as usize * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(keys_len);
            for k in 0..keys_len {
                keys.push(read_f32(
                    bytes,
                    offsets[section::KEYS_SRC_KEY] as usize + (keys_off + k) * 4,
                )?);
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
        let runtime_id = read_string(bytes, offsets[section::PARAM_SRC_ID] as usize + p * 64, 64);
        let max = read_f32(
            bytes,
            offsets[section::PARAM_SRC_MAXIMUM_VALUE] as usize + p * 4,
        )?;
        let min = read_f32(
            bytes,
            offsets[section::PARAM_SRC_MINIMUM_VALUE] as usize + p * 4,
        )?;
        let default_val = read_f32(
            bytes,
            offsets[section::PARAM_SRC_DEFAULT_VALUE] as usize + p * 4,
        )?;
        let dec_places = read_i32(
            bytes,
            offsets[section::PARAM_SRC_DECIMAL_PLACES] as usize + p * 4,
        )?;
        let repeat = if offsets.len() > 54 && offsets[section::PARAM_SRC_REPEAT] > 0 {
            read_i32(bytes, offsets[section::PARAM_SRC_REPEAT] as usize + p * 4)? != 0
        } else {
            false
        };
        let param_type = if ver >= 4 && offsets.len() > 114 && offsets[section::PARAM_SRC_TYPE] > 0
        {
            read_i32(bytes, offsets[section::PARAM_SRC_TYPE] as usize + p * 4)?
        } else {
            0
        };
        let kind = match param_type {
            0 => ParameterKind::Normal,
            1 => ParameterKind::BlendShape,
            _ => {
                return Err(Status::error(
                    "UNSUPPORTED_FEATURE",
                    format!("Parameter {p}: unknown type {param_type}"),
                ))
            }
        };

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
                kind,
                repeat,
            })
            .status
        );
    }

    // 5. Create Parts (topologically ordered: parents before children)
    let mut part_parent_indices = Vec::with_capacity(counts.parts as usize);
    for p in 0..counts.parts as usize {
        part_parent_indices.push(read_i32(
            bytes,
            offsets[section::PART_SRC_PARENT_PART_IDX] as usize + p * 4,
        )?);
    }

    let part_creation_order = parent_order(&part_parent_indices, "Part")?;

    let mut part_scene_bindings = Vec::new();
    for &p in &part_creation_order {
        let id = mapping.part_by_index[p].clone();
        let runtime_id = read_string(bytes, offsets[section::PART_SRC_ID] as usize + p * 64, 64);
        let b_idx = read_i32(
            bytes,
            offsets[section::PART_SRC_BINDING_IDX] as usize + p * 4,
        )?;
        let kf_off = read_i32(
            bytes,
            offsets[section::PART_SRC_KEYFORM_OFF] as usize + p * 4,
        )? as usize;
        let _kf_len =
            read_i32(bytes, offsets[section::PART_SRC_KEY_LEN] as usize + p * 4)? as usize;
        let enabled = read_i32(bytes, offsets[section::PART_SRC_ENABLE] as usize + p * 4)? != 0;
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
            read_f32(
                bytes,
                offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize + kf_off * 4,
            )?
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
                    read_f32(
                        bytes,
                        offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize + (kf_off + k) * 4,
                    )?
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
        deformer_parents.push(read_i32(
            bytes,
            offsets[section::DEFORMER_SRC_PARENT_DEFORMER_IDX] as usize + d * 4,
        )?);
    }

    let deformer_creation_order = parent_order(&deformer_parents, "Deformer")?;

    let mut warp_by_local_idx = vec![String::new(); counts.warps as usize];
    let mut rotation_by_local_idx = vec![String::new(); counts.rotations as usize];
    let mut deformer_scene_bindings = Vec::new();
    for &d in &deformer_creation_order {
        let id = mapping.deformer_by_index[d].clone();
        let runtime_id = read_string(
            bytes,
            offsets[section::DEFORMER_SRC_ID] as usize + d * 64,
            64,
        );
        let b_idx = read_i32(
            bytes,
            offsets[section::DEFORMER_SRC_BINDING_IDX] as usize + d * 4,
        )?;
        let enabled = read_i32(
            bytes,
            offsets[section::DEFORMER_SRC_ENABLE] as usize + d * 4,
        )? != 0;
        let part_idx = read_i32(
            bytes,
            offsets[section::DEFORMER_SRC_PARENT_PART_IDX] as usize + d * 4,
        )?;
        let parent_def_idx = deformer_parents[d];
        let dtype = read_i32(bytes, offsets[section::DEFORMER_SRC_TYPE] as usize + d * 4)?;
        let local_idx = read_i32(
            bytes,
            offsets[section::DEFORMER_SRC_LOCAL_IDX] as usize + d * 4,
        )? as usize;

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
            let kf_off = read_i32(
                bytes,
                offsets[section::WARP_SRC_KEYFORM_OFF] as usize + local_idx * 4,
            )? as usize;
            let rows = read_i32(
                bytes,
                offsets[section::WARP_SRC_ROW] as usize + local_idx * 4,
            )? as u32;
            let cols = read_i32(
                bytes,
                offsets[section::WARP_SRC_COL] as usize + local_idx * 4,
            )? as u32;
            let quad = if offsets.len() > 101 && counts.warps > 0 {
                read_i32(
                    bytes,
                    offsets[section::WARP_SRC_QUAD_TRANSFORM] as usize + local_idx * 4,
                )? != 0
            } else {
                true
            };

            let pt_count = ((rows + 1) * (cols + 1)) as usize;
            let mut keyforms = Vec::with_capacity(total_combos);

            for k in 0..total_combos {
                let opacity = if kf_off + k < counts.warp_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::WARP_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    1.0
                };
                let pos_off = if kf_off + k < counts.warp_keyforms as usize {
                    read_i32(
                        bytes,
                        offsets[section::WARP_KEY_SRC_KEY_POS_OFF] as usize + (kf_off + k) * 4,
                    )? as usize
                } else {
                    0
                };

                let mut points = Vec::with_capacity(pt_count);
                for p_idx in 0..pt_count {
                    let rx = read_f32(
                        bytes,
                        offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + p_idx * 2) * 4,
                    )?;
                    let ry = read_f32(
                        bytes,
                        offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + p_idx * 2 + 1) * 4,
                    )?;
                    if is_root {
                        points.push(Vec2::new(
                            rx * ppu + canvas.origin.x,
                            canvas.origin.y - ry * ppu,
                        ));
                    } else {
                        points.push(Vec2::new(rx, ry));
                    }
                }

                let mut appearance = get_colors(section::WARP_SRC_KEY_COLOR_OFF, local_idx, k)?;
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
            warp_by_local_idx[local_idx] = id.clone();

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
            let kf_off = read_i32(
                bytes,
                offsets[section::ROTATION_SRC_KEYFORM_OFF] as usize + local_idx * 4,
            )? as usize;
            let base_angle = read_f32(
                bytes,
                offsets[section::ROTATION_SRC_BASE_ANGLE] as usize + local_idx * 4,
            )?;

            let mut keyforms = Vec::with_capacity(total_combos);
            for k in 0..total_combos {
                let opacity = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    1.0
                };
                let angle = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_ANGLE] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    0.0
                };
                let ox = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_ORIGIN_X] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    0.0
                };
                let oy = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_ORIGIN_Y] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    0.0
                };
                let scale = if kf_off + k < counts.rotation_keyforms as usize {
                    read_f32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_SCALE] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    1.0
                };
                let ref_x = if kf_off + k < counts.rotation_keyforms as usize {
                    read_i32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_REFLECT_X] as usize + (kf_off + k) * 4,
                    )? != 0
                } else {
                    false
                };
                let ref_y = if kf_off + k < counts.rotation_keyforms as usize {
                    read_i32(
                        bytes,
                        offsets[section::ROTATION_KEY_SRC_REFLECT_Y] as usize + (kf_off + k) * 4,
                    )? != 0
                } else {
                    false
                };

                let origin = if is_root {
                    kasane_core::types::PreciseVec2::new(
                        ox as f64 * ppu as f64 + canvas.origin.x as f64,
                        canvas.origin.y as f64 - oy as f64 * ppu as f64,
                    )
                } else {
                    kasane_core::types::PreciseVec2::new(ox as f64, oy as f64)
                };

                let mut appearance = get_colors(section::ROTATION_SRC_KEY_COLOR_OFF, local_idx, k)?;
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
            rotation_by_local_idx[local_idx] = id.clone();

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
        let runtime_id = read_string(
            bytes,
            offsets[section::ART_MESH_SRC_ID] as usize + m * 64,
            64,
        );
        let b_idx = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_BINDING_IDX] as usize + m * 4,
        )?;
        let kf_off = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_KEYFORM_OFF] as usize + m * 4,
        )? as usize;
        let enabled = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_ENABLE] as usize + m * 4,
        )? != 0;
        let part_idx = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_PARENT_PART_IDX] as usize + m * 4,
        )?;
        let def_idx = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_PARENT_DEFORMER_IDX] as usize + m * 4,
        )?;
        let tex_no = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_TEXTURE_NO] as usize + m * 4,
        )?;
        let flag = bytes[offsets[section::ART_MESH_SRC_DRAWABLE_FLAG] as usize + m];
        let vc = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_VERTEX_COUNT] as usize + m * 4,
        )? as usize;
        let uv_off = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_UV_OFF] as usize + m * 4,
        )? as usize;
        let idx_off = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_IDX_OFF] as usize + m * 4,
        )? as usize;
        let idx_len = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_IDX_LEN] as usize + m * 4,
        )? as usize;
        let mask_off = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_MASK_OFF] as usize + m * 4,
        )? as usize;
        let mask_len = read_i32(
            bytes,
            offsets[section::ART_MESH_SRC_MASK_LEN] as usize + m * 4,
        )? as usize;

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
                        source: format!("assets/textures/unmapped_slot_{slot}.png"),
                        width: 1,
                        height: 1,
                        sha256: "0".repeat(64),
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
        let raw_blend_mode =
            if ver >= 6 && offsets.len() > 153 && offsets[section::ART_MESH_SRC_BLEND_MODE] > 0 {
                let raw_bm = read_u32(
                    bytes,
                    offsets[section::ART_MESH_SRC_BLEND_MODE] as usize + m * 4,
                )?;
                if raw_bm != 0 {
                    Some(raw_bm)
                } else {
                    None
                }
            } else {
                None
            };

        // Read UVs
        let mut uvs = Vec::with_capacity(vc);
        for v in 0..vc {
            let u = read_f32(
                bytes,
                offsets[section::UV_SRC_XY] as usize + (uv_off + v * 2) * 4,
            )?;
            let v_val = read_f32(
                bytes,
                offsets[section::UV_SRC_XY] as usize + (uv_off + v * 2 + 1) * 4,
            )?;
            uvs.push(Vec2::new(u, 1.0 - v_val));
        }

        // Generate 1-indexed dense vertex IDs
        let vertex_ids: Vec<VertexId> = (1..=vc as u32).collect();

        // Read triangles (inverting winding by swapping 1 and 2)
        if !idx_len.is_multiple_of(3) {
            return Err(Status::error(
                "INVALID_LENGTH",
                format!("Mesh {m} has index count {idx_len} not divisible by 3"),
            ));
        }
        let mut triangles = Vec::with_capacity(idx_len / 3);
        for t in 0..(idx_len / 3) {
            let i0 = read_u16(
                bytes,
                offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3) * 2,
            )? as usize;
            let i1 = read_u16(
                bytes,
                offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3 + 1) * 2,
            )? as usize;
            let i2 = read_u16(
                bytes,
                offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3 + 2) * 2,
            )? as usize;
            if i0 >= vc || i1 >= vc || i2 >= vc {
                return Err(Status::error(
                    "INVALID_INDEX",
                    format!(
                        "Mesh {m} triangle {t} index out of bounds: ({i0}, {i1}, {i2}) with vertex count {vc}"
                    ),
                ));
            }
            if i0 == i1 || i1 == i2 || i0 == i2 {
                return Err(Status::error(
                    "REPEATED_VERTEX",
                    format!(
                        "Mesh {m} triangle {t} contains duplicate vertices: ({i0}, {i1}, {i2})"
                    ),
                ));
            }
            // Invert winding swap (render swapped 1 and 2, so swapping 1 and 2 restores source)
            triangles.push([vertex_ids[i0], vertex_ids[i2], vertex_ids[i1]]);
        }

        // Masks
        let mut masks = Vec::with_capacity(mask_len);
        for m_idx in 0..mask_len {
            let target_mesh_idx = read_i32(
                bytes,
                offsets[section::MASK_SRC_ART_MESH_IDX] as usize + (mask_off + m_idx) * 4,
            )?;
            if target_mesh_idx >= 0 && (target_mesh_idx as usize) < counts.art_meshes as usize {
                let target_id = mapping.mesh_by_index[target_mesh_idx as usize].clone();
                if !masks.contains(&target_id) {
                    masks.push(target_id);
                }
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
                read_f32(
                    bytes,
                    offsets[section::ART_MESH_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                )?
            } else {
                1.0
            };
            let d_order = if kf_off + k < counts.art_mesh_keyforms as usize {
                read_f32(
                    bytes,
                    offsets[section::ART_MESH_KEY_SRC_DRAW_ORDER] as usize + (kf_off + k) * 4,
                )?
            } else {
                m as f32
            };
            let pos_off = if kf_off + k < counts.art_mesh_keyforms as usize {
                read_i32(
                    bytes,
                    offsets[section::ART_MESH_KEY_SRC_KEY_POS_OFF] as usize + (kf_off + k) * 4,
                )? as usize
            } else {
                0
            };

            let mut positions = Vec::with_capacity(vc);
            for v in 0..vc {
                let rx = read_f32(
                    bytes,
                    offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2) * 4,
                )?;
                let ry = read_f32(
                    bytes,
                    offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2 + 1) * 4,
                )?;
                if is_root {
                    positions.push(Vec2::new(
                        rx * ppu + canvas.origin.x,
                        canvas.origin.y - ry * ppu,
                    ));
                } else {
                    positions.push(Vec2::new(rx, ry));
                }
            }

            let mut appearance = get_colors(section::ART_MESH_SRC_KEY_COLOR_OFF, m, k)?;
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
                raw_blend_mode,
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

    // Decode Glues (sections 89..100)
    let mut seen_glue_ids = std::collections::HashSet::new();
    for g_idx in 0..counts.glues as usize {
        let raw_id = read_string(
            bytes,
            offsets[section::GLUE_SRC_ID] as usize + g_idx * 64,
            64,
        );
        let runtime_id = if raw_id.trim().is_empty() || seen_glue_ids.contains(&raw_id) {
            let gen = format!("Glue_{g_idx}");
            generated_ids.push(gen.clone());
            gen
        } else {
            raw_id
        };
        seen_glue_ids.insert(runtime_id.clone());
        let id = stable_id(&doc_id, "glue", g_idx, &runtime_id);

        let binding_idx = read_i32(
            bytes,
            offsets[section::GLUE_SRC_BINDING_IDX] as usize + g_idx * 4,
        )?;
        let axes = get_binding_axes(binding_idx)?;
        let keyform_off = read_i32(
            bytes,
            offsets[section::GLUE_SRC_KEYFORM_OFF] as usize + g_idx * 4,
        )?;
        let key_len = read_i32(
            bytes,
            offsets[section::GLUE_SRC_KEY_LEN] as usize + g_idx * 4,
        )?;
        let mesh_idx_a = read_i32(
            bytes,
            offsets[section::GLUE_SRC_ART_MESH_IDX_A] as usize + g_idx * 4,
        )?;
        let mesh_idx_b = read_i32(
            bytes,
            offsets[section::GLUE_SRC_ART_MESH_IDX_B] as usize + g_idx * 4,
        )?;
        let info_off = read_i32(
            bytes,
            offsets[section::GLUE_SRC_INFO_OFF] as usize + g_idx * 4,
        )?;
        let info_len = read_i32(
            bytes,
            offsets[section::GLUE_SRC_INFO_LEN] as usize + g_idx * 4,
        )?;

        if mesh_idx_a < 0
            || mesh_idx_a as usize >= mapping.mesh_by_index.len()
            || mesh_idx_b < 0
            || mesh_idx_b as usize >= mapping.mesh_by_index.len()
        {
            return Err(Status::error(
                "INVALID_GLUE",
                format!("Glue {g_idx} references invalid mesh index {mesh_idx_a} or {mesh_idx_b}"),
            ));
        }

        let mesh_a_id = mapping.mesh_by_index[mesh_idx_a as usize].clone();
        let mesh_b_id = mapping.mesh_by_index[mesh_idx_b as usize].clone();
        let mesh_a = doc.get_mesh(&mesh_a_id).unwrap();
        let mesh_b = doc.get_mesh(&mesh_b_id).unwrap();

        let intensity = if key_len > 0 && offsets[section::GLUE_KEY_SRC_INTENSITY] > 0 {
            if keyform_off < 0 || keyform_off as usize >= counts.glue_keyforms as usize {
                return Err(Status::error(
                    "INVALID_GLUE",
                    format!("Glue {g_idx} keyform_off {keyform_off} out of bounds"),
                ));
            }
            read_f32(
                bytes,
                offsets[section::GLUE_KEY_SRC_INTENSITY] as usize + keyform_off as usize * 4,
            )?
        } else {
            1.0
        };

        if (info_len & 1) != 0 {
            return Err(Status::error(
                "FILE_CORRUPT",
                format!("Glue {g_idx} has odd info_len {info_len}"),
            ));
        }
        if info_off < 0
            || info_len < 0
            || info_off as usize + info_len as usize > counts.glue_info as usize
        {
            return Err(Status::error(
                "INVALID_GLUE",
                format!(
                    "Glue {g_idx} info range [{info_off}, {}) out of bounds (max {})",
                    info_off + info_len,
                    counts.glue_info
                ),
            ));
        }

        let mut pairs = Vec::with_capacity(info_len as usize / 2);
        for p in (0..info_len as usize).step_by(2) {
            let pos_a = read_u16(
                bytes,
                offsets[section::GLUE_INFO_SRC_POS_IDX] as usize + (info_off as usize + p) * 2,
            )? as usize;
            let wt_a = read_f32(
                bytes,
                offsets[section::GLUE_INFO_SRC_WEIGHT] as usize + (info_off as usize + p) * 4,
            )?;
            let pos_b = read_u16(
                bytes,
                offsets[section::GLUE_INFO_SRC_POS_IDX] as usize + (info_off as usize + p + 1) * 2,
            )? as usize;
            let wt_b = read_f32(
                bytes,
                offsets[section::GLUE_INFO_SRC_WEIGHT] as usize + (info_off as usize + p + 1) * 4,
            )?;

            if pos_a >= mesh_a.vertex_ids.len() || pos_b >= mesh_b.vertex_ids.len() {
                return Err(Status::error(
                    "FILE_CORRUPT",
                    format!(
                        "Glue {g_idx} pos_idx out of range for mesh (pos_a={pos_a}, vc_a={}, pos_b={pos_b}, vc_b={})",
                        mesh_a.vertex_ids.len(),
                        mesh_b.vertex_ids.len()
                    ),
                ));
            }

            pairs.push(GlueVertexPair {
                vertex_a: mesh_a.vertex_ids[pos_a],
                vertex_b: mesh_b.vertex_ids[pos_b],
                weight_a: wt_a,
                weight_b: wt_b,
            });
        }

        let glue = Glue {
            id: id.clone(),
            runtime_id: runtime_id.clone(),
            name: runtime_id.clone(),
            mesh_a_id,
            mesh_b_id,
            pairs,
            intensity,
            binding: if axes.is_empty() {
                None
            } else {
                let mut keyforms = Vec::new();
                for k in 0..key_len as usize {
                    keyforms.push(kasane_core::types::GlueKeyform {
                        intensity: read_f32(
                            bytes,
                            offsets[section::GLUE_KEY_SRC_INTENSITY] as usize
                                + (keyform_off as usize + k) * 4,
                        )?,
                    });
                }
                Some(kasane_core::types::GlueBinding { axes, keyforms })
            },
        };
        check_status!(doc.create_glue(glue).status);
        mapping.glue_by_index.push(id);
    }

    // 9. Decode BlendShapes (BlendShapeKeyTables, BlendShapeConstraints, BlendShapeBindings)
    if counts.blend_key_tables > 0 {
        for i in 0..counts.blend_key_tables as usize {
            let mut owner_param: Option<usize> = None;
            for p in 0..counts.parameters as usize {
                let p_off = read_i32(
                    bytes,
                    offsets[section::PARAM_SRC_BLEND_KEY_TABLE_OFF] as usize + p * 4,
                )? as usize;
                let p_len = read_i32(
                    bytes,
                    offsets[section::PARAM_SRC_BLEND_KEY_TABLE_LEN] as usize + p * 4,
                )? as usize;
                if i >= p_off && i < p_off + p_len {
                    owner_param = Some(p);
                    break;
                }
            }
            let p = owner_param.ok_or_else(|| {
                Status::error(
                    "INVALID_KEY_TABLE",
                    format!("Blend key table {i} not referenced by any parameter"),
                )
            })?;
            let parameter_id = mapping.parameter_by_index[p].clone();
            let keys_off = read_i32(
                bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_KEYS_OFF] as usize + i * 4,
            )? as usize;
            let keys_len = read_i32(
                bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_KEYS_LEN] as usize + i * 4,
            )? as usize;
            let base_key_idx = read_i32(
                bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_BASE_KEY_IDX] as usize + i * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(keys_len);
            for k in 0..keys_len {
                keys.push(read_f32(
                    bytes,
                    offsets[section::KEYS_SRC_KEY] as usize + (keys_off + k) * 4,
                )?);
            }
            let bkt_id = stable_id(&doc_id, "blend_key_table", i, &format!("bkt_{i}"));
            mapping.blend_key_table_by_index.push(bkt_id.clone());
            check_status!(
                doc.create_blend_key_table(BlendShapeKeyTable {
                    id: bkt_id,
                    parameter_id,
                    keys,
                    base_key_idx,
                })
                .status
            );
        }
    }

    if counts.bs_constraints > 0 {
        for i in 0..counts.bs_constraints as usize {
            let param_idx = read_i32(
                bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_PARAMETER_IDX] as usize + i * 4,
            )? as usize;
            if param_idx >= mapping.parameter_by_index.len() {
                return Err(Status::error(
                    "INVALID_CONSTRAINT",
                    format!("Constraint {i}: invalid parameter {param_idx}"),
                ));
            }
            let parameter_id = mapping.parameter_by_index[param_idx].clone();
            let val_off = read_i32(
                bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_VALUE_OFF] as usize + i * 4,
            )? as usize;
            let val_len = read_i32(
                bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_VALUE_LEN] as usize + i * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(val_len);
            let mut weights = Vec::with_capacity(val_len);
            for k in 0..val_len {
                keys.push(read_f32(
                    bytes,
                    offsets[section::BLEND_CONSTRAINT_VAL_SRC_KEY] as usize + (val_off + k) * 4,
                )?);
                weights.push(read_f32(
                    bytes,
                    offsets[section::BLEND_CONSTRAINT_VAL_SRC_WEIGHT] as usize + (val_off + k) * 4,
                )?);
            }
            let bsc_id = stable_id(&doc_id, "blend_constraint", i, &format!("bsc_{i}"));
            mapping.blend_constraint_by_index.push(bsc_id.clone());
            check_status!(
                doc.create_blend_constraint(BlendShapeConstraint {
                    id: bsc_id,
                    parameter_id,
                    keys,
                    weights,
                })
                .status
            );
        }
    }

    if ver >= 6 && counts.offscreens > 0 {
        for i in 0..counts.offscreens as usize {
            let owner_part_idx = read_i32(
                bytes,
                offsets[section::OFFSCREEN_SRC_OWNER_IDX] as usize + i * 4,
            )? as usize;
            if owner_part_idx >= mapping.part_by_index.len() {
                return Err(Status::error(
                    "INVALID_OFFSCREEN",
                    format!("Offscreen {i}: invalid owner part index {owner_part_idx}"),
                ));
            }
            let part_id = mapping.part_by_index[owner_part_idx].clone();
            let part_runtime_id = mapping
                .parts
                .iter()
                .find(|(_, id)| *id == &part_id)
                .map(|(r, _)| r.clone())
                .unwrap_or_else(|| format!("Part{owner_part_idx}"));
            let part_name = doc
                .get_part(&part_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Part{owner_part_idx}"));

            let flags = bytes[offsets[section::OFFSCREEN_SRC_DRAWABLE_FLAG] as usize + i];
            let blend_mode = read_u32(
                bytes,
                offsets[section::OFFSCREEN_SRC_BLEND_MODE] as usize + i * 4,
            )?;
            let mask_off = read_i32(
                bytes,
                offsets[section::OFFSCREEN_SRC_MASK_OFF] as usize + i * 4,
            )? as usize;
            let mask_len = read_i32(
                bytes,
                offsets[section::OFFSCREEN_SRC_MASK_LEN] as usize + i * 4,
            )? as usize;
            let mut masks = Vec::with_capacity(mask_len);
            for m in 0..mask_len {
                let mesh_idx = read_i32(
                    bytes,
                    offsets[section::MASK_SRC_ART_MESH_IDX] as usize + (mask_off + m) * 4,
                )? as usize;
                if mesh_idx >= mapping.mesh_by_index.len() {
                    return Err(Status::error(
                        "INVALID_OFFSCREEN_MASK",
                        format!("Offscreen {i}: invalid mask mesh index {mesh_idx}"),
                    ));
                }
                masks.push(mapping.mesh_by_index[mesh_idx].clone());
            }

            let part_keyform_off = read_i32(
                bytes,
                offsets[section::PART_SRC_KEYFORM_OFF] as usize + owner_part_idx * 4,
            )? as usize;
            let part_key_len = read_i32(
                bytes,
                offsets[section::PART_SRC_KEY_LEN] as usize + owner_part_idx * 4,
            )? as usize;
            let mut keyforms = Vec::new();
            let mut part_keyform_indices = Vec::with_capacity(part_key_len);

            for k in 0..part_key_len {
                let global_kf_idx = read_i32(
                    bytes,
                    offsets[section::PART_KEY_SRC_KEY_IDX] as usize + (part_keyform_off + k) * 4,
                )?;
                if global_kf_idx < 0 {
                    part_keyform_indices.push(-1);
                } else {
                    let g = global_kf_idx as usize;
                    if g >= counts.offscreen_keyforms as usize {
                        return Err(Status::error(
                            "INDEX_OUT_OF_BOUNDS",
                            "Offscreen keyform index exceeds its table",
                        ));
                    }
                    let opacity = read_f32(
                        bytes,
                        offsets[section::OFFSCREEN_KEY_SRC_OPACITY] as usize + g * 4,
                    )?;
                    let mul_idx = read_i32(
                        bytes,
                        offsets[section::OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF] as usize + g * 4,
                    )?;
                    if mul_idx >= counts.keyform_mul_colors {
                        return Err(Status::error(
                            "INDEX_OUT_OF_BOUNDS",
                            "Offscreen multiply color index exceeds its pool",
                        ));
                    }
                    let multiply = if mul_idx >= 0
                        && offsets.len() > 110
                        && offsets[section::KEYFORM_MUL_COLOR_SRC_R] > 0
                    {
                        Some([
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_R] as usize
                                    + mul_idx as usize * 4,
                            )?,
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_G] as usize
                                    + mul_idx as usize * 4,
                            )?,
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_B] as usize
                                    + mul_idx as usize * 4,
                            )?,
                        ])
                    } else {
                        None
                    };
                    let scr_idx = read_i32(
                        bytes,
                        offsets[section::OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF] as usize + g * 4,
                    )?;
                    if scr_idx >= counts.keyform_scr_colors {
                        return Err(Status::error(
                            "INDEX_OUT_OF_BOUNDS",
                            "Offscreen screen color index exceeds its pool",
                        ));
                    }
                    let screen = if scr_idx >= 0
                        && offsets.len() > 113
                        && offsets[section::KEYFORM_SCR_COLOR_SRC_R] > 0
                    {
                        Some([
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_SCR_COLOR_SRC_R] as usize
                                    + scr_idx as usize * 4,
                            )?,
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_SCR_COLOR_SRC_G] as usize
                                    + scr_idx as usize * 4,
                            )?,
                            read_f32(
                                bytes,
                                offsets[section::KEYFORM_SCR_COLOR_SRC_B] as usize
                                    + scr_idx as usize * 4,
                            )?,
                        ])
                    } else {
                        None
                    };
                    part_keyform_indices.push(keyforms.len() as i32);
                    keyforms.push(OffscreenKeyform {
                        opacity,
                        multiply,
                        screen,
                    });
                }
            }

            let runtime_id = format!("Offscreen_{part_runtime_id}");
            let name = format!("{part_name} (Offscreen)");
            let os_id = stable_id(&doc_id, "offscreen", i, &runtime_id);
            mapping.offscreen_by_index.push(os_id.clone());
            check_status!(
                doc.create_offscreen(Offscreen {
                    id: os_id,
                    runtime_id,
                    name,
                    part_id,
                    blend_mode,
                    flags,
                    masks,
                    part_keyform_indices,
                    keyforms,
                })
                .status
            );
        }
    }

    if counts.blend_bindings > 0 {
        let mut binding_targets: HashMap<usize, (String, BlendShapeTargetKind)> = HashMap::new();
        // The current Document represents one ordered BlendShape group per target.
        // Do not silently flatten repeated groups (Core clamps between groups),
        // or overwrite bindings shared by multiple targets.
        let mut target_groups = std::collections::HashSet::new();
        let mut register_group = |target_id: &str, kind, start, len| -> Result<(), Status> {
            if !target_groups.insert(target_id.to_owned()) {
                return Err(Status::error(
                    "UNSUPPORTED_FEATURE",
                    format!(
                        "{target_id}: multiple BlendShape target groups are not yet representable"
                    ),
                ));
            }
            checked_window(start, len, counts.blend_bindings as usize, "blend_binding")?;
            for binding in start..start + len {
                if binding_targets
                    .insert(binding, (target_id.to_owned(), kind))
                    .is_some()
                {
                    return Err(Status::error("UNSUPPORTED_FEATURE", format!("Blend binding {binding}: shared target windows are not yet representable")));
                }
            }
            Ok(())
        };

        for i in 0..counts.bs_warps as usize {
            let target_local = read_i32(
                bytes,
                offsets[section::BS_WARP_SRC_TARGET_IDX] as usize + i * 4,
            )? as usize;
            let target_id =
                checked_reference(&warp_by_local_idx, target_local, "bs_warp.target")?.clone();
            let b_off = read_i32(
                bytes,
                offsets[section::BS_WARP_SRC_BS_BINDING_OFF] as usize + i * 4,
            )? as usize;
            let b_len = read_i32(
                bytes,
                offsets[section::BS_WARP_SRC_BS_BINDING_LEN] as usize + i * 4,
            )? as usize;
            register_group(&target_id, BlendShapeTargetKind::Warp, b_off, b_len)?;
        }

        if ver >= 5 {
            for i in 0..counts.bs_rotations as usize {
                let target_local = read_i32(
                    bytes,
                    offsets[section::BS_ROTATION_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(&rotation_by_local_idx, target_local, "bs_rotation.target")?
                        .clone();
                let b_off = read_i32(
                    bytes,
                    offsets[section::BS_ROTATION_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    bytes,
                    offsets[section::BS_ROTATION_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Rotation, b_off, b_len)?;
            }

            for i in 0..counts.bs_parts as usize {
                let target_part = read_i32(
                    bytes,
                    offsets[section::BS_PART_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(&mapping.part_by_index, target_part, "bs_part.target")?
                        .clone();
                let b_off = read_i32(
                    bytes,
                    offsets[section::BS_PART_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    bytes,
                    offsets[section::BS_PART_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Part, b_off, b_len)?;
            }

            for i in 0..counts.bs_glues as usize {
                let target_glue = read_i32(
                    bytes,
                    offsets[section::BS_GLUE_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(&mapping.glue_by_index, target_glue, "bs_glue.target")?
                        .clone();
                let b_off = read_i32(
                    bytes,
                    offsets[section::BS_GLUE_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    bytes,
                    offsets[section::BS_GLUE_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Glue, b_off, b_len)?;
            }

            if ver >= 6 {
                for i in 0..counts.bs_offscreens as usize {
                    let target_os = read_i32(
                        bytes,
                        offsets[section::BS_OFFSCREEN_SRC_TARGET_IDX] as usize + i * 4,
                    )? as usize;
                    let target_id = checked_reference(
                        &mapping.offscreen_by_index,
                        target_os,
                        "bs_offscreen.target",
                    )?
                    .clone();
                    let b_off = read_i32(
                        bytes,
                        offsets[section::BS_OFFSCREEN_SRC_BS_BINDING_OFF] as usize + i * 4,
                    )? as usize;
                    let b_len = read_i32(
                        bytes,
                        offsets[section::BS_OFFSCREEN_SRC_BS_BINDING_LEN] as usize + i * 4,
                    )? as usize;
                    register_group(&target_id, BlendShapeTargetKind::Offscreen, b_off, b_len)?;
                }
            }
        }

        for i in 0..counts.bs_art_meshes as usize {
            let target_mesh = read_i32(
                bytes,
                offsets[section::BS_ART_MESH_SRC_TARGET_IDX] as usize + i * 4,
            )? as usize;
            let target_id =
                checked_reference(&mapping.mesh_by_index, target_mesh, "bs_mesh.target")?.clone();
            let b_off = read_i32(
                bytes,
                offsets[section::BS_ART_MESH_SRC_BS_BINDING_OFF] as usize + i * 4,
            )? as usize;
            let b_len = read_i32(
                bytes,
                offsets[section::BS_ART_MESH_SRC_BS_BINDING_LEN] as usize + i * 4,
            )? as usize;
            register_group(&target_id, BlendShapeTargetKind::Mesh, b_off, b_len)?;
        }

        let get_bs_colors = |sec_mul: usize,
                             sec_scr: usize,
                             key_idx: usize|
         -> Result<(Option<[f32; 3]>, Option<[f32; 3]>), Status> {
            if ver < 5 {
                return Ok((None, None));
            }
            let mul = if offsets.len() > sec_mul && offsets[sec_mul] > 0 {
                let idx = read_i32(bytes, offsets[sec_mul] as usize + key_idx * 4)?;
                if idx >= 0 {
                    Some([
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_MUL_COLOR_SRC_R] as usize + idx as usize * 4,
                        )?,
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_MUL_COLOR_SRC_G] as usize + idx as usize * 4,
                        )?,
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_MUL_COLOR_SRC_B] as usize + idx as usize * 4,
                        )?,
                    ])
                } else {
                    None
                }
            } else {
                None
            };
            let scr = if offsets.len() > sec_scr && offsets[sec_scr] > 0 {
                let idx = read_i32(bytes, offsets[sec_scr] as usize + key_idx * 4)?;
                if idx >= 0 {
                    Some([
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_SCR_COLOR_SRC_R] as usize + idx as usize * 4,
                        )?,
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_SCR_COLOR_SRC_G] as usize + idx as usize * 4,
                        )?,
                        read_f32(
                            bytes,
                            offsets[section::KEYFORM_SCR_COLOR_SRC_B] as usize + idx as usize * 4,
                        )?,
                    ])
                } else {
                    None
                }
            } else {
                None
            };
            Ok((mul, scr))
        };

        for b in 0..counts.blend_bindings as usize {
            let (target_id, target_kind) = binding_targets
                .get(&b)
                .ok_or_else(|| {
                    Status::error("ORPHAN_BINDING", format!("Blend binding {b} has no target"))
                })?
                .clone();

            let kt_idx = read_i32(
                bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_TABLE_IDX] as usize + b * 4,
            )? as usize;
            let key_table_id = checked_reference(
                &mapping.blend_key_table_by_index,
                kt_idx,
                "blend_binding.key_table",
            )?
            .clone();
            let key_bs_off = read_i32(
                bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_BS_OFF] as usize + b * 4,
            )? as usize;
            let key_bs_len = read_i32(
                bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_BS_LEN] as usize + b * 4,
            )? as usize;
            let c_off = read_i32(
                bytes,
                offsets[section::BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_OFF] as usize + b * 4,
            )? as usize;
            let c_len = read_i32(
                bytes,
                offsets[section::BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_LEN] as usize + b * 4,
            )? as usize;

            checked_window(
                c_off,
                c_len,
                counts.bs_constraint_idx as usize,
                "blend_binding.constraints",
            )?;
            let keyform_count = match target_kind {
                BlendShapeTargetKind::Part => counts.part_keyforms,
                BlendShapeTargetKind::Warp => counts.warp_keyforms,
                BlendShapeTargetKind::Rotation => counts.rotation_keyforms,
                BlendShapeTargetKind::Mesh => counts.art_mesh_keyforms,
                BlendShapeTargetKind::Glue => counts.glue_keyforms,
                BlendShapeTargetKind::Offscreen => counts.offscreen_keyforms,
            };
            checked_window(
                key_bs_off,
                key_bs_len,
                keyform_count as usize,
                "blend_binding.keyforms",
            )?;
            let mut constraint_ids = Vec::with_capacity(c_len);
            for c in 0..c_len {
                let c_idx = read_i32(
                    bytes,
                    offsets[section::BLEND_CONSTRAINT_IDX_SRC_CONSTRAINT_IDX] as usize
                        + (c_off + c) * 4,
                )? as usize;
                constraint_ids.push(
                    checked_reference(
                        &mapping.blend_constraint_by_index,
                        c_idx,
                        "blend_binding.constraint",
                    )?
                    .clone(),
                );
            }

            let keyforms = match target_kind {
                BlendShapeTargetKind::Part => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let draw_order = read_f32(
                            bytes,
                            offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize
                                + (key_bs_off + k) * 4,
                        )?;
                        forms.push(DeltaPartKeyform { draw_order });
                    }
                    DeltaKeyforms::Part(forms)
                }
                BlendShapeTargetKind::Warp => {
                    let warp = doc.get_transform(&target_id).unwrap();
                    let pt_count = ((warp.rows + 1) * (warp.columns + 1)) as usize;
                    let is_root = warp.parent_id.is_empty();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            bytes,
                            offsets[section::WARP_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let pos_off = read_i32(
                            bytes,
                            offsets[section::WARP_KEY_SRC_KEY_POS_OFF] as usize + ki * 4,
                        )? as usize;
                        let mut points = Vec::with_capacity(pt_count);
                        for p in 0..pt_count {
                            let rx = read_f32(
                                bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + p * 2) * 4,
                            )?;
                            let ry = read_f32(
                                bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize
                                    + (pos_off + p * 2 + 1) * 4,
                            )?;
                            if is_root {
                                points.push(Vec2::new(rx * ppu, -ry * ppu));
                            } else {
                                points.push(Vec2::new(rx, ry));
                            }
                        }
                        let (mul, scr) = get_bs_colors(
                            section::WARP_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::WARP_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaWarpKeyform {
                            points,
                            opacity: Some(op),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Warp(forms)
                }
                BlendShapeTargetKind::Rotation => {
                    let rot = doc.get_transform(&target_id).unwrap();
                    let is_root = rot.parent_id.is_empty();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            bytes,
                            offsets[section::ROTATION_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let ang = read_f32(
                            bytes,
                            offsets[section::ROTATION_KEY_SRC_ANGLE] as usize + ki * 4,
                        )?;
                        let ox = read_f32(
                            bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_X] as usize + ki * 4,
                        )?;
                        let oy = read_f32(
                            bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_Y] as usize + ki * 4,
                        )?;
                        let sc = read_f32(
                            bytes,
                            offsets[section::ROTATION_KEY_SRC_SCALE] as usize + ki * 4,
                        )?;
                        let origin = if is_root {
                            Vec2::new(ox * ppu, -oy * ppu)
                        } else {
                            Vec2::new(ox, oy)
                        };
                        let (mul, scr) = get_bs_colors(
                            section::ROTATION_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::ROTATION_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaRotationKeyform {
                            origin: Some(origin),
                            angle: Some(ang),
                            scale: Some(sc),
                            opacity: Some(op),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Rotation(forms)
                }
                BlendShapeTargetKind::Mesh => {
                    let mesh = doc.get_mesh(&target_id).unwrap();
                    let vc = mesh.vertex_ids.len();
                    let is_root = mesh.deformer_id.is_empty();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            bytes,
                            offsets[section::ART_MESH_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let d_order = read_f32(
                            bytes,
                            offsets[section::ART_MESH_KEY_SRC_DRAW_ORDER] as usize + ki * 4,
                        )?;
                        let pos_off = read_i32(
                            bytes,
                            offsets[section::ART_MESH_KEY_SRC_KEY_POS_OFF] as usize + ki * 4,
                        )? as usize;
                        let mut positions = Vec::with_capacity(vc);
                        for v in 0..vc {
                            let rx = read_f32(
                                bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2) * 4,
                            )?;
                            let ry = read_f32(
                                bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize
                                    + (pos_off + v * 2 + 1) * 4,
                            )?;
                            if is_root {
                                positions.push(Vec2::new(rx * ppu, -ry * ppu));
                            } else {
                                positions.push(Vec2::new(rx, ry));
                            }
                        }
                        let (mul, scr) = get_bs_colors(
                            section::ART_MESH_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::ART_MESH_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaMeshKeyform {
                            positions,
                            opacity: Some(op),
                            draw_order: Some(d_order),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Mesh(forms)
                }
                BlendShapeTargetKind::Glue => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let intensity = read_f32(
                            bytes,
                            offsets[section::GLUE_KEY_SRC_INTENSITY] as usize + ki * 4,
                        )?;
                        forms.push(DeltaGlueKeyform { intensity });
                    }
                    DeltaKeyforms::Glue(forms)
                }
                BlendShapeTargetKind::Offscreen => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            bytes,
                            offsets[section::OFFSCREEN_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let (mul, scr) = get_bs_colors(
                            section::OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaOffscreenKeyform {
                            opacity: op,
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Offscreen(forms)
                }
            };

            let b_id = stable_id(&doc_id, "blend_binding", b, &format!("bb_{b}"));
            mapping.blend_binding_by_index.push(b_id.clone());
            check_status!(
                doc.create_blend_binding(BlendShapeBinding {
                    id: b_id,
                    target_id,
                    target_kind,
                    key_table_id,
                    constraint_ids,
                    keyforms,
                })
                .status
            );
        }
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
        if read_i32(
            bytes,
            offsets[section::DRAW_GROUP_OBJ_SRC_TYPE] as usize + index * 4,
        )? == 1
        {
            let part = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_OBJ_SRC_IDX] as usize + index * 4,
            )?;
            let child = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_OBJ_SRC_SELF_GROUP_IDX] as usize + index * 4,
            )?;
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
        let start = read_i32(
            bytes,
            offsets[section::DRAW_GROUP_SRC_OBJ_OFF] as usize + index * 4,
        )?;
        let length = read_i32(
            bytes,
            offsets[section::DRAW_GROUP_SRC_OBJ_LEN] as usize + index * 4,
        )?;
        if start < 0 || length < 0 || start as i64 + length as i64 > counts.draw_items as i64 {
            return Err(Status::error(
                "INVALID_DRAW_GROUP",
                format!("Invalid item range in group {index}"),
            ));
        }
        let mut items = Vec::new();
        for item in start as usize..(start + length) as usize {
            let kind = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_OBJ_SRC_TYPE] as usize + item * 4,
            )?;
            let object = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_OBJ_SRC_IDX] as usize + item * 4,
            )?;
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
            min_order: read_i32(
                bytes,
                offsets[section::DRAW_GROUP_SRC_MIN_ORDER] as usize + index * 4,
            )?,
            max_order: read_i32(
                bytes,
                offsets[section::DRAW_GROUP_SRC_MAX_ORDER] as usize + index * 4,
            )?,
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
