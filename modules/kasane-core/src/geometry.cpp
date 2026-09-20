// SPDX-License-Identifier: MIT
#include <kasane/geometry.hpp>
#include <cmath>
#include <limits>

namespace kasane {
Status validate_positions(std::span<const Vec2> positions) {
    for (const auto &p : positions) {
        if (!std::isfinite(p.x) || !std::isfinite(p.y))
            return Status::error("NON_FINITE", "Coordinates must be finite float32 values.");
    }
    return {};
}

Status validate_render_mesh(std::span<const Vec2> positions, std::span<const Vec2> uvs,
                            std::span<const uint32_t> indices) {
    if (positions.size() < 3 || positions.size() > INT32_MAX || positions.size() != uvs.size())
        return Status::error("INVALID_LENGTH",
                             "A mesh needs matching positions/UVs and at least three vertices.");
    if (indices.empty() || indices.size() % 3 != 0 || indices.size() > INT32_MAX)
        return Status::error("INVALID_LENGTH", "Triangle indices must be a non-empty multiple of three.");
    if (auto s = validate_positions(positions); !s.ok())
        return s;
    if (auto s = validate_positions(uvs); !s.ok())
        return s;
    for (size_t i = 0; i < indices.size(); i += 3) {
        const auto a = indices[i], b = indices[i + 1], c = indices[i + 2];
        if (a >= positions.size() || b >= positions.size() || c >= positions.size())
            return Status::error("INVALID_INDEX", "Triangle index is outside the vertex array.");
        if (a == b || b == c || a == c)
            return Status::error("REPEATED_VERTEX", "A triangle must reference three distinct vertices.");
    }
    return {};
}
} // namespace kasane
