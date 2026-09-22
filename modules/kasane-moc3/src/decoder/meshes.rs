use kasane_core::types::{BlendMode, Mesh, MeshBinding, MeshKeyform, Status, Vec2, VertexId};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{
    read_f32, read_i32, read_string, read_u16, read_u32, stable_id, texture_slot_uuid,
};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_art_meshes(&mut self) -> Result<Vec<MeshBinding>, Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;
        let ppu = self.inspection.canvas.pixels_per_unit;
        let canvas_origin = self.doc.canvas().origin;

        let mut mesh_bindings = Vec::new();
        let mut mesh_masks = Vec::new();

        for m in 0..counts.art_meshes as usize {
            let id = self.mapping.mesh_by_index[m].clone();
            let runtime_id = read_string(
                self.bytes,
                offsets[section::ART_MESH_SRC_ID] as usize + m * 64,
                64,
            );
            let b_idx = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_BINDING_IDX] as usize + m * 4,
            )?;
            let kf_off = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_KEYFORM_OFF] as usize + m * 4,
            )? as usize;
            let enabled = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_ENABLE] as usize + m * 4,
            )? != 0;
            let part_idx = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_PARENT_PART_IDX] as usize + m * 4,
            )?;
            let def_idx = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_PARENT_DEFORMER_IDX] as usize + m * 4,
            )?;
            let tex_no = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_TEXTURE_NO] as usize + m * 4,
            )?;
            let flag = self.bytes[offsets[section::ART_MESH_SRC_DRAWABLE_FLAG] as usize + m];
            let vc = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_VERTEX_COUNT] as usize + m * 4,
            )? as usize;
            let uv_off = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_UV_OFF] as usize + m * 4,
            )? as usize;
            let idx_off = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_IDX_OFF] as usize + m * 4,
            )? as usize;
            let idx_len = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_IDX_LEN] as usize + m * 4,
            )? as usize;
            let mask_off = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_MASK_OFF] as usize + m * 4,
            )? as usize;
            let mask_len = read_i32(
                self.bytes,
                offsets[section::ART_MESH_SRC_MASK_LEN] as usize + m * 4,
            )? as usize;

            let part_id = if part_idx >= 0 && (part_idx as usize) < counts.parts as usize {
                self.mapping.part_by_index[part_idx as usize].clone()
            } else {
                String::new()
            };

            let deformer_id = if def_idx >= 0 && (def_idx as usize) < counts.deformers as usize {
                self.mapping.deformer_by_index[def_idx as usize].clone()
            } else {
                String::new()
            };

            let is_root = deformer_id.is_empty();

            let slot = if tex_no >= 0 { tex_no as usize } else { 0 };
            let texture_asset_id = if let Some(t) = self.texture_by_slot.get(&slot) {
                t.asset_id.clone()
            } else {
                let id = texture_slot_uuid(slot);
                if self.doc.get_asset(&id).is_none() {
                    check_status!(
                        self.doc
                            .add_asset(kasane_core::types::ImageAsset {
                                id: id.clone(),
                                name: format!("Texture {slot} (Unmapped)"),
                                source: format!("assets/textures/unmapped_slot_{slot}.png"),
                                width: 1,
                                height: 1,
                                sha256: "0".repeat(64),
                            })
                            .status
                    );
                }
                id
            };

            let double_sided = (flag & 4) != 0;
            let inverted_mask = (flag & 8) != 0;
            let blend_mode = match flag & 3 {
                1 => BlendMode::Additive,
                2 => BlendMode::Multiplicative,
                _ => BlendMode::Normal,
            };
            let raw_blend_mode =
                if ver >= 6 && offsets.len() > 153 && offsets[section::ART_MESH_SRC_BLEND_MODE] > 0
                {
                    let raw_bm = read_u32(
                        self.bytes,
                        offsets[section::ART_MESH_SRC_BLEND_MODE] as usize + m * 4,
                    )?;
                    if raw_bm != 0 {
                        Some(raw_bm)
                    } else {
                        None
                    }
                } else {
                    None
                };

            // Read UVs
            let mut uvs = Vec::with_capacity(vc);
            for v in 0..vc {
                let u = read_f32(
                    self.bytes,
                    offsets[section::UV_SRC_XY] as usize + (uv_off + v * 2) * 4,
                )?;
                let v_val = read_f32(
                    self.bytes,
                    offsets[section::UV_SRC_XY] as usize + (uv_off + v * 2 + 1) * 4,
                )?;
                uvs.push(Vec2::new(u, 1.0 - v_val));
            }

            // Generate 1-indexed dense vertex IDs
            let vertex_ids: Vec<VertexId> = (1..=vc as u32).collect();

            // Read triangles (inverting winding by swapping 1 and 2)
            if !idx_len.is_multiple_of(3) {
                return Err(Status::error(
                    "INVALID_LENGTH",
                    format!("Mesh {m} has index count {idx_len} not divisible by 3"),
                ));
            }
            let mut triangles = Vec::with_capacity(idx_len / 3);
            for t in 0..(idx_len / 3) {
                let i0 = read_u16(
                    self.bytes,
                    offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3) * 2,
                )? as usize;
                let i1 = read_u16(
                    self.bytes,
                    offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3 + 1) * 2,
                )? as usize;
                let i2 = read_u16(
                    self.bytes,
                    offsets[section::IDX_SRC_IDX] as usize + (idx_off + t * 3 + 2) * 2,
                )? as usize;
                if i0 >= vc || i1 >= vc || i2 >= vc {
                    return Err(Status::error(
                        "INVALID_INDEX",
                        format!(
                            "Mesh {m} triangle {t} index out of bounds: ({i0}, {i1}, {i2}) with vertex count {vc}"
                        ),
                    ));
                }
                if i0 == i1 || i1 == i2 || i0 == i2 {
                    return Err(Status::error(
                        "REPEATED_VERTEX",
                        format!(
                            "Mesh {m} triangle {t} contains duplicate vertices: ({i0}, {i1}, {i2})"
                        ),
                    ));
                }
                // Invert winding swap (render swapped 1 and 2, so swapping 1 and 2 restores source)
                triangles.push([vertex_ids[i0], vertex_ids[i2], vertex_ids[i1]]);
            }

            // Masks
            let mut masks = Vec::with_capacity(mask_len);
            for m_idx in 0..mask_len {
                let target_mesh_idx = read_i32(
                    self.bytes,
                    offsets[section::MASK_SRC_ART_MESH_IDX] as usize + (mask_off + m_idx) * 4,
                )?;
                if target_mesh_idx >= 0 && (target_mesh_idx as usize) < counts.art_meshes as usize {
                    let target_id = self.mapping.mesh_by_index[target_mesh_idx as usize].clone();
                    if !masks.contains(&target_id) {
                        masks.push(target_id);
                    }
                }
            }

            let axes = self.get_binding_axes(b_idx)?;
            let is_bound = !axes.is_empty();
            let total_combos = if is_bound {
                axes.iter().map(|a| a.keys.len()).product()
            } else {
                1
            };

            let mut keyforms = Vec::with_capacity(total_combos);
            for k in 0..total_combos {
                let opacity = if kf_off + k < counts.art_mesh_keyforms as usize {
                    read_f32(
                        self.bytes,
                        offsets[section::ART_MESH_KEY_SRC_OPACITY] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    1.0
                };
                let d_order = if kf_off + k < counts.art_mesh_keyforms as usize {
                    read_f32(
                        self.bytes,
                        offsets[section::ART_MESH_KEY_SRC_DRAW_ORDER] as usize + (kf_off + k) * 4,
                    )?
                } else {
                    m as f32
                };
                let pos_off = if kf_off + k < counts.art_mesh_keyforms as usize {
                    read_i32(
                        self.bytes,
                        offsets[section::ART_MESH_KEY_SRC_KEY_POS_OFF] as usize + (kf_off + k) * 4,
                    )? as usize
                } else {
                    0
                };

                let mut positions = Vec::with_capacity(vc);
                for v in 0..vc {
                    let rx = read_f32(
                        self.bytes,
                        offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2) * 4,
                    )?;
                    let ry = read_f32(
                        self.bytes,
                        offsets[section::KEY_POS_SRC_XY] as usize + (pos_off + v * 2 + 1) * 4,
                    )?;
                    if is_root {
                        positions.push(Vec2::new(
                            rx * ppu + canvas_origin.x,
                            canvas_origin.y - ry * ppu,
                        ));
                    } else {
                        positions.push(Vec2::new(rx, ry));
                    }
                }

                let mut appearance = self.get_colors(section::ART_MESH_SRC_KEY_COLOR_OFF, m, k)?;
                appearance.opacity = opacity;

                keyforms.push(MeshKeyform {
                    keys: if is_bound {
                        Self::combo_keys(&axes, k)
                    } else {
                        Vec::new()
                    },
                    positions,
                    appearance,
                    draw_order: Some(d_order),
                });
            }

            let base = &keyforms[0];
            check_status!(
                self.doc
                    .create_mesh(Mesh {
                        id: id.clone(),
                        name: if runtime_id.is_empty() {
                            format!("ArtMesh{m}")
                        } else {
                            runtime_id.clone()
                        },
                        texture_asset_id,
                        vertex_ids,
                        base_positions: base.positions.clone(),
                        uvs,
                        triangles,
                        runtime_id: if runtime_id.is_empty() {
                            format!("ArtMesh{m}")
                        } else {
                            runtime_id
                        },
                        part_id,
                        deformer_id,
                        appearance: base.appearance,
                        draw_order: base.draw_order,
                        blend_mode,
                        enabled,
                        double_sided,
                        inverted_mask,
                        masks: Vec::new(),
                        raw_blend_mode,
                    })
                    .status
            );

            if !masks.is_empty() {
                mesh_masks.push((id.clone(), masks));
            }

            if is_bound {
                mesh_bindings.push(MeshBinding {
                    id: stable_id(&self.doc_id, "mesh_binding", m, &id),
                    mesh_id: id,
                    axes,
                    keyforms,
                });
            }
        }

        // Set mesh masks now that all meshes exist in document
        for (m_id, masks) in mesh_masks {
            let mut mesh = self.doc.get_mesh(&m_id).unwrap().clone();
            mesh.masks = masks;
            check_status!(self.doc.replace_mesh(mesh).status);
        }

        Ok(mesh_bindings)
    }
}
