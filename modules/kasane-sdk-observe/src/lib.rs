//! Immutable input capture for rendering an authoring session.
//!
//! Capture borrows the session briefly. Resource IO happens later, from the
//! captured descriptors, so an observer never holds an editing lock while
//! decoding images or submitting GPU work.

use std::collections::BTreeSet;
use std::path::PathBuf;

use kasane_core::{DrawableFrame, ImageAsset, PreviewValues};
use kasane_preview::required_asset_ids;
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
        let version = session.version();
        let evaluation_revision = session.evaluation_revision();
        let frame = session
            .evaluate(requested)
            .map_err(|error| ObservationError {
                code: error.code.into(),
                message: error.message.into(),
                asset_id: error.object_ids.first().cloned(),
            })?;
        let required: BTreeSet<_> = required_asset_ids(&frame).into_iter().collect();
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
            requested: requested.clone(),
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
