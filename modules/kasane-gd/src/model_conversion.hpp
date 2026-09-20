// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <godot_cpp/variant/array.hpp>
#include <godot_cpp/variant/packed_float32_array.hpp>

namespace kasane_gd {
kasane::Status parameter_from_dictionary(const godot::Dictionary &, kasane::Parameter &);
kasane::Status binding_from_dictionary(const godot::Dictionary &, kasane::MeshBinding &);
godot::Dictionary parameter_dictionary(const kasane::Parameter &);
godot::Dictionary binding_dictionary(const kasane::MeshBinding &);
godot::Dictionary appearance_dictionary(const kasane::Appearance &);
kasane::Status appearance_from_dictionary(const godot::Dictionary &, kasane::Appearance &);
godot::Dictionary mesh_properties_dictionary(const kasane::Mesh &);
kasane::Status mesh_properties_from_dictionary(const godot::Dictionary &, kasane::Mesh &);
godot::Dictionary transform_dictionary(const kasane::Transform &);
kasane::Status transform_from_dictionary(const godot::Dictionary &, kasane::Transform &);
godot::Dictionary part_dictionary(const kasane::Part &);
kasane::Status part_from_dictionary(const godot::Dictionary &, kasane::Part &);
godot::Dictionary scene_binding_dictionary(const kasane::SceneBinding &);
kasane::Status scene_binding_from_dictionary(const godot::Dictionary &, kasane::SceneBinding &);
} // namespace kasane_gd
