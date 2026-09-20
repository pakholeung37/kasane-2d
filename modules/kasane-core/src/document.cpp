// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <cmath>
#include <unordered_set>

namespace kasane {
bool valid_uuid(const std::string &id) {
    if (id.size() != 36)
        return false;
    bool nonzero = false;
    for (size_t i = 0; i < id.size(); ++i) {
        if (i == 8 || i == 13 || i == 18 || i == 23) {
            if (id[i] != '-')
                return false;
        } else {
            if (!((id[i] >= '0' && id[i] <= '9') || (id[i] >= 'a' && id[i] <= 'f')))
                return false;
            nonzero |= id[i] != '0';
        }
    }
    return nonzero;
}

Status Document::initialize(std::string id, Canvas canvas) {
    if (initialized())
        return Status::error("ALREADY_INITIALIZED", "Create a new Document to open another model.");
    if (!valid_uuid(id))
        return Status::error("INVALID_ID", "Use a nonzero canonical lowercase UUID.");
    if (!std::isfinite(canvas.width) || !std::isfinite(canvas.height) || canvas.width <= 0 ||
        canvas.height <= 0 || !std::isfinite(canvas.origin.x) || !std::isfinite(canvas.origin.y) ||
        !std::isfinite(canvas.pixels_per_unit) || canvas.pixels_per_unit <= 0)
        return Status::error("INVALID_CANVAS", "Canvas dimensions must be finite and positive.");
    id_ = std::move(id);
    canvas_ = canvas;
    return {};
}

bool Document::contains_id(const std::string &id) const {
    return transforms_.contains(id) || parts_.contains(id) || scene_bindings_.contains(id) || id == id_ ||
           assets_.contains(id) || meshes_.contains(id) || deformers_.contains(id) ||
           parameters_.contains(id) || bindings_.contains(id);
}

const ImageAsset *Document::get_asset(const std::string &id) const {
    auto it = assets_.find(id);
    return it == assets_.end() ? nullptr : &it->second;
}

const Mesh *Document::get_mesh(const std::string &id) const {
    auto it = meshes_.find(id);
    return it == meshes_.end() ? nullptr : &it->second;
}

EditResult Document::failed(Status status) const {
    return {std::move(status), {ChangeKind::none, {}, revision_, {}}, {}};
}

void Document::advance_state() {
    current_state_id_ = next_state_id_++;
}

EditResult Document::changed(ChangeKind kind, std::vector<std::string> ids,
                             std::vector<std::string> objects) {
    if (objects.empty())
        objects = ids;
    advance_state();
    return {{}, {kind, std::move(ids), ++revision_, std::move(objects)}, {}};
}

EditResult Document::add_asset(ImageAsset asset) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(asset.id))
        return failed(Status::error("INVALID_ID", "Asset ID must be a canonical UUID."));
    if (contains_id(asset.id))
        return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (!asset.width || !asset.height || asset.source.empty())
        return failed(Status::error("INVALID_ASSET", "Asset requires source and positive pixel dimensions."));
    const auto key = asset.id;
    assets_.emplace(key, std::move(asset));
    asset_order_.push_back(key);
    return changed(ChangeKind::metadata, {}, {key});
}

EditResult Document::create_mesh(Mesh mesh) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (!initialized())
        return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(mesh.id))
        return failed(Status::error("INVALID_ID", "Mesh ID must be a canonical UUID."));
    if (contains_id(mesh.id))
        return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (!get_asset(mesh.texture_asset_id))
        return failed(Status::error("MISSING_ASSET", "Texture asset does not exist."));
    if (auto s = validate_mesh_properties(mesh); !s.ok())
        return failed(s);
    if (mesh.runtime_id.empty())
        mesh.runtime_id = mesh.id;
    for (const auto &[id, other] : meshes_)
        if (other.runtime_id == mesh.runtime_id)
            return failed(Status::error("DUPLICATE_RUNTIME_ID", mesh.id + ".runtime_id duplicates " + id));
    if (mesh.vertex_ids.size() != mesh.base_positions.size())
        return failed(Status::error("INVALID_LENGTH", "Vertex IDs must match positions."));
    std::unordered_map<VertexId, uint32_t> slots;
    for (size_t i = 0; i < mesh.vertex_ids.size(); ++i) {
        if (!slots.emplace(mesh.vertex_ids[i], static_cast<uint32_t>(i)).second)
            return failed(Status::error("DUPLICATE_VERTEX", "Vertex IDs must be unique within a mesh."));
    }
    std::vector<uint32_t> indices;
    for (const auto &triangle : mesh.triangles) {
        for (auto vertex : triangle) {
            auto it = slots.find(vertex);
            if (it == slots.end())
                return failed(Status::error("MISSING_VERTEX", "Triangle references an unknown vertex ID."));
            indices.push_back(it->second);
        }
    }
    if (auto status = validate_render_mesh(mesh.base_positions, mesh.uvs, indices); !status.ok())
        return failed(status);
    const auto key = mesh.id;
    meshes_.emplace(key, std::move(mesh));
    vertex_slots_.emplace(key, std::move(slots));
    mesh_order_.push_back(key);
    return changed(ChangeKind::structure, {key});
}

EditResult Document::rename_mesh(const std::string &id, std::string name) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    auto it = meshes_.find(id);
    if (it == meshes_.end())
        return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
    if (it->second.name == name)
        return {{}, {ChangeKind::none, {}, revision_, {}}, {}};
    it->second.name = std::move(name);
    return changed(ChangeKind::metadata, {id});
}

EditResult Document::set_vertex_positions(const std::string &id, std::span<const VertexId> vertices,
                                          std::span<const Vec2> positions) {
    VertexPositionUpdate update{id, {vertices.begin(), vertices.end()}, {positions.begin(), positions.end()}};
    return apply_vertex_position_updates(std::span<const VertexPositionUpdate>(&update, 1));
}

EditResult Document::apply_vertex_position_updates(std::span<const VertexPositionUpdate> updates) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Use commit_transaction for staged edits."));

    struct PositionDelta {
        std::string mesh_id;
        std::vector<uint32_t> slots;
        std::vector<Vec2> after;
    };

    std::vector<PositionDelta> deltas;
    std::unordered_map<std::string, std::unordered_set<VertexId>> seen;
    std::vector<std::string> changed_meshes;
    std::unordered_set<std::string> changed_mesh_set;
    for (const auto &update : updates) {
        auto mesh_it = meshes_.find(update.mesh_id);
        if (mesh_it == meshes_.end())
            return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
        if (update.vertex_ids.size() != update.positions.size())
            return failed(Status::error("INVALID_LENGTH", "IDs and positions must match."));
        if (auto status = validate_positions(update.positions); !status.ok())
            return failed(status);
        PositionDelta delta;
        delta.mesh_id = update.mesh_id;
        const auto &lookup = vertex_slots_.at(update.mesh_id);
        for (size_t i = 0; i < update.vertex_ids.size(); ++i) {
            const auto vertex = update.vertex_ids[i];
            if (!seen[update.mesh_id].insert(vertex).second)
                return failed(
                    Status::error("DUPLICATE_VERTEX", "A transaction cannot write a vertex twice."));
            auto slot = lookup.find(vertex);
            if (slot == lookup.end())
                return failed(Status::error("MISSING_VERTEX", "Vertex ID does not exist."));
            const auto old = mesh_it->second.base_positions[slot->second];
            if (old == update.positions[i])
                continue;
            delta.slots.push_back(slot->second);
            delta.after.push_back(update.positions[i]);
        }
        if (!delta.slots.empty()) {
            if (changed_mesh_set.insert(update.mesh_id).second)
                changed_meshes.push_back(update.mesh_id);
            deltas.push_back(std::move(delta));
        }
    }
    if (deltas.empty())
        return {{}, {ChangeKind::none, {}, revision_, {}}, {}};
    for (const auto &delta : deltas) {
        auto &positions = meshes_.at(delta.mesh_id).base_positions;
        for (size_t i = 0; i < delta.slots.size(); ++i)
            positions[delta.slots[i]] = delta.after[i];
    }
    return changed(ChangeKind::positions, std::move(changed_meshes));
}

EditResult Document::apply_vertex_position_updates_at_revision(std::span<const VertexPositionUpdate> updates,
                                                               uint64_t expected_revision) {
    if (revision_ != expected_revision)
        return failed(Status::error("STALE_REVISION", "Document changed since this transaction began."));
    return apply_vertex_position_updates(updates);
}

Status Document::begin_transaction() {
    if (!initialized())
        return Status::error("NOT_INITIALIZED", "Initialize Document first.");
    if (transaction_active_)
        return Status::error("TRANSACTION_ACTIVE", "A transaction is already active.");
    transaction_active_ = true;
    staged_updates_.clear();
    return {};
}

Status Document::stage_vertex_positions(VertexPositionUpdate update) {
    if (!transaction_active_)
        return Status::error("NO_TRANSACTION", "Begin a transaction first.");
    if (update.vertex_ids.size() != update.positions.size())
        return Status::error("INVALID_LENGTH", "IDs and positions must match.");
    if (auto status = validate_positions(update.positions); !status.ok())
        return status;
    if (!get_mesh(update.mesh_id))
        return Status::error("MISSING_MESH", "Mesh does not exist.");
    staged_updates_.push_back(std::move(update));
    return {};
}

EditResult Document::commit_transaction() {
    if (!transaction_active_)
        return failed(Status::error("NO_TRANSACTION", "Begin a transaction first."));
    transaction_active_ = false;
    auto updates = std::move(staged_updates_);
    staged_updates_.clear();
    return apply_vertex_position_updates(updates);
}

Status Document::cancel_transaction() {
    if (!transaction_active_)
        return Status::error("NO_TRANSACTION", "No transaction is active.");
    staged_updates_.clear();
    transaction_active_ = false;
    return {};
}

void Document::restore_from(const Document &source) {
    const auto next_revision = revision_ + 1;
    const auto next_state = next_state_id_;
    *this = source;
    revision_ = next_revision;
    next_state_id_ = next_state;
    advance_state();
    transaction_active_ = false;
    staged_updates_.clear();
}

EditResult Document::replace_mesh(Mesh mesh) {
    if (mutation_blocked())
        return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (!get_mesh(mesh.id))
        return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
    const auto *previous = get_mesh(mesh.id);
    if (binding_for_mesh(mesh.id) &&
        (previous->vertex_ids != mesh.vertex_ids || previous->triangles != mesh.triangles))
        return failed(
            Status::error("KEYFORMS_REQUIRED",
                          mesh.id + ": topology replacement requires every keyform and vertex mapping"));
    // Validate through the same creation path before replacing any live arrays.
    Document candidate = *this;
    candidate.meshes_.erase(mesh.id);
    candidate.vertex_slots_.erase(mesh.id);
    const auto key = mesh.id;
    auto edit = candidate.create_mesh(std::move(mesh));
    if (!edit.status.ok())
        return failed(edit.status);
    meshes_.at(key) = std::move(candidate.meshes_.at(key));
    vertex_slots_.at(key) = std::move(candidate.vertex_slots_.at(key));
    return changed(ChangeKind::structure, {key});
}

Status Document::render_indices(const std::string &id, std::vector<uint32_t> &out) const {
    const auto *mesh = get_mesh(id);
    if (!mesh)
        return Status::error("MISSING_MESH", "Mesh does not exist.");
    const auto &slots = vertex_slots_.at(id);
    std::vector<uint32_t> next;
    next.reserve(mesh->triangles.size() * 3);
    for (const auto &triangle : mesh->triangles)
        for (auto vertex : triangle)
            next.push_back(slots.at(vertex));
    out = std::move(next);
    return {};
}
} // namespace kasane
