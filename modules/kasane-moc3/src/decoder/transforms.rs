use kasane_core::types::{RotationPose, SceneBinding, Status, Transform, Vec2};
use kasane_core::{
    RotationKeyform, RotationTransform, SceneTrack, TransformData, WarpKeyform, WarpTransform,
};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{parent_order, read_f32, read_i32, read_string, stable_id};

pub(super) type DecodedTransforms = (Vec<SceneBinding>, Vec<String>, Vec<String>);

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_transforms(&mut self) -> Result<DecodedTransforms, Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ppu = self.inspection.canvas.pixels_per_unit;
        let canvas_origin = self.doc.canvas().origin;

        let mut deformer_parents = Vec::with_capacity(counts.deformers as usize);
        for d in 0..counts.deformers as usize {
            deformer_parents.push(read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_PARENT_DEFORMER_IDX] as usize + d * 4,
            )?);
        }

        let deformer_creation_order = parent_order(&deformer_parents, "Deformer")?;

        let mut warp_by_local_idx = vec![String::new(); counts.warps as usize];
        let mut rotation_by_local_idx = vec![String::new(); counts.rotations as usize];
        let mut deformer_scene_bindings = Vec::new();

        for &d in &deformer_creation_order {
            let id = self.mapping.deformer_by_index[d].clone();
            let runtime_id = read_string(
                self.bytes,
                offsets[section::DEFORMER_SRC_ID] as usize + d * 64,
                64,
            );
            let b_idx = read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_BINDING_IDX] as usize + d * 4,
            )?;
            let enabled = read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_ENABLE] as usize + d * 4,
            )? != 0;
            let part_idx = read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_PARENT_PART_IDX] as usize + d * 4,
            )?;
            let parent_def_idx = deformer_parents[d];
            let dtype = read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_TYPE] as usize + d * 4,
            )?;
            let local_idx = read_i32(
                self.bytes,
                offsets[section::DEFORMER_SRC_LOCAL_IDX] as usize + d * 4,
            )? as usize;

            let part_id = if part_idx >= 0 && (part_idx as usize) < counts.parts as usize {
                self.mapping.part_by_index[part_idx as usize].clone()
            } else {
                String::new()
            };

            let parent_id =
                if parent_def_idx >= 0 && (parent_def_idx as usize) < counts.deformers as usize {
                    self.mapping.deformer_by_index[parent_def_idx as usize].clone()
                } else {
                    String::new()
                };

            let is_root = parent_id.is_empty();
            let axes = self.get_binding_axes(b_idx)?;
            let is_bound = !axes.is_empty();
            let total_combos = if is_bound {
                axes.iter().map(|a| a.keys.len()).product()
            } else {
                1
            };

            let is_warp = dtype == 0;
            if is_warp {
                let kf_off = read_i32(
                    self.bytes,
                    offsets[section::WARP_SRC_KEYFORM_OFF] as usize + local_idx * 4,
                )? as usize;
                let rows = read_i32(
                    self.bytes,
                    offsets[section::WARP_SRC_ROW] as usize + local_idx * 4,
                )? as u32;
                let cols = read_i32(
                    self.bytes,
                    offsets[section::WARP_SRC_COL] as usize + local_idx * 4,
                )? as u32;
                let quad = if offsets.len() > 101 && counts.warps > 0 {
                    read_i32(
                        self.bytes,
                        offsets[section::WARP_SRC_QUAD_TRANSFORM] as usize + local_idx * 4,
                    )? != 0
                } else {
                    true
                };

                let pt_count = ((rows + 1) * (cols + 1)) as usize;
                let mut keyforms = Vec::with_capacity(total_combos);

                for k in 0..total_combos {
                    let opacity = if kf_off + k < counts.warp_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::WARP_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        1.0
                    };
                    let pos_off = if kf_off + k < counts.warp_keyforms as usize {
                        read_i32(
                            self.bytes,
                            offsets[section::WARP_KEY_SRC_KEY_POS_OFF] as usize + (kf_off + k) * 4,
                        )? as usize
                    } else {
                        0
                    };

                    let mut points = Vec::with_capacity(pt_count);
                    for p_idx in 0..pt_count {
                        let rx = read_f32(
                            self.bytes,
                            offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + p_idx * 2) * 4,
                        )?;
                        let ry = read_f32(
                            self.bytes,
                            offsets[section::KEY_POS_SRC_XY] as usize
                                + (pos_off + p_idx * 2 + 1) * 4,
                        )?;
                        if is_root {
                            points.push(Vec2::new(
                                rx * ppu + canvas_origin.x,
                                canvas_origin.y - ry * ppu,
                            ));
                        } else {
                            points.push(Vec2::new(rx, ry));
                        }
                    }

                    let mut appearance =
                        self.get_colors(section::WARP_SRC_KEY_COLOR_OFF, local_idx, k)?;
                    appearance.opacity = opacity;

                    keyforms.push(WarpKeyform {
                        keys: if is_bound {
                            Self::combo_keys(&axes, k)
                        } else {
                            Vec::new()
                        },
                        positions: points,
                        appearance,
                    });
                }

                let base = &keyforms[0];
                check_status!(
                    self.doc
                        .create_transform(Transform {
                            id: id.clone(),
                            runtime_id: if runtime_id.is_empty() {
                                format!("Warp{local_idx}")
                            } else {
                                runtime_id.clone()
                            },
                            name: if runtime_id.is_empty() {
                                format!("Warp{local_idx}")
                            } else {
                                runtime_id
                            },
                            part_id: kasane_core::PartId::optional(part_id),
                            parent_id: kasane_core::TransformId::optional(parent_id),
                            enabled,
                            appearance: base.appearance,
                            data: TransformData::Warp(WarpTransform {
                                rows,
                                columns: cols,
                                quad,
                                points: base.positions.clone(),
                            }),
                        })
                        .status
                );
                warp_by_local_idx[local_idx] = id.clone();

                if is_bound {
                    deformer_scene_bindings.push(SceneBinding {
                        id: stable_id(&self.doc_id, "deformer_binding", d, &id),
                        axes,
                        track: SceneTrack::Warp {
                            target_id: id.into(),
                            keyforms,
                        },
                    });
                }
            } else {
                // Rotation Deformer
                let kf_off = read_i32(
                    self.bytes,
                    offsets[section::ROTATION_SRC_KEYFORM_OFF] as usize + local_idx * 4,
                )? as usize;
                let base_angle = read_f32(
                    self.bytes,
                    offsets[section::ROTATION_SRC_BASE_ANGLE] as usize + local_idx * 4,
                )?;

                let mut keyforms = Vec::with_capacity(total_combos);
                for k in 0..total_combos {
                    let opacity = if kf_off + k < counts.rotation_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        1.0
                    };
                    let angle = if kf_off + k < counts.rotation_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ANGLE] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        0.0
                    };
                    let ox = if kf_off + k < counts.rotation_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_X] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        0.0
                    };
                    let oy = if kf_off + k < counts.rotation_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_ORIGIN_Y] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        0.0
                    };
                    let scale = if kf_off + k < counts.rotation_keyforms as usize {
                        read_f32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_SCALE] as usize + (kf_off + k) * 4,
                        )?
                    } else {
                        1.0
                    };
                    let ref_x = if kf_off + k < counts.rotation_keyforms as usize {
                        read_i32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_REFLECT_X] as usize
                                + (kf_off + k) * 4,
                        )? != 0
                    } else {
                        false
                    };
                    let ref_y = if kf_off + k < counts.rotation_keyforms as usize {
                        read_i32(
                            self.bytes,
                            offsets[section::ROTATION_KEY_SRC_REFLECT_Y] as usize
                                + (kf_off + k) * 4,
                        )? != 0
                    } else {
                        false
                    };

                    let origin = if is_root {
                        kasane_core::types::PreciseVec2::new(
                            ox as f64 * ppu as f64 + canvas_origin.x as f64,
                            canvas_origin.y as f64 - oy as f64 * ppu as f64,
                        )
                    } else {
                        kasane_core::types::PreciseVec2::new(ox as f64, oy as f64)
                    };

                    let mut appearance =
                        self.get_colors(section::ROTATION_SRC_KEY_COLOR_OFF, local_idx, k)?;
                    appearance.opacity = opacity;

                    keyforms.push(RotationKeyform {
                        keys: if is_bound {
                            Self::combo_keys(&axes, k)
                        } else {
                            Vec::new()
                        },
                        rotation: RotationPose {
                            origin,
                            angle,
                            scale,
                            reflect_x: ref_x,
                            reflect_y: ref_y,
                        },
                        appearance,
                    });
                }

                let base = &keyforms[0];
                check_status!(
                    self.doc
                        .create_transform(Transform {
                            id: id.clone(),
                            runtime_id: if runtime_id.is_empty() {
                                format!("Rotation{local_idx}")
                            } else {
                                runtime_id.clone()
                            },
                            name: if runtime_id.is_empty() {
                                format!("Rotation{local_idx}")
                            } else {
                                runtime_id
                            },
                            part_id: kasane_core::PartId::optional(part_id),
                            parent_id: kasane_core::TransformId::optional(parent_id),
                            enabled,
                            appearance: base.appearance,
                            data: TransformData::Rotation(RotationTransform {
                                base_angle,
                                pose: base.rotation,
                            }),
                        })
                        .status
                );
                rotation_by_local_idx[local_idx] = id.clone();

                if is_bound {
                    deformer_scene_bindings.push(SceneBinding {
                        id: stable_id(&self.doc_id, "deformer_binding", d, &id),
                        axes,
                        track: SceneTrack::Rotation {
                            target_id: id.into(),
                            keyforms,
                        },
                    });
                }
            }
        }

        Ok((
            deformer_scene_bindings,
            warp_by_local_idx,
            rotation_by_local_idx,
        ))
    }
}
