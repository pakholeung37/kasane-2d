use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SceneEvent<'a> {
    Draw(DrawItem<'a>),
    Composite(&'a str),
}

pub(super) struct SceneGraph<'a> {
    pub(super) main: Vec<SceneEvent<'a>>,
    pub(super) surfaces: HashMap<&'a str, Vec<SceneEvent<'a>>>,
}

#[derive(Clone, Copy)]
pub(super) enum PipelineKind {
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

pub(super) struct SceneCommand {
    pub(super) resource_index: usize,
    pub(super) pipeline: PipelineKind,
}

pub(super) struct ScenePipelines<'a> {
    pub(super) draw: &'a wgpu::RenderPipeline,
    pub(super) additive_draw: &'a wgpu::RenderPipeline,
    pub(super) multiplicative_draw: &'a wgpu::RenderPipeline,
    pub(super) composite: &'a wgpu::RenderPipeline,
    pub(super) masked_draw: &'a wgpu::RenderPipeline,
    pub(super) masked_additive_draw: &'a wgpu::RenderPipeline,
    pub(super) masked_multiplicative_draw: &'a wgpu::RenderPipeline,
    pub(super) masked_composite: &'a wgpu::RenderPipeline,
    pub(super) extended_draw: &'a wgpu::RenderPipeline,
    pub(super) extended_composite: &'a wgpu::RenderPipeline,
    pub(super) masked_extended_draw: &'a wgpu::RenderPipeline,
    pub(super) masked_extended_composite: &'a wgpu::RenderPipeline,
    pub(super) mask_source: &'a wgpu::RenderPipeline,
}

pub(super) struct MaskRenderInfo {
    pub(super) key: MaskKey,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) origin: Vec2,
    pub(super) scale: f32,
    pub(super) source_ids: Vec<String>,
    pub(super) view: wgpu::TextureView,
}

pub(super) fn build_scene<'a>(prepared: &PreparedFrame<'a>) -> Result<SceneGraph<'a>, Status> {
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

pub(super) fn scene_events<'scene, 'a>(
    scene: &'scene mut SceneGraph<'a>,
    target: Option<&'a str>,
) -> &'scene mut Vec<SceneEvent<'a>> {
    match target {
        Some(id) => scene.surfaces.entry(id).or_default(),
        None => &mut scene.main,
    }
}

pub(super) struct SceneEncoder<'renderer, 'context, 'frame, 'texture> {
    pub(super) renderer: &'renderer WgpuBasicRenderer,
    pub(super) pipelines: ScenePipelines<'renderer>,
    pub(super) device: &'context wgpu::Device,
    pub(super) encoder: &'context mut wgpu::CommandEncoder,
    pub(super) resources: &'context mut Vec<DrawResources>,
    pub(super) scene: &'context SceneGraph<'frame>,
    pub(super) surface_pool: &'context WgpuSurfacePool,
    pub(super) mask_pool: &'context WgpuMaskPool,
    pub(super) destination_pool: &'context WgpuDestinationPool,
    pub(super) frame: &'frame DrawableFrame,
    pub(super) textures: &'context WgpuTextureCatalog<'texture>,
    pub(super) prepared: &'context PreparedFrame<'frame>,
    pub(super) surface_to_model: Affine2,
    pub(super) queue: &'context wgpu::Queue,
    pub(super) geometry: Option<&'context mut modern::GeometryCache>,
    pub(super) bindings: Option<&'context mut modern::BindingCache>,
    pub(super) dirty_masks: Option<&'context HashSet<MaskKey>>,
}

pub(super) struct TargetEncoding<'frame, 'context> {
    pub(super) id: Option<&'frame str>,
    pub(super) view: &'context wgpu::TextureView,
    pub(super) texture: Option<&'context wgpu::Texture>,
    pub(super) size: (u32, u32),
    pub(super) draw_transform: Affine2,
    pub(super) composite_transform: Affine2,
    pub(super) label: &'static str,
}

impl<'renderer, 'context, 'frame, 'texture> SceneEncoder<'renderer, 'context, 'frame, 'texture> {
    pub(super) fn encode_masks(&mut self) -> Result<(), Status> {
        let masks: Vec<MaskRenderInfo> = self
            .mask_pool
            .masks
            .iter()
            .map(|(key, mask)| MaskRenderInfo {
                key: key.clone(),
                width: mask.width,
                height: mask.height,
                origin: mask.origin,
                scale: mask.scale,
                source_ids: mask.source_ids.clone(),
                view: mask.view.clone(),
            })
            .collect();

        for mask in masks {
            if self
                .dirty_masks
                .is_some_and(|dirty| !dirty.contains(&mask.key))
            {
                continue;
            }
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
                let cache_geometry = self.geometry.is_some();
                let vertices = vertices_for(
                    drawable,
                    self.frame,
                    ViewportConfig {
                        transform: if cache_geometry {
                            Affine2::IDENTITY
                        } else {
                            transform
                        },
                        target_extent: Vec2::new(mask.width as f32, mask.height as f32),
                        mask_scale: 1.0,
                    },
                );
                let indices = triangle_indices(&drawable.indices);
                let resource_index = self.resources.len();
                let mut uniform = draw_uniform(target_size, [1.0; 4], [0.0, 0.0, 0.0, 1.0], 1.0);
                if cache_geometry {
                    uniform = with_view_transform(uniform, transform);
                }
                let input = ResourceInput {
                    device: self.device,
                    texture_view: texture.view,
                    texture_repeat: self.textures.repeats(&drawable.texture_asset_id),
                    vertices: &vertices,
                    indices: &indices,
                    uniform,
                    mask: None,
                    destination: None,
                };
                let resource = if let Some(cache) = self.geometry.as_deref_mut() {
                    let (vertex_buffer, index_buffer) =
                        cache.sync(self.device, self.queue, source_id, &vertices, &indices);
                    if let Some(bindings) = self.bindings.as_deref_mut() {
                        bindings.prepare(
                            modern::DrawKey::MaskSource {
                                mask: mask.key.clone(),
                                mesh: source_id.clone(),
                            },
                            self.renderer,
                            self.queue,
                            input,
                            vertex_buffer,
                            index_buffer,
                        )
                    } else {
                        self.renderer.create_resources_with_geometry(
                            input,
                            vertex_buffer,
                            index_buffer,
                        )
                    }
                } else {
                    self.renderer.create_resources(input)
                };
                self.resources.push(resource);
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

    pub(super) fn encode_surface(
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

    pub(super) fn encode_target<'target, I>(
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

    pub(super) fn destination_read(&self, event: SceneEvent<'frame>) -> bool {
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

    pub(super) fn copy_destination(
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

    pub(super) fn append_scene_command(
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
                let cache_geometry = self.geometry.is_some();
                let vertices = vertices_for(
                    drawable,
                    self.frame,
                    ViewportConfig {
                        transform: if cache_geometry {
                            Affine2::IDENTITY
                        } else {
                            draw_transform
                        },
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
                if cache_geometry {
                    uniform = with_view_transform(uniform, draw_transform);
                }
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
                let input = ResourceInput {
                    device: self.device,
                    texture_view: texture.view,
                    texture_repeat: self.textures.repeats(&drawable.texture_asset_id),
                    vertices: &vertices,
                    indices: &indices,
                    uniform,
                    mask: mask_binding,
                    destination: destination_binding,
                };
                let resource = if let Some(cache) = self.geometry.as_deref_mut() {
                    let (vertex_buffer, index_buffer) = cache.sync(
                        self.device,
                        self.queue,
                        item.drawable_id,
                        &vertices,
                        &indices,
                    );
                    if let Some(bindings) = self.bindings.as_deref_mut() {
                        bindings.prepare(
                            modern::DrawKey::Mesh(item.drawable_id.to_owned()),
                            self.renderer,
                            self.queue,
                            input,
                            vertex_buffer,
                            index_buffer,
                        )
                    } else {
                        self.renderer.create_resources_with_geometry(
                            input,
                            vertex_buffer,
                            index_buffer,
                        )
                    }
                } else {
                    self.renderer.create_resources(input)
                };
                self.resources.push(resource);
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
                let cache_geometry = self.geometry.is_some();
                let source_scale = Affine2 {
                    a: Vec2::new(surface.width as f32, 0.0),
                    b: Vec2::new(0.0, surface.height as f32),
                    origin: Vec2::new(0.0, 0.0),
                };
                let vertices = quad_vertices(
                    if cache_geometry {
                        (1, 1)
                    } else {
                        (surface.width, surface.height)
                    },
                    if cache_geometry {
                        Affine2::IDENTITY
                    } else {
                        composite_transform
                    },
                    if cache_geometry {
                        Affine2::IDENTITY
                    } else {
                        self.surface_to_model
                    },
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
                if cache_geometry {
                    uniform = with_view_transform(
                        uniform,
                        compose_affine(composite_transform, source_scale),
                    );
                    uniform = with_mask_transform(
                        uniform,
                        compose_affine(self.surface_to_model, source_scale),
                    );
                }
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
                let input = ResourceInput {
                    device: self.device,
                    texture_view: &surface.view,
                    texture_repeat: false,
                    vertices: &vertices,
                    indices: &indices,
                    uniform,
                    mask: mask_binding,
                    destination: destination_binding,
                };
                let resource = if let Some(bindings) = self.bindings.as_deref_mut() {
                    let (vertex_buffer, index_buffer) = self
                        .geometry
                        .as_deref_mut()
                        .expect("binding cache requires geometry cache")
                        .sync(
                            self.device,
                            self.queue,
                            &format!("\0composite:{id}"),
                            &vertices,
                            &indices,
                        );
                    bindings.prepare(
                        modern::DrawKey::Composite(id.to_owned()),
                        self.renderer,
                        self.queue,
                        input,
                        vertex_buffer,
                        index_buffer,
                    )
                } else {
                    self.renderer.create_resources(input)
                };
                self.resources.push(resource);
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

pub(super) fn encode_render_pass(
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
