use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::types::{
    Appearance, BindingAxis, BlendShapeTargetKind, ImageAsset, OffscreenKeyform, Part, RotationPose,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(super) struct MeshPropertiesWire {
    pub(super) part_id: String,
    pub(super) deformer_id: String,
    pub(super) appearance: AppearanceWire,
    pub(super) blend_mode: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) raw_blend_mode: Option<u32>,
    pub(super) enabled: bool,
    pub(super) double_sided: bool,
    pub(super) inverted_mask: bool,
    pub(super) masks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) draw_order: Option<f32>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct MeshWire {
    pub(super) id: String,
    pub(super) runtime_id: String,
    pub(super) name: String,
    pub(super) texture_asset_id: String,
    pub(super) vertex_ids: Vec<u32>,
    pub(super) base_positions: Vec<[f32; 2]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) triangles: Vec<u32>,
    pub(super) properties: MeshPropertiesWire,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct AppearanceWire {
    pub(super) opacity: f32,
    pub(super) multiply: [f32; 3],
    pub(super) screen: [f32; 3],
}

impl From<&Appearance> for AppearanceWire {
    fn from(a: &Appearance) -> Self {
        Self {
            opacity: a.opacity,
            multiply: a.multiply,
            screen: a.screen,
        }
    }
}

impl From<AppearanceWire> for Appearance {
    fn from(a: AppearanceWire) -> Self {
        Self {
            opacity: a.opacity,
            multiply: a.multiply,
            screen: a.screen,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct RotationPoseWire {
    pub(super) origin: [f64; 2],
    pub(super) angle: f32,
    pub(super) scale: f32,
    pub(super) reflect_x: bool,
    pub(super) reflect_y: bool,
}

impl From<&RotationPose> for RotationPoseWire {
    fn from(r: &RotationPose) -> Self {
        Self {
            origin: [r.origin.x, r.origin.y],
            angle: r.angle,
            scale: r.scale,
            reflect_x: r.reflect_x,
            reflect_y: r.reflect_y,
        }
    }
}

impl From<RotationPoseWire> for RotationPose {
    fn from(r: RotationPoseWire) -> Self {
        Self {
            origin: kasane_core::types::PreciseVec2::new(r.origin[0], r.origin[1]),
            angle: r.angle,
            scale: r.scale,
            reflect_x: r.reflect_x,
            reflect_y: r.reflect_y,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct TransformWire {
    pub(super) id: String,
    pub(super) runtime_id: String,
    pub(super) name: String,
    pub(super) part_id: String,
    pub(super) parent_id: String,
    pub(super) kind: i32,
    pub(super) base_angle: f32,
    pub(super) rotation: RotationPoseWire,
    pub(super) rows: u32,
    pub(super) columns: u32,
    pub(super) quad: bool,
    pub(super) enabled: bool,
    pub(super) points: Vec<[f32; 2]>,
    pub(super) appearance: AppearanceWire,
}

#[derive(Serialize, Deserialize)]
pub(super) struct MeshKeyformWire {
    pub(super) keys: Vec<f32>,
    pub(super) positions: Vec<[f32; 2]>,
    #[serde(default)]
    pub(super) appearance: Option<AppearanceWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) draw_order: Option<f32>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SceneKeyformWire {
    pub(super) keys: Vec<f32>,
    pub(super) positions: Vec<[f32; 2]>,
    pub(super) rotation: RotationPoseWire,
    #[serde(default)]
    pub(super) appearance: Option<AppearanceWire>,
    #[serde(default)]
    pub(super) draw_order: f32,
}

#[derive(Serialize, Deserialize)]
pub(super) struct MeshBindingWire {
    pub(super) id: String,
    pub(super) mesh_id: String,
    pub(super) axes: Vec<BindingAxisWire>,
    pub(super) keyforms: Vec<MeshKeyformWire>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SceneBindingWire {
    pub(super) id: String,
    pub(super) target_id: String,
    pub(super) axes: Vec<BindingAxisWire>,
    pub(super) keyforms: Vec<SceneKeyformWire>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct BindingAxisWire {
    pub(super) parameter_id: String,
    pub(super) keys: Vec<f32>,
}

impl From<&BindingAxis> for BindingAxisWire {
    fn from(b: &BindingAxis) -> Self {
        Self {
            parameter_id: b.parameter_id.clone(),
            keys: b.keys.clone(),
        }
    }
}

impl From<BindingAxisWire> for BindingAxis {
    fn from(b: BindingAxisWire) -> Self {
        Self {
            parameter_id: b.parameter_id,
            keys: b.keys,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct BlendShapeKeyTableWire {
    pub(super) id: String,
    pub(super) parameter_id: String,
    pub(super) keys: Vec<f32>,
    pub(super) base_key_idx: usize,
}

#[derive(Serialize, Deserialize)]
pub(super) struct BlendShapeConstraintWire {
    pub(super) id: String,
    pub(super) parameter_id: String,
    pub(super) keys: Vec<f32>,
    pub(super) weights: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeltaMeshKeyformWire {
    pub(super) positions: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) draw_order: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeltaWarpKeyformWire {
    pub(super) points: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeltaRotationKeyformWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) origin: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) angle: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeltaPartKeyformWire {
    pub(super) draw_order: f32,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeltaGlueKeyformWire {
    pub(super) intensity: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct DeltaOffscreenKeyformWire {
    pub(super) opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "items", rename_all = "snake_case")]
pub(super) enum DeltaKeyformsWire {
    Mesh(Vec<DeltaMeshKeyformWire>),
    Warp(Vec<DeltaWarpKeyformWire>),
    Rotation(Vec<DeltaRotationKeyformWire>),
    Part(Vec<DeltaPartKeyformWire>),
    Glue(Vec<DeltaGlueKeyformWire>),
    Offscreen(Vec<DeltaOffscreenKeyformWire>),
}

#[derive(Serialize, Deserialize)]
pub(super) struct BlendShapeBindingWire {
    pub(super) id: String,
    pub(super) target_id: String,
    pub(super) target_kind: BlendShapeTargetKind,
    pub(super) key_table_id: String,
    pub(super) constraint_ids: Vec<String>,
    pub(super) keyforms: DeltaKeyformsWire,
}

#[derive(Serialize, Deserialize)]
pub(super) struct GlueVertexPairWire {
    pub(super) vertex_a: u32,
    pub(super) vertex_b: u32,
    pub(super) weight_a: f32,
    pub(super) weight_b: f32,
}

#[derive(Serialize, Deserialize)]
pub(super) struct GlueWire {
    pub(super) id: String,
    pub(super) runtime_id: String,
    pub(super) name: String,
    pub(super) mesh_a_id: String,
    pub(super) mesh_b_id: String,
    pub(super) pairs: Vec<GlueVertexPairWire>,
    pub(super) intensity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) binding: Option<kasane_core::types::GlueBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) binding_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct OffscreenKeyformWire {
    pub(super) opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) screen: Option<[f32; 3]>,
}

impl From<&OffscreenKeyform> for OffscreenKeyformWire {
    fn from(k: &OffscreenKeyform) -> Self {
        Self {
            opacity: k.opacity,
            multiply: k.multiply,
            screen: k.screen,
        }
    }
}

impl From<OffscreenKeyformWire> for OffscreenKeyform {
    fn from(k: OffscreenKeyformWire) -> Self {
        Self {
            opacity: k.opacity,
            multiply: k.multiply,
            screen: k.screen,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct OffscreenWire {
    pub(super) id: String,
    pub(super) runtime_id: String,
    pub(super) name: String,
    pub(super) part_id: String,
    pub(super) blend_mode: u32,
    pub(super) flags: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) masks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) part_keyform_indices: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) keyforms: Vec<OffscreenKeyformWire>,
}

fn default_canvas_flag() -> u8 {
    1
}

#[derive(Serialize, Deserialize)]
pub(super) struct DocumentWire {
    pub(super) id: String,
    pub(super) canvas: [f32; 2],
    pub(super) canvas_origin: [f32; 2],
    pub(super) pixels_per_unit: f32,
    #[serde(default = "default_canvas_flag")]
    pub(super) canvas_flag: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) draw_order_groups: Option<Vec<DrawOrderGroup>>,
    pub(super) assets: Vec<ImageAsset>,
    pub(super) meshes: Vec<MeshWire>,
    pub(super) parts: Vec<Part>,
    pub(super) transforms: Vec<TransformWire>,
    pub(super) parameters: Vec<kasane_core::types::Parameter>,
    pub(super) bindings: Vec<MeshBindingWire>,
    pub(super) scene_bindings: Vec<SceneBindingWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) blend_key_tables: Vec<BlendShapeKeyTableWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) blend_constraints: Vec<BlendShapeConstraintWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) blend_bindings: Vec<BlendShapeBindingWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) glues: Vec<GlueWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) offscreens: Vec<OffscreenWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) deformers: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) deformation_links: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) organization_links: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ProjectWire {
    pub(super) format: String,
    pub(super) format_version: u32,
    pub(super) document: DocumentWire,
}
