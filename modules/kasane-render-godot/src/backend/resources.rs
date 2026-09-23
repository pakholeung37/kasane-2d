use super::*;

use kasane_render::{
    prepare_frame, Affine2, PreparedFrame, TextureCatalog, TextureInfo, ViewportConfig,
};

pub(super) type SubmissionPlan<'a> = PreparedFrame<'a>;

struct GodotTextureCatalog<'a> {
    textures: &'a HashMap<String, Gd<Texture2D>>,
}

impl TextureCatalog for GodotTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<TextureInfo> {
        let texture = self.textures.get(id)?;
        Some(TextureInfo {
            width: u32::try_from(texture.get_width()).unwrap_or(0),
            height: u32::try_from(texture.get_height()).unwrap_or(0),
        })
    }
}

/// Translate Godot handles into backend-neutral metadata before planning.
///
/// This is intentionally the only resource-specific part of preflight: the
/// shared planner validates the frame and calculates logical attachment
/// requirements without touching Godot objects.
pub(super) fn plan<'a>(
    frame: &'a DrawableFrame,
    textures: &HashMap<String, Gd<Texture2D>>,
    transform: Transform2D,
    target: Vector2,
    scale: f64,
) -> Result<SubmissionPlan<'a>, Status> {
    let texture_catalog = GodotTextureCatalog { textures };
    prepare_frame(
        frame,
        &texture_catalog,
        ViewportConfig {
            transform: to_affine(transform),
            target_extent: Vec2::new(target.x, target.y),
            mask_scale: scale,
        },
    )
}

fn to_affine(transform: Transform2D) -> Affine2 {
    Affine2 {
        a: Vec2::new(transform.a.x, transform.a.y),
        b: Vec2::new(transform.b.x, transform.b.y),
        origin: Vec2::new(transform.origin.x, transform.origin.y),
    }
}

pub(super) fn to_godot_transform(transform: Affine2) -> Transform2D {
    let mut result = Transform2D::IDENTITY;
    result.a = Vector2::new(transform.a.x, transform.a.y);
    result.b = Vector2::new(transform.b.x, transform.b.y);
    result.origin = Vector2::new(transform.origin.x, transform.origin.y);
    result
}
