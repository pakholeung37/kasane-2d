// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <godot_cpp/classes/ref_counted.hpp>

namespace kasane_gd {
class KasaneDocumentBridge;

// A stable-ID handle, not a second copy of the mesh or a scene node.
class KasaneMeshData : public godot::RefCounted {
    GDCLASS(KasaneMeshData, godot::RefCounted)
    uint64_t owner_ = 0;
    uint64_t generation_ = 0;
    godot::String id_;
    KasaneDocumentBridge *owner() const;

  protected:
    static void _bind_methods();

  public:
    void attach(uint64_t owner, uint64_t generation, const godot::String &id);
    bool is_valid() const;

    godot::String get_id() const { return id_; }

    godot::Dictionary snapshot() const;
    godot::String get_name() const;
    void set_name(const godot::String &value);
    godot::PackedVector2Array get_positions() const;
    void set_positions(const godot::PackedVector2Array &value);
    godot::PackedInt64Array get_vertex_ids() const;
    godot::Dictionary set_vertex_positions(const godot::PackedInt64Array &ids,
                                           const godot::PackedVector2Array &positions);
    godot::Dictionary replace_geometry(const godot::PackedInt64Array &ids,
                                       const godot::PackedVector2Array &positions,
                                       const godot::PackedVector2Array &uvs,
                                       const godot::PackedInt64Array &triangles);
};
} // namespace kasane_gd
