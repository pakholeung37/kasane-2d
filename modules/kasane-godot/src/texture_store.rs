use godot::classes::{image::Format, Image, ImageTexture, Texture2D};
use godot::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kasane_core::types::{ImageAsset, Status};
use kasane_preview::PreviewAssetSource;
use kasane_project::AssetData;
use kasane_project::DocumentSession;

use crate::conversions::{error_dict, status_to_dict, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;

pub(crate) struct ProjectPreviewSource<'a> {
    session: &'a DocumentSession,
    root: PathBuf,
}

impl<'a> ProjectPreviewSource<'a> {
    pub(crate) fn new(session: &'a DocumentSession) -> Self {
        Self {
            session,
            root: session.root(),
        }
    }
}

impl PreviewAssetSource for ProjectPreviewSource<'_> {
    fn manifest_path(&self) -> &Path {
        self.session.manifest()
    }

    fn project_root(&self) -> &Path {
        &self.root
    }

    fn asset_ids(&self) -> &[String] {
        self.session.document().asset_order()
    }

    fn asset(&self, asset_id: &str) -> Option<&ImageAsset> {
        self.session.document().get_asset(asset_id)
    }

    fn read_asset(&self, asset_id: &str) -> Result<AssetData, Status> {
        self.session.read_asset(asset_id)
    }

    fn read_asset_if_changed(
        &self,
        asset_id: &str,
        validated_image: Option<(&str, u32, u32)>,
    ) -> Result<Option<AssetData>, Status> {
        self.session
            .read_asset_if_changed(asset_id, validated_image)
    }
}

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct KasaneTextureStore {
    base: Base<RefCounted>,
    textures: HashMap<String, Gd<Texture2D>>,
    content_hashes: HashMap<String, String>,
}

#[godot_api]
impl KasaneTextureStore {
    #[signal]
    fn changed();

    #[func]
    pub fn set_texture(&mut self, id: GString, texture: Option<Gd<Texture2D>>) -> Dictionary {
        let Some(tex) = texture else {
            return error_dict("INVALID_TEXTURE", "Provide a loaded texture.");
        };
        if tex.get_width() <= 0 || tex.get_height() <= 0 {
            return error_dict("INVALID_TEXTURE", "Provide a loaded texture.");
        }
        let id_str = id.to_string();
        self.content_hashes.remove(&id_str);
        self.textures.insert(id_str, tex);
        self.base_mut().emit_signal("changed", &[]);
        status_to_dict(&Status::ok())
    }

    #[func]
    pub fn load_asset(&mut self, doc: Option<Gd<KasaneDocumentBridge>>, id: GString) -> Dictionary {
        let status = self.resolve_asset(doc, id);
        self.base_mut().emit_signal("changed", &[]);
        status_to_dict(&status)
    }

    pub fn resolve_asset(&mut self, doc: Option<Gd<KasaneDocumentBridge>>, id: GString) -> Status {
        let Some(d) = doc else {
            return Status::error("MISSING_DOCUMENT", "Provide a Document.");
        };
        let d_bind = d.bind();
        let id_str = id.to_string();
        self.resolve_asset_from_session(d_bind.session(), &id_str)
    }

    /// Resolve an asset using an already-borrowed project session.
    pub fn resolve_asset_from_session(
        &mut self,
        session: &DocumentSession,
        id_str: &str,
    ) -> Status {
        self.resolve_asset_from_source(&ProjectPreviewSource::new(session), id_str)
    }

    pub(crate) fn resolve_asset_from_source(
        &mut self,
        source: &dyn PreviewAssetSource,
        id_str: &str,
    ) -> Status {
        let Some(_) = source.asset(id_str) else {
            return Status::error("MISSING_ASSET", id_str);
        };
        let validated_image = self.textures.get(id_str).and_then(|t| {
            self.content_hashes
                .get(id_str)
                .map(|hash| (hash.as_str(), t.get_width() as u32, t.get_height() as u32))
        });
        let bytes = match source.read_asset_if_changed(id_str, validated_image) {
            Ok(None) => return Status::ok(),
            Ok(Some(b)) => b,
            Err(s) => {
                self.textures.remove(id_str);
                self.content_hashes.remove(id_str);
                return s;
            }
        };
        let mut packed = PackedByteArray::new();
        packed.resize(bytes.rgba.len());
        for (i, &b) in bytes.rgba.iter().enumerate() {
            packed[i] = b;
        }
        let image = Image::create_from_data(
            bytes.width as i32,
            bytes.height as i32,
            false,
            Format::RGBA8,
            &packed,
        );
        if let Some(mut img) = image {
            let _ = img.generate_mipmaps();
            if let Some(tex) = ImageTexture::create_from_image(&img) {
                self.textures.insert(id_str.to_owned(), tex.upcast());
                self.content_hashes.insert(id_str.to_owned(), bytes.sha256);
            }
        }
        Status::ok()
    }

    #[func]
    pub fn get_texture(&self, id: GString) -> Option<Gd<Texture2D>> {
        self.textures.get(&id.to_string()).cloned()
    }

    #[func]
    pub fn clear(&mut self) {
        self.textures.clear();
        self.content_hashes.clear();
        self.base_mut().emit_signal("changed", &[]);
    }
}
