use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Status {
    pub code: String,
    pub message: String,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: String::new(),
            message: String::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.is_empty()
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

pub type VertexId = u32;

fn default_canvas_flag() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: f32,
    pub height: f32,
    pub origin: Vec2,
    pub pixels_per_unit: f32,
    #[serde(default = "default_canvas_flag")]
    pub flag: u8,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            origin: Vec2::default(),
            pixels_per_unit: 1.0,
            flag: 1,
        }
    }
}

impl Canvas {
    pub fn new(width: f32, height: f32, origin: Vec2, pixels_per_unit: f32) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag: 1,
        }
    }

    pub fn with_flag(width: f32, height: f32, origin: Vec2, pixels_per_unit: f32, flag: u8) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageAsset {
    pub id: String,
    pub name: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Additive,
    Multiplicative,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    pub opacity: f32,
    pub multiply: [f32; 3],
    pub screen: [f32; 3],
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            multiply: [1.0, 1.0, 1.0],
            screen: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RotationPose {
    pub origin: Vec2,
    pub angle: f32,
    pub scale: f32,
    pub reflect_x: bool,
    pub reflect_y: bool,
}

impl Default for RotationPose {
    fn default() -> Self {
        Self {
            origin: Vec2::default(),
            angle: 0.0,
            scale: 1.0,
            reflect_x: false,
            reflect_y: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransformKind {
    Warp,
    #[default]
    Rotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub id: String,
    pub runtime_id: String,
    pub name: String,
    pub part_id: String,
    pub parent_id: String,
    pub kind: TransformKind,
    pub base_angle: f32,
    pub rotation: RotationPose,
    pub rows: u32,
    pub columns: u32,
    pub quad: bool,
    pub enabled: bool,
    pub points: Vec<Vec2>,
    pub appearance: Appearance,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            part_id: String::new(),
            parent_id: String::new(),
            kind: TransformKind::Rotation,
            base_angle: 0.0,
            rotation: RotationPose::default(),
            rows: 1,
            columns: 1,
            quad: true,
            enabled: true,
            points: Vec::new(),
            appearance: Appearance::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub runtime_id: String,
    pub name: String,
    pub parent_id: String,
    pub enabled: bool,
    pub draw_order: f32,
}

impl Default for Part {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneKeyform {
    pub keys: Vec<f32>,
    pub positions: Vec<Vec2>,
    pub rotation: RotationPose,
    pub appearance: Appearance,
    pub draw_order: f32,
}

impl Default for SceneKeyform {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            positions: Vec::new(),
            rotation: RotationPose::default(),
            appearance: Appearance::default(),
            draw_order: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub id: String,
    pub name: String,
    pub texture_asset_id: String,
    pub vertex_ids: Vec<VertexId>,
    pub base_positions: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[VertexId; 3]>,
    pub runtime_id: String,
    pub part_id: String,
    pub deformer_id: String,
    pub appearance: Appearance,
    pub draw_order: Option<f32>,
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            texture_asset_id: String::new(),
            vertex_ids: Vec::new(),
            base_positions: Vec::new(),
            uvs: Vec::new(),
            triangles: Vec::new(),
            runtime_id: String::new(),
            part_id: String::new(),
            deformer_id: String::new(),
            appearance: Appearance::default(),
            draw_order: None,
            blend_mode: BlendMode::Normal,
            enabled: true,
            double_sided: true,
            inverted_mask: false,
            masks: Vec::new(),
        }
    }
}

fn default_decimal_places() -> i32 {
    6
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub runtime_id: String,
    pub name: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    #[serde(default = "default_decimal_places")]
    pub decimal_places: i32,
}

impl Default for Parameter {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BindingAxis {
    pub parameter_id: String,
    pub keys: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MeshKeyform {
    pub keys: Vec<f32>,
    pub positions: Vec<Vec2>,
    pub appearance: Appearance,
    pub draw_order: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MeshBinding {
    pub id: String,
    pub mesh_id: String,
    pub axes: Vec<BindingAxis>,
    pub keyforms: Vec<MeshKeyform>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SceneBinding {
    pub id: String,
    pub target_id: String,
    pub axes: Vec<BindingAxis>,
    pub keyforms: Vec<SceneKeyform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VertexMapping {
    pub new_id: VertexId,
    pub old_id: Option<VertexId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChangeKind {
    #[default]
    None,
    Metadata,
    Positions,
    Structure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChangeSet {
    pub kind: ChangeKind,
    pub mesh_ids: Vec<String>,
    pub revision: u64,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EditResult {
    pub status: Status,
    pub changes: ChangeSet,
    pub referrers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VertexPositionUpdate {
    pub mesh_id: String,
    pub vertex_ids: Vec<VertexId>,
    pub positions: Vec<Vec2>,
}
