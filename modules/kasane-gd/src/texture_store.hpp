// SPDX-License-Identifier: MIT
#pragma once
#include "document_bridge.hpp"
#include <godot_cpp/classes/texture2d.hpp>

namespace kasane_gd {
class KasaneTextureStore : public godot::RefCounted {
    GDCLASS(KasaneTextureStore, godot::RefCounted)
    std::unordered_map<std::string, godot::Ref<godot::Texture2D>> textures_;

    std::unordered_map<std::string, std::string> content_hashes_;

  protected:
    static void _bind_methods();

  public:
    godot::Dictionary set_texture(const godot::String &, const godot::Ref<godot::Texture2D> &);
    godot::Dictionary load_asset(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
    godot::Ref<godot::Texture2D> get_texture(const godot::String &) const;
    void clear();
    kasane::Status resolve_asset(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
};
} // namespace kasane_gd
