// SPDX-License-Identifier: MIT
#include "project_io.hpp"
#include "model_conversion.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/dir_access.hpp>
#include <godot_cpp/classes/file_access.hpp>
#include <godot_cpp/classes/json.hpp>
#include <godot_cpp/classes/os.hpp>
#include <godot_cpp/classes/project_settings.hpp>
#include <cmath>
#include <unordered_set>
using namespace godot;

namespace kasane_gd {
void KasaneProjectIO::_bind_methods() {
    ClassDB::bind_method(D_METHOD("save_project", "document", "path"), &KasaneProjectIO::save_project);
    ClassDB::bind_method(D_METHOD("open_project", "document", "path"), &KasaneProjectIO::open_project);
}

namespace {
constexpr int64_t PROJECT_FORMAT_VERSION = 5;

Array pair_array(kasane::Vec2 p) {
    Array a;
    a.push_back(p.x);
    a.push_back(p.y);
    return a;
}

Dictionary project_dictionary(const kasane::Document &document) {
    Dictionary root;
    root["format"] = "kasane-project";
    root["format_version"] = PROJECT_FORMAT_VERSION;
    Dictionary doc;
    doc["id"] = string(document.id());
    Array canvas;
    canvas.push_back(document.canvas().width);
    canvas.push_back(document.canvas().height);
    doc["canvas"] = canvas;
    doc["canvas_origin"] = pair_array(document.canvas().origin);
    doc["pixels_per_unit"] = document.canvas().pixels_per_unit;
    Array assets;
    for (const auto &id : document.asset_order()) {
        const auto *asset = document.get_asset(id);
        Dictionary item;
        item["id"] = string(asset->id);
        item["name"] = string(asset->name);
        item["source"] = string(asset->source);
        item["width"] = static_cast<int64_t>(asset->width);
        item["height"] = static_cast<int64_t>(asset->height);
        assets.push_back(item);
    }
    doc["assets"] = assets;
    Array meshes;
    for (const auto &id : document.mesh_order()) {
        const auto *mesh = document.get_mesh(id);
        Dictionary item;
        item["id"] = string(mesh->id);
        item["name"] = string(mesh->name);
        item["texture_asset_id"] = string(mesh->texture_asset_id);
        item["runtime_id"] = string(mesh->runtime_id);
        item["properties"] = mesh_properties_dictionary(*mesh);
        item["vertex_ids"] = ids(mesh->vertex_ids);
        Array positions;
        for (const auto &p : mesh->base_positions) {
            Array pair;
            pair.push_back(p.x);
            pair.push_back(p.y);
            positions.push_back(pair);
        }
        item["base_positions"] = positions;
        Array uvs;
        for (const auto &p : mesh->uvs) {
            Array pair;
            pair.push_back(p.x);
            pair.push_back(p.y);
            uvs.push_back(pair);
        }
        item["uvs"] = uvs;
        std::vector<uint32_t> triangles;
        for (const auto &triangle : mesh->triangles)
            triangles.insert(triangles.end(), triangle.begin(), triangle.end());
        item["triangles"] = ids(triangles);
        meshes.push_back(item);
    }
    doc["meshes"] = meshes;
    Array parts, transforms, scene_bindings;
    for (const auto &id : document.sorted_parts())
        parts.push_back(part_dictionary(*document.get_part(id)));
    for (const auto &id : document.sorted_transforms())
        transforms.push_back(transform_dictionary(*document.get_transform(id)));
    for (const auto &id : document.scene_binding_order())
        scene_bindings.push_back(scene_binding_dictionary(*document.get_scene_binding(id)));
    doc["parts"] = parts;
    doc["transforms"] = transforms;
    doc["scene_bindings"] = scene_bindings;
    Array parameters, bindings;
    for (const auto &id : document.parameter_order())
        parameters.push_back(parameter_dictionary(*document.get_parameter(id)));
    for (const auto &id : document.binding_order())
        bindings.push_back(binding_dictionary(*document.get_binding(id)));
    doc["parameters"] = parameters;
    doc["bindings"] = bindings;
    Array deformers, deformation_links, organization_links;
    for (const auto &id : document.deformer_order()) {
        const auto &d = *document.get_deformer(id);
        Dictionary item;
        item["id"] = string(id);
        item["name"] = string(d.name);
        item["kind"] = d.kind == kasane::DeformerKind::rotation ? "rotation" : "warp";
        if (d.kind == kasane::DeformerKind::rotation) {
            item["center"] = pair_array(d.center);
            item["angle_degrees"] = d.angle_degrees;
        } else {
            item["origin"] = pair_array(d.origin);
            item["size"] = pair_array(d.size);
            item["columns"] = d.columns;
            item["rows"] = d.rows;
            Array points;
            for (auto p : d.control_points)
                points.push_back(pair_array(p));
            item["control_points"] = points;
        }
        deformers.push_back(item);
    }
    auto append_links = [&](const std::string &id) {
        for (bool organization : {false, true}) {
            auto parent = document.parent_of(id, organization);
            if (parent.empty())
                continue;
            Dictionary link;
            link["child"] = string(id);
            link["parent"] = string(parent);
            if (organization)
                organization_links.push_back(link);
            else
                deformation_links.push_back(link);
        }
    };
    for (const auto &id : document.mesh_order())
        append_links(id);
    for (const auto &id : document.deformer_order())
        append_links(id);
    doc["deformers"] = deformers;
    doc["deformation_links"] = deformation_links;
    doc["organization_links"] = organization_links;
    root["document"] = doc;
    return root;
}

bool number(const Variant &value) {
    return value.get_type() == Variant::INT || value.get_type() == Variant::FLOAT;
}

bool unsigned_integer(const Variant &value, uint64_t maximum, uint64_t &out) {
    if (!number(value))
        return false;
    const double parsed = static_cast<double>(value);
    if (!std::isfinite(parsed) || parsed < 0 || parsed > static_cast<double>(maximum) ||
        std::floor(parsed) != parsed)
        return false;
    out = static_cast<uint64_t>(parsed);
    return true;
}

kasane::Status require(const Dictionary &d, const char *key, Variant::Type type) {
    if (!d.has(key) || d[key].get_type() != type)
        return kasane::Status::error("INVALID_PROJECT",
                                     utf8(String("Missing or invalid field: ") + String(key)));
    return {};
}

kasane::Status parse_vectors(const Variant &value, std::vector<kasane::Vec2> &out) {
    if (value.get_type() != Variant::ARRAY)
        return kasane::Status::error("INVALID_PROJECT", "Expected an array of coordinate pairs.");
    Array array = value;
    for (int64_t i = 0; i < array.size(); ++i) {
        if (array[i].get_type() != Variant::ARRAY)
            return kasane::Status::error("INVALID_PROJECT", "Expected a coordinate pair.");
        Array pair = array[i];
        if (pair.size() != 2 || !number(pair[0]) || !number(pair[1]))
            return kasane::Status::error("INVALID_PROJECT", "Coordinates require two numbers.");
        out.push_back({static_cast<float>(pair[0]), static_cast<float>(pair[1])});
    }
    return {};
}

kasane::Status parse_vertex_ids(const Variant &value, std::vector<uint32_t> &out) {
    if (value.get_type() != Variant::ARRAY)
        return kasane::Status::error("INVALID_PROJECT", "Expected an array of vertex IDs.");
    Array array = value;
    for (int64_t i = 0; i < array.size(); ++i) {
        uint64_t parsed = 0;
        if (!unsigned_integer(array[i], UINT32_MAX, parsed))
            return kasane::Status::error("INVALID_VERTEX_ID", "Vertex IDs must fit uint32.");
        out.push_back(static_cast<uint32_t>(parsed));
    }
    return {};
}

kasane::Status parse_pair(const Dictionary &d, const char *key, kasane::Vec2 &out) {
    if (auto s = require(d, key, Variant::ARRAY); !s.ok())
        return s;
    Array pair = d[key];
    if (pair.size() != 2 || !number(pair[0]) || !number(pair[1]))
        return kasane::Status::error("INVALID_PROJECT", "Expected a numeric pair.");
    out = {static_cast<float>(pair[0]), static_cast<float>(pair[1])};
    return {};
}

kasane::Status parse_deformers(const Dictionary &doc, kasane::Document &document) {
    if (auto s = require(doc, "deformers", Variant::ARRAY); !s.ok())
        return s;
    Array list = doc["deformers"];
    for (int64_t i = 0; i < list.size(); ++i) {
        if (list[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Deformer must be an object.");
        Dictionary item = list[i];
        for (auto key : {"id", "name", "kind"})
            if (auto s = require(item, key, Variant::STRING); !s.ok())
                return s;
        kasane::Deformer d;
        d.id = utf8(item["id"]);
        d.name = utf8(item["name"]);
        const String kind = item["kind"];
        if (kind == "rotation") {
            if (auto s = parse_pair(item, "center", d.center); !s.ok())
                return s;
            if (!item.has("angle_degrees") || !number(item["angle_degrees"]))
                return kasane::Status::error("INVALID_PROJECT", "Rotation requires numeric angle.");
            d.angle_degrees = static_cast<float>(item["angle_degrees"]);
        } else if (kind == "warp") {
            d.kind = kasane::DeformerKind::warp;
            if (auto s = parse_pair(item, "origin", d.origin); !s.ok())
                return s;
            if (auto s = parse_pair(item, "size", d.size); !s.ok())
                return s;
            uint64_t columns = 0, rows = 0;
            if (!item.has("columns") || !item.has("rows") ||
                !unsigned_integer(item["columns"], 16, columns) ||
                !unsigned_integer(item["rows"], 16, rows) || !columns || !rows)
                return kasane::Status::error("INVALID_WARP",
                                             "Warp cell counts must be integers from 1 to 16.");
            d.columns = uint32_t(columns);
            d.rows = uint32_t(rows);
            if (!item.has("control_points"))
                return kasane::Status::error("INVALID_PROJECT", "Warp control_points are required.");
            if (auto s = parse_vectors(item["control_points"], d.control_points); !s.ok())
                return s;
            if (d.control_points.size() != (columns + 1) * (rows + 1))
                return kasane::Status::error("INVALID_LENGTH",
                                             "Warp control point count does not match grid.");
        } else
            return kasane::Status::error("INVALID_PROJECT", "Unknown deformer kind.");
        auto edit = document.create_deformer(std::move(d));
        if (!edit.status.ok())
            return edit.status;
    }
    for (bool organization : {false, true}) {
        const char *key = organization ? "organization_links" : "deformation_links";
        if (auto s = require(doc, key, Variant::ARRAY); !s.ok())
            return s;
        Array links = doc[key];
        std::unordered_set<std::string> children;
        for (int64_t i = 0; i < links.size(); ++i) {
            if (links[i].get_type() != Variant::DICTIONARY)
                return kasane::Status::error("INVALID_PROJECT", "Parent link must be an object.");
            Dictionary link = links[i];
            for (auto field : {"child", "parent"})
                if (auto s = require(link, field, Variant::STRING); !s.ok())
                    return s;
            auto child = utf8(link["child"]), parent = utf8(link["parent"]);
            if (!children.insert(child).second || parent.empty())
                return kasane::Status::error("INVALID_PROJECT", "Duplicate child or empty parent link.");
            auto edit = document.set_parent(child, parent, organization);
            if (!edit.status.ok())
                return edit.status;
        }
    }
    for (const auto &id : document.mesh_order()) {
        std::vector<kasane::Vec2> evaluated;
        if (auto status = document.evaluate_legacy_mesh(id, evaluated); !status.ok())
            return status;
    }
    return {};
}

kasane::Status parse_project(const Dictionary &root, kasane::Document &document) {
    if (auto s = require(root, "format", Variant::STRING); !s.ok())
        return s;
    if (String(root["format"]) != "kasane-project")
        return kasane::Status::error("INVALID_PROJECT", "Not a Kasane project file.");
    uint64_t format_version = 0;
    if (!root.has("format_version") || !unsigned_integer(root["format_version"], UINT32_MAX, format_version))
        return kasane::Status::error("INVALID_PROJECT", "Missing integer format_version.");
    if (format_version != PROJECT_FORMAT_VERSION)
        return kasane::Status::error("UNSUPPORTED_VERSION", "This project format version is not supported.");
    if (auto s = require(root, "document", Variant::DICTIONARY); !s.ok())
        return s;
    Dictionary doc = root["document"];
    if (auto s = require(doc, "id", Variant::STRING); !s.ok())
        return s;
    if (auto s = require(doc, "canvas", Variant::ARRAY); !s.ok())
        return s;
    Array canvas = doc["canvas"];
    if (canvas.size() != 2 || !number(canvas[0]) || !number(canvas[1]))
        return kasane::Status::error("INVALID_PROJECT", "Canvas requires two numbers.");
    kasane::Canvas source_canvas{static_cast<float>(canvas[0]), static_cast<float>(canvas[1])};
    if (auto s = parse_pair(doc, "canvas_origin", source_canvas.origin); !s.ok())
        return s;
    if (!doc.has("pixels_per_unit") || !number(doc["pixels_per_unit"]))
        return kasane::Status::error("INVALID_PROJECT", "Canvas pixels_per_unit must be a number.");
    source_canvas.pixels_per_unit = static_cast<float>(doc["pixels_per_unit"]);
    if (auto s = document.initialize(utf8(doc["id"]), source_canvas); !s.ok())
        return s;
    if (auto s = require(doc, "assets", Variant::ARRAY); !s.ok())
        return s;
    Array assets = doc["assets"];
    for (int64_t i = 0; i < assets.size(); ++i) {
        if (assets[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Asset entries must be objects.");
        Dictionary item = assets[i];
        for (const auto key : {"id", "name", "source"})
            if (auto s = require(item, key, Variant::STRING); !s.ok())
                return s;
        uint64_t width_value = 0, height_value = 0;
        if (!item.has("width") || !unsigned_integer(item["width"], UINT32_MAX, width_value) ||
            !item.has("height") || !unsigned_integer(item["height"], UINT32_MAX, height_value))
            return kasane::Status::error("INVALID_PROJECT", "Asset dimensions must be integers.");
        const auto width = static_cast<int64_t>(width_value), height = static_cast<int64_t>(height_value);
        if (width <= 0 || height <= 0)
            return kasane::Status::error("INVALID_ASSET", "Asset dimensions are outside uint32.");
        const String source = item["source"];
        const auto id = utf8(item["id"]);
        auto edit = document.add_asset({id, utf8(item["name"]), utf8(source), static_cast<uint32_t>(width),
                                        static_cast<uint32_t>(height)});
        if (!edit.status.ok())
            return edit.status;
    }
    for (auto field : {"parts", "transforms", "scene_bindings"})
        if (auto s = require(doc, field, Variant::ARRAY); !s.ok())
            return s;
    Array parts = doc["parts"], transforms = doc["transforms"];
    for (int64_t i = 0; i < parts.size(); ++i) {
        if (parts[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Part must be object");
        kasane::Part p;
        if (auto s = part_from_dictionary(parts[i], p); !s.ok())
            return s;
        if (auto e = document.create_part(std::move(p)); !e.status.ok())
            return e.status;
    }
    for (int64_t i = 0; i < transforms.size(); ++i) {
        if (transforms[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Transform must be object");
        kasane::Transform t;
        if (auto s = transform_from_dictionary(transforms[i], t); !s.ok())
            return s;
        if (auto e = document.create_transform(std::move(t)); !e.status.ok())
            return e.status;
    }
    if (auto s = require(doc, "meshes", Variant::ARRAY); !s.ok())
        return s;
    Array meshes = doc["meshes"];
    for (int64_t i = 0; i < meshes.size(); ++i) {
        if (meshes[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Mesh entries must be objects.");
        Dictionary item = meshes[i];
        for (const auto key : {"id", "name", "texture_asset_id"})
            if (auto s = require(item, key, Variant::STRING); !s.ok())
                return s;
        kasane::Mesh mesh;
        mesh.id = utf8(item["id"]);
        mesh.name = utf8(item["name"]);
        mesh.texture_asset_id = utf8(item["texture_asset_id"]);
        if (auto s = require(item, "runtime_id", Variant::STRING); !s.ok())
            return s;
        mesh.runtime_id = utf8(item["runtime_id"]);
        if (!item.has("vertex_ids") || !item.has("base_positions") || !item.has("uvs") ||
            !item.has("triangles"))
            return kasane::Status::error("INVALID_PROJECT", "Mesh geometry fields are required.");
        if (auto s = parse_vertex_ids(item["vertex_ids"], mesh.vertex_ids); !s.ok())
            return s;
        if (auto s = parse_vectors(item["base_positions"], mesh.base_positions); !s.ok())
            return s;
        if (auto s = parse_vectors(item["uvs"], mesh.uvs); !s.ok())
            return s;
        std::vector<uint32_t> flat;
        if (auto s = parse_vertex_ids(item["triangles"], flat); !s.ok())
            return s;
        if (flat.size() % 3)
            return kasane::Status::error("INVALID_LENGTH",
                                         "Triangle vertex IDs must be a multiple of three.");
        for (size_t j = 0; j < flat.size(); j += 3)
            mesh.triangles.push_back({flat[j], flat[j + 1], flat[j + 2]});
        auto edit = document.create_mesh(std::move(mesh));
        if (!edit.status.ok())
            return edit.status;
    }
    // All meshes exist before mask references are installed (forward references).
    for (int64_t i = 0; i < meshes.size(); ++i) {
        Dictionary item = meshes[i];
        if (auto s = require(item, "properties", Variant::DICTIONARY); !s.ok())
            return s;
        auto m = *document.get_mesh(utf8(item["id"]));
        if (auto s = mesh_properties_from_dictionary(item["properties"], m); !s.ok())
            return s;
        if (auto e = document.replace_mesh(std::move(m)); !e.status.ok())
            return e.status;
    }
    if (auto status = parse_deformers(doc, document); !status.ok())
        return status;
    for (auto field : {"parameters", "bindings"})
        if (auto s = require(doc, field, Variant::ARRAY); !s.ok())
            return s;
    Array parameters = doc["parameters"], bindings = doc["bindings"];
    for (int64_t i = 0; i < parameters.size(); ++i) {
        if (parameters[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Parameter must be an object");
        kasane::Parameter p;
        if (auto s = parameter_from_dictionary(parameters[i], p); !s.ok())
            return s;
        if (auto e = document.create_parameter(std::move(p)); !e.status.ok())
            return e.status;
    }
    for (int64_t i = 0; i < bindings.size(); ++i) {
        if (bindings[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Binding must be an object");
        kasane::MeshBinding b;
        if (auto s = binding_from_dictionary(bindings[i], b); !s.ok())
            return s;
        if (auto e = document.create_binding(std::move(b)); !e.status.ok())
            return e.status;
    }
    Array scene_bindings = doc["scene_bindings"];
    for (int64_t i = 0; i < scene_bindings.size(); ++i) {
        if (scene_bindings[i].get_type() != Variant::DICTIONARY)
            return kasane::Status::error("INVALID_PROJECT", "Scene binding must be object");
        kasane::SceneBinding b;
        if (auto s = scene_binding_from_dictionary(scene_bindings[i], b); !s.ok())
            return s;
        if (auto e = document.create_scene_binding(std::move(b)); !e.status.ok())
            return e.status;
    }
    document.mark_saved();
    return {};
}
} // namespace

Dictionary KasaneProjectIO::save_project(const Ref<KasaneDocumentBridge> &owner, const String &path) {
    if (owner.is_null())
        return error("MISSING_DOCUMENT", "Provide a Document.");
    auto &document_ = owner->document_;
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (!document_.initialized())
        return error("NOT_INITIALIZED", "Initialize Document first.");
    if (document_.transaction_active())
        return error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction before saving.");
    const String absolute = ProjectSettings::get_singleton()->globalize_path(path);
    const String temporary =
        absolute + String(".tmp.") + String::num_int64(OS::get_singleton()->get_process_id());
    Ref<FileAccess> file = FileAccess::open(temporary, FileAccess::WRITE);
    if (file.is_null())
        return error("SAVE_FAILED", "Could not create a temporary project file.");
    file->store_string(JSON::stringify(project_dictionary(document_), "  ", true, true) + "\n");
    file->flush();
    const Error write_error = file->get_error();
    file.unref();
    if (write_error != OK) {
        DirAccess::remove_absolute(temporary);
        return error("SAVE_FAILED", "Could not finish writing the temporary project file.");
    }
    const Error rename_error = DirAccess::rename_absolute(temporary, absolute);
    if (rename_error != OK) {
        DirAccess::remove_absolute(temporary);
        return error("SAVE_FAILED",
                     "Could not atomically replace the project file; the previous file was kept.");
    }
    document_.mark_saved();
    auto out = result({});
    out["path"] = path;
    return out;
}

Dictionary KasaneProjectIO::open_project(const Ref<KasaneDocumentBridge> &owner, const String &path) {
    if (owner.is_null())
        return error("MISSING_DOCUMENT", "Provide a Document.");
    auto &document_ = owner->document_;
    if (document_.transaction_active())
        return error("TRANSACTION_ACTIVE", "Commit or cancel first.");
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const String absolute = ProjectSettings::get_singleton()->globalize_path(path);
    Ref<FileAccess> file = FileAccess::open(absolute, FileAccess::READ);
    if (file.is_null())
        return error("OPEN_FAILED", "Project file could not be opened.");
    Ref<JSON> json;
    json.instantiate();
    const Error parse_error = json->parse(file->get_as_text());
    if (parse_error != OK || json->get_data().get_type() != Variant::DICTIONARY)
        return error("INVALID_PROJECT", "Project file is not valid JSON object data.");
    kasane::Document candidate;
    if (auto status = parse_project(json->get_data(), candidate); !status.ok())
        return result(status);
    owner->replace_source(candidate);
    document_.mark_saved();
    auto out = result({});
    out["path"] = path;
    out["revision"] = document_.revision();
    return out;
}

} // namespace kasane_gd
