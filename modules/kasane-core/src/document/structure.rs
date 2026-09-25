//! Read-only validation of the complete persistent document graph.
use super::*;
use crate::draw_order::validate_groups;
use crate::geometry::validate_render_mesh;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureIssue {
    pub object_id: String,
    pub status: Status,
}

impl Document {
    /// Validate all persistent collections and references without reading resources
    /// or changing this document. Restored checkpoints use the same checks.
    pub fn validate_structure(&self) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        let mut initialized = Document::new();
        let status = initialized.initialize(self.id.clone(), self.canvas);
        if !status.is_ok() {
            issues.push(StructureIssue {
                object_id: self.id.clone(),
                status,
            });
        }

        let mut all_ids = HashSet::new();
        all_ids.insert(self.id.clone());
        macro_rules! check_collection {
            ($map:ident, $order:ident) => {{
                let mut ordered = HashSet::new();
                for id in &self.$order {
                    if !ordered.insert(id.clone()) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("DUPLICATE_ORDER_ID", "Repeated object in order"),
                        });
                    }
                    if !self.$map.contains_key(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error(
                                "MISSING_OBJECT",
                                "Order references absent object",
                            ),
                        });
                    }
                }
                for (id, object) in &self.$map {
                    if !ordered.contains(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("UNORDERED_OBJECT", "Object missing from order"),
                        });
                    }
                    if object.id != *id || !valid_uuid(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("INVALID_ID", "Object ID or map key is invalid"),
                        });
                    }
                    if !all_ids.insert(id.clone()) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("DUPLICATE_ID", "ID is used by another object"),
                        });
                    }
                }
            }};
        }
        check_collection!(assets, asset_order);
        check_collection!(parts, part_order);
        check_collection!(transforms, transform_order);
        check_collection!(meshes, mesh_order);
        check_collection!(parameters, parameter_order);
        check_collection!(bindings, binding_order);
        check_collection!(scene_bindings, scene_binding_order);
        check_collection!(blend_key_tables, blend_key_table_order);
        check_collection!(blend_constraints, blend_constraint_order);
        check_collection!(blend_bindings, blend_binding_order);
        check_collection!(glues, glue_order);
        check_collection!(offscreens, offscreen_order);
        check_collection!(expressions, expression_order);
        check_collection!(motions, motion_order);
        if let Some(pose) = &self.pose {
            if !all_ids.insert(pose.id.clone()) {
                issues.push(StructureIssue {
                    object_id: pose.id.clone(),
                    status: Status::error("DUPLICATE_ID", "Pose ID is used by another object"),
                });
            }
        }
        if let Some(physics) = &self.physics {
            if !all_ids.insert(physics.id.clone()) {
                issues.push(StructureIssue {
                    object_id: physics.id.clone(),
                    status: Status::error("DUPLICATE_ID", "Physics ID is used by another object"),
                });
            }
        }
        for motion in self.motions.values() {
            for id in motion
                .tracks
                .iter()
                .map(|track| &track.id)
                .chain(motion.events.iter().map(|event| &event.id))
            {
                if !all_ids.insert(id.clone()) {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "DUPLICATE_ID",
                            "Motion child ID is used by another object",
                        ),
                    });
                }
            }
        }
        for id in self
            .display_info
            .parameter_groups
            .as_ref()
            .into_iter()
            .flatten()
            .map(|group| &group.id)
            .chain(
                self.display_info
                    .combined_parameters
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|set| &set.id),
            )
        {
            if !all_ids.insert(id.clone()) {
                issues.push(StructureIssue {
                    object_id: id.clone(),
                    status: Status::error("DUPLICATE_ID", "CDI identity is used by another object"),
                });
            }
        }
        issues.extend(self.validate_display_info());
        issues.extend(self.validate_expressions());
        issues.extend(self.validate_model3());
        issues.extend(self.validate_motions());
        issues.extend(self.validate_pose_asset());
        issues.extend(self.validate_physics_asset());

        let mut add_issue = |id: &str, status: Status| {
            if !status.is_ok() {
                issues.push(StructureIssue {
                    object_id: id.into(),
                    status,
                });
            }
        };
        for (id, asset) in &self.assets {
            if asset.width == 0 || asset.height == 0 || asset.source.is_empty() {
                add_issue(id, Status::error("INVALID_ASSET", id));
            }
        }
        for (id, part) in &self.parts {
            add_issue(id, self.validate_part(part));
        }
        for (id, transform) in &self.transforms {
            add_issue(id, self.validate_transform(transform));
        }
        for (id, mesh) in &self.meshes {
            let status = self.validate_mesh_snapshot(mesh);
            add_issue(id, status);
        }
        for (id, parameter) in &self.parameters {
            add_issue(id, self.validate_parameter(parameter));
        }
        for (id, binding) in &self.bindings {
            add_issue(id, self.canonicalize_binding(&mut binding.clone()));
        }
        for (id, binding) in &self.scene_bindings {
            add_issue(id, self.canonicalize_scene_binding(&mut binding.clone()));
        }
        for (id, table) in &self.blend_key_tables {
            add_issue(id, self.validate_blend_key_table(table));
        }
        for (id, constraint) in &self.blend_constraints {
            add_issue(id, self.validate_blend_constraint(constraint));
        }
        for (id, binding) in &self.blend_bindings {
            add_issue(id, self.validate_blend_binding(binding));
        }
        for (id, glue) in &self.glues {
            let duplicate = self
                .glues
                .iter()
                .any(|(other_id, other)| other_id != id && other.runtime_id == glue.runtime_id);
            add_issue(
                id,
                if duplicate {
                    Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id", glue.id))
                } else {
                    self.validate_glue(glue)
                },
            );
        }
        for (id, offscreen) in &self.offscreens {
            let duplicate = self.offscreens.iter().any(|(other_id, other)| {
                other_id != id && other.runtime_id == offscreen.runtime_id
            });
            add_issue(
                id,
                if duplicate {
                    Status::error(
                        "DUPLICATE_RUNTIME_ID",
                        format!("{}.runtime_id", offscreen.id),
                    )
                } else {
                    self.validate_offscreen(offscreen)
                },
            );
        }
        if let Some(groups) = &self.draw_order_groups {
            if let Err(status) = validate_groups(self, groups) {
                issues.push(StructureIssue {
                    object_id: self.id.clone(),
                    status,
                });
            }
        }
        issues.sort_by(|a, b| {
            (&a.object_id, &a.status.code, &a.status.message).cmp(&(
                &b.object_id,
                &b.status.code,
                &b.status.message,
            ))
        });
        issues
    }

    fn validate_mesh_snapshot(&self, mesh: &Mesh) -> Status {
        if !self.assets.contains_key(&mesh.texture_asset_id) {
            return Status::error("MISSING_ASSET", "Texture asset does not exist.");
        }
        let status = self.validate_mesh_properties(mesh);
        if !status.is_ok() {
            return status;
        }
        if let Some(id) = self.mesh_runtime_ids.get(&mesh.runtime_id) {
            if id != &mesh.id {
                return Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", mesh.id, id),
                );
            }
        }
        if mesh.vertex_ids.len() != mesh.base_positions.len() {
            return Status::error("INVALID_LENGTH", "Vertex IDs must match positions.");
        }
        let slots: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        if slots.len() != mesh.vertex_ids.len() {
            return Status::error(
                "DUPLICATE_VERTEX",
                "Vertex IDs must be unique within a mesh.",
            );
        }
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for id in triangle {
                let Some(slot) = slots.get(id) else {
                    return Status::error(
                        "MISSING_VERTEX",
                        "Triangle references an unknown vertex ID.",
                    );
                };
                indices.push(*slot as u32);
            }
        }
        validate_render_mesh(&mesh.base_positions, &mesh.uvs, &indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Vec2};

    const DOC: &str = "00000000-0000-4000-8000-000000000701";
    const PART: &str = "00000000-0000-4000-8000-000000000702";

    #[test]
    fn restored_checkpoint_is_checked_without_mutating_the_document() {
        let mut source = Document::new();
        assert!(source
            .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
            .is_ok());
        assert!(source
            .create_part(Part {
                id: PART.into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let mut restored = source.clone();
        assert!(restored.validate_structure().is_empty());

        source.parts.get_mut(PART).unwrap().parent_id = PART.into();
        source.part_order.push(PART.into());
        let mut checkpoint = source.checkpoint();
        restored.exchange_checkpoint(&mut checkpoint).unwrap();
        let revision = restored.revision();
        let issues = restored.validate_structure();
        assert!(issues
            .iter()
            .any(|issue| issue.status.code == "RELATION_CYCLE"));
        assert!(issues
            .iter()
            .any(|issue| issue.status.code == "DUPLICATE_ORDER_ID"));
        assert_eq!(restored.revision(), revision);
        assert_eq!(restored.get_part(PART).unwrap().parent_id, PART);
    }
}
