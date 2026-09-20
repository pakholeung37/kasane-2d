// SPDX-License-Identifier: MIT
#include "model_conversion.hpp"
#include <cmath>
using namespace godot;

namespace kasane_gd {
namespace {
bool numeric(const Variant &v) {
    return v.get_type() == Variant::INT || v.get_type() == Variant::FLOAT;
}

kasane::Status fail() {
    return kasane::Status::error("INVALID_FIELD", "Required field is absent or has the wrong type");
}

kasane::Status strings(const Dictionary &d, std::initializer_list<const char *> fields) {
    for (auto f : fields)
        if (!d.has(f) || d[f].get_type() != Variant::STRING)
            return fail();
    return {};
}

kasane::Status floats(const Variant &v, std::vector<float> &out) {
    if (v.get_type() == Variant::PACKED_FLOAT32_ARRAY) {
        PackedFloat32Array a = v;
        for (int64_t i = 0; i < a.size(); ++i)
            out.push_back(a[i]);
        return {};
    }
    if (v.get_type() != Variant::ARRAY)
        return fail();
    Array a = v;
    for (int64_t i = 0; i < a.size(); ++i) {
        if (!numeric(a[i]))
            return fail();
        out.push_back(float(a[i]));
    }
    return {};
}

Array float_array(const std::vector<float> &v) {
    Array a;
    for (float x : v)
        a.push_back(x);
    return a;
}
} // namespace

kasane::Status parameter_from_dictionary(const Dictionary &d, kasane::Parameter &p) {
    if (auto s = strings(d, {"id", "runtime_id", "name"}); !s.ok())
        return s;
    p.id = utf8(d["id"]);
    p.runtime_id = utf8(d["runtime_id"]);
    p.name = utf8(d["name"]);
    for (auto field : {"minimum", "maximum", "default_value"})
        if (!d.has(field) || !numeric(d[field]))
            return fail();
    p.minimum = float(d["minimum"]);
    p.maximum = float(d["maximum"]);
    p.default_value = float(d["default_value"]);
    if (d.has("decimal_places")) {
        if (!numeric(d["decimal_places"]))
            return fail();
        double places = d["decimal_places"];
        if (!std::isfinite(places) || std::floor(places) != places || places < 0 || places > 9)
            return fail();
        p.decimal_places = int32_t(places);
    }
    return {};
}

kasane::Status binding_from_dictionary(const Dictionary &d, kasane::MeshBinding &b) {
    if (auto s = strings(d, {"id", "mesh_id"}); !s.ok())
        return s;
    b.id = utf8(d["id"]);
    b.mesh_id = utf8(d["mesh_id"]);
    for (auto field : {"axes", "keyforms"})
        if (!d.has(field) || d[field].get_type() != Variant::ARRAY)
            return fail();
    Array axes = d["axes"], forms = d["keyforms"];
    for (int64_t i = 0; i < axes.size(); ++i) {
        if (axes[i].get_type() != Variant::DICTIONARY)
            return fail();
        Dictionary a = axes[i];
        if (auto s = strings(a, {"parameter_id"}); !s.ok())
            return s;
        if (!a.has("keys"))
            return fail();
        kasane::BindingAxis axis;
        axis.parameter_id = utf8(a["parameter_id"]);
        if (auto s = floats(a["keys"], axis.keys); !s.ok())
            return s;
        b.axes.push_back(std::move(axis));
    }
    for (int64_t i = 0; i < forms.size(); ++i) {
        if (forms[i].get_type() != Variant::DICTIONARY)
            return fail();
        Dictionary f = forms[i];
        if (!f.has("keys") || !f.has("positions"))
            return fail();
        kasane::MeshKeyform form;
        if (auto s = floats(f["keys"], form.keys); !s.ok())
            return s;
        if (f["positions"].get_type() == Variant::PACKED_VECTOR2_ARRAY)
            form.positions = vectors(PackedVector2Array(f["positions"]));
        else if (f["positions"].get_type() == Variant::ARRAY) {
            Array points = f["positions"];
            for (int64_t j = 0; j < points.size(); ++j) {
                if (points[j].get_type() != Variant::ARRAY)
                    return fail();
                Array p = points[j];
                if (p.size() != 2 || !numeric(p[0]) || !numeric(p[1]))
                    return fail();
                form.positions.push_back({float(p[0]), float(p[1])});
            }
        } else
            return fail();
        if (f.has("appearance")) {
            if (f["appearance"].get_type() != Variant::DICTIONARY)
                return fail();
            if (auto e = appearance_from_dictionary(f["appearance"], form.appearance); !e.ok())
                return e;
        }
        if (f.has("draw_order")) {
            if (!numeric(f["draw_order"]))
                return fail();
            form.draw_order = float(f["draw_order"]);
        }
        b.keyforms.push_back(std::move(form));
    }
    return {};
}

Dictionary parameter_dictionary(const kasane::Parameter &p) {
    Dictionary d;
    d["id"] = string(p.id);
    d["runtime_id"] = string(p.runtime_id);
    d["name"] = string(p.name);
    d["minimum"] = p.minimum;
    d["maximum"] = p.maximum;
    d["default_value"] = p.default_value;
    d["decimal_places"] = p.decimal_places;
    return d;
}

Dictionary binding_dictionary(const kasane::MeshBinding &b) {
    Dictionary d;
    d["id"] = string(b.id);
    d["mesh_id"] = string(b.mesh_id);
    Array axes, forms;
    for (const auto &a : b.axes) {
        Dictionary axis;
        axis["parameter_id"] = string(a.parameter_id);
        axis["keys"] = float_array(a.keys);
        axes.push_back(axis);
    }
    for (const auto &f : b.keyforms) {
        Dictionary form;
        form["keys"] = float_array(f.keys);
        Array points;
        for (auto p : f.positions) {
            Array point;
            point.push_back(p.x);
            point.push_back(p.y);
            points.push_back(point);
        }
        form["positions"] = points;
        form["appearance"] = appearance_dictionary(f.appearance);
        if (f.draw_order)
            form["draw_order"] = *f.draw_order;
        forms.push_back(form);
    }
    d["axes"] = axes;
    d["keyforms"] = forms;
    return d;
}

namespace {
Array point_array(kasane::Vec2 p) {
    Array a;
    a.push_back(p.x);
    a.push_back(p.y);
    return a;
}

Array points_array(const std::vector<kasane::Vec2> &p) {
    Array a;
    for (auto v : p)
        a.push_back(point_array(v));
    return a;
}

kasane::Status read_points(const Variant &v, std::vector<kasane::Vec2> &out) {
    if (v.get_type() != Variant::ARRAY)
        return fail();
    Array a = v;
    for (int64_t i = 0; i < a.size(); ++i) {
        std::vector<float> pair;
        if (auto s = floats(a[i], pair); !s.ok())
            return s;
        if (pair.size() != 2)
            return fail();
        out.push_back({pair[0], pair[1]});
    }
    return {};
}

kasane::Status numbers(const Dictionary &d, std::initializer_list<const char *> fields) {
    for (auto f : fields)
        if (!d.has(f) || !numeric(d[f]))
            return fail();
    return {};
}

kasane::Status booleans(const Dictionary &d, std::initializer_list<const char *> fields) {
    for (auto f : fields)
        if (!d.has(f) || d[f].get_type() != Variant::BOOL)
            return fail();
    return {};
}

Dictionary pose_dictionary(const kasane::RotationPose &p) {
    Dictionary d;
    d["origin"] = point_array(p.origin);
    d["angle"] = p.angle;
    d["scale"] = p.scale;
    d["reflect_x"] = p.reflect_x;
    d["reflect_y"] = p.reflect_y;
    return d;
}

kasane::Status pose_from_dictionary(const Dictionary &d, kasane::RotationPose &p) {
    if (auto s = numbers(d, {"angle", "scale"}); !s.ok())
        return s;
    if (auto s = booleans(d, {"reflect_x", "reflect_y"}); !s.ok())
        return s;
    if (!d.has("origin"))
        return fail();
    std::vector<float> xy;
    if (auto s = floats(d["origin"], xy); !s.ok())
        return s;
    if (xy.size() != 2)
        return fail();
    p.origin = {xy[0], xy[1]};
    p.angle = float(d["angle"]);
    p.scale = float(d["scale"]);
    p.reflect_x = bool(d["reflect_x"]);
    p.reflect_y = bool(d["reflect_y"]);
    return {};
}
} // namespace

Dictionary appearance_dictionary(const kasane::Appearance &a) {
    Dictionary d;
    d["opacity"] = a.opacity;
    d["multiply"] = float_array({a.multiply.begin(), a.multiply.end()});
    d["screen"] = float_array({a.screen.begin(), a.screen.end()});
    return d;
}

kasane::Status appearance_from_dictionary(const Dictionary &d, kasane::Appearance &a) {
    if (auto s = numbers(d, {"opacity"}); !s.ok())
        return s;
    a.opacity = float(d["opacity"]);
    for (auto key : {"multiply", "screen"}) {
        if (!d.has(key))
            return fail();
        std::vector<float> values;
        if (auto s = floats(d[key], values); !s.ok())
            return s;
        if (values.size() != 3)
            return fail();
        auto &color = String(key) == "multiply" ? a.multiply : a.screen;
        std::copy(values.begin(), values.end(), color.begin());
    }
    return {};
}

Dictionary mesh_properties_dictionary(const kasane::Mesh &m) {
    Dictionary d;
    d["part_id"] = string(m.part_id);
    d["deformer_id"] = string(m.deformer_id);
    d["appearance"] = appearance_dictionary(m.appearance);
    if (m.draw_order)
        d["draw_order"] = *m.draw_order;
    d["blend_mode"] = int(m.blend_mode);
    d["enabled"] = m.enabled;
    d["double_sided"] = m.double_sided;
    d["inverted_mask"] = m.inverted_mask;
    Array masks;
    for (auto &id : m.masks)
        masks.push_back(string(id));
    d["masks"] = masks;
    return d;
}

kasane::Status mesh_properties_from_dictionary(const Dictionary &d, kasane::Mesh &m) {
    if (auto s = strings(d, {"part_id", "deformer_id"}); !s.ok())
        return s;
    if (auto s = numbers(d, {"blend_mode"}); !s.ok())
        return s;
    if (auto s = booleans(d, {"enabled", "double_sided", "inverted_mask"}); !s.ok())
        return s;
    double blend = d["blend_mode"];
    if (!std::isfinite(blend) || blend < 0 || blend > 2 || std::floor(blend) != blend)
        return fail();
    m.part_id = utf8(d["part_id"]);
    m.deformer_id = utf8(d["deformer_id"]);
    m.blend_mode = kasane::BlendMode(int(blend));
    m.enabled = bool(d["enabled"]);
    m.double_sided = bool(d["double_sided"]);
    m.inverted_mask = bool(d["inverted_mask"]);
    if (!d.has("appearance") || d["appearance"].get_type() != Variant::DICTIONARY)
        return fail();
    if (auto s = appearance_from_dictionary(d["appearance"], m.appearance); !s.ok())
        return s;
    if (d.has("draw_order")) {
        if (!numeric(d["draw_order"]))
            return fail();
        m.draw_order = float(d["draw_order"]);
    }
    if (!d.has("masks") || d["masks"].get_type() != Variant::ARRAY)
        return fail();
    Array masks = d["masks"];
    for (int64_t i = 0; i < masks.size(); ++i) {
        if (masks[i].get_type() != Variant::STRING)
            return fail();
        m.masks.push_back(utf8(masks[i]));
    }
    return {};
}

Dictionary part_dictionary(const kasane::Part &p) {
    Dictionary d;
    d["id"] = string(p.id);
    d["runtime_id"] = string(p.runtime_id);
    d["name"] = string(p.name);
    d["parent_id"] = string(p.parent_id);
    d["enabled"] = p.enabled;
    d["draw_order"] = p.draw_order;
    return d;
}

kasane::Status part_from_dictionary(const Dictionary &d, kasane::Part &p) {
    if (auto s = strings(d, {"id", "runtime_id", "name", "parent_id"}); !s.ok())
        return s;
    if (auto s = booleans(d, {"enabled"}); !s.ok())
        return s;
    if (auto s = numbers(d, {"draw_order"}); !s.ok())
        return s;
    p.id = utf8(d["id"]);
    p.runtime_id = utf8(d["runtime_id"]);
    p.name = utf8(d["name"]);
    p.parent_id = utf8(d["parent_id"]);
    p.enabled = bool(d["enabled"]);
    p.draw_order = float(d["draw_order"]);
    return {};
}

Dictionary transform_dictionary(const kasane::Transform &t) {
    Dictionary d;
    d["id"] = string(t.id);
    d["runtime_id"] = string(t.runtime_id);
    d["name"] = string(t.name);
    d["part_id"] = string(t.part_id);
    d["parent_id"] = string(t.parent_id);
    d["kind"] = int(t.kind);
    d["base_angle"] = t.base_angle;
    d["rotation"] = pose_dictionary(t.rotation);
    d["rows"] = t.rows;
    d["columns"] = t.columns;
    d["quad"] = t.quad;
    d["enabled"] = t.enabled;
    d["points"] = points_array(t.points);
    d["appearance"] = appearance_dictionary(t.appearance);
    return d;
}

kasane::Status transform_from_dictionary(const Dictionary &d, kasane::Transform &t) {
    if (auto s = strings(d, {"id", "runtime_id", "name", "part_id", "parent_id"}); !s.ok())
        return s;
    if (auto s = numbers(d, {"kind", "base_angle", "rows", "columns"}); !s.ok())
        return s;
    if (auto s = booleans(d, {"quad", "enabled"}); !s.ok())
        return s;
    for (auto f : {"kind", "rows", "columns"}) {
        double v = d[f];
        if (!std::isfinite(v) || v < 0 || v > (String(f) == "kind" ? 1 : 1024) || std::floor(v) != v)
            return fail();
    }
    t.id = utf8(d["id"]);
    t.runtime_id = utf8(d["runtime_id"]);
    t.name = utf8(d["name"]);
    t.part_id = utf8(d["part_id"]);
    t.parent_id = utf8(d["parent_id"]);
    t.kind = kasane::TransformKind(int(d["kind"]));
    t.base_angle = float(d["base_angle"]);
    t.rows = int(d["rows"]);
    t.columns = int(d["columns"]);
    t.quad = bool(d["quad"]);
    t.enabled = bool(d["enabled"]);
    for (auto f : {"rotation", "appearance"})
        if (!d.has(f) || d[f].get_type() != Variant::DICTIONARY)
            return fail();
    if (auto s = pose_from_dictionary(d["rotation"], t.rotation); !s.ok())
        return s;
    if (auto s = appearance_from_dictionary(d["appearance"], t.appearance); !s.ok())
        return s;
    if (!d.has("points"))
        return fail();
    return read_points(d["points"], t.points);
}

Dictionary scene_binding_dictionary(const kasane::SceneBinding &b) {
    kasane::MeshBinding mesh;
    mesh.id = b.id;
    mesh.mesh_id = b.target_id;
    mesh.axes = b.axes;
    for (auto &f : b.keyforms)
        mesh.keyforms.push_back({f.keys, f.positions, f.appearance, f.draw_order});
    Dictionary d = binding_dictionary(mesh);
    d.erase("mesh_id");
    d["target_id"] = string(b.target_id);
    Array forms = d["keyforms"];
    for (int64_t i = 0; i < forms.size(); ++i) {
        Dictionary f = forms[i];
        f["rotation"] = pose_dictionary(b.keyforms[i].rotation);
    }
    return d;
}

kasane::Status scene_binding_from_dictionary(const Dictionary &d, kasane::SceneBinding &b) {
    if (auto s = strings(d, {"target_id"}); !s.ok())
        return s;
    Dictionary copy = d.duplicate();
    copy["mesh_id"] = d["target_id"];
    kasane::MeshBinding mesh;
    if (auto s = binding_from_dictionary(copy, mesh); !s.ok())
        return s;
    b.id = mesh.id;
    b.target_id = mesh.mesh_id;
    b.axes = std::move(mesh.axes);
    Array forms = d["keyforms"];
    for (size_t i = 0; i < mesh.keyforms.size(); ++i) {
        auto &m = mesh.keyforms[i];
        kasane::SceneKeyform f;
        f.keys = std::move(m.keys);
        f.positions = std::move(m.positions);
        f.appearance = m.appearance;
        f.draw_order = m.draw_order.value_or(0);
        Dictionary v = forms[i];
        if (!v.has("rotation") || v["rotation"].get_type() != Variant::DICTIONARY)
            return fail();
        if (auto s = pose_from_dictionary(v["rotation"], f.rotation); !s.ok())
            return s;
        b.keyforms.push_back(std::move(f));
    }
    return {};
}
} // namespace kasane_gd
