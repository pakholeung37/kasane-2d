use super::*;

/// A backend-owned offscreen color attachment.
pub struct WgpuSurface {
    pub width: u32,
    pub height: u32,
    pub view: wgpu::TextureView,
    pub(super) _texture: wgpu::Texture,
}

/// Reusable offscreen attachments for one wgpu device.
///
/// The pool deliberately owns only render targets created by this backend.
/// External swapchain or embedding targets remain owned by the host and are
/// passed to [`WgpuRenderer::encode`].
pub struct WgpuSurfacePool {
    pub(super) format: wgpu::TextureFormat,
    pub(super) surfaces: HashMap<String, WgpuSurface>,
}

/// A sampled copy of one render target's current contents.
pub struct WgpuDestination {
    pub width: u32,
    pub height: u32,
    pub view: wgpu::TextureView,
    pub(super) _texture: wgpu::Texture,
}

/// Reusable destination snapshots for explicit screen-reading passes.
///
/// The empty string is reserved for the externally supplied main target;
/// non-empty keys identify backend-owned offscreen surfaces.
pub struct WgpuDestinationPool {
    pub(super) format: Option<wgpu::TextureFormat>,
    pub(super) snapshots: HashMap<String, WgpuDestination>,
}

impl Default for WgpuDestinationPool {
    fn default() -> Self {
        Self::new()
    }
}

impl WgpuDestinationPool {
    pub fn new() -> Self {
        Self {
            format: None,
            snapshots: HashMap::new(),
        }
    }

    pub(super) fn get(&self, target: Option<&str>) -> Option<&WgpuDestination> {
        self.snapshots.get(target.unwrap_or_default())
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.format = None;
    }
}

/// A backend-owned alpha mask attachment.
pub struct WgpuMask {
    pub width: u32,
    pub height: u32,
    pub origin: Vec2,
    pub logical_size: Vec2,
    pub scale: f32,
    pub view: wgpu::TextureView,
    pub(super) source_ids: Vec<String>,
    pub(super) _texture: wgpu::Texture,
}

/// Reusable mask attachments shared by drawables with the same mask key.
pub struct WgpuMaskPool {
    pub(super) masks: HashMap<MaskKey, WgpuMask>,
    pub(super) target_keys: HashMap<String, MaskKey>,
}

impl Default for WgpuMaskPool {
    fn default() -> Self {
        Self::new()
    }
}

impl WgpuMaskPool {
    pub fn new() -> Self {
        Self {
            masks: HashMap::new(),
            target_keys: HashMap::new(),
        }
    }

    /// Synchronize mask attachments and their logical bounds for one frame.
    pub fn sync(
        &mut self,
        device: &wgpu::Device,
        frame: &DrawableFrame,
        prepared: &PreparedFrame<'_>,
        viewport: ViewportConfig,
    ) -> Result<(), Status> {
        let mut required = HashMap::new();
        self.target_keys.clear();

        for drawable in &frame.drawables {
            if drawable.masks.is_empty() {
                continue;
            }
            let consumer = prepared
                .mask_consumers
                .get(&drawable.id)
                .map(String::as_str)
                .unwrap_or("");
            let key = MaskKey::new(&drawable.masks, viewport.mask_scale, consumer);
            let layout = mask_layout_with_limit(
                frame,
                &drawable.masks,
                viewport.mask_scale,
                device.limits().max_texture_dimension_2d.min(4096),
            )?;
            required.insert(key.clone(), (drawable.masks.clone(), layout));
            self.target_keys.insert(drawable.id.clone(), key);
        }

        for offscreen in &frame.offscreens {
            if offscreen.masks.is_empty()
                || !prepared.active_offscreens.contains(offscreen.id.as_str())
            {
                continue;
            }
            let consumer = prepared
                .mask_consumers
                .get(&offscreen.id)
                .map(String::as_str)
                .unwrap_or("");
            let requested_scale = viewport.mask_scale.max(1.0);
            let key = MaskKey::new(&offscreen.masks, requested_scale, consumer);
            let layout = mask_layout_with_limit(
                frame,
                &offscreen.masks,
                requested_scale,
                device.limits().max_texture_dimension_2d.min(4096),
            )?;
            required.insert(key.clone(), (offscreen.masks.clone(), layout));
            self.target_keys.insert(offscreen.id.clone(), key);
        }

        self.masks.retain(|key, _| required.contains_key(key));
        for (key, (source_ids, layout)) in required {
            let recreate = self
                .masks
                .get(&key)
                .is_none_or(|mask| mask.width != layout.width || mask.height != layout.height);
            if recreate {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("kasane.wgpu.mask"),
                    size: wgpu::Extent3d {
                        width: layout.width,
                        height: layout.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.masks.insert(
                    key.clone(),
                    WgpuMask {
                        width: layout.width,
                        height: layout.height,
                        origin: layout.origin,
                        logical_size: layout.logical_size,
                        scale: layout.scale,
                        view,
                        source_ids,
                        _texture: texture,
                    },
                );
            } else if let Some(mask) = self.masks.get_mut(&key) {
                mask.origin = layout.origin;
                mask.logical_size = layout.logical_size;
                mask.scale = layout.scale;
                mask.source_ids = source_ids;
            }
        }
        Ok(())
    }

    pub fn get_for_target(&self, target_id: &str) -> Option<&WgpuMask> {
        self.target_keys
            .get(target_id)
            .and_then(|key| self.masks.get(key))
    }

    pub fn iter(&self) -> impl Iterator<Item = &WgpuMask> {
        self.masks.values()
    }

    pub fn len(&self) -> usize {
        self.masks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }

    pub fn clear(&mut self) {
        self.masks.clear();
        self.target_keys.clear();
    }
}

pub(super) use kasane_render::gpu::MaskLayout;

pub(super) fn mask_layout_with_limit(
    frame: &DrawableFrame,
    source_ids: &[String],
    requested_scale: f64,
    max_dimension: u32,
) -> Result<MaskLayout, Status> {
    kasane_render::gpu::mask_layout(frame, source_ids, requested_scale, max_dimension)
}

impl WgpuSurfacePool {
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            surfaces: HashMap::new(),
        }
    }

    /// Synchronize the pool with the active logical offscreens in a prepared frame.
    pub fn sync(
        &mut self,
        device: &wgpu::Device,
        prepared: &PreparedFrame<'_>,
    ) -> Result<(), Status> {
        let (width, height) = surface_extent(prepared.surface_size)?;
        self.surfaces
            .retain(|id, _| prepared.active_offscreens.contains(id.as_str()));

        for id in &prepared.active_offscreens {
            let needs_recreate = self
                .surfaces
                .get(*id)
                .is_none_or(|surface| surface.width != width || surface.height != height);
            if needs_recreate {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("kasane.wgpu.offscreen"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.surfaces.insert(
                    (*id).to_owned(),
                    WgpuSurface {
                        width,
                        height,
                        view,
                        _texture: texture,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&WgpuSurface> {
        self.surfaces.get(id)
    }

    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn clear(&mut self) {
        self.surfaces.clear();
    }
}

pub(super) fn surface_extent(size: Size2) -> Result<(u32, u32), Status> {
    let width = u32::try_from(size.width).map_err(|_| {
        Status::error(
            "INVALID_SURFACE_EXTENT",
            "The prepared offscreen width must be positive.",
        )
    })?;
    let height = u32::try_from(size.height).map_err(|_| {
        Status::error(
            "INVALID_SURFACE_EXTENT",
            "The prepared offscreen height must be positive.",
        )
    })?;
    if width == 0 || height == 0 {
        return Err(Status::error(
            "INVALID_SURFACE_EXTENT",
            "The prepared offscreen extent must be positive.",
        ));
    }
    Ok((width, height))
}
