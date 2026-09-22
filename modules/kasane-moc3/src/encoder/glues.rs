use kasane_core::types::Status;

use crate::layout::checked;
use crate::schema::section;

use super::context::Moc3EncoderContext;
use super::helpers::{find_vertex_pos, index_of, write_id};

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_glues(&mut self) -> Result<(), Status> {
        self.l.counts[20] = checked(self.doc.glue_order().len(), "glues")? as u32;
        let mut glue_info_count = 0;
        for g_id in self.doc.glue_order() {
            let g = self.doc.get_glue(g_id).unwrap();
            let mesh_a = self.doc.get_mesh(&g.mesh_a_id).unwrap();
            let mesh_b = self.doc.get_mesh(&g.mesh_b_id).unwrap();
            let ma_idx = index_of(&self.mesh_indices, &g.mesh_a_id);
            let mb_idx = index_of(&self.mesh_indices, &g.mesh_b_id);
            let b_idx = if g.binding.is_some() {
                self.binding_indices[g_id.as_str()]
            } else {
                0
            };
            let key_off = checked(
                self.l.field("glue_key_src.intensity")?.len() / 4,
                "glue key offset",
            )?;
            let key_len = g.binding.as_ref().map_or(1, |b| b.keyforms.len());
            write_id(
                &mut self.l,
                section::GLUE_SRC_ID,
                "glue_src.id",
                &g.runtime_id,
            )?;
            self.l.integer("glue_src.binding_idx", b_idx)?;
            self.l.integer("glue_src.keyform_off", key_off)?;
            self.l
                .integer("glue_src.key_len", checked(key_len, "glue key count")?)?;
            self.l.integer("glue_src.art_mesh_idx_a", ma_idx)?;
            self.l.integer("glue_src.art_mesh_idx_b", mb_idx)?;
            let info_off = glue_info_count;
            let info_len = checked(g.pairs.len() * 2, "glue_info_len")?;
            self.l.integer("glue_src.info_off", info_off)?;
            self.l.integer("glue_src.info_len", info_len)?;
            if let Some(binding) = &g.binding {
                for key in &binding.keyforms {
                    self.l.scalar("glue_key_src.intensity", key.intensity)?;
                }
            } else {
                self.l.scalar("glue_key_src.intensity", g.intensity)?;
            }
            for pair in &g.pairs {
                let pos_a = find_vertex_pos(mesh_a, pair.vertex_a)?;
                let pos_b = find_vertex_pos(mesh_b, pair.vertex_b)?;
                self.l.scalar("glue_info_src.weight", pair.weight_a)?;
                self.l.short("glue_info_src.pos_idx", pos_a)?;
                self.l.scalar("glue_info_src.weight", pair.weight_b)?;
                self.l.short("glue_info_src.pos_idx", pos_b)?;
            }
            glue_info_count += info_len;
        }
        self.l.counts[21] = glue_info_count as u32;
        self.l.counts[22] = checked(
            self.l.field("glue_key_src.intensity")?.len() / 4,
            "glue keyforms",
        )? as u32;

        Ok(())
    }
}
