use std::collections::HashSet;

use kasane_core::evaluation::{DrawableFrame, RenderCommand};
use kasane_core::geometry::validate_render_mesh;
use kasane_core::types::Status;

/// Shared preflight for both Document evaluation and external runtime frames.
pub(crate) fn validate_frame(frame: &DrawableFrame) -> Status {
    let canvas = frame.canvas;
    if !canvas.width.is_finite()
        || !canvas.height.is_finite()
        || !canvas.origin.x.is_finite()
        || !canvas.origin.y.is_finite()
        || !canvas.pixels_per_unit.is_finite()
        || canvas.width <= 0.0
        || canvas.height <= 0.0
        || canvas.pixels_per_unit <= 0.0
    {
        return Status::error(
            "INVALID_CANVAS",
            "Canvas dimensions and scale must be positive and finite.",
        );
    }
    let mut ids = HashSet::new();
    for drawable in &frame.drawables {
        if drawable.id.is_empty() || !ids.insert(drawable.id.as_str()) {
            return Status::error("INVALID_ID", "Drawable IDs must be non-empty and unique.");
        }
        let status = validate_render_mesh(&drawable.positions, &drawable.uvs, &drawable.indices);
        if !status.is_ok() {
            return status;
        }
        if !drawable.opacity.is_finite()
            || drawable
                .multiply_color
                .iter()
                .chain(&drawable.screen_color)
                .any(|v| !v.is_finite())
            || drawable.positions.iter().any(|p| {
                !(p.x * canvas.pixels_per_unit + canvas.origin.x).is_finite()
                    || !(canvas.origin.y - p.y * canvas.pixels_per_unit).is_finite()
            })
        {
            return Status::error(
                "NON_FINITE",
                "Appearance and converted positions must be finite.",
            );
        }
        if let Some(mode) = drawable.raw_blend_mode {
            if !valid_blend_mode(mode) {
                return Status::error("UNSUPPORTED_BLEND_MODE", format!("{}: {mode}", drawable.id));
            }
        }
    }
    for drawable in &frame.drawables {
        if drawable.masks.iter().any(|id| !ids.contains(id.as_str())) {
            return Status::error("MISSING_MASK", &drawable.id);
        }
    }
    let mut offscreen_ids = HashSet::new();
    for offscreen in &frame.offscreens {
        if offscreen.id.is_empty()
            || ids.contains(offscreen.id.as_str())
            || !offscreen_ids.insert(offscreen.id.as_str())
        {
            return Status::error(
                "INVALID_ID",
                "Offscreen IDs must be non-empty and unique across the frame.",
            );
        }
        if !offscreen.opacity.is_finite()
            || offscreen
                .multiply_color
                .iter()
                .chain(&offscreen.screen_color)
                .any(|v| !v.is_finite())
        {
            return Status::error("NON_FINITE", format!("{}.appearance", offscreen.id));
        }
        if !valid_blend_mode(offscreen.blend_mode) {
            return Status::error(
                "UNSUPPORTED_BLEND_MODE",
                format!("{}: {}", offscreen.id, offscreen.blend_mode),
            );
        }
        if offscreen.masks.iter().any(|id| !ids.contains(id.as_str())) {
            return Status::error("MISSING_MASK", &offscreen.id);
        }
        if offscreen
            .parent_offscreen_id
            .as_ref()
            .is_some_and(|id| id == &offscreen.id)
        {
            return Status::error("INVALID_RENDER_PLAN", "An offscreen cannot parent itself.");
        }
    }
    for offscreen in &frame.offscreens {
        if offscreen
            .parent_offscreen_id
            .as_ref()
            .is_some_and(|id| !offscreen_ids.contains(id.as_str()))
        {
            return Status::error(
                "INVALID_RENDER_PLAN",
                format!("{}: missing parent", offscreen.id),
            );
        }
    }
    if !frame.offscreens.is_empty() || !frame.render_plan.is_empty() {
        let mut stack: Vec<&str> = Vec::new();
        let mut begun = HashSet::new();
        let mut drawn = HashSet::new();
        for command in &frame.render_plan {
            match command {
                RenderCommand::BeginOffscreen { offscreen_id } => {
                    if !offscreen_ids.contains(offscreen_id.as_str())
                        || !begun.insert(offscreen_id.as_str())
                    {
                        return Status::error("INVALID_RENDER_PLAN", offscreen_id);
                    }
                    let expected_parent = frame
                        .offscreens
                        .iter()
                        .find(|o| o.id == *offscreen_id)
                        .and_then(|o| o.parent_offscreen_id.as_deref());
                    if stack.last().copied() != expected_parent {
                        return Status::error(
                            "INVALID_RENDER_PLAN",
                            format!("{}: parent mismatch", offscreen_id),
                        );
                    }
                    stack.push(offscreen_id);
                }
                RenderCommand::DrawMesh { mesh_id } => {
                    if !ids.contains(mesh_id.as_str()) || !drawn.insert(mesh_id.as_str()) {
                        return Status::error("INVALID_RENDER_PLAN", mesh_id);
                    }
                }
                RenderCommand::EndOffscreen { offscreen_id } => {
                    if stack.pop() != Some(offscreen_id.as_str()) {
                        return Status::error("INVALID_RENDER_PLAN", offscreen_id);
                    }
                }
            }
        }
        if !stack.is_empty() || begun.len() != offscreen_ids.len() || drawn.len() != ids.len() {
            return Status::error(
                "INVALID_RENDER_PLAN",
                "Render plan must contain every drawable and balanced offscreen exactly once.",
            );
        }
    }
    Status::ok()
}

fn valid_blend_mode(mode: u32) -> bool {
    let color = mode & 0xff;
    let alpha = (mode >> 8) & 0xff;
    mode >> 16 == 0 && color <= 17 && alpha <= 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::evaluation::Drawable;
    use kasane_core::types::{Canvas, Vec2};

    fn frame() -> DrawableFrame {
        DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![Drawable {
                id: "mesh".into(),
                positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                ],
                uvs: vec![Vec2::new(0.0, 0.0); 3],
                indices: vec![0, 1, 2],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn accepts_complete_frame_and_empty_model() {
        let mut frame = frame();
        assert!(validate_frame(&frame).is_ok());
        frame.drawables.clear();
        assert!(validate_frame(&frame).is_ok());
    }

    #[test]
    fn accepts_core_flat_plan_but_rejects_duplicate_commands() {
        let mut frame = frame();
        frame.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: "mesh".into(),
        });
        assert!(validate_frame(&frame).is_ok());
        frame.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: "mesh".into(),
        });
        assert_eq!(validate_frame(&frame).code, "INVALID_RENDER_PLAN");
    }

    #[test]
    fn allows_zero_triangles_and_rejects_incomplete_triangles() {
        let mut f = frame();
        f.drawables[0].indices = vec![];
        assert!(validate_frame(&f).is_ok());

        for indices in [vec![0], vec![0, 1], vec![0, 1, 2, 0]] {
            let mut f = frame();
            f.drawables[0].indices = indices;
            assert!(!validate_frame(&f).is_ok());
        }
    }

    #[test]
    fn rejects_out_of_range_indices_and_mismatched_uvs() {
        let mut frame = frame();
        frame.drawables[0].indices[2] = u32::MAX;
        assert!(!validate_frame(&frame).is_ok());
        frame.drawables[0].indices[2] = 2;
        frame.drawables[0].uvs.pop();
        assert!(!validate_frame(&frame).is_ok());
    }

    #[test]
    fn rejects_duplicate_ids_and_dangling_masks() {
        let mut frame = frame();
        frame.drawables.push(frame.drawables[0].clone());
        assert!(!validate_frame(&frame).is_ok());
        frame.drawables[1].id = "mask".into();
        frame.drawables[0].masks = vec!["mask".into()];
        assert!(validate_frame(&frame).is_ok());
        frame.drawables.pop();
        assert!(!validate_frame(&frame).is_ok());
    }

    #[test]
    fn rejects_non_finite_and_overflowing_positions() {
        let mut frame = frame();
        frame.drawables[0].positions[0].x = f32::NAN;
        assert!(!validate_frame(&frame).is_ok());
        frame.drawables[0].positions[0].x = f32::MAX;
        assert!(!validate_frame(&frame).is_ok());
    }

    #[test]
    fn rejects_invalid_canvas_and_appearance() {
        let mut frame = frame();
        frame.canvas.pixels_per_unit = 0.0;
        assert!(!validate_frame(&frame).is_ok());
        frame.canvas.pixels_per_unit = 100.0;
        frame.drawables[0].screen_color[1] = f32::INFINITY;
        assert!(!validate_frame(&frame).is_ok());
    }

    #[test]
    fn rejects_invalid_extended_blend_mode() {
        let mut frame = frame();
        frame.drawables[0].raw_blend_mode = Some(18);
        assert_eq!(validate_frame(&frame).code, "UNSUPPORTED_BLEND_MODE");
        frame.drawables[0].raw_blend_mode = Some(17 | (4 << 8));
        assert!(validate_frame(&frame).is_ok());
    }
}
