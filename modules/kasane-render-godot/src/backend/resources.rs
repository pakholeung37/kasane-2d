use super::*;
use kasane_render::{
    surface_layout, Affine2, MaskId, ScenePlan, Size2, TargetId, TextureCatalog, TextureInfo,
};

pub(super) struct GodotTextureCatalog<'a>(pub &'a HashMap<String, Gd<Texture2D>>);
impl TextureCatalog for GodotTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<TextureInfo> {
        let texture = self.0.get(id)?;
        Some(TextureInfo {
            width: u32::try_from(texture.get_width()).unwrap_or(0),
            height: u32::try_from(texture.get_height()).unwrap_or(0),
        })
    }
}

/// Godot requires separate viewport dependency edges for distinct consumers.
/// Resolution is mutable layout, not attachment identity. Below model density,
/// composites require a separate instance from drawable masks in that viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct MaskInstanceKey {
    pub mask: MaskId,
    pub consumer: TargetId,
    pub model_density: bool,
}
#[derive(Clone, Copy)]
pub(super) struct MaskLayout {
    pub size: Vector2i,
    pub scale: f32,
    pub bounds: Vector4,
}
pub(super) struct ViewLayout {
    pub surface_size: Vector2i,
    pub surface_transform: Transform2D,
    pub masks: HashMap<MaskInstanceKey, MaskLayout>,
    pub mesh_masks: Vec<Option<MaskInstanceKey>>,
    pub target_masks: Vec<Option<MaskInstanceKey>>,
}

impl ViewLayout {
    pub fn prepare(
        scene: &ScenePlan,
        transform: Transform2D,
        target: Vector2,
        scale: f64,
    ) -> Result<Self, Status> {
        if !scale.is_finite() || scale <= 0.0 || scale > f32::MAX as f64 {
            return Err(Status::error(
                "INVALID_VIEWPORT",
                "Mask scale must be positive and finite.",
            ));
        }
        let count = scene.active_target_count();
        let (size, root) = if count == 0 {
            (
                Size2 {
                    width: 2,
                    height: 2,
                },
                Affine2::IDENTITY,
            )
        } else {
            surface_layout(
                Vec2::new(scene.canvas().width, scene.canvas().height),
                to_affine(transform),
                Vec2::new(target.x, target.y),
            )?
        };
        if size.width > 4096 || size.height > 4096 {
            return Err(budget_error());
        }
        let mut bytes = i64::from(size.width) * i64::from(size.height) * 8 * count as i64;
        // This is Godot's conservative RGBA8 color + destination reservation.
        // Other backends need not inherit its format, copies or limits.
        let mut masks = HashMap::new();
        let mut instance = |mask: MaskId, consumer: TargetId, composite: bool| {
            let model_density = composite && scale < 1.0;
            let key = MaskInstanceKey {
                mask,
                consumer,
                model_density,
            };
            masks.entry(key).or_insert_with(|| {
                let bounds = scene.masks()[mask.0].bounds.grow(4.0);
                let width = bounds.size.x.ceil().max(1.0);
                let height = bounds.size.y.ceil().max(1.0);
                let requested = if composite { scale.max(1.0) } else { scale } as f32;
                let density = requested.min(4096.0 / width.max(height));
                let width = (width * density).ceil().max(2.0) as i32;
                let height = (height * density).ceil().max(2.0) as i32;
                bytes += i64::from(width) * i64::from(height) * 4;
                MaskLayout {
                    size: Vector2i::new(width, height),
                    scale: density,
                    bounds: Vector4::new(
                        bounds.position.x,
                        bounds.position.y,
                        width as f32 / density,
                        height as f32 / density,
                    ),
                }
            });
            key
        };
        let mesh_masks = scene
            .meshes()
            .iter()
            .map(|m| m.mask.map(|mask| instance(mask, m.target, false)))
            .collect();
        let target_masks = scene
            .targets()
            .iter()
            .map(|t| {
                t.mask
                    .filter(|_| t.active)
                    .map(|mask| instance(mask, t.parent, true))
            })
            .collect();
        if bytes > OFFSCREEN_BUDGET_BYTES {
            return Err(budget_error());
        }
        // Reject overflow before any engine resource is resized/allocated.
        if masks.values().any(|m| {
            !m.scale.is_finite()
                || m.scale <= 0.0
                || !m.bounds.x.is_finite()
                || !m.bounds.y.is_finite()
                || !m.bounds.z.is_finite()
                || !m.bounds.w.is_finite()
        }) {
            return Err(Status::error("INVALID_VIEWPORT", "Mask layout overflowed."));
        }
        Ok(Self {
            surface_size: Vector2i::new(size.width, size.height),
            surface_transform: to_godot_transform(root),
            masks,
            mesh_masks,
            target_masks,
        })
    }
}
fn budget_error() -> Status {
    Status::error(
        "OFFSCREEN_BUDGET_EXCEEDED",
        "Godot color, destination and mask attachments exceed their size or 512 MiB budget.",
    )
}
fn to_affine(transform: Transform2D) -> Affine2 {
    Affine2 {
        a: Vec2::new(transform.a.x, transform.a.y),
        b: Vec2::new(transform.b.x, transform.b.y),
        origin: Vec2::new(transform.origin.x, transform.origin.y),
    }
}
fn to_godot_transform(transform: Affine2) -> Transform2D {
    Transform2D::from_cols(
        Vector2::new(transform.a.x, transform.a.y),
        Vector2::new(transform.b.x, transform.b.y),
        Vector2::new(transform.origin.x, transform.origin.y),
    )
}
