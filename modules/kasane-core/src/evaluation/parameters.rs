use crate::document::Document;
use crate::types::Status;

use super::evaluator::{set_value, EvalContext};
use super::types::{Drawable, EvaluatedParameter};

pub(super) fn evaluate(
    doc: &Document,
    preview: &super::types::PreviewValues,
    state: &mut EvalContext<'_>,
) -> Status {
    state.frame.source_revision = doc.revision();
    state.frame.canvas = doc.canvas();
    state
        .frame
        .parameters
        .resize_with(doc.parameter_order().len(), EvaluatedParameter::default);
    state.frame.offscreens.clear();

    state
        .frame
        .drawables
        .resize_with(doc.mesh_order().len(), Drawable::default);

    state.values.retain(|id, _| doc.get_parameter(id).is_some());
    for (parameter_slot, id) in doc.parameter_order().iter().enumerate() {
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
        let sample = &mut state.frame.parameters[parameter_slot];
        sample.id.clone_from(id);
        sample.requested = requested;
        sample.value = v;
        sample.clamped = clamped;
        set_value(state.values, id, v);
    }
    Status::ok()
}

pub(super) fn validate_preview(doc: &Document, preview: &super::types::PreviewValues) -> Status {
    for (id, &v) in preview {
        if doc.get_parameter(id).is_none() {
            return Status::error("MISSING_PARAMETER", id);
        }
        if !v.is_finite() {
            return Status::error("NON_FINITE", format!("{}.preview_value", id));
        }
    }
    Status::ok()
}
