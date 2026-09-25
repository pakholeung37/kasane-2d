//! Versioned, data-only scene bundle for rendering a resolved capture later.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use kasane_core::{DrawableFrame, ImageAsset};
use kasane_project::{decode_png, AssetData};
use kasane_sdk::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    CapturedAuthoring, ObservationError, ObservationInput, ObservationSource, RenderRequest,
    ResolvedObservation, ResolvedTexture, MAX_CAPTURE_TEXTURE_BYTES,
};

const MANIFEST_LIMIT: u64 = 64 * 1024 * 1024;
const TEXTURE_FILE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneBundle {
    schema_version: u32,
    kind: String,
    #[serde(default)]
    capture_id: String,
    #[serde(default)]
    scene_digest: String,
    version: [u64; 3],
    evaluation_revision: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    snapshot_clone_ns: u64,
    document_id: String,
    requested: BTreeMap<String, f32>,
    source: ObservationSource,
    authoring: CapturedAuthoring,
    frame: DrawableFrame,
    textures: Vec<BundleTexture>,
}

#[derive(PartialEq, Serialize, Deserialize)]
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

fn is_zero(value: &u64) -> bool {
    *value == 0
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

fn canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<(), ObservationError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(number) => {
            if number.is_f64() && number.as_f64() == Some(0.0) {
                output.extend_from_slice(b"0.0");
            } else {
                output.extend_from_slice(number.to_string().as_bytes());
            }
        }
        Value::String(value) => output.extend_from_slice(
            serde_json::to_string(value)
                .map_err(|e| failure("BUNDLE_ENCODE", e.to_string()))?
                .as_bytes(),
        ),
        Value::Array(items) => {
            output.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                canonical_json(item, output)?;
            }
            output.push(b']');
        }
        Value::Object(fields) => {
            output.push(b'{');
            let sorted: BTreeMap<_, _> = fields.iter().collect();
            for (index, (key, item)) in sorted.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                canonical_json(&Value::String(key.clone()), output)?;
                output.push(b':');
                canonical_json(item, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn scene_hash(bundle: &SceneBundle) -> Result<String, ObservationError> {
    let mut value =
        serde_json::to_value(bundle).map_err(|e| failure("BUNDLE_ENCODE", e.to_string()))?;
    let decoded: SceneBundle = serde_json::from_value(value.clone())
        .map_err(|e| failure("NONFINITE_SCENE", e.to_string()))?;
    if decoded != *bundle {
        return Err(failure(
            "NONFINITE_SCENE",
            "Scene has a nonfinite or non-roundtrippable value",
        ));
    }
    let fields = value.as_object_mut().expect("scene bundle is an object");
    for identity in [
        "schema_version",
        "kind",
        "capture_id",
        "scene_digest",
        "version",
        "evaluation_revision",
        "snapshot_clone_ns",
    ] {
        fields.remove(identity);
    }
    if let Some(source) = fields.get_mut("source").and_then(Value::as_object_mut) {
        // A live preview's operation ID distinguishes evidence acquisitions,
        // but does not change frozen geometry, source snapshot or textures.
        source.remove("operation");
    }
    let mut bytes = b"kasane-observe-scene-digest-v1\0".to_vec();
    canonical_json(&value, &mut bytes)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl ResolvedObservation {
    fn scene_bundle(&self) -> SceneBundle {
        let input = &self.input;
        SceneBundle {
            schema_version: 2,
            kind: "kasane-observe-scene".into(),
            capture_id: self.capture_id.clone(),
            scene_digest: self.scene_digest.clone(),
            version: [
                input.version.session_id,
                input.version.generation,
                input.version.revision,
            ],
            evaluation_revision: input.evaluation_revision,
            snapshot_clone_ns: input.snapshot_clone_ns,
            document_id: input.document_id.clone(),
            requested: input
                .requested
                .iter()
                .map(|(id, value)| (id.clone(), *value))
                .collect(),
            source: input.source.clone(),
            authoring: input.authoring.clone(),
            frame: input.frame.clone(),
            textures: self
                .textures
                .iter()
                .enumerate()
                .map(|(index, texture)| BundleTexture {
                    asset: texture.asset.clone(),
                    path: format!("texture-{index:03}.png"),
                })
                .collect(),
        }
    }

    pub(crate) fn compute_scene_digest(&self) -> Result<String, ObservationError> {
        scene_hash(&self.scene_bundle())
    }

    /// Digest of the captured scene and explicit raw context render settings.
    pub fn render_digest(&self, request: RenderRequest) -> Result<String, ObservationError> {
        request.mapping()?;
        let mut bytes = b"kasane-observe-render-digest-v1\0".to_vec();
        let request = serde_json::json!({
            "scene_digest": self.scene_digest,
            "width": request.width,
            "height": request.height,
            "roi": [request.roi.x0, request.roi.y0, request.roi.x1, request.roi.y1],
            "padding_canvas": request.padding_canvas,
            "mode": "context",
            "output": "raw_rgba8_renderer_default",
            "sample_policy": "single_evaluated_frame",
        });
        canonical_json(&request, &mut bytes)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

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
        let bundle = self.scene_bundle();
        for (entry, texture) in bundle.textures.iter().zip(self.textures.iter()) {
            write_atomically(&directory.join(&entry.path), &texture.data.bytes)?;
        }
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
        if !matches!(bundle.schema_version, 1 | 2) || bundle.kind != "kasane-observe-scene" {
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
        let declared_digest = bundle.scene_digest.clone();
        let capture_id = if bundle.schema_version == 1 {
            uuid::Uuid::new_v4().to_string()
        } else {
            uuid::Uuid::parse_str(&bundle.capture_id)
                .map_err(|_| failure("BUNDLE_FORMAT", "Invalid capture ID"))?;
            bundle.capture_id.clone()
        };
        let mut capture = Self {
            input: ObservationInput {
                version: Version {
                    session_id: bundle.version[0],
                    generation: bundle.version[1],
                    revision: bundle.version[2],
                },
                evaluation_revision: bundle.evaluation_revision,
                snapshot_clone_ns: bundle.snapshot_clone_ns,
                document_id: bundle.document_id,
                requested: bundle.requested.into_iter().collect(),
                source: bundle.source,
                authoring: bundle.authoring,
                frame: bundle.frame,
                assets,
                root: directory.to_path_buf(),
            },
            textures: textures.into(),
            capture_id,
            scene_digest: String::new(),
        };
        capture.scene_digest = capture.compute_scene_digest()?;
        if bundle.schema_version == 2 && capture.scene_digest != declared_digest {
            return Err(failure(
                "BUNDLE_HASH_MISMATCH",
                "Saved scene does not match its digest",
            ));
        }
        Ok(capture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_bundle() -> SceneBundle {
        SceneBundle {
            schema_version: 2,
            kind: "kasane-observe-scene".into(),
            capture_id: String::new(),
            scene_digest: String::new(),
            version: [1, 2, 3],
            evaluation_revision: 4,
            snapshot_clone_ns: 0,
            document_id: "document".into(),
            requested: BTreeMap::new(),
            source: ObservationSource::Parameters,
            authoring: CapturedAuthoring {
                meshes: vec![],
                parts: vec![],
                transforms: vec![],
                bindings: vec![],
                parameters: vec![],
                assets: vec![],
                draw_order_groups: vec![],
                scene_bindings: vec![],
                offscreens: vec![],
                glues: vec![],
                blend_key_tables: vec![],
                blend_constraints: vec![],
                blend_bindings: vec![],
            },
            frame: DrawableFrame::default(),
            textures: vec![],
        }
    }

    #[test]
    fn scene_hash_ignores_acquisition_identity_and_normalizes_negative_zero() {
        let mut first = empty_bundle();
        first.requested.insert("parameter".into(), -0.0);
        let mut second = empty_bundle();
        second.requested.insert("parameter".into(), 0.0);
        second.capture_id = "another-capture".into();
        second.version = [9, 8, 7];
        second.evaluation_revision = 20;
        assert_eq!(scene_hash(&first).unwrap(), scene_hash(&second).unwrap());
    }

    #[test]
    fn scene_hash_rejects_nonfinite_values() {
        let mut bundle = empty_bundle();
        bundle.requested.insert("parameter".into(), f32::NAN);
        assert_eq!(scene_hash(&bundle).unwrap_err().code, "NONFINITE_SCENE");
    }
}
