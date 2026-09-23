use super::*;

pub(crate) fn full_frame_tuple(frame: &DrawableFrame, version: Version) -> FullEvaluationTuple {
    (
        version_tuple(version),
        frame.source_revision,
        (
            frame.canvas.width,
            frame.canvas.height,
            frame.canvas.origin.x,
            frame.canvas.origin.y,
            frame.canvas.pixels_per_unit,
        ),
        frame
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.requested, p.value, p.clamped))
            .collect(),
        frame
            .drawables
            .iter()
            .map(|d| {
                (
                    (
                        d.id.clone(),
                        d.runtime_id.clone(),
                        d.part_id.clone(),
                        d.raw_blend_mode,
                        d.texture_asset_id.clone(),
                        d.texture_slot,
                        d.positions.iter().map(|p| (p.x, p.y)).collect(),
                        d.uvs.iter().map(|p| (p.x, p.y)).collect(),
                        d.indices.to_vec(),
                        d.draw_order,
                    ),
                    (
                        d.render_order,
                        d.opacity,
                        d.multiply_color,
                        d.screen_color,
                        blend_mode_name(d.blend_mode).into(),
                        d.enabled,
                        d.visible,
                        d.double_sided,
                        d.inverted_mask,
                        d.masks.clone(),
                    ),
                )
            })
            .collect(),
        frame
            .offscreens
            .iter()
            .map(|o| {
                (
                    o.id.clone(),
                    o.runtime_id.clone(),
                    o.owner_part_id.clone(),
                    o.parent_offscreen_id.clone(),
                    o.render_order,
                    o.opacity,
                    o.enabled,
                    o.blend_mode,
                    o.flags,
                    o.masks.clone(),
                    o.multiply_color,
                    o.screen_color,
                )
            })
            .collect(),
        frame
            .render_plan
            .iter()
            .map(|command| match command {
                RenderCommand::BeginOffscreen { offscreen_id } => {
                    ("begin_offscreen".into(), offscreen_id.clone())
                }
                RenderCommand::DrawMesh { mesh_id } => ("draw_mesh".into(), mesh_id.clone()),
                RenderCommand::EndOffscreen { offscreen_id } => {
                    ("end_offscreen".into(), offscreen_id.clone())
                }
            })
            .collect(),
    )
}
pub(crate) fn frame_tuple(frame: &DrawableFrame) -> EvaluationTuple {
    (
        frame
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.id.clone(),
                    parameter.requested,
                    parameter.value,
                    parameter.clamped,
                )
            })
            .collect(),
        frame
            .drawables
            .iter()
            .map(|drawable| {
                (
                    drawable.id.clone(),
                    drawable.positions.iter().map(|p| (p.x, p.y)).collect(),
                )
            })
            .collect(),
    )
}
