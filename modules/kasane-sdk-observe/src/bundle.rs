//! Versioned, data-only scene bundle for rendering a resolved capture later.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use kasane_core::{DrawableFrame, ImageAsset};
use kasane_project::{decode_png, AssetData};
use kasane_sdk::Version;
use serde::{Deserialize, Serialize};

use crate::{
    CapturedAuthoring, ObservationError, ObservationInput, ObservationSource, ResolvedObservation,
    ResolvedTexture, MAX_CAPTURE_TEXTURE_BYTES,
};

const MANIFEST_LIMIT: u64 = 64 * 1024 * 1024;
const TEXTURE_FILE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneBundle {
    schema_version: u32,
    kind: String,
    version: [u64; 3],
    evaluation_revision: u64,
    document_id: String,
    requested: BTreeMap<String, f32>,
    source: ObservationSource,
    authoring: CapturedAuthoring,
    frame: DrawableFrame,
    textures: Vec<BundleTexture>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleTexture {
    asset: ImageAsset,
    path: String,
}

fn failure(code: &'static str, message: impl Into<String>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: None,
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ObservationError> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|e| failure("BUNDLE_IO", e.to_string()))?;
    fs::rename(temporary, path).map_err(|e| failure("BUNDLE_IO", e.to_string()))
}

fn regular_file_size(path: &Path) -> Result<u64, ObservationError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| failure("BUNDLE_IO", e.to_string()))?;
    if !metadata.file_type().is_file() {
        return Err(failure(
            "BUNDLE_FORMAT",
            "Bundle member must be a regular file",
        ));
    }
    Ok(metadata.len())
}

impl ResolvedObservation {
    /// Save an evaluated scene and its exact source PNG bytes to a new absolute
    /// directory. The output is data only and cannot continue animation state.
    pub fn save_scene(&self, directory: &Path) -> Result<(), ObservationError> {
        if !directory.is_absolute() {
            return Err(failure(
                "INVALID_BUNDLE_PATH",
                "Scene path must be absolute",
            ));
        }
        fs::create_dir(directory).map_err(|e| failure("BUNDLE_IO", e.to_string()))?;
        let textures: Vec<_> = self
            .textures
            .iter()
            .enumerate()
            .map(|(index, texture)| BundleTexture {
                asset: texture.asset.clone(),
                path: format!("texture-{index:03}.png"),
            })
            .collect();
        for (entry, texture) in textures.iter().zip(&self.textures) {
            write_atomically(&directory.join(&entry.path), &texture.data.bytes)?;
        }
        let input = &self.input;
        let bundle = SceneBundle {
            schema_version: 1,
            kind: "kasane-observe-scene".into(),
            version: [
                input.version.session_id,
                input.version.generation,
                input.version.revision,
            ],
            evaluation_revision: input.evaluation_revision,
            document_id: input.document_id.clone(),
            requested: input
                .requested
                .iter()
                .map(|(id, value)| (id.clone(), *value))
                .collect(),
            source: input.source.clone(),
            authoring: input.authoring.clone(),
            frame: input.frame.clone(),
            textures,
        };
        let content = serde_json::to_vec_pretty(&bundle)
            .map_err(|e| failure("BUNDLE_ENCODE", e.to_string()))?;
        if content.len() as u64 > MANIFEST_LIMIT {
            return Err(failure(
                "OBSERVATION_BUDGET_EXCEEDED",
                "Scene manifest exceeds 64 MiB",
            ));
        }
        write_atomically(&directory.join("scene.json"), &content)
    }

    /// Open a saved scene without an AuthoringSession or original asset files.
    /// The reader checks fixed relative paths, content hashes and size bounds
    /// before making the capture available to the renderer.
    pub fn open_scene(directory: &Path) -> Result<Self, ObservationError> {
        if !directory.is_absolute() {
            return Err(failure(
                "INVALID_BUNDLE_PATH",
                "Scene path must be absolute",
            ));
        }
        let manifest_path = directory.join("scene.json");
        let manifest_size = regular_file_size(&manifest_path)?;
        if manifest_size > MANIFEST_LIMIT {
            return Err(failure(
                "OBSERVATION_BUDGET_EXCEEDED",
                "Scene manifest exceeds 64 MiB",
            ));
        }
        let content = fs::read(manifest_path).map_err(|e| failure("BUNDLE_IO", e.to_string()))?;
        if content.len() as u64 != manifest_size {
            return Err(failure("BUNDLE_IO", "Scene manifest changed while reading"));
        }
        let bundle: SceneBundle = serde_json::from_slice(&content)
            .map_err(|e| failure("BUNDLE_FORMAT", e.to_string()))?;
        if bundle.schema_version != 1 || bundle.kind != "kasane-observe-scene" {
            return Err(failure(
                "UNSUPPORTED_BUNDLE_VERSION",
                "Unknown scene bundle version",
            ));
        }
        if bundle.textures.len() > 256 {
            return Err(failure(
                "OBSERVATION_BUDGET_EXCEEDED",
                "Too many scene textures",
            ));
        }
        let mut retained_bytes = 0u64;
        let mut seen = HashSet::new();
        let mut assets = Vec::with_capacity(bundle.textures.len());
        let mut textures = Vec::with_capacity(bundle.textures.len());
        for (index, entry) in bundle.textures.into_iter().enumerate() {
            if entry.path != format!("texture-{index:03}.png")
                || !seen.insert(entry.asset.id.clone())
            {
                return Err(failure(
                    "BUNDLE_FORMAT",
                    "Invalid texture path or duplicate asset ID",
                ));
            }
            let path = directory.join(&entry.path);
            let length = regular_file_size(&path)?;
            if length > TEXTURE_FILE_LIMIT {
                return Err(failure(
                    "OBSERVATION_BUDGET_EXCEEDED",
                    "Texture file exceeds 64 MiB",
                ));
            }
            retained_bytes = retained_bytes
                .checked_add(length)
                .and_then(|bytes| {
                    bytes.checked_add(
                        u64::from(entry.asset.width) * u64::from(entry.asset.height) * 4,
                    )
                })
                .ok_or_else(|| failure("OBSERVATION_BUDGET_EXCEEDED", "Texture size overflow"))?;
            if retained_bytes > MAX_CAPTURE_TEXTURE_BYTES {
                return Err(failure(
                    "OBSERVATION_BUDGET_EXCEEDED",
                    "Retained textures exceed 256 MiB",
                ));
            }
            let bytes = fs::read(path).map_err(|e| failure("BUNDLE_IO", e.to_string()))?;
            if bytes.len() as u64 != length {
                return Err(failure("BUNDLE_IO", "Texture changed while reading"));
            }
            let declared_size = bytes.get(16..24).and_then(|header| {
                (bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n".as_slice())
                    && bytes.get(12..16) == Some(b"IHDR".as_slice()))
                .then_some((
                    u32::from_be_bytes(header[..4].try_into().ok()?),
                    u32::from_be_bytes(header[4..].try_into().ok()?),
                ))
            });
            if declared_size != Some((entry.asset.width, entry.asset.height)) {
                return Err(failure(
                    "BUNDLE_FORMAT",
                    "Texture PNG dimensions differ from descriptor",
                ));
            }
            let data: AssetData =
                decode_png(&bytes).map_err(|e| failure("BUNDLE_FORMAT", e.message))?;
            if data.sha256 != entry.asset.sha256
                || data.width != entry.asset.width
                || data.height != entry.asset.height
            {
                return Err(failure(
                    "BUNDLE_HASH_MISMATCH",
                    "Saved texture does not match its descriptor",
                ));
            }
            assets.push(entry.asset.clone());
            textures.push(ResolvedTexture {
                asset: entry.asset,
                data,
            });
        }
        if bundle
            .frame
            .drawables
            .iter()
            .any(|drawable| !seen.contains(&drawable.texture_asset_id))
        {
            return Err(failure(
                "BUNDLE_FORMAT",
                "Frame references a missing texture",
            ));
        }
        Ok(Self {
            input: ObservationInput {
                version: Version {
                    session_id: bundle.version[0],
                    generation: bundle.version[1],
                    revision: bundle.version[2],
                },
                evaluation_revision: bundle.evaluation_revision,
                document_id: bundle.document_id,
                requested: bundle.requested.into_iter().collect(),
                source: bundle.source,
                authoring: bundle.authoring,
                frame: bundle.frame,
                assets,
                root: directory.to_path_buf(),
            },
            textures,
        })
    }
}
