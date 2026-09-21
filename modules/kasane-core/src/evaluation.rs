use std::collections::HashMap;

use crate::deformers::{rotation_parent_angle, rotation_points, warp_points, PsmVec2};
use crate::document::Document;
use crate::geometry::{to_runtime_positions, validate_positions};
use crate::keyforms::{blend_vectors, find_key_segment, key_combinations, KeyAxis};
use crate::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint, Canvas,
    DeltaKeyforms, Mesh, RotationPose, Status, Transform, TransformKind, Vec2, VertexId,
};

pub type PreviewValues = HashMap<String, f32>;

#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    pub id: String,
    pub runtime_id: String,
    pub texture_asset_id: String,
    pub texture_slot: i32,
    pub positions: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub indices: Vec<u32>,
    pub draw_order: i32,
    pub render_order: i32,
    pub opacity: f32,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub visible: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl Default for Drawable {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            texture_asset_id: String::new(),
            texture_slot: 0,
            positions: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            draw_order: 0,
            render_order: 0,
            opacity: 1.0,
            multiply_color: [1.0, 1.0, 1.0, 1.0],
            screen_color: [0.0, 0.0, 0.0, 1.0],
            blend_mode: BlendMode::Normal,
            enabled: true,
            visible: true,
            double_sided: true,
            inverted_mask: false,
            masks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedParameter {
    pub id: String,
    pub requested: f32,
    pub value: f32,
    pub clamped: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawableFrame {
    pub source_revision: u64,
    pub canvas: Canvas,
    pub parameters: Vec<EvaluatedParameter>,
    pub drawables: Vec<Drawable>,
}

pub fn to_parent_positions(
    doc: &Document,
    parent: &str,
    positions: &[Vec2],
) -> Result<Vec<Vec2>, Status> {
    if parent.is_empty() {
        to_runtime_positions(doc.canvas(), positions)
    } else {
        if doc.get_transform(parent).is_none() {
            return Err(Status::error("MISSING_TRANSFORM", parent));
        }
        let s = validate_positions(positions);
        if !s.is_ok() {
            return Err(s);
        }
        Ok(positions.to_vec())
    }
}

fn find_vertex_index(mesh: &Mesh, vid: VertexId) -> Option<usize> {
    if vid >= 1 && (vid as usize) <= mesh.vertex_ids.len() {
        let idx = (vid - 1) as usize;
        if mesh.vertex_ids[idx] == vid {
            return Some(idx);
        }
    }
    mesh.vertex_ids.iter().position(|&v| v == vid)
}

#[derive(Debug, Clone)]
struct Selection {
    indices: Vec<usize>,
    weights: Vec<f32>,
    enabled: bool,
}

fn select(doc: &Document, values: &HashMap<String, f32>, binding: &[BindingAxis]) -> Selection {
    let mut axes = Vec::with_capacity(binding.len());
    let mut enabled = true;

    for axis in binding {
        let p = doc.get_parameter(&axis.parameter_id).unwrap();
        let epsilon = 0.1f32.powi(p.decimal_places);
        let segment = find_key_segment(values[&p.id], &axis.keys, epsilon, epsilon * 1.5);
        enabled &= !segment.is_outside;
        axes.push(KeyAxis {
            index: segment.index,
            key_count: axis.keys.len() as i32,
            weight: segment.weight,
        });
    }

    let max_count = 1usize << axes.len();
    let mut indices = vec![0i32; max_count];
    let mut weights = vec![1.0f32; max_count];
    let count = key_combinations(&axes, &mut indices, &mut weights);

    Selection {
        indices: indices[..count].iter().map(|&i| i as usize).collect(),
        weights: weights[..count].to_vec(),
        enabled,
    }
}

fn blend_appearance<F>(s: &Selection, mut get: F) -> Appearance
where
    F: FnMut(usize) -> Appearance,
{
    let mut a = Appearance {
        opacity: 0.0,
        multiply: [0.0, 0.0, 0.0],
        screen: [0.0, 0.0, 0.0],
    };
    for k in 0..s.indices.len() {
        let f = get(s.indices[k]);
        let w = s.weights[k];
        a.opacity += f.opacity * w;
        for c in 0..3 {
            a.multiply[c] += f.multiply[c] * w;
            a.screen[c] += f.screen[c] * w;
        }
    }
    a
}

fn inherit_appearance(child: &mut Appearance, parent: &Appearance) {
    child.opacity *= parent.opacity;
    for c in 0..3 {
        child.multiply[c] *= parent.multiply[c];
        child.screen[c] = child.screen[c] + parent.screen[c] - child.screen[c] * parent.screen[c];
    }
}

fn evaluate_constraint(c: &BlendShapeConstraint, val: f32) -> f32 {
    let n = c.keys.len();
    if n == 0 {
        return 1.0;
    }
    if n == 1 || val <= c.keys[0] {
        return c.weights[0];
    }
    if val >= c.keys[n - 1] {
        return c.weights[n - 1];
    }
    let mut idx = 0;
    while idx + 1 < n && val >= c.keys[idx + 1] {
        idx += 1;
    }
    let span = c.keys[idx + 1] - c.keys[idx];
    if span <= 0.0 {
        return c.weights[idx];
    }
    let t = (val - c.keys[idx]) / span;
    c.weights[idx] * (1.0 - t) + c.weights[idx + 1] * t
}

pub fn evaluate_blend_binding(
    doc: &Document,
    values: &HashMap<String, f32>,
    b: &BlendShapeBinding,
) -> Vec<(usize, f32)> {
    let kt = match doc.get_blend_key_table(&b.key_table_id) {
        Some(kt) => kt,
        None => return Vec::new(),
    };
    let v = values.get(&kt.parameter_id).copied().unwrap_or(0.0);
    let key_count = kt.keys.len();
    if key_count < 2 {
        return Vec::new();
    }

    let mut index = 0usize;
    let mut weight = 0.0f32;
    if v > kt.keys[0] {
        while index + 1 < key_count && v >= kt.keys[index + 1] {
            index += 1;
        }
        if index < key_count - 1 {
            let span = kt.keys[index + 1] - kt.keys[index];
            if span > 0.0 {
                weight = (v - kt.keys[index]) / span;
            }
        }
    }

    let base_key_idx = kt.base_key_idx;
    let mut keyforms = Vec::with_capacity(2);
    if weight != 0.0 && index == base_key_idx {
        keyforms.push((index + 1, weight));
    } else if weight == 0.0 {
        if index != base_key_idx {
            keyforms.push((index, 1.0));
        }
    } else {
        if index + 1 == base_key_idx {
            keyforms.push((index, 1.0 - weight));
        } else {
            keyforms.push((index, 1.0 - weight));
            keyforms.push((index + 1, weight));
        }
    }

    let mut constraint_weight = 1.0f32;
    for c_id in &b.constraint_ids {
        if let Some(c) = doc.get_blend_constraint(c_id) {
            let cv = values.get(&c.parameter_id).copied().unwrap_or(0.0);
            let cw = evaluate_constraint(c, cv);
            constraint_weight = constraint_weight.min(cw);
        }
    }

    if constraint_weight == 0.0 {
        return Vec::new();
    }

    keyforms
        .into_iter()
        .map(|(idx, w)| (idx, w * constraint_weight))
        .filter(|(_, w)| *w != 0.0)
        .collect()
}

fn blend_positions<F>(
    doc: &Document,
    parent: &str,
    s: &Selection,
    mut get: F,
) -> Result<Vec<Vec2>, Status>
where
    F: FnMut(usize) -> Vec<Vec2>,
{
    let mut data: Vec<Vec<f32>> = Vec::with_capacity(s.indices.len());
    let mut size = 0usize;

    for k in 0..s.indices.len() {
        let raw = get(s.indices[k]);
        let converted = to_parent_positions(doc, parent, &raw)?;
        size = converted.len();
        let mut row = Vec::with_capacity(size * 2);
        for p in converted {
            row.push(p.x);
            row.push(p.y);
        }
        data.push(row);
    }

    if size > (i32::MAX as usize) / 2 {
        return Err(Status::error("CAPACITY", "positions"));
    }

    let mut xy = vec![0.0f32; size * 2];
    let pointers: Vec<&[f32]> = data.iter().map(|v| v.as_slice()).collect();
    blend_vectors(&pointers, &s.weights, size * 2, &mut xy);

    let mut out = Vec::with_capacity(size);
    for i in 0..size {
        out.push(Vec2::new(xy[2 * i], xy[2 * i + 1]));
    }
    let status = validate_positions(&out);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct TransformState<'a> {
    source: &'a Transform,
    pose: RotationPose,
    points: Vec<f32>,
    appearance: Appearance,
    inherited_scale: f32,
    enabled: bool,
}

impl<'a> TransformState<'a> {
    fn point(&self, p: PsmVec2) -> PsmVec2 {
        let input = [p.x, p.y];
        let mut output = [0.0f32; 2];
        if self.source.kind == TransformKind::Warp {
            warp_points(
                self.source.rows as i32,
                self.source.columns as i32,
                self.source.quad,
                &self.points,
                &input,
                &mut output,
                1,
            );
        } else {
            rotation_points(
                self.source.base_angle,
                self.pose.angle,
                self.pose.scale,
                PsmVec2::new(self.pose.origin.x, self.pose.origin.y),
                self.pose.reflect_x,
                self.pose.reflect_y,
                &input,
                &mut output,
                1,
            );
        }
        PsmVec2::new(output[0], output[1])
    }
}

fn f32_to_i32(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        v.clamp(-2147483648.0, 2147483520.0) as i32
    }
}

pub fn evaluate_frame(doc: &Document, preview: &PreviewValues, out: &mut DrawableFrame) -> Status {
    if !doc.initialized() {
        return Status::error("NOT_INITIALIZED", "Initialize Document first");
    }
    if doc.mesh_order().len() > 16777216 || doc.asset_order().len() > (i32::MAX as usize) {
        return Status::error("CAPACITY", "Object count");
    }

    for (id, &v) in preview {
        if doc.get_parameter(id).is_none() {
            return Status::error("MISSING_PARAMETER", id);
        }
        if !v.is_finite() {
            return Status::error("NON_FINITE", format!("{}.preview_value", id));
        }
    }

    let mut frame = DrawableFrame {
        source_revision: doc.revision(),
        canvas: doc.canvas(),
        parameters: Vec::with_capacity(doc.parameter_order().len()),
        drawables: Vec::with_capacity(doc.mesh_order().len()),
    };

    let mut values = HashMap::new();
    for id in doc.parameter_order() {
        let p = doc.get_parameter(id).unwrap();
        let requested = preview.get(id).copied().unwrap_or(p.default_value);
        let v = requested.clamp(p.minimum, p.maximum);
        frame.parameters.push(EvaluatedParameter {
            id: id.clone(),
            requested,
            value: v,
            clamped: requested != v,
        });
        values.insert(id.clone(), v);
    }

    let mut enabled_parts = HashMap::new();
    let mut part_orders = HashMap::new();
    for id in &doc.sorted_parts() {
        let p = doc.get_part(id).unwrap();
        let mut enabled = p.enabled && (p.parent_id.is_empty() || enabled_parts[&p.parent_id]);
        let mut order = p.draw_order;
        if let Some(b) = doc.binding_for_scene(id) {
            let s = select(doc, &values, &b.axes);
            enabled &= s.enabled;
            if s.enabled {
                order = 0.0;
                for k in 0..s.indices.len() {
                    order += b.keyforms[s.indices[k]].draw_order * s.weights[k];
                }
            }
        }
        let bs_list = doc.blend_bindings_for_target(id);
        if !bs_list.is_empty() {
            order = f32_to_i32(order + 0.001) as f32;
            for bs in bs_list {
                if let DeltaKeyforms::Part(ref forms) = bs.keyforms {
                    for (kf_idx, eff_w) in evaluate_blend_binding(doc, &values, bs) {
                        if kf_idx < forms.len() {
                            order += forms[kf_idx].draw_order * eff_w;
                        }
                    }
                }
            }
            order = f32_to_i32((order + 0.001).clamp(0.0, 1000.0)) as f32;
        }
        enabled_parts.insert(id.clone(), enabled);
        part_orders.insert(id.clone(), f32_to_i32(order + 0.001));
    }

    let mut transforms: HashMap<String, TransformState> = HashMap::new();
    for id in &doc.sorted_transforms() {
        let t = doc.get_transform(id).unwrap();
        let mut state = TransformState {
            source: t,
            pose: t.rotation,
            points: Vec::new(),
            appearance: t.appearance,
            inherited_scale: 1.0,
            enabled: t.enabled && (t.part_id.is_empty() || enabled_parts[&t.part_id]),
        };
        let mut points = t.points.clone();
        let b = doc.binding_for_scene(id);
        let selection = b.map(|binding| select(doc, &values, &binding.axes));

        if let Some(ref sel) = selection {
            state.enabled &= sel.enabled;
        }
        if !t.parent_id.is_empty() {
            state.enabled &= transforms[&t.parent_id].enabled;
        }

        if state.enabled {
            if let Some(b_ref) = b {
                let sel = selection.as_ref().unwrap();
                state.appearance = blend_appearance(sel, |i| b_ref.keyforms[i].appearance);
                if t.kind == TransformKind::Rotation {
                    state.pose = RotationPose::default();
                    state.pose.scale = 0.0;
                    let first = b_ref.keyforms[sel.indices[0]].rotation;
                    state.pose.reflect_x = first.reflect_x;
                    state.pose.reflect_y = first.reflect_y;
                    for k in 0..sel.indices.len() {
                        let p = b_ref.keyforms[sel.indices[k]].rotation;
                        let origin = match to_parent_positions(doc, &t.parent_id, &[p.origin]) {
                            Ok(orig) => orig,
                            Err(s) => return s,
                        };
                        let w = sel.weights[k];
                        state.pose.origin.x += origin[0].x * w;
                        state.pose.origin.y += origin[0].y * w;
                        state.pose.angle += p.angle * w;
                        state.pose.scale += p.scale * w;
                    }
                }
            }
            if t.kind == TransformKind::Warp {
                let default_sel = Selection {
                    indices: vec![0],
                    weights: vec![1.0],
                    enabled: true,
                };
                let sel_ref = selection.as_ref().unwrap_or(&default_sel);
                let blended = blend_positions(doc, &t.parent_id, sel_ref, |i| {
                    if let Some(b_ref) = b {
                        b_ref.keyforms[i].positions.clone()
                    } else {
                        t.points.clone()
                    }
                });
                match blended {
                    Ok(pts) => points = pts,
                    Err(s) => return s,
                }
            } else if b.is_none() {
                let origin = match to_parent_positions(doc, &t.parent_id, &[t.rotation.origin]) {
                    Ok(orig) => orig,
                    Err(s) => return s,
                };
                state.pose.origin = origin[0];
            }

            let bs_list = doc.blend_bindings_for_target(id);
            if !bs_list.is_empty() {
                for bs in bs_list {
                    match (&bs.keyforms, t.kind) {
                        (DeltaKeyforms::Warp(ref forms), TransformKind::Warp) => {
                            let selection = evaluate_blend_binding(doc, &values, bs);
                            let has_multiply = selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen = selection.iter().all(|(i, _)| forms[*i].screen.is_some());
                            for (kf_idx, eff_w) in selection {
                                if kf_idx < forms.len() {
                                    let f = &forms[kf_idx];
                                    for (p, dp) in points.iter_mut().zip(&f.points) {
                                        if t.parent_id.is_empty() {
                                            let ppu = doc.canvas().pixels_per_unit;
                                            p.x += (dp.x / ppu) * eff_w;
                                            p.y += (-dp.y / ppu) * eff_w;
                                        } else {
                                            p.x += dp.x * eff_w;
                                            p.y += dp.y * eff_w;
                                        }
                                    }
                                    if let Some(d_op) = f.opacity {
                                        state.appearance.opacity += d_op * eff_w;
                                    }
                                    if let Some(d_mul) = f.multiply.filter(|_| has_multiply) {
                                        for c in 0..3 {
                                            state.appearance.multiply[c] += d_mul[c] * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for c in 0..3 {
                                            state.appearance.screen[c] += d_scr[c] * eff_w;
                                        }
                                    }
                                }
                            }
                        }
                        (DeltaKeyforms::Rotation(ref forms), TransformKind::Rotation) => {
                            let selection = evaluate_blend_binding(doc, &values, bs);
                            let has_multiply = selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen = selection.iter().all(|(i, _)| forms[*i].screen.is_some());
                            for (kf_idx, eff_w) in selection {
                                if kf_idx < forms.len() {
                                    let f = &forms[kf_idx];
                                    if let Some(d_orig) = f.origin {
                                        if t.parent_id.is_empty() {
                                            let ppu = doc.canvas().pixels_per_unit;
                                            state.pose.origin.x += (d_orig.x / ppu) * eff_w;
                                            state.pose.origin.y += (-d_orig.y / ppu) * eff_w;
                                        } else {
                                            state.pose.origin.x += d_orig.x * eff_w;
                                            state.pose.origin.y += d_orig.y * eff_w;
                                        }
                                    }
                                    if let Some(d_ang) = f.angle {
                                        state.pose.angle += d_ang * eff_w;
                                    }
                                    if let Some(d_scale) = f.scale {
                                        state.pose.scale += d_scale * eff_w;
                                    }
                                    if let Some(d_op) = f.opacity {
                                        state.appearance.opacity += d_op * eff_w;
                                    }
                                    if let Some(d_mul) = f.multiply.filter(|_| has_multiply) {
                                        for c in 0..3 {
                                            state.appearance.multiply[c] += d_mul[c] * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for c in 0..3 {
                                            state.appearance.screen[c] += d_scr[c] * eff_w;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if t.kind == TransformKind::Rotation {
                    state.pose.angle = state.pose.angle.clamp(-3600.0, 3600.0);
                    state.pose.scale = state.pose.scale.clamp(0.0001, 100.0);
                }
                state.appearance.opacity = state.appearance.opacity.clamp(0.0, 1.0);
                for c in 0..3 {
                    state.appearance.multiply[c] = state.appearance.multiply[c].clamp(0.0, 1.0);
                    state.appearance.screen[c] = state.appearance.screen[c].clamp(0.0, 1.0);
                }
            }

            state.inherited_scale = if t.kind == TransformKind::Rotation {
                state.pose.scale
            } else {
                1.0
            };

            if !t.parent_id.is_empty() {
                let parent = transforms.get(&t.parent_id).unwrap().clone();
                inherit_appearance(&mut state.appearance, &parent.appearance);
                if t.kind == TransformKind::Warp {
                    for p in &mut points {
                        let q = parent.point(PsmVec2::new(p.x, p.y));
                        *p = Vec2::new(q.x, q.y);
                    }
                    state.inherited_scale = parent.inherited_scale;
                } else {
                    let mut origin = PsmVec2::new(state.pose.origin.x, state.pose.origin.y);
                    let adj = rotation_parent_angle(
                        parent.source.kind == TransformKind::Rotation,
                        |pt| parent.point(pt),
                        &mut origin,
                    );
                    state.pose.angle += adj;
                    state.pose.origin = Vec2::new(origin.x, origin.y);
                    state.pose.scale *= parent.inherited_scale;
                    state.inherited_scale = state.pose.scale;
                }
            }

            state.points.reserve(points.len() * 2);
            for p in &points {
                state.points.push(p.x);
                state.points.push(p.y);
            }
            let s = validate_positions(&points);
            if !s.is_ok() {
                return Status::error(s.code, format!("{}.evaluated_points", id));
            }
        }
        transforms.insert(id.clone(), state);
    }

    for id in doc.mesh_order() {
        let mesh = doc.get_mesh(id).unwrap();
        let mut d = Drawable {
            id: id.clone(),
            runtime_id: mesh.runtime_id.clone(),
            texture_asset_id: mesh.texture_asset_id.clone(),
            blend_mode: mesh.blend_mode,
            double_sided: mesh.double_sided,
            inverted_mask: mesh.inverted_mask,
            masks: mesh.masks.clone(),
            ..Default::default()
        };

        let mut order = mesh.draw_order.unwrap_or(frame.drawables.len() as f32);
        let mut appearance = mesh.appearance;

        let slot = doc
            .asset_order()
            .iter()
            .position(|aid| aid == &mesh.texture_asset_id);
        match slot {
            Some(s) => d.texture_slot = s as i32,
            None => return Status::error("MISSING_ASSET", id),
        }

        d.visible = mesh.enabled
            && (mesh.part_id.is_empty() || enabled_parts[&mesh.part_id])
            && (mesh.deformer_id.is_empty() || transforms[&mesh.deformer_id].enabled);

        d.uvs.reserve(mesh.uvs.len());
        for uv in &mesh.uvs {
            d.uvs.push(Vec2::new(
                uv.x,
                if doc.canvas().flag & 1 == 0 {
                    uv.y
                } else {
                    1.0 - uv.y
                },
            ));
        }

        match doc.render_indices(id) {
            Ok(indices) => d.indices = indices,
            Err(s) => return s,
        }

        // Swap triangle indices winding for runtime
        for i in (0..d.indices.len()).step_by(3) {
            d.indices.swap(i + 1, i + 2);
            if doc.canvas().flag & 1 == 0 {
                d.indices.swap(i, i + 2);
            }
        }

        let b = doc.binding_for_mesh(id);
        let selection = b.map(|binding| select(doc, &values, &binding.axes));
        if let Some(ref sel) = selection {
            d.visible &= sel.enabled;
        }
        d.enabled = d.visible;

        if d.visible {
            let default_sel = Selection {
                indices: vec![0],
                weights: vec![1.0],
                enabled: true,
            };
            let sel_ref = selection.as_ref().unwrap_or(&default_sel);
            let blended = blend_positions(doc, &mesh.deformer_id, sel_ref, |i| {
                if let Some(b_ref) = b {
                    b_ref.keyforms[i].positions.clone()
                } else {
                    mesh.base_positions.clone()
                }
            });
            match blended {
                Ok(pos) => d.positions = pos,
                Err(e) => return Status::error(e.code, format!("{}: {}", id, e.message)),
            }

            if let Some(b_ref) = b {
                appearance = blend_appearance(sel_ref, |i| b_ref.keyforms[i].appearance);
                let mut sum = 0.0f32;
                for k in 0..sel_ref.indices.len() {
                    sum += b_ref.keyforms[sel_ref.indices[k]]
                        .draw_order
                        .unwrap_or(order)
                        * sel_ref.weights[k];
                }
                order = sum;
            }

            let bs_list = doc.blend_bindings_for_target(id);
            if !bs_list.is_empty() {
                order = f32_to_i32(order + 0.001) as f32;
                for bs in bs_list {
                    if let DeltaKeyforms::Mesh(ref forms) = bs.keyforms {
                        let selection = evaluate_blend_binding(doc, &values, bs);
                        let has_multiply = selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                        let has_screen = selection.iter().all(|(i, _)| forms[*i].screen.is_some());
                        for (kf_idx, eff_w) in selection {
                            if kf_idx < forms.len() {
                                let f = &forms[kf_idx];
                                for (p, dp) in d.positions.iter_mut().zip(&f.positions) {
                                    if mesh.deformer_id.is_empty() {
                                        let ppu = doc.canvas().pixels_per_unit;
                                        p.x += (dp.x / ppu) * eff_w;
                                        p.y += (-dp.y / ppu) * eff_w;
                                    } else {
                                        p.x += dp.x * eff_w;
                                        p.y += dp.y * eff_w;
                                    }
                                }
                                if let Some(d_do) = f.draw_order {
                                    order += d_do * eff_w;
                                }
                                if let Some(d_op) = f.opacity {
                                    appearance.opacity += d_op * eff_w;
                                }
                                if let Some(d_mul) = f.multiply.filter(|_| has_multiply) {
                                    for c in 0..3 {
                                        appearance.multiply[c] += d_mul[c] * eff_w;
                                    }
                                }
                                if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                    for c in 0..3 {
                                        appearance.screen[c] += d_scr[c] * eff_w;
                                    }
                                }
                            }
                        }
                    }
                }

                order = f32_to_i32((order + 0.001).clamp(0.0, 1000.0)) as f32;
                appearance.opacity = appearance.opacity.clamp(0.0, 1.0);
                for c in 0..3 {
                    appearance.multiply[c] = appearance.multiply[c].clamp(0.0, 1.0);
                    appearance.screen[c] = appearance.screen[c].clamp(0.0, 1.0);
                }
            }

            if !mesh.deformer_id.is_empty() {
                let parent = transforms.get(&mesh.deformer_id).unwrap();
                inherit_appearance(&mut appearance, &parent.appearance);
                for p in &mut d.positions {
                    let q = parent.point(PsmVec2::new(p.x, p.y));
                    *p = Vec2::new(q.x, q.y);
                }
            }
        } else {
            d.positions.resize(mesh.vertex_ids.len(), Vec2::default());
            appearance = Appearance::default();
            order = 0.0;
        }

        d.draw_order = f32_to_i32(order + 0.001);
        d.opacity = appearance.opacity;
        d.visible &= d.opacity != 0.0;
        for c in 0..3 {
            d.multiply_color[c] = appearance.multiply[c];
            d.screen_color[c] = appearance.screen[c];
        }
        frame.drawables.push(d);
    }

    // Apply Glues across transformed mesh positions (before canvas Y-reversal, matching PurismCore)
    let glue_order = doc.glue_order();
    if !glue_order.is_empty() {
        let mesh_slots: HashMap<&str, usize> = doc
            .mesh_order()
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();

        for gid in glue_order {
            if let Some(glue) = doc.get_glue(gid) {
                if glue.intensity == 0.0 {
                    continue;
                }
                let slot_a = match mesh_slots.get(glue.mesh_a_id.as_str()) {
                    Some(&s) => s,
                    None => continue,
                };
                let slot_b = match mesh_slots.get(glue.mesh_b_id.as_str()) {
                    Some(&s) => s,
                    None => continue,
                };

                let mesh_a = doc.get_mesh(&glue.mesh_a_id).unwrap();
                let mesh_b = doc.get_mesh(&glue.mesh_b_id).unwrap();

                for pair in &glue.pairs {
                    let idx_a = match find_vertex_index(mesh_a, pair.vertex_a) {
                        Some(idx) => idx,
                        None => continue,
                    };
                    let idx_b = match find_vertex_index(mesh_b, pair.vertex_b) {
                        Some(idx) => idx,
                        None => continue,
                    };

                    let len_a = frame.drawables[slot_a].positions.len();
                    let len_b = frame.drawables[slot_b].positions.len();
                    if idx_a >= len_a || idx_b >= len_b {
                        continue;
                    }

                    if slot_a == slot_b {
                        let p0 = frame.drawables[slot_a].positions[idx_a];
                        let p1 = frame.drawables[slot_a].positions[idx_b];
                        let d = Vec2::new(p1.x - p0.x, p1.y - p0.y);
                        frame.drawables[slot_a].positions[idx_a].x += d.x * (glue.intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_a].y += d.y * (glue.intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_b].x -= d.x * (glue.intensity * pair.weight_b);
                        frame.drawables[slot_a].positions[idx_b].y -= d.y * (glue.intensity * pair.weight_b);
                    } else {
                        let p0 = frame.drawables[slot_a].positions[idx_a];
                        let p1 = frame.drawables[slot_b].positions[idx_b];
                        let d = Vec2::new(p1.x - p0.x, p1.y - p0.y);
                        frame.drawables[slot_a].positions[idx_a].x += d.x * (glue.intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_a].y += d.y * (glue.intensity * pair.weight_a);
                        frame.drawables[slot_b].positions[idx_b].x -= d.x * (glue.intensity * pair.weight_b);
                        frame.drawables[slot_b].positions[idx_b].y -= d.y * (glue.intensity * pair.weight_b);
                    }
                }
            }
        }
    }

    // Apply canvas Y reversal and validate positions
    for d in &mut frame.drawables {
        if d.enabled {
            if doc.canvas().flag & 1 == 0 {
                for p in &mut d.positions {
                    p.y = -p.y;
                }
            }
            let s = validate_positions(&d.positions);
            if !s.is_ok() {
                return Status::error(s.code, format!("{}.evaluated_positions", d.id));
            }
        }
    }

    let groups = crate::draw_order::resolved_groups(doc);
    let totals = crate::draw_order::descendant_counts(&groups);
    let slots: HashMap<&str, usize> = frame
        .drawables
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();
    let mut orders = HashMap::new();
    for group in &groups {
        let mut items: Vec<(&str, i32)> = group
            .items
            .iter()
            .map(|id| {
                let (order, enabled) = if let Some(&slot) = slots.get(id.as_str()) {
                    let d = &frame.drawables[slot];
                    (d.draw_order, d.enabled)
                } else {
                    (part_orders[id], enabled_parts[id])
                };
                (
                    id.as_str(),
                    if enabled {
                        order.clamp(group.min_order, group.max_order)
                    } else {
                        group.min_order
                    },
                )
            })
            .collect();
        items.sort_by_key(|item| item.1);
        let mut rank = orders.get(group.owner.as_str()).copied().unwrap_or(0);
        for (id, _) in items {
            orders.insert(id, rank);
            rank += totals.get(id).copied().unwrap_or(1) as i32;
        }
    }
    for d in &mut frame.drawables {
        d.render_order = orders[d.id.as_str()];
    }

    *out = frame;
    Status::ok()
}
