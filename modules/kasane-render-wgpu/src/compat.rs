use super::*;

impl WgpuFramePlanner {
    pub fn new(target: WgpuTargetConfig) -> Result<Self, Status> {
        if target.width == 0 || target.height == 0 {
            return Err(Status::error(
                "INVALID_TARGET",
                "The wgpu target extent must be positive.",
            ));
        }
        Ok(Self { target })
    }

    pub fn target(&self) -> WgpuTargetConfig {
        self.target
    }

    /// Prepare normal and destination-reading scene compositing.
    pub fn prepare_scene<'a, T: TextureCatalog>(
        &self,
        frame: &'a DrawableFrame,
        textures: &T,
        viewport: ViewportConfig,
    ) -> Result<PreparedFrame<'a>, Status> {
        prepare_frame(frame, textures, viewport)
    }

    /// Prepare the subset supported by the first wgpu render pass.
    pub fn prepare_basic<'a, T: TextureCatalog>(
        &self,
        frame: &'a DrawableFrame,
        textures: &T,
        viewport: ViewportConfig,
    ) -> Result<WgpuBasicFrame<'a>, Status> {
        let prepared = self.prepare_scene(frame, textures, viewport)?;
        if !prepared.destination_reads.is_empty() {
            return Err(unsupported("destination reads"));
        }
        if !prepared.active_offscreens.is_empty() {
            return Err(unsupported("offscreen render targets"));
        }

        let mut draws = Vec::new();
        for pass in &prepared.passes {
            match pass {
                RenderPass::Main => {}
                RenderPass::Draw(item) => {
                    let drawable = frame
                        .drawables
                        .iter()
                        .find(|drawable| drawable.id == item.drawable_id)
                        .ok_or_else(|| Status::error("INVALID_RENDER_PLAN", item.drawable_id))?;
                    if !drawable.visible || drawable.opacity <= 0.0 || drawable.indices.is_empty() {
                        continue;
                    }
                    ensure_basic_blend(drawable)?;
                    draws.push(WgpuDraw {
                        drawable_id: item.drawable_id,
                        texture_id: item.texture_id,
                        index_count: drawable.indices.len() as u32,
                    });
                }
                RenderPass::Offscreen { .. }
                | RenderPass::Mask { .. }
                | RenderPass::Composite { .. }
                | RenderPass::EndOffscreen { .. } => {
                    return Err(unsupported("offscreen or mask pass"));
                }
            }
        }

        Ok(WgpuBasicFrame {
            prepared,
            target: self.target,
            draws,
        })
    }
}

pub(super) fn unsupported(feature: &str) -> Status {
    Status::error(
        "UNSUPPORTED_WGPU_FEATURE",
        format!("The current wgpu backend does not support {feature}."),
    )
}

pub(super) fn ensure_basic_blend(drawable: &Drawable) -> Result<(), Status> {
    if drawable.raw_blend_mode.is_some() || drawable.blend_mode != BlendMode::Normal {
        return Err(unsupported("extended or non-normal blend modes"));
    }
    Ok(())
}
