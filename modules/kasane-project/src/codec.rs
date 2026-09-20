use std::collections::HashSet;
use std::fmt;

use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint,
    BlendShapeKeyTable, BlendShapeTargetKind, Canvas, DeltaKeyforms, DeltaMeshKeyform,
    DeltaPartKeyform, DeltaRotationKeyform, DeltaWarpKeyform, Glue, GlueVertexPair, ImageAsset,
    Mesh, MeshBinding, MeshKeyform, Parameter, Part, RotationPose, SceneBinding, SceneKeyform,
    Status, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::resources::is_valid_asset_path;

struct DuplicateKeyCheckerSeed(usize);

impl<'de> Visitor<'de> for DuplicateKeyCheckerSeed {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("valid JSON without duplicate keys")
    }

    fn visit_map<M>(self, mut access: M) -> Result<(), M::Error>
    where
        M: MapAccess<'de>,
    {
        if self.0 > 128 {
            return Err(serde::de::Error::custom("JSON nesting exceeds 128 levels"));
        }
        let mut seen = HashSet::new();
        while let Some(key) = access.next_key::<String>()? {
            if key.contains('\0') {
                return Err(serde::de::Error::custom("Embedded NUL is not supported"));
            }
            if !seen.insert(key) {
                return Err(serde::de::Error::custom("Duplicate JSON member"));
            }
            let () = access.next_value_seed(DuplicateKeyCheckerSeed(self.0 + 1))?;
        }
        Ok(())
    }

    fn visit_seq<S>(self, mut access: S) -> Result<(), S::Error>
    where
        S: SeqAccess<'de>,
    {
        if self.0 > 128 {
            return Err(serde::de::Error::custom("JSON nesting exceeds 128 levels"));
        }
        while access
            .next_element_seed(DuplicateKeyCheckerSeed(self.0 + 1))?
            .is_some()
        {}
        Ok(())
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, v: f64) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        if !v.is_finite() {
            return Err(serde::de::Error::custom("Non-finite number"));
        }
        Ok(())
    }

    fn visit_str<E>(self, s: &str) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        if s.contains('\0') {
            return Err(serde::de::Error::custom("Embedded NUL is not supported"));
        }
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for DuplicateKeyCheckerSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

pub fn validate_json_syntax(text: &str) -> Result<(), Status> {
    let mut de = serde_json::Deserializer::from_str(text);
    DuplicateKeyCheckerSeed(0)
        .deserialize(&mut de)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct MeshPropertiesWire {
    part_id: String,
    deformer_id: String,
    appearance: AppearanceWire,
    blend_mode: i32,
    enabled: bool,
    double_sided: bool,
    inverted_mask: bool,
    masks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    draw_order: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct MeshWire {
    id: String,
    runtime_id: String,
    name: String,
    texture_asset_id: String,
    vertex_ids: Vec<u32>,
    base_positions: Vec<[f32; 2]>,
    uvs: Vec<[f32; 2]>,
    triangles: Vec<u32>,
    properties: MeshPropertiesWire,
}

#[derive(Clone, Serialize, Deserialize)]
struct AppearanceWire {
    opacity: f32,
    multiply: [f32; 3],
    screen: [f32; 3],
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
struct RotationPoseWire {
    origin: [f32; 2],
    angle: f32,
    scale: f32,
    reflect_x: bool,
    reflect_y: bool,
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
            origin: Vec2::new(r.origin[0], r.origin[1]),
            angle: r.angle,
            scale: r.scale,
            reflect_x: r.reflect_x,
            reflect_y: r.reflect_y,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct TransformWire {
    id: String,
    runtime_id: String,
    name: String,
    part_id: String,
    parent_id: String,
    kind: i32,
    base_angle: f32,
    rotation: RotationPoseWire,
    rows: u32,
    columns: u32,
    quad: bool,
    enabled: bool,
    points: Vec<[f32; 2]>,
    appearance: AppearanceWire,
}

#[derive(Serialize, Deserialize)]
struct MeshKeyformWire {
    keys: Vec<f32>,
    positions: Vec<[f32; 2]>,
    #[serde(default)]
    appearance: Option<AppearanceWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draw_order: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct SceneKeyformWire {
    keys: Vec<f32>,
    positions: Vec<[f32; 2]>,
    rotation: RotationPoseWire,
    #[serde(default)]
    appearance: Option<AppearanceWire>,
    #[serde(default)]
    draw_order: f32,
}

#[derive(Serialize, Deserialize)]
struct MeshBindingWire {
    id: String,
    mesh_id: String,
    axes: Vec<BindingAxisWire>,
    keyforms: Vec<MeshKeyformWire>,
}

#[derive(Serialize, Deserialize)]
struct SceneBindingWire {
    id: String,
    target_id: String,
    axes: Vec<BindingAxisWire>,
    keyforms: Vec<SceneKeyformWire>,
}

#[derive(Serialize, Deserialize)]
struct BindingAxisWire {
    parameter_id: String,
    keys: Vec<f32>,
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
struct BlendShapeKeyTableWire {
    id: String,
    parameter_id: String,
    keys: Vec<f32>,
    base_key_idx: usize,
}

#[derive(Serialize, Deserialize)]
struct BlendShapeConstraintWire {
    id: String,
    parameter_id: String,
    keys: Vec<f32>,
    weights: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
struct DeltaMeshKeyformWire {
    positions: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draw_order: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
struct DeltaWarpKeyformWire {
    points: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
struct DeltaRotationKeyformWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    angle: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screen: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize)]
struct DeltaPartKeyformWire {
    draw_order: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "items", rename_all = "snake_case")]
enum DeltaKeyformsWire {
    Mesh(Vec<DeltaMeshKeyformWire>),
    Warp(Vec<DeltaWarpKeyformWire>),
    Rotation(Vec<DeltaRotationKeyformWire>),
    Part(Vec<DeltaPartKeyformWire>),
}

#[derive(Serialize, Deserialize)]
struct BlendShapeBindingWire {
    id: String,
    target_id: String,
    target_kind: BlendShapeTargetKind,
    key_table_id: String,
    constraint_ids: Vec<String>,
    keyforms: DeltaKeyformsWire,
}

#[derive(Serialize, Deserialize)]
struct GlueVertexPairWire {
    vertex_a: u32,
    vertex_b: u32,
    weight_a: f32,
    weight_b: f32,
}

#[derive(Serialize, Deserialize)]
struct GlueWire {
    id: String,
    runtime_id: String,
    name: String,
    mesh_a_id: String,
    mesh_b_id: String,
    pairs: Vec<GlueVertexPairWire>,
    intensity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding_id: Option<String>,
}

fn default_canvas_flag() -> u8 {
    1
}

#[derive(Serialize, Deserialize)]
struct DocumentWire {
    id: String,
    canvas: [f32; 2],
    canvas_origin: [f32; 2],
    pixels_per_unit: f32,
    #[serde(default = "default_canvas_flag")]
    canvas_flag: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draw_order_groups: Option<Vec<DrawOrderGroup>>,
    assets: Vec<ImageAsset>,
    meshes: Vec<MeshWire>,
    parts: Vec<Part>,
    transforms: Vec<TransformWire>,
    parameters: Vec<Parameter>,
    bindings: Vec<MeshBindingWire>,
    scene_bindings: Vec<SceneBindingWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_key_tables: Vec<BlendShapeKeyTableWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_constraints: Vec<BlendShapeConstraintWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blend_bindings: Vec<BlendShapeBindingWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    glues: Vec<GlueWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deformers: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deformation_links: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization_links: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct ProjectWire {
    format: String,
    format_version: u32,
    document: DocumentWire,
}

pub fn encode_project(document: &Document) -> Result<String, Status> {
    if !document.initialized() {
        return Err(Status::error(
            "NOT_INITIALIZED",
            "Initialize Document first",
        ));
    }

    let c = document.canvas();
    let mut meshes_wire = Vec::with_capacity(document.mesh_order().len());
    for id in document.mesh_order() {
        let m = document.get_mesh(id).unwrap();
        let mut triangles_flat = Vec::with_capacity(m.triangles.len() * 3);
        for t in &m.triangles {
            triangles_flat.push(t[0]);
            triangles_flat.push(t[1]);
            triangles_flat.push(t[2]);
        }
        let base_positions: Vec<[f32; 2]> = m.base_positions.iter().map(|p| [p.x, p.y]).collect();
        let uvs: Vec<[f32; 2]> = m.uvs.iter().map(|p| [p.x, p.y]).collect();

        meshes_wire.push(MeshWire {
            id: m.id.clone(),
            runtime_id: m.runtime_id.clone(),
            name: m.name.clone(),
            texture_asset_id: m.texture_asset_id.clone(),
            vertex_ids: m.vertex_ids.clone(),
            base_positions,
            uvs,
            triangles: triangles_flat,
            properties: MeshPropertiesWire {
                part_id: m.part_id.clone(),
                deformer_id: m.deformer_id.clone(),
                appearance: AppearanceWire::from(&m.appearance),
                blend_mode: match m.blend_mode {
                    BlendMode::Normal => 0,
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                },
                enabled: m.enabled,
                double_sided: m.double_sided,
                inverted_mask: m.inverted_mask,
                masks: m.masks.clone(),
                draw_order: m.draw_order,
            },
        });
    }

    let mut parts_wire = Vec::with_capacity(document.part_order().len());
    for id in document.part_order() {
        parts_wire.push(document.get_part(id).unwrap().clone());
    }

    let mut transforms_wire = Vec::with_capacity(document.transform_order().len());
    for id in document.transform_order() {
        let t = document.get_transform(id).unwrap();
        transforms_wire.push(TransformWire {
            id: t.id.clone(),
            runtime_id: t.runtime_id.clone(),
            name: t.name.clone(),
            part_id: t.part_id.clone(),
            parent_id: t.parent_id.clone(),
            kind: match t.kind {
                TransformKind::Rotation => 1,
                TransformKind::Warp => 0,
            },
            base_angle: t.base_angle,
            rotation: RotationPoseWire::from(&t.rotation),
            rows: t.rows,
            columns: t.columns,
            quad: t.quad,
            enabled: t.enabled,
            points: t.points.iter().map(|p| [p.x, p.y]).collect(),
            appearance: AppearanceWire::from(&t.appearance),
        });
    }

    let mut params_wire = Vec::with_capacity(document.parameter_order().len());
    for id in document.parameter_order() {
        params_wire.push(document.get_parameter(id).unwrap().clone());
    }

    let mut bindings_wire = Vec::with_capacity(document.binding_order().len());
    for id in document.binding_order() {
        let b = document.get_binding(id).unwrap();
        let keyforms_wire = b
            .keyforms
            .iter()
            .map(|f| MeshKeyformWire {
                keys: f.keys.clone(),
                positions: f.positions.iter().map(|p| [p.x, p.y]).collect(),
                appearance: Some(AppearanceWire::from(&f.appearance)),
                draw_order: f.draw_order,
            })
            .collect();
        bindings_wire.push(MeshBindingWire {
            id: b.id.clone(),
            mesh_id: b.mesh_id.clone(),
            axes: b.axes.iter().map(BindingAxisWire::from).collect(),
            keyforms: keyforms_wire,
        });
    }

    let mut scene_bindings_wire = Vec::with_capacity(document.scene_binding_order().len());
    for id in document.scene_binding_order() {
        let sb = document.get_scene_binding(id).unwrap();
        let keyforms_wire = sb
            .keyforms
            .iter()
            .map(|f| SceneKeyformWire {
                keys: f.keys.clone(),
                positions: f.positions.iter().map(|p| [p.x, p.y]).collect(),
                rotation: RotationPoseWire::from(&f.rotation),
                appearance: Some(AppearanceWire::from(&f.appearance)),
                draw_order: f.draw_order,
            })
            .collect();
        scene_bindings_wire.push(SceneBindingWire {
            id: sb.id.clone(),
            target_id: sb.target_id.clone(),
            axes: sb.axes.iter().map(BindingAxisWire::from).collect(),
            keyforms: keyforms_wire,
        });
    }

    let mut blend_key_tables_wire = Vec::with_capacity(document.blend_key_table_order().len());
    for id in document.blend_key_table_order() {
        let t = document.get_blend_key_table(id).unwrap();
        blend_key_tables_wire.push(BlendShapeKeyTableWire {
            id: t.id.clone(),
            parameter_id: t.parameter_id.clone(),
            keys: t.keys.clone(),
            base_key_idx: t.base_key_idx,
        });
    }

    let mut blend_constraints_wire = Vec::with_capacity(document.blend_constraint_order().len());
    for id in document.blend_constraint_order() {
        let c = document.get_blend_constraint(id).unwrap();
        blend_constraints_wire.push(BlendShapeConstraintWire {
            id: c.id.clone(),
            parameter_id: c.parameter_id.clone(),
            keys: c.keys.clone(),
            weights: c.weights.clone(),
        });
    }

    let mut blend_bindings_wire = Vec::with_capacity(document.blend_binding_order().len());
    for id in document.blend_binding_order() {
        let b = document.get_blend_binding(id).unwrap();
        let keyforms = match &b.keyforms {
            DeltaKeyforms::Mesh(forms) => DeltaKeyformsWire::Mesh(
                forms
                    .iter()
                    .map(|f| DeltaMeshKeyformWire {
                        positions: f.positions.iter().map(|p| [p.x, p.y]).collect(),
                        opacity: f.opacity,
                        draw_order: f.draw_order,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyforms::Warp(forms) => DeltaKeyformsWire::Warp(
                forms
                    .iter()
                    .map(|f| DeltaWarpKeyformWire {
                        points: f.points.iter().map(|p| [p.x, p.y]).collect(),
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyforms::Rotation(forms) => DeltaKeyformsWire::Rotation(
                forms
                    .iter()
                    .map(|f| DeltaRotationKeyformWire {
                        origin: f.origin.map(|p| [p.x, p.y]),
                        angle: f.angle,
                        scale: f.scale,
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyforms::Part(forms) => DeltaKeyformsWire::Part(
                forms
                    .iter()
                    .map(|f| DeltaPartKeyformWire {
                        draw_order: f.draw_order,
                    })
                    .collect(),
            ),
        };
        blend_bindings_wire.push(BlendShapeBindingWire {
            id: b.id.clone(),
            target_id: b.target_id.clone(),
            target_kind: b.target_kind,
            key_table_id: b.key_table_id.clone(),
            constraint_ids: b.constraint_ids.clone(),
            keyforms,
        });
    }

    let mut glues_wire = Vec::with_capacity(document.glue_order().len());
    for id in document.glue_order() {
        let g = document.get_glue(id).unwrap();
        let pairs = g
            .pairs
            .iter()
            .map(|p| GlueVertexPairWire {
                vertex_a: p.vertex_a,
                vertex_b: p.vertex_b,
                weight_a: p.weight_a,
                weight_b: p.weight_b,
            })
            .collect();
        glues_wire.push(GlueWire {
            id: g.id.clone(),
            runtime_id: g.runtime_id.clone(),
            name: g.name.clone(),
            mesh_a_id: g.mesh_a_id.clone(),
            mesh_b_id: g.mesh_b_id.clone(),
            pairs,
            intensity: g.intensity,
            binding_id: g.binding_id.clone(),
        });
    }

    let mut assets_wire = Vec::with_capacity(document.asset_order().len());
    for id in document.asset_order() {
        assets_wire.push(document.get_asset(id).unwrap().clone());
    }

    let project = ProjectWire {
        format: "kasane-directory-project".to_string(),
        format_version: 2,
        document: DocumentWire {
            id: document.id().to_string(),
            canvas: [c.width, c.height],
            canvas_origin: [c.origin.x, c.origin.y],
            pixels_per_unit: c.pixels_per_unit,
            canvas_flag: c.flag,
            draw_order_groups: document.draw_order_groups().map(|g| g.to_vec()),
            assets: assets_wire,
            meshes: meshes_wire,
            parts: parts_wire,
            transforms: transforms_wire,
            parameters: params_wire,
            bindings: bindings_wire,
            scene_bindings: scene_bindings_wire,
            blend_key_tables: blend_key_tables_wire,
            blend_constraints: blend_constraints_wire,
            blend_bindings: blend_bindings_wire,
            glues: glues_wire,
            deformers: None,
            deformation_links: None,
            organization_links: None,
        },
    };

    let mut out = serde_json::to_string_pretty(&project)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    out.push('\n');
    Ok(out)
}

pub fn decode_project(text: &str) -> Result<Document, Status> {
    validate_json_syntax(text)?;

    let root: ProjectWire = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            // Check legacy format
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(fmt) = v.get("format").and_then(|f| f.as_str()) {
                    if fmt == "kasane-project" {
                        let ver = v
                            .get("format_version")
                            .map(|x| x.to_string())
                            .unwrap_or_else(|| "\"missing\"".to_string());
                        return Err(Status::error(
                            "LEGACY_PROJECT",
                            format!("Experimental kasane-project version {ver} is unsupported; no automatic migration"),
                        ));
                    }
                }
            }
            return Err(Status::error("INVALID_PROJECT", e.to_string()));
        }
    };

    if root.format == "kasane-project" {
        return Err(Status::error(
            "LEGACY_PROJECT",
            "Experimental kasane-project version is unsupported; no automatic migration",
        ));
    }

    if root.format != "kasane-directory-project" {
        return Err(Status::error("INVALID_PROJECT", "Unknown project format"));
    }

    if root.format_version != 1 && root.format_version != 2 {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            format!(
                "Unsupported directory-project version: {}",
                root.format_version
            ),
        ));
    }

    let doc = root.document;

    // Check prototype relationships
    for val in [
        &doc.deformers,
        &doc.deformation_links,
        &doc.organization_links,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(arr) = val.as_array() {
            if !arr.is_empty() {
                return Err(Status::error(
                    "LEGACY_PROJECT",
                    "Prototype relationships are unsupported",
                ));
            }
        } else {
            return Err(Status::error(
                "LEGACY_PROJECT",
                "Prototype relationships are unsupported",
            ));
        }
    }

    let mut candidate = Document::new();
    let canvas = Canvas::with_flag(
        doc.canvas[0],
        doc.canvas[1],
        Vec2::new(doc.canvas_origin[0], doc.canvas_origin[1]),
        doc.pixels_per_unit,
        doc.canvas_flag,
    );
    let s = candidate.initialize(doc.id, canvas);
    if !s.is_ok() {
        return Err(s);
    }

    for asset in doc.assets {
        if !is_valid_asset_path(&asset.source)
            || asset.sha256.len() != 64
            || !asset
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(Status::error(
                "INVALID_PROJECT",
                format!("{}: invalid relative asset path or SHA-256", asset.id),
            ));
        }
        let res = candidate.add_asset(asset);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Parts: Two-pass creation to avoid topological cycle order issues
    for p in &doc.parts {
        let mut tmp = p.clone();
        tmp.parent_id.clear();
        let res = candidate.create_part(tmp);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for p in doc.parts {
        let res = candidate.replace_part(p);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Transforms: Two-pass creation
    for t in &doc.transforms {
        let mut tmp = Transform {
            id: t.id.clone(),
            runtime_id: t.runtime_id.clone(),
            name: t.name.clone(),
            part_id: t.part_id.clone(),
            parent_id: String::new(),
            kind: match t.kind {
                0 => TransformKind::Warp,
                1 => TransformKind::Rotation,
                _ => return Err(Status::error("INVALID_PROJECT", "Unknown Transform kind")),
            },
            base_angle: t.base_angle,
            rotation: RotationPose::from(t.rotation.clone()),
            rows: t.rows,
            columns: t.columns,
            quad: t.quad,
            enabled: t.enabled,
            points: t.points.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
            appearance: t.appearance.clone().into(),
        };
        tmp.parent_id.clear();
        let res = candidate.create_transform(tmp);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for t in doc.transforms {
        let actual = Transform {
            id: t.id,
            runtime_id: t.runtime_id,
            name: t.name,
            part_id: t.part_id,
            parent_id: t.parent_id,
            kind: match t.kind {
                0 => TransformKind::Warp,
                1 => TransformKind::Rotation,
                _ => return Err(Status::error("INVALID_PROJECT", "Unknown Transform kind")),
            },
            base_angle: t.base_angle,
            rotation: RotationPose::from(t.rotation),
            rows: t.rows,
            columns: t.columns,
            quad: t.quad,
            enabled: t.enabled,
            points: t.points.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
            appearance: t.appearance.into(),
        };
        let res = candidate.replace_transform(actual);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Meshes: Two-pass creation
    for m in &doc.meshes {
        if m.triangles.len() % 3 != 0 {
            return Err(Status::error(
                "INVALID_PROJECT",
                "triangles: expected triples of vertex IDs",
            ));
        }
        let blend_mode = match m.properties.blend_mode {
            0 => BlendMode::Normal,
            1 => BlendMode::Additive,
            2 => BlendMode::Multiplicative,
            _ => return Err(Status::error("INVALID_PROJECT", "Unknown blend mode")),
        };
        let triangles = m.triangles.as_chunks::<3>().0.to_vec();
        let mesh = Mesh {
            id: m.id.clone(),
            runtime_id: m.runtime_id.clone(),
            name: m.name.clone(),
            texture_asset_id: m.texture_asset_id.clone(),
            part_id: m.properties.part_id.clone(),
            deformer_id: m.properties.deformer_id.clone(),
            vertex_ids: m.vertex_ids.clone(),
            base_positions: m
                .base_positions
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect(),
            uvs: m.uvs.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
            triangles,
            draw_order: m.properties.draw_order,
            appearance: m.properties.appearance.clone().into(),
            blend_mode,
            enabled: m.properties.enabled,
            double_sided: m.properties.double_sided,
            inverted_mask: m.properties.inverted_mask,
            masks: Vec::new(),
        };
        let res = candidate.create_mesh(mesh);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for m in doc.meshes {
        let blend_mode = match m.properties.blend_mode {
            0 => BlendMode::Normal,
            1 => BlendMode::Additive,
            2 => BlendMode::Multiplicative,
            _ => return Err(Status::error("INVALID_PROJECT", "Unknown blend mode")),
        };
        let triangles = m.triangles.as_chunks::<3>().0.to_vec();
        let mesh = Mesh {
            id: m.id,
            runtime_id: m.runtime_id,
            name: m.name,
            texture_asset_id: m.texture_asset_id,
            part_id: m.properties.part_id,
            deformer_id: m.properties.deformer_id,
            vertex_ids: m.vertex_ids,
            base_positions: m
                .base_positions
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect(),
            uvs: m.uvs.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
            triangles,
            draw_order: m.properties.draw_order,
            appearance: m.properties.appearance.into(),
            blend_mode,
            enabled: m.properties.enabled,
            double_sided: m.properties.double_sided,
            inverted_mask: m.properties.inverted_mask,
            masks: m.properties.masks,
        };
        let res = candidate.replace_mesh(mesh);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for p in doc.parameters {
        let res = candidate.create_parameter(p);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for b in doc.bindings {
        let axes = b.axes.into_iter().map(BindingAxis::from).collect();
        let keyforms = b
            .keyforms
            .into_iter()
            .map(|f| MeshKeyform {
                keys: f.keys,
                positions: f.positions.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
                appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                draw_order: f.draw_order,
            })
            .collect();
        let res = candidate.create_binding(MeshBinding {
            id: b.id,
            mesh_id: b.mesh_id,
            axes,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for sb in doc.scene_bindings {
        let axes = sb.axes.into_iter().map(BindingAxis::from).collect();
        let keyforms = sb
            .keyforms
            .into_iter()
            .map(|f| SceneKeyform {
                keys: f.keys,
                positions: f.positions.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
                rotation: RotationPose::from(f.rotation),
                appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                draw_order: f.draw_order,
            })
            .collect();
        let res = candidate.create_scene_binding(SceneBinding {
            id: sb.id,
            target_id: sb.target_id,
            axes,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    if let Some(groups) = doc.draw_order_groups {
        let result = candidate.replace_draw_order_groups(groups);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }

    for t in doc.blend_key_tables {
        let res = candidate.create_blend_key_table(BlendShapeKeyTable {
            id: t.id,
            parameter_id: t.parameter_id,
            keys: t.keys,
            base_key_idx: t.base_key_idx,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for c in doc.blend_constraints {
        let res = candidate.create_blend_constraint(BlendShapeConstraint {
            id: c.id,
            parameter_id: c.parameter_id,
            keys: c.keys,
            weights: c.weights,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for b in doc.blend_bindings {
        let keyforms = match b.keyforms {
            DeltaKeyformsWire::Mesh(forms) => DeltaKeyforms::Mesh(
                forms
                    .into_iter()
                    .map(|f| DeltaMeshKeyform {
                        positions: f
                            .positions
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        opacity: f.opacity,
                        draw_order: f.draw_order,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Warp(forms) => DeltaKeyforms::Warp(
                forms
                    .into_iter()
                    .map(|f| DeltaWarpKeyform {
                        points: f
                            .points
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Rotation(forms) => DeltaKeyforms::Rotation(
                forms
                    .into_iter()
                    .map(|f| DeltaRotationKeyform {
                        origin: f.origin.map(|p| Vec2::new(p[0], p[1])),
                        angle: f.angle,
                        scale: f.scale,
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Part(forms) => DeltaKeyforms::Part(
                forms
                    .into_iter()
                    .map(|f| DeltaPartKeyform {
                        draw_order: f.draw_order,
                    })
                    .collect(),
            ),
        };
        let res = candidate.create_blend_binding(BlendShapeBinding {
            id: b.id,
            target_id: b.target_id,
            target_kind: b.target_kind,
            key_table_id: b.key_table_id,
            constraint_ids: b.constraint_ids,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for g in doc.glues {
        let pairs = g
            .pairs
            .into_iter()
            .map(|p| GlueVertexPair {
                vertex_a: p.vertex_a,
                vertex_b: p.vertex_b,
                weight_a: p.weight_a,
                weight_b: p.weight_b,
            })
            .collect();
        let res = candidate.create_glue(Glue {
            id: g.id,
            runtime_id: g.runtime_id,
            name: g.name,
            mesh_a_id: g.mesh_a_id,
            mesh_b_id: g.mesh_b_id,
            pairs,
            intensity: g.intensity,
            binding_id: g.binding_id,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    candidate.mark_saved();
    Ok(candidate)
}
