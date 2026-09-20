// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <godot_cpp/classes/ref_counted.hpp>

namespace kasane_gd {
class KasaneDocumentBridge;

class KasaneDeformerData : public godot::RefCounted {
    GDCLASS(KasaneDeformerData, godot::RefCounted)
    uint64_t owner_ = 0, generation_ = 0;
    godot::String id_;
    KasaneDocumentBridge *owner() const;

  protected:
    static void _bind_methods();

  public:
    void attach(uint64_t owner, uint64_t generation, const godot::String &id);
    bool is_valid() const;
    godot::Dictionary snapshot() const;

    godot::String get_id() const { return id_; }

    double get_angle_degrees() const;
    void set_angle_degrees(double angle);
    godot::Vector2 get_center() const;
    void set_center(godot::Vector2 center);
    godot::PackedVector2Array get_control_points() const;
    void set_control_points(const godot::PackedVector2Array &points);
    godot::Dictionary update_rotation(godot::Vector2 center, double angle);
    godot::Dictionary update_control_points(const godot::PackedVector2Array &points);
    godot::Dictionary bind_to(const godot::String &parent);
};
} // namespace kasane_gd
