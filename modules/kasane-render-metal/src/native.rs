use std::collections::HashMap;
use std::ffi::c_void;

use bytemuck::{Pod, Zeroable};
use kasane_core::evaluation::{DrawableFrame, OffscreenFrame};
use kasane_core::types::{BlendMode, Status, Vec2};
use kasane_render::gpu::{
    compose_affine, inverse_affine, mask_layout, quad_indices, quad_vertices, triangle_indices,
    vertices_for, MaskLayout, Vertex,
};
use kasane_render::{
    surface_layout, Affine2, ScenePlan, TargetId, TargetItem, TextureCatalog, ViewportConfig,
};
use metal::{
    Buffer, CommandBufferRef, CommandQueue, Device, DeviceRef, MTLBlendFactor, MTLClearColor,
    MTLIndexType, MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion,
    MTLResourceOptions, MTLSamplerAddressMode, MTLSamplerMinMagFilter, MTLSamplerMipFilter,
    MTLSize, MTLStorageMode, MTLStoreAction, MTLTextureUsage, RenderPassDescriptor,
    RenderPipelineDescriptor, RenderPipelineState, SamplerDescriptor, SamplerState, Texture,
    TextureDescriptor, TextureRef,
};

const MAX_TEXTURE_DIMENSION: u32 = 16_384;

#[derive(Debug, thiserror::Error)]
pub enum MetalInitError {
    #[error("no Metal device is available")]
    DeviceUnavailable,
}

/// A native Metal device and command queue. No wgpu adapter or device is made.
pub struct MetalContext {
    device: Device,
    queue: CommandQueue,
}

impl MetalContext {
    pub fn new() -> Result<Self, MetalInitError> {
        let device = Device::system_default().ok_or(MetalInitError::DeviceUnavailable)?;
        Ok(Self::from_device(device))
    }

    /// Use an existing host `MTLDevice`, such as the device assigned to a
    /// `CAMetalLayer`, while creating a dedicated command queue for Kasane.
    pub fn from_device(device: Device) -> Self {
        let queue = device.new_command_queue();
        Self { device, queue }
    }

    pub fn device(&self) -> &DeviceRef {
        &self.device
    }

    pub fn queue(&self) -> &metal::CommandQueueRef {
        &self.queue
    }

    pub fn name(&self) -> &str {
        self.device.name()
    }

    pub fn max_texture_dimension(&self) -> u32 {
        MAX_TEXTURE_DIMENSION
    }

    pub fn upload_rgba8(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
        mipmaps: &[(u32, u32, Vec<u8>)],
    ) -> Result<Texture, Status> {
        if width == 0
            || height == 0
            || width > MAX_TEXTURE_DIMENSION
            || height > MAX_TEXTURE_DIMENSION
            || rgba.len() != width as usize * height as usize * 4
        {
            return Err(Status::error(
                "INVALID_TEXTURE",
                "Invalid RGBA8 texture dimensions or bytes.",
            ));
        }
        let (mut expected_width, mut expected_height) = (width, height);
        for (w, h, bytes) in mipmaps {
            expected_width = (expected_width / 2).max(1);
            expected_height = (expected_height / 2).max(1);
            if (*w, *h) != (expected_width, expected_height)
                || bytes.len() != *w as usize * *h as usize * 4
            {
                return Err(Status::error("INVALID_TEXTURE", "Invalid RGBA8 mip level."));
            }
        }
        let max_mip_levels = 32 - width.max(height).leading_zeros();
        if mipmaps.len() >= max_mip_levels as usize {
            return Err(Status::error(
                "INVALID_TEXTURE",
                "Too many RGBA8 mip levels.",
            ));
        }
        let descriptor = texture_descriptor(
            width,
            height,
            MTLPixelFormat::RGBA8Unorm,
            MTLTextureUsage::ShaderRead,
        );
        descriptor.set_mipmap_level_count(1 + mipmaps.len() as u64);
        let texture = self.device.new_texture(&descriptor);
        texture.replace_region(
            MTLRegion::new_2d(0, 0, width as u64, height as u64),
            0,
            rgba.as_ptr().cast(),
            (width * 4) as u64,
        );
        for (index, (w, h, bytes)) in mipmaps.iter().enumerate() {
            texture.replace_region(
                MTLRegion::new_2d(0, 0, *w as u64, *h as u64),
                (index + 1) as u64,
                bytes.as_ptr().cast(),
                (*w * 4) as u64,
            );
        }
        Ok(texture)
    }

    pub fn output_texture(&self, target: MetalTargetConfig) -> Result<Texture, Status> {
        validate_target(target)?;
        Ok(create_shared_texture(
            &self.device,
            target.width,
            target.height,
            target.format,
        ))
    }
}

/// Only four byte per pixel color formats are supported by the render path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetalTargetConfig {
    pub width: u32,
    pub height: u32,
    pub format: MTLPixelFormat,
}

impl Default for MetalTargetConfig {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: MTLPixelFormat::RGBA8Unorm,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetalOutputMode {
    #[default]
    Replace,
    Composite,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetalRenderStats {
    pub draw_calls: usize,
    pub masks: usize,
    pub active_surfaces: usize,
    pub destination_copies: usize,
}

pub struct MetalTexture<'a> {
    pub view: &'a TextureRef,
    pub width: u32,
    pub height: u32,
}

pub struct MetalTextureCatalog<'a> {
    textures: HashMap<String, MetalTexture<'a>>,
    revisions: HashMap<String, u64>,
    repeating: std::collections::HashSet<String>,
}

impl<'a> MetalTextureCatalog<'a> {
    pub fn new(textures: HashMap<String, MetalTexture<'a>>) -> Self {
        Self {
            textures,
            revisions: HashMap::new(),
            repeating: Default::default(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&MetalTexture<'a>> {
        self.textures.get(id)
    }

    pub fn set_repeat(&mut self, id: impl Into<String>, repeat: bool) {
        let id = id.into();
        if repeat {
            self.repeating.insert(id);
        } else {
            self.repeating.remove(&id);
        }
    }

    pub fn repeats(&self, id: &str) -> bool {
        self.repeating.contains(id)
    }

    pub fn set_revision(&mut self, id: impl Into<String>, revision: u64) {
        self.revisions.insert(id.into(), revision);
    }

    pub fn revision(&self, id: &str) -> Option<u64> {
        self.revisions.get(id).copied()
    }
}

impl TextureCatalog for MetalTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<kasane_render::TextureInfo> {
        self.get(id).map(|texture| kasane_render::TextureInfo {
            width: texture.width,
            height: texture.height,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    target_size: [f32; 4],
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
    mask_bounds: [f32; 4],
    opacity: [f32; 4],
    flags: [u32; 4],
    modes: [u32; 4],
    view_a: [f32; 4],
    view_b: [f32; 4],
    view_origin: [f32; 4],
    mask_a: [f32; 4],
    mask_b: [f32; 4],
    mask_origin: [f32; 4],
}

impl Uniform {
    fn new(
        size: (u32, u32),
        multiply_color: [f32; 4],
        screen_color: [f32; 4],
        opacity: f32,
    ) -> Self {
        Self {
            target_size: [size.0 as f32, size.1 as f32, 0.0, 0.0],
            multiply_color,
            screen_color,
            mask_bounds: [0.0; 4],
            opacity: [opacity, 0.0, 0.0, 0.0],
            flags: [0; 4],
            modes: [0; 4],
            view_a: [1.0, 0.0, 0.0, 0.0],
            view_b: [0.0, 1.0, 0.0, 0.0],
            view_origin: [0.0; 4],
            mask_a: [1.0, 0.0, 0.0, 0.0],
            mask_b: [0.0, 1.0, 0.0, 0.0],
            mask_origin: [0.0; 4],
        }
    }

    fn view(&mut self, value: Affine2) {
        self.view_a = [value.a.x, value.a.y, 0.0, 0.0];
        self.view_b = [value.b.x, value.b.y, 0.0, 0.0];
        self.view_origin = [value.origin.x, value.origin.y, 0.0, 0.0];
    }

    fn mask_view(&mut self, value: Affine2) {
        self.mask_a = [value.a.x, value.a.y, 0.0, 0.0];
        self.mask_b = [value.b.x, value.b.y, 0.0, 0.0];
        self.mask_origin = [value.origin.x, value.origin.y, 0.0, 0.0];
    }

    fn mask(&mut self, layout: MaskLayout, inverted: bool) {
        self.mask_bounds = [
            layout.origin.x,
            layout.origin.y,
            layout.logical_size.x,
            layout.logical_size.y,
        ];
        self.flags[0] = 1;
        self.flags[1] = u32::from(inverted);
    }

    fn raw_blend(&mut self, mode: u32) {
        self.modes = [mode & 0xff, (mode >> 8) & 0xff, 0, 0];
    }
}

#[derive(Clone, Copy)]
enum Blend {
    Replace,
    Normal,
    Additive,
    Multiplicative,
}

struct Pipelines {
    draw: RenderPipelineState,
    additive: RenderPipelineState,
    multiplicative: RenderPipelineState,
    composite: RenderPipelineState,
    mask: RenderPipelineState,
    extended_draw: RenderPipelineState,
    extended_composite: RenderPipelineState,
    present_replace: RenderPipelineState,
    present_composite: RenderPipelineState,
}

impl Pipelines {
    fn new(device: &DeviceRef, format: MTLPixelFormat) -> Result<Self, Status> {
        let library = device
            .new_library_with_source(include_str!("kasane.metal"), &metal::CompileOptions::new())
            .map_err(|message| Status::error("METAL_SHADER", message))?;
        let build = |fragment: &str, target_format, blend| {
            pipeline(device, &library, fragment, target_format, blend)
        };
        Ok(Self {
            draw: build("fs_draw", format, Blend::Normal)?,
            additive: build("fs_draw", format, Blend::Additive)?,
            multiplicative: build("fs_draw", format, Blend::Multiplicative)?,
            composite: build("fs_composite", format, Blend::Normal)?,
            mask: build("fs_mask", MTLPixelFormat::RGBA8Unorm, Blend::Normal)?,
            extended_draw: build("fs_extended_draw", format, Blend::Replace)?,
            extended_composite: build("fs_extended_composite", format, Blend::Replace)?,
            present_replace: build("fs_present", format, Blend::Replace)?,
            present_composite: build("fs_present", format, Blend::Normal)?,
        })
    }
}

fn pipeline(
    device: &DeviceRef,
    library: &metal::LibraryRef,
    fragment: &str,
    format: MTLPixelFormat,
    blend: Blend,
) -> Result<RenderPipelineState, Status> {
    let vertex = library
        .get_function("vs_main", None)
        .map_err(|message| Status::error("METAL_SHADER", message))?;
    let fragment = library
        .get_function(fragment, None)
        .map_err(|message| Status::error("METAL_SHADER", message))?;
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&vertex));
    descriptor.set_fragment_function(Some(&fragment));
    let color = descriptor.color_attachments().object_at(0).unwrap();
    color.set_pixel_format(format);
    if !matches!(blend, Blend::Replace) {
        color.set_blending_enabled(true);
        let (src_rgb, dst_rgb, src_alpha, dst_alpha) = match blend {
            Blend::Normal => (
                MTLBlendFactor::One,
                MTLBlendFactor::OneMinusSourceAlpha,
                MTLBlendFactor::One,
                MTLBlendFactor::OneMinusSourceAlpha,
            ),
            Blend::Additive => (
                MTLBlendFactor::One,
                MTLBlendFactor::One,
                MTLBlendFactor::Zero,
                MTLBlendFactor::One,
            ),
            Blend::Multiplicative => (
                MTLBlendFactor::Zero,
                MTLBlendFactor::SourceColor,
                MTLBlendFactor::Zero,
                MTLBlendFactor::One,
            ),
            Blend::Replace => unreachable!(),
        };
        color.set_source_rgb_blend_factor(src_rgb);
        color.set_destination_rgb_blend_factor(dst_rgb);
        color.set_source_alpha_blend_factor(src_alpha);
        color.set_destination_alpha_blend_factor(dst_alpha);
    }
    device
        .new_render_pipeline_state(&descriptor)
        .map_err(|message| Status::error("METAL_PIPELINE", message))
}

fn texture_descriptor(
    width: u32,
    height: u32,
    format: MTLPixelFormat,
    usage: MTLTextureUsage,
) -> TextureDescriptor {
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(metal::MTLTextureType::D2);
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_pixel_format(format);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(usage);
    descriptor
}

fn create_texture(device: &DeviceRef, width: u32, height: u32, format: MTLPixelFormat) -> Texture {
    let descriptor = texture_descriptor(
        width,
        height,
        format,
        MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
    );
    descriptor.set_storage_mode(MTLStorageMode::Private);
    device.new_texture(&descriptor)
}

fn create_shared_texture(
    device: &DeviceRef,
    width: u32,
    height: u32,
    format: MTLPixelFormat,
) -> Texture {
    let descriptor = texture_descriptor(
        width,
        height,
        format,
        MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
    );
    device.new_texture(&descriptor)
}

fn validate_target(target: MetalTargetConfig) -> Result<(), Status> {
    if target.width == 0
        || target.height == 0
        || target.width > MAX_TEXTURE_DIMENSION
        || target.height > MAX_TEXTURE_DIMENSION
    {
        return Err(Status::error(
            "INVALID_TARGET",
            "Output extent exceeds Metal limits.",
        ));
    }
    if !matches!(
        target.format,
        MTLPixelFormat::RGBA8Unorm
            | MTLPixelFormat::RGBA8Unorm_sRGB
            | MTLPixelFormat::BGRA8Unorm
            | MTLPixelFormat::BGRA8Unorm_sRGB
    ) {
        return Err(Status::error(
            "INVALID_TARGET_FORMAT",
            "Only RGBA8 and BGRA8 color targets are supported.",
        ));
    }
    Ok(())
}

/// ScenePlan-driven native Metal renderer. It owns the Metal pipelines but
/// source textures and the presentation texture remain host owned.
pub struct MetalRenderer {
    device: Device,
    queue: CommandQueue,
    target: MetalTargetConfig,
    pipelines: Pipelines,
    clamp_sampler: SamplerState,
    repeat_sampler: SamplerState,
    scene: ScenePlan,
    frame: Option<DrawableFrame>,
    viewport: Option<ViewportConfig>,
}

impl MetalRenderer {
    pub fn new(context: &MetalContext, target: MetalTargetConfig) -> Result<Self, Status> {
        validate_target(target)?;
        let clamp_sampler = sampler(&context.device, false);
        let repeat_sampler = sampler(&context.device, true);
        Ok(Self {
            device: context.device.clone(),
            queue: context.queue.clone(),
            target,
            pipelines: Pipelines::new(&context.device, target.format)?,
            clamp_sampler,
            repeat_sampler,
            scene: ScenePlan::default(),
            frame: None,
            viewport: None,
        })
    }

    pub fn target(&self) -> MetalTargetConfig {
        self.target
    }

    pub fn resize(&mut self, target: MetalTargetConfig) -> Result<(), Status> {
        validate_target(target)?;
        if self.target.format != target.format {
            self.pipelines = Pipelines::new(&self.device, target.format)?;
        }
        if self.target != target {
            self.viewport = None;
        }
        self.target = target;
        Ok(())
    }

    pub fn sync_model(
        &mut self,
        frame: &DrawableFrame,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<(), Status> {
        let max_buffer = self.device.max_buffer_length();
        for drawable in &frame.drawables {
            let vertex_bytes = (drawable.positions.len() as u64)
                .saturating_mul(std::mem::size_of::<Vertex>() as u64);
            let index_bytes =
                (drawable.indices.len() as u64).saturating_mul(std::mem::size_of::<u32>() as u64);
            if vertex_bytes > max_buffer
                || index_bytes > max_buffer
                || drawable.indices.len() > u32::MAX as usize
            {
                return Err(Status::error(
                    "GPU_BUFFER_LIMIT",
                    format!("{} exceeds the Metal mesh buffer limit.", drawable.id),
                ));
            }
        }
        let mut candidate = self.scene.clone();
        candidate.update(frame, textures)?;
        self.scene = candidate;
        self.frame = Some(frame.clone());
        Ok(())
    }

    pub fn update_view(&mut self, viewport: ViewportConfig) -> Result<(), Status> {
        if self.frame.is_none() {
            return Err(Status::error(
                "MISSING_MODEL",
                "Submit a model before updating the view.",
            ));
        }
        if viewport.target_extent != Vec2::new(self.target.width as f32, self.target.height as f32)
        {
            return Err(Status::error(
                "INVALID_TARGET",
                "Viewport extent must match the output target.",
            ));
        }
        if !viewport.mask_scale.is_finite() || viewport.mask_scale <= 0.0 {
            return Err(Status::error(
                "INVALID_MASK_SCALE",
                "Mask scale must be positive and finite.",
            ));
        }
        let (size, _) = surface_layout(
            Vec2::new(self.scene.canvas().width, self.scene.canvas().height),
            viewport.transform,
            viewport.target_extent,
        )?;
        if size.width > MAX_TEXTURE_DIMENSION as i32 || size.height > MAX_TEXTURE_DIMENSION as i32 {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Offscreen surface exceeds Metal limits.",
            ));
        }
        self.viewport = Some(viewport);
        Ok(())
    }

    /// Add all Metal render and blit passes to a host-owned command buffer.
    /// Commit that buffer before encoding another frame with this renderer.
    pub fn encode(
        &mut self,
        command: &CommandBufferRef,
        output: &TextureRef,
        output_mode: MetalOutputMode,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<MetalRenderStats, Status> {
        // Encoders and render-pass descriptors are autoreleased by metal-rs.
        // Rust/Python callers need not have an AppKit event-loop pool.
        metal::objc::rc::autoreleasepool(|| {
            self.encode_inner(command, output, output_mode, textures)
        })
    }

    fn encode_inner(
        &mut self,
        command: &CommandBufferRef,
        output: &TextureRef,
        output_mode: MetalOutputMode,
        textures: &MetalTextureCatalog<'_>,
    ) -> Result<MetalRenderStats, Status> {
        let frame = self
            .frame
            .as_ref()
            .ok_or_else(|| Status::error("MISSING_MODEL", "Submit a model before encoding."))?;
        let viewport = self
            .viewport
            .ok_or_else(|| Status::error("MISSING_VIEW", "Update the view before encoding."))?;
        if output.width() != self.target.width as u64
            || output.height() != self.target.height as u64
            || output.pixel_format() != self.target.format
            || output.texture_type() != metal::MTLTextureType::D2
            || output.sample_count() != 1
            || !output.usage().contains(MTLTextureUsage::RenderTarget)
        {
            return Err(Status::error(
                "INVALID_TARGET",
                "Metal output texture extent, format, or usage differs from target.",
            ));
        }
        for drawable in &frame.drawables {
            let texture = textures
                .get(&drawable.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
            if texture.view.width() != texture.width as u64
                || texture.view.height() != texture.height as u64
                || texture.view.texture_type() != metal::MTLTextureType::D2
                || texture.view.sample_count() != 1
                || !texture.view.usage().contains(MTLTextureUsage::ShaderRead)
                || !std::ptr::eq(texture.view.device(), self.device.as_ref())
            {
                return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
            }
        }
        if !std::ptr::eq(output.device(), self.device.as_ref()) {
            return Err(Status::error(
                "DEVICE_MISMATCH",
                "Output texture belongs to another Metal device.",
            ));
        }
        let (size, surface_transform) = surface_layout(
            Vec2::new(self.scene.canvas().width, self.scene.canvas().height),
            viewport.transform,
            viewport.target_extent,
        )?;
        let surface_size = (size.width as u32, size.height as u32);
        self.validate_attachment_budget(frame, viewport, surface_size)?;
        let surface_to_model = inverse_affine(surface_transform)?;
        let main_color = create_texture(
            &self.device,
            self.target.width,
            self.target.height,
            self.target.format,
        );
        let mut state = FrameEncoder {
            renderer: self,
            frame,
            viewport,
            textures,
            command,
            surface_size,
            surface_transform,
            surface_to_model,
            surfaces: HashMap::new(),
            masks: HashMap::new(),
            buffers: Vec::new(),
            stats: MetalRenderStats::default(),
        };
        state.encode_masks()?;
        state.encode_target(TargetId::MAIN, &main_color)?;
        let vertices = quad_vertices(
            (self.target.width, self.target.height),
            Affine2::IDENTITY,
            Affine2::IDENTITY,
        );
        let indices = quad_indices();
        let uniform = Uniform::new(
            (self.target.width, self.target.height),
            [1.0; 4],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        let pipeline = match output_mode {
            MetalOutputMode::Replace => &self.pipelines.present_replace,
            MetalOutputMode::Composite => &self.pipelines.present_composite,
        };
        state.draw(
            output,
            pipeline,
            &main_color,
            None,
            None,
            &vertices,
            &indices,
            &uniform,
            false,
            matches!(output_mode, MetalOutputMode::Replace),
        );
        Ok(state.stats)
    }

    pub fn render(
        &mut self,
        output: &TextureRef,
        frame: &DrawableFrame,
        textures: &MetalTextureCatalog<'_>,
        viewport: ViewportConfig,
    ) -> Result<MetalRenderStats, Status> {
        self.sync_model(frame, textures)?;
        self.update_view(viewport)?;
        metal::objc::rc::autoreleasepool(|| {
            let queue = self.queue.clone();
            let command = queue.new_command_buffer();
            let stats = self.encode(command, output, MetalOutputMode::Replace, textures)?;
            command.commit();
            Ok(stats)
        })
    }

    fn validate_attachment_budget(
        &self,
        frame: &DrawableFrame,
        viewport: ViewportConfig,
        surface_size: (u32, u32),
    ) -> Result<(), Status> {
        let bytes = |size: (u32, u32)| u64::from(size.0) * u64::from(size.1) * 4;
        let mut total = bytes((self.target.width, self.target.height));
        total = total.saturating_add(self.scene.active_target_count() as u64 * bytes(surface_size));
        for drawable in &frame.drawables {
            if !drawable.masks.is_empty() {
                let layout = mask_layout(frame, &drawable.masks, viewport.mask_scale, 4096)?;
                total = total.saturating_add(bytes((layout.width, layout.height)));
            }
        }
        for (index, offscreen) in frame.offscreens.iter().enumerate() {
            if self.scene.targets()[index + 1].active && !offscreen.masks.is_empty() {
                let layout =
                    mask_layout(frame, &offscreen.masks, viewport.mask_scale.max(1.0), 4096)?;
                total = total.saturating_add(bytes((layout.width, layout.height)));
            }
        }
        for (index, target) in self.scene.targets().iter().enumerate() {
            if !target.active {
                continue;
            }
            let target_bytes = bytes(if index == 0 {
                (self.target.width, self.target.height)
            } else {
                surface_size
            });
            for item in &target.items {
                let reads = match item {
                    TargetItem::Draw(id) => {
                        let drawable = &frame.drawables[id.0];
                        drawable.raw_blend_mode.is_some()
                            && drawable.visible
                            && drawable.opacity > 0.0
                            && !drawable.indices.is_empty()
                    }
                    TargetItem::Composite(id) => {
                        self.scene.targets()[id.0].reads_destination
                            && self.scene.targets()[id.0].active
                    }
                };
                if reads {
                    total = total.saturating_add(target_bytes);
                }
            }
        }
        if total > kasane_render::OFFSCREEN_BUDGET_BYTES as u64 {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Metal attachments exceed the configured budget.",
            ));
        }
        Ok(())
    }
}

fn sampler(device: &DeviceRef, repeat: bool) -> SamplerState {
    let descriptor = SamplerDescriptor::new();
    descriptor.set_min_filter(MTLSamplerMinMagFilter::Linear);
    descriptor.set_mag_filter(MTLSamplerMinMagFilter::Linear);
    descriptor.set_mip_filter(MTLSamplerMipFilter::Linear);
    let address = if repeat {
        MTLSamplerAddressMode::Repeat
    } else {
        MTLSamplerAddressMode::ClampToEdge
    };
    descriptor.set_address_mode_s(address);
    descriptor.set_address_mode_t(address);
    device.new_sampler(&descriptor)
}

#[derive(Clone)]
struct MaskAttachment {
    texture: Texture,
    layout: MaskLayout,
}

struct FrameEncoder<'a, 'tex> {
    renderer: &'a MetalRenderer,
    frame: &'a DrawableFrame,
    viewport: ViewportConfig,
    textures: &'a MetalTextureCatalog<'tex>,
    command: &'a CommandBufferRef,
    surface_size: (u32, u32),
    surface_transform: Affine2,
    surface_to_model: Affine2,
    surfaces: HashMap<TargetId, Texture>,
    masks: HashMap<String, MaskAttachment>,
    buffers: Vec<Buffer>,
    stats: MetalRenderStats,
}

impl FrameEncoder<'_, '_> {
    fn encode_masks(&mut self) -> Result<(), Status> {
        for drawable in &self.frame.drawables {
            if !drawable.masks.is_empty() {
                self.encode_mask(&drawable.id, &drawable.masks, self.viewport.mask_scale)?;
            }
        }
        for (index, offscreen) in self.frame.offscreens.iter().enumerate() {
            if !offscreen.masks.is_empty() && self.renderer.scene.targets()[index + 1].active {
                self.encode_mask(
                    &offscreen.id,
                    &offscreen.masks,
                    self.viewport.mask_scale.max(1.0),
                )?;
            }
        }
        Ok(())
    }

    fn encode_mask(&mut self, id: &str, sources: &[String], scale: f64) -> Result<(), Status> {
        let layout = mask_layout(self.frame, sources, scale, MAX_TEXTURE_DIMENSION.min(4096))?;
        let texture = create_texture(
            &self.renderer.device,
            layout.width,
            layout.height,
            MTLPixelFormat::RGBA8Unorm,
        );
        let transform = Affine2 {
            a: Vec2::new(layout.scale, 0.0),
            b: Vec2::new(0.0, layout.scale),
            origin: Vec2::new(
                -layout.origin.x * layout.scale,
                -layout.origin.y * layout.scale,
            ),
        };
        let mut clear = true;
        let mut seen = std::collections::HashSet::new();
        for source_id in sources {
            // Match ScenePlan's unique mask inputs: repeated references must
            // not blend the same alpha into the mask more than once.
            if !seen.insert(source_id) {
                continue;
            }
            let source = self
                .frame
                .drawables
                .iter()
                .find(|mesh| mesh.id == *source_id)
                .ok_or_else(|| Status::error("INVALID_MASK", source_id))?;
            if source.indices.is_empty() {
                continue;
            }
            let texture_view = self
                .textures
                .get(&source.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &source.texture_asset_id))?;
            let vertices = vertices_for(source, self.frame);
            let indices = triangle_indices(&source.indices);
            let mut uniform = Uniform::new(
                (layout.width, layout.height),
                [1.0; 4],
                [0.0, 0.0, 0.0, 1.0],
                1.0,
            );
            uniform.view(transform);
            self.draw(
                &texture,
                &self.renderer.pipelines.mask,
                texture_view.view,
                None,
                None,
                &vertices,
                &indices,
                &uniform,
                self.textures.repeats(&source.texture_asset_id),
                clear,
            );
            clear = false;
        }
        if clear {
            self.clear(&texture);
        }
        self.masks
            .insert(id.to_owned(), MaskAttachment { texture, layout });
        self.stats.masks += 1;
        Ok(())
    }

    fn encode_target(&mut self, id: TargetId, main: &TextureRef) -> Result<(), Status> {
        let target = &self.renderer.scene.targets()[id.0];
        if !target.active {
            return Ok(());
        }
        let items = target.items.clone();
        for item in &items {
            if let TargetItem::Composite(child) = item {
                if self.renderer.scene.targets()[child.0].active {
                    self.encode_target(*child, main)?;
                }
            }
        }
        let target_texture = if id == TargetId::MAIN {
            main.to_owned()
        } else {
            let texture = create_texture(
                &self.renderer.device,
                self.surface_size.0,
                self.surface_size.1,
                self.renderer.target.format,
            );
            self.surfaces.insert(id, texture.clone());
            self.stats.active_surfaces += 1;
            texture
        };
        let size = if id == TargetId::MAIN {
            (self.renderer.target.width, self.renderer.target.height)
        } else {
            self.surface_size
        };
        let mut clear = true;
        for item in items {
            let destination_read = match item {
                TargetItem::Draw(mesh) => {
                    self.renderer.scene.meshes()[mesh.0].reads_destination
                        && self.frame.drawables[mesh.0].visible
                        && self.frame.drawables[mesh.0].opacity > 0.0
                        && !self.frame.drawables[mesh.0].indices.is_empty()
                }
                TargetItem::Composite(child) => {
                    self.renderer.scene.targets()[child.0].reads_destination
                        && self.renderer.scene.targets()[child.0].active
                }
            };
            let snapshot = if destination_read {
                if clear {
                    self.clear(&target_texture);
                    clear = false;
                }
                Some(self.snapshot(&target_texture, size))
            } else {
                None
            };
            let drawn = match item {
                TargetItem::Draw(mesh) => self.encode_mesh(
                    mesh.0,
                    id,
                    &target_texture,
                    size,
                    snapshot.as_deref(),
                    clear,
                )?,
                TargetItem::Composite(child) => self.encode_composite(
                    child,
                    id,
                    &target_texture,
                    size,
                    snapshot.as_deref(),
                    clear,
                )?,
            };
            if drawn {
                clear = false;
            }
        }
        if clear {
            self.clear(&target_texture);
        }
        Ok(())
    }

    fn encode_mesh(
        &mut self,
        index: usize,
        target_id: TargetId,
        target: &TextureRef,
        size: (u32, u32),
        destination: Option<&TextureRef>,
        clear: bool,
    ) -> Result<bool, Status> {
        let drawable = &self.frame.drawables[index];
        if !drawable.visible || drawable.opacity <= 0.0 || drawable.indices.is_empty() {
            return Ok(false);
        }
        let texture = self
            .textures
            .get(&drawable.texture_asset_id)
            .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
        let vertices = vertices_for(drawable, self.frame);
        let indices = triangle_indices(&drawable.indices);
        let mut uniform = Uniform::new(
            size,
            drawable.multiply_color,
            drawable.screen_color,
            drawable.opacity,
        );
        uniform.view(if target_id == TargetId::MAIN {
            self.viewport.transform
        } else {
            self.surface_transform
        });
        let mask = self.masks.get(&drawable.id).cloned();
        if let Some(mask) = &mask {
            uniform.mask(mask.layout, drawable.inverted_mask);
        }
        let pipeline = if let Some(mode) = drawable.raw_blend_mode {
            uniform.raw_blend(mode);
            &self.renderer.pipelines.extended_draw
        } else {
            match drawable.blend_mode {
                BlendMode::Normal => &self.renderer.pipelines.draw,
                BlendMode::Additive => {
                    uniform.flags[2] = 1;
                    &self.renderer.pipelines.additive
                }
                BlendMode::Multiplicative => {
                    uniform.flags[2] = 2;
                    &self.renderer.pipelines.multiplicative
                }
            }
        };
        self.draw(
            target,
            pipeline,
            texture.view,
            mask.as_ref().map(|m| m.texture.as_ref()),
            destination,
            &vertices,
            &indices,
            &uniform,
            self.textures.repeats(&drawable.texture_asset_id),
            clear,
        );
        Ok(true)
    }

    fn encode_composite(
        &mut self,
        child: TargetId,
        parent: TargetId,
        target: &TextureRef,
        size: (u32, u32),
        destination: Option<&TextureRef>,
        clear: bool,
    ) -> Result<bool, Status> {
        let group = &self.renderer.scene.targets()[child.0];
        if !group.active {
            return Ok(false);
        }
        let source = self
            .surfaces
            .get(&child)
            .ok_or_else(|| Status::error("MISSING_SURFACE", &group.id))?
            .clone();
        let offscreen: &OffscreenFrame = &self.frame.offscreens[child.0 - 1];
        let vertices = quad_vertices((1, 1), Affine2::IDENTITY, Affine2::IDENTITY);
        let indices = quad_indices();
        let mut uniform = Uniform::new(
            size,
            offscreen.multiply_color,
            offscreen.screen_color,
            offscreen.opacity,
        );
        let source_scale = Affine2 {
            a: Vec2::new(self.surface_size.0 as f32, 0.0),
            b: Vec2::new(0.0, self.surface_size.1 as f32),
            origin: Vec2::default(),
        };
        let composite_transform = if parent == TargetId::MAIN {
            compose_affine(self.viewport.transform, self.surface_to_model)
        } else {
            Affine2::IDENTITY
        };
        uniform.view(compose_affine(composite_transform, source_scale));
        uniform.mask_view(compose_affine(self.surface_to_model, source_scale));
        let mask = self.masks.get(&offscreen.id).cloned();
        if let Some(mask) = &mask {
            uniform.mask(mask.layout, offscreen.flags & 8 != 0);
        }
        let pipeline = if offscreen.blend_mode != 0 {
            uniform.raw_blend(offscreen.blend_mode);
            &self.renderer.pipelines.extended_composite
        } else {
            &self.renderer.pipelines.composite
        };
        self.draw(
            target,
            pipeline,
            &source,
            mask.as_ref().map(|m| m.texture.as_ref()),
            destination,
            &vertices,
            &indices,
            &uniform,
            false,
            clear,
        );
        Ok(true)
    }

    fn snapshot(&mut self, source: &TextureRef, size: (u32, u32)) -> Texture {
        let destination = create_texture(
            &self.renderer.device,
            size.0,
            size.1,
            self.renderer.target.format,
        );
        let blit = self.command.new_blit_command_encoder();
        blit.copy_from_texture(
            source,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLSize::new(size.0 as u64, size.1 as u64, 1),
            &destination,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
        );
        blit.end_encoding();
        self.stats.destination_copies += 1;
        destination
    }

    fn clear(&self, target: &TextureRef) {
        let descriptor = render_pass(target, true);
        let encoder = self.command.new_render_command_encoder(descriptor);
        encoder.end_encoding();
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        target: &TextureRef,
        pipeline: &RenderPipelineState,
        source: &TextureRef,
        mask: Option<&TextureRef>,
        destination: Option<&TextureRef>,
        vertices: &[Vertex],
        indices: &[u32],
        uniform: &Uniform,
        repeat: bool,
        clear: bool,
    ) {
        let vertex = self.renderer.device.new_buffer_with_data(
            vertices.as_ptr().cast::<c_void>(),
            std::mem::size_of_val(vertices) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let index = self.renderer.device.new_buffer_with_data(
            indices.as_ptr().cast::<c_void>(),
            std::mem::size_of_val(indices) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let descriptor = render_pass(target, clear);
        let encoder = self.command.new_render_command_encoder(descriptor);
        encoder.set_render_pipeline_state(pipeline);
        encoder.set_vertex_buffer(0, Some(&vertex), 0);
        encoder.set_vertex_bytes(
            1,
            std::mem::size_of::<Uniform>() as u64,
            (uniform as *const Uniform).cast(),
        );
        encoder.set_fragment_bytes(
            1,
            std::mem::size_of::<Uniform>() as u64,
            (uniform as *const Uniform).cast(),
        );
        encoder.set_fragment_texture(0, Some(source));
        if let Some(mask) = mask {
            encoder.set_fragment_texture(1, Some(mask));
        }
        if let Some(destination) = destination {
            encoder.set_fragment_texture(2, Some(destination));
        }
        let sampler = if repeat {
            &self.renderer.repeat_sampler
        } else {
            &self.renderer.clamp_sampler
        };
        encoder.set_fragment_sampler_state(0, Some(sampler));
        encoder.set_fragment_sampler_state(1, Some(&self.renderer.clamp_sampler));
        encoder.set_fragment_sampler_state(2, Some(&self.renderer.clamp_sampler));
        encoder.draw_indexed_primitives(
            MTLPrimitiveType::Triangle,
            indices.len() as u64,
            MTLIndexType::UInt32,
            &index,
            0,
        );
        encoder.end_encoding();
        self.buffers.push(vertex);
        self.buffers.push(index);
        self.stats.draw_calls += 1;
    }
}

fn render_pass(target: &TextureRef, clear: bool) -> &metal::RenderPassDescriptorRef {
    let descriptor = RenderPassDescriptor::new();
    let color = descriptor.color_attachments().object_at(0).unwrap();
    color.set_texture(Some(target));
    color.set_load_action(if clear {
        MTLLoadAction::Clear
    } else {
        MTLLoadAction::Load
    });
    color.set_store_action(MTLStoreAction::Store);
    color.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 0.0));
    descriptor
}

/// Read an RGBA8 shared Metal texture after its command buffer completes.
pub fn read_rgba8(texture: &TextureRef) -> Result<Vec<u8>, Status> {
    if texture.pixel_format() != MTLPixelFormat::RGBA8Unorm
        || texture.storage_mode() != MTLStorageMode::Shared
        || texture.texture_type() != metal::MTLTextureType::D2
        || texture.sample_count() != 1
    {
        return Err(Status::error(
            "INVALID_READBACK",
            "Readback requires a shared, single-sample 2D RGBA8Unorm texture.",
        ));
    }
    let width = texture.width() as usize;
    let height = texture.height() as usize;
    let mut rgba = vec![0u8; width * height * 4];
    texture.get_bytes(
        rgba.as_mut_ptr().cast(),
        (width * 4) as u64,
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
    );
    Ok(rgba)
}
