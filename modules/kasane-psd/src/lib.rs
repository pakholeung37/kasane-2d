//! Convert layered PSD artwork into a Kasane document and PNG or RGBA assets.
//!
//! This crate does no filesystem writes. For [`import_psd`], the caller must
//! publish every [`PngAsset::source`] with its bytes. [`import_psd_pixels`] leaves
//! PNG encoding and publication to the project store.

use ag_psd::psd::{BlendMode as PsdBlendMode, ColorMode, Layer, ReadOptions};
use ag_psd::{read_psd, PixelData};
use kasane_core::types::{Appearance, BlendMode, Canvas, ImageAsset, Mesh, Offscreen, Part, Vec2};
use kasane_core::Document;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::io::Cursor;

const MAX_FILE_BYTES: usize = 512 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;
const MAX_PIXELS: u64 = 64_000_000;
const MAX_LAYERS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub code: &'static str,
    pub message: String,
}

impl ImportError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ImportError {}

/// A PNG that must be written relative to the future project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PngAsset {
    pub id: String,
    pub source: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub width: u32,
    pub height: u32,
    pub raster_layers: usize,
    pub groups: usize,
    pub warnings: Vec<String>,
}

pub struct ImportBundle {
    pub document: Document,
    pub assets: Vec<PngAsset>,
    pub report: ImportReport,
}

/// Raster pixels owned by an unsaved draft. PNG encoding is left to the caller.
pub struct PixelAsset {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct PixelImportBundle {
    pub document: Document,
    pub assets: Vec<PixelAsset>,
    pub report: ImportReport,
}

/// Parse an 8-bit RGB PSD. Raster layers become cropped PNGs and rectangular
/// meshes; groups become Parts. Layers are ordered from back to front.
pub fn import_psd(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let builder = parse_psd(bytes, true)?;
    Ok(ImportBundle {
        document: builder.document,
        assets: builder.assets,
        report: builder.report,
    })
}

/// Import directly into RGBA pixels without encoding PNGs. Asset SHA-256 fields
/// remain empty until the project store encodes and publishes the PNG bytes.
pub fn import_psd_pixels(bytes: &[u8]) -> Result<PixelImportBundle, ImportError> {
    let builder = parse_psd(bytes, false)?;
    Ok(PixelImportBundle {
        document: builder.document,
        assets: builder.pixels,
        report: builder.report,
    })
}

fn parse_psd(bytes: &[u8], encode_pngs: bool) -> Result<Builder, ImportError> {
    preflight(bytes)?;
    let options = ReadOptions {
        skip_composite_image_data: Some(true),
        use_image_data: Some(true),
        total_memory_limit: Some(MAX_DECODED_BYTES),
        ..ReadOptions::default()
    };
    let mut psd = read_psd(bytes, &options)
        .map_err(|error| ImportError::new("INVALID_PSD", error.to_string()))?;
    if psd.color_mode != Some(ColorMode::Rgb) || psd.bits_per_channel != Some(8.0) {
        return Err(ImportError::new(
            "UNSUPPORTED_PSD",
            "Only 8-bit RGB PSD files are supported",
        ));
    }
    let width = dimension(psd.width, "width")?;
    let height = dimension(psd.height, "height")?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ImportError::new(
            "PSD_LIMIT",
            "Canvas exceeds 64 megapixels",
        ));
    }

    let fingerprint: [u8; 32] = Sha256::digest(bytes).into();
    let mut document = Document::new();
    let status = document.initialize(
        stable_id(&fingerprint, "document", 0),
        Canvas::new(
            width as f32,
            height as f32,
            Vec2::new(width as f32 / 2.0, height as f32 / 2.0),
            100.0,
        ),
    );
    check(status.code, status.message)?;
    let mut builder = Builder {
        document,
        assets: Vec::new(),
        pixels: Vec::new(),
        encode_pngs,
        report: ImportReport {
            width,
            height,
            raster_layers: 0,
            groups: 0,
            warnings: Vec::new(),
        },
        fingerprint,
        next_id: 0,
        seen_layers: 0,
        used_mesh_runtime_ids: HashSet::new(),
    };
    builder.visit_layers(psd.children.as_deref_mut().unwrap_or_default(), "")?;
    if builder.report.raster_layers == 0 {
        return Err(ImportError::new(
            "EMPTY_PSD",
            "PSD contains no raster layers",
        ));
    }
    if let Some(issue) = builder.document.validate_structure().into_iter().next() {
        return Err(ImportError::new(
            "INVALID_DOCUMENT",
            format!("{}: {}", issue.status.code, issue.status.message),
        ));
    }
    Ok(builder)
}

fn preflight(bytes: &[u8]) -> Result<(), ImportError> {
    if bytes.len() < 26 || &bytes[..4] != b"8BPS" || bytes[4..6] != [0, 1] {
        return Err(ImportError::new(
            "INVALID_PSD",
            "Expected a PSD version 1 header",
        ));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ImportError::new("PSD_LIMIT", "PSD exceeds 512 MiB"));
    }
    let height = u32::from_be_bytes(bytes[14..18].try_into().unwrap());
    let width = u32::from_be_bytes(bytes[18..22].try_into().unwrap());
    if width == 0
        || height == 0
        || width > 30_000
        || height > 30_000
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(ImportError::new(
            "PSD_LIMIT",
            "PSD canvas dimensions are outside the supported range",
        ));
    }
    if bytes[22..24] != [0, 8] || bytes[24..26] != [0, 3] {
        return Err(ImportError::new(
            "UNSUPPORTED_PSD",
            "Only 8-bit RGB PSD files are supported",
        ));
    }
    Ok(())
}

fn dimension(value: f64, field: &str) -> Result<u32, ImportError> {
    if !value.is_finite() || value.fract() != 0.0 || !(1.0..=30_000.0).contains(&value) {
        return Err(ImportError::new("INVALID_PSD", format!("Invalid {field}")));
    }
    Ok(value as u32)
}

fn coordinate(value: Option<f64>, field: &str) -> Result<f32, ImportError> {
    let value = value.ok_or_else(|| ImportError::new("INVALID_PSD", format!("Missing {field}")))?;
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > 1_000_000.0 {
        return Err(ImportError::new("INVALID_PSD", format!("Invalid {field}")));
    }
    Ok(value as f32)
}

fn check(code: String, message: String) -> Result<(), ImportError> {
    if code.is_empty() {
        Ok(())
    } else {
        Err(ImportError::new(
            "INVALID_DOCUMENT",
            format!("{code}: {message}"),
        ))
    }
}

fn stable_id(fingerprint: &[u8; 32], kind: &str, ordinal: usize) -> String {
    let digest = Sha256::new()
        .chain_update(fingerprint)
        .chain_update(kind.as_bytes())
        .chain_update(ordinal.to_be_bytes())
        .finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(id[0..4].try_into().unwrap()),
        u16::from_be_bytes(id[4..6].try_into().unwrap()),
        u16::from_be_bytes(id[6..8].try_into().unwrap()),
        u16::from_be_bytes(id[8..10].try_into().unwrap()),
        u64::from_be_bytes([0, 0, id[10], id[11], id[12], id[13], id[14], id[15]])
    )
}

struct Builder {
    document: Document,
    assets: Vec<PngAsset>,
    pixels: Vec<PixelAsset>,
    encode_pngs: bool,
    report: ImportReport,
    fingerprint: [u8; 32],
    next_id: usize,
    seen_layers: usize,
    used_mesh_runtime_ids: HashSet<String>,
}

impl Builder {
    fn id(&mut self, kind: &str) -> String {
        let id = stable_id(&self.fingerprint, kind, self.next_id);
        self.next_id += 1;
        id
    }

    fn mesh_runtime_id(&mut self, name: &str, mesh_id: &str) -> String {
        let valid_name = name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if valid_name && self.used_mesh_runtime_ids.insert(name.to_owned()) {
            return name.to_owned();
        }
        let mut fallback = format!("ArtMesh_{}", mesh_id.replace('-', ""));
        while !self.used_mesh_runtime_ids.insert(fallback.clone()) {
            fallback.push('_');
        }
        fallback
    }

    fn visit_layers(&mut self, layers: &mut [Layer], parent_id: &str) -> Result<(), ImportError> {
        // PSD children are ordered back to front. Isolate each raster base and
        // its consecutive clipped siblings from the surrounding artwork.
        let mut next = 0;
        while next < layers.len() {
            let (base, following) = layers[next..].split_first_mut().unwrap();
            next += 1;
            let count = following
                .iter()
                .take_while(|layer| layer.clipping == Some(true))
                .count();
            let end = next + count;
            if base.clipping != Some(true) && base.children.is_none() && end > next {
                self.visit_clipping_group(base, &mut following[..count], parent_id)?;
                next = end;
            } else {
                self.visit(base, parent_id, None)?;
            }
        }
        Ok(())
    }

    fn visit_clipping_group(
        &mut self,
        base: &mut Layer,
        clipped: &mut [Layer],
        parent_id: &str,
    ) -> Result<(), ImportError> {
        let name = base.additional_info.name.as_deref().unwrap_or("Untitled");
        if base.additional_info.blend_clippend_elements == Some(false) {
            return Err(ImportError::new(
                "UNSUPPORTED_LAYER",
                format!("{name}: clipping with Blend Clipped Layers As Group disabled cannot be represented"),
            ));
        }
        let part_id = self.id("part");
        let result = self.document.create_part(Part {
            id: part_id.clone(),
            runtime_id: format!("Part_{}", part_id.replace('-', "")),
            name: format!("{name} (Clipping)"),
            parent_id: parent_id.into(),
            draw_order: self.report.raster_layers as f32,
            ..Part::default()
        });
        check(result.status.code, result.status.message)?;
        let group_name = format!("{name} (Clipping)");
        let base_id = self
            .visit(base, &part_id, None)?
            .expect("raster clipping base produces a mesh");
        let mut mesh = self.document.get_mesh(&base_id).unwrap().clone();
        // The base's alpha and opacity seed the group. Its color blend applies
        // once, when the entire clipping group is composited onto its parent.
        let blend_mode = extended_color_mode(mesh.blend_mode);
        mesh.blend_mode = BlendMode::Normal;
        let result = self.document.replace_mesh(mesh);
        check(result.status.code, result.status.message)?;
        for layer in clipped {
            self.visit(layer, &part_id, Some(&base_id))?;
        }
        let id = self.id("offscreen");
        let result = self.document.create_offscreen(Offscreen {
            runtime_id: format!("Offscreen_{}", id.replace('-', "")),
            id,
            name: group_name,
            part_id,
            blend_mode,
            ..Offscreen::default()
        });
        check(result.status.code, result.status.message)
    }

    fn visit(
        &mut self,
        layer: &mut Layer,
        parent_id: &str,
        clipping_base: Option<&str>,
    ) -> Result<Option<String>, ImportError> {
        self.seen_layers += 1;
        if self.seen_layers > MAX_LAYERS {
            return Err(ImportError::new("PSD_LIMIT", "PSD exceeds 8192 layers"));
        }
        let name = layer.additional_info.name.as_deref().unwrap_or("Untitled");
        if layer.additional_info.mask.is_some()
            || layer.additional_info.real_mask.is_some()
            || layer.additional_info.vector_mask.is_some()
            || layer.additional_info.effects.is_some()
            || layer.additional_info.adjustment.is_some()
            || layer.additional_info.filter_mask.is_some()
            || layer.additional_info.filter_effects_masks.is_some()
            || layer
                .additional_info
                .fill_opacity
                .is_some_and(|opacity| opacity != 1.0)
        {
            return Err(ImportError::new(
                "UNSUPPORTED_LAYER",
                format!("{name}: masks, adjustments, and layer effects cannot be represented"),
            ));
        }
        if let Some(children) = &mut layer.children {
            if layer.clipping == Some(true) {
                return Err(ImportError::new(
                    "UNSUPPORTED_LAYER",
                    format!("{name}: clipped groups cannot be represented"),
                ));
            }
            if layer.opacity.is_some_and(|opacity| opacity != 1.0) {
                return Err(ImportError::new(
                    "UNSUPPORTED_LAYER",
                    format!("{name}: group opacity cannot be represented"),
                ));
            }
            if !matches!(
                layer.blend_mode,
                None | Some(PsdBlendMode::PassThrough | PsdBlendMode::Normal)
            ) {
                return Err(ImportError::new(
                    "UNSUPPORTED_LAYER",
                    format!("{name}: group blend mode cannot be represented"),
                ));
            }
            let id = self.id("part");
            let part = Part {
                id: id.clone(),
                runtime_id: format!("Part_{}", id.replace('-', "")),
                name: name.into(),
                parent_id: parent_id.into(),
                enabled: layer.hidden != Some(true),
                // Parts participate in drawing order separately from their
                // organization hierarchy. Place this group where its first
                // raster occurs, between its lower and upper siblings.
                draw_order: self.report.raster_layers as f32,
                ..Part::default()
            };
            let result = self.document.create_part(part);
            check(result.status.code, result.status.message)?;
            self.report.groups += 1;
            self.visit_layers(children, &id)?;
            return Ok(None);
        }

        if layer.clipping == Some(true) && clipping_base.is_none() {
            return Err(ImportError::new(
                "UNSUPPORTED_LAYER",
                format!("{name}: clipping requires a raster base in the same group"),
            ));
        }

        let Some(image) = layer.image_data.take().or_else(|| layer.canvas.take()) else {
            return Err(ImportError::new(
                "UNSUPPORTED_LAYER",
                format!("{name}: layer has no raster pixels"),
            ));
        };
        let left = coordinate(layer.left, "layer left")?;
        let top = coordinate(layer.top, "layer top")?;
        let right = coordinate(layer.right, "layer right")?;
        let bottom = coordinate(layer.bottom, "layer bottom")?;
        if right - left != image.width as f32
            || bottom - top != image.height as f32
            || image.width == 0
            || image.height == 0
        {
            return Err(ImportError::new(
                "INVALID_PSD",
                format!("{name}: layer bounds and pixels differ"),
            ));
        }
        let expected =
            usize::try_from(u64::from(image.width) * u64::from(image.height) * 4).unwrap();
        if image.data.len() != expected {
            return Err(ImportError::new(
                "INVALID_PSD",
                format!("{name}: invalid RGBA pixel count"),
            ));
        }
        if self.report.raster_layers >= MAX_LAYERS {
            return Err(ImportError::new(
                "PSD_LIMIT",
                "PSD exceeds 8192 raster layers",
            ));
        }
        let blend_mode = match layer.blend_mode.unwrap_or(PsdBlendMode::Normal) {
            PsdBlendMode::Normal => BlendMode::Normal,
            PsdBlendMode::Multiply => BlendMode::Multiplicative,
            PsdBlendMode::LinearDodge => BlendMode::Additive,
            other => {
                return Err(ImportError::new(
                    "UNSUPPORTED_LAYER",
                    format!("{name}: blend mode {other:?} cannot be represented"),
                ))
            }
        };
        let opacity = layer.opacity.unwrap_or(1.0);
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(ImportError::new(
                "INVALID_PSD",
                format!("{name}: invalid opacity"),
            ));
        }
        if layer.additional_info.text.is_some()
            || layer.additional_info.vector_fill.is_some()
            || layer.additional_info.vector_stroke.is_some()
            || layer.additional_info.placed_layer.is_some()
        {
            self.report
                .warnings
                .push(format!("{name}: editable source data is rasterized"));
        }
        let asset_id = self.id("asset");
        let source = format!("assets/{asset_id}.png");
        let png = self.encode_pngs.then(|| encode_png(&image)).transpose()?;
        let sha256 = png
            .as_ref()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            .unwrap_or_default();
        let result = self.document.add_asset(ImageAsset {
            id: asset_id.clone(),
            name: name.into(),
            source: source.clone(),
            width: image.width,
            height: image.height,
            sha256,
        });
        check(result.status.code, result.status.message)?;
        if let Some(bytes) = png {
            self.assets.push(PngAsset {
                id: asset_id.clone(),
                source,
                bytes,
            });
        } else {
            self.pixels.push(PixelAsset {
                id: asset_id.clone(),
                width: image.width,
                height: image.height,
                rgba: image.data,
            });
        }

        let mesh_id = self.id("mesh");
        let runtime_id = self.mesh_runtime_id(name, &mesh_id);
        let mesh = Mesh {
            id: mesh_id.clone(),
            runtime_id,
            name: name.into(),
            texture_asset_id: asset_id,
            part_id: parent_id.into(),
            vertex_ids: vec![1, 2, 3, 4],
            base_positions: vec![
                Vec2::new(left, top),
                Vec2::new(right, top),
                Vec2::new(right, bottom),
                Vec2::new(left, bottom),
            ],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
            triangles: vec![[1, 2, 3], [1, 3, 4]],
            appearance: Appearance {
                opacity: opacity as f32,
                ..Appearance::default()
            },
            draw_order: Some(self.report.raster_layers as f32),
            blend_mode,
            enabled: layer.hidden != Some(true),
            // Source-atop blends clipped color while retaining the base alpha,
            // including its opacity, without applying the texture alpha twice.
            raw_blend_mode: clipping_base.map(|_| extended_color_mode(blend_mode) | (1 << 8)),
            ..Mesh::default()
        };
        let result = self.document.create_mesh(mesh);
        check(result.status.code, result.status.message)?;
        self.report.raster_layers += 1;
        Ok(Some(mesh_id))
    }
}

fn extended_color_mode(mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Additive => 3,
        BlendMode::Multiplicative => 6,
    }
}

fn encode_png(image: &PixelData) -> Result<Vec<u8>, ImportError> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut output), image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| ImportError::new("PNG_ENCODE", error.to_string()))?;
        writer
            .write_image_data(&image.data)
            .map_err(|error| ImportError::new("PNG_ENCODE", error.to_string()))?;
    }
    Ok(output)
}
