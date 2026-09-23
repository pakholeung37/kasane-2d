//! Persistent logical scene description. No frame positions or host handles are retained.
use std::collections::HashMap;

use kasane_core::evaluation::{DrawableFrame, RenderCommand};
use kasane_core::types::{Canvas, Status, Vec2};

use crate::{Affine2, Size2, TextureCatalog};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MeshId(pub usize);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);
impl TargetId {
    pub const MAIN: Self = Self(0);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaskId(pub usize);

/// Ordering is local to a target. A composite consumes the completed child
/// target; a destination-reading item reads its own target immediately before
/// the item executes. Mask inputs are raw mesh geometry/texture, not color draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetItem {
    Draw(MeshId),
    Composite(TargetId),
}

#[derive(Debug)]
pub struct MeshPlan {
    pub id: String,
    pub target: TargetId,
    pub mask: Option<MaskId>,
    pub reads_destination: bool,
    mask_sources: Vec<String>,
}

#[derive(Debug)]
pub struct TargetPlan {
    /// Empty only for the host's main target.
    pub id: String,
    pub parent: TargetId,
    pub active: bool,
    pub mask: Option<MaskId>,
    pub reads_destination: bool,
    pub items: Vec<TargetItem>,
    mask_sources: Vec<String>,
}

#[derive(Debug)]
pub struct MaskPlan {
    /// Unique raw mesh inputs, in first-occurrence order.
    pub sources: Vec<MeshId>,
    /// Canvas-pixel bounds, before the backend's padding/sampling policy.
    pub bounds: Bounds2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds2 {
    pub position: Vec2,
    pub size: Vec2,
}
impl Bounds2 {
    pub fn from_point(point: Vec2) -> Self {
        Self {
            position: point,
            size: Vec2::default(),
        }
    }
    pub fn end(self) -> Vec2 {
        Vec2::new(self.position.x + self.size.x, self.position.y + self.size.y)
    }
    pub fn include(self, point: Vec2) -> Self {
        let end = self.end();
        let position = Vec2::new(self.position.x.min(point.x), self.position.y.min(point.y));
        Self {
            position,
            size: Vec2::new(
                end.x.max(point.x) - position.x,
                end.y.max(point.y) - position.y,
            ),
        }
    }
    pub fn grow(self, amount: f32) -> Self {
        Self {
            position: Vec2::new(self.position.x - amount, self.position.y - amount),
            size: Vec2::new(self.size.x + 2.0 * amount, self.size.y + 2.0 * amount),
        }
    }
}

/// Owns IDs, dependencies, immutable topology and small layout data,
/// but never a published frame or dynamic positions.
/// Indices are scoped to this scene's current successful update. External
/// callers cannot mutate its validated records through this API.
#[derive(Debug, Default)]
pub struct ScenePlan {
    validator: crate::validation::FrameValidator,
    canvas: Canvas,
    meshes: Vec<MeshPlan>,
    targets: Vec<TargetPlan>,
    masks: Vec<MaskPlan>,
    mesh_ids: HashMap<String, MeshId>,
    target_ids: HashMap<String, TargetId>,
    stack: Vec<TargetId>,
    ordered: Vec<usize>,
    bounds: Vec<Option<Bounds2>>,
}

impl ScenePlan {
    pub fn meshes(&self) -> &[MeshPlan] {
        &self.meshes
    }
    pub fn targets(&self) -> &[TargetPlan] {
        &self.targets
    }
    pub fn masks(&self) -> &[MaskPlan] {
        &self.masks
    }
    pub fn canvas(&self) -> Canvas {
        self.canvas
    }
    pub fn mesh_id(&self, id: &str) -> Option<MeshId> {
        self.mesh_ids.get(id).copied()
    }
    pub fn target_id(&self, id: &str) -> Option<TargetId> {
        self.target_ids.get(id).copied()
    }
    pub fn active_target_count(&self) -> usize {
        self.targets.iter().skip(1).filter(|t| t.active).count()
    }

    /// Validate before changing the published logical state. The unversioned
    /// compatibility input is checked on each model submission; immutable topology
    /// is reused only while its allocation identity and vertex count match.
    /// View-only changes do not call this method.
    pub fn update<T: TextureCatalog>(
        &mut self,
        frame: &DrawableFrame,
        textures: &T,
    ) -> Result<(), Status> {
        let status = self.validator.validate(frame);
        if !status.is_ok() {
            return Err(status);
        }
        for mesh in &frame.drawables {
            let texture = textures
                .texture_info(&mesh.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &mesh.texture_asset_id))?;
            if texture.width == 0 || texture.height == 0 {
                return Err(Status::error("INVALID_TEXTURE", &mesh.texture_asset_id));
            }
        }
        let same_structure = self.meshes.len() == frame.drawables.len()
            && self.targets.len() == frame.offscreens.len() + 1
            && self
                .meshes
                .iter()
                .zip(&frame.drawables)
                .all(|(a, b)| a.id == b.id && a.mask_sources == b.masks)
            && self
                .targets
                .iter()
                .skip(1)
                .zip(&frame.offscreens)
                .all(|(a, b)| a.id == b.id && a.mask_sources == b.masks);
        if !same_structure {
            self.rebuild_structure(frame);
        }
        self.canvas = frame.canvas;
        for target in &mut self.targets {
            target.items.clear();
            target.active = false;
        }
        self.targets[0].active = true;
        self.stack.clear();
        self.stack.push(TargetId::MAIN);
        for (record, mesh) in self.meshes.iter_mut().zip(&frame.drawables) {
            record.reads_destination = mesh.raw_blend_mode.is_some();
        }
        for (record, group) in self.targets.iter_mut().skip(1).zip(&frame.offscreens) {
            record.reads_destination = group.blend_mode != 0;
        }
        if frame.render_plan.is_empty() {
            self.ordered.clear();
            self.ordered.extend(0..frame.drawables.len());
            self.ordered
                .sort_unstable_by_key(|&i| (frame.drawables[i].render_order, i));
            for &i in &self.ordered {
                self.meshes[i].target = TargetId::MAIN;
                self.targets[0].items.push(TargetItem::Draw(MeshId(i)));
            }
        } else {
            for command in &frame.render_plan {
                match command {
                    RenderCommand::BeginOffscreen { offscreen_id } => {
                        let id = self.target_ids[offscreen_id];
                        let parent = *self.stack.last().unwrap();
                        self.targets[id.0].parent = parent;
                        self.targets[parent.0].items.push(TargetItem::Composite(id));
                        self.stack.push(id);
                    }
                    RenderCommand::EndOffscreen { .. } => {
                        self.stack.pop();
                    }
                    RenderCommand::DrawMesh { mesh_id } => {
                        let id = self.mesh_ids[mesh_id];
                        let target = *self.stack.last().unwrap();
                        self.meshes[id.0].target = target;
                        self.targets[target.0].items.push(TargetItem::Draw(id));
                        let mesh = &frame.drawables[id.0];
                        if mesh.visible
                            && mesh.opacity > 0.0
                            && !mesh.indices.is_empty()
                            && self.stack.iter().skip(1).all(|id| {
                                let group = &frame.offscreens[id.0 - 1];
                                group.enabled && group.opacity > 0.0
                            })
                        {
                            for id in &self.stack {
                                self.targets[id.0].active = true;
                            }
                        }
                    }
                }
            }
        }
        self.bounds.clear();
        self.bounds.resize(frame.drawables.len(), None);
        // One geometry traversal per required source, even when several masks
        // reference it. These are raw inputs irrespective of color visibility.
        for mask in &mut self.masks {
            let mut union: Option<Bounds2> = None;
            for id in &mask.sources {
                let bounds = *self.bounds[id.0].get_or_insert_with(|| {
                    let mut bounds: Option<Bounds2> = None;
                    for p in &frame.drawables[id.0].positions {
                        let point = Vec2::new(
                            p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                            frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit,
                        );
                        bounds =
                            Some(bounds.map_or(Bounds2::from_point(point), |b| b.include(point)));
                    }
                    bounds.unwrap_or_default()
                });
                // Empty geometry contributes no points (including no origin).
                if !frame.drawables[id.0].positions.is_empty() {
                    union = Some(
                        union.map_or(bounds, |b| b.include(bounds.position).include(bounds.end())),
                    );
                }
            }
            mask.bounds = union.unwrap_or_default();
        }
        Ok(())
    }

    fn rebuild_structure(&mut self, frame: &DrawableFrame) {
        self.mesh_ids.clear();
        self.target_ids.clear();
        self.meshes.clear();
        self.targets.clear();
        self.masks.clear();
        for (i, mesh) in frame.drawables.iter().enumerate() {
            self.mesh_ids.insert(mesh.id.clone(), MeshId(i));
        }
        let mut masks: HashMap<Vec<MeshId>, MaskId> = HashMap::new();
        let mut intern = |sources: &[String]| {
            if sources.is_empty() {
                return None;
            }
            let mut inputs = Vec::with_capacity(sources.len());
            for id in sources {
                let id = self.mesh_ids[id];
                if !inputs.contains(&id) {
                    inputs.push(id);
                }
            }
            Some(*masks.entry(inputs.clone()).or_insert_with(|| {
                let id = MaskId(self.masks.len());
                self.masks.push(MaskPlan {
                    sources: inputs,
                    bounds: Bounds2::default(),
                });
                id
            }))
        };
        for mesh in &frame.drawables {
            self.meshes.push(MeshPlan {
                id: mesh.id.clone(),
                target: TargetId::MAIN,
                mask: intern(&mesh.masks),
                reads_destination: false,
                mask_sources: mesh.masks.clone(),
            });
        }
        self.targets.push(TargetPlan {
            id: String::new(),
            parent: TargetId::MAIN,
            active: true,
            mask: None,
            reads_destination: false,
            items: Vec::new(),
            mask_sources: Vec::new(),
        });
        for (i, group) in frame.offscreens.iter().enumerate() {
            self.target_ids.insert(group.id.clone(), TargetId(i + 1));
            self.targets.push(TargetPlan {
                id: group.id.clone(),
                parent: TargetId::MAIN,
                active: false,
                mask: intern(&group.masks),
                reads_destination: false,
                items: Vec::new(),
                mask_sources: group.masks.clone(),
            });
        }
    }
}

/// Geometry-only screen crop. Resource limits and attachment formats belong to
/// backend lowering, not to this coordinate calculation.
pub fn surface_layout(
    canvas: Vec2,
    transform: Affine2,
    target: Vec2,
) -> Result<(Size2, Affine2), Status> {
    if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    if !target.x.is_finite() || !target.y.is_finite() || target.x <= 0.0 || target.y <= 0.0 {
        return Err(Status::error(
            "INVALID_VIEWPORT",
            "Target extent must be positive and finite.",
        ));
    }
    let mut bounds = Bounds2::from_point(transform.transform_point(Vec2::default()));
    for point in [Vec2::new(canvas.x, 0.0), Vec2::new(0.0, canvas.y), canvas] {
        let point = transform.transform_point(point);
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Transformed canvas overflowed.",
            ));
        }
        bounds = bounds.include(point);
    }
    let min = Vec2::new(bounds.position.x.max(0.0), bounds.position.y.max(0.0));
    let max = Vec2::new(bounds.end().x.min(target.x), bounds.end().y.min(target.y));
    let (origin, end) = if max.x <= min.x || max.y <= min.y {
        (Vec2::default(), Vec2::default())
    } else {
        (
            Vec2::new(min.x.floor(), min.y.floor()),
            Vec2::new(max.x.ceil(), max.y.ceil()),
        )
    };
    let mut root = transform;
    root.origin.x -= origin.x;
    root.origin.y -= origin.y;
    Ok((
        Size2 {
            width: ((end.x - origin.x) as i32).max(2),
            height: ((end.y - origin.y) as i32).max(2),
        },
        root,
    ))
}
