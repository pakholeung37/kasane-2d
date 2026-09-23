use super::*;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct Vertex {
    pub(super) position: [f32; 2],
    pub(super) uv: [f32; 2],
    pub(super) mask_point: [f32; 2],
}

pub(super) const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
];

pub(super) const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &VERTEX_ATTRIBUTES,
};

pub(super) fn premultiplied_blend() -> wgpu::BlendState {
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

pub(super) fn additive_blend() -> wgpu::BlendState {
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

pub(super) fn multiplicative_blend() -> wgpu::BlendState {
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
pub(super) struct DrawUniform {
    pub(super) target_size: [f32; 2],
    pub(super) padding: [f32; 2],
    pub(super) multiply_color: [f32; 4],
    pub(super) screen_color: [f32; 4],
    pub(super) opacity: f32,
    // WGSL aligns the following vec3 to 16 bytes. Keep mask_bounds at byte 80.
    pub(super) padding_end: [f32; 7],
    pub(super) mask_bounds: [f32; 4],
    pub(super) mask_flags: [u32; 4],
    pub(super) blend_modes: [u32; 4],
    pub(super) view_a: [f32; 4],
    pub(super) view_b: [f32; 4],
    pub(super) view_origin: [f32; 4],
}

#[derive(Clone, Copy)]
pub(super) struct MaskBinding<'a> {
    pub(super) view: &'a wgpu::TextureView,
}

#[derive(Clone, Copy)]
pub(super) struct DestinationBinding<'a> {
    pub(super) view: &'a wgpu::TextureView,
}

pub(super) struct ResourceInput<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) texture_view: &'a wgpu::TextureView,
    pub(super) vertices: &'a [Vertex],
    pub(super) indices: &'a [u32],
    pub(super) uniform: DrawUniform,
    pub(super) mask: Option<MaskBinding<'a>>,
    pub(super) destination: Option<DestinationBinding<'a>>,
}

pub(super) struct DrawResources {
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) index_buffer: wgpu::Buffer,
    pub(super) _uniform_buffer: wgpu::Buffer,
    pub(super) texture_bind_group: wgpu::BindGroup,
    pub(super) uniform_bind_group: wgpu::BindGroup,
    pub(super) mask_bind_group: Option<wgpu::BindGroup>,
    pub(super) destination_bind_group: Option<wgpu::BindGroup>,
    pub(super) index_count: u32,
}

pub(super) fn create_pipeline(
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
    pub(super) planner: WgpuFramePlanner,
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) additive_pipeline: wgpu::RenderPipeline,
    pub(super) multiplicative_pipeline: wgpu::RenderPipeline,
    pub(super) composite_pipeline: wgpu::RenderPipeline,
    pub(super) masked_pipeline: wgpu::RenderPipeline,
    pub(super) masked_additive_pipeline: wgpu::RenderPipeline,
    pub(super) masked_multiplicative_pipeline: wgpu::RenderPipeline,
    pub(super) masked_composite_pipeline: wgpu::RenderPipeline,
    pub(super) extended_pipeline: wgpu::RenderPipeline,
    pub(super) extended_composite_pipeline: wgpu::RenderPipeline,
    pub(super) masked_extended_pipeline: wgpu::RenderPipeline,
    pub(super) masked_extended_composite_pipeline: wgpu::RenderPipeline,
    pub(super) mask_pipeline: wgpu::RenderPipeline,
    pub(super) texture_layout: wgpu::BindGroupLayout,
    pub(super) uniform_layout: wgpu::BindGroupLayout,
    pub(super) mask_layout: wgpu::BindGroupLayout,
    pub(super) destination_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
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

    pub(super) fn scene_pipelines(&self) -> ScenePipelines<'_> {
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

    pub(super) fn create_resources(&self, input: ResourceInput<'_>) -> DrawResources {
        let vertex_buffer = input
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kasane.wgpu.scene.vertices"),
                contents: bytemuck::cast_slice(input.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = input
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kasane.wgpu.scene.indices"),
                contents: bytemuck::cast_slice(input.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.create_resources_with_geometry(input, vertex_buffer, index_buffer)
    }

    pub(super) fn create_resources_with_geometry(
        &self,
        input: ResourceInput<'_>,
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
    ) -> DrawResources {
        let ResourceInput {
            device,
            texture_view,
            vertices: _,
            indices,
            uniform,
            mask,
            destination,
        } = input;
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
                queue,
                geometry: None,
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
