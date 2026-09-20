use std::collections::HashSet;

use kasane_core::evaluation::DrawableFrame;
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
    }
    for drawable in &frame.drawables {
        if drawable.masks.iter().any(|id| !ids.contains(id.as_str())) {
            return Status::error("MISSING_MASK", &drawable.id);
        }
    }
    Status::ok()
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
    fn rejects_incomplete_triangles_before_conversion() {
        for indices in [vec![], vec![0], vec![0, 1], vec![0, 1, 2, 0]] {
            let mut frame = frame();
            frame.drawables[0].indices = indices;
            assert!(!validate_frame(&frame).is_ok());
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
}
