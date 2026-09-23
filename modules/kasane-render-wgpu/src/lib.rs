//! wgpu rendering adapter for the backend-neutral `kasane-render` plan.
//!
//! The current execution path supports normal drawable meshes, alpha masks,
//! nested offscreen surfaces, destination snapshots for extended blend modes,
//! and fixed-function additive/multiplicative drawable blends.

use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use kasane_core::evaluation::{Drawable, DrawableFrame};
use kasane_core::types::{BlendMode, Status, Vec2};
use kasane_render::{
    prepare_frame, Affine2, DrawItem, MaskKey, PreparedFrame, RenderPass, Size2, TextureCatalog,
    ViewportConfig,
};
use wgpu::util::DeviceExt;

/// The external color target supplied by the eventual wgpu host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuTargetConfig {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

impl Default for WgpuTargetConfig {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        }
    }
}

/// One normal draw that a basic wgpu pipeline can submit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuDraw<'a> {
    pub drawable_id: &'a str,
    pub texture_id: &'a str,
    pub index_count: u32,
}

/// Device-free result of preparing a basic wgpu frame.
#[derive(Clone, Debug, PartialEq)]
pub struct WgpuBasicFrame<'a> {
    pub prepared: PreparedFrame<'a>,
    pub target: WgpuTargetConfig,
    pub draws: Vec<WgpuDraw<'a>>,
}

/// Result of executing a normal-draw scene with optional offscreen surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct WgpuSceneFrame<'a> {
    pub prepared: PreparedFrame<'a>,
    pub target: WgpuTargetConfig,
    pub active_surface_count: usize,
}

/// Host-provided objects used for one scene submission.
pub struct WgpuSceneTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    /// The texture behind `view`, when the host can expose it.
    ///
    /// Destination-reading passes need this handle to copy the current main
    /// target into a sampled snapshot. It is optional so normal scenes can
    /// still render into an embedding API that only exposes a view.
    pub texture: Option<&'a wgpu::Texture>,
    pub surface_pool: &'a mut WgpuSurfacePool,
    pub mask_pool: &'a mut WgpuMaskPool,
    pub destination_pool: &'a mut WgpuDestinationPool,
}

/// First-stage wgpu backend entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuFramePlanner {
    target: WgpuTargetConfig,
}

/// A host-owned texture view made available to the wgpu backend.
///
/// The host remains responsible for image decoding and texture lifetime. The
/// backend only needs the view for binding and the dimensions for the shared
/// preflight contract.
pub struct WgpuTexture<'a> {
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Texture catalog used by [`WgpuBasicRenderer`]. The catalog owns the map but
/// borrows the host-created texture views.
pub struct WgpuTextureCatalog<'a> {
    textures: HashMap<String, WgpuTexture<'a>>,
}

impl<'a> WgpuTextureCatalog<'a> {
    pub fn new(textures: HashMap<String, WgpuTexture<'a>>) -> Self {
        Self { textures }
    }

    pub fn get(&self, id: &str) -> Option<&WgpuTexture<'a>> {
        self.textures.get(id)
    }
}

impl TextureCatalog for WgpuTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<kasane_render::TextureInfo> {
        self.get(id).map(|texture| kasane_render::TextureInfo {
            width: texture.width,
            height: texture.height,
        })
    }
}

/// A backend-owned offscreen color attachment.
pub struct WgpuSurface {
    pub width: u32,
    pub height: u32,
    pub view: wgpu::TextureView,
    _texture: wgpu::Texture,
}

/// Reusable offscreen attachments for one wgpu device.
///
/// The pool deliberately owns only render targets created by this backend.
/// External swapchain or embedding targets remain owned by the host and are
/// passed to [`WgpuBasicRenderer::render`] or
/// [`WgpuBasicRenderer::render_scene`].
pub struct WgpuSurfacePool {
    format: wgpu::TextureFormat,
    surfaces: HashMap<String, WgpuSurface>,
}

/// A sampled copy of one render target's current contents.
pub struct WgpuDestination {
    pub width: u32,
    pub height: u32,
    pub view: wgpu::TextureView,
    _texture: wgpu::Texture,
}

/// Reusable destination snapshots for explicit screen-reading passes.
///
/// The empty string is reserved for the externally supplied main target;
/// non-empty keys identify backend-owned offscreen surfaces.
pub struct WgpuDestinationPool {
    format: Option<wgpu::TextureFormat>,
    snapshots: HashMap<String, WgpuDestination>,
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

    /// Synchronize snapshot attachments with the targets that read destination
    /// color in this frame.
    pub fn sync(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        frame: &DrawableFrame,
        prepared: &PreparedFrame<'_>,
        main_size: (u32, u32),
    ) -> Result<(), Status> {
        let (surface_width, surface_height) = surface_extent(prepared.surface_size)?;
        let required = destination_targets(frame, prepared);

        if self.format != Some(format) {
            self.snapshots.clear();
            self.format = Some(format);
        }
        self.snapshots.retain(|id, _| required.contains(id));

        for id in required {
            let (width, height) = if id.is_empty() {
                main_size
            } else {
                (surface_width, surface_height)
            };
            if width == 0 || height == 0 {
                return Err(Status::error(
                    "INVALID_TARGET",
                    "Destination snapshot extents must be positive.",
                ));
            }
            let needs_recreate = self
                .snapshots
                .get(&id)
                .is_none_or(|snapshot| snapshot.width != width || snapshot.height != height);
            if needs_recreate {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("kasane.wgpu.destination"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.snapshots.insert(
                    id,
                    WgpuDestination {
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

    fn get(&self, target: Option<&str>) -> Option<&WgpuDestination> {
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
    source_ids: Vec<String>,
    _texture: wgpu::Texture,
}

/// Reusable mask attachments shared by drawables with the same mask key.
pub struct WgpuMaskPool {
    masks: HashMap<MaskKey, WgpuMask>,
    target_keys: HashMap<String, MaskKey>,
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
            let layout = mask_layout(frame, &drawable.masks, viewport.mask_scale)?;
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
            let layout = mask_layout(frame, &offscreen.masks, requested_scale)?;
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

#[derive(Clone, Copy)]
struct MaskLayout {
    width: u32,
    height: u32,
    origin: Vec2,
    logical_size: Vec2,
    scale: f32,
}

#[derive(Clone, Copy, Default)]
struct MaskRect {
    position: Vec2,
    size: Vec2,
}

impl MaskRect {
    fn expand(self, point: Vec2) -> Self {
        let end = Vec2::new(self.position.x + self.size.x, self.position.y + self.size.y);
        let min = Vec2::new(self.position.x.min(point.x), self.position.y.min(point.y));
        let max = Vec2::new(end.x.max(point.x), end.y.max(point.y));
        Self {
            position: min,
            size: Vec2::new(max.x - min.x, max.y - min.y),
        }
    }

    fn grow(self, amount: f32) -> Self {
        Self {
            position: Vec2::new(self.position.x - amount, self.position.y - amount),
            size: Vec2::new(self.size.x + amount * 2.0, self.size.y + amount * 2.0),
        }
    }
}

fn mask_layout(
    frame: &DrawableFrame,
    source_ids: &[String],
    requested_scale: f64,
) -> Result<MaskLayout, Status> {
    if !requested_scale.is_finite() || requested_scale <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask scale must be finite and positive.",
        ));
    }
    let mut bounds = None;
    for source_id in source_ids {
        let drawable = frame
            .drawables
            .iter()
            .find(|drawable| drawable.id == *source_id)
            .ok_or_else(|| Status::error("INVALID_MASK", source_id))?;
        for position in &drawable.positions {
            let point = Vec2::new(
                position.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                frame.canvas.origin.y - position.y * frame.canvas.pixels_per_unit,
            );
            bounds = Some(
                bounds
                    .map(|current: MaskRect| current.expand(point))
                    .unwrap_or(MaskRect {
                        position: point,
                        size: Vec2::new(0.0, 0.0),
                    }),
            );
        }
    }
    let bounds = bounds.unwrap_or_default().grow(4.0);
    let size_x = bounds.size.x.ceil().max(1.0);
    let size_y = bounds.size.y.ceil().max(1.0);
    let max_dim = size_x.max(size_y);
    let scale = (requested_scale as f32).min(4096.0 / max_dim);
    let width = (size_x * scale).ceil().max(1.0);
    let height = (size_y * scale).ceil().max(1.0);
    Ok(MaskLayout {
        width: width as u32,
        height: height as u32,
        origin: bounds.position,
        logical_size: Vec2::new(width / scale, height / scale),
        scale,
    })
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

fn surface_extent(size: Size2) -> Result<(u32, u32), Status> {
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

fn destination_targets(frame: &DrawableFrame, prepared: &PreparedFrame<'_>) -> HashSet<String> {
    let mut targets = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();

    for pass in &prepared.passes {
        match *pass {
            RenderPass::Main => {}
            RenderPass::Offscreen { id, .. } => stack.push(id),
            RenderPass::Composite { id, parent } => {
                if prepared.destination_reads.contains(id)
                    && prepared.active_offscreens.contains(id)
                {
                    targets.insert(parent.unwrap_or_default().to_owned());
                }
            }
            RenderPass::Draw(item) => {
                if !prepared.destination_reads.contains(item.drawable_id)
                    || stack
                        .iter()
                        .any(|id| !prepared.active_offscreens.contains(id))
                {
                    continue;
                }
                let is_visible_destination_read = frame
                    .drawables
                    .iter()
                    .find(|drawable| drawable.id == item.drawable_id)
                    .is_some_and(|drawable| {
                        drawable.visible && drawable.opacity > 0.0 && !drawable.indices.is_empty()
                    });
                if is_visible_destination_read {
                    targets.insert(stack.last().copied().unwrap_or_default().to_owned());
                }
            }
            RenderPass::EndOffscreen { id } => {
                debug_assert_eq!(stack.pop(), Some(id));
            }
            RenderPass::Mask { .. } => {}
        }
    }

    targets
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    mask_point: [f32; 2],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
];

const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &VERTEX_ATTRIBUTES,
};

fn premultiplied_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn additive_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn multiplicative_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::Src,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniform {
    target_size: [f32; 2],
    padding: [f32; 2],
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
    opacity: f32,
    padding_end: [f32; 3],
    mask_bounds: [f32; 4],
    mask_flags: [u32; 4],
    blend_modes: [u32; 4],
}

#[derive(Clone, Copy)]
struct MaskBinding<'a> {
    view: &'a wgpu::TextureView,
}

#[derive(Clone, Copy)]
struct DestinationBinding<'a> {
    view: &'a wgpu::TextureView,
}

struct ResourceInput<'a> {
    device: &'a wgpu::Device,
    texture_view: &'a wgpu::TextureView,
    vertices: &'a [Vertex],
    indices: &'a [u32],
    uniform: DrawUniform,
    mask: Option<MaskBinding<'a>>,
    destination: Option<DestinationBinding<'a>>,
}

struct DrawResources {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    _uniform_buffer: wgpu::Buffer,
    texture_bind_group: wgpu::BindGroup,
    uniform_bind_group: wgpu::BindGroup,
    mask_bind_group: Option<wgpu::BindGroup>,
    destination_bind_group: Option<wgpu::BindGroup>,
    index_count: u32,
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[VERTEX_LAYOUT],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// WGPU renderer with a flat normal-draw entry point and a full scene path.
///
/// Device and queue ownership stay with the host application. This keeps the
/// backend usable with a window surface, an offscreen target, or an embedding
/// runtime that already manages adapter selection and device loss.
pub struct WgpuBasicRenderer {
    planner: WgpuFramePlanner,
    pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    multiplicative_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    masked_pipeline: wgpu::RenderPipeline,
    masked_additive_pipeline: wgpu::RenderPipeline,
    masked_multiplicative_pipeline: wgpu::RenderPipeline,
    masked_composite_pipeline: wgpu::RenderPipeline,
    extended_pipeline: wgpu::RenderPipeline,
    extended_composite_pipeline: wgpu::RenderPipeline,
    masked_extended_pipeline: wgpu::RenderPipeline,
    masked_extended_composite_pipeline: wgpu::RenderPipeline,
    mask_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    uniform_layout: wgpu::BindGroupLayout,
    mask_layout: wgpu::BindGroupLayout,
    destination_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl WgpuBasicRenderer {
    pub fn new(device: &wgpu::Device, target: WgpuTargetConfig) -> Result<Self, Status> {
        let planner = WgpuFramePlanner::new(target)?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.basic.shader"),
            source: wgpu::ShaderSource::Wgsl(BASIC_SHADER.into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.composite.shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let masked_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.masked.shader"),
            source: wgpu::ShaderSource::Wgsl(MASKED_SHADER.into()),
        });
        let masked_composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.masked-composite.shader"),
            source: wgpu::ShaderSource::Wgsl(MASKED_COMPOSITE_SHADER.into()),
        });
        let mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.mask-source.shader"),
            source: wgpu::ShaderSource::Wgsl(MASK_SHADER.into()),
        });
        let extended_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.extended.shader"),
            source: wgpu::ShaderSource::Wgsl(extended_shader_source(false, false).into()),
        });
        let extended_composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.extended-composite.shader"),
            source: wgpu::ShaderSource::Wgsl(extended_shader_source(true, false).into()),
        });
        let masked_extended_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasane.wgpu.masked-extended.shader"),
            source: wgpu::ShaderSource::Wgsl(extended_shader_source(false, true).into()),
        });
        let masked_extended_composite_shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("kasane.wgpu.masked-extended-composite.shader"),
                source: wgpu::ShaderSource::Wgsl(extended_shader_source(true, true).into()),
            });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kasane.wgpu.basic.texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kasane.wgpu.basic.uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kasane.wgpu.mask.texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let destination_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("kasane.wgpu.destination.texture-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kasane.wgpu.basic.pipeline-layout"),
            bind_group_layouts: &[Some(&texture_layout), Some(&uniform_layout)],
            immediate_size: 0,
        });
        let masked_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("kasane.wgpu.masked.pipeline-layout"),
                bind_group_layouts: &[
                    Some(&texture_layout),
                    Some(&uniform_layout),
                    Some(&mask_layout),
                ],
                immediate_size: 0,
            });
        let extended_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("kasane.wgpu.extended.pipeline-layout"),
                bind_group_layouts: &[
                    Some(&texture_layout),
                    Some(&uniform_layout),
                    Some(&destination_layout),
                ],
                immediate_size: 0,
            });
        let masked_extended_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("kasane.wgpu.masked-extended.pipeline-layout"),
                bind_group_layouts: &[
                    Some(&texture_layout),
                    Some(&uniform_layout),
                    Some(&destination_layout),
                    Some(&mask_layout),
                ],
                immediate_size: 0,
            });
        let pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target.format,
            Some(premultiplied_blend()),
            "kasane.wgpu.basic.pipeline",
        );
        let additive_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target.format,
            Some(additive_blend()),
            "kasane.wgpu.additive.pipeline",
        );
        let multiplicative_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target.format,
            Some(multiplicative_blend()),
            "kasane.wgpu.multiplicative.pipeline",
        );
        let composite_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &composite_shader,
            target.format,
            Some(premultiplied_blend()),
            "kasane.wgpu.composite.pipeline",
        );
        let masked_pipeline = create_pipeline(
            device,
            &masked_pipeline_layout,
            &masked_shader,
            target.format,
            Some(premultiplied_blend()),
            "kasane.wgpu.masked.pipeline",
        );
        let masked_additive_pipeline = create_pipeline(
            device,
            &masked_pipeline_layout,
            &masked_shader,
            target.format,
            Some(additive_blend()),
            "kasane.wgpu.masked-additive.pipeline",
        );
        let masked_multiplicative_pipeline = create_pipeline(
            device,
            &masked_pipeline_layout,
            &masked_shader,
            target.format,
            Some(multiplicative_blend()),
            "kasane.wgpu.masked-multiplicative.pipeline",
        );
        let masked_composite_pipeline = create_pipeline(
            device,
            &masked_pipeline_layout,
            &masked_composite_shader,
            target.format,
            Some(premultiplied_blend()),
            "kasane.wgpu.masked-composite.pipeline",
        );
        let extended_pipeline = create_pipeline(
            device,
            &extended_pipeline_layout,
            &extended_shader,
            target.format,
            None,
            "kasane.wgpu.extended.pipeline",
        );
        let extended_composite_pipeline = create_pipeline(
            device,
            &extended_pipeline_layout,
            &extended_composite_shader,
            target.format,
            None,
            "kasane.wgpu.extended-composite.pipeline",
        );
        let masked_extended_pipeline = create_pipeline(
            device,
            &masked_extended_pipeline_layout,
            &masked_extended_shader,
            target.format,
            None,
            "kasane.wgpu.masked-extended.pipeline",
        );
        let masked_extended_composite_pipeline = create_pipeline(
            device,
            &masked_extended_pipeline_layout,
            &masked_extended_composite_shader,
            target.format,
            None,
            "kasane.wgpu.masked-extended-composite.pipeline",
        );
        let mask_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &mask_shader,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(premultiplied_blend()),
            "kasane.wgpu.mask-source.pipeline",
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kasane.wgpu.basic.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            planner,
            pipeline,
            additive_pipeline,
            multiplicative_pipeline,
            composite_pipeline,
            masked_pipeline,
            masked_additive_pipeline,
            masked_multiplicative_pipeline,
            masked_composite_pipeline,
            extended_pipeline,
            extended_composite_pipeline,
            masked_extended_pipeline,
            masked_extended_composite_pipeline,
            mask_pipeline,
            texture_layout,
            uniform_layout,
            mask_layout,
            destination_layout,
            sampler,
        })
    }

    pub fn target(&self) -> WgpuTargetConfig {
        self.planner.target()
    }

    fn scene_pipelines(&self) -> ScenePipelines<'_> {
        ScenePipelines {
            draw: &self.pipeline,
            additive_draw: &self.additive_pipeline,
            multiplicative_draw: &self.multiplicative_pipeline,
            composite: &self.composite_pipeline,
            masked_draw: &self.masked_pipeline,
            masked_additive_draw: &self.masked_additive_pipeline,
            masked_multiplicative_draw: &self.masked_multiplicative_pipeline,
            masked_composite: &self.masked_composite_pipeline,
            extended_draw: &self.extended_pipeline,
            extended_composite: &self.extended_composite_pipeline,
            masked_extended_draw: &self.masked_extended_pipeline,
            masked_extended_composite: &self.masked_extended_composite_pipeline,
            mask_source: &self.mask_pipeline,
        }
    }

    fn create_resources(&self, input: ResourceInput<'_>) -> DrawResources {
        let ResourceInput {
            device,
            texture_view,
            vertices,
            indices,
            uniform,
            mask,
            destination,
        } = input;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kasane.wgpu.scene.vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kasane.wgpu.scene.indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kasane.wgpu.scene.uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kasane.wgpu.scene.texture-bind-group"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kasane.wgpu.scene.uniform-bind-group"),
            layout: &self.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let mask_bind_group = mask.map(|mask| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kasane.wgpu.scene.mask-bind-group"),
                layout: &self.mask_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(mask.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
        let destination_bind_group = destination.map(|destination| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kasane.wgpu.scene.destination-bind-group"),
                layout: &self.destination_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(destination.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
        DrawResources {
            vertex_buffer,
            index_buffer,
            _uniform_buffer: uniform_buffer,
            texture_bind_group,
            uniform_bind_group,
            mask_bind_group,
            destination_bind_group,
            index_count: indices.len() as u32,
        }
    }

    /// Encode and submit one flat frame into `target_view`.
    pub fn render<'frame, 'texture>(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        frame: &'frame DrawableFrame,
        textures: &WgpuTextureCatalog<'texture>,
        viewport: ViewportConfig,
    ) -> Result<WgpuBasicFrame<'frame>, Status> {
        if !viewport.transform.is_finite() || viewport.transform.determinant().abs() < 1.0e-12 {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Preview transform must be finite and invertible.",
            ));
        }
        let prepared = self.planner.prepare_basic(frame, textures, viewport)?;
        let mut resources = Vec::with_capacity(prepared.draws.len());
        for draw in &prepared.draws {
            if draw.index_count == 0 {
                continue;
            }
            let drawable = frame
                .drawables
                .iter()
                .find(|drawable| drawable.id == draw.drawable_id)
                .ok_or_else(|| Status::error("INVALID_RENDER_PLAN", draw.drawable_id))?;
            let texture = textures
                .get(draw.texture_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", draw.texture_id))?;
            let vertices = vertices_for(drawable, frame, viewport);
            let indices = triangle_indices(&drawable.indices);
            let uniform = draw_uniform(
                (self.planner.target.width, self.planner.target.height),
                drawable.multiply_color,
                drawable.screen_color,
                drawable.opacity,
            );
            resources.push(self.create_resources(ResourceInput {
                device,
                texture_view: texture.view,
                vertices: &vertices,
                indices: &indices,
                uniform,
                mask: None,
                destination: None,
            }));
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("kasane.wgpu.basic.encoder"),
        });
        {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kasane.wgpu.basic.pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for resource in &resources {
                pass.set_bind_group(0, &resource.texture_bind_group, &[]);
                pass.set_bind_group(1, &resource.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, resource.vertex_buffer.slice(..));
                pass.set_index_buffer(resource.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..resource.index_count, 0, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
        drop(resources);
        Ok(prepared)
    }

    /// Encode a scene, including fixed blend modes, masks, offscreen surfaces,
    /// and destination snapshots for extended blend modes.
    ///
    /// If the main target contains a destination-reading item, the host must
    /// provide `WgpuSceneTarget::texture` and create that texture with
    /// `TextureUsages::COPY_SRC`. The host supplies all attachment pools so it
    /// can decide when a device-loss or resize should discard backend-owned
    /// resources.
    pub fn render_scene<'frame, 'texture>(
        &self,
        target: WgpuSceneTarget<'_>,
        frame: &'frame DrawableFrame,
        textures: &WgpuTextureCatalog<'texture>,
        viewport: ViewportConfig,
    ) -> Result<WgpuSceneFrame<'frame>, Status> {
        let WgpuSceneTarget {
            device,
            queue,
            view: target_view,
            texture: target_texture,
            surface_pool,
            mask_pool,
            destination_pool,
        } = target;
        if surface_pool.format() != self.planner.target.format {
            return Err(Status::error(
                "INVALID_TARGET",
                "The wgpu surface pool format must match the render target format.",
            ));
        }
        if let Some(texture) = target_texture {
            if texture.width() != self.planner.target.width
                || texture.height() != self.planner.target.height
                || texture.format() != self.planner.target.format
            {
                return Err(Status::error(
                    "INVALID_TARGET",
                    "The supplied main target texture does not match the wgpu target config.",
                ));
            }
        }
        if !viewport.transform.is_finite() || viewport.transform.determinant().abs() < 1.0e-12 {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Preview transform must be finite and invertible.",
            ));
        }
        let prepared = self.planner.prepare_scene(frame, textures, viewport)?;
        let scene = build_scene(&prepared)?;
        surface_pool.sync(device, &prepared)?;
        mask_pool.sync(device, frame, &prepared, viewport)?;
        destination_pool.sync(
            device,
            self.planner.target.format,
            frame,
            &prepared,
            (self.planner.target.width, self.planner.target.height),
        )?;
        let surface_size = surface_extent(prepared.surface_size)?;
        let surface_to_model = inverse_affine(prepared.surface_transform)?;
        let surface_to_target = compose_affine(viewport.transform, surface_to_model);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("kasane.wgpu.scene.encoder"),
        });
        let mut resources = Vec::new();
        let mut rendered_surfaces = HashSet::new();
        {
            let mut context = SceneEncoder {
                renderer: self,
                pipelines: self.scene_pipelines(),
                device,
                encoder: &mut encoder,
                resources: &mut resources,
                scene: &scene,
                surface_pool,
                mask_pool,
                destination_pool,
                frame,
                textures,
                prepared: &prepared,
                surface_to_model,
            };
            context.encode_masks()?;
            for id in prepared.active_offscreens.iter().copied() {
                context.encode_surface(&mut rendered_surfaces, id, surface_size)?;
            }

            context.encode_target(
                TargetEncoding {
                    id: None,
                    view: target_view,
                    texture: target_texture,
                    size: (self.planner.target.width, self.planner.target.height),
                    draw_transform: viewport.transform,
                    composite_transform: surface_to_target,
                    label: "kasane.wgpu.scene.main-pass",
                },
                scene.main.iter().copied(),
            )?;
        }
        queue.submit(Some(encoder.finish()));
        drop(resources);

        Ok(WgpuSceneFrame {
            prepared,
            target: self.planner.target,
            active_surface_count: surface_pool.len(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneEvent<'a> {
    Draw(DrawItem<'a>),
    Composite(&'a str),
}

struct SceneGraph<'a> {
    main: Vec<SceneEvent<'a>>,
    surfaces: HashMap<&'a str, Vec<SceneEvent<'a>>>,
}

#[derive(Clone, Copy)]
enum PipelineKind {
    Draw,
    Composite,
    MaskedDraw,
    MaskedComposite,
    AdditiveDraw,
    MultiplicativeDraw,
    MaskedAdditiveDraw,
    MaskedMultiplicativeDraw,
    ExtendedDraw,
    ExtendedComposite,
    MaskedExtendedDraw,
    MaskedExtendedComposite,
    MaskSource,
}

struct SceneCommand {
    resource_index: usize,
    pipeline: PipelineKind,
}

struct ScenePipelines<'a> {
    draw: &'a wgpu::RenderPipeline,
    additive_draw: &'a wgpu::RenderPipeline,
    multiplicative_draw: &'a wgpu::RenderPipeline,
    composite: &'a wgpu::RenderPipeline,
    masked_draw: &'a wgpu::RenderPipeline,
    masked_additive_draw: &'a wgpu::RenderPipeline,
    masked_multiplicative_draw: &'a wgpu::RenderPipeline,
    masked_composite: &'a wgpu::RenderPipeline,
    extended_draw: &'a wgpu::RenderPipeline,
    extended_composite: &'a wgpu::RenderPipeline,
    masked_extended_draw: &'a wgpu::RenderPipeline,
    masked_extended_composite: &'a wgpu::RenderPipeline,
    mask_source: &'a wgpu::RenderPipeline,
}

struct MaskRenderInfo {
    width: u32,
    height: u32,
    origin: Vec2,
    scale: f32,
    source_ids: Vec<String>,
    view: wgpu::TextureView,
}

fn build_scene<'a>(prepared: &PreparedFrame<'a>) -> Result<SceneGraph<'a>, Status> {
    let mut scene = SceneGraph {
        main: Vec::new(),
        surfaces: HashMap::new(),
    };
    let mut stack: Vec<&'a str> = Vec::new();

    for pass in &prepared.passes {
        match *pass {
            RenderPass::Main => {}
            RenderPass::Offscreen { id, parent } => {
                if parent != stack.last().copied() {
                    return Err(Status::error(
                        "INVALID_RENDER_PLAN",
                        "Offscreen parent does not match the active surface.",
                    ));
                }
                scene.surfaces.entry(id).or_default();
                stack.push(id);
            }
            RenderPass::Composite { id, parent } => {
                let current = stack.last().copied();
                let expected_parent = stack.iter().rev().nth(1).copied();
                if current != Some(id) || parent != expected_parent {
                    return Err(Status::error(
                        "INVALID_RENDER_PLAN",
                        "Composite pass does not match the active surface.",
                    ));
                }
                scene_events(&mut scene, parent).push(SceneEvent::Composite(id));
            }
            // Mask attachments are rendered once before scene surfaces. The
            // pass remains in the shared stream as a dependency marker.
            RenderPass::Mask { .. } => {}
            RenderPass::Draw(item) => {
                scene_events(&mut scene, stack.last().copied()).push(SceneEvent::Draw(item));
            }
            RenderPass::EndOffscreen { id } => {
                if stack.pop() != Some(id) {
                    return Err(Status::error(
                        "INVALID_RENDER_PLAN",
                        "End-offscreen pass does not match the active surface.",
                    ));
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err(Status::error(
            "INVALID_RENDER_PLAN",
            "Prepared render passes contain an unclosed offscreen.",
        ));
    }
    Ok(scene)
}

fn scene_events<'scene, 'a>(
    scene: &'scene mut SceneGraph<'a>,
    target: Option<&'a str>,
) -> &'scene mut Vec<SceneEvent<'a>> {
    match target {
        Some(id) => scene.surfaces.entry(id).or_default(),
        None => &mut scene.main,
    }
}

struct SceneEncoder<'renderer, 'context, 'frame, 'texture> {
    renderer: &'renderer WgpuBasicRenderer,
    pipelines: ScenePipelines<'renderer>,
    device: &'context wgpu::Device,
    encoder: &'context mut wgpu::CommandEncoder,
    resources: &'context mut Vec<DrawResources>,
    scene: &'context SceneGraph<'frame>,
    surface_pool: &'context WgpuSurfacePool,
    mask_pool: &'context WgpuMaskPool,
    destination_pool: &'context WgpuDestinationPool,
    frame: &'frame DrawableFrame,
    textures: &'context WgpuTextureCatalog<'texture>,
    prepared: &'context PreparedFrame<'frame>,
    surface_to_model: Affine2,
}

struct TargetEncoding<'frame, 'context> {
    id: Option<&'frame str>,
    view: &'context wgpu::TextureView,
    texture: Option<&'context wgpu::Texture>,
    size: (u32, u32),
    draw_transform: Affine2,
    composite_transform: Affine2,
    label: &'static str,
}

impl<'renderer, 'context, 'frame, 'texture> SceneEncoder<'renderer, 'context, 'frame, 'texture> {
    fn encode_masks(&mut self) -> Result<(), Status> {
        let masks: Vec<MaskRenderInfo> = self
            .mask_pool
            .iter()
            .map(|mask| MaskRenderInfo {
                width: mask.width,
                height: mask.height,
                origin: mask.origin,
                scale: mask.scale,
                source_ids: mask.source_ids.clone(),
                view: mask.view.clone(),
            })
            .collect();

        for mask in masks {
            let transform = Affine2 {
                a: Vec2::new(mask.scale, 0.0),
                b: Vec2::new(0.0, mask.scale),
                origin: Vec2::new(-mask.origin.x * mask.scale, -mask.origin.y * mask.scale),
            };
            let mut commands = Vec::new();
            for source_id in &mask.source_ids {
                let drawable = self
                    .frame
                    .drawables
                    .iter()
                    .find(|drawable| drawable.id == *source_id)
                    .ok_or_else(|| Status::error("INVALID_MASK", source_id))?;
                if drawable.indices.is_empty() {
                    continue;
                }
                let texture = self
                    .textures
                    .get(&drawable.texture_asset_id)
                    .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
                let target_size = (mask.width, mask.height);
                let vertices = vertices_for(
                    drawable,
                    self.frame,
                    ViewportConfig {
                        transform,
                        target_extent: Vec2::new(mask.width as f32, mask.height as f32),
                        mask_scale: 1.0,
                    },
                );
                let indices = triangle_indices(&drawable.indices);
                let resource_index = self.resources.len();
                self.resources
                    .push(self.renderer.create_resources(ResourceInput {
                        device: self.device,
                        texture_view: texture.view,
                        vertices: &vertices,
                        indices: &indices,
                        uniform: draw_uniform(target_size, [1.0; 4], [0.0, 0.0, 0.0, 1.0], 1.0),
                        mask: None,
                        destination: None,
                    }));
                commands.push(SceneCommand {
                    resource_index,
                    pipeline: PipelineKind::MaskSource,
                });
            }
            encode_render_pass(
                self.encoder,
                &mask.view,
                &self.pipelines,
                self.resources,
                &commands,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                "kasane.wgpu.scene.mask-pass",
            );
        }
        Ok(())
    }

    fn encode_surface(
        &mut self,
        rendered_surfaces: &mut HashSet<&'frame str>,
        id: &'frame str,
        surface_size: (u32, u32),
    ) -> Result<(), Status> {
        if !rendered_surfaces.insert(id) {
            return Ok(());
        }
        let events = self
            .scene
            .surfaces
            .get(id)
            .ok_or_else(|| {
                Status::error(
                    "INVALID_RENDER_PLAN",
                    "Missing surface scene for offscreen target.",
                )
            })?
            .clone();
        for event in events.iter().copied() {
            if let SceneEvent::Composite(child) = event {
                if self.prepared.active_offscreens.contains(child) {
                    self.encode_surface(rendered_surfaces, child, surface_size)?;
                }
            }
        }

        let (surface_view, surface_texture) = self
            .surface_pool
            .get(id)
            .map(|surface| (surface.view.clone(), surface._texture.clone()))
            .ok_or_else(|| {
                Status::error(
                    "MISSING_SURFACE",
                    format!("No wgpu surface was allocated for {id}."),
                )
            })?;
        self.encode_target(
            TargetEncoding {
                id: Some(id),
                view: &surface_view,
                texture: Some(&surface_texture),
                size: surface_size,
                draw_transform: self.prepared.surface_transform,
                composite_transform: Affine2::IDENTITY,
                label: "kasane.wgpu.scene.offscreen-pass",
            },
            events,
        )
    }

    fn encode_target<'target, I>(
        &mut self,
        target: TargetEncoding<'frame, 'target>,
        events: I,
    ) -> Result<(), Status>
    where
        I: IntoIterator<Item = SceneEvent<'frame>>,
    {
        let mut commands = Vec::new();
        let mut first_pass = true;
        for event in events {
            if self.destination_read(event) {
                encode_render_pass(
                    self.encoder,
                    target.view,
                    &self.pipelines,
                    self.resources,
                    &commands,
                    if first_pass {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    target.label,
                );
                first_pass = false;
                commands.clear();
                self.copy_destination(target.id, target.texture, target.size)?;
            }
            self.append_scene_command(
                &mut commands,
                event,
                target.id,
                target.size,
                target.draw_transform,
                target.composite_transform,
            )?;
        }
        if first_pass || !commands.is_empty() {
            encode_render_pass(
                self.encoder,
                target.view,
                &self.pipelines,
                self.resources,
                &commands,
                if first_pass {
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                } else {
                    wgpu::LoadOp::Load
                },
                target.label,
            );
        }
        Ok(())
    }

    fn destination_read(&self, event: SceneEvent<'frame>) -> bool {
        match event {
            SceneEvent::Draw(item) => {
                self.prepared.destination_reads.contains(item.drawable_id)
                    && self
                        .frame
                        .drawables
                        .iter()
                        .find(|drawable| drawable.id == item.drawable_id)
                        .is_some_and(|drawable| {
                            drawable.visible
                                && drawable.opacity > 0.0
                                && !drawable.indices.is_empty()
                        })
            }
            SceneEvent::Composite(id) => {
                self.prepared.destination_reads.contains(id)
                    && self.prepared.active_offscreens.contains(id)
            }
        }
    }

    fn copy_destination(
        &mut self,
        target_id: Option<&'frame str>,
        target_texture: Option<&wgpu::Texture>,
        target_size: (u32, u32),
    ) -> Result<(), Status> {
        let source = target_texture
            .or_else(|| target_id.and_then(|id| self.surface_pool.get(id).map(|s| &s._texture)))
            .ok_or_else(|| {
                Status::error(
                    "MISSING_TARGET_TEXTURE",
                    "Destination reads require the main target texture, not only its view.",
                )
            })?;
        if !source.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(Status::error(
                "INVALID_TARGET_USAGE",
                "Destination reads require COPY_SRC usage on the target texture.",
            ));
        }
        let destination = self.destination_pool.get(target_id).ok_or_else(|| {
            Status::error(
                "MISSING_DESTINATION",
                "No destination snapshot was allocated for the active target.",
            )
        })?;
        self.encoder.copy_texture_to_texture(
            source.as_image_copy(),
            destination._texture.as_image_copy(),
            wgpu::Extent3d {
                width: target_size.0,
                height: target_size.1,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    fn append_scene_command(
        &mut self,
        commands: &mut Vec<SceneCommand>,
        event: SceneEvent<'frame>,
        target_id: Option<&'frame str>,
        target_size: (u32, u32),
        draw_transform: Affine2,
        composite_transform: Affine2,
    ) -> Result<(), Status> {
        match event {
            SceneEvent::Draw(item) => {
                let drawable = self
                    .frame
                    .drawables
                    .iter()
                    .find(|drawable| drawable.id == item.drawable_id)
                    .ok_or_else(|| Status::error("INVALID_RENDER_PLAN", item.drawable_id))?;
                if !drawable.visible || drawable.opacity <= 0.0 || drawable.indices.is_empty() {
                    return Ok(());
                }
                let destination_read = drawable.raw_blend_mode.is_some();
                let texture = self
                    .textures
                    .get(item.texture_id)
                    .ok_or_else(|| Status::error("MISSING_TEXTURE", item.texture_id))?;
                let vertices = vertices_for(
                    drawable,
                    self.frame,
                    ViewportConfig {
                        transform: draw_transform,
                        target_extent: Vec2::new(target_size.0 as f32, target_size.1 as f32),
                        mask_scale: 1.0,
                    },
                );
                let indices = triangle_indices(&drawable.indices);
                let mask_data = if drawable.masks.is_empty() {
                    None
                } else {
                    let mask = self
                        .mask_pool
                        .get_for_target(&drawable.id)
                        .ok_or_else(|| Status::error("MISSING_MASK", &drawable.id))?;
                    Some((
                        mask.view.clone(),
                        [
                            mask.origin.x,
                            mask.origin.y,
                            mask.logical_size.x,
                            mask.logical_size.y,
                        ],
                        drawable.inverted_mask,
                    ))
                };
                let mut uniform = draw_uniform(
                    target_size,
                    drawable.multiply_color,
                    drawable.screen_color,
                    drawable.opacity,
                );
                if let Some((_, bounds, inverted)) = &mask_data {
                    uniform.mask_bounds = *bounds;
                    uniform.mask_flags = [1, u32::from(*inverted), 0, 0];
                }
                if let Some(mode) = drawable.raw_blend_mode {
                    uniform = with_blend_mode(uniform, mode);
                } else {
                    uniform = with_fixed_blend_mode(uniform, drawable.blend_mode);
                }
                let mask_binding = mask_data.as_ref().map(|(view, _, _)| MaskBinding { view });
                let destination_binding = if destination_read {
                    Some(DestinationBinding {
                        view: &self
                            .destination_pool
                            .get(target_id)
                            .ok_or_else(|| {
                                Status::error(
                                    "MISSING_DESTINATION",
                                    "No destination snapshot was allocated for the active target.",
                                )
                            })?
                            .view,
                    })
                } else {
                    None
                };
                let resource_index = self.resources.len();
                self.resources
                    .push(self.renderer.create_resources(ResourceInput {
                        device: self.device,
                        texture_view: texture.view,
                        vertices: &vertices,
                        indices: &indices,
                        uniform,
                        mask: mask_binding,
                        destination: destination_binding,
                    }));
                let pipeline = if destination_read {
                    if mask_data.is_some() {
                        PipelineKind::MaskedExtendedDraw
                    } else {
                        PipelineKind::ExtendedDraw
                    }
                } else {
                    match (drawable.blend_mode, mask_data.is_some()) {
                        (BlendMode::Normal, false) => PipelineKind::Draw,
                        (BlendMode::Normal, true) => PipelineKind::MaskedDraw,
                        (BlendMode::Additive, false) => PipelineKind::AdditiveDraw,
                        (BlendMode::Additive, true) => PipelineKind::MaskedAdditiveDraw,
                        (BlendMode::Multiplicative, false) => PipelineKind::MultiplicativeDraw,
                        (BlendMode::Multiplicative, true) => PipelineKind::MaskedMultiplicativeDraw,
                    }
                };
                commands.push(SceneCommand {
                    resource_index,
                    pipeline,
                });
            }
            SceneEvent::Composite(id) => {
                if !self.prepared.active_offscreens.contains(id) {
                    return Ok(());
                }
                let surface = self.surface_pool.get(id).ok_or_else(|| {
                    Status::error(
                        "MISSING_SURFACE",
                        format!("No wgpu surface was allocated for {id}."),
                    )
                })?;
                let offscreen = self
                    .frame
                    .offscreens
                    .iter()
                    .find(|offscreen| offscreen.id == id)
                    .ok_or_else(|| Status::error("INVALID_RENDER_PLAN", id))?;
                let destination_read = offscreen.blend_mode != 0;
                let vertices = quad_vertices(
                    (surface.width, surface.height),
                    composite_transform,
                    self.surface_to_model,
                );
                let indices = quad_indices();
                let mask_data = if offscreen.masks.is_empty() {
                    None
                } else {
                    let mask = self
                        .mask_pool
                        .get_for_target(id)
                        .ok_or_else(|| Status::error("MISSING_MASK", id))?;
                    Some((
                        mask.view.clone(),
                        [
                            mask.origin.x,
                            mask.origin.y,
                            mask.logical_size.x,
                            mask.logical_size.y,
                        ],
                        offscreen.flags & 8 != 0,
                    ))
                };
                let mut uniform = draw_uniform(
                    target_size,
                    offscreen.multiply_color,
                    offscreen.screen_color,
                    offscreen.opacity,
                );
                if let Some((_, bounds, inverted)) = &mask_data {
                    uniform.mask_bounds = *bounds;
                    uniform.mask_flags = [1, u32::from(*inverted), 0, 0];
                }
                if destination_read {
                    uniform = with_blend_mode(uniform, offscreen.blend_mode);
                }
                let mask_binding = mask_data.as_ref().map(|(view, _, _)| MaskBinding { view });
                let destination_binding = if destination_read {
                    Some(DestinationBinding {
                        view: &self
                            .destination_pool
                            .get(target_id)
                            .ok_or_else(|| {
                                Status::error(
                                    "MISSING_DESTINATION",
                                    "No destination snapshot was allocated for the active target.",
                                )
                            })?
                            .view,
                    })
                } else {
                    None
                };
                let resource_index = self.resources.len();
                self.resources
                    .push(self.renderer.create_resources(ResourceInput {
                        device: self.device,
                        texture_view: &surface.view,
                        vertices: &vertices,
                        indices: &indices,
                        uniform,
                        mask: mask_binding,
                        destination: destination_binding,
                    }));
                commands.push(SceneCommand {
                    resource_index,
                    pipeline: if destination_read {
                        if mask_data.is_some() {
                            PipelineKind::MaskedExtendedComposite
                        } else {
                            PipelineKind::ExtendedComposite
                        }
                    } else if mask_data.is_some() {
                        PipelineKind::MaskedComposite
                    } else {
                        PipelineKind::Composite
                    },
                });
            }
        }
        Ok(())
    }
}

fn encode_render_pass(
    encoder: &mut wgpu::CommandEncoder,
    target_view: &wgpu::TextureView,
    pipelines: &ScenePipelines<'_>,
    resources: &[DrawResources],
    commands: &[SceneCommand],
    load: wgpu::LoadOp<wgpu::Color>,
    label: &'static str,
) {
    let color_attachments = [Some(wgpu::RenderPassColorAttachment {
        view: target_view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    })];
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    for command in commands {
        let resource = &resources[command.resource_index];
        pass.set_pipeline(match command.pipeline {
            PipelineKind::Draw => pipelines.draw,
            PipelineKind::Composite => pipelines.composite,
            PipelineKind::MaskedDraw => pipelines.masked_draw,
            PipelineKind::MaskedComposite => pipelines.masked_composite,
            PipelineKind::AdditiveDraw => pipelines.additive_draw,
            PipelineKind::MultiplicativeDraw => pipelines.multiplicative_draw,
            PipelineKind::MaskedAdditiveDraw => pipelines.masked_additive_draw,
            PipelineKind::MaskedMultiplicativeDraw => pipelines.masked_multiplicative_draw,
            PipelineKind::ExtendedDraw => pipelines.extended_draw,
            PipelineKind::ExtendedComposite => pipelines.extended_composite,
            PipelineKind::MaskedExtendedDraw => pipelines.masked_extended_draw,
            PipelineKind::MaskedExtendedComposite => pipelines.masked_extended_composite,
            PipelineKind::MaskSource => pipelines.mask_source,
        });
        pass.set_bind_group(0, &resource.texture_bind_group, &[]);
        pass.set_bind_group(1, &resource.uniform_bind_group, &[]);
        match command.pipeline {
            PipelineKind::ExtendedDraw | PipelineKind::ExtendedComposite => {
                if let Some(destination_bind_group) = &resource.destination_bind_group {
                    pass.set_bind_group(2, destination_bind_group, &[]);
                }
            }
            PipelineKind::MaskedExtendedDraw | PipelineKind::MaskedExtendedComposite => {
                if let Some(destination_bind_group) = &resource.destination_bind_group {
                    pass.set_bind_group(2, destination_bind_group, &[]);
                }
                if let Some(mask_bind_group) = &resource.mask_bind_group {
                    pass.set_bind_group(3, mask_bind_group, &[]);
                }
            }
            PipelineKind::MaskedDraw
            | PipelineKind::MaskedComposite
            | PipelineKind::MaskedAdditiveDraw
            | PipelineKind::MaskedMultiplicativeDraw => {
                if let Some(mask_bind_group) = &resource.mask_bind_group {
                    pass.set_bind_group(2, mask_bind_group, &[]);
                }
            }
            PipelineKind::Draw
            | PipelineKind::Composite
            | PipelineKind::AdditiveDraw
            | PipelineKind::MultiplicativeDraw
            | PipelineKind::MaskSource => {}
        }
        pass.set_vertex_buffer(0, resource.vertex_buffer.slice(..));
        pass.set_index_buffer(resource.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..resource.index_count, 0, 0..1);
    }
}

fn draw_uniform(
    target_size: (u32, u32),
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
    opacity: f32,
) -> DrawUniform {
    DrawUniform {
        target_size: [target_size.0 as f32, target_size.1 as f32],
        padding: [0.0; 2],
        multiply_color,
        screen_color,
        opacity,
        padding_end: [0.0; 3],
        mask_bounds: [0.0; 4],
        mask_flags: [0; 4],
        blend_modes: [0; 4],
    }
}

fn with_blend_mode(mut uniform: DrawUniform, mode: u32) -> DrawUniform {
    uniform.blend_modes = [mode & 0xff, (mode >> 8) & 0xff, 0, 0];
    uniform
}

fn with_fixed_blend_mode(mut uniform: DrawUniform, mode: BlendMode) -> DrawUniform {
    uniform.blend_modes[2] = match mode {
        BlendMode::Normal => 0,
        BlendMode::Additive => 1,
        BlendMode::Multiplicative => 2,
    };
    uniform
}

fn quad_vertices(size: (u32, u32), transform: Affine2, mask_transform: Affine2) -> Vec<Vertex> {
    [
        (Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
        (Vec2::new(size.0 as f32, 0.0), Vec2::new(1.0, 0.0)),
        (Vec2::new(size.0 as f32, size.1 as f32), Vec2::new(1.0, 1.0)),
        (Vec2::new(0.0, size.1 as f32), Vec2::new(0.0, 1.0)),
    ]
    .into_iter()
    .map(|(position, uv)| {
        let mask_point = mask_transform.transform_point(position);
        let position = transform.transform_point(position);
        Vertex {
            position: [position.x, position.y],
            uv: [uv.x, uv.y],
            mask_point: [mask_point.x, mask_point.y],
        }
    })
    .collect()
}

fn quad_indices() -> Vec<u32> {
    triangle_indices(&[0, 1, 2, 0, 2, 3])
}

fn inverse_affine(transform: Affine2) -> Result<Affine2, Status> {
    let determinant = transform.determinant();
    if !transform.is_finite() || determinant.abs() < 1.0e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    let inverse_a = Vec2::new(transform.b.y / determinant, -transform.a.y / determinant);
    let inverse_b = Vec2::new(-transform.b.x / determinant, transform.a.x / determinant);
    let origin = Vec2::new(
        -(inverse_a.x * transform.origin.x + inverse_b.x * transform.origin.y),
        -(inverse_a.y * transform.origin.x + inverse_b.y * transform.origin.y),
    );
    Ok(Affine2 {
        a: inverse_a,
        b: inverse_b,
        origin,
    })
}

fn compose_affine(lhs: Affine2, rhs: Affine2) -> Affine2 {
    let origin = lhs.transform_point(rhs.origin);
    let lhs_origin = lhs.transform_point(Vec2::new(0.0, 0.0));
    let a_point = lhs.transform_point(rhs.a);
    let b_point = lhs.transform_point(rhs.b);
    Affine2 {
        a: Vec2::new(a_point.x - lhs_origin.x, a_point.y - lhs_origin.y),
        b: Vec2::new(b_point.x - lhs_origin.x, b_point.y - lhs_origin.y),
        origin,
    }
}

fn vertices_for(
    drawable: &Drawable,
    frame: &DrawableFrame,
    viewport: ViewportConfig,
) -> Vec<Vertex> {
    drawable
        .positions
        .iter()
        .zip(drawable.uvs.iter())
        .map(|(position, uv)| {
            let pixel = Vec2::new(
                position.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                frame.canvas.origin.y - position.y * frame.canvas.pixels_per_unit,
            );
            let mask_point = pixel;
            let pixel = viewport.transform.transform_point(pixel);
            Vertex {
                position: [pixel.x, pixel.y],
                uv: [uv.x, 1.0 - uv.y],
                mask_point: [mask_point.x, mask_point.y],
            }
        })
        .collect()
}

fn triangle_indices(indices: &[u32]) -> Vec<u32> {
    let (triangles, _) = indices.as_chunks::<3>();
    triangles
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect()
}

const BASIC_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let multiplied = source.rgb * draw.multiply_color.rgb;
    let rgb = multiplied + draw.screen_color.rgb - multiplied * draw.screen_color.rgb;
    let alpha = source.a * draw.opacity;
    if (draw.blend_modes.z == 1u) {
        return vec4<f32>(rgb * alpha, 0.0);
    }
    if (draw.blend_modes.z == 2u) {
        return vec4<f32>(rgb * alpha + vec3<f32>(1.0 - alpha), 1.0);
    }
    return vec4<f32>(rgb * alpha, alpha);
}
"#;

const COMPOSITE_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let rgb = source.rgb * draw.multiply_color.rgb
        + draw.screen_color.rgb * source.a
        - source.rgb * draw.screen_color.rgb;
    return vec4<f32>(rgb * draw.opacity, source.a * draw.opacity);
}
"#;

const MASKED_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var mask_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let multiplied = source.rgb * draw.multiply_color.rgb;
    let rgb = multiplied + draw.screen_color.rgb - multiplied * draw.screen_color.rgb;
    let alpha = source.a * draw.opacity * mask_alpha(input.mask_point);
    if (draw.blend_modes.z == 1u) {
        return vec4<f32>(rgb * alpha, 0.0);
    }
    if (draw.blend_modes.z == 2u) {
        return vec4<f32>(rgb * alpha + vec3<f32>(1.0 - alpha), 1.0);
    }
    return vec4<f32>(rgb * alpha, alpha);
}
"#;

const MASKED_COMPOSITE_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var mask_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let rgb = source.rgb * draw.multiply_color.rgb
        + draw.screen_color.rgb * source.a
        - source.rgb * draw.screen_color.rgb;
    let mask = mask_alpha(input.mask_point);
    return vec4<f32>(rgb * draw.opacity * mask, source.a * draw.opacity * mask);
}
"#;

const MASK_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, textureSample(main_texture, main_sampler, input.uv).a);
}
"#;

const EXTENDED_HEADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var destination_texture: texture_2d<f32>;
@group(2) @binding(1) var destination_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        input.position.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - input.position.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}
"#;

const EXTENDED_MASK: &str = r#"
@group(3) @binding(0) var mask_texture: texture_2d<f32>;
@group(3) @binding(1) var mask_sampler: sampler;

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}
"#;

const EXTENDED_DRAW_BODY: &str = r#"
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var source = textureSample(main_texture, main_sampler, input.uv);
    source.rgb *= draw.multiply_color.rgb;
    source.rgb = source.rgb + draw.screen_color.rgb - source.rgb * draw.screen_color.rgb;
    source.a *= draw.opacity;
    if (draw.mask_flags.x != 0u) {
        source *= mask_alpha(input.mask_point);
    }
    let destination_uv = input.position.xy / draw.target_size;
    let destination = to_straight(textureSample(destination_texture, destination_sampler, destination_uv));
    return composite_blend(to_straight(source), destination, draw.blend_modes.x, draw.blend_modes.y);
}
"#;

const EXTENDED_COMPOSITE_BODY: &str = r#"
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var source = textureSample(main_texture, main_sampler, input.uv);
    source.rgb *= draw.multiply_color.rgb;
    source.rgb = source.rgb + draw.screen_color.rgb * source.a - source.rgb * draw.screen_color.rgb;
    source *= draw.opacity;
    if (draw.mask_flags.x != 0u) {
        source *= mask_alpha(input.mask_point);
    }
    let destination_uv = input.position.xy / draw.target_size;
    let destination = to_straight(textureSample(destination_texture, destination_sampler, destination_uv));
    return composite_blend(to_straight(source), destination, draw.blend_modes.x, draw.blend_modes.y);
}
"#;

fn extended_shader_source(composite: bool, masked: bool) -> String {
    let body = if composite {
        EXTENDED_COMPOSITE_BODY
    } else {
        EXTENDED_DRAW_BODY
    };
    [
        EXTENDED_HEADER,
        include_str!("../shaders/extended_blend.wgsl"),
        if masked { EXTENDED_MASK } else { "" },
        body,
    ]
    .concat()
}

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

fn unsupported(feature: &str) -> Status {
    Status::error(
        "UNSUPPORTED_WGPU_FEATURE",
        format!("The current wgpu backend does not support {feature}."),
    )
}

fn ensure_basic_blend(drawable: &Drawable) -> Result<(), Status> {
    if drawable.raw_blend_mode.is_some() || drawable.blend_mode != BlendMode::Normal {
        return Err(unsupported("extended or non-normal blend modes"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use kasane_core::evaluation::{Drawable, RenderCommand};
    use kasane_core::types::{Canvas, Vec2};
    use kasane_render::{Affine2, TextureInfo};

    fn drawable(id: &str) -> Drawable {
        Drawable {
            id: id.to_owned(),
            texture_asset_id: "texture".to_owned(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
            uvs: Arc::from([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }
    }

    fn planner() -> WgpuFramePlanner {
        WgpuFramePlanner::new(WgpuTargetConfig {
            width: 640,
            height: 480,
            ..Default::default()
        })
        .unwrap()
    }

    fn viewport() -> ViewportConfig {
        ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(640.0, 480.0),
            mask_scale: 1.0,
        }
    }

    fn textures() -> HashMap<String, TextureInfo> {
        HashMap::from([(
            "texture".to_owned(),
            TextureInfo {
                width: 64,
                height: 64,
            },
        )])
    }

    #[test]
    fn prepares_flat_normal_draws_without_a_gpu_device() {
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![drawable("mesh")],
            ..Default::default()
        };
        let prepared = planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap();
        assert_eq!(prepared.target.width, 640);
        assert_eq!(prepared.draws[0].drawable_id, "mesh");
        assert_eq!(prepared.draws[0].index_count, 3);
    }

    #[test]
    fn rejects_offscreen_passes_before_gpu_allocation() {
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![drawable("mesh")],
            render_plan: vec![
                kasane_core::evaluation::RenderCommand::BeginOffscreen {
                    offscreen_id: "surface".to_owned(),
                },
                kasane_core::evaluation::RenderCommand::DrawMesh {
                    mesh_id: "mesh".to_owned(),
                },
                kasane_core::evaluation::RenderCommand::EndOffscreen {
                    offscreen_id: "surface".to_owned(),
                },
            ],
            offscreens: vec![kasane_core::evaluation::OffscreenFrame {
                id: "surface".to_owned(),
                enabled: true,
                opacity: 1.0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let failure = planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap_err();
        assert_eq!(failure.code, "UNSUPPORTED_WGPU_FEATURE");
    }

    #[test]
    fn rejects_non_normal_blends_at_the_backend_boundary() {
        let mut mesh = drawable("mesh");
        mesh.blend_mode = BlendMode::Additive;
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![mesh],
            ..Default::default()
        };
        let failure = planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap_err();
        assert_eq!(failure.code, "UNSUPPORTED_WGPU_FEATURE");
    }

    #[test]
    fn omits_invisible_and_transparent_draws_from_basic_plan() {
        let mut invisible = drawable("invisible");
        invisible.visible = false;
        let mut transparent = drawable("transparent");
        transparent.opacity = 0.0;
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![invisible, transparent],
            ..Default::default()
        };

        let prepared = planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap();
        assert!(prepared.draws.is_empty());
    }

    #[test]
    fn reverses_triangle_winding_for_y_down_targets() {
        assert_eq!(
            triangle_indices(&[0, 1, 2, 3, 4, 5]),
            vec![0, 2, 1, 3, 5, 4]
        );
    }

    #[test]
    fn prepares_normal_offscreen_scene_without_a_gpu_device() {
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![drawable("mesh")],
            offscreens: vec![kasane_core::evaluation::OffscreenFrame {
                id: "surface".to_owned(),
                enabled: true,
                opacity: 1.0,
                ..Default::default()
            }],
            render_plan: vec![
                RenderCommand::BeginOffscreen {
                    offscreen_id: "surface".to_owned(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "mesh".to_owned(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "surface".to_owned(),
                },
            ],
            ..Default::default()
        };

        let prepared = planner()
            .prepare_scene(&frame, &textures(), viewport())
            .unwrap();
        assert!(prepared.active_offscreens.contains("surface"));
        assert_eq!(prepared.surface_size.width, 640);
        assert_eq!(
            build_scene(&prepared).unwrap().main,
            vec![SceneEvent::Composite("surface")]
        );
    }

    #[test]
    fn prepares_destination_read_scene_without_a_gpu_device() {
        let mut mesh = drawable("mesh");
        mesh.raw_blend_mode = Some(1);
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![mesh],
            ..Default::default()
        };

        let prepared = planner()
            .prepare_scene(&frame, &textures(), viewport())
            .unwrap();
        assert!(prepared.destination_reads.contains("mesh"));
        assert_eq!(
            build_scene(&prepared).unwrap().main,
            vec![SceneEvent::Draw(DrawItem {
                drawable_id: "mesh",
                texture_id: "texture"
            })]
        );
        assert_eq!(
            planner()
                .prepare_basic(&frame, &textures(), viewport())
                .unwrap_err()
                .code,
            "UNSUPPORTED_WGPU_FEATURE"
        );
    }

    #[test]
    fn prepares_masked_scene_and_keeps_mask_out_of_scene_order() {
        let source = drawable("source");
        let mut target = drawable("target");
        target.masks.push("source".to_owned());
        let frame = DrawableFrame {
            canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
            drawables: vec![source, target],
            ..Default::default()
        };

        let prepared = planner()
            .prepare_scene(&frame, &textures(), viewport())
            .unwrap();
        assert!(prepared.mask_consumers.is_empty());
        let scene = build_scene(&prepared).unwrap();
        assert_eq!(
            scene.main,
            vec![
                SceneEvent::Draw(DrawItem {
                    drawable_id: "source",
                    texture_id: "texture"
                }),
                SceneEvent::Draw(DrawItem {
                    drawable_id: "target",
                    texture_id: "texture"
                })
            ]
        );
        assert_eq!(
            mask_layout(&frame, &["source".to_owned()], 1.0)
                .unwrap()
                .logical_size,
            Vec2::new(108.0, 108.0)
        );
        assert_eq!(
            planner()
                .prepare_basic(&frame, &textures(), viewport())
                .unwrap_err()
                .code,
            "UNSUPPORTED_WGPU_FEATURE"
        );
    }

    #[test]
    fn scene_parser_keeps_nested_composites_in_parent_order() {
        let prepared = PreparedFrame {
            active_offscreens: HashSet::from(["outer", "inner"]),
            mask_consumers: HashMap::new(),
            destination_reads: HashSet::new(),
            surface_size: Size2 {
                width: 640,
                height: 480,
            },
            surface_transform: Affine2::IDENTITY,
            passes: vec![
                RenderPass::Main,
                RenderPass::Draw(DrawItem {
                    drawable_id: "root",
                    texture_id: "texture",
                }),
                RenderPass::Offscreen {
                    id: "outer",
                    parent: None,
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
                RenderPass::Draw(DrawItem {
                    drawable_id: "inner-mesh",
                    texture_id: "texture",
                }),
                RenderPass::EndOffscreen { id: "inner" },
                RenderPass::EndOffscreen { id: "outer" },
            ],
        };

        let scene = build_scene(&prepared).unwrap();
        assert_eq!(
            scene.main,
            vec![
                SceneEvent::Draw(DrawItem {
                    drawable_id: "root",
                    texture_id: "texture"
                }),
                SceneEvent::Composite("outer")
            ]
        );
        assert_eq!(
            scene.surfaces["outer"],
            vec![
                SceneEvent::Draw(DrawItem {
                    drawable_id: "outer-mesh",
                    texture_id: "texture"
                }),
                SceneEvent::Composite("inner")
            ]
        );
        assert_eq!(
            scene.surfaces["inner"],
            vec![SceneEvent::Draw(DrawItem {
                drawable_id: "inner-mesh",
                texture_id: "texture"
            })]
        );
    }

    #[test]
    fn validates_surface_extent_and_affine_round_trip() {
        assert_eq!(
            surface_extent(Size2 {
                width: 3,
                height: 4
            }),
            Ok((3, 4))
        );
        assert!(surface_extent(Size2 {
            width: 0,
            height: 4
        })
        .is_err());
        assert!(surface_extent(Size2 {
            width: 3,
            height: -1
        })
        .is_err());

        let transform = Affine2 {
            a: Vec2::new(2.0, 0.25),
            b: Vec2::new(-0.5, 1.5),
            origin: Vec2::new(12.0, -7.0),
        };
        let inverse = inverse_affine(transform).unwrap();
        let point = Vec2::new(8.0, 13.0);
        let round_trip = compose_affine(inverse, transform).transform_point(point);
        assert!((round_trip.x - point.x).abs() < 1.0e-5);
        assert!((round_trip.y - point.y).abs() < 1.0e-5);
    }
}
