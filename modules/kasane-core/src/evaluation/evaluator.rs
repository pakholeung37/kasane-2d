use std::collections::HashMap;

use crate::document::Document;
use crate::types::Status;

use super::selection::Selection;
use super::trace::{build_trace, EvaluationTrace};
use super::transforms::TransformState;
use super::types::{DrawableFrame, PreviewValues};

pub(super) struct EvalContext<'a> {
    pub(super) frame: &'a mut DrawableFrame,
    pub(super) values: &'a mut HashMap<String, f32>,
    pub(super) enabled_parts: &'a mut HashMap<String, bool>,
    pub(super) part_orders: &'a mut HashMap<String, i32>,
    pub(super) selection: &'a mut Selection,
    pub(super) transforms: &'a mut Vec<TransformState>,
    pub(super) trace_axes: Option<&'a mut Vec<Vec<crate::types::Vec2>>>,
    pub(super) points: &'a mut Vec<crate::types::Vec2>,
    pub(super) orders: &'a mut Vec<i32>,
    pub(super) offscreen_orders: &'a mut Vec<i32>,
    pub(super) items: &'a mut Vec<(usize, i32)>,
    pub(super) plan_items: &'a mut Vec<(i32, usize)>,
    pub(super) active_offscreens: &'a mut Vec<usize>,
    pub(super) include_hidden_geometry: bool,
}

#[derive(Debug, Default)]
pub struct FrameEvaluator {
    scratch: DrawableFrame,
    values: HashMap<String, f32>,
    enabled_parts: HashMap<String, bool>,
    part_orders: HashMap<String, i32>,
    selection: Selection,
    transforms: Vec<TransformState>,
    trace_axes: Vec<Vec<crate::types::Vec2>>,
    points: Vec<crate::types::Vec2>,
    orders: Vec<i32>,
    offscreen_orders: Vec<i32>,
    items: Vec<(usize, i32)>,
    plan_items: Vec<(i32, usize)>,
    active_offscreens: Vec<usize>,
}

impl FrameEvaluator {
    pub fn evaluate(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
    ) -> Status {
        self.evaluate_with_hidden_geometry(doc, preview, out, false)
    }

    /// Evaluate once and retain geometry evidence only for this explicit call.
    pub fn evaluate_with_trace(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
        trace: &mut EvaluationTrace,
    ) -> Status {
        self.evaluate_with_trace_and_hidden_geometry(doc, preview, out, trace, false)
    }

    pub fn evaluate_with_trace_and_hidden_geometry(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
        trace: &mut EvaluationTrace,
        include_hidden_geometry: bool,
    ) -> Status {
        let status = evaluate_into(doc, preview, self, include_hidden_geometry, true);
        if !status.is_ok() {
            return status;
        }
        match build_trace(doc, &self.scratch, &self.transforms, &self.trace_axes) {
            Ok(next_trace) => {
                std::mem::swap(out, &mut self.scratch);
                *trace = next_trace;
                Status::ok()
            }
            Err(status) => status,
        }
    }

    fn evaluate_with_hidden_geometry(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
        include_hidden_geometry: bool,
    ) -> Status {
        let status = evaluate_into(doc, preview, self, include_hidden_geometry, false);
        if status.is_ok() {
            std::mem::swap(out, &mut self.scratch);
        }
        status
    }
}

pub fn evaluate_frame(doc: &Document, preview: &PreviewValues, out: &mut DrawableFrame) -> Status {
    FrameEvaluator::default().evaluate(doc, preview, out)
}

/// Evaluate geometry for hidden drawables while retaining their visibility.
/// This is intended for raster exports that include invisible ArtMeshes.
pub fn evaluate_frame_including_hidden(
    doc: &Document,
    preview: &PreviewValues,
    out: &mut DrawableFrame,
) -> Status {
    FrameEvaluator::default().evaluate_with_hidden_geometry(doc, preview, out, true)
}

pub(super) fn set_value<T>(map: &mut HashMap<String, T>, id: &str, value: T) {
    if let Some(slot) = map.get_mut(id) {
        *slot = value;
    } else {
        map.insert(id.to_owned(), value);
    }
}

fn evaluate_into(
    doc: &Document,
    preview: &PreviewValues,
    workspace: &mut FrameEvaluator,
    include_hidden_geometry: bool,
    capture_trace: bool,
) -> Status {
    let FrameEvaluator {
        scratch: frame,
        values,
        enabled_parts,
        part_orders,
        selection,
        transforms,
        trace_axes,
        points,
        orders,
        offscreen_orders,
        items,
        plan_items,
        active_offscreens,
    } = workspace;

    if !doc.initialized() {
        return Status::error("NOT_INITIALIZED", "Initialize Document first");
    }
    if doc.mesh_order().len() > 16777216 || doc.asset_order().len() > (i32::MAX as usize) {
        return Status::error("CAPACITY", "Object count");
    }
    let status = super::parameters::validate_preview(doc, preview);
    if !status.is_ok() {
        return status;
    }

    let prepared = match doc.prepared_evaluation() {
        Ok(p) => p,
        Err(s) => return s.clone(),
    };

    let mut state = EvalContext {
        frame,
        values,
        enabled_parts,
        part_orders,
        selection,
        transforms,
        trace_axes: capture_trace.then_some(trace_axes),
        points,
        orders,
        offscreen_orders,
        items,
        plan_items,
        active_offscreens,
        include_hidden_geometry,
    };

    let status = super::parameters::evaluate(doc, preview, &mut state);
    if !status.is_ok() {
        return status;
    }
    super::parts::evaluate(doc, prepared, &mut state);
    let status = super::transforms::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    let status = super::meshes::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    let status = super::glue::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    super::render::evaluate(doc, prepared, &mut state)
}
