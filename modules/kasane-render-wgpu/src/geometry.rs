use super::*;

pub(super) use kasane_render::gpu::{
    compose_affine, inverse_affine, quad_indices, quad_vertices, triangle_indices,
};

pub(super) fn vertices_for(
    drawable: &Drawable,
    frame: &DrawableFrame,
    viewport: ViewportConfig,
) -> Vec<Vertex> {
    kasane_render::gpu::transformed_vertices_for(drawable, frame, viewport)
}

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
        mask_a: [1.0, 0.0, 0.0, 0.0],
        mask_b: [0.0, 1.0, 0.0, 0.0],
        mask_origin: [0.0; 4],
    }
}

pub(super) fn with_view_transform(mut uniform: DrawUniform, transform: Affine2) -> DrawUniform {
    uniform.view_a = [transform.a.x, transform.a.y, 0.0, 0.0];
    uniform.view_b = [transform.b.x, transform.b.y, 0.0, 0.0];
    uniform.view_origin = [transform.origin.x, transform.origin.y, 0.0, 0.0];
    uniform
}

pub(super) fn with_mask_transform(mut uniform: DrawUniform, transform: Affine2) -> DrawUniform {
    uniform.mask_a = [transform.a.x, transform.a.y, 0.0, 0.0];
    uniform.mask_b = [transform.b.x, transform.b.y, 0.0, 0.0];
    uniform.mask_origin = [transform.origin.x, transform.origin.y, 0.0, 0.0];
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
