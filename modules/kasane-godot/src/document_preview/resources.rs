use super::*;

pub(super) struct SubmissionPlan<'a> {
    pub active: std::collections::HashSet<&'a str>,
    pub size: Vector2i,
    pub transform: Transform2D,
}

/// Validate and calculate the complete allocation before touching GPU resources.
pub(super) fn plan<'a>(
    frame: &'a DrawableFrame,
    textures: &HashMap<String, Gd<Texture2D>>,
    transform: Transform2D,
    target: Vector2,
    scale: f64,
) -> Result<SubmissionPlan<'a>, Status> {
    let status = validate_frame(frame);
    if !status.is_ok() {
        return Err(status);
    }
    for drawable in &frame.drawables {
        let texture = textures
            .get(&drawable.texture_asset_id)
            .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
        if texture.get_width() <= 0 || texture.get_height() <= 0 {
            return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
        }
    }
    let active = active_offscreens(frame);
    let (size, transform) = offscreen_layout(
        Vector2::new(frame.canvas.width, frame.canvas.height),
        transform,
        target,
        active.len(),
    )?;
    let surface_bytes = i64::from(size.x) * i64::from(size.y) * 8 * active.len() as i64;
    let mask_bytes: i64 = frame
        .drawables
        .iter()
        .map(|d| mask_reserved_bytes(frame, &d.masks, scale))
        .chain(
            frame
                .offscreens
                .iter()
                .filter(|o| active.contains(o.id.as_str()))
                .map(|o| mask_reserved_bytes(frame, &o.masks, scale.max(1.0))),
        )
        .sum();
    if surface_bytes + mask_bytes > OFFSCREEN_BUDGET_BYTES {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen and mask attachments exceed the shared 512 MiB budget.",
        ));
    }
    Ok(SubmissionPlan {
        active,
        size,
        transform,
    })
}

fn active_offscreens(frame: &DrawableFrame) -> std::collections::HashSet<&str> {
    let groups: HashMap<&str, _> = frame
        .offscreens
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();
    let meshes: HashMap<&str, _> = frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut stack = Vec::new();
    let mut active = std::collections::HashSet::new();
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => stack.push(offscreen_id.as_str()),
            RenderCommand::EndOffscreen { .. } => {
                stack.pop();
            }
            RenderCommand::DrawMesh { mesh_id } => {
                let d = meshes[mesh_id.as_str()];
                if d.visible
                    && d.opacity > 0.0
                    && !d.indices.is_empty()
                    && stack
                        .iter()
                        .all(|id| groups[id].enabled && groups[id].opacity > 0.0)
                {
                    active.extend(stack.iter().copied());
                }
            }
        }
    }
    active
}

fn mask_reserved_bytes(frame: &DrawableFrame, masks: &[String], scale: f64) -> i64 {
    if masks.is_empty() {
        return 0;
    }
    let mut bounds: Option<Rect2> = None;
    for id in masks {
        if let Some(mesh) = frame.drawables.iter().find(|d| &d.id == id) {
            for p in &mesh.positions {
                let point = Vector2::new(
                    p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                    frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit,
                );
                bounds = Some(
                    bounds
                        .map(|b| b.expand(point))
                        .unwrap_or(Rect2::new(point, Vector2::ZERO)),
                );
            }
        }
    }
    let bounds = bounds.unwrap_or_default().grow(4.0);
    let w = bounds.size.x.ceil().max(1.0) as f64;
    let h = bounds.size.y.ceil().max(1.0) as f64;
    let scale = scale.min(4096.0 / w.max(h));
    // Same dimensions as both mask allocation paths, including Godot's minimum.
    (w * scale).ceil().max(2.0) as i64 * (h * scale).ceil().max(2.0) as i64 * 4
}

// Keep surfaces at preview resolution, bounded by the target viewport. Budget
// validation happens before any GPU allocation; never silently drop a group.
pub(super) const OFFSCREEN_BUDGET_BYTES: i64 = 512 * 1024 * 1024;
fn offscreen_layout(
    canvas: Vector2,
    transform: Transform2D,
    target: Vector2,
    count: usize,
) -> Result<(Vector2i, Transform2D), Status> {
    if count == 0 {
        return Ok((Vector2i::new(2, 2), Transform2D::IDENTITY));
    }
    if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    // Crop to the visible canvas and snap BOTH edges to physical pixels. All
    // nested surfaces share this grid, so compositing never resamples a model
    // at fractional offsets (which otherwise blurs small Editor previews).
    let bounds = (transform * Rect2::new(Vector2::ZERO, canvas))
        .intersect(Rect2::new(Vector2::ZERO, target))
        .unwrap_or_default();
    let origin = bounds.position.floor();
    let extent = bounds.end().ceil() - origin;
    let size = Vector2i::new((extent.x as i32).max(2), (extent.y as i32).max(2));
    let bytes = i64::from(size.x) * i64::from(size.y) * 8;
    if size.x > 4096 || size.y > 4096 || count > (OFFSCREEN_BUDGET_BYTES / bytes) as usize {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            format!(
                "{count} surfaces at {}x{} exceed the {} byte color/destination budget",
                size.x, size.y, OFFSCREEN_BUDGET_BYTES
            ),
        ));
    }
    let mut root = transform;
    root.origin -= origin;
    Ok((size, root))
}

#[cfg(test)]
mod surface_budget_tests {
    use super::*;
    #[test]
    fn fractional_translation_preserves_the_screen_pixel_grid() {
        let transform = Transform2D::IDENTITY.scaled(Vector2::splat(0.25));
        let mut transform = transform;
        transform.origin = Vector2::new(20.25, 30.75);
        let (size, root) = offscreen_layout(
            Vector2::new(5200.0, 7000.0),
            transform,
            Vector2::splat(2048.0),
            24,
        )
        .unwrap();
        assert_eq!(size, Vector2i::new(1301, 1751));
        assert_eq!(root.origin, Vector2::new(0.25, 0.75));
        let placement = transform * root.affine_inverse();
        assert_eq!(placement.origin, Vector2::new(20.0, 30.0));
        assert_eq!(placement.a, Vector2::RIGHT);
    }
    #[test]
    fn zoom_crops_to_target_and_excess_surfaces_fail_before_allocation() {
        let (size, _) = offscreen_layout(
            Vector2::new(5200.0, 7000.0),
            Transform2D::IDENTITY.scaled(Vector2::splat(100.0)),
            Vector2::new(1024.0, 768.0),
            24,
        )
        .unwrap();
        assert_eq!(size, Vector2i::new(1024, 768));
        assert!(offscreen_layout(
            Vector2::splat(4096.0),
            Transform2D::IDENTITY,
            Vector2::splat(4096.0),
            5
        )
        .is_err());
    }
}
