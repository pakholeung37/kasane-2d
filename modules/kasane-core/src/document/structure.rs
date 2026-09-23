//! Read-only validation of the complete persistent document graph.
use super::*;
use crate::draw_order::validate_groups;
use std::collections::HashSet;

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

        // Every replacement reuses the normal core validator. The scratch graph
        // is isolated, so canonicalization and cache invalidation cannot affect
        // the source document or its revision.
        let mut scratch = self.fork_candidate();
        macro_rules! check_replacements {
            ($map:ident, $replace:ident) => {
                for (id, object) in &self.$map {
                    let result = scratch.$replace(object.clone());
                    if !result.status.is_ok() {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: result.status,
                        });
                    }
                }
            };
        }
        check_replacements!(assets, replace_asset);
        check_replacements!(parts, replace_part);
        check_replacements!(transforms, replace_transform);
        check_replacements!(meshes, replace_mesh);
        check_replacements!(parameters, replace_parameter);
        check_replacements!(bindings, replace_binding);
        check_replacements!(scene_bindings, replace_scene_binding);
        check_replacements!(blend_key_tables, replace_blend_key_table);
        check_replacements!(blend_constraints, replace_blend_constraint);
        check_replacements!(blend_bindings, replace_blend_binding);
        check_replacements!(glues, replace_glue);
        check_replacements!(offscreens, replace_offscreen);
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
