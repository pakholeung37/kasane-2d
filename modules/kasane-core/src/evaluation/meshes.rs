use crate::deformers::PsmVec2;
use crate::document::Document;
use crate::types::{Appearance, DeltaKeyforms, Status, Vec2};

use super::evaluator::EvalContext;
use super::prepared::PreparedEvaluation;
use super::selection::{
    blend_appearance, blend_positions, default_selection, evaluate_blend_binding,
    inherit_appearance, select,
};
use super::transforms::f32_to_i32;

pub(super) fn evaluate(
    doc: &Document,
    prepared: &PreparedEvaluation,
    state: &mut EvalContext<'_>,
) -> Status {
    let asset_slots = &prepared.assets;
    for (mesh_index, id) in doc.mesh_order().iter().enumerate() {
        let mesh = doc.get_mesh(id).unwrap();
        let d = &mut state.frame.drawables[mesh_index];
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
        d.uvs = prepared.geometry[mesh_index].uvs.clone();
        d.indices = prepared.geometry[mesh_index].indices.clone();
        let mut order = mesh.draw_order.unwrap_or(mesh_index as f32);
        let mut appearance = mesh.appearance;

        let slot = asset_slots.get(mesh.texture_asset_id.as_str()).copied();
        match slot {
            Some(s) => d.texture_slot = s as i32,
            None => return Status::error("MISSING_ASSET", id),
        }

        d.visible = mesh.enabled
            && (mesh.part_id.is_empty() || state.enabled_parts[&mesh.part_id])
            && (mesh.deformer_id.is_empty()
                || state.transforms[prepared.transform_slots[&mesh.deformer_id]].enabled);

        let b = doc.binding_for_mesh(id);
        let selection = b.map(|binding| select(doc, state.values, &binding.axes, state.selection));
        if let Some(sel) = selection {
            d.visible &= sel.enabled;
        }
        d.enabled = d.visible;

        if d.visible {
            let sel_ref = selection.unwrap_or(default_selection());
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
                        let selection = evaluate_blend_binding(doc, state.values, bs);
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
                let parent = &state.transforms[prepared.transform_slots[&mesh.deformer_id]];
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
    Status::ok()
}
