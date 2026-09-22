use std::collections::HashSet;

use kasane_core::types::{Glue, GlueVertexPair, Status};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{read_f32, read_i32, read_string, read_u16, stable_id};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_glues(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;

        let mut seen_glue_ids = HashSet::new();
        for g_idx in 0..counts.glues as usize {
            let raw_id = read_string(
                self.bytes,
                offsets[section::GLUE_SRC_ID] as usize + g_idx * 64,
                64,
            );
            let runtime_id = if raw_id.trim().is_empty() || seen_glue_ids.contains(&raw_id) {
                let gen = format!("Glue_{g_idx}");
                self.generated_ids.push(gen.clone());
                gen
            } else {
                raw_id
            };
            seen_glue_ids.insert(runtime_id.clone());
            let id = stable_id(&self.doc_id, "glue", g_idx, &runtime_id);

            let binding_idx = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_BINDING_IDX] as usize + g_idx * 4,
            )?;
            let axes = self.get_binding_axes(binding_idx)?;
            let keyform_off = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_KEYFORM_OFF] as usize + g_idx * 4,
            )?;
            let key_len = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_KEY_LEN] as usize + g_idx * 4,
            )?;
            let mesh_idx_a = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_ART_MESH_IDX_A] as usize + g_idx * 4,
            )?;
            let mesh_idx_b = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_ART_MESH_IDX_B] as usize + g_idx * 4,
            )?;
            let info_off = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_INFO_OFF] as usize + g_idx * 4,
            )?;
            let info_len = read_i32(
                self.bytes,
                offsets[section::GLUE_SRC_INFO_LEN] as usize + g_idx * 4,
            )?;

            if mesh_idx_a < 0
                || mesh_idx_a as usize >= self.mapping.mesh_by_index.len()
                || mesh_idx_b < 0
                || mesh_idx_b as usize >= self.mapping.mesh_by_index.len()
            {
                return Err(Status::error(
                    "INVALID_GLUE",
                    format!(
                        "Glue {g_idx} references invalid mesh index {mesh_idx_a} or {mesh_idx_b}"
                    ),
                ));
            }

            let mesh_a_id = self.mapping.mesh_by_index[mesh_idx_a as usize].clone();
            let mesh_b_id = self.mapping.mesh_by_index[mesh_idx_b as usize].clone();
            let mesh_a = self.doc.get_mesh(&mesh_a_id).unwrap();
            let mesh_b = self.doc.get_mesh(&mesh_b_id).unwrap();

            let intensity = if key_len > 0 && offsets[section::GLUE_KEY_SRC_INTENSITY] > 0 {
                if keyform_off < 0 || keyform_off as usize >= counts.glue_keyforms as usize {
                    return Err(Status::error(
                        "INVALID_GLUE",
                        format!("Glue {g_idx} keyform_off {keyform_off} out of bounds"),
                    ));
                }
                read_f32(
                    self.bytes,
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
                    self.bytes,
                    offsets[section::GLUE_INFO_SRC_POS_IDX] as usize + (info_off as usize + p) * 2,
                )? as usize;
                let wt_a = read_f32(
                    self.bytes,
                    offsets[section::GLUE_INFO_SRC_WEIGHT] as usize + (info_off as usize + p) * 4,
                )?;
                let pos_b = read_u16(
                    self.bytes,
                    offsets[section::GLUE_INFO_SRC_POS_IDX] as usize
                        + (info_off as usize + p + 1) * 2,
                )? as usize;
                let wt_b = read_f32(
                    self.bytes,
                    offsets[section::GLUE_INFO_SRC_WEIGHT] as usize
                        + (info_off as usize + p + 1) * 4,
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
                                self.bytes,
                                offsets[section::GLUE_KEY_SRC_INTENSITY] as usize
                                    + (keyform_off as usize + k) * 4,
                            )?,
                        });
                    }
                    Some(kasane_core::types::GlueBinding { axes, keyforms })
                },
            };
            check_status!(self.doc.create_glue(glue).status);
            self.mapping.glue_by_index.push(id);
        }

        Ok(())
    }
}
