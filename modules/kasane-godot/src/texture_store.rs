use godot::classes::{image::Format, Image, ImageTexture, Texture2D};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::types::Status;
use kasane_project::DocumentSession;

use crate::conversions::{error_dict, status_to_dict, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;

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

    /// Resolve an asset using an already-borrowed engine-independent session.
    ///
    /// This is the adapter seam used by `kasane-preview`: project IO and
    /// validation stay outside Godot, while only the final decoded image upload
    /// remains here.
    pub fn resolve_asset_from_session(
        &mut self,
        session: &DocumentSession,
        id_str: &str,
    ) -> Status {
        let Some(_) = session.document().get_asset(id_str) else {
            return Status::error("MISSING_ASSET", id_str);
        };
        let validated_image = self.textures.get(id_str).and_then(|t| {
            self.content_hashes
                .get(id_str)
                .map(|hash| (hash.as_str(), t.get_width() as u32, t.get_height() as u32))
        });
        let bytes = match session.read_asset_if_changed(id_str, validated_image) {
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
