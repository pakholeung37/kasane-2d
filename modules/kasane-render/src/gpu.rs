//! CPU geometry and mask layout shared by GPU backends.

use bytemuck::{Pod, Zeroable};
use kasane_core::evaluation::{Drawable, DrawableFrame};
use kasane_core::types::{Status, Vec2};

use crate::{Affine2, ViewportConfig};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub mask_point: [f32; 2],
}

pub fn vertices_for(drawable: &Drawable, frame: &DrawableFrame) -> Vec<Vertex> {
    drawable
        .positions
        .iter()
        .zip(drawable.uvs.iter())
        .map(|(position, uv)| {
            let pixel = Vec2::new(
                position.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                frame.canvas.origin.y - position.y * frame.canvas.pixels_per_unit,
            );
            Vertex {
                position: [pixel.x, pixel.y],
                uv: [uv.x, 1.0 - uv.y],
                mask_point: [pixel.x, pixel.y],
            }
        })
        .collect()
}

pub fn transformed_vertices_for(
    drawable: &Drawable,
    frame: &DrawableFrame,
    viewport: ViewportConfig,
) -> Vec<Vertex> {
    let mut vertices = vertices_for(drawable, frame);
    for vertex in &mut vertices {
        let point = viewport
            .transform
            .transform_point(Vec2::new(vertex.position[0], vertex.position[1]));
        vertex.position = [point.x, point.y];
    }
    vertices
}

pub fn triangle_indices(indices: &[u32]) -> Vec<u32> {
    let (triangles, _) = indices.as_chunks::<3>();
    triangles
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect()
}

pub fn quad_vertices(size: (u32, u32), transform: Affine2, mask_transform: Affine2) -> Vec<Vertex> {
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

pub fn quad_indices() -> Vec<u32> {
    triangle_indices(&[0, 1, 2, 0, 2, 3])
}

pub fn inverse_affine(transform: Affine2) -> Result<Affine2, Status> {
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

pub fn compose_affine(lhs: Affine2, rhs: Affine2) -> Affine2 {
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

#[derive(Clone, Copy, Debug)]
pub struct MaskLayout {
    pub width: u32,
    pub height: u32,
    pub origin: Vec2,
    pub logical_size: Vec2,
    pub scale: f32,
}

pub fn mask_layout(
    frame: &DrawableFrame,
    source_ids: &[String],
    requested_scale: f64,
    max_dimension: u32,
) -> Result<MaskLayout, Status> {
    if !requested_scale.is_finite() || requested_scale <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask scale must be finite and positive.",
        ));
    }
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for source_id in source_ids {
        let drawable = frame
            .drawables
            .iter()
            .find(|drawable| drawable.id == *source_id)
            .ok_or_else(|| Status::error("INVALID_MASK", source_id))?;
        for position in &drawable.positions {
            let point = Vec2::new(
                position.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                frame.canvas.origin.y - position.y * frame.canvas.pixels_per_unit,
            );
            bounds = Some(bounds.map_or((point, point), |(min, max)| {
                (
                    Vec2::new(min.x.min(point.x), min.y.min(point.y)),
                    Vec2::new(max.x.max(point.x), max.y.max(point.y)),
                )
            }));
        }
    }
    let (min, max) = bounds.unwrap_or((Vec2::default(), Vec2::default()));
    let origin = Vec2::new(min.x - 4.0, min.y - 4.0);
    let size_x = (max.x - min.x + 8.0).ceil().max(1.0);
    let size_y = (max.y - min.y + 8.0).ceil().max(1.0);
    let scale = (requested_scale as f32).min(max_dimension as f32 / size_x.max(size_y));
    let width = (size_x * scale).ceil().max(1.0);
    let height = (size_y * scale).ceil().max(1.0);
    Ok(MaskLayout {
        width: width as u32,
        height: height as u32,
        origin,
        logical_size: Vec2::new(width / scale, height / scale),
        scale,
    })
}
