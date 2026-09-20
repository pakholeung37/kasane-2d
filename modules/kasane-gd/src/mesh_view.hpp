// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <godot_cpp/classes/mesh_instance2d.hpp>
#include <godot_cpp/classes/array_mesh.hpp>
#include <godot_cpp/classes/texture2d.hpp>

namespace kasane_gd {
class KasaneMeshView : public godot::MeshInstance2D {
    GDCLASS(KasaneMeshView, godot::MeshInstance2D)
    godot::Ref<godot::ArrayMesh> surface_;
    std::vector<kasane::Vec2> positions_;
    uint64_t uploads_ = 0;
    uint64_t creations_ = 0;
    void update_bounds();

  protected:
    static void _bind_methods();

  public:
    godot::Dictionary initialize(const godot::PackedVector2Array &positions,
                                 const godot::PackedVector2Array &uvs, const godot::PackedInt32Array &indices,
                                 const godot::Ref<godot::Texture2D> &texture);
    godot::Dictionary update_positions(const godot::PackedVector2Array &positions);
    void clear();
    godot::Dictionary get_render_stats() const;

    godot::PackedVector2Array get_positions_snapshot() const { return vectors(positions_); }
};
} // namespace kasane_gd
