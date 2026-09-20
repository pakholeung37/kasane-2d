// SPDX-License-Identifier: MIT
#pragma once
#include "texture_store.hpp"
#include "mesh_view.hpp"
#include <godot_cpp/classes/node2d.hpp>
#include <godot_cpp/classes/sub_viewport.hpp>

namespace kasane_gd {
// Temporary preview adapter for M1. M4 will replace MeshView drawing with the
// shared Cubism renderer. The input is already the common DrawableFrame.
class KasaneDocumentPreview : public godot::Node2D {
    GDCLASS(KasaneDocumentPreview, godot::Node2D)
    godot::Ref<KasaneDocumentBridge> document_;
    godot::Ref<KasaneTextureStore> textures_;
    std::unordered_map<std::string, KasaneMeshView *> views_;
    std::vector<godot::SubViewport *> masks_;
    double mask_scale_ = 1;
    godot::Dictionary last_result_;
    void clear_views();
    void document_changed(const godot::Dictionary &);

  protected:
    static void _bind_methods();

  public:
    void _process(double) override;
    void set_document(const godot::Ref<KasaneDocumentBridge> &);

    godot::Ref<KasaneDocumentBridge> get_document() const { return document_; }

    void set_texture_store(const godot::Ref<KasaneTextureStore> &);
    godot::Dictionary refresh();

    godot::Dictionary get_last_result() const { return last_result_; }

    KasaneMeshView *get_mesh_view(const godot::String &) const;
};
} // namespace kasane_gd
