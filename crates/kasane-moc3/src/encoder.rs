use std::collections::HashMap;

use kasane_core::evaluation::{evaluate_frame, to_parent_positions, DrawableFrame};
use kasane_core::types::{Appearance, BlendMode, SceneKeyform, Status, TransformKind, Vec2};
use kasane_core::Document;

use crate::layout::{checked, Layout};
use crate::types::{Moc3Artifact, TextureSlot};

fn is_representable(id: &str) -> bool {
    !id.is_empty() && id.len() <= 63 && id.bytes().all(|c| (0x20..=0x7e).contains(&c))
}

fn index_of(ids: &[String], id: &str) -> i32 {
    if id.is_empty() {
        -1
    } else {
        ids.iter()
            .position(|x| x == id)
            .map(|i| i as i32)
            .unwrap_or(-1)
    }
}

fn write_id(l: &mut Layout, field: &str, value: &str) -> Result<(), Status> {
    let bytes = l.field(field)?;
    let start = bytes.len();
    bytes.extend_from_slice(value.as_bytes());
    bytes.resize(start + 64, 0);
    Ok(())
}

fn write_colors(l: &mut Layout, prefix: &str, appearance: &Appearance) -> Result<(), Status> {
    let offset = checked(l.field("keyform_mul_color_src.r")?.len() / 4, "colors")?;
    l.integer(&format!("{prefix}.key_mul_color_off"), offset)?;
    l.integer(&format!("{prefix}.key_scr_color_off"), offset)?;
    let channels = ["r", "g", "b"];
    for (c, &ch) in channels.iter().enumerate() {
        l.scalar(
            &format!("keyform_mul_color_src.{ch}"),
            appearance.multiply[c],
        )?;
        l.scalar(&format!("keyform_scr_color_src.{ch}"), appearance.screen[c])?;
    }
    Ok(())
}

fn write_positions(
    l: &mut Layout,
    doc: &Document,
    prefix: &str,
    parent: &str,
    positions: &[Vec2],
) -> Result<(), Status> {
    let converted = to_parent_positions(doc, parent, positions)?;
    let offset = checked(l.field("key_pos_src.xy")?.len() / 4, "positions")?;
    l.integer(&format!("{prefix}.key_pos_off"), offset)?;
    for p in converted {
        l.scalar("key_pos_src.xy", p.x)?;
        l.scalar("key_pos_src.xy", p.y)?;
    }
    Ok(())
}

fn descendant_count(doc: &Document, parts: &[String], parent: &str) -> i32 {
    let mut count = 0;
    for id in doc.mesh_order() {
        if doc.get_mesh(id).unwrap().part_id == parent {
            count += 1;
        }
    }
    for id in parts {
        if doc.get_part(id).unwrap().parent_id == parent {
            count += descendant_count(doc, parts, id);
        }
    }
    count
}

pub fn encode_moc3(doc: &Document) -> Result<Moc3Artifact, Status> {
    if doc.transaction_active() {
        return Err(Status::error(
            "TRANSACTION_ACTIVE",
            "Commit or cancel edits before export",
        ));
    }

    let mut frame = DrawableFrame::default();
    let s = evaluate_frame(doc, &HashMap::new(), &mut frame);
    if !s.is_ok() {
        return Err(s);
    }

    let drawables = &frame.drawables;
    if drawables.is_empty() {
        return Err(Status::error(
            "EMPTY_MODEL",
            "At least one mesh is required",
        ));
    }

    if !(doc.canvas().height - doc.canvas().origin.y).is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            format!(
                "{}.canvas.origin: runtime origin overflows float32",
                doc.id()
            ),
        ));
    }

    for d in drawables {
        if !is_representable(&d.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", d.id),
            ));
        }
        if d.positions.len() > 65536 {
            return Err(Status::error(
                "CAPACITY",
                format!("{}.vertex_ids: at most 65536 vertices", d.id),
            ));
        }
    }

    for id in doc.parameter_order() {
        let p = doc.get_parameter(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    let parts = doc.sorted_parts();
    let transforms = doc.sorted_transforms();

    for id in &parts {
        let p = doc.get_part(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in &transforms {
        let t = doc.get_transform(id).unwrap();
        if !is_representable(&t.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    struct BindingView<'a> {
        id: &'a str,
        axes: &'a [kasane_core::types::BindingAxis],
    }

    let mut all_bindings = Vec::new();
    for id in doc.binding_order() {
        all_bindings.push(BindingView {
            id,
            axes: &doc.get_binding(id).unwrap().axes,
        });
    }
    for id in doc.scene_binding_order() {
        all_bindings.push(BindingView {
            id,
            axes: &doc.get_scene_binding(id).unwrap().axes,
        });
    }

    let mut l = Layout::new();
    let n = checked(drawables.len(), "art_meshes")?;
    l.counts[4] = n as u32;
    l.counts[19] = n as u32;
    l.counts[12] = checked(all_bindings.len() + 1, "bindings")? as u32; // Binding 0 is static.
    l.counts[5] = checked(doc.parameter_order().len(), "parameters")? as u32;
    l.counts[0] = checked(parts.len(), "parts")? as u32;
    l.counts[1] = checked(transforms.len(), "deformers")? as u32;
    l.counts[18] = checked(parts.len() + 1, "groups")? as u32; // Root draw group.

    let c = doc.canvas();
    {
        let canvas = l.field("canvas_info")?;
        canvas.extend_from_slice(&c.pixels_per_unit.to_le_bytes());
        canvas.extend_from_slice(&c.origin.x.to_le_bytes());
        canvas.extend_from_slice(&(c.height - c.origin.y).to_le_bytes());
        canvas.extend_from_slice(&c.width.to_le_bytes());
        canvas.extend_from_slice(&c.height.to_le_bytes());
        canvas.resize(24, 0);
        canvas[20] = 1; // Positions/winding already use runtime Y direction.
    }

    l.integer("binding_src.key_table_idx_off", 0)?;
    l.integer("binding_src.key_table_idx_len", 0)?;

    let mut table_indices: HashMap<&str, Vec<i32>> = HashMap::new();
    for b in &all_bindings {
        table_indices.insert(b.id, vec![0; b.axes.len()]);
    }

    let mut table_count: i32 = 0;
    for id in doc.parameter_order() {
        let p = doc.get_parameter(id).unwrap();
        {
            let ids = l.field("param_src.id")?;
            let start = ids.len();
            ids.extend_from_slice(p.runtime_id.as_bytes());
            ids.resize(start + 64, 0);
        }
        l.scalar("param_src.maximum_value", p.maximum)?;
        l.scalar("param_src.minimum_value", p.minimum)?;
        l.scalar("param_src.default_value", p.default_value)?;
        l.integer("param_src.repeat", 0)?;
        l.integer("param_src.decimal_places", p.decimal_places)?;
        l.integer("param_src.type", 0)?;
        l.integer("param_src.blend_key_table_off", 0)?;
        l.integer("param_src.blend_key_table_len", 0)?;

        let first = table_count;
        let mut union_keys = Vec::new();
        for binding in &all_bindings {
            let bid = binding.id;
            for a in 0..binding.axes.len() {
                let axis = &binding.axes[a];
                if axis.parameter_id != *id {
                    continue;
                }
                table_indices.get_mut(bid).unwrap()[a] = table_count;
                table_count += 1;
                let keys_off = checked(
                    l.field("keys_src.key")?.len() / 4,
                    &format!("{bid}.keys_off"),
                )?;
                l.integer("key_table_src.keys_off", keys_off)?;
                l.integer(
                    "key_table_src.keys_len",
                    checked(axis.keys.len(), &format!("{bid}.keys_len"))?,
                )?;
                for &k in &axis.keys {
                    l.scalar("keys_src.key", k)?;
                }
                union_keys.extend_from_slice(&axis.keys);
            }
        }

        l.integer("param_src.key_table_off", first)?;
        l.integer("param_src.key_table_len", table_count - first)?;

        union_keys.sort_by(|a, b| a.total_cmp(b));
        union_keys.dedup();

        let union_keys_off = checked(
            l.field("keys_src.key")?.len() / 4,
            &format!("{id}.keys_off"),
        )?;
        l.integer("param_keys_src.keys_off", union_keys_off)?;
        l.integer(
            "param_keys_src.keys_len",
            checked(union_keys.len(), &format!("{id}.keys_len"))?,
        )?;
        for &k in &union_keys {
            l.scalar("keys_src.key", k)?;
        }
    }

    l.counts[13] = table_count as u32;
    l.counts[14] = checked(l.field("keys_src.key")?.len() / 4, "keys")? as u32;

    let mut binding_indices: HashMap<&str, i32> = HashMap::new();
    for b in &all_bindings {
        let bid = b.id;
        let b_idx = checked(binding_indices.len() + 1, "binding_index")?;
        binding_indices.insert(bid, b_idx);
        let axes_offset = checked(
            l.field("key_table_idx_src.idx")?.len() / 4,
            &format!("{bid}.axes_offset"),
        )?;
        l.integer("binding_src.key_table_idx_off", axes_offset)?;
        let t_indices = &table_indices[bid];
        l.integer(
            "binding_src.key_table_idx_len",
            checked(t_indices.len(), &format!("{bid}.axes_count"))?,
        )?;
        for &idx in t_indices {
            l.integer("key_table_idx_src.idx", idx)?;
        }
    }
    l.counts[11] = checked(l.field("key_table_idx_src.idx")?.len() / 4, "key_table_idx")? as u32;

    let mut keyform_offset: i32 = 0;

    for id in &parts {
        let part = doc.get_part(id).unwrap();
        let b = doc.binding_for_scene(id);
        let count = if let Some(b) = b {
            checked(b.keyforms.len(), id)?
        } else {
            1
        };
        let stored = if let Some(b) = b {
            count.max(1i32 << b.axes.len())
        } else {
            1
        };

        write_id(&mut l, "part_src.id", &part.runtime_id)?;
        l.integer(
            "part_src.binding_idx",
            if let Some(b) = b {
                binding_indices[b.id.as_str()]
            } else {
                0
            },
        )?;
        l.integer("part_src.keyform_off", l.counts[6] as i32)?;
        l.integer("part_src.key_len", count)?;
        l.integer("part_src.visible", 1)?;
        l.integer("part_src.enable", if part.enabled { 1 } else { 0 })?;
        l.integer(
            "part_src.parent_part_idx",
            index_of(&parts, &part.parent_id),
        )?;

        for k in 0..stored {
            let draw_order = if let Some(b) = b {
                b.keyforms[k.min(count - 1) as usize].draw_order
            } else {
                part.draw_order
            };
            l.scalar("part_key_src.draw_order", draw_order)?;
        }
        l.counts[6] += stored as u32;
    }

    for id in &transforms {
        let t = doc.get_transform(id).unwrap();
        let b = doc.binding_for_scene(id);
        let warp = t.kind == TransformKind::Warp;
        let prefix = if warp { "warp" } else { "rotation" };
        let count = if let Some(b) = b {
            checked(b.keyforms.len(), id)?
        } else {
            1
        };
        let stored = if let Some(b) = b {
            count.max(1i32 << b.axes.len())
        } else {
            1
        };

        write_id(&mut l, "deformer_src.id", &t.runtime_id)?;
        l.integer(
            "deformer_src.binding_idx",
            if let Some(b) = b {
                binding_indices[b.id.as_str()]
            } else {
                0
            },
        )?;
        l.integer("deformer_src.visible", 1)?;
        l.integer("deformer_src.enable", if t.enabled { 1 } else { 0 })?;
        l.integer("deformer_src.parent_part_idx", index_of(&parts, &t.part_id))?;
        l.integer(
            "deformer_src.parent_deformer_idx",
            index_of(&transforms, &t.parent_id),
        )?;
        l.integer("deformer_src.type", if warp { 0 } else { 1 })?;

        let local_slot = if warp { 2 } else { 3 };
        let local_idx = l.counts[local_slot];
        l.counts[local_slot] += 1;
        l.integer("deformer_src.local_idx", local_idx as i32)?;

        l.integer(
            &format!("{prefix}_src.binding_idx"),
            if let Some(b) = b {
                binding_indices[b.id.as_str()]
            } else {
                0
            },
        )?;
        l.integer(
            &format!("{prefix}_src.keyform_off"),
            l.counts[if warp { 7 } else { 8 }] as i32,
        )?;
        l.integer(&format!("{prefix}_src.key_len"), count)?;
        let color_off = checked(l.field("keyform_mul_color_src.r")?.len() / 4, id)?;
        l.integer(&format!("{prefix}_src.key_color_off"), color_off)?;

        if warp {
            l.integer("warp_src.vertex_count", checked(t.points.len(), id)?)?;
            l.integer("warp_src.row", t.rows as i32)?;
            l.integer("warp_src.col", t.columns as i32)?;
            l.integer("warp_src.quad_transform", if t.quad { 1 } else { 0 })?;
        } else {
            l.scalar("rotation_src.base_angle", t.base_angle)?;
        }

        for k in 0..stored {
            let f = if let Some(b) = b {
                b.keyforms[k.min(count - 1) as usize].clone()
            } else {
                SceneKeyform {
                    keys: Vec::new(),
                    positions: t.points.clone(),
                    rotation: t.rotation,
                    appearance: t.appearance,
                    draw_order: 0.0,
                }
            };

            l.scalar(&format!("{prefix}_key_src.opacity"), f.appearance.opacity)?;
            write_colors(&mut l, &format!("{prefix}_key_src"), &f.appearance)?;

            if warp {
                write_positions(&mut l, doc, "warp_key_src", &t.parent_id, &f.positions)?;
            } else {
                let origin = to_parent_positions(doc, &t.parent_id, &[f.rotation.origin])?;
                l.scalar("rotation_key_src.origin_x", origin[0].x)?;
                l.scalar("rotation_key_src.origin_y", origin[0].y)?;
                l.scalar("rotation_key_src.angle", f.rotation.angle)?;
                l.scalar("rotation_key_src.scale", f.rotation.scale)?;
                l.integer(
                    "rotation_key_src.reflect_x",
                    if f.rotation.reflect_x { 1 } else { 0 },
                )?;
                l.integer(
                    "rotation_key_src.reflect_y",
                    if f.rotation.reflect_y { 1 } else { 0 },
                )?;
            }
        }
        l.counts[if warp { 7 } else { 8 }] += stored as u32;
    }

    // One group per Part, in parent-first order, plus root.
    let mut groups = vec![String::new()];
    groups.extend_from_slice(&parts);

    for parent in &groups {
        let first = l.field("draw_group_obj_src.idx")?.len() / 4;
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;

        for (i, d) in drawables.iter().enumerate() {
            let m = doc.get_mesh(&d.id).unwrap();
            if m.part_id != *parent {
                continue;
            }
            let order = m.draw_order.unwrap_or(i as f32);
            lo = lo.min(order);
            hi = hi.max(order);
            if let Some(b) = doc.binding_for_mesh(&m.id) {
                for f in &b.keyforms {
                    let k_order = f.draw_order.unwrap_or(order);
                    lo = lo.min(k_order);
                    hi = hi.max(k_order);
                }
            }
            l.integer("draw_group_obj_src.type", 0)?;
            l.integer("draw_group_obj_src.idx", i as i32)?;
            l.integer("draw_group_obj_src.self_group_idx", -1)?;
        }

        for (i, pid) in parts.iter().enumerate() {
            let p = doc.get_part(pid).unwrap();
            if p.parent_id != *parent {
                continue;
            }
            lo = lo.min(p.draw_order);
            hi = hi.max(p.draw_order);
            if let Some(b) = doc.binding_for_scene(pid) {
                for f in &b.keyforms {
                    lo = lo.min(f.draw_order);
                    hi = hi.max(f.draw_order);
                }
            }
            l.integer("draw_group_obj_src.type", 1)?;
            l.integer("draw_group_obj_src.idx", i as i32)?;
            l.integer("draw_group_obj_src.self_group_idx", (i + 1) as i32)?;
        }

        l.integer("draw_group_src.obj_off", checked(first, "draw_group")?)?;
        let obj_len = checked(
            l.field("draw_group_obj_src.idx")?.len() / 4 - first,
            "draw_group",
        )?;
        l.integer("draw_group_src.obj_len", obj_len)?;
        l.integer(
            "draw_group_src.obj_total_count",
            descendant_count(doc, &parts, parent),
        )?;
        l.integer("draw_group_src.min_order", lo.floor() as i32)?;
        l.integer("draw_group_src.max_order", hi.ceil() as i32)?;
    }

    l.counts[19] = checked(l.field("draw_group_obj_src.idx")?.len() / 4, "draw_items")? as u32;

    for (i, d) in drawables.iter().enumerate() {
        {
            let ids = l.field("art_mesh_src.id")?;
            ids.extend_from_slice(d.runtime_id.as_bytes());
            ids.resize((i + 1) * 64, 0);
        }
        let mesh = doc.get_mesh(&d.id).unwrap();
        let binding = doc.binding_for_mesh(&d.id);
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

        l.integer(
            "art_mesh_src.binding_idx",
            if let Some(b) = binding {
                binding_indices[b.id.as_str()]
            } else {
                0
            },
        )?;
        l.integer("art_mesh_src.keyform_off", keyform_offset)?;
        l.integer("art_mesh_src.key_len", key_count)?;
        let color_off = checked(l.field("keyform_mul_color_src.r")?.len() / 4, &d.id)?;
        l.integer("art_mesh_src.key_color_off", color_off)?;
        l.integer("art_mesh_src.visible", 1)?;
        l.integer("art_mesh_src.enable", if mesh.enabled { 1 } else { 0 })?;
        l.integer(
            "art_mesh_src.parent_part_idx",
            index_of(&parts, &mesh.part_id),
        )?;
        l.integer(
            "art_mesh_src.parent_deformer_idx",
            index_of(&transforms, &mesh.deformer_id),
        )?;
        l.integer("art_mesh_src.texture_no", d.texture_slot)?;

        let flag: u8 = (if mesh.double_sided { 4 } else { 0 })
            | (if mesh.inverted_mask { 8 } else { 0 })
            | (match mesh.blend_mode {
                BlendMode::Additive => 1,
                BlendMode::Multiplicative => 2,
                BlendMode::Normal => 0,
            });
        l.field("art_mesh_src.drawable_flag")?.push(flag);

        l.integer(
            "art_mesh_src.vertex_count",
            checked(d.positions.len(), &format!("{}.vertex_count", d.id))?,
        )?;
        let uv_off = checked(l.field("uv_src.xy")?.len() / 4, &format!("{}.uv_off", d.id))?;
        l.integer("art_mesh_src.uv_off", uv_off)?;
        let idx_off = checked(
            l.field("idx_src.idx")?.len() / 2,
            &format!("{}.idx_off", d.id),
        )?;
        l.integer("art_mesh_src.idx_off", idx_off)?;
        l.integer(
            "art_mesh_src.idx_len",
            checked(d.indices.len(), &format!("{}.idx_len", d.id))?,
        )?;
        let mask_off = checked(l.field("mask_src.art_mesh_idx")?.len() / 4, &d.id)?;
        l.integer("art_mesh_src.mask_off", mask_off)?;
        l.integer("art_mesh_src.mask_len", checked(mesh.masks.len(), &d.id)?)?;

        for mask in &mesh.masks {
            l.integer("mask_src.art_mesh_idx", index_of(doc.mesh_order(), mask))?;
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
            l.scalar("art_mesh_key_src.opacity", appearance.opacity)?;
            let order = if let Some(b) = binding {
                b.keyforms[k.min(key_count - 1) as usize]
                    .draw_order
                    .unwrap_or(base_order)
            } else {
                base_order
            };
            l.scalar("art_mesh_key_src.draw_order", order)?;
            write_positions(
                &mut l,
                doc,
                "art_mesh_key_src",
                &mesh.deformer_id,
                positions,
            )?;
            write_colors(&mut l, "art_mesh_key_src", appearance)?;
            keyform_offset = checked(keyform_offset as usize + 1, &format!("{}.keyforms", d.id))?;
        }

        for p in &d.uvs {
            l.scalar("uv_src.xy", p.x)?;
            l.scalar("uv_src.xy", p.y)?;
        }

        let idx_field = l.field("idx_src.idx")?;
        for &v in &d.indices {
            idx_field.extend_from_slice(&(v as u16).to_le_bytes());
        }
    }

    l.counts[9] = keyform_offset as u32;
    let colors_count = checked(l.field("keyform_mul_color_src.r")?.len() / 4, "colors")? as u32;
    l.counts[23] = colors_count;
    l.counts[24] = colors_count;
    l.counts[17] = checked(l.field("mask_src.art_mesh_idx")?.len() / 4, "masks")? as u32;
    l.counts[10] = checked(l.field("key_pos_src.xy")?.len() / 4, "keyform_pos")? as u32;
    l.counts[15] = checked(l.field("uv_src.xy")?.len() / 4, "uvs")? as u32;
    l.counts[16] = checked(l.field("idx_src.idx")?.len() / 2, "idx")? as u32;

    let mut result = Moc3Artifact {
        bytes: l.finish()?,
        model3_json: "{\n  \"Version\": 3,\n  \"FileReferences\": {\n    \"Moc\": \"model.moc3\",\n    \"Textures\": [".to_string(),
        textures: Vec::new(),
    };

    for id in doc.asset_order() {
        let asset = doc.get_asset(id).unwrap();
        let path = format!("textures/{}.png", result.textures.len());
        if !result.textures.is_empty() {
            result.model3_json.push(',');
        }
        result.model3_json.push_str(&format!("\n      \"{path}\""));
        result.textures.push(TextureSlot {
            asset_id: id.clone(),
            source: asset.source.clone(),
            package_path: path,
            width: asset.width,
            height: asset.height,
        });
    }

    result.model3_json.push_str("\n    ]\n  }\n}\n");
    Ok(result)
}

pub fn encode_moc3_into(doc: &Document, out: &mut Moc3Artifact) -> Status {
    match encode_moc3(doc) {
        Ok(artifact) => {
            *out = artifact;
            Status::ok()
        }
        Err(status) => status,
    }
}
