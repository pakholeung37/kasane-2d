use super::*;

pub(super) fn draw_uniform(
    target_size: (u32, u32),
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
    opacity: f32,
) -> DrawUniform {
    DrawUniform {
        target_size: [target_size.0 as f32, target_size.1 as f32],
        padding: [0.0; 2],
        multiply_color,
        screen_color,
        opacity,
        padding_end: [0.0; 7],
        mask_bounds: [0.0; 4],
        mask_flags: [0; 4],
        blend_modes: [0; 4],
        view_a: [1.0, 0.0, 0.0, 0.0],
        view_b: [0.0, 1.0, 0.0, 0.0],
        view_origin: [0.0; 4],
    }
}

pub(super) fn with_view_transform(mut uniform: DrawUniform, transform: Affine2) -> DrawUniform {
    uniform.view_a = [transform.a.x, transform.a.y, 0.0, 0.0];
    uniform.view_b = [transform.b.x, transform.b.y, 0.0, 0.0];
    uniform.view_origin = [transform.origin.x, transform.origin.y, 0.0, 0.0];
    uniform
}

pub(super) fn with_blend_mode(mut uniform: DrawUniform, mode: u32) -> DrawUniform {
    uniform.blend_modes = [mode & 0xff, (mode >> 8) & 0xff, 0, 0];
    uniform
}

pub(super) fn with_fixed_blend_mode(mut uniform: DrawUniform, mode: BlendMode) -> DrawUniform {
    uniform.blend_modes[2] = match mode {
        BlendMode::Normal => 0,
        BlendMode::Additive => 1,
        BlendMode::Multiplicative => 2,
    };
    uniform
}

pub(super) fn quad_vertices(
    size: (u32, u32),
    transform: Affine2,
    mask_transform: Affine2,
) -> Vec<Vertex> {
    [
        (Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
        (Vec2::new(size.0 as f32, 0.0), Vec2::new(1.0, 0.0)),
        (Vec2::new(size.0 as f32, size.1 as f32), Vec2::new(1.0, 1.0)),
        (Vec2::new(0.0, size.1 as f32), Vec2::new(0.0, 1.0)),
    ]
    .into_iter()
    .map(|(position, uv)| {
        let mask_point = mask_transform.transform_point(position);
        let position = transform.transform_point(position);
        Vertex {
            position: [position.x, position.y],
            uv: [uv.x, uv.y],
            mask_point: [mask_point.x, mask_point.y],
        }
    })
    .collect()
}

pub(super) fn quad_indices() -> Vec<u32> {
    triangle_indices(&[0, 1, 2, 0, 2, 3])
}

pub(super) fn inverse_affine(transform: Affine2) -> Result<Affine2, Status> {
    let determinant = transform.determinant();
    if !transform.is_finite() || determinant.abs() < 1.0e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    let inverse_a = Vec2::new(transform.b.y / determinant, -transform.a.y / determinant);
    let inverse_b = Vec2::new(-transform.b.x / determinant, transform.a.x / determinant);
    let origin = Vec2::new(
        -(inverse_a.x * transform.origin.x + inverse_b.x * transform.origin.y),
        -(inverse_a.y * transform.origin.x + inverse_b.y * transform.origin.y),
    );
    Ok(Affine2 {
        a: inverse_a,
        b: inverse_b,
        origin,
    })
}

pub(super) fn compose_affine(lhs: Affine2, rhs: Affine2) -> Affine2 {
    let origin = lhs.transform_point(rhs.origin);
    let lhs_origin = lhs.transform_point(Vec2::new(0.0, 0.0));
    let a_point = lhs.transform_point(rhs.a);
    let b_point = lhs.transform_point(rhs.b);
    Affine2 {
        a: Vec2::new(a_point.x - lhs_origin.x, a_point.y - lhs_origin.y),
        b: Vec2::new(b_point.x - lhs_origin.x, b_point.y - lhs_origin.y),
        origin,
    }
}

pub(super) fn vertices_for(
    drawable: &Drawable,
    frame: &DrawableFrame,
    viewport: ViewportConfig,
) -> Vec<Vertex> {
    drawable
        .positions
        .iter()
        .zip(drawable.uvs.iter())
        .map(|(position, uv)| {
            let pixel = Vec2::new(
                position.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                frame.canvas.origin.y - position.y * frame.canvas.pixels_per_unit,
            );
            let mask_point = pixel;
            let pixel = viewport.transform.transform_point(pixel);
            Vertex {
                position: [pixel.x, pixel.y],
                uv: [uv.x, 1.0 - uv.y],
                mask_point: [mask_point.x, mask_point.y],
            }
        })
        .collect()
}

pub(super) fn triangle_indices(indices: &[u32]) -> Vec<u32> {
    let (triangles, _) = indices.as_chunks::<3>();
    triangles
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect()
}
