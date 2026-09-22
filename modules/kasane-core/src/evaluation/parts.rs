use crate::document::Document;

use super::evaluator::{set_value, EvalContext};
use super::prepared::PreparedEvaluation;
use super::selection::{evaluate_blend_binding, select};
use super::transforms::f32_to_i32;
use crate::types::DeltaKeyforms;

pub(super) fn evaluate(doc: &Document, prepared: &PreparedEvaluation, state: &mut EvalContext<'_>) {
    state
        .enabled_parts
        .retain(|id, _| doc.get_part(id).is_some());
    state.part_orders.retain(|id, _| doc.get_part(id).is_some());
    for id in &prepared.parts {
        let p = doc.get_part(id).unwrap();
        let mut enabled =
            p.enabled && (p.parent_id.is_empty() || state.enabled_parts[&p.parent_id]);
        let mut order = p.draw_order;
        if let Some(b) = doc.binding_for_scene(id) {
            let s = select(doc, state.values, &b.axes, state.selection);
            enabled &= s.enabled;
            if s.enabled {
                order = 0.0;
                for k in 0..s.indices.len() {
                    order += b.track.sample(s.indices[k]).draw_order * s.weights[k];
                }
            }
        }
        let bs_list = doc.blend_bindings_for_target(id);
        if !bs_list.is_empty() {
            order = f32_to_i32(order + 0.001) as f32;
            for bs in bs_list {
                if let DeltaKeyforms::Part(ref forms) = bs.keyforms {
                    for (kf_idx, eff_w) in evaluate_blend_binding(doc, state.values, bs) {
                        if kf_idx < forms.len() {
                            order += forms[kf_idx].draw_order * eff_w;
                        }
                    }
                }
            }
            order = f32_to_i32((order + 0.001).clamp(0.0, 1000.0)) as f32;
        }
        set_value(state.enabled_parts, id, enabled);
        set_value(state.part_orders, id, f32_to_i32(order + 0.001));
    }
}
