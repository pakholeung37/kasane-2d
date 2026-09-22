use crate::deformers::{rotation_parent_angle, rotation_points, warp_points, PsmVec2};
use crate::document::Document;
use crate::geometry::validate_positions;
use crate::types::{Appearance, DeltaKeyforms, Status, Transform, TransformKind, Vec2};

use super::evaluator::EvalContext;
use super::prepared::PreparedEvaluation;
use super::selection::{
    blend_appearance, blend_positions, default_selection, evaluate_blend_binding,
    inherit_appearance, select, RuntimeRotationPose,
};
use super::types::to_parent_origin;

#[derive(Debug, Clone, Default)]
pub(super) struct TransformSource {
    kind: TransformKind,
    rows: usize,
    columns: usize,
    quad: bool,
    base_angle: f32,
}
impl From<&Transform> for TransformSource {
    fn from(t: &Transform) -> Self {
        Self {
            kind: t.kind(),
            rows: t.warp().map_or(0, |w| w.rows as usize),
            columns: t.warp().map_or(0, |w| w.columns as usize),
            quad: t.warp().is_some_and(|w| w.quad),
            base_angle: t.rotation().map_or(0.0, |r| r.base_angle),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct TransformState {
    pub(super) source: TransformSource,
    pub(super) pose: RuntimeRotationPose,
    pub(super) points: Vec<f32>,
    pub(super) appearance: Appearance,
    pub(super) inherited_scale: f32,
    pub(super) enabled: bool,
}

impl TransformState {
    pub(super) fn point(&self, p: PsmVec2) -> PsmVec2 {
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

pub(super) fn f32_to_i32(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        v.clamp(-2147483648.0, 2147483520.0) as i32
    }
}

pub(super) fn evaluate(
    doc: &Document,
    prepared: &PreparedEvaluation,
    state: &mut EvalContext<'_>,
) -> crate::types::Status {
    state
        .transforms
        .resize_with(prepared.transforms.len(), TransformState::default);
    for (transform_slot, id) in prepared.transforms.iter().enumerate() {
        let t = doc.get_transform(id).unwrap();
        let mut transform_state = std::mem::take(&mut state.transforms[transform_slot]);
        transform_state.points.clear();
        transform_state = TransformState {
            source: t.into(),
            pose: t.rotation().map(|r| r.pose).unwrap_or_default().into(),
            points: transform_state.points,
            appearance: t.appearance,
            inherited_scale: 1.0,
            enabled: t.enabled && (t.part_id.is_none() || state.enabled_parts[t.part()]),
        };
        state.points.clear();
        if let Some(w) = t.warp() {
            state.points.extend_from_slice(&w.points);
        }
        let b = doc.binding_for_scene(id);
        let selection = b.map(|binding| select(doc, state.values, &binding.axes, state.selection));

        if let Some(sel) = selection {
            transform_state.enabled &= sel.enabled;
        }
        if !t.parent_id.is_none() {
            transform_state.enabled &=
                state.transforms[prepared.transform_slots[t.parent()]].enabled;
        }

        if transform_state.enabled {
            if let Some(b_ref) = b {
                let sel = selection.as_ref().unwrap();
                transform_state.appearance =
                    blend_appearance(sel, |i| b_ref.track.sample(i).appearance);
                if t.kind() == TransformKind::Rotation {
                    transform_state.pose = RuntimeRotationPose::default();
                    transform_state.pose.scale = 0.0;
                    let first = b_ref.track.sample(sel.indices[0]).rotation;
                    transform_state.pose.reflect_x = first.reflect_x;
                    transform_state.pose.reflect_y = first.reflect_y;
                    for k in 0..sel.indices.len() {
                        let p = b_ref.track.sample(sel.indices[k]).rotation;
                        let origin = match to_parent_origin(doc, t.parent(), p.origin) {
                            Ok(orig) => orig,
                            Err(s) => return s,
                        };
                        let w = sel.weights[k];
                        transform_state.pose.origin.x += origin.x * w;
                        transform_state.pose.origin.y += origin.y * w;
                        transform_state.pose.angle += p.angle * w;
                        transform_state.pose.scale += p.scale * w;
                    }
                }
            }
            if t.kind() == TransformKind::Warp {
                let sel_ref = selection.unwrap_or(default_selection());
                let blended = blend_positions(
                    doc,
                    t.parent(),
                    sel_ref,
                    |i| {
                        if let Some(b_ref) = b {
                            b_ref.track.sample(i).positions
                        } else {
                            &t.warp().unwrap().points
                        }
                    },
                    state.points,
                );
                match blended {
                    Ok(()) => (),
                    Err(s) => return s,
                }
            } else if b.is_none() {
                let origin =
                    match to_parent_origin(doc, t.parent(), t.rotation().unwrap().pose.origin) {
                        Ok(orig) => orig,
                        Err(s) => return s,
                    };
                transform_state.pose.origin = origin;
            }

            let bs_list = doc.blend_bindings_for_target(id);
            if !bs_list.is_empty() {
                for bs in bs_list {
                    match (&bs.keyforms, t.kind()) {
                        (DeltaKeyforms::Warp(ref forms), TransformKind::Warp) => {
                            let selection = evaluate_blend_binding(doc, state.values, bs);
                            let has_multiply =
                                selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen =
                                selection.iter().all(|(i, _)| forms[*i].screen.is_some());
                            for (kf_idx, eff_w) in selection {
                                if kf_idx < forms.len() {
                                    let f = &forms[kf_idx];
                                    for (p, dp) in state.points.iter_mut().zip(&f.points) {
                                        if t.parent_id.is_none() {
                                            let ppu = doc.canvas().pixels_per_unit;
                                            p.x += (dp.x / ppu) * eff_w;
                                            p.y += (-dp.y / ppu) * eff_w;
                                        } else {
                                            p.x += dp.x * eff_w;
                                            p.y += dp.y * eff_w;
                                        }
                                    }
                                    if let Some(d_op) = f.opacity {
                                        transform_state.appearance.opacity += d_op * eff_w;
                                    }
                                    if let Some(d_mul) = f.multiply.filter(|_| has_multiply) {
                                        for (channel, delta) in transform_state
                                            .appearance
                                            .multiply
                                            .iter_mut()
                                            .zip(d_mul)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for (channel, delta) in
                                            transform_state.appearance.screen.iter_mut().zip(d_scr)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                }
                            }
                        }
                        (DeltaKeyforms::Rotation(ref forms), TransformKind::Rotation) => {
                            let selection = evaluate_blend_binding(doc, state.values, bs);
                            let has_multiply =
                                selection.iter().all(|(i, _)| forms[*i].multiply.is_some());
                            let has_screen =
                                selection.iter().all(|(i, _)| forms[*i].screen.is_some());
                            for (kf_idx, eff_w) in selection {
                                if kf_idx < forms.len() {
                                    let f = &forms[kf_idx];
                                    if let Some(d_orig) = f.origin {
                                        if t.parent_id.is_none() {
                                            let ppu = doc.canvas().pixels_per_unit;
                                            transform_state.pose.origin.x +=
                                                (d_orig.x / ppu) * eff_w;
                                            transform_state.pose.origin.y +=
                                                (-d_orig.y / ppu) * eff_w;
                                        } else {
                                            transform_state.pose.origin.x += d_orig.x * eff_w;
                                            transform_state.pose.origin.y += d_orig.y * eff_w;
                                        }
                                    }
                                    if let Some(d_ang) = f.angle {
                                        transform_state.pose.angle += d_ang * eff_w;
                                    }
                                    if let Some(d_scale) = f.scale {
                                        transform_state.pose.scale += d_scale * eff_w;
                                    }
                                    if let Some(d_op) = f.opacity {
                                        transform_state.appearance.opacity += d_op * eff_w;
                                    }
                                    if let Some(d_mul) = f.multiply.filter(|_| has_multiply) {
                                        for (channel, delta) in transform_state
                                            .appearance
                                            .multiply
                                            .iter_mut()
                                            .zip(d_mul)
                                        {
                                            *channel += delta * eff_w;
                                        }
                                    }
                                    if let Some(d_scr) = f.screen.filter(|_| has_screen) {
                                        for (channel, delta) in
                                            transform_state.appearance.screen.iter_mut().zip(d_scr)
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

                if t.kind() == TransformKind::Rotation {
                    transform_state.pose.angle = transform_state.pose.angle.clamp(-3600.0, 3600.0);
                    transform_state.pose.scale = transform_state.pose.scale.clamp(0.0001, 100.0);
                }
                transform_state.appearance.opacity =
                    transform_state.appearance.opacity.clamp(0.0, 1.0);
                for c in 0..3 {
                    transform_state.appearance.multiply[c] =
                        transform_state.appearance.multiply[c].clamp(0.0, 1.0);
                    transform_state.appearance.screen[c] =
                        transform_state.appearance.screen[c].clamp(0.0, 1.0);
                }
            }

            transform_state.inherited_scale = if t.kind() == TransformKind::Rotation {
                transform_state.pose.scale
            } else {
                1.0
            };

            if !t.parent_id.is_none() {
                let parent = &state.transforms[prepared.transform_slots[t.parent()]];
                inherit_appearance(&mut transform_state.appearance, &parent.appearance);
                if t.kind() == TransformKind::Warp {
                    for p in state.points.iter_mut() {
                        let q = parent.point(PsmVec2::new(p.x, p.y));
                        *p = Vec2::new(q.x, q.y);
                    }
                    transform_state.inherited_scale = parent.inherited_scale;
                } else {
                    let mut origin =
                        PsmVec2::new(transform_state.pose.origin.x, transform_state.pose.origin.y);
                    let adj = rotation_parent_angle(
                        parent.source.kind == TransformKind::Rotation,
                        |pt| parent.point(pt),
                        &mut origin,
                    );
                    transform_state.pose.angle += adj;
                    transform_state.pose.origin = Vec2::new(origin.x, origin.y);
                    transform_state.pose.scale *= parent.inherited_scale;
                    transform_state.inherited_scale = transform_state.pose.scale;
                }
            }

            transform_state.points.reserve(state.points.len() * 2);
            for p in state.points.iter() {
                transform_state.points.push(p.x);
                transform_state.points.push(p.y);
            }
            let s = validate_positions(state.points);
            if !s.is_ok() {
                return Status::error(s.code, format!("{}.evaluated_points", id));
            }
        }
        state.transforms[transform_slot] = transform_state;
    }
    Status::ok()
}
