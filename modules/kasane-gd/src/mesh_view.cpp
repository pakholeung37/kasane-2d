// SPDX-License-Identifier: MIT
#include "mesh_view.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/os.hpp>
#include <godot_cpp/classes/rendering_server.hpp>
#include <godot_cpp/variant/packed_vector3_array.hpp>
#include <algorithm>
#include <cstring>

using namespace godot;

namespace kasane_gd {
void KasaneMeshView::_bind_methods() {
    ClassDB::bind_method(D_METHOD("initialize", "positions", "uvs", "indices", "texture"),
                         &KasaneMeshView::initialize);
    ClassDB::bind_method(D_METHOD("update_positions", "positions"), &KasaneMeshView::update_positions);
    ClassDB::bind_method(D_METHOD("clear"), &KasaneMeshView::clear);
    ClassDB::bind_method(D_METHOD("get_render_stats"), &KasaneMeshView::get_render_stats);
    ClassDB::bind_method(D_METHOD("get_positions_snapshot"), &KasaneMeshView::get_positions_snapshot);
}

Dictionary KasaneMeshView::initialize(const PackedVector2Array &positions, const PackedVector2Array &uvs,
                                      const PackedInt32Array &indices, const Ref<Texture2D> &texture) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Rendering calls require the main thread.");
    if (texture.is_null() || texture->get_width() <= 0 || texture->get_height() <= 0)
        return error("INVALID_TEXTURE", "Supply a loaded Texture2D.");
    auto next = vectors(positions);
    auto next_uvs = vectors(uvs);
    std::vector<uint32_t> dense;
    for (int64_t i = 0; i < indices.size(); ++i) {
        if (indices[i] < 0)
            return error("INVALID_INDEX", "Indices cannot be negative.");
        dense.push_back(indices[i]);
    }
    if (auto status = kasane::validate_render_mesh(next, next_uvs, dense); !status.ok())
        return result(status);
    Ref<ArrayMesh> next_surface;
    next_surface.instantiate();
    PackedVector3Array gpu_positions;
    gpu_positions.resize(next.size());
    for (size_t i = 0; i < next.size(); ++i)
        gpu_positions.set(i, {next[i].x, next[i].y, 0});
    Array arrays;
    arrays.resize(Mesh::ARRAY_MAX);
    arrays[Mesh::ARRAY_VERTEX] = gpu_positions;
    arrays[Mesh::ARRAY_TEX_UV] = uvs;
    arrays[Mesh::ARRAY_INDEX] = indices;
    next_surface->add_surface_from_arrays(Mesh::PRIMITIVE_TRIANGLES, arrays, {}, {},
                                          Mesh::ARRAY_FLAG_USE_DYNAMIC_UPDATE);
    surface_ = next_surface;
    positions_ = std::move(next);
    set_mesh(surface_);
    set_texture(texture);
    set_texture_filter(CanvasItem::TEXTURE_FILTER_NEAREST);
    set_texture_repeat(CanvasItem::TEXTURE_REPEAT_DISABLED);
    update_bounds();
    ++creations_;
    return result({});
}

Dictionary KasaneMeshView::update_positions(const PackedVector2Array &positions) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Rendering calls require the main thread.");
    if (surface_.is_null())
        return error("NOT_INITIALIZED", "Initialize the mesh first.");
    if (positions.size() != static_cast<int64_t>(positions_.size()))
        return error("INVALID_LENGTH", "Position updates must preserve topology and vertex count.");
    auto next = vectors(positions);
    if (auto status = kasane::validate_positions(next); !status.ok())
        return result(status);
    PackedByteArray upload;
    upload.resize(next.size() * 3 * sizeof(float));
    auto *bytes = upload.ptrw();
    for (size_t i = 0; i < next.size(); ++i) {
        const float vertex[3] = {next[i].x, next[i].y, 0};
        std::memcpy(bytes + i * sizeof(vertex), vertex, sizeof(vertex));
    }
    surface_->surface_update_vertex_region(0, 0, upload);
    positions_ = std::move(next);
    update_bounds();
    ++uploads_;
    return result({});
}

void KasaneMeshView::update_bounds() {
    float left = positions_[0].x, right = left, top = positions_[0].y, bottom = top;
    for (const auto &p : positions_) {
        left = std::min(left, p.x);
        right = std::max(right, p.x);
        top = std::min(top, p.y);
        bottom = std::max(bottom, p.y);
    }
    surface_->set_custom_aabb(AABB(Vector3(left, top, -0.5), Vector3(right - left, bottom - top, 1)));
}

void KasaneMeshView::clear() {
    ERR_FAIL_COND_MSG(
        !(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()),
        "Rendering calls require the main thread.");
    set_mesh(Ref<Mesh>());
    set_texture(Ref<Texture2D>());
    surface_.unref();
    positions_.clear();
}

Dictionary KasaneMeshView::get_render_stats() const {
    Dictionary out;
    out["initialized"] = surface_.is_valid();
    out["mesh_rid"] = surface_.is_valid() ? surface_->get_rid().get_id() : uint64_t(0);
    out["texture_instance_id"] = get_texture().is_valid() ? get_texture()->get_instance_id() : uint64_t(0);
    out["vertex_count"] = static_cast<int64_t>(positions_.size());
    out["position_uploads"] = uploads_;
    out["surface_creations"] = creations_;
    return out;
}
} // namespace kasane_gd
