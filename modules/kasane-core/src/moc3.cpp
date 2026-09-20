// SPDX-License-Identifier: MIT
#include <kasane/moc3.hpp>
#include <kasane/evaluation.hpp>
#include <algorithm>
#include <array>
#include <bit>
#include <cmath>
#include <limits>
#include <functional>
#include <stdexcept>
#include <string_view>
#include <unordered_set>

namespace kasane {
namespace {
struct Section {
    const char *name;
    size_t width;
    int count_index;
};

constexpr Section schema[] = {
#define SECTION(name, width, count) {name, width, count},
#include "moc3_sections.inc"
#undef SECTION
};
using Bytes = std::vector<uint8_t>;

void u32(Bytes &b, uint32_t v) {
    for (unsigned i = 0; i < 4; ++i)
        b.push_back(uint8_t(v >> (8 * i)));
}

void f32(Bytes &b, float v) {
    u32(b, std::bit_cast<uint32_t>(v));
}

void patch_u32(Bytes &b, size_t at, uint32_t v) {
    for (unsigned i = 0; i < 4; ++i)
        b.at(at + i) = uint8_t(v >> (8 * i));
}

int32_t checked(size_t n, const std::string &field) {
    if (n > size_t(INT32_MAX))
        throw std::length_error(field + ": exceeds signed 32-bit count");
    return int32_t(n);
}

struct Layout {
    std::array<uint32_t, 64> counts{};
    std::array<Bytes, std::size(schema)> data;

    Bytes &field(std::string_view name) {
        for (size_t i = 0; i < std::size(schema); ++i)
            if (name == schema[i].name)
                return data[i];
        throw std::logic_error("Unknown MOC3 field: " + std::string(name));
    }

    void integer(std::string_view name, int32_t v) { u32(field(name), uint32_t(v)); }

    void scalar(std::string_view name, float v) { f32(field(name), v); }

    Bytes finish() {
        for (auto n : counts)
            u32(data[0], n);
        // Reserve loader scratch after the 64-byte header and 160 offsets.
        // Both Core implementations revive pointers in this area in-place.
        // This is zeroed wire padding, never a serialized native structure.
        Bytes out(1984, 0);
        out[0] = 'M';
        out[1] = 'O';
        out[2] = 'C';
        out[3] = '3';
        out[4] = 5;
        for (size_t i = 0; i < std::size(schema); ++i) {
            const auto &s = schema[i];
            const size_t count = s.count_index < 0 ? 1 : counts[s.count_index];
            const size_t expected = count * s.width;
            // Only loader-owned runtime pointer slots may be synthesized.
            if (std::string_view(s.name).ends_with("_runtime"))
                data[i].resize(expected, 0);
            if (data[i].size() != expected)
                throw std::logic_error(std::string(s.name) + ": schema size mismatch");
            out.resize((out.size() + 63) & ~size_t(63), 0);
            if (out.size() > INT32_MAX || expected > size_t(INT32_MAX) - out.size())
                throw std::length_error(std::string(s.name) + ": exceeds Core signed 32-bit offset limit");
            patch_u32(out, 64 + i * 4, uint32_t(out.size()));
            out.insert(out.end(), data[i].begin(), data[i].end());
        }
        out.resize((out.size() + 63) & ~size_t(63), 0);
        // Unused offset-table entries remain zero.
        return out;
    }
};

Status error(const std::string &code, const std::string &id, const std::string &field,
             const std::string &why) {
    return Status::error(code, id + "." + field + ": " + why);
}
} // namespace

Status encode_moc3(const Document &doc, Moc3Artifact &out) {
    if (doc.transaction_active())
        return Status::error("TRANSACTION_ACTIVE", "Commit or cancel edits before export");
    DrawableFrame frame;
    if (auto s = evaluate_frame(doc, {}, frame); !s.ok())
        return s;
    const auto &drawables = frame.drawables;
    if (drawables.empty())
        return Status::error("EMPTY_MODEL", "At least one mesh is required");
    if (!std::isfinite(doc.canvas().height - doc.canvas().origin.y))
        return error("NON_FINITE", doc.id(), "canvas.origin", "runtime origin overflows float32");
    const auto representable = [](const std::string &id) {
        return !id.empty() && id.size() <= 63 &&
               std::all_of(id.begin(), id.end(), [](unsigned char c) { return c >= 0x20 && c <= 0x7e; });
    };
    for (const auto &d : drawables) {
        if (!representable(d.runtime_id))
            return error("UNREPRESENTABLE_ID", d.id, "runtime_id", "requires 1..63 printable ASCII bytes");
        if (d.positions.size() > 65536)
            return error("CAPACITY", d.id, "vertex_ids", "at most 65536 vertices");
    }
    for (const auto &id : doc.parameter_order())
        if (!representable(doc.get_parameter(id)->runtime_id))
            return error("UNREPRESENTABLE_ID", id, "runtime_id", "requires 1..63 printable ASCII bytes");
    const auto parts = doc.sorted_parts(), transforms = doc.sorted_transforms();
    for (const auto &id : parts)
        if (!representable(doc.get_part(id)->runtime_id))
            return error("UNREPRESENTABLE_ID", id, "runtime_id", "requires 1..63 printable ASCII bytes");
    for (const auto &id : transforms)
        if (!representable(doc.get_transform(id)->runtime_id))
            return error("UNREPRESENTABLE_ID", id, "runtime_id", "requires 1..63 printable ASCII bytes");

    struct BindingView {
        std::string id;
        const std::vector<BindingAxis> *axes;
    };

    std::vector<BindingView> all_bindings;
    for (const auto &id : doc.binding_order())
        all_bindings.push_back({id, &doc.get_binding(id)->axes});
    for (const auto &id : doc.scene_binding_order())
        all_bindings.push_back({id, &doc.get_scene_binding(id)->axes});
    auto index = [](const std::vector<std::string> &ids, const std::string &id) -> int32_t {
        return id.empty() ? -1 : int32_t(std::find(ids.begin(), ids.end(), id) - ids.begin());
    };
    try {
        Layout l;
        const auto n = checked(drawables.size(), "art_meshes");
        l.counts[4] = l.counts[19] = n;
        l.counts[12] = checked(all_bindings.size() + 1, "bindings"); // Binding 0 is static.
        l.counts[5] = checked(doc.parameter_order().size(), "parameters");
        l.counts[0] = checked(parts.size(), "parts");
        l.counts[1] = checked(transforms.size(), "deformers");
        l.counts[18] = checked(parts.size() + 1, "groups"); // Root draw group.
        const auto c = doc.canvas();
        auto &canvas = l.field("canvas_info");
        f32(canvas, c.pixels_per_unit);
        f32(canvas, c.origin.x);
        f32(canvas, c.height - c.origin.y);
        f32(canvas, c.width);
        f32(canvas, c.height);
        canvas.resize(24, 0);
        canvas[20] = 1; // Positions/winding already use runtime Y direction.
        l.integer("binding_src.key_table_idx_off", 0);
        l.integer("binding_src.key_table_idx_len", 0);
        std::unordered_map<std::string, std::vector<int32_t>> table_indices;
        for (const auto &b : all_bindings)
            table_indices[b.id].resize(b.axes->size());
        int32_t table_count = 0;
        for (const auto &id : doc.parameter_order()) {
            const auto &p = *doc.get_parameter(id);
            auto &ids = l.field("param_src.id");
            auto start = ids.size();
            ids.insert(ids.end(), p.runtime_id.begin(), p.runtime_id.end());
            ids.resize(start + 64, 0);
            l.scalar("param_src.maximum_value", p.maximum);
            l.scalar("param_src.minimum_value", p.minimum);
            l.scalar("param_src.default_value", p.default_value);
            l.integer("param_src.repeat", 0);
            l.integer("param_src.decimal_places", p.decimal_places);
            l.integer("param_src.type", 0);
            l.integer("param_src.blend_key_table_off", 0);
            l.integer("param_src.blend_key_table_len", 0);
            const auto first = table_count;
            std::vector<float> union_keys;
            for (const auto &binding : all_bindings) {
                const auto &bid = binding.id;
                for (size_t a = 0; a < binding.axes->size(); ++a) {
                    const auto &axis = (*binding.axes)[a];
                    if (axis.parameter_id != id)
                        continue;
                    table_indices[bid][a] = table_count++;
                    l.integer("key_table_src.keys_off",
                              checked(l.field("keys_src.key").size() / 4, bid + ".keys_off"));
                    l.integer("key_table_src.keys_len", checked(axis.keys.size(), bid + ".keys_len"));
                    for (float k : axis.keys)
                        l.scalar("keys_src.key", k);
                    union_keys.insert(union_keys.end(), axis.keys.begin(), axis.keys.end());
                }
            }
            l.integer("param_src.key_table_off", first);
            l.integer("param_src.key_table_len", table_count - first);
            std::sort(union_keys.begin(), union_keys.end());
            union_keys.erase(std::unique(union_keys.begin(), union_keys.end()), union_keys.end());
            l.integer("param_keys_src.keys_off",
                      checked(l.field("keys_src.key").size() / 4, id + ".keys_off"));
            l.integer("param_keys_src.keys_len", checked(union_keys.size(), id + ".keys_len"));
            for (float k : union_keys)
                l.scalar("keys_src.key", k);
        }
        l.counts[13] = table_count;
        l.counts[14] = checked(l.field("keys_src.key").size() / 4, "keys");
        std::unordered_map<std::string, int32_t> binding_indices;
        for (const auto &b : all_bindings) {
            const auto &id = b.id;
            binding_indices[id] = checked(binding_indices.size() + 1, "binding_index");
            l.integer("binding_src.key_table_idx_off",
                      checked(l.field("key_table_idx_src.idx").size() / 4, id + ".axes_offset"));
            l.integer("binding_src.key_table_idx_len", checked(table_indices[id].size(), id + ".axes_count"));
            for (auto index : table_indices[id])
                l.integer("key_table_idx_src.idx", index);
        }
        l.counts[11] = checked(l.field("key_table_idx_src.idx").size() / 4, "key_table_idx");
        int32_t keyform_offset = 0;
        auto write_id = [&](const std::string &field, const std::string &value) {
            auto &bytes = l.field(field);
            auto start = bytes.size();
            bytes.insert(bytes.end(), value.begin(), value.end());
            bytes.resize(start + 64, 0);
        };
        auto write_colors = [&](const std::string &prefix, const Appearance &appearance) {
            auto offset = checked(l.field("keyform_mul_color_src.r").size() / 4, "colors");
            l.integer(prefix + ".key_mul_color_off", offset);
            l.integer(prefix + ".key_scr_color_off", offset);
            const char *channels[] = {"r", "g", "b"};
            for (int c = 0; c < 3; ++c) {
                l.scalar(std::string("keyform_mul_color_src.") + channels[c], appearance.multiply[c]);
                l.scalar(std::string("keyform_scr_color_src.") + channels[c], appearance.screen[c]);
            }
        };
        auto write_positions = [&](const std::string &prefix, const std::string &parent,
                                   const std::vector<Vec2> &positions) {
            std::vector<Vec2> converted;
            auto status = to_parent_positions(doc, parent, positions, converted);
            if (!status.ok())
                throw std::length_error(status.message);
            l.integer(prefix + ".key_pos_off", checked(l.field("key_pos_src.xy").size() / 4, "positions"));
            for (auto p : converted) {
                l.scalar("key_pos_src.xy", p.x);
                l.scalar("key_pos_src.xy", p.y);
            }
        };
        for (const auto &id : parts) {
            auto &part = *doc.get_part(id);
            auto b = doc.binding_for_scene(id);
            int count = b ? checked(b->keyforms.size(), id) : 1;
            int stored = b ? std::max(count, int(1u << b->axes.size())) : 1;
            write_id("part_src.id", part.runtime_id);
            l.integer("part_src.binding_idx", b ? binding_indices.at(b->id) : 0);
            l.integer("part_src.keyform_off", l.counts[6]);
            l.integer("part_src.key_len", count);
            l.integer("part_src.visible", 1);
            l.integer("part_src.enable", part.enabled);
            l.integer("part_src.parent_part_idx", index(parts, part.parent_id));
            for (int k = 0; k < stored; ++k)
                l.scalar("part_key_src.draw_order",
                         b ? b->keyforms[std::min(k, count - 1)].draw_order : part.draw_order);
            l.counts[6] += stored;
        }
        for (const auto &id : transforms) {
            auto &t = *doc.get_transform(id);
            auto b = doc.binding_for_scene(id);
            bool warp = t.kind == TransformKind::warp;
            std::string prefix = warp ? "warp" : "rotation";
            int count = b ? checked(b->keyforms.size(), id) : 1;
            int stored = b ? std::max(count, int(1u << b->axes.size())) : 1;
            write_id("deformer_src.id", t.runtime_id);
            l.integer("deformer_src.binding_idx", b ? binding_indices.at(b->id) : 0);
            l.integer("deformer_src.visible", 1);
            l.integer("deformer_src.enable", t.enabled);
            l.integer("deformer_src.parent_part_idx", index(parts, t.part_id));
            l.integer("deformer_src.parent_deformer_idx", index(transforms, t.parent_id));
            l.integer("deformer_src.type", warp ? 0 : 1);
            l.integer("deformer_src.local_idx", l.counts[warp ? 2 : 3]++);
            l.integer(prefix + "_src.binding_idx", b ? binding_indices.at(b->id) : 0);
            l.integer(prefix + "_src.keyform_off", l.counts[warp ? 7 : 8]);
            l.integer(prefix + "_src.key_len", count);
            l.integer(prefix + "_src.key_color_off",
                      checked(l.field("keyform_mul_color_src.r").size() / 4, id));
            if (warp) {
                l.integer("warp_src.vertex_count", checked(t.points.size(), id));
                l.integer("warp_src.row", t.rows);
                l.integer("warp_src.col", t.columns);
                l.integer("warp_src.quad_transform", t.quad);
            } else
                l.scalar("rotation_src.base_angle", t.base_angle);
            for (int k = 0; k < stored; ++k) {
                auto f = b ? b->keyforms[std::min(k, count - 1)]
                           : SceneKeyform{{}, t.points, t.rotation, t.appearance, 0};
                l.scalar(prefix + "_key_src.opacity", f.appearance.opacity);
                write_colors(prefix + "_key_src", f.appearance);
                if (warp)
                    write_positions("warp_key_src", t.parent_id, f.positions);
                else {
                    std::vector<Vec2> origin;
                    if (auto status = to_parent_positions(
                            doc, t.parent_id, std::span<const Vec2>(&f.rotation.origin, 1), origin);
                        !status.ok())
                        return status;
                    l.scalar("rotation_key_src.origin_x", origin[0].x);
                    l.scalar("rotation_key_src.origin_y", origin[0].y);
                    l.scalar("rotation_key_src.angle", f.rotation.angle);
                    l.scalar("rotation_key_src.scale", f.rotation.scale);
                    l.integer("rotation_key_src.reflect_x", f.rotation.reflect_x);
                    l.integer("rotation_key_src.reflect_y", f.rotation.reflect_y);
                }
            }
            l.counts[warp ? 7 : 8] += stored;
        }
        // One group per Part, in parent-first order, plus root. Bounds include
        // every keyform, not merely the currently evaluated order.
        std::vector<std::string> groups{""};
        groups.insert(groups.end(), parts.begin(), parts.end());
        std::function<int(const std::string &)> descendant_count = [&](const auto &parent) {
            int count = 0;
            for (const auto &id : doc.mesh_order())
                if (doc.get_mesh(id)->part_id == parent)
                    ++count;
            for (const auto &id : parts)
                if (doc.get_part(id)->parent_id == parent)
                    count += descendant_count(id);
            return count;
        };
        for (const auto &parent : groups) {
            auto first = l.field("draw_group_obj_src.idx").size() / 4;
            float lo = 0, hi = 0;
            for (int32_t i = 0; i < n; ++i) {
                auto &m = *doc.get_mesh(drawables[i].id);
                if (m.part_id != parent)
                    continue;
                float order = m.draw_order.value_or(float(i));
                lo = std::min(lo, order);
                hi = std::max(hi, order);
                if (auto b = doc.binding_for_mesh(m.id))
                    for (auto &f : b->keyforms) {
                        lo = std::min(lo, f.draw_order.value_or(order));
                        hi = std::max(hi, f.draw_order.value_or(order));
                    }
                l.integer("draw_group_obj_src.type", 0);
                l.integer("draw_group_obj_src.idx", i);
                l.integer("draw_group_obj_src.self_group_idx", -1);
            }
            for (size_t i = 0; i < parts.size(); ++i) {
                auto &p = *doc.get_part(parts[i]);
                if (p.parent_id != parent)
                    continue;
                lo = std::min(lo, p.draw_order);
                hi = std::max(hi, p.draw_order);
                if (auto b = doc.binding_for_scene(p.id))
                    for (auto &f : b->keyforms) {
                        lo = std::min(lo, f.draw_order);
                        hi = std::max(hi, f.draw_order);
                    }
                l.integer("draw_group_obj_src.type", 1);
                l.integer("draw_group_obj_src.idx", int32_t(i));
                l.integer("draw_group_obj_src.self_group_idx", int32_t(i + 1));
            }
            l.integer("draw_group_src.obj_off", checked(first, "draw_group"));
            l.integer("draw_group_src.obj_len",
                      checked(l.field("draw_group_obj_src.idx").size() / 4 - first, "draw_group"));
            l.integer("draw_group_src.obj_total_count", descendant_count(parent));
            l.integer("draw_group_src.min_order", int32_t(std::floor(lo)));
            l.integer("draw_group_src.max_order", int32_t(std::ceil(hi)));
        }
        l.counts[19] = checked(l.field("draw_group_obj_src.idx").size() / 4, "draw_items");
        for (int32_t i = 0; i < n; ++i) {
            const auto &d = drawables[i];
            auto &ids = l.field("art_mesh_src.id");
            ids.insert(ids.end(), d.runtime_id.begin(), d.runtime_id.end());
            ids.resize(size_t(i + 1) * 64, 0);
            const auto &mesh = *doc.get_mesh(d.id);
            const auto *binding = doc.binding_for_mesh(d.id);
            const auto key_count = binding ? checked(binding->keyforms.size(), binding->id + ".keyforms") : 1;
            // Purism runtime allocates 2^axes gather scratch. One-key axes do
            // not add combinations, but verifier still checks the maximum span.
            const auto stored_count = binding ? std::max(key_count, int32_t(1u << binding->axes.size())) : 1;
            l.integer("art_mesh_src.binding_idx", binding ? binding_indices.at(binding->id) : 0);
            l.integer("art_mesh_src.keyform_off", keyform_offset);
            l.integer("art_mesh_src.key_len", key_count);
            l.integer("art_mesh_src.key_color_off",
                      checked(l.field("keyform_mul_color_src.r").size() / 4, d.id));
            l.integer("art_mesh_src.visible", 1);
            l.integer("art_mesh_src.enable", mesh.enabled);
            l.integer("art_mesh_src.parent_part_idx", index(parts, mesh.part_id));
            l.integer("art_mesh_src.parent_deformer_idx", index(transforms, mesh.deformer_id));
            l.integer("art_mesh_src.texture_no", d.texture_slot);
            l.field("art_mesh_src.drawable_flag")
                .push_back((mesh.double_sided ? 4 : 0) | (mesh.inverted_mask ? 8 : 0) |
                           (mesh.blend_mode == BlendMode::additive         ? 1
                            : mesh.blend_mode == BlendMode::multiplicative ? 2
                                                                           : 0));
            l.integer("art_mesh_src.vertex_count", checked(d.positions.size(), d.id + ".vertex_count"));
            l.integer("art_mesh_src.uv_off", checked(l.field("uv_src.xy").size() / 4, d.id + ".uv_off"));
            l.integer("art_mesh_src.idx_off", checked(l.field("idx_src.idx").size() / 2, d.id + ".idx_off"));
            l.integer("art_mesh_src.idx_len", checked(d.indices.size(), d.id + ".idx_len"));
            l.integer("art_mesh_src.mask_off", checked(l.field("mask_src.art_mesh_idx").size() / 4, d.id));
            l.integer("art_mesh_src.mask_len", checked(mesh.masks.size(), d.id));
            for (const auto &mask : mesh.masks)
                l.integer("mask_src.art_mesh_idx", index(doc.mesh_order(), mask));
            for (int32_t k = 0; k < stored_count; ++k) {
                const auto &positions =
                    binding ? binding->keyforms[std::min(k, key_count - 1)].positions : mesh.base_positions;
                const auto appearance =
                    binding ? binding->keyforms[std::min(k, key_count - 1)].appearance : mesh.appearance;
                const auto base_order = mesh.draw_order.value_or(float(i));
                l.scalar("art_mesh_key_src.opacity", appearance.opacity);
                l.scalar("art_mesh_key_src.draw_order",
                         binding
                             ? binding->keyforms[std::min(k, key_count - 1)].draw_order.value_or(base_order)
                             : base_order);
                write_positions("art_mesh_key_src", mesh.deformer_id, positions);
                write_colors("art_mesh_key_src", appearance);
                keyform_offset = checked(size_t(keyform_offset) + 1, d.id + ".keyforms");
            }
            for (auto p : d.uvs) {
                l.scalar("uv_src.xy", p.x);
                l.scalar("uv_src.xy", p.y);
            }
            for (auto v : d.indices) {
                auto &b = l.field("idx_src.idx");
                b.push_back(uint8_t(v));
                b.push_back(uint8_t(v >> 8));
            }
        }
        l.counts[9] = keyform_offset;
        l.counts[23] = l.counts[24] = checked(l.field("keyform_mul_color_src.r").size() / 4, "colors");
        l.counts[17] = checked(l.field("mask_src.art_mesh_idx").size() / 4, "masks");
        l.counts[10] = checked(l.field("key_pos_src.xy").size() / 4, "keyform_pos");
        l.counts[15] = checked(l.field("uv_src.xy").size() / 4, "uvs");
        l.counts[16] = checked(l.field("idx_src.idx").size() / 2, "idx");
        Moc3Artifact result;
        result.bytes = l.finish();
        result.model3_json = "{\n  \"Version\": 3,\n  \"FileReferences\": {\n    \"Moc\": \"model.moc3\",\n  "
                             "  \"Textures\": [";
        for (const auto &id : doc.asset_order()) {
            const auto &asset = *doc.get_asset(id);
            const auto path = "textures/" + std::to_string(result.textures.size()) + ".png";
            if (!result.textures.empty())
                result.model3_json += ",";
            result.model3_json += "\n      \"" + path + "\"";
            result.textures.push_back({id, asset.source, path, asset.width, asset.height});
        }
        result.model3_json += "\n    ]\n  }\n}\n";
        out = std::move(result);
        return {};
    } catch (const std::length_error &e) {
        return Status::error("CAPACITY", e.what());
    } catch (const std::logic_error &e) {
        return Status::error("CODEC_LAYOUT", e.what());
    }
}
} // namespace kasane
