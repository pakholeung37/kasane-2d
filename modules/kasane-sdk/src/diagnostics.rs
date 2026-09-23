use std::collections::HashMap;

use kasane_core::Vec2;

use crate::{AuthoringSession, SdkError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryBounds {
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryChecks {
    /// Area in source coordinates: canvas pixels for root meshes, parent-local
    /// units for meshes with a deform parent.
    pub min_triangle_area: f64,
    /// Optional expected source bounds for root meshes only.
    pub canvas_bounds: Option<GeometryBounds>,
}

impl Default for GeometryChecks {
    fn default() -> Self {
        Self {
            min_triangle_area: 0.0,
            canvas_bounds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryDiagnosticKind {
    SmallTriangle,
    InconsistentWinding,
    OutsideCanvasBounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryDiagnostic {
    pub kind: GeometryDiagnosticKind,
    pub mesh_id: String,
    pub triangle_index: Option<usize>,
}

impl AuthoringSession {
    /// Advisory source-geometry checks. These findings never reject an edit.
    pub fn diagnose_geometry(
        &self,
        checks: GeometryChecks,
    ) -> Result<Vec<GeometryDiagnostic>, SdkError> {
        if !checks.min_triangle_area.is_finite() || checks.min_triangle_area < 0.0 {
            return Err(SdkError::new(
                "INVALID_DIAGNOSTIC_OPTIONS",
                "Minimum triangle area must be finite and non-negative",
                "diagnose_geometry",
            ));
        }
        if let Some(bounds) = checks.canvas_bounds {
            let finite = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
                .into_iter()
                .all(f32::is_finite);
            if !finite || bounds.min.x >= bounds.max.x || bounds.min.y >= bounds.max.y {
                return Err(SdkError::new(
                    "INVALID_DIAGNOSTIC_OPTIONS",
                    "Canvas bounds require finite min below max",
                    "diagnose_geometry",
                ));
            }
        }
        if let Some(issue) = self.validate_structure().into_iter().next() {
            return Err(SdkError::from_status(
                issue.status,
                "diagnose_geometry",
                vec![issue.object_id],
            ));
        }
        let mut diagnostics = Vec::new();
        for mesh_id in self.mesh_ids() {
            let mesh = self.mesh(mesh_id).expect("validated mesh exists");
            let slots: HashMap<_, _> = mesh
                .vertex_ids
                .iter()
                .enumerate()
                .map(|(index, &vertex)| (vertex, index))
                .collect();
            let mut signed_areas = Vec::with_capacity(mesh.triangles.len());
            let mut positive = 0usize;
            let mut negative = 0usize;
            for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
                let a = mesh.base_positions[slots[&triangle[0]]];
                let b = mesh.base_positions[slots[&triangle[1]]];
                let c = mesh.base_positions[slots[&triangle[2]]];
                let cross = (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
                    - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64);
                let area = cross * 0.5;
                if area.abs() <= checks.min_triangle_area {
                    diagnostics.push(GeometryDiagnostic {
                        kind: GeometryDiagnosticKind::SmallTriangle,
                        mesh_id: mesh_id.clone(),
                        triangle_index: Some(triangle_index),
                    });
                } else if area > 0.0 {
                    positive += 1;
                } else {
                    negative += 1;
                }
                signed_areas.push(area);
            }
            let expected_positive = positive >= negative;
            if positive > 0 && negative > 0 {
                for (triangle_index, area) in signed_areas.into_iter().enumerate() {
                    if area.abs() > checks.min_triangle_area && (area > 0.0) != expected_positive {
                        diagnostics.push(GeometryDiagnostic {
                            kind: GeometryDiagnosticKind::InconsistentWinding,
                            mesh_id: mesh_id.clone(),
                            triangle_index: Some(triangle_index),
                        });
                    }
                }
            }
            if mesh.deformer_id.is_empty()
                && checks.canvas_bounds.is_some_and(|bounds| {
                    mesh.base_positions.iter().any(|p| {
                        p.x < bounds.min.x
                            || p.x > bounds.max.x
                            || p.y < bounds.min.y
                            || p.y > bounds.max.y
                    })
                })
            {
                diagnostics.push(GeometryDiagnostic {
                    kind: GeometryDiagnosticKind::OutsideCanvasBounds,
                    mesh_id: mesh_id.clone(),
                    triangle_index: None,
                });
            }
        }
        Ok(diagnostics)
    }
}
