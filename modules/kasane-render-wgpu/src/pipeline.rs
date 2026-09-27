use super::*;

pub(super) use kasane_render::gpu::Vertex;

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
    pub(super) mask_a: [f32; 4],
    pub(super) mask_b: [f32; 4],
    pub(super) mask_origin: [f32; 4],
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
    pub(super) texture_repeat: bool,
    pub(super) vertices: &'a [Vertex],
    pub(super) indices: &'a [u32],
    pub(super) uniform: DrawUniform,
    pub(super) mask: Option<MaskBinding<'a>>,
    pub(super) destination: Option<DestinationBinding<'a>>,
}

#[derive(Clone)]
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

/// Pipeline and binding resources owned by the scene renderer.
pub(super) struct WgpuPipelines {
    pub(super) target: WgpuTargetConfig,
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
    pub(super) repeat_sampler: wgpu::Sampler,
}

impl WgpuPipelines {
    pub(super) fn new(device: &wgpu::Device, target: WgpuTargetConfig) -> Self {
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
        let repeat_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kasane.wgpu.basic.repeat-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Self {
            target,
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
            repeat_sampler,
        }
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
            texture_repeat,
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
                    resource: wgpu::BindingResource::Sampler(if texture_repeat {
                        &self.repeat_sampler
                    } else {
                        &self.sampler
                    }),
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
}
