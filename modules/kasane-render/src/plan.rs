use std::collections::{HashMap, HashSet};

use kasane_core::evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand};
use kasane_core::types::{Status, Vec2};

use crate::validate_frame;
use crate::{
    Affine2, DrawItem, MaskKey, PreparedFrame, RenderPass, Size2, TextureCatalog, ViewportConfig,
    OFFSCREEN_BUDGET_BYTES,
};

/// Validate a frame and calculate all logical render-target requirements before
/// a backend allocates any physical resource.
pub fn build_plan<'a, T: TextureCatalog>(
    frame: &'a DrawableFrame,
    textures: &T,
    viewport: ViewportConfig,
) -> Result<PreparedFrame<'a>, Status> {
    prepare_frame(frame, textures, viewport)
}

/// Validate a frame, calculate attachment requirements, and produce the
/// backend-neutral pass stream consumed by render adapters.
pub fn prepare_frame<'a, T: TextureCatalog>(
    frame: &'a DrawableFrame,
    textures: &T,
    viewport: ViewportConfig,
) -> Result<PreparedFrame<'a>, Status> {
    let status = validate_frame(frame);
    if !status.is_ok() {
        return Err(status);
    }
    for drawable in &frame.drawables {
        let texture = textures
            .texture_info(&drawable.texture_asset_id)
            .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
        if texture.width == 0 || texture.height == 0 {
            return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
        }
    }

    let index = FrameIndex::new(frame);
    let active_offscreens = active_offscreens(frame, &index);
    let (surface_size, surface_transform) = offscreen_layout(
        Vec2::new(frame.canvas.width, frame.canvas.height),
        viewport.transform,
        viewport.target_extent,
        active_offscreens.len(),
    )?;
    let surface_bytes = i64::from(surface_size.width)
        * i64::from(surface_size.height)
        * 8
        * active_offscreens.len() as i64;

    let mask_consumers = mask_consumers(frame);
    let mut mask_keys = HashSet::with_capacity(frame.drawables.len() + active_offscreens.len());
    let mut mask_bytes = 0i64;
    for (id, ids, scale) in frame
        .drawables
        .iter()
        .map(|d| (&d.id, &d.masks, viewport.mask_scale))
        .chain(
            frame
                .offscreens
                .iter()
                .filter(|o| active_offscreens.contains(o.id.as_str()))
                .map(|o| (&o.id, &o.masks, viewport.mask_scale.max(1.0))),
        )
    {
        if !ids.is_empty()
            && mask_keys.insert(MaskKey::new(
                ids,
                scale,
                mask_consumers.get(id).map(String::as_str).unwrap_or(""),
            ))
        {
            mask_bytes += mask_reserved_bytes(frame, &index, ids, scale);
        }
    }
    if surface_bytes + mask_bytes > OFFSCREEN_BUDGET_BYTES {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen and mask attachments exceed the shared 512 MiB budget.",
        ));
    }

    let passes = prepare_passes(frame, &index)?;
    let destination_reads = destination_reads(frame);
    Ok(PreparedFrame {
        active_offscreens,
        mask_consumers,
        destination_reads,
        surface_size,
        surface_transform,
        passes,
    })
}

struct FrameIndex<'a> {
    drawables: HashMap<&'a str, &'a Drawable>,
    offscreens: HashMap<&'a str, &'a OffscreenFrame>,
}

impl<'a> FrameIndex<'a> {
    fn new(frame: &'a DrawableFrame) -> Self {
        Self {
            drawables: frame
                .drawables
                .iter()
                .map(|drawable| (drawable.id.as_str(), drawable))
                .collect(),
            offscreens: frame
                .offscreens
                .iter()
                .map(|offscreen| (offscreen.id.as_str(), offscreen))
                .collect(),
        }
    }
}

fn active_offscreens<'a>(frame: &'a DrawableFrame, index: &FrameIndex<'a>) -> HashSet<&'a str> {
    let mut stack = Vec::new();
    let mut active = HashSet::with_capacity(frame.offscreens.len());
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => stack.push(offscreen_id.as_str()),
            RenderCommand::EndOffscreen { .. } => {
                stack.pop();
            }
            RenderCommand::DrawMesh { mesh_id } => {
                let drawable = index.drawables[mesh_id.as_str()];
                if drawable.visible
                    && drawable.opacity > 0.0
                    && !drawable.indices.is_empty()
                    && stack.iter().all(|id| {
                        index.offscreens[id].enabled && index.offscreens[id].opacity > 0.0
                    })
                {
                    active.extend(stack.iter().copied());
                }
            }
        }
    }
    active
}

fn mask_consumers(frame: &DrawableFrame) -> HashMap<String, String> {
    let masked: HashSet<&str> = frame
        .drawables
        .iter()
        .filter(|d| !d.masks.is_empty())
        .map(|d| d.id.as_str())
        .chain(
            frame
                .offscreens
                .iter()
                .filter(|o| !o.masks.is_empty())
                .map(|o| o.id.as_str()),
        )
        .collect();
    let mut consumers = HashMap::with_capacity(masked.len());
    if masked.is_empty() {
        return consumers;
    }

    let mut stack: Vec<&str> = Vec::new();
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => {
                if masked.contains(offscreen_id.as_str()) {
                    consumers.insert(
                        offscreen_id.clone(),
                        stack.last().copied().unwrap_or("").to_owned(),
                    );
                }
                stack.push(offscreen_id.as_str());
            }
            RenderCommand::DrawMesh { mesh_id } => {
                if masked.contains(mesh_id.as_str()) {
                    consumers.insert(
                        mesh_id.clone(),
                        stack.last().copied().unwrap_or("").to_owned(),
                    );
                }
            }
            RenderCommand::EndOffscreen { .. } => {
                stack.pop();
            }
        }
    }
    consumers
}

fn destination_reads(frame: &DrawableFrame) -> HashSet<&str> {
    frame
        .offscreens
        .iter()
        .filter(|offscreen| offscreen.blend_mode != 0)
        .map(|offscreen| offscreen.id.as_str())
        .chain(
            frame
                .drawables
                .iter()
                .filter(|drawable| drawable.raw_blend_mode.is_some())
                .map(|drawable| drawable.id.as_str()),
        )
        .collect()
}

fn prepare_passes<'a>(
    frame: &'a DrawableFrame,
    index: &FrameIndex<'a>,
) -> Result<Vec<RenderPass<'a>>, Status> {
    let mut passes = Vec::with_capacity(
        frame
            .render_plan
            .len()
            .max(frame.drawables.len())
            .saturating_mul(2)
            .saturating_add(1),
    );
    passes.push(RenderPass::Main);

    // A manually submitted flat frame may omit the core render_plan. Preserve
    // the old Godot adapter behavior by producing the same render-order draw
    // sequence in that case.
    if frame.render_plan.is_empty() {
        let mut ordered: Vec<&Drawable> = frame.drawables.iter().collect();
        ordered.sort_by_key(|drawable| drawable.render_order);
        for drawable in ordered {
            push_draw_passes(&mut passes, drawable, None);
        }
        return Ok(passes);
    }

    let mut stack: Vec<&'a str> = Vec::new();
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => {
                let Some(offscreen) = index.offscreens.get(offscreen_id.as_str()) else {
                    return Err(Status::error("INVALID_RENDER_PLAN", offscreen_id));
                };
                let parent = stack.last().copied();
                passes.push(RenderPass::Offscreen {
                    id: offscreen.id.as_str(),
                    parent,
                });
                if !offscreen.masks.is_empty() {
                    passes.push(RenderPass::Mask {
                        target: offscreen.id.as_str(),
                        consumer: parent,
                    });
                }
                passes.push(RenderPass::Composite {
                    id: offscreen.id.as_str(),
                    parent,
                });
                stack.push(offscreen.id.as_str());
            }
            RenderCommand::DrawMesh { mesh_id } => {
                let Some(drawable) = index.drawables.get(mesh_id.as_str()) else {
                    return Err(Status::error("INVALID_RENDER_PLAN", mesh_id));
                };
                push_draw_passes(&mut passes, drawable, stack.last().copied());
            }
            RenderCommand::EndOffscreen { offscreen_id } => {
                if stack.pop() != Some(offscreen_id.as_str()) {
                    return Err(Status::error("INVALID_RENDER_PLAN", offscreen_id));
                }
                passes.push(RenderPass::EndOffscreen {
                    id: offscreen_id.as_str(),
                });
            }
        }
    }
    if !stack.is_empty() {
        return Err(Status::error(
            "INVALID_RENDER_PLAN",
            "Prepared render passes contain an unclosed offscreen.",
        ));
    }
    Ok(passes)
}

fn push_draw_passes<'a>(
    passes: &mut Vec<RenderPass<'a>>,
    drawable: &'a Drawable,
    consumer: Option<&'a str>,
) {
    if !drawable.masks.is_empty() {
        passes.push(RenderPass::Mask {
            target: drawable.id.as_str(),
            consumer,
        });
    }
    passes.push(RenderPass::Draw(DrawItem {
        drawable_id: drawable.id.as_str(),
        texture_id: drawable.texture_asset_id.as_str(),
    }));
}

fn mask_reserved_bytes(
    frame: &DrawableFrame,
    index: &FrameIndex<'_>,
    masks: &[String],
    scale: f64,
) -> i64 {
    if masks.is_empty() {
        return 0;
    }
    let mut bounds: Option<Rect> = None;
    for id in masks {
        if let Some(mesh) = index.drawables.get(id.as_str()) {
            for p in &mesh.positions {
                let point = Vec2::new(
                    p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                    frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit,
                );
                bounds = Some(
                    bounds
                        .map(|b| b.expand(point))
                        .unwrap_or(Rect::new(point, Vec2::new(0.0, 0.0))),
                );
            }
        }
    }
    let bounds = bounds.unwrap_or_default().grow(4.0);
    let width = f64::from(bounds.size.x.ceil().max(1.0));
    let height = f64::from(bounds.size.y.ceil().max(1.0));
    let scale = scale.min(4096.0 / width.max(height));
    (width * scale).ceil().max(2.0) as i64 * (height * scale).ceil().max(2.0) as i64 * 4
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
    if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }

    let source = [
        Vec2::new(0.0, 0.0),
        Vec2::new(canvas.x, 0.0),
        Vec2::new(0.0, canvas.y),
        canvas,
    ];
    let mut bounds = Rect::new(transform.transform_point(source[0]), Vec2::new(0.0, 0.0));
    for point in source.iter().skip(1).copied() {
        bounds = bounds.expand(transform.transform_point(point));
    }
    let bounds = bounds
        .intersect(Rect::new(Vec2::new(0.0, 0.0), target))
        .unwrap_or_default();
    let origin = Vec2::new(bounds.position.x.floor(), bounds.position.y.floor());
    let end = bounds.end();
    let extent = Vec2::new(end.x.ceil() - origin.x, end.y.ceil() - origin.y);
    let size = Size2 {
        width: (extent.x as i32).max(2),
        height: (extent.y as i32).max(2),
    };
    let bytes = i64::from(size.width) * i64::from(size.height) * 8;
    if size.width > 4096 || size.height > 4096 || count > (OFFSCREEN_BUDGET_BYTES / bytes) as usize
    {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            format!(
                "{count} surfaces at {}x{} exceed the {} byte color/destination budget",
                size.width, size.height, OFFSCREEN_BUDGET_BYTES
            ),
        ));
    }
    let mut root = transform;
    root.origin.x -= origin.x;
    root.origin.y -= origin.y;
    Ok((size, root))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    position: Vec2,
    size: Vec2,
}

impl Rect {
    fn new(position: Vec2, size: Vec2) -> Self {
        Self { position, size }
    }

    fn end(self) -> Vec2 {
        Vec2::new(self.position.x + self.size.x, self.position.y + self.size.y)
    }

    fn expand(self, point: Vec2) -> Self {
        let end = self.end();
        let min = Vec2::new(self.position.x.min(point.x), self.position.y.min(point.y));
        let max = Vec2::new(end.x.max(point.x), end.y.max(point.y));
        Self::new(min, Vec2::new(max.x - min.x, max.y - min.y))
    }

    fn grow(self, amount: f32) -> Self {
        Self::new(
            Vec2::new(self.position.x - amount, self.position.y - amount),
            Vec2::new(self.size.x + amount * 2.0, self.size.y + amount * 2.0),
        )
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let min = Vec2::new(
            self.position.x.max(other.position.x),
            self.position.y.max(other.position.y),
        );
        let max = Vec2::new(
            self.end().x.min(other.end().x),
            self.end().y.min(other.end().y),
        );
        if max.x <= min.x || max.y <= min.y {
            None
        } else {
            Some(Self::new(min, Vec2::new(max.x - min.x, max.y - min.y)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureInfo;

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
