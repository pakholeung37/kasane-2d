// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <algorithm>
#include <cmath>
#include <unordered_set>

namespace kasane {
const Parameter *Document::get_parameter(const std::string &id) const {
    auto it = parameters_.find(id);
    return it == parameters_.end() ? nullptr : &it->second;
}

const MeshBinding *Document::get_binding(const std::string &id) const {
    auto it = bindings_.find(id);
    return it == bindings_.end() ? nullptr : &it->second;
}

const MeshBinding *Document::binding_for_mesh(const std::string &id) const {
    for (const auto &key : binding_order_)
        if (bindings_.at(key).mesh_id == id)
            return &bindings_.at(key);
    return nullptr;
}

Status Document::validate_parameter(const Parameter &p) const {
    if (!valid_uuid(p.id))
        return Status::error("INVALID_ID", p.id + ": parameter requires a canonical UUID");
    if (p.runtime_id.empty())
        return Status::error("INVALID_ID", p.id + ".runtime_id is empty");
    for (const auto &[id, other] : parameters_)
        if (id != p.id && other.runtime_id == p.runtime_id)
            return Status::error("DUPLICATE_RUNTIME_ID", p.id + ".runtime_id duplicates " + id);
    if (!std::isfinite(p.minimum) || !std::isfinite(p.maximum) || !std::isfinite(p.default_value) ||
        !std::isfinite(p.maximum - p.minimum) || p.minimum >= p.maximum || p.default_value < p.minimum ||
        p.default_value > p.maximum)
        return Status::error("INVALID_PARAMETER",
                             p.id + ": require finite minimum < maximum and default in range");
    if (p.decimal_places < 0 || p.decimal_places > 9)
        return Status::error("INVALID_PARAMETER", p.id + ".decimal_places must be 0..9");
    return {};
}

Status Document::canonicalize_binding(MeshBinding &b) const {
    if (!valid_uuid(b.id))
        return Status::error("INVALID_ID", b.id + ": binding requires a canonical UUID");
    const auto *mesh = get_mesh(b.mesh_id);
    if (!mesh)
        return Status::error("MISSING_MESH", b.id + ".mesh_id: " + b.mesh_id);
    if (const auto *old = binding_for_mesh(b.mesh_id); old && old->id != b.id)
        return Status::error("BINDING_CONFLICT", b.id + ": mesh positions already bound by " + old->id);
    if (b.axes.empty() || b.axes.size() > 3)
        return Status::error("INVALID_BINDING", b.id + ": this increment supports 1..3 axes");
    size_t total = 1;
    std::unordered_set<std::string> seen;
    for (const auto &axis : b.axes) {
        const auto *p = get_parameter(axis.parameter_id);
        if (!p)
            return Status::error("MISSING_PARAMETER", b.id + ".axes: " + axis.parameter_id);
        if (!seen.insert(p->id).second)
            return Status::error("DUPLICATE_AXIS", b.id + ".axes: " + p->id);
        if (axis.keys.empty() || axis.keys.size() > size_t(INT32_MAX) / total)
            return Status::error("INVALID_KEYS", b.id + ".axes: empty or overflowing key grid");
        total *= axis.keys.size();
        for (size_t i = 0; i < axis.keys.size(); ++i) {
            const auto k = axis.keys[i];
            if (!std::isfinite(k) || k < p->minimum || k > p->maximum || (i && k <= axis.keys[i - 1]))
                return Status::error(
                    "INVALID_KEYS",
                    b.id + ".axes[" + p->id +
                        "]: finite, strictly increasing keys within parameter range required");
        }
    }
    if (b.keyforms.size() != total)
        return Status::error("INCOMPLETE_KEYFORMS", b.id + ": require every Cartesian key combination");
    std::vector<MeshKeyform> ordered(total);
    std::vector<bool> occupied(total, false);
    for (auto &form : b.keyforms) {
        if (form.keys.size() != b.axes.size() || form.positions.size() != mesh->vertex_ids.size())
            return Status::error("INVALID_LENGTH",
                                 b.id + ".keyforms: axes and positions must match binding and mesh");
        if (auto s = validate_positions(form.positions); !s.ok())
            return Status::error(s.code, b.id + ".keyforms: " + s.message);
        if (auto s = validate_appearance(form.appearance, b.id); !s.ok())
            return s;
        if (form.draw_order)
            if (auto s = validate_draw_order(*form.draw_order, b.id); !s.ok())
                return s;
        size_t index = 0, stride = 1;
        for (size_t a = 0; a < b.axes.size(); ++a) {
            const auto &keys = b.axes[a].keys;
            auto it = std::find(keys.begin(), keys.end(), form.keys[a]);
            if (it == keys.end())
                return Status::error("INVALID_KEY_COMBINATION", b.id + ".keyforms: unknown key value");
            index += size_t(it - keys.begin()) * stride;
            stride *= keys.size();
        }
        if (occupied[index])
            return Status::error("DUPLICATE_KEYFORM", b.id + ".keyforms: repeated combination");
        occupied[index] = true;
        ordered[index] = std::move(form);
    }
    b.keyforms = std::move(ordered);
    return {};
}

EditResult Document::create_parameter(Parameter p) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", "Initialize first"));
    if (contains_id(p.id))
        return failed(Status::error("DUPLICATE_ID", p.id));
    if (p.runtime_id.empty())
        p.runtime_id = p.id;
    if (auto s = validate_parameter(p); !s.ok())
        return failed(s);
    const auto id = p.id;
    parameters_.emplace(id, std::move(p));
    parameter_order_.push_back(id);
    return changed(ChangeKind::structure, {}, {id});
}

EditResult Document::replace_parameter(Parameter p) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    if (!get_parameter(p.id))
        return failed(Status::error("MISSING_PARAMETER", p.id));
    if (auto s = validate_parameter(p); !s.ok())
        return failed(s);
    Document candidate = *this;
    candidate.parameters_[p.id] = p;
    std::vector<std::string> meshes;
    for (const auto &id : binding_order_) {
        auto binding = bindings_.at(id);
        if (auto s = candidate.canonicalize_binding(binding); !s.ok())
            return failed(s);
        for (const auto &axis : binding.axes)
            if (axis.parameter_id == p.id)
                meshes.push_back(binding.mesh_id);
    }
    for (const auto &bid : scene_binding_order_) {
        auto b = scene_bindings_.at(bid);
        if (auto s = candidate.canonicalize_scene_binding(b); !s.ok())
            return failed(s);
    }
    if (!scene_binding_order_.empty())
        meshes = mesh_order_;
    const auto id = p.id;
    parameters_[id] = std::move(p);
    return changed(ChangeKind::structure, std::move(meshes), {id});
}

EditResult Document::create_binding(MeshBinding b) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    if (contains_id(b.id))
        return failed(Status::error("DUPLICATE_ID", b.id));
    if (auto s = canonicalize_binding(b); !s.ok())
        return failed(s);
    const auto id = b.id, mesh = b.mesh_id;
    bindings_.emplace(id, std::move(b));
    binding_order_.push_back(id);
    return changed(ChangeKind::structure, {mesh}, {id, mesh});
}

EditResult Document::replace_binding(MeshBinding b) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    const auto *old = get_binding(b.id);
    if (!old)
        return failed(Status::error("MISSING_BINDING", b.id));
    if (auto s = canonicalize_binding(b); !s.ok())
        return failed(s);
    auto previous = old->mesh_id;
    const auto id = b.id, mesh = b.mesh_id;
    bindings_[id] = std::move(b);
    std::vector<std::string> affected{mesh};
    if (previous != mesh)
        affected.push_back(previous);
    return changed(ChangeKind::structure, affected, {id, mesh, previous});
}

EditResult Document::set_mesh_keyform(const std::string &id, MeshKeyform form) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    const auto *old = get_binding(id);
    if (!old)
        return failed(Status::error("MISSING_BINDING", id));
    auto b = *old;
    auto it = std::find_if(b.keyforms.begin(), b.keyforms.end(),
                           [&](const auto &f) { return f.keys == form.keys; });
    if (it == b.keyforms.end())
        return failed(Status::error("INVALID_KEY_COMBINATION", id));
    *it = std::move(form);
    if (auto s = canonicalize_binding(b); !s.ok())
        return failed(s);
    const auto mesh = b.mesh_id;
    bindings_[id] = std::move(b);
    return changed(ChangeKind::positions, {mesh}, {id, mesh});
}

EditResult Document::replace_mesh_with_keyforms(Mesh mesh, std::span<const VertexMapping> mapping,
                                                std::vector<MeshKeyform> forms) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    const auto *old = get_mesh(mesh.id);
    if (!old)
        return failed(Status::error("MISSING_MESH", mesh.id));
    const auto *binding = binding_for_mesh(mesh.id);
    if (!binding)
        return failed(Status::error("MISSING_BINDING", mesh.id));
    if (mapping.size() != mesh.vertex_ids.size())
        return failed(Status::error("INCOMPLETE_VERTEX_MAPPING", mesh.id));
    std::unordered_set<VertexId> mapped, old_mapped;
    for (const auto &m : mapping) {
        if (!mapped.insert(m.new_id).second ||
            std::find(mesh.vertex_ids.begin(), mesh.vertex_ids.end(), m.new_id) == mesh.vertex_ids.end())
            return failed(Status::error("INVALID_VERTEX_MAPPING", mesh.id + ": new vertex IDs must match"));
        if (m.old_id &&
            (!old_mapped.insert(*m.old_id).second ||
             std::find(old->vertex_ids.begin(), old->vertex_ids.end(), *m.old_id) == old->vertex_ids.end()))
            return failed(
                Status::error("INVALID_VERTEX_MAPPING", mesh.id + ": unknown or repeated source vertex"));
    }
    Document candidate = *this;
    const auto binding_id = binding->id;
    auto next = *binding;
    next.keyforms = std::move(forms);
    candidate.bindings_.erase(binding_id);
    std::erase(candidate.binding_order_, binding_id);
    if (auto e = candidate.replace_mesh(std::move(mesh)); !e.status.ok())
        return failed(e.status);
    if (auto e = candidate.create_binding(std::move(next)); !e.status.ok())
        return failed(e.status);
    const auto id = old->id;
    meshes_[id] = std::move(candidate.meshes_.at(id));
    vertex_slots_[id] = std::move(candidate.vertex_slots_.at(id));
    bindings_[binding_id] = std::move(candidate.bindings_.at(binding_id));
    return changed(ChangeKind::structure, {id}, {id, binding_id});
}

std::vector<std::string> Document::references_to(const std::string &id) const {
    std::vector<std::string> refs;
    for (const auto &m : mesh_order_)
        if (meshes_.at(m).texture_asset_id == id)
            refs.push_back(m);
    for (const auto &b : binding_order_) {
        const auto &binding = bindings_.at(b);
        bool refers = binding.mesh_id == id;
        for (const auto &axis : binding.axes)
            refers |= axis.parameter_id == id;
        if (refers)
            refs.push_back(b);
    }
    for (const auto &[key, t] : transforms_)
        if (t.parent_id == id || t.part_id == id)
            refs.push_back(key);
    for (const auto &[key, p] : parts_)
        if (p.parent_id == id)
            refs.push_back(key);
    for (const auto &[key, m] : meshes_)
        if (m.part_id == id || m.deformer_id == id ||
            std::find(m.masks.begin(), m.masks.end(), id) != m.masks.end())
            refs.push_back(key);
    for (const auto &[key, b] : scene_bindings_) {
        bool refers = b.target_id == id;
        for (const auto &a : b.axes)
            refers |= a.parameter_id == id;
        if (refers)
            refs.push_back(key);
    }
    for (const auto *links : {&organization_parents_, &deformation_parents_})
        for (const auto &[child, parent] : *links)
            if (parent == id)
                refs.push_back(child);
    std::sort(refs.begin(), refs.end());
    refs.erase(std::unique(refs.begin(), refs.end()), refs.end());
    return refs;
}

EditResult Document::erase_object(const std::string &id) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
    if (id == id_ || !contains_id(id))
        return failed(Status::error("MISSING_OBJECT", id));
    auto refs = references_to(id);
    if (!refs.empty()) {
        auto e = failed(Status::error("OBJECT_REFERENCED", id + ": explicitly unbind references first"));
        e.referrers = std::move(refs);
        return e;
    }
    std::vector<std::string> meshes;
    if (const auto *b = get_binding(id))
        meshes.push_back(b->mesh_id);
    if (get_mesh(id))
        meshes.push_back(id);
    if (get_scene_binding(id) || get_transform(id) || get_part(id))
        meshes = mesh_order_;
    transforms_.erase(id);
    parts_.erase(id);
    scene_bindings_.erase(id);
    std::erase(transform_order_, id);
    std::erase(part_order_, id);
    std::erase(scene_binding_order_, id);
    assets_.erase(id);
    meshes_.erase(id);
    vertex_slots_.erase(id);
    deformers_.erase(id);
    parameters_.erase(id);
    bindings_.erase(id);
    std::erase(asset_order_, id);
    std::erase(mesh_order_, id);
    std::erase(deformer_order_, id);
    std::erase(parameter_order_, id);
    std::erase(binding_order_, id);
    organization_parents_.erase(id);
    deformation_parents_.erase(id);
    return changed(ChangeKind::structure, std::move(meshes), {id});
}
} // namespace kasane
