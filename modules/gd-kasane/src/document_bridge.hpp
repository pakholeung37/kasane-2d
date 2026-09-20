// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/evaluation.hpp>
#include <kasane/project.hpp>
#include "mesh_data.hpp"
#include "deformer_data.hpp"
#include <godot_cpp/classes/ref_counted.hpp>
#include <godot_cpp/variant/packed_float32_array.hpp>

namespace kasane_gd {
class KasaneDocumentState : public godot::RefCounted {
    GDCLASS(KasaneDocumentState, godot::RefCounted)
    friend class KasaneDocumentBridge;
    kasane::Document document;
    uint64_t owner = 0;
    uint64_t generation = 0;

  protected:
    static void _bind_methods() {}
};

class KasaneDocumentBridge : public godot::RefCounted {
    GDCLASS(KasaneDocumentBridge, godot::RefCounted)
    kasane::DocumentSession session_;
    uint64_t generation_ = 1;
    friend class KasaneProjectIO;
    kasane::PreviewValues preview_values_;
    godot::Dictionary write_mesh(const godot::Dictionary &description, bool replace);
    godot::Dictionary apply(const kasane::EditResult &edit);

  protected:
    static void _bind_methods();

  public:
    godot::Dictionary create_rotation(const godot::String &id, const godot::String &name,
                                      godot::Vector2 center, double angle);
    godot::Dictionary create_warp(const godot::String &id, const godot::String &name, godot::Vector2 origin,
                                  godot::Vector2 size, int64_t columns, int64_t rows);
    godot::Dictionary set_rotation(const godot::String &id, godot::Vector2 center, double angle);
    godot::Dictionary set_warp_points(const godot::String &id, const godot::PackedVector2Array &points);
    godot::Dictionary set_deform_parent(const godot::String &id, const godot::String &parent);
    godot::Dictionary set_organization_parent(const godot::String &id, const godot::String &parent);
    godot::Dictionary get_deformer_snapshot(const godot::String &id) const;
    godot::Ref<KasaneDeformerData> get_deformer(const godot::String &id) const;
    godot::Dictionary evaluate_mesh(const godot::String &id) const;
    godot::Dictionary initialize(const godot::String &id, godot::Vector2 canvas_size,
                                 godot::Vector2 origin = {}, double pixels_per_unit = 1);
    godot::Dictionary add_image_asset(const godot::String &id, const godot::String &name,
                                      const godot::String &source, int64_t width, int64_t height);
    godot::Dictionary create_mesh(const godot::Dictionary &description);
    godot::Dictionary replace_mesh(const godot::Dictionary &description);
    godot::Ref<KasaneMeshData> get_mesh(const godot::String &id) const;
    godot::Ref<KasaneDocumentState> capture_state() const;
    godot::Dictionary restore_state(const godot::Ref<KasaneDocumentState> &state);

    const kasane::DocumentSession &document_session() const { return session_; }

    uint64_t generation() const { return generation_; }

    const kasane::Document &source() const { return session_.document(); }

    kasane::Status evaluate(kasane::DrawableFrame &out) const;
    godot::Dictionary get_frame() const;
    godot::Dictionary set_preview_values(const godot::Dictionary &values);
    godot::Dictionary write_part(const godot::Dictionary &, bool replace = false);
    godot::Dictionary write_transform(const godot::Dictionary &, bool replace = false);
    godot::Dictionary write_scene_binding(const godot::Dictionary &, bool replace = false);
    godot::Dictionary set_mesh_properties(const godot::String &, const godot::Dictionary &);
    godot::Dictionary create_parameter(const godot::Dictionary &description);
    godot::Dictionary write_binding(const godot::Dictionary &description, bool replace = false);
    godot::Dictionary set_mesh_keyform(const godot::String &binding_id, const godot::PackedFloat32Array &keys,
                                       const godot::PackedVector2Array &positions);
    godot::Dictionary erase_object(const godot::String &id);
    godot::Dictionary set_vertex_positions(const godot::String &mesh_id,
                                           const godot::PackedInt64Array &vertex_ids,
                                           const godot::PackedVector2Array &positions);
    godot::Dictionary rename_mesh(const godot::String &id, const godot::String &name);
    godot::Dictionary begin_transaction();
    godot::Dictionary stage_vertex_positions(const godot::String &mesh_id,
                                             const godot::PackedInt64Array &vertex_ids,
                                             const godot::PackedVector2Array &positions);
    godot::Dictionary commit_transaction();
    godot::Dictionary cancel_transaction();

    godot::Dictionary commit_vertex_updates(const godot::Array &updates, int64_t expected_revision);
    godot::Dictionary get_asset_snapshot(const godot::String &id) const;
    godot::Dictionary get_mesh_snapshot(const godot::String &id) const;
    godot::Dictionary get_document_summary() const;
};
} // namespace kasane_gd
