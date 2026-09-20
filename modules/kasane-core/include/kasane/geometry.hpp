// SPDX-License-Identifier: MIT
#pragma once
#include <cstdint>
#include <span>
#include <string>
#include <utility>
#include <vector>

namespace kasane {
struct Status {
    std::string code;
    std::string message;

    bool ok() const { return code.empty(); }

    static Status error(std::string code, std::string message) {
        return {std::move(code), std::move(message)};
    }
};

struct Vec2 {
    float x = 0;
    float y = 0;
    bool operator==(const Vec2 &) const = default;
};

using VertexId = uint32_t;

// Borrowed views are used only for the duration of a synchronous call.
Status validate_positions(std::span<const Vec2> positions);
Status validate_render_mesh(std::span<const Vec2> positions, std::span<const Vec2> uvs,
                            std::span<const uint32_t> indices);
} // namespace kasane
