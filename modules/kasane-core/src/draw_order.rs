//! Drawing groups are independent of the Part organization hierarchy.
use crate::{Document, Status};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawOrderGroup {
    /// Empty for the root; otherwise the stable ID of the Part driving this group.
    pub owner: String,
    /// Stable mesh/Part IDs, in tie-breaking order. A Part references its group.
    pub items: Vec<String>,
    pub min_order: i32,
    pub max_order: i32,
}

pub(crate) fn validate_groups(
    doc: &Document,
    groups: &[DrawOrderGroup],
) -> Result<Vec<DrawOrderGroup>, Status> {
    let fail = |message| Status::error("INVALID_DRAW_GROUP", message);
    let mut owners = HashMap::new();
    let mut items = HashSet::new();
    for (i, group) in groups.iter().enumerate() {
        if (!group.owner.is_empty() && doc.get_part(&group.owner).is_none())
            || owners.insert(group.owner.as_str(), i).is_some()
            || group.min_order < -32768
            || group.max_order > 32767
            || group.min_order > group.max_order
        {
            return Err(fail(format!(
                "Invalid group owner or range: {}",
                group.owner
            )));
        }
        for id in &group.items {
            if (doc.get_mesh(id).is_none() && doc.get_part(id).is_none())
                || !items.insert(id.as_str())
            {
                return Err(fail(format!("Missing or repeated drawing object: {id}")));
            }
        }
    }
    if !owners.contains_key("")
        || doc
            .mesh_order()
            .iter()
            .any(|id| !items.contains(id.as_str()))
    {
        return Err(fail("Missing root or mesh in drawing groups".into()));
    }
    for id in &items {
        if doc.get_part(id).is_some() && !owners.contains_key(id) {
            return Err(fail(format!("Part {id} has no child drawing group")));
        }
    }
    // Canonical parent-first order, rejecting detached groups and cycles.
    let mut order = vec![owners[""]];
    let mut visited = HashSet::from([owners[""]]);
    let mut cursor = 0;
    while cursor < order.len() {
        for id in &groups[order[cursor]].items {
            if let Some(&child) = owners.get(id.as_str()) {
                if !visited.insert(child) {
                    return Err(fail(format!("Drawing group cycle: {id}")));
                }
                order.push(child);
            }
        }
        cursor += 1;
    }
    if order.len() != groups.len() {
        return Err(fail("Detached drawing groups or cycle".into()));
    }
    Ok(order.into_iter().map(|i| groups[i].clone()).collect())
}

/// Existing documents derive groups from Parts; imports retain explicit groups.
/// Expand stored bounds when edits introduce orders outside the source range.
pub fn resolved_groups(doc: &Document) -> Vec<DrawOrderGroup> {
    let mut groups = if let Some(groups) = doc.draw_order_groups() {
        groups.to_vec()
    } else {
        let parts = doc.sorted_parts();
        std::iter::once(String::new())
            .chain(parts.iter().cloned())
            .map(|owner| {
                let items = doc
                    .mesh_order()
                    .iter()
                    .filter(|id| doc.get_mesh(id).unwrap().part_id == owner)
                    .cloned()
                    .chain(
                        parts
                            .iter()
                            .filter(|id| doc.get_part(id).unwrap().parent_id == owner)
                            .cloned(),
                    )
                    .collect();
                DrawOrderGroup {
                    owner,
                    items,
                    min_order: 0,
                    max_order: 0,
                }
            })
            .collect()
    };
    let mesh_slots: HashMap<&str, usize> = doc
        .mesh_order()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for group in &mut groups {
        let mut lo = group.min_order as f32;
        let mut hi = group.max_order as f32;
        for id in &group.items {
            let mut add = |value: f32| {
                lo = lo.min(value);
                hi = hi.max(value);
            };
            if let Some(mesh) = doc.get_mesh(id) {
                let base = mesh.draw_order.unwrap_or(mesh_slots[id.as_str()] as f32);
                add(base);
                if let Some(binding) = doc.binding_for_mesh(id) {
                    for form in &binding.keyforms {
                        add(form.draw_order.unwrap_or(base));
                    }
                }
            } else {
                add(doc.get_part(id).unwrap().draw_order);
                if let Some(binding) = doc.binding_for_scene(id) {
                    for form in &binding.keyforms {
                        add(form.draw_order);
                    }
                }
            }
        }
        group.min_order = lo.floor() as i32;
        group.max_order = hi.ceil() as i32;
    }
    groups
}

pub fn descendant_counts(groups: &[DrawOrderGroup]) -> HashMap<&str, usize> {
    let mut totals = HashMap::new();
    for group in groups.iter().rev() {
        let count = group
            .items
            .iter()
            .map(|id| totals.get(id.as_str()).copied().unwrap_or(1))
            .sum();
        totals.insert(group.owner.as_str(), count);
    }
    totals
}

pub fn descendant_counts_with_offscreens<'a>(
    doc: &'a Document,
    groups: &'a [DrawOrderGroup],
) -> HashMap<&'a str, usize> {
    let mut totals = HashMap::new();
    for group in groups.iter().rev() {
        let count = group
            .items
            .iter()
            .map(|id| {
                let base = totals.get(id.as_str()).copied().unwrap_or(1);
                let os_count = if doc.offscreen_for_part(id).is_some() {
                    1
                } else {
                    0
                };
                base + os_count
            })
            .sum();
        totals.insert(group.owner.as_str(), count);
    }
    totals
}
