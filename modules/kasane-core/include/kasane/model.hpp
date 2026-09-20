// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/geometry.hpp>
#include <array>
#include <optional>

namespace kasane {
// Source coordinates: pixels, X right / Y down. Origin is measured from the
// top-left. Runtime coordinates: (x-origin.x)/ppu, (origin.y-y)/ppu.
// Source UVs: (0,0) top-left; runtime UVs: (u, 1-v).
struct Canvas {
    float width = 0;
    float height = 0;
    Vec2 origin{};
    float pixels_per_unit = 1;
};

struct ImageAsset {
    std::string id;
    std::string name;
    std::string source;
    uint32_t width = 0;
    uint32_t height = 0;
};
enum class BlendMode { normal, additive, multiplicative };

struct Appearance {
    float opacity = 1;
    std::array<float, 3> multiply{1, 1, 1}, screen{0, 0, 0};
};

struct RotationPose {
    Vec2 origin{};
    float angle = 0, scale = 1;
    bool reflect_x = false, reflect_y = false;
};
// Root coordinates are canvas pixels. Under Rotation coordinates are local
// runtime units; under Warp they are normalized grid coordinates (unbounded).
enum class TransformKind { warp, rotation };

struct Transform {
    std::string id, runtime_id, name, part_id, parent_id;
    TransformKind kind = TransformKind::rotation;
    float base_angle = 0;
    RotationPose rotation;
    uint32_t rows = 1, columns = 1; // Cell counts; points=(rows+1)*(columns+1).
    bool quad = true, enabled = true;
    std::vector<Vec2> points;
    Appearance appearance;
};

struct Part {
    std::string id, runtime_id, name, parent_id;
    bool enabled = true;
    float draw_order = 0;
};

struct SceneKeyform {
    std::vector<float> keys;
    std::vector<Vec2> positions;
    RotationPose rotation;
    Appearance appearance;
    float draw_order = 0;
};

struct Mesh {
    std::string id;
    std::string name;
    std::string texture_asset_id;
    std::vector<VertexId> vertex_ids;
    std::vector<Vec2> base_positions;
    std::vector<Vec2> uvs;
    std::vector<std::array<VertexId, 3>> triangles;
    // Independent from name and internal identity. Empty on creation uses id.
    std::string runtime_id;
    std::string part_id, deformer_id;
    Appearance appearance;
    // Unspecified order uses mesh creation order.
    std::optional<float> draw_order;
    BlendMode blend_mode = BlendMode::normal;
    bool enabled = true, double_sided = true, inverted_mask = false;
    std::vector<std::string> masks;
};

struct Parameter {
    std::string id, runtime_id, name;
    float minimum = -1, maximum = 1, default_value = 0;
    int32_t decimal_places = 6;
};

struct BindingAxis {
    std::string parameter_id;
    std::vector<float> keys;
};

struct MeshKeyform {
    // Explicit key value per axis, in binding axis order.
    std::vector<float> keys;
    std::vector<Vec2> positions;
    Appearance appearance;
    std::optional<float> draw_order;
};

struct MeshBinding {
    std::string id, mesh_id;
    std::vector<BindingAxis> axes;
    // Stored in canonical Cartesian order: axis 0 varies fastest.
    std::vector<MeshKeyform> keyforms;
};

struct SceneBinding {
    std::string id, target_id;
    std::vector<BindingAxis> axes;
    std::vector<SceneKeyform> keyforms;
};

struct VertexMapping {
    VertexId new_id;
    std::optional<VertexId> old_id;
};
} // namespace kasane
