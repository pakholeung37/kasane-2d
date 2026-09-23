//! Engine-independent preview resource orchestration.
//!
//! This crate owns the part of preview preparation that should remain shared
//! by Godot, wgpu, and headless validation: deciding when project resources
//! must be revalidated, loading the resources required by a frame through a
//! host-provided adapter, and checking the loaded texture metadata against the
//! document. Host APIs only implement [`AssetResolver`].

use std::collections::{HashMap, HashSet};

use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::{ImageAsset, Status};
use kasane_project::store::{DocumentSession, ResourceDiagnostic};

/// Metadata for a texture that a host has made available to the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedTextureInfo {
    pub width: u32,
    pub height: u32,
}

/// The host-specific portion of preview resource loading.
///
/// The adapter does not expose a native texture handle here. The renderer gets
/// those handles through the host backend; this trait only supplies the
/// metadata and the operation needed to make a project asset available.
pub trait AssetResolver {
    fn texture_info(&self, asset_id: &str) -> Option<LoadedTextureInfo>;

    fn resolve_asset(&mut self, session: &DocumentSession, asset_id: &str) -> Status;
}

/// Failure returned by [`PreviewResources::verify_frame`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceFailure {
    pub status: Status,
    pub diagnostics: Vec<ResourceDiagnostic>,
}

impl ResourceFailure {
    fn status(status: Status) -> Self {
        Self {
            status,
            diagnostics: Vec::new(),
        }
    }
}

/// Cached project-resource state used to choose between a full and incremental
/// preview refresh.
#[derive(Clone, Debug, Default)]
pub struct PreviewResources {
    verified_assets: HashMap<String, ImageAsset>,
    verified_manifest: String,
}

impl PreviewResources {
    /// Forget the last successful project verification.
    pub fn reset(&mut self) {
        self.verified_assets.clear();
        self.verified_manifest.clear();
    }

    /// Return whether the project resource snapshot has changed since the last
    /// successful verification.
    pub fn needs_reload(&self, session: &DocumentSession) -> bool {
        let source = session.document();
        self.verified_manifest != session.manifest().to_string_lossy()
            || source.asset_order().len() != self.verified_assets.len()
            || source
                .asset_order()
                .iter()
                .any(|id| source.get_asset(id) != self.verified_assets.get(id))
    }

    /// Validate and, when necessary, load every asset used by `frame`.
    ///
    /// `reload_assets` preserves the existing preview behavior: a full refresh
    /// verifies every project asset, while a geometry-only refresh lazily
    /// resolves only assets used by the evaluated frame.
    pub fn verify_frame<R: AssetResolver>(
        &self,
        session: &DocumentSession,
        frame: &DrawableFrame,
        resolver: &mut R,
        reload_assets: bool,
    ) -> Result<(), ResourceFailure> {
        let has_project_root = !session.root().as_os_str().is_empty();
        let used = required_asset_ids(frame);

        if reload_assets && has_project_root {
            let mut diagnostics = Vec::new();
            for asset_id in session.document().asset_order() {
                let status = if used.contains(asset_id.as_str()) {
                    resolver.resolve_asset(session, asset_id)
                } else {
                    session
                        .read_asset(asset_id)
                        .map(|_| Status::ok())
                        .unwrap_or_else(|status| status)
                };
                if !status.is_ok() {
                    diagnostics.push(ResourceDiagnostic {
                        asset_id: asset_id.clone(),
                        code: status.code,
                        message: status.message,
                    });
                }
            }
            if !diagnostics.is_empty() {
                return Err(ResourceFailure {
                    status: Status::error(
                        "INCOMPLETE_RESOURCES",
                        "Project resources failed verification.",
                    ),
                    diagnostics,
                });
            }
        }

        for asset_id in used {
            if resolver.texture_info(asset_id).is_none() && !reload_assets && has_project_root {
                let status = resolver.resolve_asset(session, asset_id);
                if !status.is_ok() {
                    return Err(ResourceFailure::status(status));
                }
            }

            let Some(texture) = resolver.texture_info(asset_id) else {
                return Err(ResourceFailure::status(Status::error(
                    "MISSING_TEXTURE",
                    "Preview texture is not loaded; source edits remain valid.",
                )));
            };
            let Some(asset) = session.document().get_asset(asset_id) else {
                return Err(ResourceFailure::status(Status::error(
                    "MISSING_ASSET",
                    asset_id,
                )));
            };
            if texture.width != asset.width || texture.height != asset.height {
                return Err(ResourceFailure::status(Status::error(
                    "RESOURCE_MISMATCH",
                    "Preview texture dimensions differ from source metadata.",
                )));
            }
        }

        Ok(())
    }

    /// Record the current project snapshot after the host has rendered it
    /// successfully. Keeping this separate prevents a failed draw from
    /// certifying a resource snapshot that was never displayed.
    pub fn mark_verified(&mut self, session: &DocumentSession) {
        let source = session.document();
        self.verified_assets = source
            .asset_order()
            .iter()
            .filter_map(|id| {
                source
                    .get_asset(id)
                    .map(|asset| (id.clone(), asset.clone()))
            })
            .collect();
        self.verified_manifest = session.manifest().to_string_lossy().into_owned();
    }
}

/// Return the unique project assets referenced by the evaluated frame.
pub fn required_asset_ids(frame: &DrawableFrame) -> HashSet<&str> {
    frame
        .drawables
        .iter()
        .map(|drawable| drawable.texture_asset_id.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::types::{Canvas, ImageAsset, Vec2};

    const ASSET_ID: &str = "00000000-0000-0000-0000-000000000001";

    #[derive(Default)]
    struct TestResolver {
        textures: HashMap<String, LoadedTextureInfo>,
        resolved: Vec<String>,
    }

    impl AssetResolver for TestResolver {
        fn texture_info(&self, asset_id: &str) -> Option<LoadedTextureInfo> {
            self.textures.get(asset_id).copied()
        }

        fn resolve_asset(&mut self, _session: &DocumentSession, asset_id: &str) -> Status {
            self.resolved.push(asset_id.to_owned());
            Status::ok()
        }
    }

    fn session_with_asset() -> DocumentSession {
        let mut session = DocumentSession::new();
        assert!(session
            .document_mut()
            .initialize(
                "00000000-0000-0000-0000-000000000010",
                Canvas::new(64.0, 32.0, Vec2::default(), 1.0),
            )
            .is_ok());
        assert!(session
            .document_mut()
            .add_asset(ImageAsset {
                id: ASSET_ID.to_owned(),
                name: "atlas".to_owned(),
                source: "assets/atlas.png".to_owned(),
                width: 64,
                height: 32,
                ..Default::default()
            })
            .status
            .is_ok());
        session
    }

    #[test]
    fn required_assets_are_unique_and_borrow_the_frame() {
        let frame = DrawableFrame {
            drawables: vec![
                kasane_core::evaluation::Drawable {
                    texture_asset_id: "atlas".to_owned(),
                    ..Default::default()
                },
                kasane_core::evaluation::Drawable {
                    texture_asset_id: "atlas".to_owned(),
                    ..Default::default()
                },
                kasane_core::evaluation::Drawable {
                    texture_asset_id: "other".to_owned(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            required_asset_ids(&frame),
            HashSet::from(["atlas", "other"])
        );
    }

    #[test]
    fn successful_verification_can_commit_and_detect_a_later_metadata_change() {
        let mut session = session_with_asset();
        let frame = DrawableFrame {
            canvas: Canvas::new(64.0, 32.0, Vec2::default(), 1.0),
            drawables: vec![kasane_core::evaluation::Drawable {
                texture_asset_id: ASSET_ID.to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut resolver = TestResolver {
            textures: HashMap::from([(
                ASSET_ID.to_owned(),
                LoadedTextureInfo {
                    width: 64,
                    height: 32,
                },
            )]),
            ..Default::default()
        };
        let mut resources = PreviewResources::default();

        assert!(resources
            .verify_frame(&session, &frame, &mut resolver, true)
            .is_ok());
        resources.mark_verified(&session);
        assert!(!resources.needs_reload(&session));

        let replacement = ImageAsset {
            id: ASSET_ID.to_owned(),
            name: "atlas-updated".to_owned(),
            source: "assets/atlas.png".to_owned(),
            width: 64,
            height: 32,
            ..Default::default()
        };
        assert!(session
            .document_mut()
            .replace_asset(replacement)
            .status
            .is_ok());
        assert!(resources.needs_reload(&session));
    }

    #[test]
    fn missing_loaded_texture_is_reported_before_document_lookup() {
        let session = DocumentSession::new();
        let frame = DrawableFrame {
            drawables: vec![kasane_core::evaluation::Drawable {
                texture_asset_id: "missing".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut resolver = TestResolver::default();

        let failure = PreviewResources::default()
            .verify_frame(&session, &frame, &mut resolver, false)
            .unwrap_err();
        assert_eq!(failure.status.code, "MISSING_TEXTURE");
    }
}
