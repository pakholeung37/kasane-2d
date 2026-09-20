// SPDX-License-Identifier: MIT
#include <kasane/project_codec.hpp>
#include <kasane/filesystem.hpp>
#include <nlohmann/json.hpp>
#include <cmath>
#include <limits>
#include <type_traits>
#include <unordered_set>

namespace kasane {
using Json = nlohmann::json;

namespace {
struct Invalid : std::runtime_error {
    using std::runtime_error::runtime_error;
};

template <class T> struct Sequence : std::false_type {};

template <class T, class A> struct Sequence<std::vector<T, A>> : std::true_type {
    using element = T;
};

template <class T> T value(const Json &j) {
    if constexpr (std::is_same_v<T, std::string>) {
        if (!j.is_string())
            throw Invalid("Expected a string");
        auto s = j.get<std::string>();
        if (s.find('\0') != std::string::npos)
            throw Invalid("Embedded NUL is not supported");
        return s;
    } else if constexpr (std::is_same_v<T, bool>) {
        if (!j.is_boolean())
            throw Invalid("Expected a boolean");
        return j.get<bool>();
    } else if constexpr (std::is_arithmetic_v<T>) {
        if (!j.is_number())
            throw Invalid("Expected a number");
        double n = j.get<double>();
        if (!std::isfinite(n) || n < double(std::numeric_limits<T>::lowest()) ||
            n > double(std::numeric_limits<T>::max()))
            throw Invalid("Number is outside its finite source type");
        if constexpr (std::is_integral_v<T>)
            if (std::floor(n) != n)
                throw Invalid("Expected an integer");
        return static_cast<T>(n);
    } else if constexpr (Sequence<T>::value) {
        if (!j.is_array())
            throw Invalid("Expected an array");
        T result;
        for (const auto &item : j)
            result.push_back(value<typename Sequence<T>::element>(item));
        return result;
    } else
        return j.get<T>();
}

template <class T> void field(const Json &j, const char *key, T &out) {
    try {
        out = value<T>(j.at(key));
    } catch (const std::exception &e) {
        throw Invalid(std::string(key) + ": " + e.what());
    }
}

void accepted(const EditResult &r) {
    if (!r.status.ok())
        throw Invalid(r.status.code + ": " + r.status.message);
}

void accepted(const Status &s) {
    if (!s.ok())
        throw Invalid(s.code + ": " + s.message);
}

const Json &array(const Json &j, const char *key) {
    const auto &a = j.at(key);
    if (!a.is_array())
        throw Invalid(std::string(key) + ": Expected array");
    return a;
}
} // namespace

void to_json(Json &j, const Vec2 &v) {
    j = Json::array({v.x, v.y});
}

void from_json(const Json &j, Vec2 &v) {
    if (!j.is_array() || j.size() != 2)
        throw Invalid("Expected a coordinate pair");
    v = {value<float>(j.at(0)), value<float>(j.at(1))};
}

void to_json(Json &j, const Appearance &a) {
    j = {{"opacity", a.opacity}, {"multiply", a.multiply}, {"screen", a.screen}};
}

void from_json(const Json &j, Appearance &a) {
    field(j, "opacity", a.opacity);
    for (auto key : {"multiply", "screen"}) {
        const auto &v = array(j, key);
        if (v.size() != 3)
            throw Invalid("Expected RGB");
        auto &target = std::string(key) == "multiply" ? a.multiply : a.screen;
        for (size_t i = 0; i < 3; ++i)
            target[i] = value<float>(v.at(i));
    }
}

void to_json(Json &j, const RotationPose &v) {
    j = Json::object();
    j["origin"] = v.origin;
    j["angle"] = v.angle;
    j["scale"] = v.scale;
    j["reflect_x"] = v.reflect_x;
    j["reflect_y"] = v.reflect_y;
}

void from_json(const Json &j, RotationPose &v) {
    field(j, "origin", v.origin);
    field(j, "angle", v.angle);
    field(j, "scale", v.scale);
    field(j, "reflect_x", v.reflect_x);
    field(j, "reflect_y", v.reflect_y);
}

void to_json(Json &j, const Part &v) {
    j = Json::object();
    j["id"] = v.id;
    j["runtime_id"] = v.runtime_id;
    j["name"] = v.name;
    j["parent_id"] = v.parent_id;
    j["enabled"] = v.enabled;
    j["draw_order"] = v.draw_order;
}

void from_json(const Json &j, Part &v) {
    field(j, "id", v.id);
    field(j, "runtime_id", v.runtime_id);
    field(j, "name", v.name);
    field(j, "parent_id", v.parent_id);
    field(j, "enabled", v.enabled);
    field(j, "draw_order", v.draw_order);
}

void to_json(Json &j, const ImageAsset &v) {
    j = Json::object();
    j["id"] = v.id;
    j["name"] = v.name;
    j["source"] = v.source;
    j["width"] = v.width;
    j["height"] = v.height;
    j["sha256"] = v.sha256;
}

void from_json(const Json &j, ImageAsset &v) {
    field(j, "id", v.id);
    field(j, "name", v.name);
    field(j, "source", v.source);
    field(j, "width", v.width);
    field(j, "height", v.height);
    field(j, "sha256", v.sha256);
}

void to_json(Json &j, const Parameter &v) {
    j = Json::object();
    j["id"] = v.id;
    j["runtime_id"] = v.runtime_id;
    j["name"] = v.name;
    j["minimum"] = v.minimum;
    j["maximum"] = v.maximum;
    j["default_value"] = v.default_value;
    j["decimal_places"] = v.decimal_places;
}

void from_json(const Json &j, Parameter &v) {
    field(j, "id", v.id);
    field(j, "runtime_id", v.runtime_id);
    field(j, "name", v.name);
    field(j, "minimum", v.minimum);
    field(j, "maximum", v.maximum);
    field(j, "default_value", v.default_value);
    if (j.contains("decimal_places"))
        field(j, "decimal_places", v.decimal_places);
}

void to_json(Json &j, const BindingAxis &v) {
    j = Json::object();
    j["parameter_id"] = v.parameter_id;
    j["keys"] = v.keys;
}

void from_json(const Json &j, BindingAxis &v) {
    field(j, "parameter_id", v.parameter_id);
    field(j, "keys", v.keys);
}

void to_json(Json &j, const MeshKeyform &v) {
    j = Json::object();
    j["keys"] = v.keys;
    j["positions"] = v.positions;
    j["appearance"] = v.appearance;
    if (v.draw_order)
        j["draw_order"] = *v.draw_order;
}

void from_json(const Json &j, MeshKeyform &v) {
    field(j, "keys", v.keys);
    field(j, "positions", v.positions);
    if (j.contains("appearance"))
        field(j, "appearance", v.appearance);
    if (j.contains("draw_order"))
        v.draw_order = value<float>(j.at("draw_order"));
}

void to_json(Json &j, const SceneKeyform &v) {
    j = Json::object();
    j["keys"] = v.keys;
    j["positions"] = v.positions;
    j["rotation"] = v.rotation;
    j["appearance"] = v.appearance;
    j["draw_order"] = v.draw_order;
}

void from_json(const Json &j, SceneKeyform &v) {
    field(j, "keys", v.keys);
    field(j, "positions", v.positions);
    field(j, "rotation", v.rotation);
    if (j.contains("appearance"))
        field(j, "appearance", v.appearance);
    if (j.contains("draw_order"))
        field(j, "draw_order", v.draw_order);
}

void to_json(Json &j, const MeshBinding &v) {
    j = Json::object();
    j["id"] = v.id;
    j["mesh_id"] = v.mesh_id;
    j["axes"] = v.axes;
    j["keyforms"] = v.keyforms;
}

void from_json(const Json &j, MeshBinding &v) {
    field(j, "id", v.id);
    field(j, "mesh_id", v.mesh_id);
    field(j, "axes", v.axes);
    field(j, "keyforms", v.keyforms);
}

void to_json(Json &j, const SceneBinding &v) {
    j = Json::object();
    j["id"] = v.id;
    j["target_id"] = v.target_id;
    j["axes"] = v.axes;
    j["keyforms"] = v.keyforms;
}

void from_json(const Json &j, SceneBinding &v) {
    field(j, "id", v.id);
    field(j, "target_id", v.target_id);
    field(j, "axes", v.axes);
    field(j, "keyforms", v.keyforms);
}

void to_json(Json &j, const Transform &v) {
    j = Json::object();
    j["id"] = v.id;
    j["runtime_id"] = v.runtime_id;
    j["name"] = v.name;
    j["part_id"] = v.part_id;
    j["parent_id"] = v.parent_id;
    j["kind"] = int(v.kind);
    j["base_angle"] = v.base_angle;
    j["rotation"] = v.rotation;
    j["rows"] = v.rows;
    j["columns"] = v.columns;
    j["quad"] = v.quad;
    j["enabled"] = v.enabled;
    j["points"] = v.points;
    j["appearance"] = v.appearance;
}

void from_json(const Json &j, Transform &v) {
    field(j, "id", v.id);
    field(j, "runtime_id", v.runtime_id);
    field(j, "name", v.name);
    field(j, "part_id", v.part_id);
    field(j, "parent_id", v.parent_id);
    auto kind = value<int>(j.at("kind"));
    if (kind < 0 || kind > 1)
        throw Invalid("Unknown Transform kind");
    v.kind = TransformKind(kind);
    field(j, "base_angle", v.base_angle);
    field(j, "rotation", v.rotation);
    field(j, "rows", v.rows);
    field(j, "columns", v.columns);
    field(j, "quad", v.quad);
    field(j, "enabled", v.enabled);
    field(j, "points", v.points);
    field(j, "appearance", v.appearance);
}

void to_json(Json &j, const Mesh &m) {
    Json properties = {{"part_id", m.part_id},
                       {"deformer_id", m.deformer_id},
                       {"appearance", m.appearance},
                       {"blend_mode", int(m.blend_mode)},
                       {"enabled", m.enabled},
                       {"double_sided", m.double_sided},
                       {"inverted_mask", m.inverted_mask},
                       {"masks", m.masks}};
    if (m.draw_order)
        properties["draw_order"] = *m.draw_order;
    std::vector<VertexId> indices;
    for (const auto &t : m.triangles)
        indices.insert(indices.end(), t.begin(), t.end());
    j = {{"id", m.id},
         {"runtime_id", m.runtime_id},
         {"name", m.name},
         {"texture_asset_id", m.texture_asset_id},
         {"vertex_ids", m.vertex_ids},
         {"base_positions", m.base_positions},
         {"uvs", m.uvs},
         {"triangles", indices},
         {"properties", properties}};
}

void from_json(const Json &j, Mesh &m) {
    field(j, "id", m.id);
    field(j, "runtime_id", m.runtime_id);
    field(j, "name", m.name);
    field(j, "texture_asset_id", m.texture_asset_id);
    field(j, "vertex_ids", m.vertex_ids);
    field(j, "base_positions", m.base_positions);
    field(j, "uvs", m.uvs);
    auto indices = value<std::vector<VertexId>>(j.at("triangles"));
    if (indices.size() % 3)
        throw Invalid("triangles: expected triples of vertex IDs");
    for (size_t i = 0; i < indices.size(); i += 3)
        m.triangles.push_back({indices[i], indices[i + 1], indices[i + 2]});
    const auto &p = j.at("properties");
    field(p, "part_id", m.part_id);
    field(p, "deformer_id", m.deformer_id);
    field(p, "appearance", m.appearance);
    field(p, "enabled", m.enabled);
    field(p, "double_sided", m.double_sided);
    field(p, "inverted_mask", m.inverted_mask);
    field(p, "masks", m.masks);
    if (p.contains("draw_order"))
        m.draw_order = value<float>(p.at("draw_order"));
    auto blend = value<int>(p.at("blend_mode"));
    if (blend < 0 || blend > 2)
        throw Invalid("Unknown blend mode");
    m.blend_mode = BlendMode(blend);
}

Status encode_project(const Document &document, std::string &output) {
    try {
        if (!document.initialized())
            return Status::error("NOT_INITIALIZED", "Initialize Document first");
        if (!document.deformer_order().empty())
            return Status::error("LEGACY_PROJECT", "Prototype deformers are unsupported");
        for (auto &id : document.mesh_order())
            if (!document.parent_of(id).empty() || !document.parent_of(id, true).empty())
                return Status::error("LEGACY_PROJECT", "Prototype parent links are unsupported");
        auto c = document.canvas();
        Json doc = {{"id", document.id()},
                    {"canvas", Json::array({c.width, c.height})},
                    {"canvas_origin", c.origin},
                    {"pixels_per_unit", c.pixels_per_unit}};
        doc["assets"] = Json::array();
        for (const auto &id : document.asset_order())
            doc["assets"].push_back(*document.get_asset(id));
        doc["meshes"] = Json::array();
        for (const auto &id : document.mesh_order())
            doc["meshes"].push_back(*document.get_mesh(id));
        doc["parts"] = Json::array();
        for (const auto &id : document.part_order())
            doc["parts"].push_back(*document.get_part(id));
        doc["transforms"] = Json::array();
        for (const auto &id : document.transform_order())
            doc["transforms"].push_back(*document.get_transform(id));
        doc["parameters"] = Json::array();
        for (const auto &id : document.parameter_order())
            doc["parameters"].push_back(*document.get_parameter(id));
        doc["bindings"] = Json::array();
        for (const auto &id : document.binding_order())
            doc["bindings"].push_back(*document.get_binding(id));
        doc["scene_bindings"] = Json::array();
        for (const auto &id : document.scene_binding_order())
            doc["scene_bindings"].push_back(*document.get_scene_binding(id));
        output =
            Json({{"format", "kasane-directory-project"}, {"format_version", 1}, {"document", doc}}).dump(2) +
            "\n";
        return {};
    } catch (const std::exception &e) {
        return Status::error("INVALID_PROJECT", e.what());
    }
}

Status decode_project(std::string_view text, Document &output) {
    try {
        // Reject duplicate JSON keys rather than silently taking the last occurrence.
        std::vector<std::unordered_set<std::string>> keys;
        auto callback = [&](int depth, Json::parse_event_t event, Json &parsed) {
            if (depth > 128)
                throw Invalid("JSON nesting exceeds 128 levels");
            if (event == Json::parse_event_t::object_start)
                keys.emplace_back();
            if (event == Json::parse_event_t::object_end)
                keys.pop_back();
            if (event == Json::parse_event_t::key && !keys.back().insert(parsed.get<std::string>()).second)
                throw Invalid("Duplicate JSON member");
            return true;
        };
        const auto root = Json::parse(text, callback);
        const auto format = value<std::string>(root.at("format"));
        if (format == "kasane-project")
            return Status::error("LEGACY_PROJECT", "Experimental kasane-project version " +
                                                       root.value("format_version", Json("missing")).dump() +
                                                       " is unsupported; no automatic migration");
        if (format != "kasane-directory-project")
            return Status::error("INVALID_PROJECT", "Unknown project format");
        auto version = value<uint32_t>(root.at("format_version"));
        if (version != 1)
            return Status::error("UNSUPPORTED_VERSION",
                                 "Unsupported directory-project version: " + std::to_string(version));
        const auto &doc = root.at("document");
        Document candidate;
        auto size = value<Vec2>(doc.at("canvas"));
        accepted(candidate.initialize(
            value<std::string>(doc.at("id")),
            {size.x, size.y, value<Vec2>(doc.at("canvas_origin")), value<float>(doc.at("pixels_per_unit"))}));
        for (const auto &item : array(doc, "assets")) {
            auto asset = value<ImageAsset>(item);
            if (!io::asset_path(asset.source) || asset.sha256.size() != 64 ||
                asset.sha256.find_first_not_of("0123456789abcdef") != std::string::npos)
                throw Invalid(asset.id + ": invalid relative asset path or SHA-256");
            accepted(candidate.add_asset(std::move(asset)));
        }
        for (const auto &item : array(doc, "parts")) {
            auto p = value<Part>(item);
            p.parent_id.clear();
            accepted(candidate.create_part(std::move(p)));
        }
        for (const auto &item : array(doc, "parts"))
            accepted(candidate.replace_part(value<Part>(item)));
        for (const auto &item : array(doc, "transforms")) {
            auto t = value<Transform>(item);
            t.parent_id.clear();
            accepted(candidate.create_transform(std::move(t)));
        }
        for (const auto &item : array(doc, "transforms"))
            accepted(candidate.replace_transform(value<Transform>(item)));
        for (const auto &item : array(doc, "meshes")) {
            auto m = value<Mesh>(item);
            m.masks.clear();
            accepted(candidate.create_mesh(std::move(m)));
        }
        for (const auto &item : array(doc, "meshes"))
            accepted(candidate.replace_mesh(value<Mesh>(item)));
        for (auto key : {"deformers", "deformation_links", "organization_links"})
            if (doc.contains(key) && (!doc.at(key).is_array() || !doc.at(key).empty()))
                return Status::error("LEGACY_PROJECT", "Prototype relationships are unsupported");
        for (const auto &item : array(doc, "parameters"))
            accepted(candidate.create_parameter(value<Parameter>(item)));
        for (const auto &item : array(doc, "bindings"))
            accepted(candidate.create_binding(value<MeshBinding>(item)));
        for (const auto &item : array(doc, "scene_bindings"))
            accepted(candidate.create_scene_binding(value<SceneBinding>(item)));
        candidate.mark_saved();
        output = std::move(candidate);
        return {};
    } catch (const std::exception &e) {
        return Status::error("INVALID_PROJECT", e.what());
    }
}
} // namespace kasane
