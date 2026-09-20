// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/document.hpp>

namespace kasane {
using PreviewValues = std::unordered_map<std::string, float>;

struct Drawable {
    std::string id, runtime_id, texture_asset_id;
    int32_t texture_slot = 0;
    std::vector<Vec2> positions, uvs; // Runtime units / bottom-left UV origin.
    std::vector<uint32_t> indices;
    int32_t draw_order = 0, render_order = 0;
    float opacity = 1;
    std::array<float, 4> multiply_color{1, 1, 1, 1}, screen_color{0, 0, 0, 1};
    BlendMode blend_mode = BlendMode::normal;
    bool enabled = true, visible = true, double_sided = true, inverted_mask = false;
    std::vector<std::string> masks;
};

struct EvaluatedParameter {
    std::string id;
    float requested, value;
    bool clamped;
};

struct DrawableFrame {
    uint64_t source_revision = 0;
    Canvas canvas;
    std::vector<EvaluatedParameter> parameters;
    std::vector<Drawable> drawables;
};

// Pure evaluation: does not change Document or use an exporter, scene, file,
// texture loader or Core runtime model. Missing values use parameter defaults.
// Failed evaluation leaves the previous output intact.
Status evaluate_frame(const Document &, const PreviewValues &, DrawableFrame &);
Status to_parent_positions(const Document &, const std::string &, std::span<const Vec2>, std::vector<Vec2> &);
Status to_runtime_positions(Canvas, std::span<const Vec2>, std::vector<Vec2> &);
} // namespace kasane
