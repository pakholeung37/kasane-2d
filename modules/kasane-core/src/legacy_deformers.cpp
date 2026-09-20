// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <algorithm>
#include <cmath>
#include <numbers>
#include <limits>
#include <unordered_set>

namespace kasane {
const Deformer *Document::get_deformer(const std::string &id) const {
    auto it = deformers_.find(id);
    return it == deformers_.end() ? nullptr : &it->second;
}

EditResult Document::create_deformer(Deformer d) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Finish the position transaction first."));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(d.id))
        return failed(Status::error("INVALID_ID", "Deformer ID must be a canonical UUID."));
    if (contains_id(d.id))
        return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (d.kind == DeformerKind::rotation) {
        if (!std::isfinite(d.center.x) || !std::isfinite(d.center.y) || !std::isfinite(d.angle_degrees))
            return failed(Status::error("NON_FINITE", "Rotation values must be finite."));
    } else {
        if (!d.columns || !d.rows || d.columns > 16 || d.rows > 16 || !std::isfinite(d.origin.x) ||
            !std::isfinite(d.origin.y) || !std::isfinite(d.size.x) || !std::isfinite(d.size.y) ||
            d.size.x <= 0 || d.size.y <= 0)
            return failed(Status::error(
                "INVALID_WARP", "Warp requires finite origin, positive size and 1–16 cells per axis."));
        if (d.control_points.empty()) {
            for (uint32_t y = 0; y <= d.rows; ++y)
                for (uint32_t x = 0; x <= d.columns; ++x)
                    d.control_points.push_back({d.origin.x + d.size.x * (float(x) / d.columns),
                                                d.origin.y + d.size.y * (float(y) / d.rows)});
        }
        if (d.control_points.size() != (d.columns + 1) * (d.rows + 1))
            return failed(Status::error("INVALID_LENGTH", "Control points must match the fixed Warp grid."));
        if (auto s = validate_positions(d.control_points); !s.ok())
            return failed(s);
    }
    auto id = d.id;
    deformers_.emplace(id, std::move(d));
    deformer_order_.push_back(id);
    return changed(ChangeKind::metadata, {}, {id});
}

std::string Document::parent_of(const std::string &id, bool organization) const {
    const auto &links = organization ? organization_parents_ : deformation_parents_;
    auto it = links.find(id);
    return it == links.end() ? std::string{} : it->second;
}

std::vector<std::string> Document::affected_meshes(const std::string &id) const {
    std::vector<std::string> affected;
    for (const auto &mesh : mesh_order_) {
        auto cursor = mesh;
        while (!cursor.empty()) {
            if (cursor == id) {
                affected.push_back(mesh);
                break;
            }
            cursor = parent_of(cursor);
        }
    }
    return affected;
}

EditResult Document::set_parent(const std::string &id, const std::string &parent, bool organization) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Finish the position transaction first."));
    if (!get_mesh(id) && !get_deformer(id))
        return failed(Status::error("MISSING_OBJECT", "Child object does not exist."));
    if (!parent.empty() && !get_deformer(parent) && !(organization && get_mesh(parent)))
        return failed(Status::error("INVALID_PARENT", "Parent must exist and have the required type."));
    if (parent_of(id, organization) == parent)
        return {{}, {ChangeKind::none, {}, revision_, {}}, {}};
    auto &links = organization ? organization_parents_ : deformation_parents_;
    auto candidate = links;
    if (parent.empty())
        candidate.erase(id);
    else
        candidate[id] = parent;
    // Also check descendants: reparenting a subtree can exceed the depth limit.
    for (const auto &[child, ignored] : candidate) {
        std::unordered_set<std::string> seen;
        auto cursor = child;
        unsigned depth = 0;
        while (true) {
            if (!seen.insert(cursor).second)
                return failed(Status::error("PARENT_CYCLE", "Parent links must be acyclic."));
            auto it = candidate.find(cursor);
            if (it == candidate.end())
                break;
            if (++depth > 16)
                return failed(Status::error("PARENT_DEPTH", "At most 16 parent links are supported."));
            cursor = it->second;
        }
    }
    links = std::move(candidate);
    return changed(organization ? ChangeKind::metadata : ChangeKind::positions,
                   organization ? std::vector<std::string>{} : affected_meshes(id), {id});
}

EditResult Document::set_rotation(const std::string &id, Vec2 center, float angle) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Finish the position transaction first."));
    auto it = deformers_.find(id);
    if (it == deformers_.end() || it->second.kind != DeformerKind::rotation)
        return failed(Status::error("NOT_ROTATION", "Expected a Rotation deformer."));
    if (!std::isfinite(center.x) || !std::isfinite(center.y) || !std::isfinite(angle))
        return failed(Status::error("NON_FINITE", "Rotation values must be finite."));
    auto &d = it->second;
    if (d.center == center && d.angle_degrees == angle)
        return {{}, {ChangeKind::none, {}, revision_, {}}, {}};
    d.center = center;
    d.angle_degrees = angle;
    return changed(ChangeKind::positions, affected_meshes(id), {id});
}

EditResult Document::set_warp_points(const std::string &id, std::span<const Vec2> points) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Finish the position transaction first."));
    auto it = deformers_.find(id);
    if (it == deformers_.end() || it->second.kind != DeformerKind::warp)
        return failed(Status::error("NOT_WARP", "Expected a Warp deformer."));
    auto &d = it->second;
    if (points.size() != d.control_points.size())
        return failed(Status::error("INVALID_LENGTH", "Warp grid topology is fixed."));
    if (auto s = validate_positions(points); !s.ok())
        return failed(s);
    if (std::equal(points.begin(), points.end(), d.control_points.begin()))
        return {{}, {ChangeKind::none, {}, revision_, {}}, {}};
    d.control_points.assign(points.begin(), points.end());
    return changed(ChangeKind::positions, affected_meshes(id), {id});
}

Status Document::evaluate_legacy_mesh(const std::string &id, std::vector<Vec2> &out) const {
    const auto *mesh = get_mesh(id);
    if (!mesh)
        return Status::error("MISSING_MESH", "Mesh does not exist.");
    auto next = mesh->base_positions;
    for (auto parent = parent_of(id); !parent.empty(); parent = parent_of(parent)) {
        const auto &d = deformers_.at(parent);
        for (auto &p : next) {
            double px, py;
            if (d.kind == DeformerKind::rotation) {
                const double a = std::remainder(double(d.angle_degrees), 360.0) * std::numbers::pi / 180.0;
                const double x = double(p.x) - d.center.x, y = double(p.y) - d.center.y;
                px = d.center.x + std::cos(a) * x - std::sin(a) * y;
                py = d.center.y + std::sin(a) * x + std::cos(a) * y;
            } else {
                const double gx = (double(p.x) - d.origin.x) / d.size.x * d.columns;
                const double gy = (double(p.y) - d.origin.y) / d.size.y * d.rows;
                const auto x = uint32_t(std::clamp(std::floor(gx), 0.0, double(d.columns - 1)));
                const auto y = uint32_t(std::clamp(std::floor(gy), 0.0, double(d.rows - 1)));
                const double u = gx - x, v = gy - y;
                const auto &a = d.control_points[y * (d.columns + 1) + x];
                const auto &b = d.control_points[y * (d.columns + 1) + x + 1];
                const auto &c = d.control_points[(y + 1) * (d.columns + 1) + x];
                const auto &e = d.control_points[(y + 1) * (d.columns + 1) + x + 1];
                px = (1 - v) * ((1 - u) * a.x + u * b.x) + v * ((1 - u) * c.x + u * e.x);
                py = (1 - v) * ((1 - u) * a.y + u * b.y) + v * ((1 - u) * c.y + u * e.y);
            }
            if (!std::isfinite(px) || !std::isfinite(py) ||
                std::abs(px) > std::numeric_limits<float>::max() ||
                std::abs(py) > std::numeric_limits<float>::max())
                return Status::error("EVALUATION_OVERFLOW",
                                     "Deformation exceeds finite float32 coordinates.");
            p = {float(px), float(py)};
        }
    }
    out = std::move(next);
    return {};
}
} // namespace kasane
