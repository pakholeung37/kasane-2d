use std::collections::HashMap;

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
    pub(super) selection: &'a mut Selection,
    pub(super) transforms: &'a mut Vec<TransformState>,
    pub(super) points: &'a mut Vec<crate::types::Vec2>,
    pub(super) orders: &'a mut Vec<i32>,
    pub(super) offscreen_orders: &'a mut Vec<i32>,
    pub(super) items: &'a mut Vec<(usize, i32)>,
    pub(super) plan_items: &'a mut Vec<(i32, usize)>,
    pub(super) active_offscreens: &'a mut Vec<usize>,
}

#[derive(Debug, Default)]
pub struct FrameEvaluator {
    scratch: DrawableFrame,
    values: HashMap<String, f32>,
    enabled_parts: HashMap<String, bool>,
    part_orders: HashMap<String, i32>,
    selection: Selection,
    transforms: Vec<TransformState>,
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
        let status = evaluate_into(doc, preview, self);
        if status.is_ok() {
            std::mem::swap(out, &mut self.scratch);
        }
        status
    }
}

pub fn evaluate_frame(doc: &Document, preview: &PreviewValues, out: &mut DrawableFrame) -> Status {
    FrameEvaluator::default().evaluate(doc, preview, out)
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
) -> Status {
    let FrameEvaluator {
        scratch: frame,
        values,
        enabled_parts,
        part_orders,
        selection,
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
        selection,
        transforms,
        points,
        orders,
        offscreen_orders,
        items,
        plan_items,
        active_offscreens,
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
