//! Immutable input capture for rendering an authoring session.
//!
//! Capture borrows the session briefly. Resource IO happens later, from the
//! captured descriptors, so an observer never holds an editing lock while
//! decoding images or submitting GPU work.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use kasane_animation::{MotionOperation, MotionPreview, MotionSnapshot};
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DrawableFrame, EvaluationTrace,
    Glue, ImageAsset, Mesh, MeshBinding, Offscreen, Parameter, Part, PreviewValues, SceneBinding,
    Transform,
};
use kasane_project::{read_project_asset, AssetData};
use kasane_sdk::{AuthoringSession, AuthoringSnapshot, Version};

mod bundle;
mod gpu;
mod view;
pub use gpu::{
    DrawableBounds, ObservedFrame, Observer, ObserverConfig, PresentationBackground,
    TextureRevision,
};
pub use view::{CanvasRoi, RenderRequest, ViewMapping};

#[derive(Clone, Debug)]
pub struct ObservationInput {
    version: Version,
    evaluation_revision: u64,
    snapshot_clone_ns: u64,
    document_id: String,
    requested: PreviewValues,
    frame: DrawableFrame,
    trace: Option<EvaluationTrace>,
    assets: Vec<ImageAsset>,
    root: PathBuf,
    source: ObservationSource,
    authoring: CapturedAuthoring,
}

/// Object identities and source topology from the same document snapshot as
/// the evaluated frame. Interpolation provenance is added by a later stage.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CapturedAuthoring {
    pub meshes: Vec<Mesh>,
    pub parts: Vec<Part>,
    pub transforms: Vec<Transform>,
    pub bindings: Vec<MeshBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ImageAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draw_order_groups: Vec<DrawOrderGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_bindings: Vec<SceneBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offscreens: Vec<Offscreen>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glues: Vec<Glue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blend_key_tables: Vec<BlendShapeKeyTable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blend_constraints: Vec<BlendShapeConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blend_bindings: Vec<BlendShapeBinding>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
pub enum ObservationSource {
    Parameters,
    Animation {
        snapshot: Box<MotionSnapshot>,
        apply_model_opacity: bool,
        /// Live preview captures do not contain a complete update journal.
        history_status: HistoryStatus,
        /// Last successful operation; not a complete replay recipe.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation: Option<MotionOperation>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStatus {
    NotRecorded,
}

#[derive(Clone, Debug)]
pub struct ResolvedTexture {
    pub asset: ImageAsset,
    pub data: AssetData,
}

/// A captured evaluated frame and its decoded textures. Rendering this value
/// never revisits the source project or its assets.
#[derive(Clone, Debug)]
pub struct ResolvedObservation {
    input: ObservationInput,
    textures: Arc<[ResolvedTexture]>,
    capture_id: String,
    scene_digest: String,
}

pub(crate) const MAX_CAPTURE_TEXTURE_BYTES: u64 = 256 * 1024 * 1024;

impl ResolvedObservation {
    pub fn capture(input: ObservationInput) -> Result<Self, ObservationError> {
        Ok(Self::capture_many(vec![input])?.remove(0))
    }

    /// Resolve the union of sample assets once and share their decoded bytes.
    /// All inputs must come from the same authoring snapshot.
    pub fn capture_many(inputs: Vec<ObservationInput>) -> Result<Vec<Self>, ObservationError> {
        if inputs.is_empty() || inputs.len() > 64 {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Sample count must be 1..64".into(),
                asset_id: None,
            });
        }
        let first = &inputs[0];
        let mut assets = BTreeMap::<String, ImageAsset>::new();
        for input in &inputs {
            if input.version != first.version
                || input.document_id != first.document_id
                || input.evaluation_revision != first.evaluation_revision
                || input.root != first.root
                || input.authoring != first.authoring
            {
                return Err(ObservationError {
                    code: "MIXED_DOCUMENT_SNAPSHOTS".into(),
                    message: "Samples must use one authoring snapshot".into(),
                    asset_id: None,
                });
            }
            for asset in &input.assets {
                if let Some(existing) = assets.insert(asset.id.clone(), asset.clone()) {
                    if existing != *asset {
                        return Err(ObservationError {
                            code: "MIXED_DOCUMENT_SNAPSHOTS".into(),
                            message: "Asset descriptor changed between samples".into(),
                            asset_id: Some(asset.id.clone()),
                        });
                    }
                }
            }
        }
        let expected_rgba = assets.values().try_fold(0u64, |total, asset| {
            total.checked_add(
                u64::from(asset.width)
                    .checked_mul(u64::from(asset.height))?
                    .checked_mul(4)?,
            )
        });
        if expected_rgba.is_none_or(|bytes| bytes > MAX_CAPTURE_TEXTURE_BYTES) {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Captured texture RGBA exceeds 256 MiB".into(),
                asset_id: None,
            });
        }
        let textures: Vec<ResolvedTexture> = assets
            .into_values()
            .map(|asset| {
                let data =
                    read_project_asset(&first.root, &asset).map_err(|status| ObservationError {
                        code: status.code,
                        message: status.message,
                        asset_id: Some(asset.id.clone()),
                    })?;
                Ok(ResolvedTexture { asset, data })
            })
            .collect::<Result<_, _>>()?;
        let retained = textures.iter().try_fold(0u64, |total, texture| {
            total
                .checked_add(texture.data.bytes.len() as u64)?
                .checked_add(texture.data.rgba.len() as u64)
        });
        if retained.is_none_or(|bytes| bytes > MAX_CAPTURE_TEXTURE_BYTES) {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Captured texture bytes exceed 256 MiB".into(),
                asset_id: None,
            });
        }
        let shared: Arc<[ResolvedTexture]> = textures.into();
        let capture_id = uuid::Uuid::new_v4().to_string();
        inputs
            .into_iter()
            .map(|input| {
                let mut capture = Self {
                    input,
                    textures: Arc::clone(&shared),
                    capture_id: capture_id.clone(),
                    scene_digest: String::new(),
                };
                capture.scene_digest = capture.compute_scene_digest()?;
                Ok(capture)
            })
            .collect()
    }

    pub fn input(&self) -> &ObservationInput {
        &self.input
    }

    pub fn textures(&self) -> &[ResolvedTexture] {
        &self.textures
    }

    pub fn capture_id(&self) -> &str {
        &self.capture_id
    }

    pub fn scene_digest(&self) -> &str {
        &self.scene_digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub message: String,
    pub asset_id: Option<String>,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ObservationError {}

impl ObservationInput {
    /// Resolve IDs or unique display names against the detached document.
    pub fn resolve_requested(
        snapshot: &AuthoringSnapshot,
        values: &PreviewValues,
    ) -> Result<PreviewValues, ObservationError> {
        let document = snapshot.document();
        let mut resolved = PreviewValues::new();
        for (name_or_id, value) in values {
            let id = if document.get_parameter(name_or_id).is_some() {
                name_or_id.clone()
            } else {
                let mut matches = document.parameter_order().iter().filter(|id| {
                    document
                        .get_parameter(id)
                        .is_some_and(|p| p.name == *name_or_id)
                });
                match (matches.next(), matches.next()) {
                    (Some(id), None) => id.clone(),
                    (Some(_), Some(_)) => {
                        return Err(ObservationError {
                            code: "AMBIGUOUS_PARAMETER_NAME".into(),
                            message: format!("Parameter name {name_or_id:?} is ambiguous"),
                            asset_id: None,
                        });
                    }
                    _ => {
                        return Err(ObservationError {
                            code: "UNKNOWN_PARAMETER".into(),
                            message: format!("Unknown parameter {name_or_id:?}"),
                            asset_id: None,
                        });
                    }
                }
            };
            if resolved.insert(id.clone(), *value).is_some() {
                return Err(ObservationError {
                    code: "DUPLICATE_PARAMETER".into(),
                    message: format!("Parameter {id:?} was supplied twice"),
                    asset_id: None,
                });
            }
        }
        Ok(resolved)
    }

    pub fn version(&self) -> Version {
        self.version
    }
    pub fn evaluation_revision(&self) -> u64 {
        self.evaluation_revision
    }
    pub fn snapshot_clone_ns(&self) -> u64 {
        self.snapshot_clone_ns
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn requested(&self) -> &PreviewValues {
        &self.requested
    }
    pub fn frame(&self) -> &DrawableFrame {
        &self.frame
    }
    pub fn trace(&self) -> Option<&EvaluationTrace> {
        self.trace.as_ref()
    }
    pub fn assets(&self) -> &[ImageAsset] {
        &self.assets
    }
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn source(&self) -> &ObservationSource {
        &self.source
    }
    pub fn authoring(&self) -> &CapturedAuthoring {
        &self.authoring
    }

    pub fn capture(
        session: &AuthoringSession,
        requested: &PreviewValues,
    ) -> Result<Self, ObservationError> {
        Self::capture_from_snapshot(&session.read_snapshot(), requested)
    }

    /// Evaluate a detached authoring snapshot; no live session is consulted.
    pub fn capture_from_snapshot(
        snapshot: &AuthoringSnapshot,
        requested: &PreviewValues,
    ) -> Result<Self, ObservationError> {
        Self::capture_from_snapshot_with_trace(snapshot, requested, false)
    }

    pub fn capture_from_snapshot_with_trace(
        snapshot: &AuthoringSnapshot,
        requested: &PreviewValues,
        with_trace: bool,
    ) -> Result<Self, ObservationError> {
        let (frame, trace) = (if with_trace {
            snapshot
                .evaluate_with_trace(requested)
                .map(|(frame, trace)| (frame, Some(trace)))
        } else {
            snapshot.evaluate(requested).map(|frame| (frame, None))
        })
        .map_err(|error| ObservationError {
            code: error.code.into(),
            message: error.message.into(),
            asset_id: error.object_ids.first().cloned(),
        })?;
        Self::capture_frame(
            snapshot,
            requested.clone(),
            frame,
            trace,
            ObservationSource::Parameters,
        )
    }

    /// Capture up to 64 parameter samples against one detached document.
    pub fn capture_samples(
        snapshot: &AuthoringSnapshot,
        samples: &[PreviewValues],
    ) -> Result<Vec<Self>, ObservationError> {
        Self::capture_samples_with_trace(snapshot, samples, false)
    }

    pub fn capture_samples_with_trace(
        snapshot: &AuthoringSnapshot,
        samples: &[PreviewValues],
        with_trace: bool,
    ) -> Result<Vec<Self>, ObservationError> {
        if samples.is_empty() || samples.len() > 64 {
            return Err(ObservationError {
                code: "OBSERVATION_BUDGET_EXCEEDED".into(),
                message: "Sample count must be 1..64".into(),
                asset_id: None,
            });
        }
        samples
            .iter()
            .map(|values| Self::capture_from_snapshot_with_trace(snapshot, values, with_trace))
            .collect()
    }

    /// Capture a previously evaluated Motion/Expression/Physics/Pose frame.
    /// The preview must still describe this session's current document.
    pub fn capture_motion(
        session: &AuthoringSession,
        preview: &MotionPreview,
    ) -> Result<Self, ObservationError> {
        Self::capture_motion_with_renderer_opacity(session, preview, false)
    }

    /// Capture an animation frame and optionally apply its Model opacity as
    /// Framework renderer color alpha at drawable draws. Offscreen opacity
    /// remains independent of this host color input.
    /// Framework does not apply Model opacity to its renderer automatically.
    pub fn capture_motion_with_renderer_opacity(
        session: &AuthoringSession,
        preview: &MotionPreview,
        apply_model_opacity: bool,
    ) -> Result<Self, ObservationError> {
        Self::capture_motion_from_snapshot(&session.read_snapshot(), preview, apply_model_opacity)
    }

    /// Capture animation against the exact document snapshot checked for identity.
    pub fn capture_motion_from_snapshot(
        snapshot: &AuthoringSnapshot,
        preview: &MotionPreview,
        apply_model_opacity: bool,
    ) -> Result<Self, ObservationError> {
        Self::capture_motion_from_snapshot_with_trace(snapshot, preview, apply_model_opacity, false)
    }

    pub fn capture_motion_from_snapshot_with_trace(
        snapshot: &AuthoringSnapshot,
        preview: &MotionPreview,
        apply_model_opacity: bool,
        with_trace: bool,
    ) -> Result<Self, ObservationError> {
        let version = snapshot.version();
        if preview.source_identity() != Some((version.session_id, version.generation))
            || preview.document_revision() != version.revision
            || preview.document_id() != snapshot.document().id()
        {
            return Err(ObservationError {
                code: "STALE_ANIMATION_PREVIEW".into(),
                message: "Animation preview was captured from another document revision".into(),
                asset_id: None,
            });
        }
        let requested: PreviewValues = preview
            .snapshot()
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let (mut frame, trace) = (if with_trace {
            preview
                .evaluate_drawables_with_trace()
                .map(|(frame, trace)| (frame, Some(trace)))
        } else {
            preview.evaluate_drawables().map(|frame| (frame, None))
        })
        .map_err(|error| ObservationError {
            code: "ANIMATION_EVALUATION".into(),
            message: error.to_string(),
            asset_id: None,
        })?;
        if apply_model_opacity {
            let opacity = preview.snapshot().model_opacity;
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err(ObservationError {
                    code: "INVALID_MODEL_OPACITY".into(),
                    message: "Model opacity must be finite and between zero and one".into(),
                    asset_id: None,
                });
            }
            for drawable in &mut frame.drawables {
                drawable.opacity *= opacity;
            }
        }
        Self::capture_frame(
            snapshot,
            requested,
            frame,
            trace,
            ObservationSource::Animation {
                snapshot: Box::new(preview.snapshot().clone()),
                apply_model_opacity,
                history_status: HistoryStatus::NotRecorded,
                operation: Some(preview.operation().clone()),
            },
        )
    }

    fn capture_frame(
        snapshot: &AuthoringSnapshot,
        requested: PreviewValues,
        frame: DrawableFrame,
        trace: Option<EvaluationTrace>,
        source: ObservationSource,
    ) -> Result<Self, ObservationError> {
        let version = snapshot.version();
        let document = snapshot.document();
        let evaluation_revision = document.evaluation_revision();
        let required: BTreeSet<_> = frame
            .drawables
            .iter()
            .map(|drawable| drawable.texture_asset_id.as_str())
            .collect();
        let mut assets = Vec::with_capacity(required.len());
        for id in required {
            let asset = document
                .get_asset(id)
                .cloned()
                .ok_or_else(|| ObservationError {
                    code: "MISSING_ASSET".into(),
                    message: "Drawable references an absent asset".into(),
                    asset_id: Some(id.to_owned()),
                })?;
            assets.push(asset);
        }
        let root = snapshot
            .project_path()
            .and_then(|path| path.parent())
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
        let authoring = CapturedAuthoring {
            meshes: document
                .mesh_order()
                .iter()
                .map(|id| {
                    document
                        .get_mesh(id)
                        .cloned()
                        .expect("mesh ID exists in captured document")
                })
                .collect(),
            parts: document
                .part_order()
                .iter()
                .map(|id| {
                    document
                        .get_part(id)
                        .cloned()
                        .expect("part ID exists in captured document")
                })
                .collect(),
            transforms: document
                .transform_order()
                .iter()
                .map(|id| {
                    document
                        .get_transform(id)
                        .cloned()
                        .expect("transform ID exists in captured document")
                })
                .collect(),
            bindings: document
                .binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_binding(id)
                        .cloned()
                        .expect("binding ID exists in captured document")
                })
                .collect(),
            parameters: document
                .parameter_order()
                .iter()
                .map(|id| {
                    document
                        .get_parameter(id)
                        .cloned()
                        .expect("parameter exists")
                })
                .collect(),
            assets: document
                .asset_order()
                .iter()
                .map(|id| document.get_asset(id).cloned().expect("asset exists"))
                .collect(),
            draw_order_groups: document.draw_order_groups().unwrap_or_default().to_vec(),
            scene_bindings: document
                .scene_binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_scene_binding(id)
                        .cloned()
                        .expect("scene binding exists")
                })
                .collect(),
            offscreens: document
                .offscreen_order()
                .iter()
                .map(|id| {
                    document
                        .get_offscreen(id)
                        .cloned()
                        .expect("offscreen exists")
                })
                .collect(),
            glues: document
                .glue_order()
                .iter()
                .map(|id| document.get_glue(id).cloned().expect("glue exists"))
                .collect(),
            blend_key_tables: document
                .blend_key_table_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_key_table(id)
                        .cloned()
                        .expect("key table exists")
                })
                .collect(),
            blend_constraints: document
                .blend_constraint_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_constraint(id)
                        .cloned()
                        .expect("constraint exists")
                })
                .collect(),
            blend_bindings: document
                .blend_binding_order()
                .iter()
                .map(|id| {
                    document
                        .get_blend_binding(id)
                        .cloned()
                        .expect("blend binding exists")
                })
                .collect(),
        };
        Ok(Self {
            version,
            evaluation_revision,
            snapshot_clone_ns: snapshot.clone_elapsed_ns(),
            document_id: document.id().to_owned(),
            requested,
            frame,
            trace,
            assets,
            root,
            source,
            authoring,
        })
    }

    /// Read, decode, and hash each source once. The returned RGBA is the exact
    /// buffer that an observer must upload for this run.
    pub fn resolve_textures(&self) -> Result<Vec<ResolvedTexture>, ObservationError> {
        self.assets
            .iter()
            .map(|asset| {
                let data =
                    read_project_asset(&self.root, asset).map_err(|status| ObservationError {
                        code: status.code,
                        message: status.message,
                        asset_id: Some(asset.id.clone()),
                    })?;
                Ok(ResolvedTexture {
                    asset: asset.clone(),
                    data,
                })
            })
            .collect()
    }
}
