//! Immutable input capture for rendering an authoring session.
//!
//! Capture borrows the session briefly. Resource IO happens later, from the
//! captured descriptors, so an observer never holds an editing lock while
//! decoding images or submitting GPU work.

use std::collections::BTreeSet;
use std::path::PathBuf;

use kasane_animation::MotionPreview;
use kasane_core::{DrawableFrame, ImageAsset, PreviewValues};
use kasane_project::{read_project_asset, AssetData};
use kasane_sdk::{AuthoringSession, Version};

mod gpu;
pub use gpu::{DrawableBounds, ObservedFrame, Observer, ObserverConfig, TextureRevision};

#[derive(Clone, Debug)]
pub struct ObservationInput {
    version: Version,
    evaluation_revision: u64,
    document_id: String,
    requested: PreviewValues,
    frame: DrawableFrame,
    assets: Vec<ImageAsset>,
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ResolvedTexture {
    pub asset: ImageAsset,
    pub data: AssetData,
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
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn evaluation_revision(&self) -> u64 {
        self.evaluation_revision
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
    pub fn assets(&self) -> &[ImageAsset] {
        &self.assets
    }
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn capture(
        session: &AuthoringSession,
        requested: &PreviewValues,
    ) -> Result<Self, ObservationError> {
        let frame = session
            .evaluate(requested)
            .map_err(|error| ObservationError {
                code: error.code.into(),
                message: error.message.into(),
                asset_id: error.object_ids.first().cloned(),
            })?;
        Self::capture_frame(session, requested.clone(), frame)
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
        let version = session.version();
        if preview.source_identity() != Some((version.session_id, version.generation))
            || preview.document_revision() != version.revision
            || preview.document_id() != session.document_id()
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
        let mut frame = preview
            .evaluate_drawables()
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
        Self::capture_frame(session, requested, frame)
    }

    fn capture_frame(
        session: &AuthoringSession,
        requested: PreviewValues,
        frame: DrawableFrame,
    ) -> Result<Self, ObservationError> {
        let version = session.version();
        let evaluation_revision = session.evaluation_revision();
        let required: BTreeSet<_> = frame
            .drawables
            .iter()
            .map(|drawable| drawable.texture_asset_id.as_str())
            .collect();
        let mut assets = Vec::with_capacity(required.len());
        for id in required {
            let asset = session.asset(id).ok_or_else(|| ObservationError {
                code: "MISSING_ASSET".into(),
                message: "Drawable references an absent asset".into(),
                asset_id: Some(id.to_owned()),
            })?;
            assets.push(asset);
        }
        let root = session
            .project_path()
            .and_then(|path| path.parent())
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
        Ok(Self {
            version,
            evaluation_revision,
            document_id: session.document_id().to_owned(),
            requested,
            frame,
            assets,
            root,
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
