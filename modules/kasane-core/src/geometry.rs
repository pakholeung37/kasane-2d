use crate::types::{Canvas, Status, Vec2};

pub fn validate_positions(positions: &[Vec2]) -> Status {
    for p in positions {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Status::error("NON_FINITE", "Coordinates must be finite float32 values.");
        }
    }
    Status::ok()
}

pub fn validate_render_mesh(positions: &[Vec2], uvs: &[Vec2], indices: &[u32]) -> Status {
    if positions.len() > (i32::MAX as usize) || positions.len() != uvs.len() {
        return Status::error(
            "INVALID_LENGTH",
            "A mesh needs matching positions and UVs within 32-bit limits.",
        );
    }
    let s = validate_positions(positions);
    if !s.is_ok() {
        return s;
    }
    let s = validate_positions(uvs);
    if !s.is_ok() {
        return s;
    }
    // Zero-triangle mesh: indices are empty; valid geometry without rasterized triangles
    if indices.is_empty() {
        return Status::ok();
    }
    if !indices.len().is_multiple_of(3) || indices.len() > (i32::MAX as usize) {
        return Status::error(
            "INVALID_LENGTH",
            "Triangle indices must be a multiple of three.",
        );
    }
    if positions.len() < 3 {
        return Status::error(
            "INVALID_LENGTH",
            "A mesh with triangles needs at least three vertices.",
        );
    }
    for &chunk in indices.as_chunks::<3>().0 {
        let (a, b, c) = (chunk[0] as usize, chunk[1] as usize, chunk[2] as usize);
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            return Status::error(
                "INVALID_INDEX",
                "Triangle index is outside the vertex array.",
            );
        }
        if a == b || b == c || a == c {
            return Status::error(
                "REPEATED_VERTEX",
                "A triangle must reference three distinct vertices.",
            );
        }
    }
    Status::ok()
}

pub fn is_renderable_mesh(positions: &[Vec2], indices: &[u32]) -> bool {
    !indices.is_empty() && positions.len() >= 3 && indices.len().is_multiple_of(3)
}

pub fn to_runtime_positions(canvas: Canvas, positions: &[Vec2]) -> Result<Vec<Vec2>, Status> {
    let mut result = Vec::with_capacity(positions.len());
    let ppu = canvas.pixels_per_unit as f64;
    let ox = canvas.origin.x as f64;
    let oy = canvas.origin.y as f64;
    for p in positions {
        let qx = ((p.x as f64 - ox) / ppu) as f32;
        let qy = ((oy - p.y as f64) / ppu) as f32;
        if !qx.is_finite() || !qy.is_finite() {
            return Err(Status::error(
                "NON_FINITE",
                "Position conversion overflows float32",
            ));
        }
        result.push(Vec2::new(qx, qy));
    }
    Ok(result)
}
