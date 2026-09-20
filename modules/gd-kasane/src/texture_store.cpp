// SPDX-License-Identifier: MIT
#include "texture_store.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/image.hpp>
#include <godot_cpp/classes/image_texture.hpp>
using namespace godot;

namespace kasane_gd {
void KasaneTextureStore::_bind_methods() {
    ClassDB::bind_method(D_METHOD("set_texture", "asset_id", "texture"), &KasaneTextureStore::set_texture);
    ClassDB::bind_method(D_METHOD("load_asset", "document", "asset_id"), &KasaneTextureStore::load_asset);
    ClassDB::bind_method(D_METHOD("get_texture", "asset_id"), &KasaneTextureStore::get_texture);
    ClassDB::bind_method(D_METHOD("clear"), &KasaneTextureStore::clear);
    ADD_SIGNAL(MethodInfo("changed"));
}

Dictionary KasaneTextureStore::set_texture(const String &id, const Ref<Texture2D> &texture) {
    if (texture.is_null() || texture->get_width() <= 0 || texture->get_height() <= 0)
        return error("INVALID_TEXTURE", "Provide a loaded texture.");
    content_hashes_.erase(utf8(id));
    textures_[utf8(id)] = texture;
    emit_signal("changed");
    return result({});
}

Dictionary KasaneTextureStore::load_asset(const Ref<KasaneDocumentBridge> &doc, const String &id) {
    auto status = resolve_asset(doc, id);
    emit_signal("changed");
    return result(status);
}

kasane::Status KasaneTextureStore::resolve_asset(const Ref<KasaneDocumentBridge> &doc, const String &id) {
    if (doc.is_null())
        return kasane::Status::error("MISSING_DOCUMENT", "Provide a Document.");
    auto asset = doc->source().get_asset(utf8(id));
    if (!asset)
        return kasane::Status::error("MISSING_ASSET", utf8(id));
    kasane::AssetData bytes;
    if (auto s = doc->document_session().read_asset(utf8(id), bytes); !s.ok()) {
        textures_.erase(utf8(id));
        content_hashes_.erase(utf8(id));
        return s;
    }
    if (content_hashes_[utf8(id)] != bytes.sha256) {
        PackedByteArray pixels;
        pixels.resize(bytes.rgba.size());
        std::copy(bytes.rgba.begin(), bytes.rgba.end(), pixels.ptrw());
        auto image = Image::create_from_data(bytes.width, bytes.height, false, Image::FORMAT_RGBA8, pixels);
        image->generate_mipmaps();
        textures_[utf8(id)] = ImageTexture::create_from_image(image);
        content_hashes_[utf8(id)] = bytes.sha256;
    }
    return {};
}

Ref<Texture2D> KasaneTextureStore::get_texture(const String &id) const {
    auto it = textures_.find(utf8(id));
    return it == textures_.end() ? Ref<Texture2D>{} : it->second;
}

void KasaneTextureStore::clear() {
    textures_.clear();
    content_hashes_.clear();
    emit_signal("changed");
}
} // namespace kasane_gd
