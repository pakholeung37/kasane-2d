use std::collections::HashMap;

use kasane_core::types::{BlendShapeBinding, BlendShapeTargetKind, DeltaKeyforms, Status, Vec2};

use crate::layout::{checked, Layout};

use super::context::Moc3EncoderContext;
use super::helpers::{index_of, write_bs_colors, write_delta_positions};

fn write_blend_binding(
    l: &mut Layout,
    b: &BlendShapeBinding,
    key_bs_off: i32,
    key_bs_len: i32,
    bkt_indices: &HashMap<&str, i32>,
    constraint_index_map: &HashMap<&str, i32>,
) -> Result<(), Status> {
    let kt_idx = bkt_indices
        .get(b.key_table_id.as_str())
        .copied()
        .ok_or_else(|| {
            Status::error("MISSING_KEY_TABLE", format!("{}: {}", b.id, b.key_table_id))
        })?;
    l.integer("blend_binding_src.key_table_idx", kt_idx)?;
    l.integer("blend_binding_src.key_bs_off", key_bs_off)?;
    l.integer("blend_binding_src.key_bs_len", key_bs_len)?;

    let c_off = checked(
        l.field("blend_constraint_idx_src.constraint_idx")?.len() / 4,
        "c_idx_off",
    )?;
    l.integer("blend_binding_src.bs_constraint_idx_off", c_off)?;
    l.integer(
        "blend_binding_src.bs_constraint_idx_len",
        checked(b.constraint_ids.len(), "c_idx_len")?,
    )?;
    for cid in &b.constraint_ids {
        let ci = constraint_index_map
            .get(cid.as_str())
            .copied()
            .ok_or_else(|| Status::error("MISSING_CONSTRAINT", format!("{}: {cid}", b.id)))?;
        l.integer("blend_constraint_idx_src.constraint_idx", ci)?;
    }
    Ok(())
}

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_constraints(&mut self) -> Result<(), Status> {
        for (c_idx, c_id) in self.doc.blend_constraint_order().iter().enumerate() {
            let c = self.doc.get_blend_constraint(c_id).unwrap();
            let p_idx = index_of(&self.parameter_indices, &c.parameter_id);
            let val_off = checked(
                self.l.field("blend_constraint_val_src.key")?.len() / 4,
                "constraint_vals",
            )?;
            let val_len = checked(c.keys.len(), "constraint_val_len")?;
            self.l
                .integer("blend_constraint_src.parameter_idx", p_idx)?;
            self.l.integer("blend_constraint_src.value_off", val_off)?;
            self.l.integer("blend_constraint_src.value_len", val_len)?;
            for (&k, &w) in c.keys.iter().zip(c.weights.iter()) {
                self.l.scalar("blend_constraint_val_src.key", k)?;
                self.l.scalar("blend_constraint_val_src.weight", w)?;
            }
            self.constraint_index_map
                .insert(c.id.as_str(), c_idx as i32);
        }
        self.l.counts[30] =
            checked(self.doc.blend_constraint_order().len(), "bs_constraints")? as u32;
        self.l.counts[31] = checked(
            self.l.field("blend_constraint_val_src.key")?.len() / 4,
            "constraint_vals",
        )? as u32;

        Ok(())
    }

    pub(super) fn encode_blendshapes(&mut self) -> Result<(), Status> {
        let mut warp_targets: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut mesh_targets: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut part_targets: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut rot_targets: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut glue_targets: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut offscreen_targets: HashMap<&str, Vec<&str>> = HashMap::new();

        for bid in self.doc.blend_binding_order() {
            let b = self.doc.get_blend_binding(bid).unwrap();
            match b.target_kind {
                BlendShapeTargetKind::Warp => {
                    warp_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
                BlendShapeTargetKind::Mesh => {
                    mesh_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
                BlendShapeTargetKind::Part => {
                    part_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
                BlendShapeTargetKind::Rotation => {
                    rot_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
                BlendShapeTargetKind::Glue => {
                    glue_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
                BlendShapeTargetKind::Offscreen => {
                    offscreen_targets
                        .entry(b.target_id.as_str())
                        .or_default()
                        .push(bid);
                }
            }
        }

        let mut sorted_warp_targets: Vec<&str> = warp_targets.keys().copied().collect();
        sorted_warp_targets.sort_by_key(|id| {
            self.warp_local_indices
                .get(*id)
                .copied()
                .unwrap_or(usize::MAX)
        });

        let mut sorted_mesh_targets: Vec<&str> = mesh_targets.keys().copied().collect();
        sorted_mesh_targets
            .sort_by_key(|id| self.mesh_indices.get(*id).copied().unwrap_or(usize::MAX));

        let mut sorted_part_targets: Vec<&str> = part_targets.keys().copied().collect();
        sorted_part_targets.sort_by_key(|id| index_of(&self.part_indices, id));

        let mut sorted_rot_targets: Vec<&str> = rot_targets.keys().copied().collect();
        sorted_rot_targets.sort_by_key(|id| {
            self.rotation_local_indices
                .get(*id)
                .copied()
                .unwrap_or(usize::MAX)
        });

        let mut sorted_glue_targets: Vec<&str> = glue_targets.keys().copied().collect();
        sorted_glue_targets.sort_by_key(|id| index_of(&self.glue_indices, id));

        // 1. Warp BlendShapes
        for target_id in sorted_warp_targets {
            let target_local = self.warp_local_indices[target_id];
            let binding_ids = &warp_targets[target_id];
            let warp = self.doc.get_transform(target_id).unwrap();
            let bs_b_off = self.l.counts[26] as i32;
            let bs_b_len = checked(binding_ids.len(), "bs_warp_b_len")?;
            self.l
                .integer("bs_warp_src.target_idx", target_local as i32)?;
            self.l.integer("bs_warp_src.bs_binding_off", bs_b_off)?;
            self.l.integer("bs_warp_src.bs_binding_len", bs_b_len)?;
            self.l.counts[27] += 1;

            for &bid in binding_ids {
                let b = self.doc.get_blend_binding(bid).unwrap();
                if let DeltaKeyforms::Warp(ref forms) = b.keyforms {
                    let key_bs_off = self.l.counts[7] as i32;
                    let key_bs_len = forms.len() as i32;
                    write_blend_binding(
                        &mut self.l,
                        b,
                        key_bs_off,
                        key_bs_len,
                        &self.bkt_indices,
                        &self.constraint_index_map,
                    )?;
                    self.l.counts[26] += 1;
                    let pt_count = ((warp.warp().unwrap().rows + 1)
                        * (warp.warp().unwrap().columns + 1))
                        as usize;
                    for f in forms {
                        self.l
                            .scalar("warp_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                        write_delta_positions(
                            &mut self.l,
                            self.doc,
                            "warp_key_src",
                            warp.parent(),
                            &f.points,
                            pt_count,
                        )?;
                        write_bs_colors(&mut self.l, "warp_key_src", f.multiply, f.screen)?;
                        self.l.counts[7] += 1;
                    }
                }
            }
        }

        // 2. Mesh BlendShapes
        for target_id in sorted_mesh_targets {
            let target_idx = index_of(&self.mesh_indices, target_id);
            let binding_ids = &mesh_targets[target_id];
            let mesh = self.doc.get_mesh(target_id).unwrap();
            let bs_b_off = self.l.counts[26] as i32;
            let bs_b_len = checked(binding_ids.len(), "bs_mesh_b_len")?;
            self.l.integer("bs_art_mesh_src.target_idx", target_idx)?;
            self.l.integer("bs_art_mesh_src.bs_binding_off", bs_b_off)?;
            self.l.integer("bs_art_mesh_src.bs_binding_len", bs_b_len)?;
            self.l.counts[28] += 1;

            for &bid in binding_ids {
                let b = self.doc.get_blend_binding(bid).unwrap();
                if let DeltaKeyforms::Mesh(ref forms) = b.keyforms {
                    let key_bs_off = self.l.counts[9] as i32;
                    let key_bs_len = forms.len() as i32;
                    write_blend_binding(
                        &mut self.l,
                        b,
                        key_bs_off,
                        key_bs_len,
                        &self.bkt_indices,
                        &self.constraint_index_map,
                    )?;
                    self.l.counts[26] += 1;
                    let pt_count = mesh.base_positions.len();
                    for f in forms {
                        self.l
                            .scalar("art_mesh_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                        self.l
                            .scalar("art_mesh_key_src.draw_order", f.draw_order.unwrap_or(0.0))?;
                        write_delta_positions(
                            &mut self.l,
                            self.doc,
                            "art_mesh_key_src",
                            &mesh.deformer_id,
                            &f.positions,
                            pt_count,
                        )?;
                        write_bs_colors(&mut self.l, "art_mesh_key_src", f.multiply, f.screen)?;
                        self.l.counts[9] += 1;
                    }
                }
            }
        }

        // 3. Part BlendShapes
        for target_id in sorted_part_targets {
            let target_idx = index_of(&self.part_indices, target_id);
            let binding_ids = &part_targets[target_id];
            let bs_b_off = self.l.counts[26] as i32;
            let bs_b_len = checked(binding_ids.len(), "bs_part_b_len")?;
            self.l.integer("bs_part_src.target_idx", target_idx)?;
            self.l.integer("bs_part_src.bs_binding_off", bs_b_off)?;
            self.l.integer("bs_part_src.bs_binding_len", bs_b_len)?;
            self.l.counts[32] += 1;

            for &bid in binding_ids {
                let b = self.doc.get_blend_binding(bid).unwrap();
                if let DeltaKeyforms::Part(ref forms) = b.keyforms {
                    let key_bs_off = self.l.counts[6] as i32;
                    let key_bs_len = forms.len() as i32;
                    write_blend_binding(
                        &mut self.l,
                        b,
                        key_bs_off,
                        key_bs_len,
                        &self.bkt_indices,
                        &self.constraint_index_map,
                    )?;
                    self.l.counts[26] += 1;
                    for f in forms {
                        self.l.scalar("part_key_src.draw_order", f.draw_order)?;
                        if self.export_version >= 6 {
                            self.l.integer("part_key_src.key_idx", -1)?;
                        }
                        self.l.counts[6] += 1;
                    }
                }
            }
        }

        // 4. Rotation BlendShapes
        if self.export_version >= 5 {
            for target_id in sorted_rot_targets {
                let target_local = self.rotation_local_indices[target_id];
                let binding_ids = &rot_targets[target_id];
                let rot = self.doc.get_transform(target_id).unwrap();
                let bs_b_off = self.l.counts[26] as i32;
                let bs_b_len = checked(binding_ids.len(), "bs_rot_b_len")?;
                self.l
                    .integer("bs_rotation_src.target_idx", target_local as i32)?;
                self.l.integer("bs_rotation_src.bs_binding_off", bs_b_off)?;
                self.l.integer("bs_rotation_src.bs_binding_len", bs_b_len)?;
                self.l.counts[33] += 1;

                for &bid in binding_ids {
                    let b = self.doc.get_blend_binding(bid).unwrap();
                    if let DeltaKeyforms::Rotation(ref forms) = b.keyforms {
                        let key_bs_off = self.l.counts[8] as i32;
                        let key_bs_len = forms.len() as i32;
                        write_blend_binding(
                            &mut self.l,
                            b,
                            key_bs_off,
                            key_bs_len,
                            &self.bkt_indices,
                            &self.constraint_index_map,
                        )?;
                        self.l.counts[26] += 1;
                        for f in forms {
                            self.l
                                .scalar("rotation_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                            self.l
                                .scalar("rotation_key_src.angle", f.angle.unwrap_or(0.0))?;
                            let origin = if let Some(o) = f.origin {
                                if rot.parent_id.is_none() {
                                    let ppu = self.doc.canvas().pixels_per_unit;
                                    Vec2::new(o.x / ppu, -o.y / ppu)
                                } else {
                                    o
                                }
                            } else {
                                Vec2::default()
                            };
                            self.l.scalar("rotation_key_src.origin_x", origin.x)?;
                            self.l.scalar("rotation_key_src.origin_y", origin.y)?;
                            self.l
                                .scalar("rotation_key_src.scale", f.scale.unwrap_or(0.0))?;
                            self.l.integer("rotation_key_src.reflect_x", 0)?;
                            self.l.integer("rotation_key_src.reflect_y", 0)?;
                            write_bs_colors(&mut self.l, "rotation_key_src", f.multiply, f.screen)?;
                            self.l.counts[8] += 1;
                        }
                    }
                }
            }
        }

        // 5. Glue BlendShapes
        for target_id in sorted_glue_targets {
            let target_idx = index_of(&self.glue_indices, target_id);
            let binding_ids = &glue_targets[target_id];
            let bs_b_off = self.l.counts[26] as i32;
            let bs_b_len = checked(binding_ids.len(), "bs_glue_b_len")?;
            self.l.integer("bs_glue_src.target_idx", target_idx)?;
            self.l.integer("bs_glue_src.bs_binding_off", bs_b_off)?;
            self.l.integer("bs_glue_src.bs_binding_len", bs_b_len)?;
            self.l.counts[34] += 1;

            for &bid in binding_ids {
                let b = self.doc.get_blend_binding(bid).unwrap();
                if let DeltaKeyforms::Glue(ref forms) = b.keyforms {
                    let key_bs_off = self.l.counts[22] as i32;
                    let key_bs_len = forms.len() as i32;
                    write_blend_binding(
                        &mut self.l,
                        b,
                        key_bs_off,
                        key_bs_len,
                        &self.bkt_indices,
                        &self.constraint_index_map,
                    )?;
                    self.l.counts[26] += 1;
                    for f in forms {
                        self.l.scalar("glue_key_src.intensity", f.intensity)?;
                        self.l.counts[22] += 1;
                    }
                }
            }
        }

        // 6. Offscreen BlendShapes
        if self.export_version >= 6 {
            let mut sorted_os_targets: Vec<&str> = offscreen_targets.keys().copied().collect();
            sorted_os_targets.sort_by_key(|id| index_of(&self.offscreen_indices, id));

            for target_id in sorted_os_targets {
                let target_idx = index_of(&self.offscreen_indices, target_id);
                let binding_ids = &offscreen_targets[target_id];
                let bs_b_off = self.l.counts[26] as i32;
                let bs_b_len = checked(binding_ids.len(), "bs_offscreen_b_len")?;
                self.l.integer("bs_offscreen_src.target_idx", target_idx)?;
                self.l
                    .integer("bs_offscreen_src.bs_binding_off", bs_b_off)?;
                self.l
                    .integer("bs_offscreen_src.bs_binding_len", bs_b_len)?;
                self.l.counts[37] += 1;

                for &bid in binding_ids {
                    let b = self.doc.get_blend_binding(bid).unwrap();
                    if let DeltaKeyforms::Offscreen(ref forms) = b.keyforms {
                        let key_bs_off = self.l.counts[36] as i32;
                        let key_bs_len = forms.len() as i32;
                        write_blend_binding(
                            &mut self.l,
                            b,
                            key_bs_off,
                            key_bs_len,
                            &self.bkt_indices,
                            &self.constraint_index_map,
                        )?;
                        self.l.counts[26] += 1;
                        for f in forms {
                            self.l.scalar("offscreen_key_src.opacity", f.opacity)?;
                            write_bs_colors(
                                &mut self.l,
                                "offscreen_key_src",
                                f.multiply,
                                f.screen,
                            )?;
                            self.l.counts[36] += 1;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
