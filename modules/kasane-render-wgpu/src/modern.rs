//! ScenePlan-driven entry point. The old pass-stream API remains temporarily
//! available while callers migrate, but is not used to plan this path.
use super::*;
use kasane_render::{surface_layout, ScenePlan, TargetItem};

/// Owns all resources for one device and one output size/format.
/// The host owns the device, queue, source textures and final output view.
pub struct WgpuRenderer {
    renderer: WgpuBasicRenderer,
    scene: ScenePlan,
    color: WgpuSurface,
    surfaces: WgpuSurfacePool,
    masks: WgpuMaskPool,
    destinations: WgpuDestinationPool,
    geometry: GeometryCache,
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
            renderer,
            scene: ScenePlan::default(),
            color,
            surfaces: WgpuSurfacePool::new(target.format),
            masks: WgpuMaskPool::new(),
            destinations: WgpuDestinationPool::new(),
            geometry: GeometryCache::default(),
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
        validate_target(device, target)?;
        if self.target() == target {
            return Ok(());
        }
        if self.target().format != target.format {
            self.renderer = WgpuBasicRenderer::new(device, target)?;
            self.surfaces = WgpuSurfacePool::new(target.format);
            self.masks.clear();
            self.destinations.clear();
        } else {
            self.renderer.planner.target = target;
        }
        self.color = create_color(device, target);
        Ok(())
    }

    /// Encode one evaluated frame into a host-owned encoder. The host chooses
    /// when to submit and how to observe GPU completion. No frame is retained.
    pub fn encode(
        &mut self,
        target: WgpuEncodeTarget<'_>,
        frame: &DrawableFrame,
        textures: &WgpuTextureCatalog<'_>,
        viewport: ViewportConfig,
    ) -> Result<WgpuRenderStats, Status> {
        let WgpuEncodeTarget {
            device,
            queue,
            encoder,
            output,
            output_mode,
        } = target;
        self.scene.update(frame, textures)?;
        self.geometry.vertex_upload_bytes = 0;
        self.geometry.index_upload_bytes = 0;
        self.geometry.buffer_creations = 0;
        self.geometry
            .entries
            .retain(|id, _| self.scene.mesh_id(id).is_some());
        let prepared = lower_scene(&self.scene, frame, viewport, self.target(), device)?;
        let graph = build_scene_from_plan(&self.scene, frame);
        self.surfaces.sync(device, &prepared)?;
        self.masks.sync(device, frame, &prepared, viewport)?;
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
        let presentation = self.renderer.create_resources(ResourceInput {
            device,
            texture_view: &self.color.view,
            vertices: &quad_vertices(
                (target.width, target.height),
                Affine2::IDENTITY,
                Affine2::IDENTITY,
            ),
            indices: &quad_indices(),
            uniform: draw_uniform(
                (target.width, target.height),
                [1.0; 4],
                [0.0, 0.0, 0.0, 1.0],
                1.0,
            ),
            mask: None,
            destination: None,
        });
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
        Ok(WgpuRenderStats {
            active_surfaces: self.surfaces.len(),
            masks: self.masks.len(),
            destination_targets: self.destinations.len(),
            vertex_upload_bytes: self.geometry.vertex_upload_bytes,
            index_upload_bytes: self.geometry.index_upload_bytes,
            geometry_buffer_creations: self.geometry.buffer_creations,
        })
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
            frame,
            textures,
            viewport,
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
    let main_bytes = u64::from(target.width) * u64::from(target.height) * 4;
    let surface_bytes = surface_size.width as u64 * surface_size.height as u64 * 4;
    let mut bytes = main_bytes + scene.active_target_count() as u64 * surface_bytes;
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
            bytes += if index == 0 {
                main_bytes
            } else {
                surface_bytes
            };
        }
    }
    for mesh in scene.meshes() {
        if let Some(mask) = mesh.mask {
            bytes += mask_bytes(scene, mask, viewport.mask_scale, limit)?;
        }
    }
    for group in scene.targets().iter().skip(1).filter(|group| group.active) {
        if let Some(mask) = group.mask {
            bytes += mask_bytes(scene, mask, viewport.mask_scale.max(1.0), limit)?;
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

fn mask_bytes(
    scene: &ScenePlan,
    mask: kasane_render::MaskId,
    scale: f64,
    limit: i32,
) -> Result<u64, Status> {
    let bounds = scene.masks()[mask.0].bounds.grow(4.0);
    let width = bounds.size.x.ceil().max(1.0);
    let height = bounds.size.y.ceil().max(1.0);
    let density = (scale as f32).min(limit.min(4096) as f32 / width.max(height));
    if !density.is_finite() || density <= 0.0 {
        return Err(Status::error(
            "INVALID_MASK_SCALE",
            "Mask layout overflowed.",
        ));
    }
    Ok((width * density).ceil() as u64 * (height * density).ceil() as u64 * 4)
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
