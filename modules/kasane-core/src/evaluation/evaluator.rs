use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::document::Document;
use crate::types::Status;

use super::selection::Selection;
use super::transforms::TransformState;
use super::types::{DrawableFrame, PreviewValues};

pub(super) struct EvalContext<'a> {
    pub(super) frame: &'a mut DrawableFrame,
    pub(super) values: &'a mut HashMap<String, f32>,
    pub(super) enabled_parts: &'a mut HashMap<String, bool>,
    pub(super) part_orders: &'a mut HashMap<String, i32>,
    pub(super) selections: &'a mut Vec<Selection>,
    pub(super) transforms: &'a mut Vec<TransformState>,
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
    selections: Vec<Selection>,
    transforms: Vec<TransformState>,
    points: Vec<crate::types::Vec2>,
    orders: Vec<i32>,
    offscreen_orders: Vec<i32>,
    items: Vec<(usize, i32)>,
    plan_items: Vec<(i32, usize)>,
    active_offscreens: Vec<usize>,
}

/// Wall-clock stages of one successful core frame evaluation. Durations are
/// measured only when `evaluate_timed` is called.
#[derive(Clone, Copy, Debug, Default)]
pub struct EvaluationTimings {
    pub preflight: Duration,
    pub parameters: Duration,
    pub selections: Duration,
    pub parts: Duration,
    pub transforms: Duration,
    pub meshes: Duration,
    pub glue: Duration,
    pub render: Duration,
}

impl FrameEvaluator {
    pub fn evaluate(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
    ) -> Status {
        self.evaluate_with_hidden_geometry(doc, preview, out, false, None)
    }

    pub fn evaluate_timed(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
        timings: &mut EvaluationTimings,
    ) -> Status {
        *timings = EvaluationTimings::default();
        self.evaluate_with_hidden_geometry(doc, preview, out, false, Some(timings))
    }

    fn evaluate_with_hidden_geometry(
        &mut self,
        doc: &Document,
        preview: &PreviewValues,
        out: &mut DrawableFrame,
        include_hidden_geometry: bool,
        timings: Option<&mut EvaluationTimings>,
    ) -> Status {
        let status = evaluate_into(doc, preview, self, include_hidden_geometry, timings);
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
/// This is intended for raster exports and observation queries. Visible
/// drawables retain the normal evaluation result, even when Glue connects
/// them to disabled meshes whose normal positions are placeholders.
pub fn evaluate_frame_including_hidden(
    doc: &Document,
    preview: &PreviewValues,
    out: &mut DrawableFrame,
) -> Status {
    let mut evaluator = FrameEvaluator::default();
    let mut captured = DrawableFrame::default();
    let status = evaluator.evaluate_with_hidden_geometry(doc, preview, &mut captured, true, None);
    if !status.is_ok() {
        return status;
    }
    if !doc.glue_order().is_empty() && captured.drawables.iter().any(|d| !d.enabled) {
        // Hidden geometry must not feed back through Glue into the rendered
        // pose. Reuse the normal result for enabled drawables; retain the
        // fully evaluated hidden geometry only for inspection/export.
        let mut normal = DrawableFrame::default();
        let status = evaluator.evaluate(doc, preview, &mut normal);
        if !status.is_ok() {
            return status;
        }
        for (drawable, normal_drawable) in captured.drawables.iter_mut().zip(normal.drawables) {
            if normal_drawable.enabled {
                *drawable = normal_drawable;
            }
        }
    }
    *out = captured;
    Status::ok()
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
    mut timings: Option<&mut EvaluationTimings>,
) -> Status {
    let mut stage_start = timings.as_ref().map(|_| Instant::now());
    macro_rules! record_stage {
        ($field:ident) => {
            if let Some(timings) = timings.as_deref_mut() {
                let now = Instant::now();
                timings.$field = now.duration_since(stage_start.replace(now).unwrap());
            }
        };
    }
    let FrameEvaluator {
        scratch: frame,
        values,
        enabled_parts,
        part_orders,
        selections,
        transforms,
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
        selections,
        transforms,
        points,
        orders,
        offscreen_orders,
        items,
        plan_items,
        active_offscreens,
        include_hidden_geometry,
    };
    record_stage!(preflight);

    let status = super::parameters::evaluate(doc, preview, &mut state);
    if !status.is_ok() {
        return status;
    }
    record_stage!(parameters);
    state
        .selections
        .resize_with(prepared.selections.len(), Selection::default);
    for (axes, selection) in prepared.selections.iter().zip(state.selections.iter_mut()) {
        super::selection::select_prepared(&state.frame.parameters, axes, selection);
    }
    record_stage!(selections);
    super::parts::evaluate(doc, prepared, &mut state);
    record_stage!(parts);
    let status = super::transforms::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    record_stage!(transforms);
    let status = super::meshes::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    record_stage!(meshes);
    let status = super::glue::evaluate(doc, prepared, &mut state);
    if !status.is_ok() {
        return status;
    }
    record_stage!(glue);
    let status = super::render::evaluate(doc, prepared, &mut state);
    record_stage!(render);
    status
}
