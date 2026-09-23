//! Shared real-model loading, evaluation and WGPU texture ownership for hosts.
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use kasane_core::evaluation::{evaluate_frame, DrawableFrame, RenderCommand};
use kasane_core::types::{Status, Vec2};
use kasane_project::store::DocumentSession;
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{WgpuTexture, WgpuTextureCatalog};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Deserialize)]
pub struct Case {
    pub model3: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fit_long_side: f32,
    #[serde(default)]
    pub parameters: HashMap<String, f32>,
    pub texture_profile: String,
}

impl Case {
    pub fn read(path: &Path) -> Result<Self, Box<dyn Error>> {
        let case: Self = serde_json::from_slice(&fs::read(path)?)?;
        if case.texture_profile != "linear_no_mipmap" && case.texture_profile != "linear_mipmap" {
            return Err("Unsupported texture profile".into());
        }
        if case.width == 0
            || case.height == 0
            || !case.fit_long_side.is_finite()
            || case.fit_long_side <= 0.0
            || case.parameters.values().any(|value| !value.is_finite())
        {
            return Err("Invalid output size, fit_long_side or parameter value".into());
        }
        Ok(case)
    }
}

pub fn checked<T>(result: Result<T, Status>) -> Result<T, io::Error> {
    result.map_err(|status| io::Error::other(format!("{}: {}", status.code, status.message)))
}

pub struct LoadedModel {
    pub session: DocumentSession,
    pub frame: DrawableFrame,
}

impl LoadedModel {
    pub fn new(case: &Case) -> Result<Self, Box<dyn Error>> {
        let mut session = DocumentSession::new();
        let (import, _) = session.import_model3(&case.model3);
        checked(if import.status.is_ok() {
            Ok(())
        } else {
            Err(import.status)
        })?;
        let mut preview_values = HashMap::new();
        for (runtime_id, value) in &case.parameters {
            let parameter = session
                .document()
                .parameter_order()
                .iter()
                .filter_map(|id| session.document().get_parameter(id))
                .find(|parameter| parameter.runtime_id == *runtime_id)
                .ok_or_else(|| format!("Unknown parameter runtime ID: {runtime_id}"))?;
            preview_values.insert(parameter.id.clone(), *value);
        }
        let mut frame = DrawableFrame::default();
        let evaluation = evaluate_frame(session.document(), &preview_values, &mut frame);
        checked(if evaluation.is_ok() {
            Ok(())
        } else {
            Err(evaluation)
        })?;
        Ok(Self { session, frame })
    }

    pub fn summary(&self) -> Value {
        let frame = &self.frame;
        let commands: Vec<String> = frame
            .render_plan
            .iter()
            .map(|command| match command {
                RenderCommand::BeginOffscreen { offscreen_id } => {
                    format!("begin_offscreen:{offscreen_id}")
                }
                RenderCommand::DrawMesh { mesh_id } => format!("draw_mesh:{mesh_id}"),
                RenderCommand::EndOffscreen { offscreen_id } => {
                    format!("end_offscreen:{offscreen_id}")
                }
            })
            .collect();
        let textures: BTreeSet<&str> = frame
            .drawables
            .iter()
            .map(|drawable| drawable.texture_asset_id.as_str())
            .collect();
        json!({
            "canvas": {
                "width": frame.canvas.width,
                "height": frame.canvas.height,
                "origin": [frame.canvas.origin.x, frame.canvas.origin.y],
                "pixels_per_unit": frame.canvas.pixels_per_unit,
            },
            "drawable_ids": frame.drawables.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            "offscreen_ids": frame.offscreens.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            "render_commands": commands,
            "texture_ids": textures,
            "parameters": frame.parameters.iter().map(|p| json!({"id": p.id, "value": p.value})).collect::<Vec<_>>(),
        })
    }

    pub fn view(
        &self,
        width: u32,
        height: u32,
        fit_long_side: f32,
    ) -> (ViewportConfig, f32, [f32; 2]) {
        let scale = fit_long_side / self.frame.canvas.width.max(self.frame.canvas.height);
        let offset = [
            (width as f32 - self.frame.canvas.width * scale) * 0.5,
            (height as f32 - self.frame.canvas.height * scale) * 0.5,
        ];
        (
            ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(scale, 0.0),
                    b: Vec2::new(0.0, scale),
                    origin: Vec2::new(offset[0], offset[1]),
                },
                target_extent: Vec2::new(width as f32, height as f32),
                mask_scale: scale as f64,
            },
            scale,
            offset,
        )
    }
}

pub struct TextureSet {
    sources: Vec<(String, wgpu::Texture, wgpu::TextureView, u32, u32)>,
    pub mip_hashes: HashMap<String, String>,
}

impl TextureSet {
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &LoadedModel,
        mipmaps: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let mut sources = Vec::new();
        let mut mip_hashes = HashMap::new();
        let ids: BTreeSet<&str> = model
            .frame
            .drawables
            .iter()
            .map(|drawable| drawable.texture_asset_id.as_str())
            .collect();
        for id in ids {
            let asset = checked(model.session.read_asset(id))?;
            let levels = mip_chain(asset.rgba, asset.width, asset.height, mipmaps);
            let mut hasher = Sha256::new();
            for level in &levels {
                hasher.update(level);
            }
            mip_hashes.insert(id.to_owned(), format!("{:x}", hasher.finalize()));
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("real-model.source"),
                size: wgpu::Extent3d {
                    width: asset.width,
                    height: asset.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, rgba) in levels.iter().enumerate() {
                let width = (asset.width >> level).max(1);
                let height = (asset.height >> level).max(1);
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            sources.push((id.to_owned(), texture, view, asset.width, asset.height));
        }
        Ok(Self {
            sources,
            mip_hashes,
        })
    }

    pub fn catalog(&self) -> WgpuTextureCatalog<'_> {
        let mut catalog = WgpuTextureCatalog::new(
            self.sources
                .iter()
                .map(|(id, _, view, width, height)| {
                    (
                        id.clone(),
                        WgpuTexture {
                            view,
                            width: *width,
                            height: *height,
                        },
                    )
                })
                .collect(),
        );
        for (id, _, _, _, _) in &self.sources {
            catalog.set_revision(id, 1);
        }
        catalog
    }
}

fn mip_chain(base: Vec<u8>, mut width: u32, mut height: u32, mipmaps: bool) -> Vec<Vec<u8>> {
    let mut levels = vec![base];
    while mipmaps && (width > 1 || height > 1) {
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let sample_width = if width > 1 { 2 } else { 1 };
        let sample_height = if height > 1 { 2 } else { 1 };
        let sample_count = sample_width * sample_height;
        let source = levels.last().unwrap();
        let mut next = vec![0; (next_width * next_height * 4) as usize];
        for y in 0..next_height {
            for x in 0..next_width {
                for channel in 0..4 {
                    let mut sum = 0u32;
                    for dy in 0..sample_height {
                        for dx in 0..sample_width {
                            let index = (((y * sample_height + dy) * width + x * sample_width + dx)
                                * 4
                                + channel) as usize;
                            sum += u32::from(source[index]);
                        }
                    }
                    next[((y * next_width + x) * 4 + channel) as usize] =
                        ((sum + sample_count / 2) / sample_count) as u8;
                }
            }
        }
        levels.push(next);
        width = next_width;
        height = next_height;
    }
    levels
}
