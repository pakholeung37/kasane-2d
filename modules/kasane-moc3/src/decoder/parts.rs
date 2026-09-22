use kasane_core::types::{Part, SceneBinding, Status};
use kasane_core::{PartKeyform, SceneTrack};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{parent_order, read_f32, read_i32, read_string, stable_id};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_parts(&mut self) -> Result<Vec<SceneBinding>, Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;

        let mut part_parent_indices = Vec::with_capacity(counts.parts as usize);
        for p in 0..counts.parts as usize {
            part_parent_indices.push(read_i32(
                self.bytes,
                offsets[section::PART_SRC_PARENT_PART_IDX] as usize + p * 4,
            )?);
        }

        let part_creation_order = parent_order(&part_parent_indices, "Part")?;
        let mut part_scene_bindings = Vec::new();

        for &p in &part_creation_order {
            let id = self.mapping.part_by_index[p].clone();
            let runtime_id = read_string(
                self.bytes,
                offsets[section::PART_SRC_ID] as usize + p * 64,
                64,
            );
            let b_idx = read_i32(
                self.bytes,
                offsets[section::PART_SRC_BINDING_IDX] as usize + p * 4,
            )?;
            let kf_off = read_i32(
                self.bytes,
                offsets[section::PART_SRC_KEYFORM_OFF] as usize + p * 4,
            )? as usize;
            let _kf_len = read_i32(
                self.bytes,
                offsets[section::PART_SRC_KEY_LEN] as usize + p * 4,
            )? as usize;
            let enabled = read_i32(
                self.bytes,
                offsets[section::PART_SRC_ENABLE] as usize + p * 4,
            )? != 0;
            let parent_idx = part_parent_indices[p];

            let parent_id = if parent_idx >= 0 && (parent_idx as usize) < counts.parts as usize {
                self.mapping.part_by_index[parent_idx as usize].clone()
            } else {
                String::new()
            };

            let axes = self.get_binding_axes(b_idx)?;
            let is_bound = !axes.is_empty();
            let total_combos = if is_bound {
                axes.iter().map(|a| a.keys.len()).product()
            } else {
                1
            };

            let base_draw_order =
                if counts.part_keyforms > 0 && kf_off < counts.part_keyforms as usize {
                    read_f32(
                        self.bytes,
                        offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize + kf_off * 4,
                    )?
                } else {
                    0.0
                };

            check_status!(
                self.doc
                    .create_part(Part {
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
                            self.bytes,
                            offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        base_draw_order
                    };
                    keyforms.push(PartKeyform {
                        keys: Self::combo_keys(&axes, k),
                        draw_order: d_order,
                    });
                }
                part_scene_bindings.push(SceneBinding {
                    id: stable_id(&self.doc_id, "part_binding", p, &id),
                    axes,
                    track: SceneTrack::Part {
                        target_id: id.into(),
                        keyforms,
                    },
                });
            }
        }

        Ok(part_scene_bindings)
    }
}
