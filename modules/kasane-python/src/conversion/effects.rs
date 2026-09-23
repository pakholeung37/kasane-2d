use super::*;

pub(crate) fn glue_from_tuple(data: GlueDataTuple, runtime_id: String) -> Glue {
    let (id, name, mesh_a_id, mesh_b_id, pairs, intensity, binding) = data;
    Glue {
        id,
        runtime_id,
        name,
        mesh_a_id,
        mesh_b_id,
        pairs: pairs
            .into_iter()
            .map(|(vertex_a, vertex_b, weight_a, weight_b)| GlueVertexPair {
                vertex_a,
                vertex_b,
                weight_a,
                weight_b,
            })
            .collect(),
        intensity,
        binding: binding.map(|(axes, forms)| GlueBinding {
            axes: axes
                .into_iter()
                .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                .collect(),
            keyforms: forms
                .into_iter()
                .map(|intensity| GlueKeyform { intensity })
                .collect(),
        }),
    }
}

pub(crate) fn glue_tuple(value: Glue, version: Version) -> GlueTuple {
    (
        value.id,
        value.runtime_id,
        value.name,
        value.mesh_a_id,
        value.mesh_b_id,
        value
            .pairs
            .into_iter()
            .map(|pair| (pair.vertex_a, pair.vertex_b, pair.weight_a, pair.weight_b))
            .collect(),
        value.intensity,
        value.binding.map(|binding| {
            (
                binding
                    .axes
                    .into_iter()
                    .map(|axis| (axis.parameter_id, axis.keys))
                    .collect(),
                binding
                    .keyforms
                    .into_iter()
                    .map(|form| form.intensity)
                    .collect(),
            )
        }),
        version_tuple(version),
    )
}

pub(crate) fn offscreen_from_tuple(data: OffscreenDataTuple, runtime_id: String) -> Offscreen {
    let (id, name, part_id, blend_mode, flags, masks, indices, keyforms) = data;
    Offscreen {
        id,
        runtime_id,
        name,
        part_id,
        blend_mode,
        flags,
        masks,
        part_keyform_indices: indices,
        keyforms: keyforms
            .into_iter()
            .map(|(opacity, multiply, screen)| OffscreenKeyform {
                opacity,
                multiply: multiply.map(|(r, g, b)| [r, g, b]),
                screen: screen.map(|(r, g, b)| [r, g, b]),
            })
            .collect(),
    }
}

pub(crate) fn offscreen_tuple(value: Offscreen, version: Version) -> OffscreenTuple {
    (
        value.id,
        value.runtime_id,
        value.name,
        value.part_id,
        value.blend_mode,
        value.flags,
        value.masks,
        value.part_keyform_indices,
        value
            .keyforms
            .into_iter()
            .map(|form| {
                (
                    form.opacity,
                    form.multiply.map(|rgb| (rgb[0], rgb[1], rgb[2])),
                    form.screen.map(|rgb| (rgb[0], rgb[1], rgb[2])),
                )
            })
            .collect(),
        version_tuple(version),
    )
}
