use kasane_core::types::{ParameterKind, Status};

use crate::layout::checked;

use super::context::Moc3EncoderContext;

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_parameters(&mut self) -> Result<(), Status> {
        let mut table_count: i32 = 0;
        for id in self.doc.parameter_order() {
            let p = self.doc.get_parameter(id).unwrap();
            {
                let ids = self.l.field("param_src.id")?;
                let start = ids.len();
                ids.extend_from_slice(p.runtime_id.as_bytes());
                ids.resize(start + 64, 0);
            }
            self.l.scalar("param_src.maximum_value", p.maximum)?;
            self.l.scalar("param_src.minimum_value", p.minimum)?;
            self.l.scalar("param_src.default_value", p.default_value)?;
            self.l
                .integer("param_src.repeat", if p.repeat { 1 } else { 0 })?;
            self.l
                .integer("param_src.decimal_places", p.decimal_places)?;

            let is_bs_param = p.kind == ParameterKind::BlendShape;
            self.l
                .integer("param_src.type", if is_bs_param { 1 } else { 0 })?;

            let bkts = self.param_bkts.get(id.as_str());
            if let Some(bkts) = bkts {
                let first_bkt = self.bkt_indices[bkts[0]];
                self.l.integer("param_src.blend_key_table_off", first_bkt)?;
                self.l
                    .integer("param_src.blend_key_table_len", bkts.len() as i32)?;
            } else {
                self.l.integer("param_src.blend_key_table_off", 0)?;
                self.l.integer("param_src.blend_key_table_len", 0)?;
            }

            let first = table_count;
            let mut union_keys = Vec::new();
            for binding in &self.all_bindings {
                let bid = binding.id;
                for a in 0..binding.axes.len() {
                    let axis = &binding.axes[a];
                    if axis.parameter_id != *id {
                        continue;
                    }
                    self.table_indices.get_mut(bid).unwrap()[a] = table_count;
                    table_count += 1;
                    let keys_off = checked(
                        self.l.field("keys_src.key")?.len() / 4,
                        &format!("{bid}.keys_off"),
                    )?;
                    self.l.integer("key_table_src.keys_off", keys_off)?;
                    self.l.integer(
                        "key_table_src.keys_len",
                        checked(axis.keys.len(), &format!("{bid}.keys_len"))?,
                    )?;
                    for &k in &axis.keys {
                        self.l.scalar("keys_src.key", k)?;
                    }
                    union_keys.extend_from_slice(&axis.keys);
                }
            }

            if let Some(bkts) = bkts {
                for &bkt_id in bkts {
                    let bkt = self.doc.get_blend_key_table(bkt_id).unwrap();
                    union_keys.extend_from_slice(&bkt.keys);
                }
            }

            self.l.integer("param_src.key_table_off", first)?;
            self.l
                .integer("param_src.key_table_len", table_count - first)?;

            union_keys.sort_by(|a, b| a.total_cmp(b));
            union_keys.dedup();

            let union_keys_off = checked(
                self.l.field("keys_src.key")?.len() / 4,
                &format!("{id}.keys_off"),
            )?;
            self.l.integer("param_keys_src.keys_off", union_keys_off)?;
            self.l.integer(
                "param_keys_src.keys_len",
                checked(union_keys.len(), &format!("{id}.keys_len"))?,
            )?;
            for &k in &union_keys {
                self.l.scalar("keys_src.key", k)?;
            }
        }

        for &bkt_id in &self.ordered_bkts {
            let bkt = self.doc.get_blend_key_table(bkt_id).unwrap();
            let keys_off = checked(self.l.field("keys_src.key")?.len() / 4, "bkt_keys_off")?;
            self.l.integer("blend_key_table_src.keys_off", keys_off)?;
            self.l.integer(
                "blend_key_table_src.keys_len",
                checked(bkt.keys.len(), "bkt_keys_len")?,
            )?;
            self.l
                .integer("blend_key_table_src.base_key_idx", bkt.base_key_idx as i32)?;
            for &k in &bkt.keys {
                self.l.scalar("keys_src.key", k)?;
            }
        }
        self.l.counts[25] = self.ordered_bkts.len() as u32;

        self.l.counts[13] = table_count as u32;
        self.l.counts[14] = checked(self.l.field("keys_src.key")?.len() / 4, "keys")? as u32;

        for b in &self.all_bindings {
            let bid = b.id;
            let b_idx = checked(self.binding_indices.len() + 1, "binding_index")?;
            self.binding_indices.insert(bid, b_idx);
            let axes_offset = checked(
                self.l.field("key_table_idx_src.idx")?.len() / 4,
                &format!("{bid}.axes_offset"),
            )?;
            self.l
                .integer("binding_src.key_table_idx_off", axes_offset)?;
            let t_indices = &self.table_indices[bid];
            self.l.integer(
                "binding_src.key_table_idx_len",
                checked(t_indices.len(), &format!("{bid}.axes_count"))?,
            )?;
            for &idx in t_indices {
                self.l.integer("key_table_idx_src.idx", idx)?;
            }
        }
        self.l.counts[11] = checked(
            self.l.field("key_table_idx_src.idx")?.len() / 4,
            "key_table_idx",
        )? as u32;

        Ok(())
    }
}
