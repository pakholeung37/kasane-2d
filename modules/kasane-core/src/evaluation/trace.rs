//! Optional, same-evaluation geometry evidence for inspection tools.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::document::Document;
use crate::types::{Status, TransformKind, Vec2, VertexId};

use super::transforms::TransformState;
use super::types::DrawableFrame;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EvaluationTrace {
    pub coordinate_space: String,
    pub meshes: Vec<MeshTrace>,
    pub transforms: Vec<TransformTrace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshTrace {
    pub id: String,
    pub enabled: bool,
    pub visible: bool,
    pub vertex_ids: Vec<VertexId>,
    pub positions: Vec<Vec2>,
    pub triangles: Vec<[VertexId; 3]>,
    pub topology_hash: String,
    pub includes_blendshape_and_glue: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformTrace {
    pub id: String,
    pub kind: TransformKind,
    pub enabled: bool,
    pub parent_chain: Vec<String>,
    pub rows: Option<u32>,
    pub columns: Option<u32>,
    pub control_points: Vec<Vec2>,
    /// Sampled through the actual parent transform, including warp curvature.
    pub rotation_axis_samples: Vec<Vec2>,
    pub reflection_parity: bool,
    pub control_point_identity: String,
}

pub(super) fn build_trace(
    doc: &Document,
    frame: &DrawableFrame,
    states: &[TransformState],
    axes: &[Vec<Vec2>],
) -> Result<EvaluationTrace, Status> {
    let mut trace = EvaluationTrace {
        coordinate_space: "runtime_canvas_units_y_up".into(),
        ..EvaluationTrace::default()
    };
    for drawable in &frame.drawables {
        let mesh = doc.get_mesh(&drawable.id).ok_or_else(|| {
            Status::error(
                "TRACE_TOPOLOGY_MISMATCH",
                format!("Missing mesh {}", drawable.id),
            )
        })?;
        if mesh.vertex_ids.len() != drawable.positions.len()
            || mesh.vertex_ids.len() != mesh.base_positions.len()
            || drawable.indices.len() != mesh.triangles.len() * 3
        {
            return Err(Status::error("TRACE_TOPOLOGY_MISMATCH", &mesh.id));
        }
        let lookup: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        if lookup.len() != mesh.vertex_ids.len() {
            return Err(Status::error("TRACE_TOPOLOGY_MISMATCH", &mesh.id));
        }
        for (triangle, indices) in mesh
            .triangles
            .iter()
            .zip(drawable.indices.as_chunks::<3>().0)
        {
            let mut expected = triangle.map(|id| lookup.get(&id).copied());
            let mut actual = indices
                .iter()
                .map(|index| Some(*index as usize))
                .collect::<Vec<_>>();
            expected.sort();
            actual.sort();
            // Rendering flips winding for its y-up runtime space. The trace
            // retains the ordered authoring triple as triangle identity.
            if expected.as_slice() != actual.as_slice() {
                return Err(Status::error("TRACE_TOPOLOGY_MISMATCH", &mesh.id));
            }
        }
        // Match the O2 object-table topology hash so a triangle key and its
        // object details refer to the same topology version.
        let mut topology_bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "triangles": mesh.triangles,
            "vertex_ids": mesh.vertex_ids,
        }))
        .expect("integer topology serializes");
        topology_bytes.push(b'\n');
        trace.meshes.push(MeshTrace {
            id: mesh.id.clone(),
            enabled: drawable.enabled,
            visible: drawable.visible,
            vertex_ids: mesh.vertex_ids.clone(),
            positions: drawable.positions.clone(),
            triangles: mesh.triangles.clone(),
            topology_hash: format!("{:x}", Sha256::digest(topology_bytes)),
            includes_blendshape_and_glue: true,
        });
    }
    let prepared = doc.prepared_evaluation().map_err(Clone::clone)?;
    for (slot, id) in prepared.transforms.iter().enumerate() {
        let transform = doc.get_transform(id).expect("prepared transform exists");
        let state = &states[slot];
        let mut parent_chain = Vec::new();
        let mut parent = transform.parent();
        while !parent.is_empty() {
            parent_chain.push(parent.to_owned());
            parent = doc
                .get_transform(parent)
                .expect("validated parent exists")
                .parent();
        }
        let control_points = state
            .points
            .as_chunks::<2>()
            .0
            .iter()
            .map(|xy| Vec2::new(xy[0], xy[1]))
            .collect();
        trace.transforms.push(TransformTrace {
            id: id.clone(),
            kind: transform.kind(),
            enabled: state.enabled,
            parent_chain,
            rows: transform.warp().map(|warp| warp.rows),
            columns: transform.warp().map(|warp| warp.columns),
            control_points,
            rotation_axis_samples: axes.get(slot).cloned().unwrap_or_default(),
            reflection_parity: transform.kind() == TransformKind::Rotation
                && (state.pose.reflect_x ^ state.pose.reflect_y),
            control_point_identity: "deformer_topology_local_index".into(),
        });
    }
    Ok(trace)
}
