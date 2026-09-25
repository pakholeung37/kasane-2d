use kasane_core::types::{BlendMode, DeltaKeyforms, Status, TransformKind};
use kasane_core::Document;

use super::types::*;

pub fn encode_project(document: &Document) -> Result<String, Status> {
    let project = encode_wire(document)?;
    let mut out = serde_json::to_string(&project)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    out.push('\n');
    Ok(out)
}

pub(super) fn encode_wire(document: &Document) -> Result<ProjectWire, Status> {
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
                raw_blend_mode: m.raw_blend_mode,
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
            part_id: t.part().to_owned(),
            parent_id: t.parent().to_owned(),
            kind: match t.kind() {
                TransformKind::Rotation => 1,
                TransformKind::Warp => 0,
            },
            base_angle: t.rotation().map_or(0.0, |r| r.base_angle),
            rotation: RotationPoseWire::from(&t.rotation().map(|r| r.pose).unwrap_or_default()),
            rows: t.warp().map_or(1, |w| w.rows),
            columns: t.warp().map_or(1, |w| w.columns),
            quad: t.warp().is_none_or(|w| w.quad),
            enabled: t.enabled,
            points: t
                .warp()
                .into_iter()
                .flat_map(|w| w.points.iter())
                .map(|p| [p.x, p.y])
                .collect(),
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
            .track
            .samples()
            .map(|f| SceneKeyformWire {
                keys: f.keys.to_vec(),
                positions: f.positions.iter().map(|p| [p.x, p.y]).collect(),
                rotation: RotationPoseWire::from(&f.rotation),
                appearance: Some(AppearanceWire::from(&f.appearance)),
                draw_order: f.draw_order,
            })
            .collect();
        scene_bindings_wire.push(SceneBindingWire {
            id: sb.id.clone(),
            target_id: sb.target_id().to_owned(),
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
            DeltaKeyforms::Glue(forms) => DeltaKeyformsWire::Glue(
                forms
                    .iter()
                    .map(|f| DeltaGlueKeyformWire {
                        intensity: f.intensity,
                    })
                    .collect(),
            ),
            DeltaKeyforms::Offscreen(forms) => DeltaKeyformsWire::Offscreen(
                forms
                    .iter()
                    .map(|f| DeltaOffscreenKeyformWire {
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
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
            binding: g.binding.clone(),
            binding_id: None,
        });
    }

    let mut offscreens_wire = Vec::with_capacity(document.offscreen_order().len());
    for id in document.offscreen_order() {
        let o = document.get_offscreen(id).unwrap();
        offscreens_wire.push(OffscreenWire {
            id: o.id.clone(),
            runtime_id: o.runtime_id.clone(),
            name: o.name.clone(),
            part_id: o.part_id.clone(),
            blend_mode: o.blend_mode,
            flags: o.flags,
            masks: o.masks.clone(),
            part_keyform_indices: o.part_keyform_indices.clone(),
            keyforms: o.keyforms.iter().map(OffscreenKeyformWire::from).collect(),
        });
    }

    let mut assets_wire = Vec::with_capacity(document.asset_order().len());
    for id in document.asset_order() {
        assets_wire.push(document.get_asset(id).unwrap().clone());
    }

    let project = ProjectWire {
        format: "kasane-directory-project".to_string(),
        format_version: 4,
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
            offscreens: offscreens_wire,
            deformers: None,
            deformation_links: None,
            organization_links: None,
        },
    };

    Ok(project)
}
