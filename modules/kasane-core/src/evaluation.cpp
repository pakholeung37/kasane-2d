// SPDX-License-Identifier: MIT
#include <kasane/evaluation.hpp>
#include <PurismKeyform.h>
#include <PurismDeformer.h>
#include <algorithm>
#include <cmath>
#include <functional>

namespace kasane {
Status to_runtime_positions(Canvas c, std::span<const Vec2> positions, std::vector<Vec2> &out) {
    std::vector<Vec2> result;
    result.reserve(positions.size());
    for (auto p : positions) {
        Vec2 q{float((double(p.x) - c.origin.x) / c.pixels_per_unit),
               float((double(c.origin.y) - p.y) / c.pixels_per_unit)};
        if (!std::isfinite(q.x) || !std::isfinite(q.y))
            return Status::error("NON_FINITE", "Position conversion overflows float32");
        result.push_back(q);
    }
    out = std::move(result);
    return {};
}

Status to_parent_positions(const Document &doc, const std::string &parent, std::span<const Vec2> positions,
                           std::vector<Vec2> &out) {
    if (parent.empty())
        return to_runtime_positions(doc.canvas(), positions, out);
    if (!doc.get_transform(parent))
        return Status::error("MISSING_TRANSFORM", parent);
    if (auto s = validate_positions(positions); !s.ok())
        return s;
    out.assign(positions.begin(), positions.end());
    return {};
}

namespace {
struct Selection {
    std::vector<int32_t> indices{0};
    std::vector<float> weights{1};
    bool enabled = true;
};

Selection select(const Document &doc, const PreviewValues &values, const std::vector<BindingAxis> &binding) {
    Selection s;
    std::vector<psm__key_axis> axes;
    for (const auto &axis : binding) {
        const auto &p = *doc.get_parameter(axis.parameter_id);
        float epsilon = std::pow(0.1f, float(p.decimal_places));
        const auto segment = psm__find_key_segment(values.at(p.id), axis.keys.data(),
                                                   int32_t(axis.keys.size()), epsilon, epsilon * 1.5f);
        s.enabled &= !segment.is_outside;
        axes.push_back({segment.index, int32_t(axis.keys.size()), segment.weight});
    }
    s.indices.resize(size_t(1) << axes.size());
    s.weights.resize(s.indices.size());
    const auto count =
        psm__key_combinations(int32_t(axes.size()), axes.data(), s.indices.data(), s.weights.data());
    s.indices.resize(count);
    s.weights.resize(count);
    return s;
}

Appearance blend_appearance(const Selection &s, const std::function<Appearance(int)> &get) {
    Appearance a;
    a.opacity = 0;
    a.multiply = {0, 0, 0};
    for (size_t k = 0; k < s.indices.size(); ++k) {
        const auto f = get(s.indices[k]);
        auto w = s.weights[k];
        a.opacity += f.opacity * w;
        for (int c = 0; c < 3; ++c) {
            a.multiply[c] += f.multiply[c] * w;
            a.screen[c] += f.screen[c] * w;
        }
    }
    return a;
}

void inherit(Appearance &child, const Appearance &parent) {
    child.opacity *= parent.opacity;
    for (int c = 0; c < 3; ++c) {
        child.multiply[c] *= parent.multiply[c];
        child.screen[c] = child.screen[c] + parent.screen[c] - child.screen[c] * parent.screen[c];
    }
}

Status blend_positions(const Document &doc, const std::string &parent, const Selection &s,
                       const std::function<const std::vector<Vec2> &(int)> &get, std::vector<Vec2> &out) {
    std::vector<std::vector<float>> data(s.indices.size());
    std::vector<float *> pointers;
    size_t size = 0;
    for (size_t k = 0; k < s.indices.size(); ++k) {
        std::vector<Vec2> converted;
        if (auto e = to_parent_positions(doc, parent, get(s.indices[k]), converted); !e.ok())
            return e;
        size = converted.size();
        for (auto p : converted) {
            data[k].push_back(p.x);
            data[k].push_back(p.y);
        }
        pointers.push_back(data[k].data());
    }
    if (size > size_t(INT32_MAX) / 2)
        return Status::error("CAPACITY", "positions");
    std::vector<float> xy(size * 2);
    psm__blend_vectors(pointers.data(), s.weights.data(), int32_t(s.indices.size()), int32_t(xy.size()),
                       xy.data());
    out.resize(size);
    for (size_t i = 0; i < size; ++i)
        out[i] = {xy[2 * i], xy[2 * i + 1]};
    return validate_positions(out);
}

struct TransformState {
    const Transform *source = nullptr;
    RotationPose pose;
    std::vector<float> points;
    Appearance appearance;
    float inherited_scale = 1;
    bool enabled = true;

    psm__vec2 point(psm__vec2 p) const {
        float in[2]{p.x, p.y}, out[2];
        if (source->kind == TransformKind::warp)
            psm__warp_points(source->rows, source->columns, source->quad, points.data(), in, out, 1);
        else
            psm__rotation_points(source->base_angle, pose.angle, pose.scale,
                                 psm__v2(pose.origin.x, pose.origin.y), pose.reflect_x, pose.reflect_y, in,
                                 out, 1);
        return {out[0], out[1]};
    }

    static psm__vec2 callback(void *self, psm__vec2 p) {
        return static_cast<TransformState *>(self)->point(p);
    }
};
} // namespace

Status evaluate_frame(const Document &doc, const PreviewValues &preview, DrawableFrame &out) {
    if (!doc.initialized())
        return Status::error("NOT_INITIALIZED", "Initialize Document first");
    if (!doc.deformer_order().empty())
        return Status::error("UNSUPPORTED_FEATURE",
                             doc.deformer_order().front() + ": migrate legacy deformers explicitly");
    if (doc.mesh_order().size() > 16777216 || doc.asset_order().size() > size_t(INT32_MAX))
        return Status::error("CAPACITY", "Object count");
    DrawableFrame frame;
    frame.canvas = doc.canvas();
    frame.source_revision = doc.revision();
    PreviewValues values;
    for (const auto &[id, v] : preview) {
        if (!doc.get_parameter(id))
            return Status::error("MISSING_PARAMETER", id);
        if (!std::isfinite(v))
            return Status::error("NON_FINITE", id + ".preview_value");
    }
    for (const auto &id : doc.parameter_order()) {
        auto &p = *doc.get_parameter(id);
        auto it = preview.find(id);
        float requested = it == preview.end() ? p.default_value : it->second;
        float v = std::clamp(requested, p.minimum, p.maximum);
        frame.parameters.push_back({id, requested, v, requested != v});
        values[id] = v;
    }
    std::unordered_map<std::string, bool> enabled_parts;
    std::unordered_map<std::string, int> part_orders;
    for (const auto &id : doc.sorted_parts()) {
        const auto &p = *doc.get_part(id);
        bool enabled = p.enabled && (p.parent_id.empty() || enabled_parts.at(p.parent_id));
        float order = p.draw_order;
        if (auto b = doc.binding_for_scene(id)) {
            auto s = select(doc, values, b->axes);
            enabled &= s.enabled;
            if (s.enabled) {
                order = 0;
                for (size_t k = 0; k < s.indices.size(); ++k)
                    order += b->keyforms[s.indices[k]].draw_order * s.weights[k];
            }
        }
        enabled_parts[id] = enabled;
        part_orders[id] = psm__f32_to_i32(order + 0.001f);
    }
    std::unordered_map<std::string, TransformState> transforms;
    for (const auto &id : doc.sorted_transforms()) {
        const auto &t = *doc.get_transform(id);
        TransformState state;
        state.source = &t;
        state.pose = t.rotation;
        state.appearance = t.appearance;
        state.enabled = t.enabled && (t.part_id.empty() || enabled_parts.at(t.part_id));
        std::vector<Vec2> points = t.points;
        auto b = doc.binding_for_scene(id);
        Selection selection;
        if (b)
            selection = select(doc, values, b->axes);
        state.enabled &= selection.enabled;
        if (!t.parent_id.empty())
            state.enabled &= transforms.at(t.parent_id).enabled;
        if (state.enabled) {
            if (b) {
                state.appearance =
                    blend_appearance(selection, [&](int i) { return b->keyforms[i].appearance; });
                if (t.kind == TransformKind::rotation) {
                    state.pose = {};
                    state.pose.scale = 0;
                    auto first = b->keyforms[selection.indices[0]].rotation;
                    state.pose.reflect_x = first.reflect_x;
                    state.pose.reflect_y = first.reflect_y;
                    for (size_t k = 0; k < selection.indices.size(); ++k) {
                        auto p = b->keyforms[selection.indices[k]].rotation;
                        std::vector<Vec2> origin;
                        if (auto e = to_parent_positions(doc, t.parent_id,
                                                         std::span<const Vec2>(&p.origin, 1), origin);
                            !e.ok())
                            return e;
                        auto w = selection.weights[k];
                        state.pose.origin.x += origin[0].x * w;
                        state.pose.origin.y += origin[0].y * w;
                        state.pose.angle += p.angle * w;
                        state.pose.scale += p.scale * w;
                    }
                }
            }
            if (t.kind == TransformKind::warp) {
                if (auto e = blend_positions(
                        doc, t.parent_id, selection,
                        [&](int i) -> const std::vector<Vec2> & {
                            return b ? b->keyforms[i].positions : t.points;
                        },
                        points);
                    !e.ok())
                    return e;
            } else if (!b) {
                std::vector<Vec2> origin;
                if (auto e = to_parent_positions(doc, t.parent_id,
                                                 std::span<const Vec2>(&t.rotation.origin, 1), origin);
                    !e.ok())
                    return e;
                state.pose.origin = origin[0];
            }
            state.inherited_scale = t.kind == TransformKind::rotation ? state.pose.scale : 1;
            if (!t.parent_id.empty()) {
                auto &parent = transforms.at(t.parent_id);
                inherit(state.appearance, parent.appearance);
                if (t.kind == TransformKind::warp) {
                    for (auto &p : points) {
                        auto q = parent.point({p.x, p.y});
                        p = {q.x, q.y};
                    }
                    state.inherited_scale = parent.inherited_scale;
                } else {
                    psm__vec2 origin{state.pose.origin.x, state.pose.origin.y};
                    state.pose.angle +=
                        psm__rotation_parent_angle(parent.source->kind == TransformKind::rotation, &parent,
                                                   TransformState::callback, &origin);
                    state.pose.origin = {origin.x, origin.y};
                    state.pose.scale *= parent.inherited_scale;
                    state.inherited_scale = state.pose.scale;
                }
            }
            for (auto p : points) {
                state.points.push_back(p.x);
                state.points.push_back(p.y);
            }
            if (auto s = validate_positions(points); !s.ok())
                return Status::error(s.code, id + ".evaluated_points");
        }
        transforms[id] = std::move(state);
    }
    for (const auto &id : doc.mesh_order()) {
        if (!doc.parent_of(id).empty() || !doc.parent_of(id, true).empty())
            return Status::error("UNSUPPORTED_FEATURE", id + ": legacy parent relation");
        const auto &mesh = *doc.get_mesh(id);
        Drawable d;
        d.id = id;
        d.runtime_id = mesh.runtime_id;
        d.texture_asset_id = mesh.texture_asset_id;
        d.blend_mode = mesh.blend_mode;
        d.double_sided = mesh.double_sided;
        d.inverted_mask = mesh.inverted_mask;
        d.masks = mesh.masks;
        float order = mesh.draw_order.value_or(float(frame.drawables.size()));
        Appearance appearance = mesh.appearance;
        auto slot = std::find(doc.asset_order().begin(), doc.asset_order().end(), mesh.texture_asset_id);
        if (slot == doc.asset_order().end())
            return Status::error("MISSING_ASSET", id);
        d.texture_slot = int32_t(slot - doc.asset_order().begin());
        d.visible = mesh.enabled && (mesh.part_id.empty() || enabled_parts.at(mesh.part_id)) &&
                    (mesh.deformer_id.empty() || transforms.at(mesh.deformer_id).enabled);
        for (auto uv : mesh.uvs)
            d.uvs.push_back({uv.x, 1 - uv.y});
        if (auto s = doc.render_indices(id, d.indices); !s.ok())
            return s;
        for (size_t i = 0; i < d.indices.size(); i += 3)
            std::swap(d.indices[i + 1], d.indices[i + 2]);
        auto b = doc.binding_for_mesh(id);
        Selection selection;
        if (b)
            selection = select(doc, values, b->axes);
        d.visible &= selection.enabled;
        d.enabled = d.visible;
        if (d.visible) {
            if (auto e = blend_positions(
                    doc, mesh.deformer_id, selection,
                    [&](int i) -> const std::vector<Vec2> & {
                        return b ? b->keyforms[i].positions : mesh.base_positions;
                    },
                    d.positions);
                !e.ok())
                return Status::error(e.code, id + ": " + e.message);
            if (b) {
                appearance = blend_appearance(selection, [&](int i) { return b->keyforms[i].appearance; });
                float sum = 0;
                for (size_t k = 0; k < selection.indices.size(); ++k)
                    sum +=
                        b->keyforms[selection.indices[k]].draw_order.value_or(order) * selection.weights[k];
                order = sum;
            }
            if (!mesh.deformer_id.empty()) {
                auto &parent = transforms.at(mesh.deformer_id);
                inherit(appearance, parent.appearance);
                for (auto &p : d.positions) {
                    auto q = parent.point({p.x, p.y});
                    p = {q.x, q.y};
                }
            }
            if (auto e = validate_positions(d.positions); !e.ok())
                return Status::error(e.code, id + ".evaluated_positions");
        } else {
            d.positions.resize(mesh.vertex_ids.size());
            appearance = Appearance{};
            order = 0;
        }
        d.draw_order = psm__f32_to_i32(order + 0.001f);
        d.opacity = appearance.opacity;
        d.visible &= d.opacity != 0;
        for (int c = 0; c < 3; ++c) {
            d.multiply_color[c] = appearance.multiply[c];
            d.screen_color[c] = appearance.screen[c];
        }
        frame.drawables.push_back(std::move(d));
    }
    // Part groups preserve contiguous subtrees. Ties retain file item order:
    // meshes first, followed by parts in parent-first order.
    int rank = 0;
    auto parts = doc.sorted_parts();
    std::function<void(const std::string &)> sort_group = [&](const auto &parent) {
        struct Item {
            int order;
            int mesh;
            std::string part;
        };
        std::vector<Item> items;
        for (size_t i = 0; i < frame.drawables.size(); ++i)
            if (doc.get_mesh(frame.drawables[i].id)->part_id == parent)
                items.push_back({frame.drawables[i].draw_order, int(i), {}});
        for (const auto &id : parts)
            if (doc.get_part(id)->parent_id == parent)
                items.push_back({part_orders.at(id), -1, id});
        float minimum = 0;
        for (size_t i = 0; i < frame.drawables.size(); ++i) {
            const auto &m = *doc.get_mesh(frame.drawables[i].id);
            if (m.part_id != parent)
                continue;
            float base = m.draw_order.value_or(float(i));
            minimum = std::min(minimum, base);
            if (auto b = doc.binding_for_mesh(m.id))
                for (auto &f : b->keyforms)
                    minimum = std::min(minimum, f.draw_order.value_or(base));
        }
        for (auto &id : parts) {
            const auto &p = *doc.get_part(id);
            if (p.parent_id != parent)
                continue;
            minimum = std::min(minimum, p.draw_order);
            if (auto b = doc.binding_for_scene(id))
                for (auto &f : b->keyforms)
                    minimum = std::min(minimum, f.draw_order);
        }
        for (auto &item : items)
            if (item.mesh >= 0 ? !frame.drawables[item.mesh].enabled : !enabled_parts.at(item.part))
                item.order = int(std::floor(minimum));
        std::stable_sort(items.begin(), items.end(),
                         [](const auto &a, const auto &b) { return a.order < b.order; });
        for (const auto &i : items)
            if (i.mesh >= 0)
                frame.drawables[i.mesh].render_order = rank++;
            else
                sort_group(i.part);
    };
    sort_group("");
    out = std::move(frame);
    return {};
}
} // namespace kasane
