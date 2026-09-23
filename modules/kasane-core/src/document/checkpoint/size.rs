//! Capacity-aware estimate of persistent content allocations. Keep this in
//! sync with DocumentContent and the value types stored in its collections.
use std::collections::HashMap;
use std::mem::size_of;

use crate::draw_order::DrawOrderGroup;
use crate::types::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DeltaGlueKeyform,
    DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform,
    DeltaWarpKeyform, Glue, GlueBinding, GlueKeyform, GlueVertexPair, ImageAsset, Mesh,
    MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, Parameter, Part, PartKeyform,
    RotationKeyform, SceneBinding, SceneTrack, Transform, TransformData, Vec2, WarpKeyform,
};

use super::super::{ContentRef, DocumentContent};

trait ExtraBytes {
    fn extra_bytes(&self) -> usize;
}

impl ExtraBytes for String {
    fn extra_bytes(&self) -> usize {
        self.capacity()
    }
}
impl<T: ExtraBytes> ExtraBytes for Vec<T> {
    fn extra_bytes(&self) -> usize {
        self.capacity() * size_of::<T>() + self.iter().map(ExtraBytes::extra_bytes).sum::<usize>()
    }
}
impl<T: ExtraBytes> ExtraBytes for Option<T> {
    fn extra_bytes(&self) -> usize {
        self.as_ref().map_or(0, ExtraBytes::extra_bytes)
    }
}
impl<T: ExtraBytes> ExtraBytes for HashMap<String, T> {
    fn extra_bytes(&self) -> usize {
        // Hashbrown's control-byte allocation is included approximately.
        self.capacity() * (size_of::<(String, T)>() + 1)
            + self
                .iter()
                .map(|(key, value)| key.capacity() + value.extra_bytes())
                .sum::<usize>()
    }
}

macro_rules! no_extra {
    ($($ty:ty),+ $(,)?) => {$(
        impl ExtraBytes for $ty { fn extra_bytes(&self) -> usize { 0 } }
    )+};
}
no_extra!(
    f32,
    i32,
    u32,
    Vec2,
    GlueVertexPair,
    GlueKeyform,
    OffscreenKeyform,
    DeltaRotationKeyform,
    DeltaPartKeyform,
    DeltaGlueKeyform,
    DeltaOffscreenKeyform
);

impl ExtraBytes for ImageAsset {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.name.capacity() + self.source.capacity() + self.sha256.capacity()
    }
}
impl ExtraBytes for Part {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.parent_id.capacity()
    }
}
impl ExtraBytes for Transform {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.part_id.as_ref().map_or(0, |id| id.capacity())
            + self.parent_id.as_ref().map_or(0, |id| id.capacity())
            + match &self.data {
                TransformData::Warp(w) => w.points.extra_bytes(),
                TransformData::Rotation(_) => 0,
            }
    }
}
impl ExtraBytes for Mesh {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.name.capacity()
            + self.texture_asset_id.capacity()
            + self.vertex_ids.extra_bytes()
            + self.base_positions.extra_bytes()
            + self.uvs.extra_bytes()
            + self.triangles.capacity() * size_of::<[u32; 3]>()
            + self.runtime_id.capacity()
            + self.part_id.capacity()
            + self.deformer_id.capacity()
            + self.masks.extra_bytes()
    }
}
impl ExtraBytes for Parameter {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.runtime_id.capacity() + self.name.capacity()
    }
}
impl ExtraBytes for BindingAxis {
    fn extra_bytes(&self) -> usize {
        self.parameter_id.capacity() + self.keys.extra_bytes()
    }
}
impl ExtraBytes for MeshKeyform {
    fn extra_bytes(&self) -> usize {
        self.keys.extra_bytes() + self.positions.extra_bytes()
    }
}
impl ExtraBytes for MeshBinding {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.mesh_id.capacity()
            + self.axes.extra_bytes()
            + self.keyforms.extra_bytes()
    }
}
impl ExtraBytes for WarpKeyform {
    fn extra_bytes(&self) -> usize {
        self.keys.extra_bytes() + self.positions.extra_bytes()
    }
}
impl ExtraBytes for RotationKeyform {
    fn extra_bytes(&self) -> usize {
        self.keys.extra_bytes()
    }
}
impl ExtraBytes for PartKeyform {
    fn extra_bytes(&self) -> usize {
        self.keys.extra_bytes()
    }
}
impl ExtraBytes for SceneTrack {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Warp {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
            Self::Rotation {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
            Self::Part {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
        }
    }
}
impl ExtraBytes for SceneBinding {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.axes.extra_bytes() + self.track.extra_bytes()
    }
}
impl ExtraBytes for DrawOrderGroup {
    fn extra_bytes(&self) -> usize {
        self.owner.capacity() + self.items.extra_bytes()
    }
}
impl ExtraBytes for BlendShapeKeyTable {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.parameter_id.capacity() + self.keys.extra_bytes()
    }
}
impl ExtraBytes for BlendShapeConstraint {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.parameter_id.capacity()
            + self.keys.extra_bytes()
            + self.weights.extra_bytes()
    }
}
impl ExtraBytes for DeltaMeshKeyform {
    fn extra_bytes(&self) -> usize {
        self.positions.extra_bytes()
    }
}
impl ExtraBytes for DeltaWarpKeyform {
    fn extra_bytes(&self) -> usize {
        self.points.extra_bytes()
    }
}
impl ExtraBytes for DeltaKeyforms {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Mesh(v) => v.extra_bytes(),
            Self::Warp(v) => v.extra_bytes(),
            Self::Rotation(v) => v.extra_bytes(),
            Self::Part(v) => v.extra_bytes(),
            Self::Glue(v) => v.extra_bytes(),
            Self::Offscreen(v) => v.extra_bytes(),
        }
    }
}
impl ExtraBytes for BlendShapeBinding {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.target_id.capacity()
            + self.key_table_id.capacity()
            + self.constraint_ids.extra_bytes()
            + self.keyforms.extra_bytes()
    }
}
impl ExtraBytes for GlueBinding {
    fn extra_bytes(&self) -> usize {
        self.axes.extra_bytes() + self.keyforms.extra_bytes()
    }
}
impl ExtraBytes for Glue {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.mesh_a_id.capacity()
            + self.mesh_b_id.capacity()
            + self.pairs.extra_bytes()
            + self.binding.extra_bytes()
    }
}
impl ExtraBytes for Offscreen {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.part_id.capacity()
            + self.masks.extra_bytes()
            + self.part_keyform_indices.extra_bytes()
            + self.keyforms.extra_bytes()
    }
}

pub(super) fn estimated_content_bytes(content: ContentRef<'_>) -> usize {
    size_of::<DocumentContent>()
        + content.id.extra_bytes()
        + content.assets.extra_bytes()
        + content.asset_order.extra_bytes()
        + content.parts.extra_bytes()
        + content.part_order.extra_bytes()
        + content.transforms.extra_bytes()
        + content.transform_order.extra_bytes()
        + content.meshes.extra_bytes()
        + content.mesh_order.extra_bytes()
        + content.parameters.extra_bytes()
        + content.parameter_order.extra_bytes()
        + content.bindings.extra_bytes()
        + content.binding_order.extra_bytes()
        + content.scene_bindings.extra_bytes()
        + content.scene_binding_order.extra_bytes()
        + content.draw_order_groups.extra_bytes()
        + content.blend_key_tables.extra_bytes()
        + content.blend_key_table_order.extra_bytes()
        + content.blend_constraints.extra_bytes()
        + content.blend_constraint_order.extra_bytes()
        + content.blend_bindings.extra_bytes()
        + content.blend_binding_order.extra_bytes()
        + content.glues.extra_bytes()
        + content.glue_order.extra_bytes()
        + content.offscreens.extra_bytes()
        + content.offscreen_order.extra_bytes()
}
