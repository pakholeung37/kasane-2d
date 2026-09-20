// SPDX-License-Identifier: MIT
#include "texture_store.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/resource_loader.hpp>
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
    textures_[utf8(id)] = texture;
    emit_signal("changed");
    return result({});
}

Dictionary KasaneTextureStore::load_asset(const Ref<KasaneDocumentBridge> &doc, const String &id) {
    if (doc.is_null())
        return error("MISSING_DOCUMENT", "Provide a Document.");
    const auto *asset = doc->source().get_asset(utf8(id));
    if (!asset)
        return error("MISSING_ASSET", "Asset does not exist.");
    const auto source = string(asset->source);
    if (!ResourceLoader::get_singleton()->exists(source, "Texture2D"))
        return error("MISSING_RESOURCE", "Asset texture could not be loaded.");
    Ref<Texture2D> texture = ResourceLoader::get_singleton()->load(source, "Texture2D");
    if (texture.is_null() || texture->get_width() != int64_t(asset->width) ||
        texture->get_height() != int64_t(asset->height))
        return error("RESOURCE_MISMATCH", "Loaded texture dimensions do not match source metadata.");
    return set_texture(id, texture);
}

Ref<Texture2D> KasaneTextureStore::get_texture(const String &id) const {
    auto it = textures_.find(utf8(id));
    return it == textures_.end() ? Ref<Texture2D>{} : it->second;
}

void KasaneTextureStore::clear() {
    textures_.clear();
    emit_signal("changed");
}
} // namespace kasane_gd
