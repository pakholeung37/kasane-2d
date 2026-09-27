//! Geometry queries over one evaluated frame and its authoring object table.
//! No renderer or texture data is required.

use std::collections::{HashMap, HashSet};

use kasane_core::{DrawableFrame, Mesh, Part, Vec2};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
/// A mesh or Part selected by its stable authoring ID.
///
/// A Part selects every descendant mesh, including meshes in nested Parts.
pub enum ObjectTarget {
    /// Select exactly one mesh.
    Mesh(String),
    /// Select all descendant meshes of a Part.
    Part(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A missing target or invalid hit-test input.
pub enum SpatialError {
    /// A requested mesh ID is absent from the authoring document.
    MissingMesh(String),
    /// A requested Part ID is absent from the authoring document.
    MissingPart(String),
    /// The query point contains a non-finite coordinate.
    InvalidPoint,
    /// The candidate limit is zero.
    InvalidLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Bounds of evaluated target vertices in source-canvas pixel coordinates.
///
/// These geometric bounds can extend outside the canvas and do not account for
/// textures, masks, or occlusion.
pub struct ObjectBounds {
    /// Resolved mesh IDs in authoring order, including meshes filtered from the bounds.
    pub mesh_ids: Vec<String>,
    /// `[min_x, min_y, max_x, max_y]`, or `None` when no vertices contribute.
    pub canvas_bounds: Option<[f64; 4]>,
    /// Why `canvas_bounds` is absent: `no_descendant_mesh`, `filtered_out`, or
    /// `no_evaluated_positions`.
    pub empty_reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// One evaluated triangle containing the query point.
pub struct TriangleHit {
    /// The triangle's zero-based position in the drawable's index buffer.
    pub triangle_index: usize,
    /// Offsets into the evaluated drawable's vertex arrays.
    pub vertex_indices: [u32; 3],
    /// Stable authoring vertex IDs, when present.
    pub vertex_ids: Option<[u32; 3]>,
    /// Interpolation weights corresponding to `vertex_indices`.
    pub barycentric: [f64; 3],
    /// UV coordinate interpolated from the triangle's evaluated UVs.
    pub uv: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// One mesh whose evaluated geometry contains the query point.
///
/// A hit is a geometric candidate, not proof that the mesh painted a visible
/// pixel. `part_path` and `part_names` run from root to leaf.
pub struct GeometryHit {
    /// Stable authoring mesh ID.
    pub mesh_id: String,
    /// Authoring mesh display name.
    pub name: String,
    /// Ancestor Part IDs, from root to leaf.
    pub part_path: Vec<String>,
    /// Ancestor Part display names, parallel to `part_path`.
    pub part_names: Vec<String>,
    /// Evaluated drawable enabled state.
    pub enabled: bool,
    /// Evaluated drawable visibility state.
    pub visible: bool,
    /// Evaluated drawable opacity.
    pub opacity: f32,
    /// Evaluated draw order, used for stable descending candidate ordering.
    /// This is not a visibility ranking across offscreen composition groups.
    pub render_order: i32,
    /// Containing triangles; populated only when `details` was requested.
    pub triangles: Vec<TriangleHit>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Mesh candidates for a source-canvas point, ordered by descending draw order.
pub struct GeometryQuery {
    /// Candidates retained after applying `max_candidates`.
    pub hits: Vec<GeometryHit>,
    /// Number of candidates before applying the limit.
    pub total: usize,
    /// Whether any candidates were omitted by the limit.
    pub truncated: bool,
}

/// Resolve target IDs in authoring mesh order. Missing IDs are errors; an
/// existing Part without descendant meshes resolves to an empty vector.
pub fn resolve_mesh_ids(
    meshes: &[Mesh],
    parts: &[Part],
    targets: &[ObjectTarget],
) -> Result<Vec<String>, SpatialError> {
    let mesh_by_id: HashSet<_> = meshes.iter().map(|mesh| mesh.id.as_str()).collect();
    let part_by_id: HashSet<_> = parts.iter().map(|part| part.id.as_str()).collect();
    let mut selected: HashSet<&str> = HashSet::new();
    let mut descendants: HashSet<&str> = HashSet::new();
    for target in targets {
        match target {
            ObjectTarget::Mesh(id) => {
                if !mesh_by_id.contains(id.as_str()) {
                    return Err(SpatialError::MissingMesh(id.clone()));
                }
                selected.insert(id.as_str());
            }
            ObjectTarget::Part(id) => {
                if !part_by_id.contains(id.as_str()) {
                    return Err(SpatialError::MissingPart(id.clone()));
                }
                descendants.insert(id.as_str());
            }
        }
    }
    loop {
        let before = descendants.len();
        for part in parts {
            if descendants.contains(part.parent_id.as_str()) {
                descendants.insert(part.id.as_str());
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    for mesh in meshes {
        if descendants.contains(mesh.part_id.as_str()) {
            selected.insert(mesh.id.as_str());
        }
    }
    Ok(meshes
        .iter()
        .filter(|mesh| selected.contains(mesh.id.as_str()))
        .map(|mesh| mesh.id.clone())
        .collect())
}

fn draws(drawable: &kasane_core::Drawable) -> bool {
    drawable.enabled && drawable.visible && drawable.opacity > 0.0
}

fn canvas_point(point: Vec2, frame: &DrawableFrame) -> [f64; 2] {
    let canvas = frame.canvas;
    [
        f64::from(point.x) * f64::from(canvas.pixels_per_unit) + f64::from(canvas.origin.x),
        f64::from(canvas.origin.y) - f64::from(point.y) * f64::from(canvas.pixels_per_unit),
    ]
}

/// Union the evaluated positions of the selected meshes in source-canvas pixels.
///
/// With `include_hidden = false`, drawables that are disabled, invisible, or
/// fully transparent do not contribute. An empty result reports its reason in
/// [`ObjectBounds::empty_reason`]. The result is not clipped to the canvas.
pub fn object_bounds(
    frame: &DrawableFrame,
    meshes: &[Mesh],
    parts: &[Part],
    targets: &[ObjectTarget],
    include_hidden: bool,
) -> Result<ObjectBounds, SpatialError> {
    let mesh_ids = resolve_mesh_ids(meshes, parts, targets)?;
    let selected: HashSet<_> = mesh_ids.iter().map(String::as_str).collect();
    let mut bounds = None::<[f64; 4]>;
    let mut filtered = false;
    for drawable in &frame.drawables {
        if !selected.contains(drawable.id.as_str()) {
            continue;
        }
        if include_hidden || draws(drawable) {
            for position in &drawable.positions {
                let [x, y] = canvas_point(*position, frame);
                bounds = Some(match bounds {
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                    None => [x, y, x, y],
                });
            }
        } else if !drawable.positions.is_empty() {
            filtered = true;
        }
    }
    let empty_reason = if bounds.is_some() {
        None
    } else if mesh_ids.is_empty() {
        Some("no_descendant_mesh")
    } else if filtered {
        Some("filtered_out")
    } else {
        Some("no_evaluated_positions")
    };
    Ok(ObjectBounds {
        mesh_ids,
        canvas_bounds: bounds,
        empty_reason,
    })
}

fn triangle_hit(
    frame: &DrawableFrame,
    positions: &[Vec2],
    uvs: &[Vec2],
    vertex_ids: &[u32],
    indices: [u32; 3],
    triangle_index: usize,
    point: [f64; 2],
) -> Option<TriangleHit> {
    let [a, b, c] = indices.map(|index| index as usize);
    let (Some(&pa), Some(&pb), Some(&pc)) = (positions.get(a), positions.get(b), positions.get(c))
    else {
        return None;
    };
    let [pa, pb, pc] = [pa, pb, pc].map(|p| canvas_point(p, frame));
    let [x, y] = point;
    if x < pa[0].min(pb[0]).min(pc[0])
        || x > pa[0].max(pb[0]).max(pc[0])
        || y < pa[1].min(pb[1]).min(pc[1])
        || y > pa[1].max(pb[1]).max(pc[1])
    {
        return None;
    }
    let denominator = (pb[1] - pc[1]) * (pa[0] - pc[0]) + (pc[0] - pb[0]) * (pa[1] - pc[1]);
    if denominator == 0.0 {
        return None;
    }
    let wa = ((pb[1] - pc[1]) * (x - pc[0]) + (pc[0] - pb[0]) * (y - pc[1])) / denominator;
    let wb = ((pc[1] - pa[1]) * (x - pc[0]) + (pa[0] - pc[0]) * (y - pc[1])) / denominator;
    let wc = 1.0 - wa - wb;
    let weights = [wa, wb, wc];
    if weights.iter().any(|&w| !(-1e-7..=1.0 + 1e-7).contains(&w)) {
        return None;
    }
    let uv = match (uvs.get(a), uvs.get(b), uvs.get(c)) {
        (Some(a), Some(b), Some(c)) => Some([
            wa * f64::from(a.x) + wb * f64::from(b.x) + wc * f64::from(c.x),
            wa * f64::from(a.y) + wb * f64::from(b.y) + wc * f64::from(c.y),
        ]),
        _ => None,
    };
    let ids = match (vertex_ids.get(a), vertex_ids.get(b), vertex_ids.get(c)) {
        (Some(&a), Some(&b), Some(&c)) => Some([a, b, c]),
        _ => None,
    };
    Some(TriangleHit {
        triangle_index,
        vertex_indices: indices,
        vertex_ids: ids,
        barycentric: weights,
        uv,
    })
}

/// Find meshes with evaluated triangles containing a source-canvas point.
///
/// Degenerate triangles are skipped. With `include_hidden = false`, disabled,
/// invisible, and fully transparent drawables are skipped. `details` includes
/// all matching triangles per mesh; otherwise only mesh candidates are returned.
/// Results are sorted by draw order and limited to `max_candidates`, but do not
/// test textures, masks, or occlusion.
pub fn hit_test_geometry(
    frame: &DrawableFrame,
    meshes: &[Mesh],
    parts: &[Part],
    point: [f64; 2],
    include_hidden: bool,
    details: bool,
    max_candidates: usize,
) -> Result<GeometryQuery, SpatialError> {
    if !point.into_iter().all(f64::is_finite) {
        return Err(SpatialError::InvalidPoint);
    }
    if max_candidates == 0 {
        return Err(SpatialError::InvalidLimit);
    }
    let mesh_by_id: HashMap<_, _> = meshes.iter().map(|mesh| (mesh.id.as_str(), mesh)).collect();
    let part_by_id: HashMap<_, _> = parts.iter().map(|part| (part.id.as_str(), part)).collect();
    let mut found = Vec::new();
    for (sequence, drawable) in frame.drawables.iter().enumerate() {
        if !include_hidden && !draws(drawable) {
            continue;
        }
        let mesh = mesh_by_id.get(drawable.id.as_str()).copied();
        let mut triangles = Vec::new();
        for (index, triple) in drawable.indices.as_chunks::<3>().0.iter().enumerate() {
            let indices = [triple[0], triple[1], triple[2]];
            if let Some(hit) = triangle_hit(
                frame,
                &drawable.positions,
                &drawable.uvs,
                mesh.map_or(&[], |mesh| mesh.vertex_ids.as_slice()),
                indices,
                index,
                point,
            ) {
                triangles.push(hit);
                if !details {
                    break;
                }
            }
        }
        if triangles.is_empty() {
            continue;
        }
        let mut part_path = Vec::new();
        let mut part_names = Vec::new();
        let mut part_id = drawable.part_id.as_str();
        let mut seen = HashSet::new();
        while let Some(part) = part_by_id.get(part_id).copied() {
            if !seen.insert(part_id) {
                break;
            }
            part_path.push(part.id.clone());
            part_names.push(part.name.clone());
            part_id = part.parent_id.as_str();
        }
        part_path.reverse();
        part_names.reverse();
        found.push((
            drawable.render_order,
            sequence,
            GeometryHit {
                mesh_id: drawable.id.clone(),
                name: mesh.map_or_else(String::new, |mesh| mesh.name.clone()),
                part_path,
                part_names,
                enabled: drawable.enabled,
                visible: drawable.visible,
                opacity: drawable.opacity,
                render_order: drawable.render_order,
                triangles: if details { triangles } else { Vec::new() },
            },
        ));
    }
    found.sort_by_key(|item| std::cmp::Reverse((item.0, item.1)));
    let total = found.len();
    Ok(GeometryQuery {
        hits: found
            .into_iter()
            .take(max_candidates)
            .map(|(_, _, hit)| hit)
            .collect(),
        total,
        truncated: total > max_candidates,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kasane_core::{Canvas, Drawable, DrawableFrame, Mesh, Part, Vec2};

    use super::*;

    #[test]
    fn part_bounds_and_triangle_hits_share_evaluated_geometry() {
        let parts = vec![
            Part {
                id: "head".into(),
                name: "Head".into(),
                ..Part::default()
            },
            Part {
                id: "eye-group".into(),
                parent_id: "head".into(),
                name: "Eyes".into(),
                ..Part::default()
            },
        ];
        let mesh = Mesh {
            id: "eye".into(),
            name: "Eye".into(),
            part_id: "eye-group".into(),
            vertex_ids: vec![10, 11, 12, 13],
            ..Mesh::default()
        };
        let drawable = Drawable {
            id: "eye".into(),
            part_id: "eye-group".into(),
            positions: vec![
                Vec2::new(-10.0, 10.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(-10.0, -10.0),
            ],
            indices: Arc::from(vec![0, 2, 1, 0, 3, 2]),
            uvs: Arc::from(vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ]),
            ..Drawable::default()
        };
        let mut frame = DrawableFrame {
            canvas: Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 1.0),
            drawables: vec![drawable],
            ..DrawableFrame::default()
        };
        let selected = [ObjectTarget::Part("head".into())];
        let bounds =
            object_bounds(&frame, std::slice::from_ref(&mesh), &parts, &selected, true).unwrap();
        assert_eq!(bounds.mesh_ids, ["eye"]);
        assert_eq!(bounds.canvas_bounds, Some([40.0, 40.0, 60.0, 60.0]));
        let hits = hit_test_geometry(
            &frame,
            std::slice::from_ref(&mesh),
            &parts,
            [50.0, 50.0],
            true,
            true,
            8,
        )
        .unwrap();
        assert_eq!(hits.total, 1);
        assert_eq!(hits.hits[0].part_path, ["head", "eye-group"]);
        assert_eq!(hits.hits[0].triangles.len(), 2); // Shared diagonal.
        assert_eq!(hits.hits[0].triangles[0].vertex_ids, Some([10, 12, 11]));
        assert_eq!(
            resolve_mesh_ids(
                std::slice::from_ref(&mesh),
                &parts,
                &[ObjectTarget::Mesh("missing".into())]
            ),
            Err(SpatialError::MissingMesh("missing".into()))
        );
        frame.drawables[0].enabled = false;
        assert_eq!(
            object_bounds(&frame, &[mesh], &parts, &selected, false)
                .unwrap()
                .empty_reason,
            Some("filtered_out")
        );
        assert_eq!(
            hit_test_geometry(&frame, &[], &parts, [50.0, 50.0], false, false, 8)
                .unwrap()
                .total,
            0
        );
        frame.drawables[0].positions.fill(Vec2::new(0.0, 0.0));
        assert_eq!(
            hit_test_geometry(&frame, &[], &parts, [50.0, 50.0], true, true, 8)
                .unwrap()
                .total,
            0
        );
    }
}
