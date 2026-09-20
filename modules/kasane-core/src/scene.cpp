// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <algorithm>
#include <cmath>
#include <functional>
#include <unordered_set>

namespace kasane {
Status validate_appearance(const Appearance &a, const std::string &id) {
    if (!std::isfinite(a.opacity) || a.opacity < 0 || a.opacity > 1)
        return Status::error("INVALID_OPACITY", id);
    for (auto color : {a.multiply, a.screen})
        for (float v : color)
            if (!std::isfinite(v) || v < 0 || v > 1)
                return Status::error("INVALID_COLOR", id);
    return {};
}

Status validate_draw_order(float order, const std::string &id) {
    if (!std::isfinite(order) || order < -32768 || order > 32767)
        return Status::error("INVALID_DRAW_ORDER", id + ": supported order range -32768..32767");
    return {};
}

static Status pose_valid(const RotationPose &p, const std::string &id) {
    if (!std::isfinite(p.origin.x) || !std::isfinite(p.origin.y) || !std::isfinite(p.angle) ||
        !std::isfinite(p.scale) || p.scale < 0)
        return Status::error("INVALID_ROTATION", id);
    return {};
}

const Transform *Document::get_transform(const std::string &id) const {
    auto i = transforms_.find(id);
    return i == transforms_.end() ? nullptr : &i->second;
}

const Part *Document::get_part(const std::string &id) const {
    auto i = parts_.find(id);
    return i == parts_.end() ? nullptr : &i->second;
}

const SceneBinding *Document::get_scene_binding(const std::string &id) const {
    auto i = scene_bindings_.find(id);
    return i == scene_bindings_.end() ? nullptr : &i->second;
}

const SceneBinding *Document::binding_for_scene(const std::string &id) const {
    for (const auto &bid : scene_binding_order_)
        if (scene_bindings_.at(bid).target_id == id)
            return &scene_bindings_.at(bid);
    return nullptr;
}

Status Document::validate_part(const Part &p) const {
    if (!valid_uuid(p.id) || p.runtime_id.empty())
        return Status::error("INVALID_ID", p.id);
    for (const auto &[id, other] : parts_)
        if (id != p.id && other.runtime_id == p.runtime_id)
            return Status::error("DUPLICATE_RUNTIME_ID", p.id);
    std::unordered_set<std::string> seen{p.id};
    for (auto id = p.parent_id; !id.empty();) {
        if (!seen.insert(id).second)
            return Status::error("RELATION_CYCLE", p.id + ".parent_id");
        auto parent = get_part(id);
        if (!parent)
            return Status::error("MISSING_PART", id);
        id = parent->parent_id;
    }
    return validate_draw_order(p.draw_order, p.id);
}

Status Document::validate_transform(const Transform &t) const {
    if (!valid_uuid(t.id) || t.runtime_id.empty())
        return Status::error("INVALID_ID", t.id);
    for (const auto &[id, other] : transforms_)
        if (id != t.id && other.runtime_id == t.runtime_id)
            return Status::error("DUPLICATE_RUNTIME_ID", t.id);
    if (!t.part_id.empty() && !get_part(t.part_id))
        return Status::error("MISSING_PART", t.id + ".part_id");
    std::unordered_set<std::string> seen{t.id};
    for (auto id = t.parent_id; !id.empty();) {
        if (!seen.insert(id).second)
            return Status::error("RELATION_CYCLE", t.id + ".parent_id");
        auto parent = get_transform(id);
        if (!parent)
            return Status::error("MISSING_TRANSFORM", id);
        id = parent->parent_id;
    }
    if (t.kind != TransformKind::warp && t.kind != TransformKind::rotation)
        return Status::error("INVALID_KIND", t.id);
    if (!std::isfinite(t.base_angle))
        return Status::error("NON_FINITE", t.id + ".base_angle");
    if (auto s = pose_valid(t.rotation, t.id); !s.ok())
        return s;
    if (t.kind == TransformKind::warp) {
        if (!t.rows || !t.columns || t.rows > 1024 || t.columns > 1024 ||
            t.points.size() != size_t(t.rows + 1) * (t.columns + 1))
            return Status::error("INVALID_WARP_GRID", t.id);
        if (auto s = validate_positions(t.points); !s.ok())
            return s;
    }
    return validate_appearance(t.appearance, t.id);
}

Status Document::validate_mesh_properties(const Mesh &m) const {
    if (!m.part_id.empty() && !get_part(m.part_id))
        return Status::error("MISSING_PART", m.id + ".part_id");
    if (!m.deformer_id.empty() && !get_transform(m.deformer_id))
        return Status::error("MISSING_TRANSFORM", m.id + ".deformer_id");
    if (m.blend_mode != BlendMode::normal && m.blend_mode != BlendMode::additive &&
        m.blend_mode != BlendMode::multiplicative)
        return Status::error("INVALID_BLEND_MODE", m.id);
    if (auto s = validate_appearance(m.appearance, m.id); !s.ok())
        return s;
    if (m.draw_order)
        if (auto s = validate_draw_order(*m.draw_order, m.id); !s.ok())
            return s;
    std::unordered_set<std::string> seen;
    for (const auto &mask : m.masks) {
        if (mask == m.id || !seen.insert(mask).second)
            return Status::error("INVALID_MASK", m.id + ".masks");
        if (!get_mesh(mask))
            return Status::error("MISSING_MESH", m.id + ".masks: " + mask);
    }
    // Mask dependencies must be acyclic, even though each mask samples raw alpha.
    std::function<bool(const std::string &, std::unordered_set<std::string> &)> reaches = [&](const auto &id,
                                                                                              auto &visited) {
        if (id == m.id)
            return true;
        if (!visited.insert(id).second)
            return false;
        for (const auto &next : get_mesh(id)->masks)
            if (reaches(next, visited))
                return true;
        return false;
    };
    for (const auto &mask : m.masks) {
        std::unordered_set<std::string> visited;
        if (reaches(mask, visited))
            return Status::error("RELATION_CYCLE", m.id + ".masks");
    }
    return {};
}

EditResult Document::create_part(Part p) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", p.id));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", p.id));
    if (contains_id(p.id))
        return failed(Status::error("DUPLICATE_ID", p.id));
    if (p.runtime_id.empty())
        p.runtime_id = p.id;
    if (auto s = validate_part(p); !s.ok())
        return failed(s);
    auto id = p.id;
    parts_[id] = std::move(p);
    part_order_.push_back(id);
    return changed(ChangeKind::structure, mesh_order_, {id});
}

EditResult Document::replace_part(Part p) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", p.id));
    if (!get_part(p.id))
        return failed(Status::error("MISSING_PART", p.id));
    if (auto s = validate_part(p); !s.ok())
        return failed(s);
    auto id = p.id;
    parts_[id] = std::move(p);
    return changed(ChangeKind::structure, mesh_order_, {id});
}

EditResult Document::create_transform(Transform t) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", t.id));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", t.id));
    if (contains_id(t.id))
        return failed(Status::error("DUPLICATE_ID", t.id));
    if (t.runtime_id.empty())
        t.runtime_id = t.id;
    if (auto s = validate_transform(t); !s.ok())
        return failed(s);
    auto id = t.id;
    transforms_[id] = std::move(t);
    transform_order_.push_back(id);
    return changed(ChangeKind::structure, mesh_order_, {id});
}

EditResult Document::replace_transform(Transform t) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", t.id));
    const auto *old = get_transform(t.id);
    if (!old)
        return failed(Status::error("MISSING_TRANSFORM", t.id));
    if (binding_for_scene(t.id) && (old->kind != t.kind || old->rows != t.rows || old->columns != t.columns))
        return failed(Status::error("KEYFORMS_REQUIRED",
                                    t.id + ": remove binding before changing transform grid/type"));
    if (auto s = validate_transform(t); !s.ok())
        return failed(s);
    auto id = t.id;
    transforms_[id] = std::move(t);
    return changed(ChangeKind::structure, mesh_order_, {id});
}

Status Document::canonicalize_scene_binding(SceneBinding &b) const {
    if (!valid_uuid(b.id))
        return Status::error("INVALID_ID", b.id);
    auto t = get_transform(b.target_id);
    auto p = get_part(b.target_id);
    if (!t && !p)
        return Status::error("MISSING_OBJECT", b.target_id);
    if (auto old = binding_for_scene(b.target_id); old && old->id != b.id)
        return Status::error("BINDING_CONFLICT", b.target_id);
    if (b.axes.empty() || b.axes.size() > 3)
        return Status::error("INVALID_BINDING", b.id);
    size_t total = 1;
    std::unordered_set<std::string> seen;
    for (const auto &a : b.axes) {
        auto param = get_parameter(a.parameter_id);
        if (!param)
            return Status::error("MISSING_PARAMETER", a.parameter_id);
        if (!seen.insert(a.parameter_id).second)
            return Status::error("DUPLICATE_AXIS", b.id);
        if (a.keys.empty() || a.keys.size() > size_t(INT32_MAX) / total)
            return Status::error("INVALID_KEYS", b.id);
        total *= a.keys.size();
        for (size_t i = 0; i < a.keys.size(); ++i)
            if (!std::isfinite(a.keys[i]) || a.keys[i] < param->minimum || a.keys[i] > param->maximum ||
                (i && a.keys[i] <= a.keys[i - 1]))
                return Status::error("INVALID_KEYS", b.id);
    }
    if (total != b.keyforms.size())
        return Status::error("INCOMPLETE_KEYFORMS", b.id);
    std::vector<SceneKeyform> ordered(total);
    std::vector<bool> occupied(total);
    for (auto &f : b.keyforms) {
        if (f.keys.size() != b.axes.size() ||
            f.positions.size() != (t && t->kind == TransformKind::warp ? t->points.size() : 0))
            return Status::error("INVALID_LENGTH", b.id);
        if (auto s = validate_positions(f.positions); !s.ok())
            return s;
        if (auto s = pose_valid(f.rotation, b.id); !s.ok())
            return s;
        if (auto s = validate_appearance(f.appearance, b.id); !s.ok())
            return s;
        if (auto s = validate_draw_order(f.draw_order, b.id); !s.ok())
            return s;
        size_t index = 0, stride = 1;
        for (size_t a = 0; a < b.axes.size(); ++a) {
            const auto &keys = b.axes[a].keys;
            auto it = std::find(keys.begin(), keys.end(), f.keys[a]);
            if (it == keys.end())
                return Status::error("INVALID_KEY_COMBINATION", b.id);
            index += (it - keys.begin()) * stride;
            stride *= keys.size();
        }
        if (occupied[index])
            return Status::error("DUPLICATE_KEYFORM", b.id);
        occupied[index] = true;
        ordered[index] = std::move(f);
    }
    b.keyforms = std::move(ordered);
    return {};
}

EditResult Document::create_scene_binding(SceneBinding b) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", b.id));
    if (contains_id(b.id))
        return failed(Status::error("DUPLICATE_ID", b.id));
    if (auto s = canonicalize_scene_binding(b); !s.ok())
        return failed(s);
    auto id = b.id, target = b.target_id;
    scene_bindings_[id] = std::move(b);
    scene_binding_order_.push_back(id);
    return changed(ChangeKind::structure, mesh_order_, {id, target});
}

EditResult Document::replace_scene_binding(SceneBinding b) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", b.id));
    if (!get_scene_binding(b.id))
        return failed(Status::error("MISSING_BINDING", b.id));
    if (auto s = canonicalize_scene_binding(b); !s.ok())
        return failed(s);
    auto id = b.id, target = b.target_id, previous = scene_bindings_.at(id).target_id;
    scene_bindings_[id] = std::move(b);
    return changed(ChangeKind::structure, mesh_order_, {id, target, previous});
}

EditResult Document::set_scene_keyform(const std::string &id, SceneKeyform f) {
    auto old = get_scene_binding(id);
    if (!old)
        return failed(Status::error("MISSING_BINDING", id));
    auto b = *old;
    auto it =
        std::find_if(b.keyforms.begin(), b.keyforms.end(), [&](const auto &v) { return v.keys == f.keys; });
    if (it == b.keyforms.end())
        return failed(Status::error("INVALID_KEY_COMBINATION", id));
    *it = std::move(f);
    return replace_scene_binding(std::move(b));
}

EditResult Document::replace_canvas(Canvas canvas) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", id_));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", id_));
    Document candidate;
    if (auto s = candidate.initialize(id_, canvas); !s.ok())
        return failed(s);
    canvas_ = canvas;
    return changed(ChangeKind::structure, mesh_order_, {id_});
}

EditResult Document::replace_asset(ImageAsset a) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", a.id));
    if (!get_asset(a.id))
        return failed(Status::error("MISSING_ASSET", a.id));
    if (!a.width || !a.height || a.source.empty())
        return failed(Status::error("INVALID_ASSET", a.id));
    auto id = a.id;
    assets_[id] = std::move(a);
    return changed(ChangeKind::metadata, mesh_order_, {id});
}

std::vector<std::string> Document::sorted_parts() const {
    std::vector<std::string> result;
    std::unordered_set<std::string> seen;
    std::function<void(const std::string &)> visit = [&](const auto &id) {
        if (id.empty() || !seen.insert(id).second)
            return;
        visit(parts_.at(id).parent_id);
        result.push_back(id);
    };
    for (const auto &id : part_order_)
        visit(id);
    return result;
}

std::vector<std::string> Document::sorted_transforms() const {
    std::vector<std::string> result;
    std::unordered_set<std::string> seen;
    std::function<void(const std::string &)> visit = [&](const auto &id) {
        if (id.empty() || !seen.insert(id).second)
            return;
        visit(transforms_.at(id).parent_id);
        result.push_back(id);
    };
    for (const auto &id : transform_order_)
        visit(id);
    return result;
}
} // namespace kasane
