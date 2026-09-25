//! Capacity-aware estimate of persistent content allocations. Keep this in
//! sync with DocumentContent and the value types stored in its collections.
use std::collections::{BTreeMap, HashMap};
use std::mem::size_of;

use crate::draw_order::DrawOrderGroup;
use crate::types::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DeltaGlueKeyform,
    DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform,
    DeltaWarpKeyform, Glue, GlueBinding, GlueKeyform, GlueVertexPair, ImageAsset, Mesh,
    MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, Parameter, Part, PartKeyform,
    RotationKeyform, SceneBinding, SceneTrack, Transform, TransformData, Vec2, WarpKeyform,
};

use super::super::PackageAttachment;
use super::super::PhysicsAsset;
use super::super::{
    CdiCombinedSet, CdiNamespaceIds, CdiParameterEntry, CdiParameterGroup, CdiParameterRef,
    CdiPartEntry, DisplayInfo,
};
use super::super::{ContentRef, DocumentContent};
use super::super::{ExpressionAsset, ExpressionEntry, ExpressionTarget};
use super::super::{Model3Settings, ModelHitArea, ModelParameterGroup, ModelTargetRef};
use super::super::{
    MotionClip, MotionEvent, MotionGroup, MotionPoint, MotionRegistration, MotionSegment,
    MotionTrack, MotionTrackTarget,
};
use super::super::{PoseAsset, PoseEntry, PosePartRef};
use serde_json::Value;

trait ExtraBytes {
    fn extra_bytes(&self) -> usize;
}

impl<T: ExtraBytes> ExtraBytes for std::sync::Arc<T> {
    fn extra_bytes(&self) -> usize {
        // Charge shared allocations in each checkpoint conservatively. This
        // keeps the existing budget a safe upper bound instead of undercounting.
        2 * size_of::<usize>() + size_of::<T>() + self.as_ref().extra_bytes()
    }
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
impl<T: ExtraBytes> ExtraBytes for BTreeMap<String, T> {
    fn extra_bytes(&self) -> usize {
        // Charge a full 11-entry B-tree node for every occupied entry. This
        // deliberately overestimates small and nested maps, including a map
        // with one item, rather than undercounting spare node slots.
        let nodes = self
            .len()
            .saturating_mul(11 * size_of::<(String, T)>() + 12 * size_of::<usize>() + 64);
        nodes
            + self
                .iter()
                .map(|(key, value)| key.capacity() + value.extra_bytes())
                .sum::<usize>()
    }
}
impl ExtraBytes for Value {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::String(value) => value.capacity(),
            Self::Array(items) => items.extra_bytes(),
            Self::Object(items) => {
                let nodes = items.len().saturating_mul(
                    11 * size_of::<(String, Value)>() + 12 * size_of::<usize>() + 64,
                );
                nodes
                    + items
                        .iter()
                        .map(|(key, value)| key.capacity() + value.extra_bytes())
                        .sum::<usize>()
            }
            _ => 0,
        }
    }
}

macro_rules! no_extra {
    ($($ty:ty),+ $(,)?) => {$(
        impl ExtraBytes for $ty { fn extra_bytes(&self) -> usize { 0 } }
    )+};
}
no_extra!(
    f32,
    f64,
    i32,
    u32,
    Vec2,
    GlueVertexPair,
    GlueKeyform,
    OffscreenKeyform,
    DeltaRotationKeyform,
    DeltaPartKeyform,
    DeltaGlueKeyform,
    DeltaOffscreenKeyform,
    MotionPoint,
    MotionSegment
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
impl ExtraBytes for CdiParameterGroup {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.parent_id.extra_bytes()
            + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for CdiParameterEntry {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved {
                parameter_id,
                group_id,
                extensions,
            } => parameter_id.capacity() + group_id.extra_bytes() + extensions.extra_bytes(),
            Self::Unresolved {
                runtime_id,
                name,
                group_id,
                extensions,
            } => {
                runtime_id.capacity()
                    + name.capacity()
                    + group_id.extra_bytes()
                    + extensions.extra_bytes()
            }
        }
    }
}
impl ExtraBytes for CdiPartEntry {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved {
                part_id,
                extensions,
            } => part_id.capacity() + extensions.extra_bytes(),
            Self::Unresolved {
                runtime_id,
                name,
                extensions,
            } => runtime_id.capacity() + name.capacity() + extensions.extra_bytes(),
        }
    }
}
impl ExtraBytes for CdiParameterRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved { parameter_id } => parameter_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}
impl ExtraBytes for CdiCombinedSet {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.members.extra_bytes()
    }
}
impl ExtraBytes for CdiNamespaceIds {
    fn extra_bytes(&self) -> usize {
        self.parameters.extra_bytes() + self.parts.extra_bytes() + self.groups.extra_bytes()
    }
}
impl ExtraBytes for DisplayInfo {
    fn extra_bytes(&self) -> usize {
        self.parameters.extra_bytes()
            + self.parameter_groups.extra_bytes()
            + self.parts.extra_bytes()
            + self.combined_parameters.extra_bytes()
            + self.extensions.extra_bytes()
            + self.opaque_source_ids.extra_bytes()
    }
}
impl ExtraBytes for ExpressionTarget {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved { parameter_id } => parameter_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}
impl ExtraBytes for ExpressionEntry {
    fn extra_bytes(&self) -> usize {
        self.target.extra_bytes() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for ExpressionAsset {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.name.capacity()
            + self.file_type.extra_bytes()
            + self.entries.extra_bytes()
            + self.extensions.extra_bytes()
            + self.opaque_source_ids.extra_bytes()
            + self.opaque_source_content_hash.extra_bytes()
    }
}
impl ExtraBytes for MotionTrackTarget {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Model { runtime_id } | Self::Unresolved { runtime_id, .. } => {
                runtime_id.capacity()
                    + if let Self::Unresolved { category, .. } = self {
                        category.capacity()
                    } else {
                        0
                    }
            }
            Self::Parameter { parameter_id } => parameter_id.capacity(),
            Self::PartOpacity { part_id } => part_id.capacity(),
        }
    }
}
impl ExtraBytes for MotionTrack {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.target.extra_bytes()
            + self.segments.extra_bytes()
            + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for MotionEvent {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.value.capacity() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for MotionClip {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.name.capacity()
            + self.tracks.extra_bytes()
            + self.events.extra_bytes()
            + self.extensions.extra_bytes()
            + self.meta_extensions.extra_bytes()
            + self.opaque_source_ids.extra_bytes()
            + self.opaque_source_content_hash.extra_bytes()
    }
}
impl ExtraBytes for MotionRegistration {
    fn extra_bytes(&self) -> usize {
        self.clip_id.capacity() + self.sound.extra_bytes() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for MotionGroup {
    fn extra_bytes(&self) -> usize {
        self.name.capacity() + self.entries.extra_bytes()
    }
}
impl ExtraBytes for PosePartRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved { part_id } => part_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}
impl ExtraBytes for PoseEntry {
    fn extra_bytes(&self) -> usize {
        self.part.extra_bytes() + self.links.extra_bytes() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for PoseAsset {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.file_type.extra_bytes()
            + self.groups.extra_bytes()
            + self.extensions.extra_bytes()
            + self.opaque_source_ids.extra_bytes()
            + self.opaque_source_content_hash.extra_bytes()
    }
}
impl ExtraBytes for PhysicsAsset {
    fn extra_bytes(&self) -> usize {
        // Physics vectors and nested extension maps are charged conservatively
        // against their serialized size, including collection spare capacity.
        self.id.capacity()
            + serde_json::to_vec(&self.data).map_or(0, |bytes| bytes.len() * 2)
            + self.parameter_bindings.extra_bytes()
            + self.opaque_source_ids.extra_bytes()
            + self.opaque_source_content_hash.extra_bytes()
    }
}
impl ExtraBytes for ModelTargetRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved { object_id } => object_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}
impl ExtraBytes for ModelParameterGroup {
    fn extra_bytes(&self) -> usize {
        self.name.capacity() + self.parameters.extra_bytes() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for ModelHitArea {
    fn extra_bytes(&self) -> usize {
        self.name.capacity() + self.mesh.extra_bytes() + self.extensions.extra_bytes()
    }
}
impl ExtraBytes for Model3Settings {
    fn extra_bytes(&self) -> usize {
        self.groups.extra_bytes()
            + self.layout.extra_bytes()
            + self.hit_areas.extra_bytes()
            + self.user_data.extra_bytes()
            + self.extensions.extra_bytes()
            + self.source_runtime_ids.extra_bytes()
            + self.source_content.extra_bytes()
    }
}
impl ExtraBytes for PackageAttachment {
    fn extra_bytes(&self) -> usize {
        self.path.capacity() + self.bytes.capacity()
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
        + content.display_info.extra_bytes()
        + content.expressions.extra_bytes()
        + content.expression_order.extra_bytes()
        + content.motions.extra_bytes()
        + content.motion_order.extra_bytes()
        + content.motion_groups.extra_bytes()
        + content.pose.extra_bytes()
        + content.physics.extra_bytes()
        + content.missing_attachments.extra_bytes()
        + content.model3_settings.extra_bytes()
        + content.package_attachments.extra_bytes()
}
