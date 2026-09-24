//! Compatibility lowering for consumers of the original pass-stream API.
//! Logical interpretation is shared with ScenePlan.
use crate::{
    surface_layout, Affine2, DrawItem, MaskKey, PreparedFrame, RenderPass, ScenePlan, Size2,
    TargetId, TargetItem, TextureCatalog, ViewportConfig, OFFSCREEN_BUDGET_BYTES,
};
use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::{Status, Vec2};
use std::collections::{HashMap, HashSet};

pub fn build_plan<'a, T: TextureCatalog>(
    frame: &'a DrawableFrame,
    textures: &T,
    viewport: ViewportConfig,
) -> Result<PreparedFrame<'a>, Status> {
    prepare_frame(frame, textures, viewport)
}

pub fn prepare_frame<'a, T: TextureCatalog>(
    frame: &'a DrawableFrame,
    textures: &T,
    viewport: ViewportConfig,
) -> Result<PreparedFrame<'a>, Status> {
    let mut scene = ScenePlan::default();
    scene.update(frame, textures)?;
    let active_offscreens: HashSet<&str> = scene
        .targets()
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, target)| target.active)
        .map(|(i, _)| frame.offscreens[i - 1].id.as_str())
        .collect();
    let (surface_size, surface_transform) = offscreen_layout(
        Vec2::new(frame.canvas.width, frame.canvas.height),
        viewport.transform,
        viewport.target_extent,
        active_offscreens.len(),
    )?;
    let mut bytes = i64::from(surface_size.width)
        * i64::from(surface_size.height)
        * 8
        * active_offscreens.len() as i64;
    let mut mask_consumers = HashMap::new();
    let mut destination_reads = HashSet::new();
    let mut mask_keys = HashSet::new();
    for (i, mesh) in scene.meshes().iter().enumerate() {
        if mesh.reads_destination {
            destination_reads.insert(frame.drawables[i].id.as_str());
        }
    }
    for (i, target) in scene.targets().iter().enumerate().skip(1) {
        if target.reads_destination {
            destination_reads.insert(frame.offscreens[i - 1].id.as_str());
        }
    }
    for (id, sources, scale, mask, consumer, active) in frame
        .drawables
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let mesh = &scene.meshes()[i];
            (
                &d.id,
                &d.masks,
                viewport.mask_scale,
                mesh.mask,
                mesh.target,
                true,
            )
        })
        .chain(frame.offscreens.iter().enumerate().map(|(i, o)| {
            let target = &scene.targets()[i + 1];
            (
                &o.id,
                &o.masks,
                viewport.mask_scale.max(1.0),
                target.mask,
                target.parent,
                target.active,
            )
        }))
    {
        let Some(mask) = mask else {
            continue;
        };
        let consumer = &scene.targets()[consumer.0].id;
        if !frame.render_plan.is_empty() {
            mask_consumers.insert(id.clone(), consumer.clone());
        }
        if active && mask_keys.insert(MaskKey::new(sources, scale, consumer)) {
            let bounds = scene.masks()[mask.0].bounds.grow(4.0);
            let width = f64::from(bounds.size.x.ceil().max(1.0));
            let height = f64::from(bounds.size.y.ceil().max(1.0));
            let density = scale.min(4096.0 / width.max(height));
            bytes += (width * density).ceil().max(2.0) as i64
                * (height * density).ceil().max(2.0) as i64
                * 4;
        }
    }
    if bytes > OFFSCREEN_BUDGET_BYTES {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen and mask attachments exceed the shared 512 MiB budget.",
        ));
    }
    // Preserve legacy assembly order at this compatibility boundary only.
    enum Step {
        Item(TargetItem),
        End(TargetId),
    }
    let target_name = |id: TargetId| {
        if id == TargetId::MAIN {
            None
        } else {
            Some(frame.offscreens[id.0 - 1].id.as_str())
        }
    };
    let mut passes = vec![RenderPass::Main];
    let mut steps: Vec<_> = scene.targets()[0]
        .items
        .iter()
        .rev()
        .copied()
        .map(Step::Item)
        .collect();
    while let Some(step) = steps.pop() {
        match step {
            Step::End(id) => passes.push(RenderPass::EndOffscreen {
                id: target_name(id).unwrap(),
            }),
            Step::Item(TargetItem::Draw(id)) => {
                let mesh = &scene.meshes()[id.0];
                let drawable = &frame.drawables[id.0];
                if mesh.mask.is_some() {
                    passes.push(RenderPass::Mask {
                        target: &drawable.id,
                        consumer: target_name(mesh.target),
                    });
                }
                passes.push(RenderPass::Draw(DrawItem {
                    drawable_id: &drawable.id,
                    texture_id: &drawable.texture_asset_id,
                }));
            }
            Step::Item(TargetItem::Composite(id)) => {
                let target = &scene.targets()[id.0];
                let name = target_name(id).unwrap();
                let parent = target_name(target.parent);
                passes.push(RenderPass::Offscreen { id: name, parent });
                if target.mask.is_some() {
                    passes.push(RenderPass::Mask {
                        target: name,
                        consumer: parent,
                    });
                }
                passes.push(RenderPass::Composite { id: name, parent });
                steps.push(Step::End(id));
                steps.extend(target.items.iter().rev().copied().map(Step::Item));
            }
        }
    }
    Ok(PreparedFrame {
        active_offscreens,
        mask_consumers,
        destination_reads,
        surface_size,
        surface_transform,
        passes,
    })
}

fn offscreen_layout(
    canvas: Vec2,
    transform: Affine2,
    target: Vec2,
    count: usize,
) -> Result<(Size2, Affine2), Status> {
    if count == 0 {
        return Ok((
            Size2 {
                width: 2,
                height: 2,
            },
            Affine2::IDENTITY,
        ));
    }
    let (size, root) = surface_layout(canvas, transform, target)?;
    if size.width > 4096 || size.height > 4096 {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen dimensions exceed 4096 pixels.",
        ));
    }
    let bytes = i64::from(size.width) * i64::from(size.height) * 8;
    if count > (OFFSCREEN_BUDGET_BYTES / bytes) as usize {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen color/destination attachments exceed their size or byte budget.",
        ));
    }
    Ok((size, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureInfo;
    use kasane_core::evaluation::{Drawable, OffscreenFrame, RenderCommand};

    #[test]
    fn fractional_translation_preserves_the_screen_pixel_grid() {
        let transform = Affine2 {
            a: Vec2::new(0.25, 0.0),
            b: Vec2::new(0.0, 0.25),
            origin: Vec2::new(20.25, 30.75),
        };
        let (size, root) = offscreen_layout(
            Vec2::new(5200.0, 7000.0),
            transform,
            Vec2::new(2048.0, 2048.0),
            24,
        )
        .unwrap();
        assert_eq!(
            size,
            Size2 {
                width: 1301,
                height: 1751
            }
        );
        assert_eq!(root.origin, Vec2::new(0.25, 0.75));
    }

    #[test]
    fn zoom_crops_to_target_and_excess_surfaces_fail_before_allocation() {
        let (size, _) = offscreen_layout(
            Vec2::new(5200.0, 7000.0),
            Affine2 {
                a: Vec2::new(100.0, 0.0),
                b: Vec2::new(0.0, 100.0),
                origin: Vec2::new(0.0, 0.0),
            },
            Vec2::new(1024.0, 768.0),
            24,
        )
        .unwrap();
        assert_eq!(
            size,
            Size2 {
                width: 1024,
                height: 768
            }
        );
        assert!(offscreen_layout(
            Vec2::new(4096.0, 4096.0),
            Affine2::IDENTITY,
            Vec2::new(4096.0, 4096.0),
            5
        )
        .is_err());
    }

    fn test_drawable(id: &str, texture: &str, render_order: i32) -> Drawable {
        Drawable {
            id: id.to_owned(),
            texture_asset_id: texture.to_owned(),
            render_order,
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
            uvs: std::sync::Arc::from([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ]),
            indices: std::sync::Arc::from([0, 1, 2]),
            ..Default::default()
        }
    }

    #[test]
    fn prepares_nested_offscreen_passes_and_destination_copies() {
        let mut inner_drawable = test_drawable("inner-mesh", "texture", 2);
        inner_drawable.masks.push("root-mesh".to_owned());
        inner_drawable.raw_blend_mode = Some(0);
        let frame = DrawableFrame {
            canvas: kasane_core::types::Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![
                test_drawable("root-mesh", "texture", 0),
                test_drawable("outer-mesh", "texture", 1),
                inner_drawable,
            ],
            offscreens: vec![
                OffscreenFrame {
                    id: "outer".to_owned(),
                    enabled: true,
                    opacity: 1.0,
                    blend_mode: 1,
                    masks: vec!["root-mesh".to_owned()],
                    ..Default::default()
                },
                OffscreenFrame {
                    id: "inner".to_owned(),
                    parent_offscreen_id: Some("outer".to_owned()),
                    enabled: true,
                    opacity: 1.0,
                    ..Default::default()
                },
            ],
            render_plan: vec![
                RenderCommand::DrawMesh {
                    mesh_id: "root-mesh".to_owned(),
                },
                RenderCommand::BeginOffscreen {
                    offscreen_id: "outer".to_owned(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "outer-mesh".to_owned(),
                },
                RenderCommand::BeginOffscreen {
                    offscreen_id: "inner".to_owned(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "inner-mesh".to_owned(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "inner".to_owned(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "outer".to_owned(),
                },
            ],
            ..Default::default()
        };
        let textures = HashMap::from([(
            "texture".to_owned(),
            TextureInfo {
                width: 64,
                height: 64,
            },
        )]);
        let prepared = prepare_frame(
            &frame,
            &textures,
            ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(640.0, 480.0),
                mask_scale: 1.0,
            },
        )
        .unwrap();

        assert_eq!(
            prepared.passes,
            vec![
                RenderPass::Main,
                RenderPass::Draw(DrawItem {
                    drawable_id: "root-mesh",
                    texture_id: "texture",
                }),
                RenderPass::Offscreen {
                    id: "outer",
                    parent: None,
                },
                RenderPass::Mask {
                    target: "outer",
                    consumer: None,
                },
                RenderPass::Composite {
                    id: "outer",
                    parent: None,
                },
                RenderPass::Draw(DrawItem {
                    drawable_id: "outer-mesh",
                    texture_id: "texture",
                }),
                RenderPass::Offscreen {
                    id: "inner",
                    parent: Some("outer"),
                },
                RenderPass::Composite {
                    id: "inner",
                    parent: Some("outer"),
                },
                RenderPass::Mask {
                    target: "inner-mesh",
                    consumer: Some("inner"),
                },
                RenderPass::Draw(DrawItem {
                    drawable_id: "inner-mesh",
                    texture_id: "texture",
                }),
                RenderPass::EndOffscreen { id: "inner" },
                RenderPass::EndOffscreen { id: "outer" },
            ]
        );
        assert!(prepared.active_offscreens.contains("outer"));
        assert!(prepared.active_offscreens.contains("inner"));
        assert!(prepared.destination_reads.contains("outer"));
        assert!(prepared.destination_reads.contains("inner-mesh"));
    }

    #[test]
    fn prepares_flat_frames_without_a_core_render_plan_in_render_order() {
        let frame = DrawableFrame {
            canvas: kasane_core::types::Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![
                test_drawable("late", "texture", 20),
                test_drawable("early", "texture", 10),
            ],
            ..Default::default()
        };
        let textures = HashMap::from([(
            "texture".to_owned(),
            TextureInfo {
                width: 64,
                height: 64,
            },
        )]);
        let prepared = prepare_frame(
            &frame,
            &textures,
            ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(640.0, 480.0),
                mask_scale: 1.0,
            },
        )
        .unwrap();
        assert_eq!(
            prepared.passes,
            vec![
                RenderPass::Main,
                RenderPass::Draw(DrawItem {
                    drawable_id: "early",
                    texture_id: "texture",
                }),
                RenderPass::Draw(DrawItem {
                    drawable_id: "late",
                    texture_id: "texture",
                }),
            ]
        );
    }
}
