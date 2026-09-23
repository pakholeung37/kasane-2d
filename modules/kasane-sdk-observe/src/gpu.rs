//! A reusable device and renderer for offscreen observation.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::future::Future;
use std::sync::mpsc;
use std::time::Duration;

use kasane_core::Vec2;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig, WgpuTexture,
    WgpuTextureCatalog,
};
use kasane_sdk::Version;
use sha2::{Digest, Sha256};

use crate::{ObservationError, ObservationInput, ResolvedTexture};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverConfig {
    pub width: u32,
    pub height: u32,
    pub fit_long_side: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureRevision {
    pub asset_id: String,
    pub sha256: String,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawableBounds {
    pub id: String,
    pub visible: bool,
    /// Clipped pixel bounds with exclusive maximum coordinates.
    pub bounds: Option<(u32, u32, u32, u32)>,
}

#[derive(Clone, Debug)]
pub struct ObservedFrame {
    /// Hash of the evaluated frame, validated texture content, and output view.
    pub input_sha256: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub version: Version,
    pub evaluation_revision: u64,
    pub document_id: String,
    pub source_revision: u64,
    pub parameters: Vec<(String, f32, f32, bool)>,
    pub canvas: (f32, f32, f32, f32, f32),
    pub view_scale: f32,
    pub view_offset: (f32, f32),
    pub drawable_bounds: Vec<DrawableBounds>,
    pub texture_revisions: Vec<TextureRevision>,
    pub adapter_name: String,
    pub adapter_backend: String,
}

impl ObservedFrame {
    pub fn png_bytes(&self) -> Result<Vec<u8>, ObservationError> {
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&self.rgba))
                .map_err(|failure| error("PNG_ENCODE", failure.to_string()))?;
        }
        Ok(output)
    }
}

struct OwnedTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    sha256: String,
    revision: u64,
}

struct DigestWriter<'a>(&'a mut Sha256);

impl std::fmt::Write for DigestWriter<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0.update(value.as_bytes());
        Ok(())
    }
}

pub struct Observer {
    config: ObserverConfig,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: WgpuRenderer,
    textures: HashMap<String, OwnedTexture>,
    next_texture_revision: u64,
    adapter_name: String,
    adapter_backend: String,
}

fn error(code: &str, message: impl Into<String>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: None,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

impl Observer {
    pub fn new(config: ObserverConfig) -> Result<Self, ObservationError> {
        if config.width == 0
            || config.height == 0
            || !config.fit_long_side.is_finite()
            || config.fit_long_side <= 0.0
        {
            return Err(error(
                "INVALID_VIEW",
                "Output size and fit_long_side must be positive",
            ));
        }
        let instance = wgpu::Instance::default();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|failure| error("GPU_ADAPTER", failure.to_string()))?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|failure| error("GPU_DEVICE", failure.to_string()))?;
        let renderer = WgpuRenderer::new(
            &device,
            WgpuTargetConfig {
                width: config.width,
                height: config.height,
                format: wgpu::TextureFormat::Rgba8Unorm,
            },
        )
        .map_err(|status| error(&status.code, status.message))?;
        Ok(Self {
            config,
            device,
            queue,
            renderer,
            textures: HashMap::new(),
            next_texture_revision: 1,
            adapter_name: info.name,
            adapter_backend: format!("{:?}", info.backend),
        })
    }

    pub fn set_fit_long_side(&mut self, fit_long_side: f32) -> Result<(), ObservationError> {
        if !fit_long_side.is_finite() || fit_long_side <= 0.0 {
            return Err(error("INVALID_VIEW", "fit_long_side must be positive"));
        }
        self.config.fit_long_side = fit_long_side;
        Ok(())
    }

    fn upload_textures(&mut self, resolved: Vec<ResolvedTexture>) {
        let required: HashSet<_> = resolved
            .iter()
            .map(|texture| texture.asset.id.as_str())
            .collect();
        self.textures.retain(|id, _| required.contains(id.as_str()));
        for texture in resolved {
            let id = texture.asset.id;
            if self.textures.get(&id).is_some_and(|cached| {
                cached.sha256 == texture.data.sha256
                    && cached.width == texture.data.width
                    && cached.height == texture.data.height
            }) {
                continue;
            }
            let source = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sdk-observe.source"),
                size: wgpu::Extent3d {
                    width: texture.data.width,
                    height: texture.data.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                source.as_image_copy(),
                &texture.data.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(texture.data.width * 4),
                    rows_per_image: Some(texture.data.height),
                },
                wgpu::Extent3d {
                    width: texture.data.width,
                    height: texture.data.height,
                    depth_or_array_layers: 1,
                },
            );
            let view = source.create_view(&wgpu::TextureViewDescriptor::default());
            let revision = self.next_texture_revision;
            self.next_texture_revision += 1;
            self.textures.insert(
                id,
                OwnedTexture {
                    _texture: source,
                    view,
                    width: texture.data.width,
                    height: texture.data.height,
                    sha256: texture.data.sha256,
                    revision,
                },
            );
        }
    }

    pub fn observe(&mut self, input: &ObservationInput) -> Result<ObservedFrame, ObservationError> {
        let resolved = input.resolve_textures()?;
        let mut digest = Sha256::new();
        {
            let mut digest_writer = DigestWriter(&mut digest);
            write!(
                digest_writer,
                "kasane-observation-input-v1:{:?}:{:?}:{:?}",
                input.frame,
                resolved
                    .iter()
                    .map(|texture| (&texture.asset.id, &texture.data.sha256))
                    .collect::<Vec<_>>(),
                self.config,
            )
            .expect("digest writer is infallible");
        }
        let input_sha256 = format!("{:x}", digest.finalize());
        self.upload_textures(resolved);
        let mut catalog = WgpuTextureCatalog::new(
            self.textures
                .iter()
                .map(|(id, texture)| {
                    (
                        id.clone(),
                        WgpuTexture {
                            view: &texture.view,
                            width: texture.width,
                            height: texture.height,
                        },
                    )
                })
                .collect(),
        );
        for (id, texture) in &self.textures {
            catalog.set_revision(id.clone(), texture.revision);
        }
        let status = |s: kasane_core::Status| error(&s.code, s.message);
        self.renderer
            .sync_model(&self.device, &input.frame, &catalog)
            .map_err(status)?;
        let canvas = input.frame.canvas;
        let scale = self.config.fit_long_side / canvas.width.max(canvas.height);
        let offset_x = (self.config.width as f32 - canvas.width * scale) * 0.5;
        let offset_y = (self.config.height as f32 - canvas.height * scale) * 0.5;
        let drawable_bounds = input
            .frame
            .drawables
            .iter()
            .map(|drawable| {
                let visible = drawable.enabled && drawable.visible && drawable.opacity > 0.0;
                let bounds = if visible && !drawable.positions.is_empty() {
                    let mut min_x = f32::INFINITY;
                    let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for position in &drawable.positions {
                        let x = (position.x * canvas.pixels_per_unit + canvas.origin.x) * scale
                            + offset_x;
                        let y = (canvas.origin.y - position.y * canvas.pixels_per_unit) * scale
                            + offset_y;
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                    let x0 = (min_x.floor() - 2.0).clamp(0.0, self.config.width as f32) as u32;
                    let y0 = (min_y.floor() - 2.0).clamp(0.0, self.config.height as f32) as u32;
                    let x1 = (max_x.ceil() + 2.0).clamp(0.0, self.config.width as f32) as u32;
                    let y1 = (max_y.ceil() + 2.0).clamp(0.0, self.config.height as f32) as u32;
                    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
                } else {
                    None
                };
                DrawableBounds {
                    id: drawable.id.clone(),
                    visible,
                    bounds,
                }
            })
            .collect();
        let view = ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(scale, 0.0),
                b: Vec2::new(0.0, scale),
                origin: Vec2::new(offset_x, offset_y),
            },
            target_extent: Vec2::new(self.config.width as f32, self.config.height as f32),
            mask_scale: scale as f64,
        };
        self.renderer
            .update_view(&self.device, view)
            .map_err(status)?;
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sdk-observe.output"),
            size: wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sdk-observe.encode"),
            });
        self.renderer
            .encode(
                WgpuEncodeTarget {
                    device: &self.device,
                    queue: &self.queue,
                    encoder: &mut encoder,
                    output: &output_view,
                    output_mode: WgpuOutputMode::Replace,
                },
                &catalog,
            )
            .map_err(status)?;
        self.queue.submit([encoder.finish()]);
        let unpadded_row = self.config.width * 4;
        let padded_row = unpadded_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sdk-observe.readback"),
            size: u64::from(padded_row) * u64::from(self.config.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sdk-observe.copy"),
            });
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(self.config.height),
                },
            },
            wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|failure| error("GPU_POLL", failure.to_string()))?;
        receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|failure| error("GPU_READBACK", failure.to_string()))?
            .map_err(|failure| error("GPU_READBACK", failure.to_string()))?;
        let mapped = readback.get_mapped_range(..);
        let mut rgba = Vec::with_capacity((unpadded_row as usize) * (self.config.height as usize));
        for row in mapped.chunks_exact(padded_row as usize) {
            rgba.extend_from_slice(&row[..unpadded_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        let mut texture_revisions: Vec<_> = self
            .textures
            .iter()
            .map(|(id, texture)| TextureRevision {
                asset_id: id.clone(),
                sha256: texture.sha256.clone(),
                revision: texture.revision,
            })
            .collect();
        texture_revisions.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        Ok(ObservedFrame {
            input_sha256,
            width: self.config.width,
            height: self.config.height,
            rgba,
            version: input.version,
            evaluation_revision: input.evaluation_revision,
            document_id: input.document_id.clone(),
            source_revision: input.frame.source_revision,
            parameters: input
                .frame
                .parameters
                .iter()
                .map(|parameter| {
                    (
                        parameter.id.clone(),
                        parameter.requested,
                        parameter.value,
                        parameter.clamped,
                    )
                })
                .collect(),
            canvas: (
                canvas.width,
                canvas.height,
                canvas.origin.x,
                canvas.origin.y,
                canvas.pixels_per_unit,
            ),
            view_scale: scale,
            view_offset: (offset_x, offset_y),
            drawable_bounds,
            texture_revisions,
            adapter_name: self.adapter_name.clone(),
            adapter_backend: self.adapter_backend.clone(),
        })
    }
}
