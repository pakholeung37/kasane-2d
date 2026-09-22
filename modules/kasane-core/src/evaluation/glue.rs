use crate::document::Document;
use crate::geometry::validate_positions;
use crate::types::{DeltaKeyforms, Status, Vec2};

use super::evaluator::EvalContext;
use super::prepared::PreparedEvaluation;
use super::selection::{evaluate_blend_binding, select};

pub(super) fn evaluate(
    doc: &Document,
    prepared: &PreparedEvaluation,
    state: &mut EvalContext<'_>,
) -> Status {
    // Apply Glues across transformed mesh positions (before canvas Y-reversal, matching PurismCore)
    let glue_order = doc.glue_order();
    if !glue_order.is_empty() {
        let mesh_slots = &prepared.meshes;

        for gid in glue_order {
            if let Some(glue) = doc.get_glue(gid) {
                let mut intensity = if let Some(binding) = &glue.binding {
                    let selection = select(doc, state.values, &binding.axes, state.selection);
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
                            for (kf_idx, eff_w) in evaluate_blend_binding(doc, state.values, bs) {
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

                for pair in &glue.pairs {
                    let idx_a = match doc.vertex_slot(&glue.mesh_a_id, pair.vertex_a) {
                        Some(idx) => idx,
                        None => continue,
                    };
                    let idx_b = match doc.vertex_slot(&glue.mesh_b_id, pair.vertex_b) {
                        Some(idx) => idx,
                        None => continue,
                    };

                    let len_a = state.frame.drawables[slot_a].positions.len();
                    let len_b = state.frame.drawables[slot_b].positions.len();
                    if idx_a >= len_a || idx_b >= len_b {
                        continue;
                    }

                    if slot_a == slot_b {
                        let p0 = state.frame.drawables[slot_a].positions[idx_a];
                        let p1 = state.frame.drawables[slot_a].positions[idx_b];
                        let d = Vec2::new(p1.x - p0.x, p1.y - p0.y);
                        state.frame.drawables[slot_a].positions[idx_a].x +=
                            d.x * (intensity * pair.weight_a);
                        state.frame.drawables[slot_a].positions[idx_a].y +=
                            d.y * (intensity * pair.weight_a);
                        state.frame.drawables[slot_a].positions[idx_b].x -=
                            d.x * (intensity * pair.weight_b);
                        state.frame.drawables[slot_a].positions[idx_b].y -=
                            d.y * (intensity * pair.weight_b);
                    } else {
                        let p0 = state.frame.drawables[slot_a].positions[idx_a];
                        let p1 = state.frame.drawables[slot_b].positions[idx_b];
                        let d = Vec2::new(p1.x - p0.x, p1.y - p0.y);
                        state.frame.drawables[slot_a].positions[idx_a].x +=
                            d.x * (intensity * pair.weight_a);
                        state.frame.drawables[slot_a].positions[idx_a].y +=
                            d.y * (intensity * pair.weight_a);
                        state.frame.drawables[slot_b].positions[idx_b].x -=
                            d.x * (intensity * pair.weight_b);
                        state.frame.drawables[slot_b].positions[idx_b].y -=
                            d.y * (intensity * pair.weight_b);
                    }
                }
            }
        }
    }

    // Apply canvas Y reversal and validate positions
    for d in &mut state.frame.drawables {
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
    Status::ok()
}
