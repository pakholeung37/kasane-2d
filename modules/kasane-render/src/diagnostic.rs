//! Immutable color/target selection beside the ordinary ScenePlan.

use std::collections::{HashMap, HashSet};

use kasane_core::evaluation::{DrawableFrame, RenderCommand};
use kasane_core::{BlendMode, Status};

/// A diagnostic selection keeps the original target order and raw mask inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticPlan {
    pub color_mesh_ids: Vec<String>,
    pub mask_only_mesh_ids: Vec<String>,
    pub ancestor_target_ids: Vec<String>,
    pub destination_context: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticOverrides {
    pub ignore_masks: bool,
    pub ignore_opacity: bool,
    pub include_disabled: bool,
    pub normalize_blend_for_coverage: bool,
}

impl DiagnosticPlan {
    pub fn isolated(frame: &DrawableFrame, focus: &[String]) -> Result<Self, Status> {
        if focus.is_empty() || focus.len() > 256 {
            return Err(Status::error(
                "INVALID_DIAGNOSTIC_FOCUS",
                "Isolation needs 1..256 mesh IDs",
            ));
        }
        let meshes: HashMap<_, _> = frame
            .drawables
            .iter()
            .map(|mesh| (mesh.id.as_str(), mesh))
            .collect();
        let groups: HashMap<_, _> = frame
            .offscreens
            .iter()
            .map(|group| (group.id.as_str(), group))
            .collect();
        let mut color = HashSet::new();
        for id in focus {
            if !meshes.contains_key(id.as_str()) || !color.insert(id.as_str()) {
                return Err(Status::error("INVALID_DIAGNOSTIC_FOCUS", id));
            }
        }
        let mut paths = HashMap::<&str, Vec<&str>>::new();
        let mut stack = Vec::<&str>::new();
        for command in &frame.render_plan {
            match command {
                RenderCommand::BeginOffscreen { offscreen_id } => stack.push(offscreen_id),
                RenderCommand::EndOffscreen { .. } => {
                    stack.pop();
                }
                RenderCommand::DrawMesh { mesh_id } => {
                    paths.insert(mesh_id, stack.clone());
                }
            }
        }
        let mut target_set = HashSet::new();
        let mut masks = HashSet::new();
        for id in focus {
            let mesh = meshes[id.as_str()];
            masks.extend(mesh.masks.iter().map(String::as_str));
            for &target in paths.get(id.as_str()).into_iter().flatten() {
                target_set.insert(target);
                let group = groups
                    .get(target)
                    .ok_or_else(|| Status::error("INVALID_DIAGNOSTIC_TARGET", target))?;
                masks.extend(group.masks.iter().map(String::as_str));
            }
        }
        if masks.iter().any(|id| !meshes.contains_key(id)) {
            return Err(Status::error(
                "INVALID_DIAGNOSTIC_MASK",
                "Mask source is missing",
            ));
        }
        let color_mesh_ids = frame
            .drawables
            .iter()
            .filter(|mesh| color.contains(mesh.id.as_str()))
            .map(|mesh| mesh.id.clone())
            .collect();
        let mask_only_mesh_ids = frame
            .drawables
            .iter()
            .filter(|mesh| masks.contains(mesh.id.as_str()) && !color.contains(mesh.id.as_str()))
            .map(|mesh| mesh.id.clone())
            .collect();
        let ancestor_target_ids = frame
            .render_plan
            .iter()
            .filter_map(|command| match command {
                RenderCommand::BeginOffscreen { offscreen_id }
                    if target_set.contains(offscreen_id.as_str()) =>
                {
                    Some(offscreen_id.clone())
                }
                _ => None,
            })
            .collect();
        Ok(Self {
            color_mesh_ids,
            mask_only_mesh_ids,
            ancestor_target_ids,
            destination_context: "isolated".into(),
        })
    }

    /// The derived frame shares no mutable state with the captured source.
    /// Mask sources keep their geometry/texture even when their color draw is hidden.
    pub fn apply(&self, source: &DrawableFrame) -> DrawableFrame {
        self.apply_with_overrides(source, DiagnosticOverrides::default())
    }

    pub fn apply_with_overrides(
        &self,
        source: &DrawableFrame,
        overrides: DiagnosticOverrides,
    ) -> DrawableFrame {
        let mut derived = source.clone();
        let color: HashSet<_> = self.color_mesh_ids.iter().map(String::as_str).collect();
        let targets: HashSet<_> = self
            .ancestor_target_ids
            .iter()
            .map(String::as_str)
            .collect();
        for mesh in &mut derived.drawables {
            if !color.contains(mesh.id.as_str()) {
                mesh.visible = false;
            } else {
                if overrides.ignore_masks {
                    mesh.masks.clear();
                    mesh.inverted_mask = false;
                }
                if overrides.ignore_opacity {
                    mesh.opacity = 1.0;
                }
                if overrides.include_disabled {
                    mesh.enabled = true;
                    mesh.visible = true;
                }
                if overrides.normalize_blend_for_coverage {
                    mesh.blend_mode = BlendMode::Normal;
                    mesh.raw_blend_mode = None;
                }
            }
        }
        for group in &mut derived.offscreens {
            if targets.contains(group.id.as_str()) {
                if overrides.ignore_masks {
                    group.masks.clear();
                    group.flags &= !8;
                }
                if overrides.ignore_opacity {
                    group.opacity = 1.0;
                }
                if overrides.include_disabled {
                    group.enabled = true;
                }
                if overrides.normalize_blend_for_coverage {
                    group.blend_mode = 0;
                }
            }
        }
        derived
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::evaluation::{Drawable, OffscreenFrame};

    #[test]
    fn selection_keeps_mask_only_source_and_nested_target_order() {
        let mut frame = DrawableFrame {
            drawables: ["source", "target", "other"]
                .map(|id| Drawable {
                    id: id.into(),
                    visible: true,
                    ..Drawable::default()
                })
                .to_vec(),
            ..DrawableFrame::default()
        };
        frame.drawables[1].masks = vec!["source".into()];
        frame.offscreens = vec![OffscreenFrame {
            id: "layer".into(),
            masks: vec!["other".into()],
            ..OffscreenFrame::default()
        }];
        frame.render_plan = vec![
            RenderCommand::DrawMesh {
                mesh_id: "source".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "layer".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "target".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "layer".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "other".into(),
            },
        ];
        let plan = DiagnosticPlan::isolated(&frame, &["target".into()]).unwrap();
        assert_eq!(plan.color_mesh_ids, ["target"]);
        assert_eq!(plan.mask_only_mesh_ids, ["source", "other"]);
        assert_eq!(plan.ancestor_target_ids, ["layer"]);
        let derived = plan.apply(&frame);
        assert!(!derived.drawables[0].visible);
        assert!(derived.drawables[1].visible);
        assert!(!derived.drawables[2].visible);
        assert_eq!(derived.render_plan, frame.render_plan);
        assert!(frame.drawables.iter().all(|mesh| mesh.visible));
    }

    #[test]
    fn coverage_normalizes_only_selected_blends_without_mutating_source() {
        let mut frame = DrawableFrame {
            drawables: vec![Drawable {
                id: "target".into(),
                blend_mode: BlendMode::Additive,
                raw_blend_mode: Some(3),
                visible: true,
                ..Drawable::default()
            }],
            offscreens: vec![OffscreenFrame {
                id: "layer".into(),
                blend_mode: 1,
                ..OffscreenFrame::default()
            }],
            render_plan: vec![
                RenderCommand::BeginOffscreen {
                    offscreen_id: "layer".into(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "target".into(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "layer".into(),
                },
            ],
            ..DrawableFrame::default()
        };
        let plan = DiagnosticPlan::isolated(&frame, &["target".into()]).unwrap();
        let derived = plan.apply_with_overrides(
            &frame,
            DiagnosticOverrides {
                normalize_blend_for_coverage: true,
                ..DiagnosticOverrides::default()
            },
        );
        assert_eq!(derived.drawables[0].blend_mode, BlendMode::Normal);
        assert_eq!(derived.drawables[0].raw_blend_mode, None);
        assert_eq!(derived.offscreens[0].blend_mode, 0);
        assert_eq!(frame.drawables[0].blend_mode, BlendMode::Additive);
        assert_eq!(frame.drawables[0].raw_blend_mode, Some(3));
        assert_eq!(frame.offscreens[0].blend_mode, 1);
        frame.drawables[0].visible = false;
        assert!(derived.drawables[0].visible);
    }
}
