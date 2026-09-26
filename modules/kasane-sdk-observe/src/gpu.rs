//! A reusable device and renderer for offscreen observation.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::future::Future;
use std::sync::mpsc;
use std::time::Duration;

use kasane_core::{BlendMode, DrawableFrame, Vec2};
use kasane_render::{Affine2, DiagnosticOverrides, DiagnosticPlan, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuMainBackground, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig,
    WgpuTexture, WgpuTextureCatalog,
};
use kasane_sdk::Version;
use sha2::{Digest, Sha256};

use crate::{
    ObservationError, ObservationInput, RenderRequest, ResolvedObservation, ResolvedTexture,
    ViewMapping,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverConfig {
    pub width: u32,
    pub height: u32,
    pub fit_long_side: f32,
}

/// Exact colors used to initialize the main scene target. Checker coordinates
/// are output pixels, with a stable origin independent of the scene camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresentationBackground {
    #[default]
    Transparent,
    Solid {
        rgb: [u8; 3],
    },
    Checker {
        light: [u8; 3],
        dark: [u8; 3],
        tile_px: u32,
        origin_px: (i32, i32),
    },
}

impl PresentationBackground {
    fn gpu(self) -> Result<WgpuMainBackground, ObservationError> {
        let opaque = |rgb: [u8; 3]| {
            [
                f32::from(rgb[0]) / 255.0,
                f32::from(rgb[1]) / 255.0,
                f32::from(rgb[2]) / 255.0,
                1.0,
            ]
        };
        match self {
            Self::Transparent => Ok(WgpuMainBackground::Transparent),
            Self::Solid { rgb } => Ok(WgpuMainBackground::Solid(opaque(rgb))),
            Self::Checker {
                light,
                dark,
                tile_px,
                origin_px,
            } => {
                if !(1..=4096).contains(&tile_px)
                    || origin_px.0.unsigned_abs() > 1_000_000
                    || origin_px.1.unsigned_abs() > 1_000_000
                {
                    return Err(error(
                        "INVALID_BACKGROUND",
                        "Checker tile or origin is outside its supported range",
                    ));
                }
                Ok(WgpuMainBackground::Checker {
                    light: opaque(light),
                    dark: opaque(dark),
                    tile_size: tile_px,
                    origin: origin_px,
                })
            }
        }
    }
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
    /// Present only for an explicit ROI render. The legacy observe path keeps
    /// its existing fitted-view fields and byte contract.
    pub explicit_view: Option<ViewMapping>,
    pub drawable_bounds: Vec<DrawableBounds>,
    pub texture_revisions: Vec<TextureRevision>,
    pub adapter_name: String,
    pub adapter_backend: String,
}

#[derive(Clone, Debug)]
pub struct ObservedMask {
    pub consumer_id: String,
    pub source_ids: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub origin: (f32, f32),
    pub logical_size: (f32, f32),
    pub scale: f32,
    pub inverted: bool,
    pub rgba: Vec<u8>,
}

impl ObservedMask {
    pub fn png_bytes(&self) -> Result<Vec<u8>, ObservationError> {
        let mut output = Vec::new();
        let mut encoder = png::Encoder::new(&mut output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&self.rgba))
            .map_err(|failure| error("PNG_ENCODE", failure.to_string()))?;
        Ok(output)
    }
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
    /// Read the renderer's actual raw mask target. An optional source narrows
    /// the consumer's mask inputs for an individual-source diagnostic pass.
    pub fn render_mask(
        &mut self,
        captured: &ResolvedObservation,
        request: RenderRequest,
        consumer_id: &str,
        source_id: Option<&str>,
    ) -> Result<ObservedMask, ObservationError> {
        let mut derived = captured.input().clone();
        let (source_ids, inverted) = if let Some(mesh) = derived
            .frame
            .drawables
            .iter_mut()
            .find(|mesh| mesh.id == consumer_id)
        {
            let sources = mesh.masks.clone();
            if let Some(source) = source_id {
                if !sources.iter().any(|id| id == source) {
                    return Err(error("INVALID_DIAGNOSTIC_MASK", source));
                }
                mesh.masks = vec![source.to_owned()];
            }
            (sources, mesh.inverted_mask)
        } else if let Some(group) = derived
            .frame
            .offscreens
            .iter_mut()
            .find(|group| group.id == consumer_id)
        {
            let sources = group.masks.clone();
            if let Some(source) = source_id {
                if !sources.iter().any(|id| id == source) {
                    return Err(error("INVALID_DIAGNOSTIC_MASK", source));
                }
                group.masks = vec![source.to_owned()];
            }
            (sources, group.flags & 8 != 0)
        } else {
            return Err(error("INVALID_DIAGNOSTIC_MASK", consumer_id));
        };
        if source_ids.is_empty() {
            return Err(error(
                "INVALID_DIAGNOSTIC_MASK",
                "Consumer has no mask inputs",
            ));
        }
        self.render_resolved(
            &derived,
            captured.textures(),
            Some(request),
            PresentationBackground::Transparent,
            false,
        )?;
        let attachment = self
            .renderer
            .mask_attachment(consumer_id)
            .ok_or_else(|| error("MASK_NOT_AVAILABLE", consumer_id))?;
        let rgba = read_texture_rgba(
            &self.device,
            &self.queue,
            &attachment.texture,
            attachment.width,
            attachment.height,
        )?;
        Ok(ObservedMask {
            consumer_id: consumer_id.to_owned(),
            source_ids: attachment.source_ids,
            width: attachment.width,
            height: attachment.height,
            origin: (attachment.origin.x, attachment.origin.y),
            logical_size: (attachment.logical_size.x, attachment.logical_size.y),
            scale: attachment.scale,
            inverted,
            rgba,
        })
    }
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
        let limit = device.limits().max_texture_dimension_2d;
        if config.width > limit || config.height > limit {
            return Err(error(
                "OUTPUT_SIZE_LIMIT",
                format!("Output size exceeds the device limit {limit}x{limit}"),
            ));
        }
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

    fn upload_textures(&mut self, resolved: &[ResolvedTexture]) -> Result<(), ObservationError> {
        let limit = self.device.limits().max_texture_dimension_2d;
        for texture in resolved {
            let width = texture.data.width;
            let height = texture.data.height;
            if width == 0 || height == 0 || width > limit || height > limit {
                return Err(ObservationError {
                    code: "TEXTURE_SIZE_LIMIT".into(),
                    message: format!(
                        "Texture {width}x{height} exceeds the device limit {limit}x{limit}"
                    ),
                    asset_id: Some(texture.asset.id.clone()),
                });
            }
        }
        let required: HashSet<_> = resolved
            .iter()
            .map(|texture| texture.asset.id.as_str())
            .collect();
        self.textures.retain(|id, _| required.contains(id.as_str()));
        for texture in resolved {
            let id = texture.asset.id.clone();
            if self.textures.get(&id).is_some_and(|cached| {
                cached.sha256 == texture.data.sha256
                    && cached.width == texture.data.width
                    && cached.height == texture.data.height
            }) {
                continue;
            }
            let mipmaps = if cfg!(feature = "framework-texture-filtering") {
                straight_rgba_mipmaps(texture.data.width, texture.data.height, &texture.data.rgba)
            } else {
                Vec::new()
            };
            let source = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sdk-observe.source"),
                size: wgpu::Extent3d {
                    width: texture.data.width,
                    height: texture.data.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1 + mipmaps.len() as u32,
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
            for (index, (width, height, rgba)) in mipmaps.iter().enumerate() {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &source,
                        mip_level: 1 + index as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(*height),
                    },
                    wgpu::Extent3d {
                        width: *width,
                        height: *height,
                        depth_or_array_layers: 1,
                    },
                );
            }
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
                    sha256: texture.data.sha256.clone(),
                    revision,
                },
            );
        }
        Ok(())
    }

    pub fn observe(&mut self, input: &ObservationInput) -> Result<ObservedFrame, ObservationError> {
        let resolved = input.resolve_textures()?;
        self.render_resolved(
            input,
            &resolved,
            None,
            PresentationBackground::Transparent,
            false,
        )
    }

    /// Rerender a previously resolved scene at a source-canvas ROI without
    /// reading the live session or source texture paths.
    pub fn render(
        &mut self,
        captured: &ResolvedObservation,
        request: RenderRequest,
    ) -> Result<ObservedFrame, ObservationError> {
        self.render_resolved(
            captured.input(),
            captured.textures(),
            Some(request),
            PresentationBackground::Transparent,
            false,
        )
    }

    /// Render presentation pixels with the background in the actual main
    /// target. Straight alpha is available only for transparent normal blend.
    pub fn render_presentation(
        &mut self,
        captured: &ResolvedObservation,
        request: RenderRequest,
        background: PresentationBackground,
        straight_alpha: bool,
    ) -> Result<ObservedFrame, ObservationError> {
        background.gpu()?;
        if straight_alpha {
            if background != PresentationBackground::Transparent {
                return Err(error(
                    "INVALID_PRESENTATION",
                    "Straight alpha requires a transparent background",
                ));
            }
            if !supports_straight_alpha(captured.input().frame()) {
                return Err(error(
                    "UNREPRESENTABLE_TRANSPARENT_OUTPUT",
                    "Special blend modes cannot be faithfully encoded as straight-alpha PNG",
                ));
            }
        }
        self.render_resolved(
            captured.input(),
            captured.textures(),
            Some(request),
            background,
            straight_alpha,
        )
    }

    /// Render selected color draws through their original target path. Raw
    /// mask sources remain available even when their color draws are hidden.
    pub fn render_isolated(
        &mut self,
        captured: &ResolvedObservation,
        request: RenderRequest,
        background: PresentationBackground,
        straight_alpha: bool,
        focus: &[String],
        overrides: DiagnosticOverrides,
    ) -> Result<(ObservedFrame, DiagnosticPlan), ObservationError> {
        if overrides.include_disabled && !captured.input().hidden_geometry_captured() {
            return Err(error(
                "CAPTURE_NOT_AVAILABLE",
                "X-ray of disabled objects requires hidden geometry capture",
            ));
        }
        let plan = DiagnosticPlan::isolated(captured.input().frame(), focus)
            .map_err(|status| error(&status.code, status.message))?;
        let mut derived = captured.input().clone();
        derived.frame = plan.apply_with_overrides(&derived.frame, overrides);
        if straight_alpha && !supports_straight_alpha(&derived.frame) {
            return Err(error(
                "UNREPRESENTABLE_TRANSPARENT_OUTPUT",
                "Special blend modes cannot be faithfully encoded as straight-alpha PNG",
            ));
        }
        let frame = self.render_resolved(
            &derived,
            captured.textures(),
            Some(request),
            background,
            straight_alpha,
        )?;
        Ok((frame, plan))
    }

    fn render_resolved(
        &mut self,
        input: &ObservationInput,
        resolved: &[ResolvedTexture],
        request: Option<RenderRequest>,
        background: PresentationBackground,
        straight_alpha: bool,
    ) -> Result<ObservedFrame, ObservationError> {
        let explicit_view = request.map(RenderRequest::mapping).transpose()?;
        let (width, height) = request.map_or((self.config.width, self.config.height), |view| {
            (view.width, view.height)
        });
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit || request.is_some_and(|_| width > 4096 || height > 4096)
        {
            return Err(error(
                "OUTPUT_SIZE_LIMIT",
                format!("Output size exceeds the inspection or device limit {limit}x{limit}"),
            ));
        }
        let mut digest = Sha256::new();
        {
            let mut digest_writer = DigestWriter(&mut digest);
            let texture_hashes = resolved
                .iter()
                .map(|texture| (&texture.asset.id, &texture.data.sha256))
                .collect::<Vec<_>>();
            if request.is_some() {
                write!(
                    digest_writer,
                    "kasane-observation-explicit-v1:{:?}:{:?}",
                    input.frame, texture_hashes,
                )
                .expect("digest writer is infallible");
            } else {
                write!(
                    digest_writer,
                    "kasane-observation-input-v1:{:?}:{:?}:{:?}",
                    input.frame, texture_hashes, self.config,
                )
                .expect("digest writer is infallible");
            }
        }
        if let Some(request) = request {
            digest.update(b"explicit-roi-v1");
            digest.update(request.width.to_le_bytes());
            digest.update(request.height.to_le_bytes());
            for value in [
                request.roi.x0,
                request.roi.y0,
                request.roi.x1,
                request.roi.y1,
                request.padding_canvas,
            ] {
                digest.update(value.to_bits().to_le_bytes());
            }
        }
        if cfg!(feature = "framework-texture-filtering") {
            digest.update(b"source-texture:linear-mipmap-linear-repeat-area-v2");
        }
        if background != PresentationBackground::Transparent || straight_alpha {
            digest.update(
                format!("presentation:{background:?}:straight={straight_alpha}").as_bytes(),
            );
        }
        let input_sha256 = format!("{:x}", digest.finalize());
        self.upload_textures(resolved)?;
        self.renderer
            .resize(
                &self.device,
                WgpuTargetConfig {
                    width,
                    height,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            )
            .map_err(|status| error(&status.code, status.message))?;
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
            catalog.set_repeat(id.clone(), cfg!(feature = "framework-texture-filtering"));
        }
        let status = |s: kasane_core::Status| error(&s.code, s.message);
        self.renderer
            .sync_model(&self.device, &input.frame, &catalog)
            .map_err(status)?;
        let canvas = input.frame.canvas;
        let (scale, offset_x, offset_y) = if let Some(view) = explicit_view {
            (view.scale, view.offset.0, view.offset.1)
        } else {
            let scale = self.config.fit_long_side / canvas.width.max(canvas.height);
            (
                scale,
                (width as f32 - canvas.width * scale) * 0.5,
                (height as f32 - canvas.height * scale) * 0.5,
            )
        };
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
                    let x0 = (min_x.floor() - 2.0).clamp(0.0, width as f32) as u32;
                    let y0 = (min_y.floor() - 2.0).clamp(0.0, height as f32) as u32;
                    let x1 = (max_x.ceil() + 2.0).clamp(0.0, width as f32) as u32;
                    let y1 = (max_y.ceil() + 2.0).clamp(0.0, height as f32) as u32;
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
        let rgba = render_sample(
            &mut self.renderer,
            &self.device,
            &self.queue,
            &catalog,
            width,
            height,
            scale,
            offset_x,
            offset_y,
            background.gpu()?,
        )?;
        let rgba = if straight_alpha {
            unpremultiply_rgba(rgba)
        } else {
            rgba
        };
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
            width,
            height,
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
            explicit_view,
            drawable_bounds,
            texture_revisions,
            adapter_name: self.adapter_name.clone(),
            adapter_backend: self.adapter_backend.clone(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn render_sample(
    renderer: &mut WgpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &WgpuTextureCatalog<'_>,
    width: u32,
    height: u32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    background: WgpuMainBackground,
) -> Result<Vec<u8>, ObservationError> {
    let view = ViewportConfig {
        transform: Affine2 {
            a: Vec2::new(scale, 0.0),
            b: Vec2::new(0.0, scale),
            origin: Vec2::new(offset_x, offset_y),
        },
        target_extent: Vec2::new(width as f32, height as f32),
        mask_scale: scale as f64,
    };
    let status = |s: kasane_core::Status| error(&s.code, s.message);
    renderer.update_view(device, view).map_err(status)?;
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sdk-observe.output"),
        size: wgpu::Extent3d {
            width,
            height,
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
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("sdk-observe.encode"),
    });
    renderer
        .encode_with_background(
            WgpuEncodeTarget {
                device,
                queue,
                encoder: &mut encoder,
                output: &output_view,
                output_mode: WgpuOutputMode::Replace,
            },
            catalog,
            background,
        )
        .map_err(status)?;
    queue.submit([encoder.finish()]);
    read_texture_rgba(device, queue, &output, width, height)
}

fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ObservationError> {
    let unpadded_row = width * 4;
    let padded_row = unpadded_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sdk-observe.readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("sdk-observe.copy"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
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
    let mut rgba = Vec::with_capacity(unpadded_row as usize * height as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        rgba.extend_from_slice(&row[..unpadded_row as usize]);
    }
    drop(mapped);
    readback.unmap();
    Ok(rgba)
}

fn unpremultiply_rgba(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    pixels
}

fn supports_straight_alpha(frame: &DrawableFrame) -> bool {
    frame
        .drawables
        .iter()
        .all(|item| item.raw_blend_mode.is_none() && item.blend_mode == BlendMode::Normal)
        && frame.offscreens.iter().all(|item| item.blend_mode == 0)
}

/// Generate straight-RGBA box-filtered mip levels from the uploaded PNG.
/// OpenGL's glGenerateMipmap is implementation-defined, so this matches its
/// filtering setup without claiming byte-identical mip texels.
fn straight_rgba_mipmaps(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let (mut source_width, mut source_height) = (width, height);
    while source_width > 1 || source_height > 1 {
        let source = levels
            .last()
            .map_or(rgba, |(_, _, pixels)| pixels.as_slice());
        let next_width = (source_width / 2).max(1);
        let next_height = (source_height / 2).max(1);
        let mut next = vec![0u8; next_width as usize * next_height as usize * 4];
        for y in 0..next_height {
            for x in 0..next_width {
                // Integrate the entire source footprint, including fractional
                // edge texels for odd dimensions. A fixed 2x2 kernel drops the
                // final row/column of every non-power-of-two mip level.
                let x0 = x as f64 * source_width as f64 / next_width as f64;
                let x1 = (x + 1) as f64 * source_width as f64 / next_width as f64;
                let y0 = y as f64 * source_height as f64 / next_height as f64;
                let y1 = (y + 1) as f64 * source_height as f64 / next_height as f64;
                let mut sums = [0.0; 4];
                for sy in y0.floor() as u32..y1.ceil() as u32 {
                    for sx in x0.floor() as u32..x1.ceil() as u32 {
                        let weight = (x1.min((sx + 1) as f64) - x0.max(sx as f64))
                            * (y1.min((sy + 1) as f64) - y0.max(sy as f64));
                        let offset = (sy as usize * source_width as usize + sx as usize) * 4;
                        for channel in 0..4 {
                            sums[channel] += f64::from(source[offset + channel]) * weight;
                        }
                    }
                }
                let area = (x1 - x0) * (y1 - y0);
                let offset = (y as usize * next_width as usize + x as usize) * 4;
                for channel in 0..4 {
                    next[offset + channel] = (sums[channel] / area).round() as u8;
                }
            }
        }
        levels.push((next_width, next_height, next));
        (source_width, source_height) = (next_width, next_height);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::{straight_rgba_mipmaps, supports_straight_alpha, unpremultiply_rgba};
    use kasane_core::evaluation::OffscreenFrame;
    use kasane_core::{BlendMode, Drawable, DrawableFrame};

    #[test]
    fn straight_alpha_rejects_every_special_blend_and_zeros_invisible_rgb() {
        let mut frame = DrawableFrame {
            drawables: vec![Drawable::default()],
            ..Default::default()
        };
        assert!(supports_straight_alpha(&frame));
        frame.drawables[0].blend_mode = BlendMode::Additive;
        assert!(!supports_straight_alpha(&frame));
        frame.drawables[0].blend_mode = BlendMode::Multiplicative;
        assert!(!supports_straight_alpha(&frame));
        frame.drawables[0].blend_mode = BlendMode::Normal;
        frame.drawables[0].raw_blend_mode = Some(3);
        assert!(!supports_straight_alpha(&frame));
        frame.drawables[0].raw_blend_mode = None;
        frame.offscreens.push(OffscreenFrame {
            blend_mode: 4,
            ..Default::default()
        });
        assert!(!supports_straight_alpha(&frame));
        assert_eq!(
            unpremultiply_rgba(vec![90, 30, 50, 0, 64, 32, 16, 128]),
            vec![0, 0, 0, 0, 128, 64, 32, 128]
        );
    }

    #[test]
    fn mipmaps_include_odd_edges_and_single_pixel_axes() {
        for (width, height) in [(3, 1), (1, 3), (3, 3), (5, 1)] {
            let mut rgba = vec![0; width * height * 4];
            rgba[(width * height - 1) * 4..].fill(255);
            let levels = straight_rgba_mipmaps(width as u32, height as u32, &rgba);
            let last = levels.last().unwrap();
            assert_eq!((last.0, last.1), (1, 1));
            assert_eq!(
                last.2,
                vec![(255.0 / (width * height) as f64).round() as u8; 4]
            );
        }
    }
}
