//! Export the default pose of a MOC3 model to a layered, 8-bit RGB PSD.
//!
//! A MOC3 contains geometry and UVs, but no pixels. Supply the texture PNGs
//! through a model3.json file or an explicit texture-slot map.

mod psd;
mod raster;

use kasane_core::evaluation::{evaluate_frame_including_hidden, DrawableFrame, PreviewValues};
use kasane_core::image::{decode_png, DecodedImage};
use kasane_moc3::{import_from_bare_moc3_file, import_from_model3_file, ImportResult};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub use raster::Layer;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Model(String),
    InvalidInput(String),
    PsdLimit(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Model(e) | Self::InvalidInput(e) | Self::PsdLimit(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Information about the PSD. Unsupported live effects are listed explicitly.
#[derive(Debug, Clone)]
pub struct ExportReport {
    pub width: u32,
    pub height: u32,
    pub layers: usize,
    pub warnings: Vec<String>,
}

/// Convert a model3.json and its referenced PNG textures into PSD bytes.
pub fn from_model3_file(path: &Path) -> Result<(Vec<u8>, ExportReport), Error> {
    let imported = import_from_model3_file(path).map_err(|e| Error::Model(e.message))?;
    convert(imported)
}

/// Convert a bare MOC3 with an explicit map from texture slot to PNG path.
pub fn from_moc3_file(
    path: &Path,
    textures: &HashMap<usize, PathBuf>,
) -> Result<(Vec<u8>, ExportReport), Error> {
    let imported =
        import_from_bare_moc3_file(path, textures).map_err(|e| Error::Model(e.message))?;
    convert(imported)
}

/// Write a model3.json conversion directly to disk.
pub fn write_model3_psd(model3: &Path, output: &Path) -> Result<ExportReport, Error> {
    let (bytes, report) = from_model3_file(model3)?;
    fs::write(output, bytes)?;
    Ok(report)
}

/// Write a bare MOC3 conversion directly to disk.
pub fn write_moc3_psd(
    moc3: &Path,
    textures: &HashMap<usize, PathBuf>,
    output: &Path,
) -> Result<ExportReport, Error> {
    let (bytes, report) = from_moc3_file(moc3, textures)?;
    fs::write(output, bytes)?;
    Ok(report)
}

fn convert(imported: ImportResult) -> Result<(Vec<u8>, ExportReport), Error> {
    if !imported.textures_complete {
        return Err(Error::InvalidInput(format!(
            "all referenced texture PNGs are required: {}",
            imported
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    let canvas = imported.document.canvas();
    if !canvas.width.is_finite() || !canvas.height.is_finite() {
        return Err(Error::InvalidInput("invalid canvas size".into()));
    }
    let width = canvas.width.round() as u32;
    let height = canvas.height.round() as u32;
    if width == 0 || height == 0 || width > 30_000 || height > 30_000 {
        return Err(Error::PsdLimit(
            "PSD dimensions must be 1..=30000 pixels".into(),
        ));
    }
    if u64::from(width) * u64::from(height) > 64_000_000 {
        return Err(Error::PsdLimit(
            "canvas exceeds the 64 megapixel export limit".into(),
        ));
    }

    let mut frame = DrawableFrame::default();
    let status =
        evaluate_frame_including_hidden(&imported.document, &PreviewValues::new(), &mut frame);
    if !status.is_ok() {
        return Err(Error::Model(status.message));
    }
    let mut textures = HashMap::<String, DecodedImage>::new();
    let mut warnings = Vec::new();
    for asset_id in imported
        .document
        .mesh_order()
        .iter()
        .filter_map(|id| imported.document.get_mesh(id))
        .map(|mesh| &mesh.texture_asset_id)
    {
        if textures.contains_key(asset_id) {
            continue;
        }
        let asset = imported
            .document
            .get_asset(asset_id)
            .ok_or_else(|| Error::Model(format!("missing texture asset {asset_id}")))?;
        let data = fs::read(&asset.source)?;
        let image = decode_png(&data)
            .map_err(|e| Error::InvalidInput(format!("{}: {}", asset.source, e.message)))?;
        textures.insert(asset_id.clone(), image);
    }

    let mut drawables: Vec<_> = frame.drawables.iter().collect();
    drawables.sort_by_key(|d| d.render_order);
    if drawables.len() > i16::MAX as usize {
        return Err(Error::PsdLimit("too many PSD layers".into()));
    }
    if drawables.iter().any(|d| !d.masks.is_empty()) {
        warnings.push("ArtMesh masks are not represented in the PSD".into());
    }
    if !frame.offscreens.is_empty() {
        warnings.push("offscreen effects are not represented in the PSD".into());
    }
    if drawables.iter().any(|d| d.raw_blend_mode.is_some()) {
        warnings.push("extended blend modes are approximated as normal layers".into());
    }
    let layers: Vec<Layer> = drawables
        .into_iter()
        .filter_map(|d| {
            raster::rasterize(d, textures.get(&d.texture_asset_id)?, canvas, width, height)
        })
        .collect();
    let layer_count = layers.len();
    let bytes = psd::encode(width, height, layers)?;
    let report = ExportReport {
        width,
        height,
        layers: layer_count,
        warnings,
    };
    Ok((bytes, report))
}
