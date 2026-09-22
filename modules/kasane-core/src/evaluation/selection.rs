use std::collections::HashMap;

use crate::document::Document;
use crate::geometry::validate_positions;
use crate::keyforms::find_key_segment;
use crate::types::{
    Appearance, BindingAxis, BlendShapeBinding, BlendShapeConstraint, RotationPose, Status, Vec2,
};

#[derive(Debug, Clone)]
pub(super) struct RuntimeRotationPose {
    pub(super) origin: Vec2,
    pub(super) angle: f32,
    pub(super) scale: f32,
    pub(super) reflect_x: bool,
    pub(super) reflect_y: bool,
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

#[derive(Debug, Clone, Default)]
pub(super) struct Selection {
    pub(super) indices: Vec<usize>,
    pub(super) weights: Vec<f32>,
    pub(super) enabled: bool,
}

pub(super) fn select<'a>(
    doc: &Document,
    values: &HashMap<String, f32>,
    binding: &[BindingAxis],
    out: &'a mut Selection,
) -> &'a Selection {
    out.indices.clear();
    out.weights.clear();
    out.indices.push(0);
    out.weights.push(1.0);
    out.enabled = true;
    let mut stride = 1;
    for axis in binding {
        let p = doc.get_parameter(&axis.parameter_id).unwrap();
        let epsilon = 0.1f32.powi(p.decimal_places);
        let segment = find_key_segment(values[&p.id], &axis.keys, epsilon, epsilon * 1.5);
        out.enabled &= !segment.is_outside;
        let count = out.indices.len();
        for i in 0..count {
            out.indices[i] += segment.index as usize * stride;
            if segment.weight != 0.0 {
                out.indices.push(out.indices[i] + stride);
                out.weights.push(out.weights[i] * segment.weight);
                out.weights[i] *= 1.0 - segment.weight;
            }
        }
        stride *= axis.keys.len();
    }
    out
}

pub(super) fn blend_appearance<F>(s: &Selection, mut get: F) -> Appearance
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

pub(super) fn inherit_appearance(child: &mut Appearance, parent: &Appearance) {
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

pub(super) fn default_selection() -> &'static Selection {
    static SELECTION: std::sync::OnceLock<Selection> = std::sync::OnceLock::new();
    SELECTION.get_or_init(|| Selection {
        indices: vec![0],
        weights: vec![1.0],
        enabled: true,
    })
}

pub(super) fn blend_positions<'a, F>(
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

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::keyforms::{key_combinations, KeyAxis};
    use crate::Parameter;

    #[test]
    fn sparse_selection_matches_cartesian_reference() {
        let mut doc = Document::new();
        doc.initialize(
            "00000000-0000-4000-8000-000000000001",
            crate::types::Canvas::new(100., 100., Vec2::default(), 1.),
        );
        let id = "00000000-0000-4000-8000-000000000002";
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let binding = vec![
            BindingAxis {
                parameter_id: id.into(),
                keys: vec![-1., 0., 1.]
            };
            5
        ];
        let mut scratch = Selection::default();
        for value in [-2., -1., -0.8, -0.5, 0., 0.3, 1., 2.] {
            let values = [(id.to_owned(), value)].into();
            let result = select(&doc, &values, &binding, &mut scratch);
            let segment = find_key_segment(value, &binding[0].keys, 0.000001, 0.0000015);
            let axes = vec![
                KeyAxis {
                    index: segment.index,
                    key_count: 3,
                    weight: segment.weight
                };
                5
            ];
            let mut indices = vec![0; 32];
            let mut weights = vec![0.; 32];
            let count = key_combinations(&axes, &mut indices, &mut weights);
            assert_eq!(
                result.indices,
                indices[..count]
                    .iter()
                    .map(|&i| i as usize)
                    .collect::<Vec<_>>()
            );
            assert_eq!(result.weights, weights[..count]);
            assert_eq!(result.enabled, !segment.is_outside);
        }
        let binding = vec![
            BindingAxis {
                parameter_id: id.into(),
                keys: vec![0.]
            };
            80
        ];
        let result = select(&doc, &[(id.into(), 0.)].into(), &binding, &mut scratch);
        assert_eq!(result.indices, [0]);
        assert_eq!(result.weights, [1.]);
    }
}
