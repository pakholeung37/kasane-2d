use std::collections::HashMap;

use kasane_core::types::{Appearance, BindingAxis, Canvas, Status, Vec2};
use kasane_core::Document;

use crate::inspector::{inspect_structure, Moc3InspectionReport};
use crate::schema::section;

use super::helpers::{read_f32, read_i32, read_string, stable_id};
use super::{ImportIdMapping, TextureSlotInfo};

pub type BlendShapeColors = (Option<[f32; 3]>, Option<[f32; 3]>);

pub(super) struct Moc3DecoderContext<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) inspection: Moc3InspectionReport,
    pub(super) doc_id: String,
    pub(super) doc: Document,
    pub(super) mapping: ImportIdMapping,
    pub(super) generated_ids: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) key_table_owners: Vec<Option<usize>>,
    pub(super) texture_by_slot: HashMap<usize, &'a TextureSlotInfo>,
}

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn new(bytes: &'a [u8], textures: &'a [TextureSlotInfo]) -> Result<Self, Status> {
        let inspection = inspect_structure(bytes)?;
        let offsets = &inspection.section_offsets;
        let counts = &inspection.counts;

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
        let mut texture_by_slot = HashMap::new();
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

        // Resolve key-table ownership once. Preserve the first owner's precedence.
        let mut key_table_owners = vec![None; counts.key_tables as usize];
        for p in 0..counts.parameters as usize {
            let offset = read_i32(
                bytes,
                offsets[section::PARAM_SRC_KEY_TABLE_OFF] as usize + p * 4,
            )?;
            let len = read_i32(
                bytes,
                offsets[section::PARAM_SRC_KEY_TABLE_LEN] as usize + p * 4,
            )?;
            if len == 0 {
                continue;
            }
            let end = offset
                .checked_add(len)
                .filter(|&end| offset >= 0 && len >= 0 && end <= counts.key_tables)
                .ok_or_else(|| {
                    Status::error("INVALID_BINDING", "Invalid parameter key-table range")
                })?;
            for owner in &mut key_table_owners[offset as usize..end as usize] {
                if owner.is_none() {
                    *owner = Some(p);
                }
            }
        }

        Ok(Self {
            bytes,
            inspection,
            doc_id,
            doc,
            mapping,
            generated_ids,
            warnings,
            key_table_owners,
            texture_by_slot,
        })
    }

    pub(super) fn get_colors(
        &self,
        section_idx: usize,
        object: usize,
        key: usize,
    ) -> Result<Appearance, Status> {
        let mut appearance = Appearance::default();
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;

        if ver < 4 || (counts.keyform_mul_colors == 0 && counts.keyform_scr_colors == 0) {
            return Ok(appearance);
        }
        let base = read_i32(self.bytes, offsets[section_idx] as usize + object * 4)?;
        if base < 0 {
            return Ok(appearance);
        }
        let index = (base as i64) + key as i64;
        if index >= counts.keyform_mul_colors as i64 || index >= counts.keyform_scr_colors as i64 {
            return Err(Status::error("INVALID_COLOR_REFERENCE", format!("section {section_idx} object[{object}] keyform[{key}]: color index {index} is outside the color pools")));
        }
        for channel in 0..3 {
            appearance.multiply[channel] = read_f32(
                self.bytes,
                offsets[section::KEYFORM_MUL_COLOR_SRC_R + channel] as usize + index as usize * 4,
            )?;
            appearance.screen[channel] = read_f32(
                self.bytes,
                offsets[section::KEYFORM_SCR_COLOR_SRC_R + channel] as usize + index as usize * 4,
            )?;
        }
        Ok(appearance)
    }

    pub(super) fn get_binding_axes(&self, b_idx: i32) -> Result<Vec<BindingAxis>, Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;

        if b_idx < 0 || b_idx >= counts.bindings {
            return Err(Status::error(
                "INVALID_BINDING",
                format!("Invalid binding index {b_idx}"),
            ));
        }
        let kt_off = read_i32(
            self.bytes,
            offsets[section::BINDING_SRC_KEY_TABLE_IDX_OFF] as usize + b_idx as usize * 4,
        )? as usize;
        let kt_len = read_i32(
            self.bytes,
            offsets[section::BINDING_SRC_KEY_TABLE_IDX_LEN] as usize + b_idx as usize * 4,
        )? as usize;
        let mut axes = Vec::with_capacity(kt_len);

        for a in 0..kt_len {
            let kt = read_i32(
                self.bytes,
                offsets[section::KEY_TABLE_IDX_SRC_IDX] as usize + (kt_off + a) * 4,
            )?;
            let param_idx = usize::try_from(kt)
                .ok()
                .and_then(|kt| self.key_table_owners.get(kt))
                .copied()
                .flatten();
            let p = param_idx.ok_or_else(|| {
                Status::error(
                    "INVALID_BINDING",
                    format!("Key table {kt} not found in any parameter ranges"),
                )
            })?;

            let keys_off = read_i32(
                self.bytes,
                offsets[section::KEY_TABLE_SRC_KEYS_OFF] as usize + kt as usize * 4,
            )? as usize;
            let keys_len = read_i32(
                self.bytes,
                offsets[section::KEY_TABLE_SRC_KEYS_LEN] as usize + kt as usize * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(keys_len);
            for k in 0..keys_len {
                keys.push(read_f32(
                    self.bytes,
                    offsets[section::KEYS_SRC_KEY] as usize + (keys_off + k) * 4,
                )?);
            }

            axes.push(BindingAxis {
                parameter_id: self.mapping.parameter_by_index[p].clone(),
                keys,
            });
        }
        Ok(axes)
    }

    pub(super) fn combo_keys(axes: &[BindingAxis], combo_idx: usize) -> Vec<f32> {
        let mut result = Vec::with_capacity(axes.len());
        let mut stride = 1usize;
        for axis in axes {
            let k_idx = (combo_idx / stride) % axis.keys.len();
            result.push(axis.keys[k_idx]);
            stride *= axis.keys.len();
        }
        result
    }

    pub(super) fn get_bs_colors(
        &self,
        sec_mul: usize,
        sec_scr: usize,
        key_idx: usize,
    ) -> Result<BlendShapeColors, Status> {
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;

        if ver < 5 {
            return Ok((None, None));
        }
        let mul = if offsets.len() > sec_mul && offsets[sec_mul] > 0 {
            let idx = read_i32(self.bytes, offsets[sec_mul] as usize + key_idx * 4)?;
            if idx >= 0 {
                Some([
                    read_f32(
                        self.bytes,
                        offsets[section::KEYFORM_MUL_COLOR_SRC_R] as usize + idx as usize * 4,
                    )?,
                    read_f32(
                        self.bytes,
                        offsets[section::KEYFORM_MUL_COLOR_SRC_G] as usize + idx as usize * 4,
                    )?,
                    read_f32(
                        self.bytes,
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
            let idx = read_i32(self.bytes, offsets[sec_scr] as usize + key_idx * 4)?;
            if idx >= 0 {
                Some([
                    read_f32(
                        self.bytes,
                        offsets[section::KEYFORM_SCR_COLOR_SRC_R] as usize + idx as usize * 4,
                    )?,
                    read_f32(
                        self.bytes,
                        offsets[section::KEYFORM_SCR_COLOR_SRC_G] as usize + idx as usize * 4,
                    )?,
                    read_f32(
                        self.bytes,
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
    }
}
