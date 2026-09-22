use std::collections::HashMap;

use kasane_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    DeltaGlueKeyform, DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform,
    DeltaRotationKeyform, DeltaWarpKeyform, Status, Vec2,
};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{checked_reference, checked_window, read_f32, read_i32, stable_id};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_blend_key_tables(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;

        if counts.blend_key_tables == 0 {
            return Ok(());
        }

        for i in 0..counts.blend_key_tables as usize {
            let mut owner_param: Option<usize> = None;
            for p in 0..counts.parameters as usize {
                let p_off = read_i32(
                    self.bytes,
                    offsets[section::PARAM_SRC_BLEND_KEY_TABLE_OFF] as usize + p * 4,
                )? as usize;
                let p_len = read_i32(
                    self.bytes,
                    offsets[section::PARAM_SRC_BLEND_KEY_TABLE_LEN] as usize + p * 4,
                )? as usize;
                if i >= p_off && i < p_off + p_len {
                    owner_param = Some(p);
                    break;
                }
            }
            let p = owner_param.ok_or_else(|| {
                Status::error(
                    "INVALID_KEY_TABLE",
                    format!("Blend key table {i} not referenced by any parameter"),
                )
            })?;
            let parameter_id = self.mapping.parameter_by_index[p].clone();
            let keys_off = read_i32(
                self.bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_KEYS_OFF] as usize + i * 4,
            )? as usize;
            let keys_len = read_i32(
                self.bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_KEYS_LEN] as usize + i * 4,
            )? as usize;
            let base_key_idx = read_i32(
                self.bytes,
                offsets[section::BLEND_KEY_TABLE_SRC_BASE_KEY_IDX] as usize + i * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(keys_len);
            for k in 0..keys_len {
                keys.push(read_f32(
                    self.bytes,
                    offsets[section::KEYS_SRC_KEY] as usize + (keys_off + k) * 4,
                )?);
            }
            let bkt_id = stable_id(&self.doc_id, "blend_key_table", i, &format!("bkt_{i}"));
            self.mapping.blend_key_table_by_index.push(bkt_id.clone());
            check_status!(
                self.doc
                    .create_blend_key_table(BlendShapeKeyTable {
                        id: bkt_id,
                        parameter_id,
                        keys,
                        base_key_idx,
                    })
                    .status
            );
        }

        Ok(())
    }

    pub(super) fn decode_blend_constraints(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;

        if counts.bs_constraints == 0 {
            return Ok(());
        }

        for i in 0..counts.bs_constraints as usize {
            let param_idx = read_i32(
                self.bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_PARAMETER_IDX] as usize + i * 4,
            )? as usize;
            if param_idx >= self.mapping.parameter_by_index.len() {
                return Err(Status::error(
                    "INVALID_CONSTRAINT",
                    format!("Constraint {i}: invalid parameter {param_idx}"),
                ));
            }
            let parameter_id = self.mapping.parameter_by_index[param_idx].clone();
            let val_off = read_i32(
                self.bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_VALUE_OFF] as usize + i * 4,
            )? as usize;
            let val_len = read_i32(
                self.bytes,
                offsets[section::BLEND_CONSTRAINT_SRC_VALUE_LEN] as usize + i * 4,
            )? as usize;
            let mut keys = Vec::with_capacity(val_len);
            let mut weights = Vec::with_capacity(val_len);
            for k in 0..val_len {
                keys.push(read_f32(
                    self.bytes,
                    offsets[section::BLEND_CONSTRAINT_VAL_SRC_KEY] as usize + (val_off + k) * 4,
                )?);
                weights.push(read_f32(
                    self.bytes,
                    offsets[section::BLEND_CONSTRAINT_VAL_SRC_WEIGHT] as usize + (val_off + k) * 4,
                )?);
            }
            let bsc_id = stable_id(&self.doc_id, "blend_constraint", i, &format!("bsc_{i}"));
            self.mapping.blend_constraint_by_index.push(bsc_id.clone());
            check_status!(
                self.doc
                    .create_blend_constraint(BlendShapeConstraint {
                        id: bsc_id,
                        parameter_id,
                        keys,
                        weights,
                    })
                    .status
            );
        }

        Ok(())
    }

    pub(super) fn decode_blend_bindings(
        &mut self,
        warp_by_local_idx: &[String],
        rotation_by_local_idx: &[String],
    ) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;
        let ppu = self.inspection.canvas.pixels_per_unit;

        if counts.blend_bindings == 0 {
            return Ok(());
        }

        let mut binding_targets: HashMap<usize, (String, BlendShapeTargetKind)> = HashMap::new();
        let mut target_groups = std::collections::HashSet::new();
        let mut register_group = |target_id: &str, kind, start, len| -> Result<(), Status> {
            if !target_groups.insert(target_id.to_owned()) {
                return Err(Status::error(
                    "UNSUPPORTED_FEATURE",
                    format!(
                        "{target_id}: multiple BlendShape target groups are not yet representable"
                    ),
                ));
            }
            checked_window(start, len, counts.blend_bindings as usize, "blend_binding")?;
            for binding in start..start + len {
                if binding_targets
                    .insert(binding, (target_id.to_owned(), kind))
                    .is_some()
                {
                    return Err(Status::error("UNSUPPORTED_FEATURE", format!("Blend binding {binding}: shared target windows are not yet representable")));
                }
            }
            Ok(())
        };

        for i in 0..counts.bs_warps as usize {
            let target_local = read_i32(
                self.bytes,
                offsets[section::BS_WARP_SRC_TARGET_IDX] as usize + i * 4,
            )? as usize;
            let target_id =
                checked_reference(warp_by_local_idx, target_local, "bs_warp.target")?.clone();
            let b_off = read_i32(
                self.bytes,
                offsets[section::BS_WARP_SRC_BS_BINDING_OFF] as usize + i * 4,
            )? as usize;
            let b_len = read_i32(
                self.bytes,
                offsets[section::BS_WARP_SRC_BS_BINDING_LEN] as usize + i * 4,
            )? as usize;
            register_group(&target_id, BlendShapeTargetKind::Warp, b_off, b_len)?;
        }

        if ver >= 5 {
            for i in 0..counts.bs_rotations as usize {
                let target_local = read_i32(
                    self.bytes,
                    offsets[section::BS_ROTATION_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(rotation_by_local_idx, target_local, "bs_rotation.target")?
                        .clone();
                let b_off = read_i32(
                    self.bytes,
                    offsets[section::BS_ROTATION_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    self.bytes,
                    offsets[section::BS_ROTATION_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Rotation, b_off, b_len)?;
            }

            for i in 0..counts.bs_parts as usize {
                let target_part = read_i32(
                    self.bytes,
                    offsets[section::BS_PART_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(&self.mapping.part_by_index, target_part, "bs_part.target")?
                        .clone();
                let b_off = read_i32(
                    self.bytes,
                    offsets[section::BS_PART_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    self.bytes,
                    offsets[section::BS_PART_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Part, b_off, b_len)?;
            }

            for i in 0..counts.bs_glues as usize {
                let target_glue = read_i32(
                    self.bytes,
                    offsets[section::BS_GLUE_SRC_TARGET_IDX] as usize + i * 4,
                )? as usize;
                let target_id =
                    checked_reference(&self.mapping.glue_by_index, target_glue, "bs_glue.target")?
                        .clone();
                let b_off = read_i32(
                    self.bytes,
                    offsets[section::BS_GLUE_SRC_BS_BINDING_OFF] as usize + i * 4,
                )? as usize;
                let b_len = read_i32(
                    self.bytes,
                    offsets[section::BS_GLUE_SRC_BS_BINDING_LEN] as usize + i * 4,
                )? as usize;
                register_group(&target_id, BlendShapeTargetKind::Glue, b_off, b_len)?;
            }

            if ver >= 6 {
                for i in 0..counts.bs_offscreens as usize {
                    let target_os = read_i32(
                        self.bytes,
                        offsets[section::BS_OFFSCREEN_SRC_TARGET_IDX] as usize + i * 4,
                    )? as usize;
                    let target_id = checked_reference(
                        &self.mapping.offscreen_by_index,
                        target_os,
                        "bs_offscreen.target",
                    )?
                    .clone();
                    let b_off = read_i32(
                        self.bytes,
                        offsets[section::BS_OFFSCREEN_SRC_BS_BINDING_OFF] as usize + i * 4,
                    )? as usize;
                    let b_len = read_i32(
                        self.bytes,
                        offsets[section::BS_OFFSCREEN_SRC_BS_BINDING_LEN] as usize + i * 4,
                    )? as usize;
                    register_group(&target_id, BlendShapeTargetKind::Offscreen, b_off, b_len)?;
                }
            }
        }

        for i in 0..counts.bs_art_meshes as usize {
            let target_mesh = read_i32(
                self.bytes,
                offsets[section::BS_ART_MESH_SRC_TARGET_IDX] as usize + i * 4,
            )? as usize;
            let target_id =
                checked_reference(&self.mapping.mesh_by_index, target_mesh, "bs_mesh.target")?
                    .clone();
            let b_off = read_i32(
                self.bytes,
                offsets[section::BS_ART_MESH_SRC_BS_BINDING_OFF] as usize + i * 4,
            )? as usize;
            let b_len = read_i32(
                self.bytes,
                offsets[section::BS_ART_MESH_SRC_BS_BINDING_LEN] as usize + i * 4,
            )? as usize;
            register_group(&target_id, BlendShapeTargetKind::Mesh, b_off, b_len)?;
        }

        for b in 0..counts.blend_bindings as usize {
            let (target_id, target_kind) = binding_targets
                .get(&b)
                .ok_or_else(|| {
                    Status::error("ORPHAN_BINDING", format!("Blend binding {b} has no target"))
                })?
                .clone();

            let kt_idx = read_i32(
                self.bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_TABLE_IDX] as usize + b * 4,
            )? as usize;
            let key_table_id = checked_reference(
                &self.mapping.blend_key_table_by_index,
                kt_idx,
                "blend_binding.key_table",
            )?
            .clone();
            let key_bs_off = read_i32(
                self.bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_BS_OFF] as usize + b * 4,
            )? as usize;
            let key_bs_len = read_i32(
                self.bytes,
                offsets[section::BLEND_BINDING_SRC_KEY_BS_LEN] as usize + b * 4,
            )? as usize;
            let c_off = read_i32(
                self.bytes,
                offsets[section::BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_OFF] as usize + b * 4,
            )? as usize;
            let c_len = read_i32(
                self.bytes,
                offsets[section::BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_LEN] as usize + b * 4,
            )? as usize;

            checked_window(
                c_off,
                c_len,
                counts.bs_constraint_idx as usize,
                "blend_binding.constraints",
            )?;
            let keyform_count = match target_kind {
                BlendShapeTargetKind::Part => counts.part_keyforms,
                BlendShapeTargetKind::Warp => counts.warp_keyforms,
                BlendShapeTargetKind::Rotation => counts.rotation_keyforms,
                BlendShapeTargetKind::Mesh => counts.art_mesh_keyforms,
                BlendShapeTargetKind::Glue => counts.glue_keyforms,
                BlendShapeTargetKind::Offscreen => counts.offscreen_keyforms,
            };
            checked_window(
                key_bs_off,
                key_bs_len,
                keyform_count as usize,
                "blend_binding.keyforms",
            )?;
            let mut constraint_ids = Vec::with_capacity(c_len);
            for c in 0..c_len {
                let c_idx = read_i32(
                    self.bytes,
                    offsets[section::BLEND_CONSTRAINT_IDX_SRC_CONSTRAINT_IDX] as usize
                        + (c_off + c) * 4,
                )? as usize;
                constraint_ids.push(
                    checked_reference(
                        &self.mapping.blend_constraint_by_index,
                        c_idx,
                        "blend_binding.constraint",
                    )?
                    .clone(),
                );
            }

            let keyforms = match target_kind {
                BlendShapeTargetKind::Part => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let draw_order = read_f32(
                            self.bytes,
                            offsets[section::PART_KEY_SRC_DRAW_ORDER] as usize
                                + (key_bs_off + k) * 4,
                        )?;
                        forms.push(DeltaPartKeyform { draw_order });
                    }
                    DeltaKeyforms::Part(forms)
                }
                BlendShapeTargetKind::Warp => {
                    let warp = self.doc.get_transform(&target_id).unwrap();
                    let pt_count = ((warp.warp().unwrap().rows + 1)
                        * (warp.warp().unwrap().columns + 1))
                        as usize;
                    let is_root = warp.parent_id.is_none();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            self.bytes,
                            offsets[section::WARP_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let pos_off = read_i32(
                            self.bytes,
                            offsets[section::WARP_KEY_SRC_KEY_POS_OFF] as usize + ki * 4,
                        )? as usize;
                        let mut points = Vec::with_capacity(pt_count);
                        for p in 0..pt_count {
                            let rx = read_f32(
                                self.bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + p * 2) * 4,
                            )?;
                            let ry = read_f32(
                                self.bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize
                                    + (pos_off + p * 2 + 1) * 4,
                            )?;
                            if is_root {
                                points.push(Vec2::new(rx * ppu, -ry * ppu));
                            } else {
                                points.push(Vec2::new(rx, ry));
                            }
                        }
                        let (mul, scr) = self.get_bs_colors(
                            section::WARP_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::WARP_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaWarpKeyform {
                            points,
                            opacity: Some(op),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Warp(forms)
                }
                BlendShapeTargetKind::Rotation => {
                    let rot = self.doc.get_transform(&target_id).unwrap();
                    let is_root = rot.parent_id.is_none();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let ang = read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ANGLE] as usize + ki * 4,
                        )?;
                        let ox = read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_X] as usize + ki * 4,
                        )?;
                        let oy = read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_Y] as usize + ki * 4,
                        )?;
                        let sc = read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_SCALE] as usize + ki * 4,
                        )?;
                        let origin = if is_root {
                            Vec2::new(ox * ppu, -oy * ppu)
                        } else {
                            Vec2::new(ox, oy)
                        };
                        let (mul, scr) = self.get_bs_colors(
                            section::ROTATION_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::ROTATION_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaRotationKeyform {
                            origin: Some(origin),
                            angle: Some(ang),
                            scale: Some(sc),
                            opacity: Some(op),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Rotation(forms)
                }
                BlendShapeTargetKind::Mesh => {
                    let mesh = self.doc.get_mesh(&target_id).unwrap();
                    let vc = mesh.vertex_ids.len();
                    let is_root = mesh.deformer_id.is_empty();
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            self.bytes,
                            offsets[section::ART_MESH_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let d_order = read_f32(
                            self.bytes,
                            offsets[section::ART_MESH_KEY_SRC_DRAW_ORDER] as usize + ki * 4,
                        )?;
                        let pos_off = read_i32(
                            self.bytes,
                            offsets[section::ART_MESH_KEY_SRC_KEY_POS_OFF] as usize + ki * 4,
                        )? as usize;
                        let mut positions = Vec::with_capacity(vc);
                        for v in 0..vc {
                            let rx = read_f32(
                                self.bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2) * 4,
                            )?;
                            let ry = read_f32(
                                self.bytes,
                                offsets[section::KEY_POS_SRC_XY] as usize
                                    + (pos_off + v * 2 + 1) * 4,
                            )?;
                            if is_root {
                                positions.push(Vec2::new(rx * ppu, -ry * ppu));
                            } else {
                                positions.push(Vec2::new(rx, ry));
                            }
                        }
                        let (mul, scr) = self.get_bs_colors(
                            section::ART_MESH_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::ART_MESH_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaMeshKeyform {
                            positions,
                            opacity: Some(op),
                            draw_order: Some(d_order),
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Mesh(forms)
                }
                BlendShapeTargetKind::Glue => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let intensity = read_f32(
                            self.bytes,
                            offsets[section::GLUE_KEY_SRC_INTENSITY] as usize + ki * 4,
                        )?;
                        forms.push(DeltaGlueKeyform { intensity });
                    }
                    DeltaKeyforms::Glue(forms)
                }
                BlendShapeTargetKind::Offscreen => {
                    let mut forms = Vec::with_capacity(key_bs_len);
                    for k in 0..key_bs_len {
                        let ki = key_bs_off + k;
                        let op = read_f32(
                            self.bytes,
                            offsets[section::OFFSCREEN_KEY_SRC_OPACITY] as usize + ki * 4,
                        )?;
                        let (mul, scr) = self.get_bs_colors(
                            section::OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF,
                            section::OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF,
                            ki,
                        )?;
                        forms.push(DeltaOffscreenKeyform {
                            opacity: op,
                            multiply: mul,
                            screen: scr,
                        });
                    }
                    DeltaKeyforms::Offscreen(forms)
                }
            };

            let b_id = stable_id(&self.doc_id, "blend_binding", b, &format!("bb_{b}"));
            self.mapping.blend_binding_by_index.push(b_id.clone());
            check_status!(
                self.doc
                    .create_blend_binding(BlendShapeBinding {
                        id: b_id,
                        target_id,
                        target_kind,
                        key_table_id,
                        constraint_ids,
                        keyforms,
                    })
                    .status
            );
        }

        Ok(())
    }
}
