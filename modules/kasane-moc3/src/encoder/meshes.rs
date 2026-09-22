use kasane_core::types::{BlendMode, Status};

use crate::layout::checked;

use super::context::Moc3EncoderContext;
use super::helpers::{index_of, write_colors, write_positions};

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_art_meshes(&mut self) -> Result<(), Status> {
        let mut keyform_offset = self.keyform_offset;

        for (i, d) in self.drawables.iter().enumerate() {
            {
                let ids = self.l.field("art_mesh_src.id")?;
                ids.extend_from_slice(d.runtime_id.as_bytes());
                ids.resize((i + 1) * 64, 0);
            }
            let mesh = self.doc.get_mesh(&d.id).unwrap();
            let binding = self.doc.binding_for_mesh(&d.id);
            let key_count = if let Some(b) = binding {
                checked(b.keyforms.len(), &format!("{}.keyforms", b.id))?
            } else {
                1
            };
            let stored_count = if let Some(b) = binding {
                key_count.max(1i32 << b.axes.len())
            } else {
                1
            };

            self.l.integer(
                "art_mesh_src.binding_idx",
                if let Some(b) = binding {
                    self.binding_indices[b.id.as_str()]
                } else {
                    0
                },
            )?;
            self.l.integer("art_mesh_src.keyform_off", keyform_offset)?;
            self.l.integer("art_mesh_src.key_len", key_count)?;
            let color_off = checked(self.l.field("keyform_mul_color_src.r")?.len() / 4, &d.id)?;
            self.l.integer("art_mesh_src.key_color_off", color_off)?;
            self.l.integer("art_mesh_src.visible", 1)?;
            self.l
                .integer("art_mesh_src.enable", if mesh.enabled { 1 } else { 0 })?;
            self.l.integer(
                "art_mesh_src.parent_part_idx",
                index_of(&self.part_indices, &mesh.part_id),
            )?;
            self.l.integer(
                "art_mesh_src.parent_deformer_idx",
                index_of(&self.transform_indices, &mesh.deformer_id),
            )?;
            self.l.integer("art_mesh_src.texture_no", d.texture_slot)?;

            let flag: u8 = (if mesh.double_sided { 4 } else { 0 })
                | (if mesh.inverted_mask { 8 } else { 0 })
                | (match mesh.blend_mode {
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                    BlendMode::Normal => 0,
                });
            self.l.field("art_mesh_src.drawable_flag")?.push(flag);

            self.l.integer(
                "art_mesh_src.vertex_count",
                checked(d.positions.len(), &format!("{}.vertex_count", d.id))?,
            )?;
            let uv_off = checked(
                self.l.field("uv_src.xy")?.len() / 4,
                &format!("{}.uv_off", d.id),
            )?;
            self.l.integer("art_mesh_src.uv_off", uv_off)?;
            let idx_off = checked(
                self.l.field("idx_src.idx")?.len() / 2,
                &format!("{}.idx_off", d.id),
            )?;
            self.l.integer("art_mesh_src.idx_off", idx_off)?;
            self.l.integer(
                "art_mesh_src.idx_len",
                checked(d.indices.len(), &format!("{}.idx_len", d.id))?,
            )?;
            let mask_off = checked(self.l.field("mask_src.art_mesh_idx")?.len() / 4, &d.id)?;
            self.l.integer("art_mesh_src.mask_off", mask_off)?;
            self.l
                .integer("art_mesh_src.mask_len", checked(mesh.masks.len(), &d.id)?)?;

            for mask in &mesh.masks {
                self.l
                    .integer("mask_src.art_mesh_idx", index_of(&self.mesh_indices, mask))?;
            }

            if self.export_version >= 6 {
                let raw_bm = mesh.raw_blend_mode.unwrap_or(0);
                self.l.integer("art_mesh_src.blend_mode", raw_bm as i32)?;
            }

            for k in 0..stored_count {
                let positions = if let Some(b) = binding {
                    &b.keyforms[k.min(key_count - 1) as usize].positions
                } else {
                    &mesh.base_positions
                };
                let appearance = if let Some(b) = binding {
                    &b.keyforms[k.min(key_count - 1) as usize].appearance
                } else {
                    &mesh.appearance
                };
                let base_order = mesh.draw_order.unwrap_or(i as f32);
                self.l
                    .scalar("art_mesh_key_src.opacity", appearance.opacity)?;
                let order = if let Some(b) = binding {
                    b.keyforms[k.min(key_count - 1) as usize]
                        .draw_order
                        .unwrap_or(base_order)
                } else {
                    base_order
                };
                self.l.scalar("art_mesh_key_src.draw_order", order)?;
                write_positions(
                    &mut self.l,
                    self.doc,
                    "art_mesh_key_src",
                    &mesh.deformer_id,
                    positions,
                )?;
                write_colors(&mut self.l, "art_mesh_key_src", appearance)?;
                keyform_offset =
                    checked(keyform_offset as usize + 1, &format!("{}.keyforms", d.id))?;
            }

            for p in d.uvs.iter() {
                self.l.scalar("uv_src.xy", p.x)?;
                self.l.scalar(
                    "uv_src.xy",
                    if self.doc.canvas().flag & 1 == 0 {
                        1.0 - p.y
                    } else {
                        p.y
                    },
                )?;
            }

            let idx_field = self.l.field("idx_src.idx")?;
            // evaluate_frame exposes revived runtime winding. Undo the canvas
            // reversal here because Core applies it while reviving the file.
            for triangle in d.indices.as_chunks::<3>().0 {
                let stored = if self.doc.canvas().flag & 1 == 0 {
                    [triangle[2], triangle[1], triangle[0]]
                } else {
                    [triangle[0], triangle[1], triangle[2]]
                };
                for v in stored {
                    idx_field.extend_from_slice(&(v as u16).to_le_bytes());
                }
            }
        }

        self.l.counts[9] = keyform_offset as u32;
        self.keyform_offset = keyform_offset;

        Ok(())
    }
}
