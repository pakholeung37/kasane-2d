use kasane_core::types::Status;

use crate::layout::checked;
use crate::schema::section;

use super::context::Moc3EncoderContext;
use super::helpers::{index_of, write_id};

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_parts(&mut self) -> Result<(), Status> {
        for id in &self.parts {
            let part = self.doc.get_part(id).unwrap();
            let b = self.doc.binding_for_scene(id);
            let count = if let Some(b) = b {
                checked(b.track.len(), id)?
            } else {
                1
            };
            let stored = if let Some(b) = b {
                count.max(1i32 << b.axes.len())
            } else {
                1
            };

            write_id(
                &mut self.l,
                section::PART_SRC_ID,
                "part_src.id",
                &part.runtime_id,
            )?;
            self.l.integer(
                "part_src.binding_idx",
                if let Some(b) = b {
                    self.binding_indices[b.id.as_str()]
                } else {
                    0
                },
            )?;
            self.l
                .integer("part_src.keyform_off", self.l.counts[6] as i32)?;
            self.l.integer("part_src.key_len", count)?;
            self.l.integer("part_src.visible", 1)?;
            self.l
                .integer("part_src.enable", if part.enabled { 1 } else { 0 })?;
            self.l.integer(
                "part_src.parent_part_idx",
                index_of(&self.part_indices, &part.parent_id),
            )?;

            if self.export_version >= 6 {
                let os_idx = if let Some(os) = self.doc.offscreen_for_part(&part.id) {
                    self.doc
                        .offscreen_order()
                        .iter()
                        .position(|id| id == &os.id)
                        .map(|i| i as i32)
                        .unwrap_or(-1)
                } else {
                    -1
                };
                self.l.integer("part_src.offscreen_idx", os_idx)?;
            }

            for k in 0..stored {
                let draw_order = if let Some(b) = b {
                    b.track.sample(k.min(count - 1) as usize).draw_order
                } else {
                    part.draw_order
                };
                self.l.scalar("part_key_src.draw_order", draw_order)?;

                if self.export_version >= 6 {
                    let key_idx = if let Some(os) = self.doc.offscreen_for_part(&part.id) {
                        self.os_key_bases[os.id.as_str()] + k
                    } else {
                        -1
                    };
                    self.l.integer("part_key_src.key_idx", key_idx)?;
                }
            }
            self.l.counts[6] += stored as u32;
        }

        Ok(())
    }
}
