use kasane_core::types::Status;

use crate::layout::checked;

use super::context::Moc3EncoderContext;
use super::helpers::{index_of, write_bs_colors};

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_initial_offscreens(&mut self) -> Result<(), Status> {
        if self.export_version >= 6 {
            for os_id in self.doc.offscreen_order() {
                let os = self.doc.get_offscreen(os_id).unwrap();
                let binding = self.doc.binding_for_scene(&os.part_id);
                let count = binding.map_or(1, |b| b.track.len());
                let stored = binding.map_or(1, |b| count.max(1usize << b.axes.len()));
                self.os_key_bases.insert(
                    os.id.as_str(),
                    checked(self.l.counts[36] as usize, "offscreen_keyforms")?,
                );
                for slot in 0..stored {
                    let key = os
                        .keyform_index(slot.min(count - 1), binding.map(|b| b.track.len()))?
                        .map(|index| &os.keyforms[index]);
                    self.l
                        .scalar("offscreen_key_src.opacity", key.map_or(1.0, |k| k.opacity))?;
                    // Ordinary colors are absolute values, unlike blendshape deltas.
                    // Both color pools must have a contiguous entry for every slot.
                    write_bs_colors(
                        &mut self.l,
                        "offscreen_key_src",
                        Some(key.and_then(|k| k.multiply).unwrap_or([1.0; 3])),
                        Some(key.and_then(|k| k.screen).unwrap_or([0.0; 3])),
                    )?;
                    self.l.counts[36] += 1;
                }
            }
        }
        Ok(())
    }

    pub(super) fn encode_offscreen_sources(&mut self) -> Result<(), Status> {
        if self.export_version >= 6 {
            for os_id in self.doc.offscreen_order() {
                let os = self.doc.get_offscreen(os_id).unwrap();
                let owner_idx = self.parts.iter().position(|p| p == &os.part_id).unwrap() as i32;
                self.l.integer("offscreen_src.owner_idx", owner_idx)?;
                self.l.field("offscreen_src.drawable_flag")?.push(os.flags);
                self.l
                    .integer("offscreen_src.blend_mode", os.blend_mode as i32)?;
                let mask_off = checked(self.l.field("mask_src.art_mesh_idx")?.len() / 4, &os.id)?;
                self.l.integer("offscreen_src.mask_off", mask_off)?;
                self.l
                    .integer("offscreen_src.mask_len", checked(os.masks.len(), &os.id)?)?;
                for mask in &os.masks {
                    self.l
                        .integer("mask_src.art_mesh_idx", index_of(&self.mesh_indices, mask))?;
                }
            }
        }
        Ok(())
    }
}
