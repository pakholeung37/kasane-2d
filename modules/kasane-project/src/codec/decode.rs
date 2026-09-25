use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint,
    BlendShapeKeyTable, Canvas, DeltaGlueKeyform, DeltaKeyforms, DeltaMeshKeyform,
    DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform, DeltaWarpKeyform, Glue,
    GlueVertexPair, Mesh, MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, RotationPose,
    SceneBinding, Status, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_core::{PartKeyform, RotationKeyform, SceneTrack, WarpKeyform};
use kasane_core::{RotationTransform, TransformData, WarpTransform};

use crate::resources::is_valid_asset_path;

use super::types::*;
use super::validation::validate_json_syntax;

fn decode_transform_kind(kind: i32) -> Result<TransformKind, Status> {
    match kind {
        0 => Ok(TransformKind::Warp),
        1 => Ok(TransformKind::Rotation),
        _ => Err(Status::error("INVALID_PROJECT", "Unknown Transform kind")),
    }
}

fn decode_blend_mode(mode: i32) -> Result<BlendMode, Status> {
    match mode {
        0 => Ok(BlendMode::Normal),
        1 => Ok(BlendMode::Additive),
        2 => Ok(BlendMode::Multiplicative),
        _ => Err(Status::error("INVALID_PROJECT", "Unknown blend mode")),
    }
}

fn transform_from_wire(
    transform: &TransformWire,
    parent_id: Option<&str>,
) -> Result<Transform, Status> {
    let kind = decode_transform_kind(transform.kind)?;
    let data = match kind {
        TransformKind::Warp => TransformData::Warp(WarpTransform {
            rows: transform.rows,
            columns: transform.columns,
            quad: transform.quad,
            points: transform
                .points
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect(),
        }),
        TransformKind::Rotation => TransformData::Rotation(RotationTransform {
            base_angle: transform.base_angle,
            pose: RotationPose::from(transform.rotation.clone()),
        }),
    };

    Ok(Transform {
        id: transform.id.clone(),
        runtime_id: transform.runtime_id.clone(),
        name: transform.name.clone(),
        part_id: kasane_core::PartId::optional(transform.part_id.clone()),
        parent_id: parent_id.and_then(|id| kasane_core::TransformId::optional(id.to_owned())),
        enabled: transform.enabled,
        appearance: transform.appearance.clone().into(),
        data,
    })
}

fn mesh_from_wire(mesh: &MeshWire, masks: Vec<String>) -> Result<Mesh, Status> {
    if !mesh.triangles.len().is_multiple_of(3) {
        return Err(Status::error(
            "INVALID_PROJECT",
            "triangles: expected triples of vertex IDs",
        ));
    }

    let blend_mode = decode_blend_mode(mesh.properties.blend_mode)?;
    Ok(Mesh {
        id: mesh.id.clone(),
        runtime_id: mesh.runtime_id.clone(),
        name: mesh.name.clone(),
        texture_asset_id: mesh.texture_asset_id.clone(),
        part_id: mesh.properties.part_id.clone(),
        deformer_id: mesh.properties.deformer_id.clone(),
        vertex_ids: mesh.vertex_ids.clone(),
        base_positions: mesh
            .base_positions
            .iter()
            .map(|p| Vec2::new(p[0], p[1]))
            .collect(),
        uvs: mesh.uvs.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
        triangles: mesh.triangles.as_chunks::<3>().0.to_vec(),
        draw_order: mesh.properties.draw_order,
        raw_blend_mode: mesh.properties.raw_blend_mode,
        appearance: mesh.properties.appearance.clone().into(),
        blend_mode,
        enabled: mesh.properties.enabled,
        double_sided: mesh.properties.double_sided,
        inverted_mask: mesh.properties.inverted_mask,
        masks,
    })
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

    decode_wire(root)
}

pub(super) fn decode_wire(root: ProjectWire) -> Result<Document, Status> {
    if root.format == "kasane-project" {
        return Err(Status::error(
            "LEGACY_PROJECT",
            "Experimental kasane-project version is unsupported; no automatic migration",
        ));
    }

    if root.format != "kasane-directory-project" {
        return Err(Status::error("INVALID_PROJECT", "Unknown project format"));
    }

    if !(1..=6).contains(&root.format_version) {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            format!(
                "Unsupported directory-project version: {}",
                root.format_version
            ),
        ));
    }

    let display_info = if root.format_version >= 5 {
        if !root.extra.is_empty() || !root.document.extra.is_empty() {
            return Err(Status::error(
                "UNSUPPORTED_VERSION",
                "v5+ root/document contains unrecognized fields",
            ));
        }
        match &root.document.display_info {
            Present::Present(Some(info)) => Some(info.clone()),
            Present::Present(None) | Present::Absent => {
                return Err(Status::error(
                    "INVALID_PROJECT",
                    "v5+ requires non-null display_info",
                ))
            }
        }
    } else {
        if !root.document.display_info.is_absent()
            || root.document.extra.contains_key("animation_assets")
        {
            return Err(Status::error(
                "UNSUPPORTED_VERSION",
                "v5 display_info/animation_assets cannot appear in v1-v4",
            ));
        }
        None
    };
    let animation_assets = if root.format_version >= 5 {
        match &root.document.animation_assets {
            Present::Present(Some(assets)) => assets.clone(),
            Present::Present(None) | Present::Absent => {
                return Err(Status::error(
                    "INVALID_PROJECT",
                    "v5+ requires non-null animation_assets",
                ));
            }
        }
    } else {
        if !root.document.animation_assets.is_absent() {
            return Err(Status::error(
                "UNSUPPORTED_VERSION",
                "v5 animation_assets cannot appear in v1-v4",
            ));
        }
        AnimationAssetsWire::default()
    };

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
    let s = candidate.begin_batch_build();
    if !s.is_ok() {
        return Err(s);
    }
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
        let tmp = transform_from_wire(t, None)?;
        let res = candidate.create_transform(tmp);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for t in doc.transforms {
        let actual = transform_from_wire(&t, Some(&t.parent_id))?;
        let res = candidate.replace_transform(actual);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Meshes: Two-pass creation
    for m in &doc.meshes {
        let mesh = mesh_from_wire(m, Vec::new())?;
        let res = candidate.create_mesh(mesh);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for m in doc.meshes {
        let mesh = mesh_from_wire(&m, m.properties.masks.clone())?;
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
        let track = match candidate.get_transform(&sb.target_id).map(|t| t.kind()) {
            Some(TransformKind::Warp) => SceneTrack::Warp {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| WarpKeyform {
                        keys: f.keys,
                        positions: f
                            .positions
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                    })
                    .collect(),
            },
            Some(TransformKind::Rotation) => SceneTrack::Rotation {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| RotationKeyform {
                        keys: f.keys,
                        rotation: f.rotation.into(),
                        appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                    })
                    .collect(),
            },
            None => SceneTrack::Part {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| PartKeyform {
                        keys: f.keys,
                        draw_order: f.draw_order,
                    })
                    .collect(),
            },
        };
        let res = candidate.create_scene_binding(SceneBinding {
            id: sb.id,
            axes,
            track,
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

    for g in doc.glues {
        if g.binding_id.is_some() {
            return Err(Status::error(
                "INVALID_GLUE_BINDING",
                "Legacy MeshBinding references cannot represent Glue intensity",
            ));
        }
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
            binding: g.binding,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for o in doc.offscreens {
        let keyforms = o.keyforms.into_iter().map(OffscreenKeyform::from).collect();
        let res = candidate.create_offscreen(Offscreen {
            id: o.id,
            runtime_id: o.runtime_id,
            name: o.name,
            part_id: o.part_id,
            blend_mode: o.blend_mode,
            flags: o.flags,
            masks: o.masks,
            part_keyform_indices: o.part_keyform_indices,
            keyforms,
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
            DeltaKeyformsWire::Glue(forms) => DeltaKeyforms::Glue(
                forms
                    .into_iter()
                    .map(|f| DeltaGlueKeyform {
                        intensity: f.intensity,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Offscreen(forms) => DeltaKeyforms::Offscreen(
                forms
                    .into_iter()
                    .map(|f| DeltaOffscreenKeyform {
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
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

    if let Some(info) = display_info {
        let result = candidate.replace_display_info(info);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    for expression in animation_assets.expressions {
        let result = candidate.create_expression(expression);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    for motion in animation_assets.motions {
        let result = candidate.create_motion(motion);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    let result = candidate.set_motion_groups(animation_assets.motion_groups);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    if let Some(pose) = animation_assets.pose {
        let result = candidate.set_pose(pose);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    if let Some(physics) = animation_assets.physics {
        let result = candidate.set_physics(physics);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    let result = candidate.set_missing_attachments(animation_assets.missing_attachments);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    let settings = if root.format_version == 5 {
        crate::model3::migrate_v5_settings(&candidate, animation_assets.model3_settings)?
    } else if root.format_version >= 6 {
        serde_json::from_value(animation_assets.model3_settings)
            .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?
    } else {
        kasane_core::document::Model3Settings::default()
    };
    let result = candidate.set_model3_settings(settings);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    let result = candidate.set_package_attachments(animation_assets.package_attachments);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    let status = candidate.finish_batch_build();
    if !status.is_ok() {
        return Err(status);
    }
    candidate.mark_saved();
    Ok(candidate)
}
