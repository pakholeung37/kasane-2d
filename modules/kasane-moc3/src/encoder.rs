use std::collections::HashMap;

use kasane_core::evaluation::{evaluate_frame, to_parent_positions, DrawableFrame};
use kasane_core::types::{
    Appearance, BlendMode, BlendShapeBinding, BlendShapeTargetKind, DeltaKeyforms, ParameterKind,
    SceneKeyform, Status, TransformKind, Vec2, VertexId,
};
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

fn find_vertex_pos(mesh: &kasane_core::types::Mesh, vid: VertexId) -> Result<u16, Status> {
    let index = mesh
        .vertex_ids
        .iter()
        .position(|&v| v == vid)
        .ok_or_else(|| {
            Status::error("MISSING_VERTEX", format!("{}: Glue vertex {vid}", mesh.id))
        })?;
    u16::try_from(index).map_err(|_| {
        Status::error(
            "CAPACITY",
            format!("{}: Glue index {index} exceeds u16", mesh.id),
        )
    })
}

fn write_id(l: &mut Layout, field: &str, value: &str) -> Result<(), Status> {
    if !is_representable(value) {
        return Err(Status::error(
            "UNREPRESENTABLE_ID",
            format!("{field}: requires 1..63 printable ASCII bytes"),
        ));
    }
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

fn write_bs_colors(
    l: &mut Layout,
    prefix: &str,
    multiply: Option<[f32; 3]>,
    screen: Option<[f32; 3]>,
) -> Result<(), Status> {
    let mul = multiply.unwrap_or([0.0, 0.0, 0.0]);
    let scr = screen.unwrap_or([0.0, 0.0, 0.0]);
    let offset = checked(l.field("keyform_mul_color_src.r")?.len() / 4, "colors")?;
    l.integer(
        &format!("{prefix}.key_mul_color_off"),
        if multiply.is_some() { offset } else { -1 },
    )?;
    l.integer(
        &format!("{prefix}.key_scr_color_off"),
        if screen.is_some() { offset } else { -1 },
    )?;
    let channels = ["r", "g", "b"];
    for c in 0..3 {
        l.scalar(&format!("keyform_mul_color_src.{}", channels[c]), mul[c])?;
        l.scalar(&format!("keyform_scr_color_src.{}", channels[c]), scr[c])?;
    }
    Ok(())
}

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

fn write_delta_positions(
    l: &mut Layout,
    doc: &Document,
    prefix: &str,
    parent: &str,
    positions: &[Vec2],
    expected_len: usize,
) -> Result<(), Status> {
    let ppu = doc.canvas().pixels_per_unit;
    let is_root = parent.is_empty();
    let offset = checked(l.field("key_pos_src.xy")?.len() / 4, "positions")?;
    l.integer(&format!("{prefix}.key_pos_off"), offset)?;
    if positions.is_empty() {
        for _ in 0..expected_len {
            l.scalar("key_pos_src.xy", 0.0)?;
            l.scalar("key_pos_src.xy", 0.0)?;
        }
    } else {
        for p in positions {
            if is_root {
                l.scalar("key_pos_src.xy", p.x / ppu)?;
                l.scalar("key_pos_src.xy", -p.y / ppu)?;
            } else {
                l.scalar("key_pos_src.xy", p.x)?;
                l.scalar("key_pos_src.xy", p.y)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Moc3ExportVersion {
    #[default]
    Auto,
    V50,
    V53,
}

pub fn encode_moc3(doc: &Document) -> Result<Moc3Artifact, Status> {
    encode_moc3_with_version(doc, Moc3ExportVersion::Auto)
}

pub fn encode_moc3_with_version(
    doc: &Document,
    target_version: Moc3ExportVersion,
) -> Result<Moc3Artifact, Status> {
    if doc.transaction_active() {
        return Err(Status::error(
            "TRANSACTION_ACTIVE",
            "Commit or cancel edits before export",
        ));
    }

    let has_v53_features = doc.offscreen_count() > 0
        || doc
            .mesh_order()
            .iter()
            .any(|id| doc.get_mesh(id).and_then(|m| m.raw_blend_mode).is_some())
        || doc.blend_binding_order().iter().any(|id| {
            doc.get_blend_binding(id)
                .map(|b| b.target_kind == BlendShapeTargetKind::Offscreen)
                .unwrap_or(false)
        });

    let export_version = match target_version {
        Moc3ExportVersion::V50 => {
            if has_v53_features {
                return Err(Status::error(
                    "INCOMPATIBLE_EXPORT_VERSION",
                    "Cannot export to MOC3 5.0: document contains Cubism 5.3 features (offscreens or extended blend modes)",
                ));
            }
            5
        }
        Moc3ExportVersion::V53 => 6,
        Moc3ExportVersion::Auto => {
            if has_v53_features {
                6
            } else {
                5
            }
        }
    };

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

    for id in doc.glue_order() {
        let g = doc.get_glue(id).unwrap();
        if !is_representable(&g.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in doc.offscreen_order() {
        let os = doc.get_offscreen(id).unwrap();
        if !is_representable(&os.runtime_id) {
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

    for id in doc.glue_order() {
        if let Some(binding) = &doc.get_glue(id).unwrap().binding {
            all_bindings.push(BindingView {
                id,
                axes: &binding.axes,
            });
        }
    }

    let mut l = Layout::with_version(export_version);
    let n = checked(drawables.len(), "art_meshes")?;
    l.counts[4] = n as u32;
    l.counts[19] = n as u32;
    l.counts[12] = checked(all_bindings.len() + 1, "bindings")? as u32; // Binding 0 is static.
    l.counts[5] = checked(doc.parameter_order().len(), "parameters")? as u32;
    l.counts[0] = checked(parts.len(), "parts")? as u32;
    l.counts[1] = checked(transforms.len(), "deformers")? as u32;

    if export_version >= 6 {
        l.counts[35] = doc.offscreen_count() as u32;
    }

    // The runtime interpolates a contiguous window beginning at the owner's
    // first Part key. Materialize document mappings (including neutral slots)
    // in canonical Part order rather than exporting arbitrary local indices.
    let mut os_key_bases: HashMap<&str, i32> = HashMap::new();
    if export_version >= 6 {
        for os_id in doc.offscreen_order() {
            let os = doc.get_offscreen(os_id).unwrap();
            let binding = doc.binding_for_scene(&os.part_id);
            let count = binding.map_or(1, |b| b.keyforms.len());
            let stored = binding.map_or(1, |b| count.max(1usize << b.axes.len()));
            os_key_bases.insert(
                os.id.as_str(),
                checked(l.counts[36] as usize, "offscreen_keyforms")?,
            );
            for slot in 0..stored {
                let key = os
                    .keyform_index(slot.min(count - 1), binding.map(|b| b.keyforms.len()))?
                    .map(|index| &os.keyforms[index]);
                l.scalar("offscreen_key_src.opacity", key.map_or(1.0, |k| k.opacity))?;
                // Ordinary colors are absolute values, unlike blendshape deltas.
                // Both color pools must have a contiguous entry for every slot.
                write_bs_colors(
                    &mut l,
                    "offscreen_key_src",
                    Some(key.and_then(|k| k.multiply).unwrap_or([1.0; 3])),
                    Some(key.and_then(|k| k.screen).unwrap_or([0.0; 3])),
                )?;
                l.counts[36] += 1;
            }
        }
    }

    let c = doc.canvas();
    {
        let canvas = l.field("canvas_info")?;
        canvas.extend_from_slice(&c.pixels_per_unit.to_le_bytes());
        canvas.extend_from_slice(&c.origin.x.to_le_bytes());
        canvas.extend_from_slice(&(c.height - c.origin.y).to_le_bytes());
        canvas.extend_from_slice(&c.width.to_le_bytes());
        canvas.extend_from_slice(&c.height.to_le_bytes());
        canvas.resize(24, 0);
        canvas[20] = c.flag; // Core applies this direction to stored geometry, UVs and winding.
    }

    l.integer("binding_src.key_table_idx_off", 0)?;
    l.integer("binding_src.key_table_idx_len", 0)?;

    let mut table_indices: HashMap<&str, Vec<i32>> = HashMap::new();
    for b in &all_bindings {
        table_indices.insert(b.id, vec![0; b.axes.len()]);
    }

    let mut param_bkts: HashMap<&str, Vec<&str>> = HashMap::new();
    for bkt_id in doc.blend_key_table_order() {
        let bkt = doc.get_blend_key_table(bkt_id).unwrap();
        param_bkts
            .entry(bkt.parameter_id.as_str())
            .or_default()
            .push(bkt_id);
    }

    let mut ordered_bkts: Vec<&str> = Vec::new();
    let mut bkt_indices: HashMap<&str, i32> = HashMap::new();
    for id in doc.parameter_order() {
        if let Some(bkts) = param_bkts.get(id.as_str()) {
            for &bkt_id in bkts {
                bkt_indices.insert(bkt_id, ordered_bkts.len() as i32);
                ordered_bkts.push(bkt_id);
            }
        }
    }
    for bkt_id in doc.blend_key_table_order() {
        if !bkt_indices.contains_key(bkt_id.as_str()) {
            bkt_indices.insert(bkt_id, ordered_bkts.len() as i32);
            ordered_bkts.push(bkt_id);
        }
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
        l.integer("param_src.repeat", if p.repeat { 1 } else { 0 })?;
        l.integer("param_src.decimal_places", p.decimal_places)?;

        let is_bs_param = p.kind == ParameterKind::BlendShape;
        l.integer("param_src.type", if is_bs_param { 1 } else { 0 })?;

        let bkts = param_bkts.get(id.as_str());
        if let Some(bkts) = bkts {
            let first_bkt = bkt_indices[bkts[0]];
            l.integer("param_src.blend_key_table_off", first_bkt)?;
            l.integer("param_src.blend_key_table_len", bkts.len() as i32)?;
        } else {
            l.integer("param_src.blend_key_table_off", 0)?;
            l.integer("param_src.blend_key_table_len", 0)?;
        }

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

        if let Some(bkts) = bkts {
            for &bkt_id in bkts {
                let bkt = doc.get_blend_key_table(bkt_id).unwrap();
                union_keys.extend_from_slice(&bkt.keys);
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

    for &bkt_id in &ordered_bkts {
        let bkt = doc.get_blend_key_table(bkt_id).unwrap();
        let keys_off = checked(l.field("keys_src.key")?.len() / 4, "bkt_keys_off")?;
        l.integer("blend_key_table_src.keys_off", keys_off)?;
        l.integer(
            "blend_key_table_src.keys_len",
            checked(bkt.keys.len(), "bkt_keys_len")?,
        )?;
        l.integer("blend_key_table_src.base_key_idx", bkt.base_key_idx as i32)?;
        for &k in &bkt.keys {
            l.scalar("keys_src.key", k)?;
        }
    }
    l.counts[25] = ordered_bkts.len() as u32;

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

        if export_version >= 6 {
            let os_idx = if let Some(os) = doc.offscreen_for_part(&part.id) {
                doc.offscreen_order()
                    .iter()
                    .position(|id| id == &os.id)
                    .map(|i| i as i32)
                    .unwrap_or(-1)
            } else {
                -1
            };
            l.integer("part_src.offscreen_idx", os_idx)?;
        }

        for k in 0..stored {
            let draw_order = if let Some(b) = b {
                b.keyforms[k.min(count - 1) as usize].draw_order
            } else {
                part.draw_order
            };
            l.scalar("part_key_src.draw_order", draw_order)?;

            if export_version >= 6 {
                let key_idx = if let Some(os) = doc.offscreen_for_part(&part.id) {
                    os_key_bases[os.id.as_str()] + k
                } else {
                    -1
                };
                l.integer("part_key_src.key_idx", key_idx)?;
            }
        }
        l.counts[6] += stored as u32;
    }

    let mut warp_local_indices: HashMap<&str, usize> = HashMap::new();
    let mut rotation_local_indices: HashMap<&str, usize> = HashMap::new();

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
        if warp {
            warp_local_indices.insert(id.as_str(), local_idx as usize);
        } else {
            rotation_local_indices.insert(id.as_str(), local_idx as usize);
        }

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
                let origin = kasane_core::evaluation::to_parent_origin(
                    doc,
                    &t.parent_id,
                    f.rotation.origin,
                )?;
                l.scalar("rotation_key_src.origin_x", origin.x)?;
                l.scalar("rotation_key_src.origin_y", origin.y)?;
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

    let groups = kasane_core::draw_order::resolved_groups(doc);
    let totals = if export_version >= 6 {
        kasane_core::draw_order::descendant_counts_with_offscreens(doc, &groups)
    } else {
        kasane_core::draw_order::descendant_counts(&groups)
    };
    let group_slots: HashMap<&str, usize> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.owner.as_str(), i))
        .collect();
    for group in &groups {
        let first = l.field("draw_group_obj_src.idx")?.len() / 4;
        for id in &group.items {
            let is_part = doc.get_part(id).is_some();
            l.integer("draw_group_obj_src.type", if is_part { 1 } else { 0 })?;
            l.integer(
                "draw_group_obj_src.idx",
                if is_part {
                    index_of(&parts, id)
                } else {
                    index_of(doc.mesh_order(), id)
                },
            )?;
            l.integer(
                "draw_group_obj_src.self_group_idx",
                if is_part {
                    checked(group_slots[id.as_str()], "draw_group")?
                } else {
                    -1
                },
            )?;
        }
        l.integer("draw_group_src.obj_off", checked(first, "draw_group")?)?;
        l.integer(
            "draw_group_src.obj_len",
            checked(group.items.len(), "draw_group")?,
        )?;
        l.integer(
            "draw_group_src.obj_total_count",
            checked(totals[group.owner.as_str()], "draw_group")?,
        )?;
        l.integer("draw_group_src.min_order", group.min_order)?;
        l.integer("draw_group_src.max_order", group.max_order)?;
    }
    l.counts[18] = checked(groups.len(), "draw_groups")? as u32;

    l.counts[19] = checked(l.field("draw_group_obj_src.idx")?.len() / 4, "draw_items")? as u32;

    let mut mesh_indices: HashMap<&str, usize> = HashMap::new();
    for (i, d) in drawables.iter().enumerate() {
        mesh_indices.insert(d.id.as_str(), i);
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

        if export_version >= 6 {
            let raw_bm = mesh.raw_blend_mode.unwrap_or(0);
            l.integer("art_mesh_src.blend_mode", raw_bm as i32)?;
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
            l.scalar(
                "uv_src.xy",
                if doc.canvas().flag & 1 == 0 {
                    1.0 - p.y
                } else {
                    p.y
                },
            )?;
        }

        let idx_field = l.field("idx_src.idx")?;
        // evaluate_frame exposes revived runtime winding. Undo the canvas
        // reversal here because Core applies it while reviving the file.
        for triangle in d.indices.chunks_exact(3) {
            let stored = if doc.canvas().flag & 1 == 0 {
                [triangle[2], triangle[1], triangle[0]]
            } else {
                [triangle[0], triangle[1], triangle[2]]
            };
            for v in stored {
                idx_field.extend_from_slice(&(v as u16).to_le_bytes());
            }
        }
    }

    l.counts[9] = keyform_offset as u32;

    // Glues
    l.counts[20] = checked(doc.glue_order().len(), "glues")? as u32;
    let mut glue_info_count = 0;
    for g_id in doc.glue_order() {
        let g = doc.get_glue(g_id).unwrap();
        let mesh_a = doc.get_mesh(&g.mesh_a_id).unwrap();
        let mesh_b = doc.get_mesh(&g.mesh_b_id).unwrap();
        let ma_idx = index_of(doc.mesh_order(), &g.mesh_a_id);
        let mb_idx = index_of(doc.mesh_order(), &g.mesh_b_id);
        let b_idx = if g.binding.is_some() {
            binding_indices[g_id.as_str()]
        } else {
            0
        };
        let key_off = checked(
            l.field("glue_key_src.intensity")?.len() / 4,
            "glue key offset",
        )?;
        let key_len = g.binding.as_ref().map_or(1, |b| b.keyforms.len());
        write_id(&mut l, "glue_src.id", &g.runtime_id)?;
        l.integer("glue_src.binding_idx", b_idx)?;
        l.integer("glue_src.keyform_off", key_off)?;
        l.integer("glue_src.key_len", checked(key_len, "glue key count")?)?;
        l.integer("glue_src.art_mesh_idx_a", ma_idx)?;
        l.integer("glue_src.art_mesh_idx_b", mb_idx)?;
        let info_off = glue_info_count;
        let info_len = checked(g.pairs.len() * 2, "glue_info_len")?;
        l.integer("glue_src.info_off", info_off)?;
        l.integer("glue_src.info_len", info_len)?;
        if let Some(binding) = &g.binding {
            for key in &binding.keyforms {
                l.scalar("glue_key_src.intensity", key.intensity)?;
            }
        } else {
            l.scalar("glue_key_src.intensity", g.intensity)?;
        }
        for pair in &g.pairs {
            let pos_a = find_vertex_pos(mesh_a, pair.vertex_a)?;
            let pos_b = find_vertex_pos(mesh_b, pair.vertex_b)?;
            l.scalar("glue_info_src.weight", pair.weight_a)?;
            l.short("glue_info_src.pos_idx", pos_a)?;
            l.scalar("glue_info_src.weight", pair.weight_b)?;
            l.short("glue_info_src.pos_idx", pos_b)?;
        }
        glue_info_count += info_len;
    }
    l.counts[21] = glue_info_count as u32;
    l.counts[22] = checked(
        l.field("glue_key_src.intensity")?.len() / 4,
        "glue keyforms",
    )? as u32;

    // BlendShape Constraints
    let mut constraint_index_map: HashMap<&str, i32> = HashMap::new();
    for (c_idx, c_id) in doc.blend_constraint_order().iter().enumerate() {
        let c = doc.get_blend_constraint(c_id).unwrap();
        let p_idx = index_of(doc.parameter_order(), &c.parameter_id);
        let val_off = checked(
            l.field("blend_constraint_val_src.key")?.len() / 4,
            "constraint_vals",
        )?;
        let val_len = checked(c.keys.len(), "constraint_val_len")?;
        l.integer("blend_constraint_src.parameter_idx", p_idx)?;
        l.integer("blend_constraint_src.value_off", val_off)?;
        l.integer("blend_constraint_src.value_len", val_len)?;
        for (&k, &w) in c.keys.iter().zip(c.weights.iter()) {
            l.scalar("blend_constraint_val_src.key", k)?;
            l.scalar("blend_constraint_val_src.weight", w)?;
        }
        constraint_index_map.insert(c.id.as_str(), c_idx as i32);
    }
    l.counts[30] = checked(doc.blend_constraint_order().len(), "bs_constraints")? as u32;
    l.counts[31] = checked(
        l.field("blend_constraint_val_src.key")?.len() / 4,
        "constraint_vals",
    )? as u32;

    if export_version >= 6 {
        for os_id in doc.offscreen_order() {
            let os = doc.get_offscreen(os_id).unwrap();
            let owner_idx = parts.iter().position(|p| p == &os.part_id).unwrap() as i32;
            l.integer("offscreen_src.owner_idx", owner_idx)?;
            l.field("offscreen_src.drawable_flag")?.push(os.flags);
            l.integer("offscreen_src.blend_mode", os.blend_mode as i32)?;
            let mask_off = checked(l.field("mask_src.art_mesh_idx")?.len() / 4, &os.id)?;
            l.integer("offscreen_src.mask_off", mask_off)?;
            l.integer("offscreen_src.mask_len", checked(os.masks.len(), &os.id)?)?;
            for mask in &os.masks {
                l.integer("mask_src.art_mesh_idx", index_of(doc.mesh_order(), mask))?;
            }
        }
    }

    let mut warp_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut mesh_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut part_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut rot_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut glue_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut offscreen_targets: HashMap<&str, Vec<&str>> = HashMap::new();

    for bid in doc.blend_binding_order() {
        let b = doc.get_blend_binding(bid).unwrap();
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
    sorted_warp_targets.sort_by_key(|id| warp_local_indices.get(id).copied().unwrap_or(usize::MAX));

    let mut sorted_mesh_targets: Vec<&str> = mesh_targets.keys().copied().collect();
    sorted_mesh_targets.sort_by_key(|id| mesh_indices.get(id).copied().unwrap_or(usize::MAX));

    let mut sorted_part_targets: Vec<&str> = part_targets.keys().copied().collect();
    sorted_part_targets.sort_by_key(|id| index_of(&parts, id));

    let mut sorted_rot_targets: Vec<&str> = rot_targets.keys().copied().collect();
    sorted_rot_targets.sort_by_key(|id| {
        rotation_local_indices
            .get(id)
            .copied()
            .unwrap_or(usize::MAX)
    });

    let mut sorted_glue_targets: Vec<&str> = glue_targets.keys().copied().collect();
    sorted_glue_targets.sort_by_key(|id| index_of(doc.glue_order(), id));

    // 1. Warp BlendShapes
    for target_id in sorted_warp_targets {
        let target_local = warp_local_indices[target_id];
        let binding_ids = &warp_targets[target_id];
        let warp = doc.get_transform(target_id).unwrap();
        let bs_b_off = l.counts[26] as i32;
        let bs_b_len = checked(binding_ids.len(), "bs_warp_b_len")?;
        l.integer("bs_warp_src.target_idx", target_local as i32)?;
        l.integer("bs_warp_src.bs_binding_off", bs_b_off)?;
        l.integer("bs_warp_src.bs_binding_len", bs_b_len)?;
        l.counts[27] += 1;

        for &bid in binding_ids {
            let b = doc.get_blend_binding(bid).unwrap();
            if let DeltaKeyforms::Warp(ref forms) = b.keyforms {
                let key_bs_off = l.counts[7] as i32;
                let key_bs_len = forms.len() as i32;
                write_blend_binding(
                    &mut l,
                    b,
                    key_bs_off,
                    key_bs_len,
                    &bkt_indices,
                    &constraint_index_map,
                )?;
                l.counts[26] += 1;
                let pt_count = ((warp.rows + 1) * (warp.columns + 1)) as usize;
                for f in forms {
                    l.scalar("warp_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                    write_delta_positions(
                        &mut l,
                        doc,
                        "warp_key_src",
                        &warp.parent_id,
                        &f.points,
                        pt_count,
                    )?;
                    write_bs_colors(&mut l, "warp_key_src", f.multiply, f.screen)?;
                    l.counts[7] += 1;
                }
            }
        }
    }

    // 2. Mesh BlendShapes
    for target_id in sorted_mesh_targets {
        let target_idx = mesh_indices[target_id];
        let binding_ids = &mesh_targets[target_id];
        let mesh = doc.get_mesh(target_id).unwrap();
        let bs_b_off = l.counts[26] as i32;
        let bs_b_len = checked(binding_ids.len(), "bs_mesh_b_len")?;
        l.integer("bs_art_mesh_src.target_idx", target_idx as i32)?;
        l.integer("bs_art_mesh_src.bs_binding_off", bs_b_off)?;
        l.integer("bs_art_mesh_src.bs_binding_len", bs_b_len)?;
        l.counts[28] += 1;

        for &bid in binding_ids {
            let b = doc.get_blend_binding(bid).unwrap();
            if let DeltaKeyforms::Mesh(ref forms) = b.keyforms {
                let key_bs_off = l.counts[9] as i32;
                let key_bs_len = forms.len() as i32;
                write_blend_binding(
                    &mut l,
                    b,
                    key_bs_off,
                    key_bs_len,
                    &bkt_indices,
                    &constraint_index_map,
                )?;
                l.counts[26] += 1;
                let vc = mesh.vertex_ids.len();
                for f in forms {
                    l.scalar("art_mesh_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                    l.scalar("art_mesh_key_src.draw_order", f.draw_order.unwrap_or(0.0))?;
                    write_delta_positions(
                        &mut l,
                        doc,
                        "art_mesh_key_src",
                        &mesh.deformer_id,
                        &f.positions,
                        vc,
                    )?;
                    write_bs_colors(&mut l, "art_mesh_key_src", f.multiply, f.screen)?;
                    l.counts[9] += 1;
                }
            }
        }
    }

    // 3. Part BlendShapes
    for target_id in sorted_part_targets {
        let target_idx = index_of(&parts, target_id);
        let binding_ids = &part_targets[target_id];
        let bs_b_off = l.counts[26] as i32;
        let bs_b_len = checked(binding_ids.len(), "bs_part_b_len")?;
        l.integer("bs_part_src.target_idx", target_idx)?;
        l.integer("bs_part_src.bs_binding_off", bs_b_off)?;
        l.integer("bs_part_src.bs_binding_len", bs_b_len)?;
        l.counts[32] += 1;

        for &bid in binding_ids {
            let b = doc.get_blend_binding(bid).unwrap();
            if let DeltaKeyforms::Part(ref forms) = b.keyforms {
                let key_bs_off = l.counts[6] as i32;
                let key_bs_len = forms.len() as i32;
                write_blend_binding(
                    &mut l,
                    b,
                    key_bs_off,
                    key_bs_len,
                    &bkt_indices,
                    &constraint_index_map,
                )?;
                l.counts[26] += 1;
                for f in forms {
                    l.scalar("part_key_src.draw_order", f.draw_order)?;
                    l.counts[6] += 1;
                }
            }
        }
    }

    // 4. Rotation BlendShapes
    for target_id in sorted_rot_targets {
        let target_local = rotation_local_indices[target_id];
        let binding_ids = &rot_targets[target_id];
        let rot = doc.get_transform(target_id).unwrap();
        let bs_b_off = l.counts[26] as i32;
        let bs_b_len = checked(binding_ids.len(), "bs_rot_b_len")?;
        l.integer("bs_rotation_src.target_idx", target_local as i32)?;
        l.integer("bs_rotation_src.bs_binding_off", bs_b_off)?;
        l.integer("bs_rotation_src.bs_binding_len", bs_b_len)?;
        l.counts[33] += 1;

        for &bid in binding_ids {
            let b = doc.get_blend_binding(bid).unwrap();
            if let DeltaKeyforms::Rotation(ref forms) = b.keyforms {
                let key_bs_off = l.counts[8] as i32;
                let key_bs_len = forms.len() as i32;
                write_blend_binding(
                    &mut l,
                    b,
                    key_bs_off,
                    key_bs_len,
                    &bkt_indices,
                    &constraint_index_map,
                )?;
                l.counts[26] += 1;
                for f in forms {
                    l.scalar("rotation_key_src.opacity", f.opacity.unwrap_or(0.0))?;
                    l.scalar("rotation_key_src.angle", f.angle.unwrap_or(0.0))?;
                    let origin = if let Some(o) = f.origin {
                        if rot.parent_id.is_empty() {
                            let ppu = doc.canvas().pixels_per_unit;
                            Vec2::new(o.x / ppu, -o.y / ppu)
                        } else {
                            o
                        }
                    } else {
                        Vec2::default()
                    };
                    l.scalar("rotation_key_src.origin_x", origin.x)?;
                    l.scalar("rotation_key_src.origin_y", origin.y)?;
                    l.scalar("rotation_key_src.scale", f.scale.unwrap_or(0.0))?;
                    l.integer("rotation_key_src.reflect_x", 0)?;
                    l.integer("rotation_key_src.reflect_y", 0)?;
                    write_bs_colors(&mut l, "rotation_key_src", f.multiply, f.screen)?;
                    l.counts[8] += 1;
                }
            }
        }
    }

    // 5. Glue BlendShapes
    for target_id in sorted_glue_targets {
        let target_idx = index_of(doc.glue_order(), target_id);
        let binding_ids = &glue_targets[target_id];
        let bs_b_off = l.counts[26] as i32;
        let bs_b_len = checked(binding_ids.len(), "bs_glue_b_len")?;
        l.integer("bs_glue_src.target_idx", target_idx)?;
        l.integer("bs_glue_src.bs_binding_off", bs_b_off)?;
        l.integer("bs_glue_src.bs_binding_len", bs_b_len)?;
        l.counts[34] += 1;

        for &bid in binding_ids {
            let b = doc.get_blend_binding(bid).unwrap();
            if let DeltaKeyforms::Glue(ref forms) = b.keyforms {
                let key_bs_off =
                    checked(l.field("glue_key_src.intensity")?.len() / 4, "glue_bs_off")?;
                let key_bs_len = forms.len() as i32;
                write_blend_binding(
                    &mut l,
                    b,
                    key_bs_off,
                    key_bs_len,
                    &bkt_indices,
                    &constraint_index_map,
                )?;
                l.counts[26] += 1;
                for f in forms {
                    l.scalar("glue_key_src.intensity", f.intensity)?;
                }
            }
        }
    }

    // 6. Offscreen BlendShapes
    if export_version >= 6 {
        let mut sorted_offscreen_targets: Vec<&str> = offscreen_targets.keys().copied().collect();
        sorted_offscreen_targets.sort_by_key(|id| index_of(doc.offscreen_order(), id));

        for target_id in sorted_offscreen_targets {
            let target_idx = index_of(doc.offscreen_order(), target_id);
            let binding_ids = &offscreen_targets[target_id];
            let bs_b_off = l.counts[26] as i32;
            let bs_b_len = checked(binding_ids.len(), "bs_offscreen_b_len")?;
            l.integer("bs_offscreen_src.target_idx", target_idx)?;
            l.integer("bs_offscreen_src.bs_binding_off", bs_b_off)?;
            l.integer("bs_offscreen_src.bs_binding_len", bs_b_len)?;
            l.counts[37] += 1;

            for &bid in binding_ids {
                let b = doc.get_blend_binding(bid).unwrap();
                if let DeltaKeyforms::Offscreen(ref forms) = b.keyforms {
                    let key_bs_off = l.counts[36] as i32;
                    let key_bs_len = forms.len() as i32;
                    write_blend_binding(
                        &mut l,
                        b,
                        key_bs_off,
                        key_bs_len,
                        &bkt_indices,
                        &constraint_index_map,
                    )?;
                    l.counts[26] += 1;
                    for f in forms {
                        l.scalar("offscreen_key_src.opacity", f.opacity)?;
                        write_bs_colors(&mut l, "offscreen_key_src", f.multiply, f.screen)?;
                        l.counts[36] += 1;
                    }
                }
            }
        }
    }

    l.counts[22] = checked(
        l.field("glue_key_src.intensity")?.len() / 4,
        "glue keyforms",
    )? as u32;

    l.counts[29] = checked(
        l.field("blend_constraint_idx_src.constraint_idx")?.len() / 4,
        "bs_constraint_idx",
    )? as u32;

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
