// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/geometry.hpp>

namespace kasane {
// Prototype data retained only for explicit legacy access and regression.
// The formal evaluator and MOC3 writer reject it until runtime-equivalent
// Rotation/Warp data and algorithms replace this representation.
enum class DeformerKind { rotation, warp };

struct Deformer {
    std::string id, name;
    DeformerKind kind = DeformerKind::rotation;
    Vec2 center{};
    float angle_degrees = 0;
    Vec2 origin{}, size{1, 1};
    uint32_t columns = 1, rows = 1;
    std::vector<Vec2> control_points;
};
} // namespace kasane
