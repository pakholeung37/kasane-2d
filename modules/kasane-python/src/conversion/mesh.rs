use super::*;

pub(crate) fn mesh_from_record(
    data: MeshRecordDataTuple,
    runtime_id: String,
) -> PyResult<kasane_core::Mesh> {
    let (id, name, texture_asset_id, geometry, (part_id, deformer_id), appearance, drawing) = data;
    let (vertex_ids, positions, uvs, triangles) = geometry;
    let (draw_order, blend_mode, enabled, double_sided, inverted_mask, masks, raw_blend_mode) =
        drawing;
    Ok(kasane_core::Mesh {
        id,
        name,
        texture_asset_id,
        vertex_ids,
        base_positions: positions
            .into_iter()
            .map(|(x, y)| Vec2::new(x, y))
            .collect(),
        uvs: uvs.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
        triangles: triangles.into_iter().map(|(a, b, c)| [a, b, c]).collect(),
        runtime_id,
        part_id,
        deformer_id,
        appearance: appearance_from_tuple(appearance),
        draw_order,
        blend_mode: blend_mode_from_name(&blend_mode)?,
        enabled,
        double_sided,
        inverted_mask,
        masks,
        raw_blend_mode,
    })
}

pub(crate) fn mesh_record_tuple(mesh: kasane_core::Mesh, version: Version) -> MeshRecordTuple {
    (
        (
            mesh.id,
            mesh.name,
            mesh.texture_asset_id,
            (
                mesh.vertex_ids,
                mesh.base_positions
                    .into_iter()
                    .map(|p| (p.x, p.y))
                    .collect(),
                mesh.uvs.into_iter().map(|p| (p.x, p.y)).collect(),
                mesh.triangles
                    .into_iter()
                    .map(|v| (v[0], v[1], v[2]))
                    .collect(),
            ),
            (mesh.part_id, mesh.deformer_id),
            appearance_tuple(mesh.appearance),
            (
                mesh.draw_order,
                blend_mode_name(mesh.blend_mode).to_owned(),
                mesh.enabled,
                mesh.double_sided,
                mesh.inverted_mask,
                mesh.masks,
                mesh.raw_blend_mode,
            ),
        ),
        mesh.runtime_id,
        version_tuple(version),
    )
}

pub(crate) fn version_tuple(version: Version) -> (u64, u64, u64) {
    (version.session_id, version.generation, version.revision)
}

pub(crate) fn tuple_version(value: (u64, u64, u64)) -> Version {
    Version {
        session_id: value.0,
        generation: value.1,
        revision: value.2,
    }
}

pub(crate) fn mesh_tuple(mesh: kasane_core::Mesh, version: Version) -> MeshTuple {
    (
        mesh.id,
        mesh.name,
        mesh.vertex_ids,
        mesh.base_positions
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect(),
        version_tuple(version),
    )
}

pub(crate) fn binding_tuple(binding: MeshBinding, version: Version) -> MeshBindingTuple {
    (
        binding.id,
        binding.mesh_id,
        binding
            .axes
            .into_iter()
            .map(|axis| (axis.parameter_id, axis.keys))
            .collect(),
        binding
            .keyforms
            .into_iter()
            .map(|form| {
                (
                    form.keys,
                    form.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                    appearance_tuple(form.appearance),
                    form.draw_order,
                )
            })
            .collect(),
        version_tuple(version),
    )
}
