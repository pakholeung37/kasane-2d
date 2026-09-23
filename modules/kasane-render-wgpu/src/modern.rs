//! ScenePlan-driven entry point. The old pass-stream API remains temporarily
//! available while callers migrate, but is not used to plan this path.
use super::*;
use kasane_render::{surface_layout, ScenePlan, TargetItem};
use std::sync::Arc;

/// Owns all resources for one device and one output size/format.
/// The host owns the device, queue, source textures and final output view.
pub struct WgpuRenderer {
    device: wgpu::Device,
    renderer: WgpuBasicRenderer,
    scene: ScenePlan,
    model: Option<RenderSnapshot>,
    viewport: Option<ViewportConfig>,
    color: WgpuSurface,
    surfaces: WgpuSurfacePool,
    masks: WgpuMaskPool,
    destinations: WgpuDestinationPool,
    geometry: GeometryCache,
    bindings: BindingCache,
    mask_signatures: HashMap<MaskKey, MaskSignature>,
    model_generation: u64,
}

/// The render-relevant portion of a submitted frame. Its topology arrays are
/// copied so the renderer never pins buffers owned by the caller's frame.
struct RenderSnapshot {
    frame: DrawableFrame,
}

impl RenderSnapshot {
    fn from_frame(frame: &DrawableFrame) -> Self {
        let drawables = frame
            .drawables
            .iter()
            .map(|drawable| {
                let mut owned = drawable.clone();
                owned.runtime_id.clear();
                owned.part_id.clear();
                owned.uvs = Arc::from(drawable.uvs.as_ref().to_vec());
                owned.indices = Arc::from(drawable.indices.as_ref().to_vec());
                owned
            })
            .collect();
        let offscreens = frame
            .offscreens
            .iter()
            .map(|offscreen| {
                let mut owned = offscreen.clone();
                owned.runtime_id.clear();
                owned.owner_part_id.clear();
                owned
            })
            .collect();
        Self {
            frame: DrawableFrame {
                canvas: frame.canvas,
                drawables,
                offscreens,
                ..Default::default()
            },
        }
    }
}

/// Host-owned objects needed to encode one scene. Submit the encoder before
/// encoding another scene with the same renderer, since mutable GPU buffers
/// are synchronized through the queue.
pub struct WgpuEncodeTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub output: &'a wgpu::TextureView,
    pub output_mode: WgpuOutputMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WgpuOutputMode {
    #[default]
    Replace,
    Composite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuRenderStats {
    pub active_surfaces: usize,
    pub masks: usize,
    pub destination_targets: usize,
    pub vertex_upload_bytes: u64,
    pub index_upload_bytes: u64,
    pub geometry_buffer_creations: u64,
    pub uniform_buffer_creations: u64,
    pub bind_group_creations: u64,
    pub mask_redraws: usize,
    pub attachment_bytes: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct MaskSignature {
    attachment: wgpu::TextureView,
    model_generation: u64,
    sources: Vec<(wgpu::TextureView, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum DrawKey {
    MaskSource { mask: MaskKey, mesh: String },
    Mesh(String),
    Composite(String),
    Present,
}

#[derive(Clone, PartialEq, Eq)]
struct BoundViews {
    texture: wgpu::TextureView,
    mask: Option<wgpu::TextureView>,
    destination: Option<wgpu::TextureView>,
}

impl BoundViews {
    fn from_input(input: &ResourceInput<'_>) -> Self {
        Self {
            texture: input.texture_view.clone(),
            mask: input.mask.map(|mask| mask.view.clone()),
            destination: input
                .destination
                .map(|destination| destination.view.clone()),
        }
    }
}

struct CachedBindings {
    views: BoundViews,
    resources: DrawResources,
}

#[derive(Default)]
pub(super) struct BindingCache {
    entries: HashMap<DrawKey, CachedBindings>,
    used: HashSet<DrawKey>,
    uniform_buffer_creations: u64,
    bind_group_creations: u64,
}

impl BindingCache {
    fn begin_frame(&mut self) {
        self.used.clear();
        self.uniform_buffer_creations = 0;
        self.bind_group_creations = 0;
    }

    fn finish_frame(&mut self, masks: &WgpuMaskPool) {
        self.entries.retain(|key, _| {
            self.used.contains(key)
                || matches!(key, DrawKey::MaskSource { mask, .. } if masks.masks.contains_key(mask))
        });
    }

    pub(super) fn prepare(
        &mut self,
        key: DrawKey,
        renderer: &WgpuBasicRenderer,
        queue: &wgpu::Queue,
        input: ResourceInput<'_>,
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
    ) -> DrawResources {
        let views = BoundViews::from_input(&input);
        self.used.insert(key.clone());
        if let Some(cached) = self.entries.get_mut(&key) {
            if cached.views == views {
                queue.write_buffer(
                    &cached.resources._uniform_buffer,
                    0,
                    bytemuck::bytes_of(&input.uniform),
                );
                cached.resources.vertex_buffer = vertex_buffer;
                cached.resources.index_buffer = index_buffer;
                cached.resources.index_count = input.indices.len() as u32;
                return cached.resources.clone();
            }
        }
        let bind_groups =
            2 + u64::from(views.mask.is_some()) + u64::from(views.destination.is_some());
        let resources = renderer.create_resources_with_geometry(input, vertex_buffer, index_buffer);
        self.entries.insert(
            key,
            CachedBindings {
                views,
                resources: resources.clone(),
            },
        );
        self.uniform_buffer_creations += 1;
        self.bind_group_creations += bind_groups;
        resources
    }
}

#[derive(Default)]
pub(super) struct GeometryCache {
    entries: HashMap<String, GpuGeometry>,
    vertex_upload_bytes: u64,
    index_upload_bytes: u64,
    buffer_creations: u64,
}

struct GpuGeometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

impl GeometryCache {
    pub(super) fn sync(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: &str,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        use std::collections::hash_map::Entry;
        match self.entries.entry(id.to_owned()) {
            Entry::Vacant(slot) => {
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("kasane.wgpu.cached.vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("kasane.wgpu.cached.indices"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                });
                self.vertex_upload_bytes += std::mem::size_of_val(vertices) as u64;
                self.index_upload_bytes += std::mem::size_of_val(indices) as u64;
                self.buffer_creations += 2;
                slot.insert(GpuGeometry {
                    vertices: vertices.to_vec(),
                    indices: indices.to_vec(),
                    vertex_buffer: vertex_buffer.clone(),
                    index_buffer: index_buffer.clone(),
                });
                (vertex_buffer, index_buffer)
            }
            Entry::Occupied(mut slot) => {
                let geometry = slot.get_mut();
                if geometry.vertices != vertices {
                    let bytes = bytemuck::cast_slice(vertices);
                    if bytes.len() as u64 > geometry.vertex_buffer.size() {
                        geometry.vertex_buffer =
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("kasane.wgpu.cached.vertices"),
                                contents: bytes,
                                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            });
                        self.buffer_creations += 1;
                    } else {
                        queue.write_buffer(&geometry.vertex_buffer, 0, bytes);
                    }
                    geometry.vertices.clear();
                    geometry.vertices.extend_from_slice(vertices);
                    self.vertex_upload_bytes += bytes.len() as u64;
                }
                if geometry.indices != indices {
                    let bytes = bytemuck::cast_slice(indices);
                    if bytes.len() as u64 > geometry.index_buffer.size() {
                        geometry.index_buffer =
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("kasane.wgpu.cached.indices"),
                                contents: bytes,
                                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                            });
                        self.buffer_creations += 1;
                    } else {
                        queue.write_buffer(&geometry.index_buffer, 0, bytes);
                    }
                    geometry.indices.clear();
                    geometry.indices.extend_from_slice(indices);
                    self.index_upload_bytes += bytes.len() as u64;
                }
                (
                    geometry.vertex_buffer.clone(),
                    geometry.index_buffer.clone(),
                )
            }
        }
    }
}

impl WgpuRenderer {
    pub fn new(device: &wgpu::Device, target: WgpuTargetConfig) -> Result<Self, Status> {
        validate_target(device, target)?;
        let renderer = WgpuBasicRenderer::new(device, target)?;
        let color = create_color(device, target);
        Ok(Self {
            device: device.clone(),
            renderer,
            scene: ScenePlan::default(),
            model: None,
            viewport: None,
            color,
            surfaces: WgpuSurfacePool::new(target.format),
            masks: WgpuMaskPool::new(),
            destinations: WgpuDestinationPool::new(),
            geometry: GeometryCache::default(),
            bindings: BindingCache::default(),
            mask_signatures: HashMap::new(),
            model_generation: 0,
        })
    }

    pub fn target(&self) -> WgpuTargetConfig {
        self.renderer.planner.target()
    }

    /// Update the output extent or format after a host surface resize.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        target: WgpuTargetConfig,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        validate_target(device, target)?;
        if self.target() == target {
            return Ok(());
        }
        if self.target().format != target.format {
            self.renderer = WgpuBasicRenderer::new(device, target)?;
            self.surfaces = WgpuSurfacePool::new(target.format);
            self.masks.clear();
            self.destinations.clear();
            self.bindings = BindingCache::default();
            self.mask_signatures.clear();
        } else {
            self.renderer.planner.target = target;
        }
        self.color = create_color(device, target);
        self.viewport = None;
        Ok(())
    }

    /// Validate and publish a model submission without retaining the caller's
    /// frame or its topology allocations. A rejected submission keeps the last
    /// successful model available for rendering.
    pub fn sync_model(
        &mut self,
        device: &wgpu::Device,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        let mut candidate = self.scene.clone();
        candidate.update(frame, textures)?;
        if let Some(viewport) = self.viewport {
            lower_scene(&candidate, frame, viewport, self.target(), device)?;
        }
        let snapshot = RenderSnapshot::from_frame(frame);
        self.scene = candidate;
        self.model = Some(snapshot);
        self.model_generation = self.model_generation.wrapping_add(1);
        Ok(())
    }

    /// Validate a camera or output-view change without revalidating the model.
    /// The previous view remains active when the new layout is rejected.
    pub fn update_view(
        &mut self,
        device: &wgpu::Device,
        viewport: ViewportConfig,
    ) -> Result<(), Status> {
        self.ensure_device(device)?;
        let model = self.model.as_ref().ok_or_else(|| {
            Status::error("MISSING_MODEL", "Submit a model before updating the view.")
        })?;
        lower_scene(&self.scene, &model.frame, viewport, self.target(), device)?;
        self.viewport = Some(viewport);
        Ok(())
    }

    /// Encode the last submitted model and view into a host-owned encoder.
    /// The host chooses when to submit and how to observe GPU completion.
    pub fn encode(
        &mut self,
        target: WgpuEncodeTarget<'_>,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<WgpuRenderStats, Status> {
        let WgpuEncodeTarget {
            device,
            queue,
            encoder,
            output,
            output_mode,
        } = target;
        self.ensure_device(device)?;
        let frame = &self
            .model
            .as_ref()
            .ok_or_else(|| Status::error("MISSING_MODEL", "Submit a model before encoding."))?
            .frame;
        let viewport = self
            .viewport
            .ok_or_else(|| Status::error("MISSING_VIEW", "Update the view before encoding."))?;
        for drawable in &frame.drawables {
            let texture = textures
                .get(&drawable.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
            if texture.width == 0 || texture.height == 0 {
                return Err(Status::error("INVALID_TEXTURE", &drawable.texture_asset_id));
            }
        }
        self.geometry.vertex_upload_bytes = 0;
        self.geometry.index_upload_bytes = 0;
        self.geometry.buffer_creations = 0;
        self.bindings.begin_frame();
        self.geometry.entries.retain(|id, _| {
            id == "\0present"
                || id
                    .strip_prefix("\0composite:")
                    .is_some_and(|name| self.scene.target_id(name).is_some())
                || self.scene.mesh_id(id).is_some()
        });
        let prepared = lower_scene(&self.scene, frame, viewport, self.target(), device)?;
        let graph = build_scene_from_plan(&self.scene, frame);
        self.surfaces.sync(device, &prepared)?;
        self.masks.sync(device, frame, &prepared, viewport)?;
        let (dirty_masks, next_signatures) = self.mask_updates(textures)?;
        let target = self.target();
        sync_destinations(
            &mut self.destinations,
            device,
            target,
            &self.scene,
            &prepared,
            frame,
        )?;
        let surface_size = surface_extent(prepared.surface_size)?;
        let surface_to_model = inverse_affine(prepared.surface_transform)?;
        let surface_to_target = compose_affine(viewport.transform, surface_to_model);
        let mut resources = Vec::new();
        let mut rendered_surfaces = HashSet::new();
        {
            let mut context = SceneEncoder {
                renderer: &self.renderer,
                pipelines: self.renderer.scene_pipelines(),
                device,
                encoder,
                resources: &mut resources,
                scene: &graph,
                surface_pool: &self.surfaces,
                mask_pool: &self.masks,
                destination_pool: &self.destinations,
                frame,
                textures,
                prepared: &prepared,
                surface_to_model,
                queue,
                geometry: Some(&mut self.geometry),
                bindings: Some(&mut self.bindings),
                dirty_masks: Some(&dirty_masks),
            };
            context.encode_masks()?;
            for id in prepared.active_offscreens.iter().copied() {
                context.encode_surface(&mut rendered_surfaces, id, surface_size)?;
            }
            context.encode_target(
                TargetEncoding {
                    id: None,
                    view: &self.color.view,
                    texture: Some(&self.color._texture),
                    size: (target.width, target.height),
                    draw_transform: viewport.transform,
                    composite_transform: surface_to_target,
                    label: "kasane.wgpu.scene-plan.main",
                },
                graph.main.iter().copied(),
            )?;
        }
        // The backend-owned color target makes destination reads independent
        // of the host output texture's COPY_SRC capability.
        let presentation_vertices = quad_vertices(
            (target.width, target.height),
            Affine2::IDENTITY,
            Affine2::IDENTITY,
        );
        let presentation_indices = quad_indices();
        let (vertex_buffer, index_buffer) = self.geometry.sync(
            device,
            queue,
            "\0present",
            &presentation_vertices,
            &presentation_indices,
        );
        let presentation = self.bindings.prepare(
            DrawKey::Present,
            &self.renderer,
            queue,
            ResourceInput {
                device,
                texture_view: &self.color.view,
                vertices: &presentation_vertices,
                indices: &presentation_indices,
                uniform: draw_uniform(
                    (target.width, target.height),
                    [1.0; 4],
                    [0.0, 0.0, 0.0, 1.0],
                    1.0,
                ),
                mask: None,
                destination: None,
            },
            vertex_buffer,
            index_buffer,
        );
        resources.push(presentation);
        encode_render_pass(
            encoder,
            output,
            &self.renderer.scene_pipelines(),
            &resources,
            &[SceneCommand {
                resource_index: resources.len() - 1,
                pipeline: PipelineKind::Composite,
            }],
            match output_mode {
                WgpuOutputMode::Replace => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                WgpuOutputMode::Composite => wgpu::LoadOp::Load,
            },
            "kasane.wgpu.scene-plan.present",
        );
        self.bindings.finish_frame(&self.masks);
        self.mask_signatures = next_signatures;
        Ok(WgpuRenderStats {
            active_surfaces: self.surfaces.len(),
            masks: self.masks.len(),
            destination_targets: self.destinations.len(),
            vertex_upload_bytes: self.geometry.vertex_upload_bytes,
            index_upload_bytes: self.geometry.index_upload_bytes,
            geometry_buffer_creations: self.geometry.buffer_creations,
            uniform_buffer_creations: self.bindings.uniform_buffer_creations,
            bind_group_creations: self.bindings.bind_group_creations,
            mask_redraws: dirty_masks.len(),
            attachment_bytes: attachment_bytes(
                &self.color,
                &self.surfaces,
                &self.masks,
                &self.destinations,
            ),
        })
    }

    fn mask_updates(
        &self,
        textures: &WgpuTextureCatalog<'_>,
    ) -> Result<(HashSet<MaskKey>, HashMap<MaskKey, MaskSignature>), Status> {
        let frame = &self.model.as_ref().expect("validated model").frame;
        let mut dirty = HashSet::new();
        let mut next = HashMap::new();
        for (key, mask) in &self.masks.masks {
            let mut sources = Vec::with_capacity(mask.source_ids.len());
            let mut has_revisions = true;
            for id in &mask.source_ids {
                let drawable = frame
                    .drawables
                    .iter()
                    .find(|drawable| drawable.id == *id)
                    .ok_or_else(|| Status::error("INVALID_MASK", id))?;
                let texture = textures
                    .get(&drawable.texture_asset_id)
                    .ok_or_else(|| Status::error("MISSING_TEXTURE", &drawable.texture_asset_id))?;
                if let Some(revision) = textures.revision(&drawable.texture_asset_id) {
                    sources.push((texture.view.clone(), revision));
                } else {
                    has_revisions = false;
                }
            }
            if has_revisions {
                let signature = MaskSignature {
                    attachment: mask.view.clone(),
                    model_generation: self.model_generation,
                    sources,
                };
                if self.mask_signatures.get(key) != Some(&signature) {
                    dirty.insert(key.clone());
                }
                next.insert(key.clone(), signature);
            } else {
                dirty.insert(key.clone());
            }
        }
        Ok((dirty, next))
    }

    fn ensure_device(&self, device: &wgpu::Device) -> Result<(), Status> {
        if *device != self.device {
            return Err(Status::error(
                "DEVICE_MISMATCH",
                "Recreate the WGPU renderer after changing devices.",
            ));
        }
        Ok(())
    }

    /// Convenience wrapper for hosts that do not need to add commands to the
    /// same encoder. `encode` is the preferred integration boundary.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &wgpu::TextureView,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
        viewport: ViewportConfig,
    ) -> Result<WgpuRenderStats, Status> {
        self.sync_model(device, frame, textures)?;
        self.update_view(device, viewport)?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("kasane.wgpu.scene-plan.encoder"),
        });
        let stats = self.encode(
            WgpuEncodeTarget {
                device,
                queue,
                encoder: &mut encoder,
                output,
                output_mode: WgpuOutputMode::Replace,
            },
            textures,
        )?;
        queue.submit([encoder.finish()]);
        Ok(stats)
    }
}

fn validate_target(device: &wgpu::Device, target: WgpuTargetConfig) -> Result<(), Status> {
    if target.width == 0
        || target.height == 0
        || target.width > device.limits().max_texture_dimension_2d
        || target.height > device.limits().max_texture_dimension_2d
    {
        return Err(Status::error(
            "INVALID_TARGET",
            "Output extent exceeds device limits.",
        ));
    }
    if !matches!(
        target.format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        return Err(Status::error(
            "UNSUPPORTED_TARGET_FORMAT",
            "The WGPU scene target requires an 8-bit RGBA or BGRA format.",
        ));
    }
    let required_usages = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    let features = target.format.guaranteed_format_features(device.features());
    if !features.allowed_usages.contains(required_usages)
        || !features.flags.contains(
            wgpu::TextureFormatFeatureFlags::FILTERABLE
                | wgpu::TextureFormatFeatureFlags::BLENDABLE,
        )
    {
        return Err(Status::error(
            "UNSUPPORTED_TARGET_FORMAT",
            "The output format lacks the required render, sample, copy, or blend capability.",
        ));
    }
    if texture_bytes(target.width, target.height) > kasane_render::OFFSCREEN_BUDGET_BYTES as u64 {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "The output target alone exceeds the WGPU attachment budget.",
        ));
    }
    Ok(())
}

fn create_color(device: &wgpu::Device, target: WgpuTargetConfig) -> WgpuSurface {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("kasane.wgpu.scene-plan.color"),
        size: wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    WgpuSurface {
        width: target.width,
        height: target.height,
        view,
        _texture: texture,
    }
}

fn lower_scene<'a>(
    scene: &'a ScenePlan,
    frame: &'a DrawableFrame,
    viewport: ViewportConfig,
    target: WgpuTargetConfig,
    device: &wgpu::Device,
) -> Result<PreparedFrame<'a>, Status> {
    for drawable in &frame.drawables {
        let vertex_bytes =
            (drawable.positions.len() as u64).saturating_mul(std::mem::size_of::<Vertex>() as u64);
        let index_bytes =
            (drawable.indices.len() as u64).saturating_mul(std::mem::size_of::<u32>() as u64);
        if vertex_bytes > device.limits().max_buffer_size
            || index_bytes > device.limits().max_buffer_size
            || drawable.indices.len() > u32::MAX as usize
        {
            return Err(Status::error(
                "GPU_BUFFER_LIMIT",
                format!("{} exceeds the device mesh buffer limit.", drawable.id),
            ));
        }
    }
    if !viewport.mask_scale.is_finite() || viewport.mask_scale <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask scale must be positive and finite.",
        ));
    }
    if viewport.target_extent != Vec2::new(target.width as f32, target.height as f32) {
        return Err(Status::error(
            "INVALID_TARGET",
            "Viewport extent must match the output target.",
        ));
    }
    let (surface_size, surface_transform) = surface_layout(
        Vec2::new(scene.canvas().width, scene.canvas().height),
        viewport.transform,
        viewport.target_extent,
    )?;
    let limit = device.limits().max_texture_dimension_2d as i32;
    if surface_size.width > limit || surface_size.height > limit {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "Offscreen surface exceeds the device texture limit.",
        ));
    }
    let active_offscreens: HashSet<_> = scene
        .targets()
        .iter()
        .skip(1)
        .filter(|target| target.active)
        .map(|target| target.id.as_str())
        .collect();
    let destination_reads: HashSet<_> = scene
        .meshes()
        .iter()
        .filter(|mesh| mesh.reads_destination)
        .map(|mesh| mesh.id.as_str())
        .chain(
            scene
                .targets()
                .iter()
                .skip(1)
                .filter(|target| target.reads_destination)
                .map(|target| target.id.as_str()),
        )
        .collect();
    let mut mask_consumers = HashMap::new();
    for mesh in scene.meshes().iter().filter(|mesh| mesh.mask.is_some()) {
        mask_consumers.insert(mesh.id.clone(), scene.targets()[mesh.target.0].id.clone());
    }
    for group in scene
        .targets()
        .iter()
        .skip(1)
        .filter(|group| group.mask.is_some())
    {
        mask_consumers.insert(group.id.clone(), scene.targets()[group.parent.0].id.clone());
    }
    // This is a physical preflight, independent of the compatibility planner.
    let main_bytes = texture_bytes(target.width, target.height);
    let surface_bytes = texture_bytes(surface_size.width as u32, surface_size.height as u32);
    let mut bytes = main_bytes
        .saturating_add((scene.active_target_count() as u64).saturating_mul(surface_bytes));
    for (index, owner) in scene.targets().iter().enumerate() {
        let reads = owner.items.iter().any(|item| match item {
            TargetItem::Draw(id) => {
                scene.meshes()[id.0].reads_destination
                    && frame.drawables[id.0].visible
                    && frame.drawables[id.0].opacity > 0.0
                    && !frame.drawables[id.0].indices.is_empty()
            }
            TargetItem::Composite(id) => {
                scene.targets()[id.0].reads_destination && scene.targets()[id.0].active
            }
        });
        if reads && (index == 0 || owner.active) {
            bytes = bytes.saturating_add(if index == 0 {
                main_bytes
            } else {
                surface_bytes
            });
        }
    }
    let mut mask_keys = HashSet::new();
    let mask_limit = device.limits().max_texture_dimension_2d.min(4096);
    for (mesh, drawable) in scene.meshes().iter().zip(&frame.drawables) {
        if mesh.mask.is_some() {
            let key = MaskKey::new(
                &drawable.masks,
                viewport.mask_scale,
                &scene.targets()[mesh.target.0].id,
            );
            if mask_keys.insert(key) {
                let layout = mask_layout_with_limit(
                    frame,
                    &drawable.masks,
                    viewport.mask_scale,
                    mask_limit,
                )?;
                bytes = bytes.saturating_add(texture_bytes(layout.width, layout.height));
            }
        }
    }
    for (group, offscreen) in scene
        .targets()
        .iter()
        .skip(1)
        .zip(&frame.offscreens)
        .filter(|(group, _)| group.active)
    {
        if group.mask.is_some() {
            let scale = viewport.mask_scale.max(1.0);
            let key = MaskKey::new(&offscreen.masks, scale, &scene.targets()[group.parent.0].id);
            if mask_keys.insert(key) {
                let layout = mask_layout_with_limit(frame, &offscreen.masks, scale, mask_limit)?;
                bytes = bytes.saturating_add(texture_bytes(layout.width, layout.height));
            }
        }
    }
    if bytes > kasane_render::OFFSCREEN_BUDGET_BYTES as u64 {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            "WGPU attachments exceed the configured budget.",
        ));
    }
    Ok(PreparedFrame {
        active_offscreens,
        mask_consumers,
        destination_reads,
        surface_size,
        surface_transform,
        passes: Vec::new(),
    })
}

fn texture_bytes(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height) * 4
}

fn attachment_bytes(
    main: &WgpuSurface,
    surfaces: &WgpuSurfacePool,
    masks: &WgpuMaskPool,
    destinations: &WgpuDestinationPool,
) -> u64 {
    texture_bytes(main.width, main.height)
        + surfaces
            .surfaces
            .values()
            .map(|surface| texture_bytes(surface.width, surface.height))
            .sum::<u64>()
        + masks
            .masks
            .values()
            .map(|mask| texture_bytes(mask.width, mask.height))
            .sum::<u64>()
        + destinations
            .snapshots
            .values()
            .map(|snapshot| texture_bytes(snapshot.width, snapshot.height))
            .sum::<u64>()
}

fn build_scene_from_plan<'a>(scene: &'a ScenePlan, frame: &'a DrawableFrame) -> SceneGraph<'a> {
    let mut graph = SceneGraph {
        main: Vec::new(),
        surfaces: HashMap::new(),
    };
    for (index, target) in scene.targets().iter().enumerate() {
        let events = if index == 0 {
            &mut graph.main
        } else {
            graph.surfaces.entry(target.id.as_str()).or_default()
        };
        for item in &target.items {
            match item {
                TargetItem::Draw(id) => events.push(SceneEvent::Draw(DrawItem {
                    drawable_id: &scene.meshes()[id.0].id,
                    texture_id: &frame.drawables[id.0].texture_asset_id,
                })),
                TargetItem::Composite(id) => {
                    events.push(SceneEvent::Composite(&scene.targets()[id.0].id))
                }
            }
        }
    }
    graph
}

fn sync_destinations(
    pool: &mut WgpuDestinationPool,
    device: &wgpu::Device,
    target: WgpuTargetConfig,
    scene: &ScenePlan,
    prepared: &PreparedFrame<'_>,
    frame: &DrawableFrame,
) -> Result<(), Status> {
    let mut required = HashSet::new();
    for (index, owner) in scene.targets().iter().enumerate() {
        let reads = owner.items.iter().any(|item| match item {
            TargetItem::Draw(id) => {
                scene.meshes()[id.0].reads_destination
                    && frame.drawables[id.0].visible
                    && frame.drawables[id.0].opacity > 0.0
                    && !frame.drawables[id.0].indices.is_empty()
            }
            TargetItem::Composite(id) => {
                scene.targets()[id.0].reads_destination && scene.targets()[id.0].active
            }
        });
        if reads && (index == 0 || owner.active) {
            required.insert(owner.id.clone());
        }
    }
    let (surface_width, surface_height) = surface_extent(prepared.surface_size)?;
    if pool.format != Some(target.format) {
        pool.snapshots.clear();
        pool.format = Some(target.format);
    }
    pool.snapshots.retain(|id, _| required.contains(id));
    for id in required {
        let (width, height) = if id.is_empty() {
            (target.width, target.height)
        } else {
            (surface_width, surface_height)
        };
        let recreate = pool
            .snapshots
            .get(&id)
            .is_none_or(|snapshot| snapshot.width != width || snapshot.height != height);
        if recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("kasane.wgpu.scene-plan.destination"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: target.format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            pool.snapshots.insert(
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
