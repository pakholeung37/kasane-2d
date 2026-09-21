use std::collections::HashMap;

use crate::deformers::{rotation_parent_angle, rotation_points, warp_points, PsmVec2};
use crate::document::Document;
use crate::geometry::{to_runtime_positions, validate_positions};
use crate::keyforms::{find_key_segment, key_combinations, KeyAxis};
use crate::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint, Canvas,
    DeltaKeyforms, Mesh, RotationPose, Status, Transform, TransformKind, Vec2, VertexId,
};

pub type PreviewValues = HashMap<String, f32>;

#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    pub id: String,
    pub runtime_id: String,
    pub part_id: String,
    pub raw_blend_mode: Option<u32>,
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
            part_id: String::new(),
            raw_blend_mode: None,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RenderCommand {
    BeginOffscreen { offscreen_id: String },
    DrawMesh { mesh_id: String },
    EndOffscreen { offscreen_id: String },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OffscreenFrame {
    pub id: String,
    pub runtime_id: String,
    pub owner_part_id: String,
    pub parent_offscreen_id: Option<String>,
    pub render_order: i32,
    pub opacity: f32,
    pub enabled: bool,
    pub blend_mode: u32,
    pub flags: u8,
    pub masks: Vec<String>,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawableFrame {
    pub source_revision: u64,
    pub canvas: Canvas,
    pub parameters: Vec<EvaluatedParameter>,
    pub drawables: Vec<Drawable>,
    pub offscreens: Vec<OffscreenFrame>,
    pub render_plan: Vec<RenderCommand>,
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

pub fn to_parent_origin(
    doc: &Document,
    parent: &str,
    origin: crate::types::PreciseVec2,
) -> Result<Vec2, Status> {
    let canvas = doc.canvas();
    let p = if parent.is_empty() {
        Vec2::new(
            ((origin.x - canvas.origin.x as f64) / canvas.pixels_per_unit as f64) as f32,
            ((canvas.origin.y as f64 - origin.y) / canvas.pixels_per_unit as f64) as f32,
        )
    } else {
        Vec2::new(origin.x as f32, origin.y as f32)
    };
    let status = validate_positions(&[p]);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(p)
}

#[derive(Debug, Clone)]
struct RuntimeRotationPose {
    origin: Vec2,
    angle: f32,
    scale: f32,
    reflect_x: bool,
    reflect_y: bool,
}
impl From<RotationPose> for RuntimeRotationPose {
    fn from(p: RotationPose) -> Self {
        Self {
            origin: Vec2::new(p.origin.x as f32, p.origin.y as f32),
            angle: p.angle,
            scale: p.scale,
            reflect_x: p.reflect_x,
            reflect_y: p.reflect_y,
        }
    }
}
impl Default for RuntimeRotationPose {
    fn default() -> Self {
        RotationPose::default().into()
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

fn default_selection() -> &'static Selection {
    static SELECTION: std::sync::OnceLock<Selection> = std::sync::OnceLock::new();
    SELECTION.get_or_init(|| Selection {
        indices: vec![0],
        weights: vec![1.0],
        enabled: true,
    })
}

fn blend_positions<'a, F>(
    doc: &Document,
    parent: &str,
    s: &Selection,
    mut get: F,
    out: &mut Vec<Vec2>,
) -> Result<(), Status>
where
    F: FnMut(usize) -> &'a [Vec2],
{
    if !parent.is_empty() && doc.get_transform(parent).is_none() {
        return Err(Status::error("MISSING_TRANSFORM", parent));
    }
    let size = s.indices.first().map(|&i| get(i).len()).unwrap_or(0);
    if size > (i32::MAX as usize) / 2 {
        return Err(Status::error("CAPACITY", "positions"));
    }
    out.clear();
    out.resize(size, Vec2::default());
    let canvas = doc.canvas();
    for (&index, &weight) in s.indices.iter().zip(&s.weights) {
        let raw = get(index);
        if raw.len() != size {
            return Err(Status::error("INVALID_LENGTH", "keyform positions"));
        }
        for (target, p) in out.iter_mut().zip(raw) {
            let q = if parent.is_empty() {
                Vec2::new(
                    ((p.x as f64 - canvas.origin.x as f64) / canvas.pixels_per_unit as f64) as f32,
                    ((canvas.origin.y as f64 - p.y as f64) / canvas.pixels_per_unit as f64) as f32,
                )
            } else {
                *p
            };
            if !q.x.is_finite() || !q.y.is_finite() {
                return Err(Status::error(
                    "NON_FINITE",
                    "Position conversion overflows float32",
                ));
            }
            if weight != 0.0 {
                target.x += q.x * weight;
                target.y += q.y * weight;
            }
        }
    }
    let status = validate_positions(out);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TransformState<'a> {
    source: &'a Transform,
    pose: RuntimeRotationPose,
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

/// Reusable, transactional evaluator. Retain both this workspace and the output
/// frame across updates. Failure leaves the last successful output untouched.
#[derive(Debug, Default)]
pub struct FrameEvaluator {
    scratch: DrawableFrame,
}

impl FrameEvaluator {
    pub fn evaluate(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
    ) -> Status {
        let status = evaluate_into(doc, preview, &mut self.scratch);
        if status.is_ok() {
            std::mem::swap(out, &mut self.scratch);
        }
        status
    }
}

pub fn evaluate_frame(doc: &Document, preview: &PreviewValues, out: &mut DrawableFrame) -> Status {
    FrameEvaluator::default().evaluate(doc, preview, out)
}

fn evaluate_into(doc: &Document, preview: &PreviewValues, frame: &mut DrawableFrame) -> Status {
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

    frame.source_revision = doc.revision();
    frame.canvas = doc.canvas();
    frame.parameters.clear();
    frame.offscreens.clear();
    frame.render_plan.clear();
    frame
        .drawables
        .resize_with(doc.mesh_order().len(), Drawable::default);

    let mut values = HashMap::new();
    for id in doc.parameter_order() {
        let p = doc.get_parameter(id).unwrap();
        let requested = preview.get(id).copied().unwrap_or(p.default_value);
        if !requested.is_finite() {
            return Status::error("NON_FINITE", format!("{}.preview_value", id));
        }
        let range_length = p.maximum - p.minimum;
        if !range_length.is_finite() || range_length <= 0.0 {
            return Status::error(
                "INVALID_PARAMETER_RANGE",
                format!("{}: range length must be positive", id),
            );
        }
        let (v, clamped) = if p.repeat {
            let normalized = (requested - p.minimum) / range_length;
            let wrapped = if normalized.is_finite() {
                normalized - normalized.floor()
            } else {
                // Finite f32 inputs can overflow during subtraction or division.
                (((requested as f64 - p.minimum as f64) % range_length as f64)
                    .rem_euclid(range_length as f64)
                    / range_length as f64) as f32
            };
            let mut val = wrapped * range_length + p.minimum;
            if val < p.minimum || val >= p.maximum {
                val = p.minimum;
            }
            (val, false)
        } else {
            let clamped_val = requested.clamp(p.minimum, p.maximum);
            (clamped_val, requested != clamped_val)
        };
        frame.parameters.push(EvaluatedParameter {
            id: id.clone(),
            requested,
            value: v,
            clamped,
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
            pose: t.rotation.into(),
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
                    state.pose = RuntimeRotationPose::default();
                    state.pose.scale = 0.0;
                    let first = b_ref.keyforms[sel.indices[0]].rotation;
                    state.pose.reflect_x = first.reflect_x;
                    state.pose.reflect_y = first.reflect_y;
                    for k in 0..sel.indices.len() {
                        let p = b_ref.keyforms[sel.indices[k]].rotation;
                        let origin = match to_parent_origin(doc, &t.parent_id, p.origin) {
                            Ok(orig) => orig,
                            Err(s) => return s,
                        };
                        let w = sel.weights[k];
                        state.pose.origin.x += origin.x * w;
                        state.pose.origin.y += origin.y * w;
                        state.pose.angle += p.angle * w;
                        state.pose.scale += p.scale * w;
                    }
                }
            }
            if t.kind == TransformKind::Warp {
                let sel_ref = selection.as_ref().unwrap_or(default_selection());
                let blended = blend_positions(
                    doc,
                    &t.parent_id,
                    sel_ref,
                    |i| {
                        if let Some(b_ref) = b {
                            &b_ref.keyforms[i].positions
                        } else {
                            &t.points
                        }
                    },
                    &mut points,
                );
                match blended {
                    Ok(()) => (),
                    Err(s) => return s,
                }
            } else if b.is_none() {
                let origin = match to_parent_origin(doc, &t.parent_id, t.rotation.origin) {
                    Ok(orig) => orig,
                    Err(s) => return s,
                };
                state.pose.origin = origin;
            }

            let bs_list = doc.blend_bindings_for_target(id);
            if !bs_list.is_empty() {
                for bs in bs_list {
                    match (&bs.keyforms, t.kind) {
                        (DeltaKeyforms::Warp(ref forms), TransformKind::Warp) => {
                            let selection = evaluate_blend_binding(doc, &values, bs);
                            let has_multiply =
                                selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen =
                                selection.iter().all(|(i, _)| forms[*i].screen.is_some());
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
                                        for (channel, delta) in
                                            state.appearance.multiply.iter_mut().zip(d_mul)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for (channel, delta) in
                                            state.appearance.screen.iter_mut().zip(d_scr)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                }
                            }
                        }
                        (DeltaKeyforms::Rotation(ref forms), TransformKind::Rotation) => {
                            let selection = evaluate_blend_binding(doc, &values, bs);
                            let has_multiply =
                                selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen =
                                selection.iter().all(|(i, _)| forms[*i].screen.is_some());
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
                                        for (channel, delta) in
                                            state.appearance.multiply.iter_mut().zip(d_mul)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for (channel, delta) in
                                            state.appearance.screen.iter_mut().zip(d_scr)
                                        {
                                            *channel += delta * eff_w;
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
                let parent = transforms.get(&t.parent_id).unwrap();
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

    let asset_slots: HashMap<&str, usize> = doc
        .asset_order()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for (mesh_index, id) in doc.mesh_order().iter().enumerate() {
        let mesh = doc.get_mesh(id).unwrap();
        let d = &mut frame.drawables[mesh_index];
        d.id.clone_from(id);
        d.runtime_id.clone_from(&mesh.runtime_id);
        d.part_id.clone_from(&mesh.part_id);
        d.raw_blend_mode = mesh.raw_blend_mode;
        d.texture_asset_id.clone_from(&mesh.texture_asset_id);
        d.blend_mode = mesh.blend_mode;
        d.double_sided = mesh.double_sided;
        d.inverted_mask = mesh.inverted_mask;
        d.masks.clone_from(&mesh.masks);
        d.multiply_color[3] = 1.0;
        d.screen_color[3] = 1.0;
        d.positions.clear();
        d.uvs.clear();
        d.indices.clear();
        let mut order = mesh.draw_order.unwrap_or(mesh_index as f32);
        let mut appearance = mesh.appearance;

        let slot = asset_slots.get(mesh.texture_asset_id.as_str()).copied();
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

        match doc.render_indices_into(id, &mut d.indices) {
            Ok(()) => (),
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
            let sel_ref = selection.as_ref().unwrap_or(default_selection());
            let blended = blend_positions(
                doc,
                &mesh.deformer_id,
                sel_ref,
                |i| {
                    if let Some(b_ref) = b {
                        &b_ref.keyforms[i].positions
                    } else {
                        &mesh.base_positions
                    }
                },
                &mut d.positions,
            );
            match blended {
                Ok(()) => (),
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
                        let has_multiply =
                            selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
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
                                    for (channel, delta) in
                                        appearance.multiply.iter_mut().zip(d_mul)
                                    {
                                        *channel += delta * eff_w;
                                    }
                                }
                                if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                    for (channel, delta) in appearance.screen.iter_mut().zip(d_scr)
                                    {
                                        *channel += delta * eff_w;
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
                let mut intensity = if let Some(binding) = &glue.binding {
                    let selection = select(doc, &values, &binding.axes);
                    selection
                        .indices
                        .iter()
                        .zip(&selection.weights)
                        .map(|(&i, &w)| binding.keyforms[i].intensity * w)
                        .sum()
                } else {
                    glue.intensity
                };

                let bs_list = doc.blend_bindings_for_target(gid);
                if !bs_list.is_empty() {
                    for bs in bs_list {
                        if let DeltaKeyforms::Glue(ref forms) = bs.keyforms {
                            for (kf_idx, eff_w) in evaluate_blend_binding(doc, &values, bs) {
                                if kf_idx < forms.len() {
                                    intensity += forms[kf_idx].intensity * eff_w;
                                }
                            }
                        }
                    }
                    intensity = intensity.clamp(0.0, 1.0);
                }

                if intensity <= 0.0 {
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
                        frame.drawables[slot_a].positions[idx_a].x +=
                            d.x * (intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_a].y +=
                            d.y * (intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_b].x -=
                            d.x * (intensity * pair.weight_b);
                        frame.drawables[slot_a].positions[idx_b].y -=
                            d.y * (intensity * pair.weight_b);
                    } else {
                        let p0 = frame.drawables[slot_a].positions[idx_a];
                        let p1 = frame.drawables[slot_b].positions[idx_b];
                        let d = Vec2::new(p1.x - p0.x, p1.y - p0.y);
                        frame.drawables[slot_a].positions[idx_a].x +=
                            d.x * (intensity * pair.weight_a);
                        frame.drawables[slot_a].positions[idx_a].y +=
                            d.y * (intensity * pair.weight_a);
                        frame.drawables[slot_b].positions[idx_b].x -=
                            d.x * (intensity * pair.weight_b);
                        frame.drawables[slot_b].positions[idx_b].y -=
                            d.y * (intensity * pair.weight_b);
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
    let totals = crate::draw_order::descendant_counts_with_offscreens(doc, &groups);
    let slots: HashMap<&str, usize> = frame
        .drawables
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();
    let mut orders = HashMap::new();
    let mut offscreen_orders = HashMap::new();
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
            if let Some(os) = doc.offscreen_for_part(id) {
                offscreen_orders.insert(os.id.as_str(), rank);
                rank += 1;
            }
            orders.insert(id, rank);
            rank += totals.get(id).copied().unwrap_or(1) as i32;
        }
    }
    for d in &mut frame.drawables {
        d.render_order = orders[d.id.as_str()];
    }

    for os_id in doc.offscreen_order() {
        if let Some(os) = doc.get_offscreen(os_id) {
            let part_id = &os.part_id;
            let owner_enabled = enabled_parts.get(part_id).copied().unwrap_or(false);
            let mut opacity = 0.0f32;
            let mut mul_color = [1.0f32, 1.0, 1.0, 1.0];
            let mut scr_color = [0.0f32, 0.0, 0.0, 1.0];

            if owner_enabled {
                if let Some(b) = doc.binding_for_scene(part_id) {
                    let s = select(doc, &values, &b.axes);
                    let mut interp_opa = 0.0f32;
                    let mut interp_mul = [0.0f32; 3];
                    let mut interp_scr = [0.0f32; 3];
                    let mut has_color = false;
                    for k in 0..s.indices.len() {
                        let kf_idx = s.indices[k];
                        let w = s.weights[k];
                        let index = match os.keyform_index(kf_idx, Some(b.keyforms.len())) {
                            Ok(index) => index,
                            Err(status) => return status,
                        };
                        if let Some(index) = index {
                            let kf = &os.keyforms[index];
                            interp_opa += kf.opacity * w;
                            if let Some(m) = kf.multiply {
                                interp_mul[0] += m[0] * w;
                                interp_mul[1] += m[1] * w;
                                interp_mul[2] += m[2] * w;
                                has_color = true;
                            } else {
                                interp_mul[0] += 1.0 * w;
                                interp_mul[1] += 1.0 * w;
                                interp_mul[2] += 1.0 * w;
                            }
                            if let Some(scr) = kf.screen {
                                interp_scr[0] += scr[0] * w;
                                interp_scr[1] += scr[1] * w;
                                interp_scr[2] += scr[2] * w;
                                has_color = true;
                            }
                        } else {
                            interp_opa += 1.0 * w;
                            interp_mul[0] += 1.0 * w;
                            interp_mul[1] += 1.0 * w;
                            interp_mul[2] += 1.0 * w;
                        }
                    }
                    opacity = interp_opa;
                    if has_color {
                        mul_color = [interp_mul[0], interp_mul[1], interp_mul[2], 1.0];
                        scr_color = [interp_scr[0], interp_scr[1], interp_scr[2], 1.0];
                    }
                } else {
                    let index = match os.keyform_index(0, None) {
                        Ok(index) => index,
                        Err(status) => return status,
                    };
                    opacity = 1.0;
                    if let Some(index) = index {
                        let kf = &os.keyforms[index];
                        opacity = kf.opacity;
                        if let Some(m) = kf.multiply {
                            mul_color = [m[0], m[1], m[2], 1.0];
                        }
                        if let Some(scr) = kf.screen {
                            scr_color = [scr[0], scr[1], scr[2], 1.0];
                        }
                    }
                }

                // Apply blendshapes
                for bs in doc.blend_bindings_for_target(os_id) {
                    if let DeltaKeyforms::Offscreen(ref forms) = bs.keyforms {
                        for (kf_idx, eff_w) in evaluate_blend_binding(doc, &values, bs) {
                            if kf_idx < forms.len() {
                                let df = &forms[kf_idx];
                                opacity += df.opacity * eff_w;
                                if let Some(m) = df.multiply {
                                    mul_color[0] += (m[0] - 1.0) * eff_w;
                                    mul_color[1] += (m[1] - 1.0) * eff_w;
                                    mul_color[2] += (m[2] - 1.0) * eff_w;
                                }
                                if let Some(scr) = df.screen {
                                    scr_color[0] += scr[0] * eff_w;
                                    scr_color[1] += scr[1] * eff_w;
                                    scr_color[2] += scr[2] * eff_w;
                                }
                            }
                        }
                    }
                }
                opacity = opacity.clamp(0.0, 1.0);
            }

            let ro = offscreen_orders.get(os.id.as_str()).copied().unwrap_or(0);
            let parent_os_id = doc
                .parent_offscreen_for_part(&os.part_id)
                .map(|p| p.id.clone());

            frame.offscreens.push(OffscreenFrame {
                id: os.id.clone(),
                runtime_id: os.runtime_id.clone(),
                owner_part_id: os.part_id.clone(),
                parent_offscreen_id: parent_os_id,
                render_order: ro,
                opacity,
                enabled: owner_enabled,
                blend_mode: os.blend_mode,
                flags: os.flags,
                masks: os.masks.clone(),
                multiply_color: mul_color,
                screen_color: scr_color,
            });
        }
    }

    // Build render_plan: sort all drawables and offscreens by render_order
    #[derive(Copy, Clone)]
    enum PlanItem<'a> {
        Mesh {
            id: &'a str,
            part_id: &'a str,
            render_order: i32,
        },
        Offscreen {
            id: &'a str,
            owner_part_id: &'a str,
            render_order: i32,
        },
    }

    let mut plan_items: Vec<PlanItem> =
        Vec::with_capacity(frame.drawables.len() + frame.offscreens.len());
    for d in &frame.drawables {
        plan_items.push(PlanItem::Mesh {
            id: d.id.as_str(),
            part_id: d.part_id.as_str(),
            render_order: d.render_order,
        });
    }
    for os in &frame.offscreens {
        plan_items.push(PlanItem::Offscreen {
            id: os.id.as_str(),
            owner_part_id: os.owner_part_id.as_str(),
            render_order: os.render_order,
        });
    }
    plan_items.sort_by_key(|item| match item {
        PlanItem::Mesh { render_order, .. } => *render_order,
        PlanItem::Offscreen { render_order, .. } => *render_order,
    });

    let mut active_offscreens: Vec<(&str, &str)> = Vec::new(); // (offscreen_id, owner_part_id)
    for item in plan_items {
        match item {
            PlanItem::Offscreen {
                id, owner_part_id, ..
            } => {
                while let Some(&(top_os, top_owner)) = active_offscreens.last() {
                    if doc.is_part_ancestor(top_owner, owner_part_id) {
                        break;
                    }
                    active_offscreens.pop();
                    frame.render_plan.push(RenderCommand::EndOffscreen {
                        offscreen_id: top_os.to_string(),
                    });
                }
                active_offscreens.push((id, owner_part_id));
                frame.render_plan.push(RenderCommand::BeginOffscreen {
                    offscreen_id: id.to_string(),
                });
            }
            PlanItem::Mesh { id, part_id, .. } => {
                while let Some(&(top_os, top_owner)) = active_offscreens.last() {
                    if doc.is_part_ancestor(top_owner, part_id) {
                        break;
                    }
                    active_offscreens.pop();
                    frame.render_plan.push(RenderCommand::EndOffscreen {
                        offscreen_id: top_os.to_string(),
                    });
                }
                frame.render_plan.push(RenderCommand::DrawMesh {
                    mesh_id: id.to_string(),
                });
            }
        }
    }
    while let Some((top_os, _)) = active_offscreens.pop() {
        frame.render_plan.push(RenderCommand::EndOffscreen {
            offscreen_id: top_os.to_string(),
        });
    }

    Status::ok()
}
