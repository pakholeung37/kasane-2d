use std::collections::HashMap;

use kasane_core::types::Status;

use crate::layout::checked;

use super::context::Moc3EncoderContext;
use super::helpers::index_of;

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_drawing_groups(&mut self) -> Result<(), Status> {
        let groups = kasane_core::draw_order::resolved_groups(self.doc);
        let totals = if self.export_version >= 6 {
            kasane_core::draw_order::descendant_counts_with_offscreens(self.doc, &groups)
        } else {
            kasane_core::draw_order::descendant_counts(&groups)
        };
        let group_slots: HashMap<&str, usize> = groups
            .iter()
            .enumerate()
            .map(|(i, g)| (g.owner.as_str(), i))
            .collect();
        for group in &groups {
            let first = self.l.field("draw_group_obj_src.idx")?.len() / 4;
            for id in &group.items {
                let is_part = self.doc.get_part(id).is_some();
                self.l
                    .integer("draw_group_obj_src.type", if is_part { 1 } else { 0 })?;
                self.l.integer(
                    "draw_group_obj_src.idx",
                    if is_part {
                        index_of(&self.part_indices, id)
                    } else {
                        index_of(&self.mesh_indices, id)
                    },
                )?;
                self.l.integer(
                    "draw_group_obj_src.self_group_idx",
                    if is_part {
                        checked(group_slots[id.as_str()], "draw_group")?
                    } else {
                        -1
                    },
                )?;
            }
            self.l
                .integer("draw_group_src.obj_off", checked(first, "draw_group")?)?;
            self.l.integer(
                "draw_group_src.obj_len",
                checked(group.items.len(), "draw_group")?,
            )?;
            self.l.integer(
                "draw_group_src.obj_total_count",
                checked(totals[group.owner.as_str()], "draw_group")?,
            )?;
            self.l
                .integer("draw_group_src.min_order", group.min_order)?;
            self.l
                .integer("draw_group_src.max_order", group.max_order)?;
        }
        self.l.counts[18] = checked(groups.len(), "draw_groups")? as u32;

        self.l.counts[19] = checked(
            self.l.field("draw_group_obj_src.idx")?.len() / 4,
            "draw_items",
        )? as u32;

        Ok(())
    }
}
