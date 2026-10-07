//! Palette order is independent of object storage and runtime draw order.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Independent palette ranks for the organization and deformation projections.
/// Partial lists are allowed: consumers retain source order for unlisted objects.
/// Entries use globally unique scene object IDs, never visible row indices.
pub struct HierarchyOrder {
    #[serde(default)]
    pub organization: Vec<String>,
    #[serde(default)]
    pub deformation: Vec<String>,
}

impl HierarchyOrder {
    pub fn is_empty(&self) -> bool {
        self.organization.is_empty() && self.deformation.is_empty()
    }
}

impl Document {
    /// Palette ordering has no effect on relationships or evaluated draw order.
    pub fn hierarchy_order(&self) -> &HierarchyOrder {
        &self.hierarchy_order
    }

    /// Validate and replace both display orders as one metadata edit.
    pub fn replace_hierarchy_order(&mut self, order: HierarchyOrder) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.hierarchy_order_issues(&order).into_iter().next() {
            return self.failed(issue.status);
        }
        if self.hierarchy_order == order {
            return self.failed(Status::ok());
        }
        self.hierarchy_order = order;
        self.changed(ChangeKind::Metadata, Vec::new(), vec![self.id.clone()])
    }

    pub(super) fn hierarchy_order_issues(&self, order: &HierarchyOrder) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        for (ids, organization) in [(&order.organization, true), (&order.deformation, false)] {
            let mut seen = HashSet::new();
            for id in ids {
                let valid = self.meshes.contains_key(id)
                    || self.transforms.contains_key(id)
                    || (organization
                        && (self.parts.contains_key(id) || self.offscreens.contains_key(id)));
                if !valid || !seen.insert(id) {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "INVALID_HIERARCHY_ORDER",
                            "Hierarchy order contains a missing, incompatible or repeated object",
                        ),
                    });
                }
            }
        }
        issues
    }
}
