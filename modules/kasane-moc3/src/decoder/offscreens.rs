use kasane_core::types::{Offscreen, OffscreenKeyform, Status};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{read_f32, read_i32, read_u32, stable_id};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_offscreens(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;

        if ver < 6 || counts.offscreens == 0 {
            return Ok(());
        }

        for i in 0..counts.offscreens as usize {
            let owner_part_idx = read_i32(
                self.bytes,
                offsets[section::OFFSCREEN_SRC_OWNER_IDX] as usize + i * 4,
            )? as usize;
            if owner_part_idx >= self.mapping.part_by_index.len() {
                return Err(Status::error(
                    "INVALID_OFFSCREEN",
                    format!("Offscreen {i}: invalid owner part index {owner_part_idx}"),
                ));
            }
            let part_id = self.mapping.part_by_index[owner_part_idx].clone();
            let part_runtime_id = self
                .mapping
                .parts
                .iter()
                .find(|(_, id)| *id == &part_id)
                .map(|(r, _)| r.clone())
                .unwrap_or_else(|| format!("Part{owner_part_idx}"));
            let part_name = self
                .doc
                .get_part(&part_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Part{owner_part_idx}"));

            let flags = self.bytes[offsets[section::OFFSCREEN_SRC_DRAWABLE_FLAG] as usize + i];
            let blend_mode = read_u32(
                self.bytes,
                offsets[section::OFFSCREEN_SRC_BLEND_MODE] as usize + i * 4,
            )?;
            let mask_off = read_i32(
                self.bytes,
                offsets[section::OFFSCREEN_SRC_MASK_OFF] as usize + i * 4,
            )? as usize;
            let mask_len = read_i32(
                self.bytes,
                offsets[section::OFFSCREEN_SRC_MASK_LEN] as usize + i * 4,
            )? as usize;
            let mut masks = Vec::with_capacity(mask_len);
            for m in 0..mask_len {
                let mesh_idx = read_i32(
                    self.bytes,
                    offsets[section::MASK_SRC_ART_MESH_IDX] as usize + (mask_off + m) * 4,
                )? as usize;
                if mesh_idx >= self.mapping.mesh_by_index.len() {
                    return Err(Status::error(
                        "INVALID_OFFSCREEN_MASK",
                        format!("Offscreen {i}: invalid mask mesh index {mesh_idx}"),
                    ));
                }
                masks.push(self.mapping.mesh_by_index[mesh_idx].clone());
            }

            let part_keyform_off = read_i32(
                self.bytes,
                offsets[section::PART_SRC_KEYFORM_OFF] as usize + owner_part_idx * 4,
            )? as usize;
            let part_key_len = read_i32(
                self.bytes,
                offsets[section::PART_SRC_KEY_LEN] as usize + owner_part_idx * 4,
            )? as usize;
            let mut keyforms = Vec::new();
            let mut part_keyform_indices = Vec::with_capacity(part_key_len);

            for k in 0..part_key_len {
                let global_kf_idx = read_i32(
                    self.bytes,
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
                        self.bytes,
                        offsets[section::OFFSCREEN_KEY_SRC_OPACITY] as usize + g * 4,
                    )?;
                    let mul_idx = read_i32(
                        self.bytes,
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
                                self.bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_R] as usize
                                    + mul_idx as usize * 4,
                            )?,
                            read_f32(
                                self.bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_G] as usize
                                    + mul_idx as usize * 4,
                            )?,
                            read_f32(
                                self.bytes,
                                offsets[section::KEYFORM_MUL_COLOR_SRC_B] as usize
                                    + mul_idx as usize * 4,
                            )?,
                        ])
                    } else {
                        None
                    };
                    let scr_idx = read_i32(
                        self.bytes,
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
                                self.bytes,
                                offsets[section::KEYFORM_SCR_COLOR_SRC_R] as usize
                                    + scr_idx as usize * 4,
                            )?,
                            read_f32(
                                self.bytes,
                                offsets[section::KEYFORM_SCR_COLOR_SRC_G] as usize
                                    + scr_idx as usize * 4,
                            )?,
                            read_f32(
                                self.bytes,
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
            let os_id = stable_id(&self.doc_id, "offscreen", i, &runtime_id);
            self.mapping.offscreen_by_index.push(os_id.clone());
            check_status!(
                self.doc
                    .create_offscreen(Offscreen {
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

        Ok(())
    }
}
