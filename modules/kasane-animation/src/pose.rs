//! Framework-style pose fade state on real or virtual Part control parameters.
use std::collections::BTreeMap;

use kasane_core::document::PosePartRef;
use kasane_core::Document;

pub(crate) struct PoseRuntime {
    pub(crate) opacities: BTreeMap<String, f32>,
}

impl PoseRuntime {
    pub(crate) fn new(
        document: &Document,
        real: &mut BTreeMap<String, f32>,
        virtual_controls: &mut BTreeMap<String, f32>,
    ) -> Self {
        let mut state = Self {
            opacities: document
                .part_order()
                .iter()
                .map(|id| (id.clone(), 1.0))
                .collect(),
        };
        state.reset(document, real, virtual_controls);
        state
    }

    pub(crate) fn reset(
        &mut self,
        document: &Document,
        real: &mut BTreeMap<String, f32>,
        virtual_controls: &mut BTreeMap<String, f32>,
    ) {
        self.opacities = document
            .part_order()
            .iter()
            .map(|id| (id.clone(), 1.0))
            .collect();
        virtual_controls.clear();
        let Some(pose) = document.pose() else {
            return;
        };
        for group in &pose.groups {
            for (index, entry) in group.iter().enumerate() {
                if let PosePartRef::Resolved { part_id } = &entry.part {
                    self.opacities
                        .insert(part_id.clone(), if index == 0 { 1.0 } else { 0.0 });
                    set_control(
                        document,
                        real,
                        virtual_controls,
                        part_id,
                        if index == 0 { 1.0 } else { 0.0 },
                    );
                    for link in entry.links.iter().flatten() {
                        if let PosePartRef::Resolved { part_id } = link {
                            set_control(document, real, virtual_controls, part_id, 1.0);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn update(
        &mut self,
        document: &Document,
        real: &BTreeMap<String, f32>,
        virtual_controls: &BTreeMap<String, f32>,
        dt: f32,
        coverage: &mut Vec<String>,
    ) {
        let Some(pose) = document.pose() else {
            return;
        };
        let fade = pose.fade_in.unwrap_or(0.5);
        for (group_index, group) in pose.groups.iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let mut visible = None;
            let mut new_opacity = 1.0;
            for (index, entry) in group.iter().enumerate() {
                let PosePartRef::Resolved { part_id } = &entry.part else {
                    coverage.push(format!(
                        "pose group {group_index} entry {index} has unresolved Part"
                    ));
                    continue;
                };
                if control(document, real, virtual_controls, part_id) > 0.001 {
                    if visible.is_some() {
                        break;
                    }
                    visible = Some(index);
                    if fade == 0.0 {
                        new_opacity = 1.0;
                        continue;
                    }
                    new_opacity = (self.opacities.get(part_id).copied().unwrap_or(1.0)
                        + dt.max(0.0) / fade)
                        .min(1.0);
                }
            }
            let visible = visible.unwrap_or(0);
            if group.iter().all(|entry| match &entry.part {
                PosePartRef::Resolved { part_id } => {
                    control(document, real, virtual_controls, part_id) <= 0.001
                }
                PosePartRef::Unresolved { .. } => true,
            }) {
                new_opacity = 1.0;
            }
            for (index, entry) in group.iter().enumerate() {
                let PosePartRef::Resolved { part_id } = &entry.part else {
                    continue;
                };
                if index == visible {
                    self.opacities.insert(part_id.clone(), new_opacity);
                } else {
                    // Framework's two Phi = 0.5 branches reduce to 1 - newOpacity.
                    let mut alpha = 1.0 - new_opacity;
                    let back = (1.0 - alpha) * (1.0 - new_opacity);
                    if back > 0.15 {
                        alpha = 1.0 - 0.15 / (1.0 - new_opacity);
                    }
                    let previous = self.opacities.get(part_id).copied().unwrap_or(1.0);
                    self.opacities.insert(part_id.clone(), previous.min(alpha));
                }
            }
        }
        for group in &pose.groups {
            for entry in group {
                let PosePartRef::Resolved { part_id } = &entry.part else {
                    continue;
                };
                let alpha = self.opacities.get(part_id).copied().unwrap_or(1.0);
                for link in entry.links.iter().flatten() {
                    if let PosePartRef::Resolved { part_id } = link {
                        self.opacities.insert(part_id.clone(), alpha);
                    }
                }
            }
        }
    }
}

pub(crate) fn real_parameter_for_part(document: &Document, part_id: &str) -> Option<String> {
    let runtime_id = &document.get_part(part_id)?.runtime_id;
    document
        .parameter_order()
        .iter()
        .find(|id| {
            document
                .get_parameter(id)
                .is_some_and(|parameter| &parameter.runtime_id == runtime_id)
        })
        .cloned()
}

fn control(
    document: &Document,
    real: &BTreeMap<String, f32>,
    virtual_controls: &BTreeMap<String, f32>,
    part_id: &str,
) -> f32 {
    if let Some(id) = real_parameter_for_part(document, part_id) {
        real.get(&id).copied().unwrap_or(0.0)
    } else {
        virtual_controls.get(part_id).copied().unwrap_or(0.0)
    }
}

fn set_control(
    document: &Document,
    real: &mut BTreeMap<String, f32>,
    virtual_controls: &mut BTreeMap<String, f32>,
    part_id: &str,
    value: f32,
) {
    if let Some(id) = real_parameter_for_part(document, part_id) {
        real.insert(id, value);
    } else {
        virtual_controls.insert(part_id.into(), value);
    }
}
