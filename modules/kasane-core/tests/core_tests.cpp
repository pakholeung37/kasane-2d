// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <iostream>
#include <limits>
#include <cstdlib>

using namespace kasane;
#define CHECK(...)                                                                                           \
    do {                                                                                                     \
        if (!(__VA_ARGS__)) {                                                                                \
            std::cerr << "FAIL line " << __LINE__ << ": " << #__VA_ARGS__ << '\n';                           \
            std::exit(1);                                                                                    \
        }                                                                                                    \
    } while (0)
static const std::string DOC = "11111111-1111-4111-8111-111111111111";
static const std::string ASSET = "22222222-2222-4222-8222-222222222222";
static const std::string MESH = "33333333-3333-4333-8333-333333333333";

Mesh sample() {
    return {MESH,
            "quad",
            ASSET,
            {40, 10, 90, 20},
            {{-10, 10}, {-10, -10}, {10, -10}, {10, 10}},
            {{0, 0}, {0, 1}, {1, 1}, {1, 0}},
            {{{40, 10, 90}}, {{40, 90, 20}}}};
}

int main() {
    Document doc;
    CHECK(!doc.create_mesh(sample()).status.ok());
    CHECK(!doc.initialize("not-a-uuid", {100, 100}).ok());
    CHECK(!doc.initialize(DOC, {0, 100}).ok());
    CHECK(doc.initialize(DOC, {100, 100}).ok());
    CHECK(!doc.initialize(DOC, {100, 100}).ok());
    CHECK(!doc.create_mesh(sample()).status.ok());
    CHECK(!doc.add_asset({DOC, "bad", "memory://test", 32, 32}).status.ok());
    CHECK(doc.add_asset({ASSET, "checker", "memory://test", 32, 32}).status.ok());
    auto invalid = sample();
    invalid.vertex_ids[1] = 40;
    CHECK(doc.create_mesh(invalid).status.code == "DUPLICATE_VERTEX");
    invalid = sample();
    invalid.triangles[0][0] = 99;
    CHECK(doc.create_mesh(invalid).status.code == "MISSING_VERTEX");
    invalid = sample();
    invalid.triangles[0][0] = 10;
    CHECK(doc.create_mesh(invalid).status.code == "REPEATED_VERTEX");
    invalid = sample();
    invalid.base_positions[0].x = std::numeric_limits<float>::infinity();
    CHECK(doc.create_mesh(invalid).status.code == "NON_FINITE");
    CHECK(doc.mesh_order().empty());
    auto source = sample();
    auto edit = doc.create_mesh(source);
    CHECK(edit.status.ok() && edit.changes.kind == ChangeKind::structure);
    source.base_positions[0].x = 999;
    CHECK(doc.get_mesh(MESH)->base_positions[0].x == -10);
    CHECK(!doc.create_mesh(sample()).status.ok());
    std::vector<uint32_t> indices;
    CHECK(doc.render_indices(MESH, indices).ok());
    CHECK(indices == std::vector<uint32_t>({0, 1, 2, 0, 2, 3}));
    auto revision = doc.revision();
    auto before = *doc.get_mesh(MESH);
    std::vector<VertexId> ids = {40, 999};
    std::vector<Vec2> values = {{0, 0}, {1, 1}};
    CHECK(doc.set_vertex_positions(MESH, ids, values).status.code == "MISSING_VERTEX");
    CHECK(doc.get_mesh(MESH)->base_positions == before.base_positions && doc.revision() == revision);
    ids = {40, 40};
    CHECK(doc.set_vertex_positions(MESH, ids, values).status.code == "DUPLICATE_VERTEX");
    ids = {40};
    CHECK(doc.set_vertex_positions(MESH, ids, values).status.code == "INVALID_LENGTH");
    values = {{std::numeric_limits<float>::quiet_NaN(), 0}};
    CHECK(doc.set_vertex_positions(MESH, ids, values).status.code == "NON_FINITE");
    values = {{-15, 12}};
    edit = doc.set_vertex_positions(MESH, ids, values);
    CHECK(edit.status.ok() && edit.changes.kind == ChangeKind::positions);
    CHECK(doc.get_mesh(MESH)->base_positions[0] == values[0]);
    CHECK(doc.get_mesh(MESH)->base_positions[1] == before.base_positions[1]);
    revision = doc.revision();
    CHECK(doc.set_vertex_positions(MESH, ids, values).changes.kind == ChangeKind::none);
    CHECK(doc.revision() == revision);
    CHECK(doc.rename_mesh(MESH, "renamed").changes.kind == ChangeKind::metadata);
    CHECK(doc.get_mesh(MESH)->id == MESH && doc.get_mesh(MESH)->vertex_ids == before.vertex_ids);
    // Coincident vertices and UVs outside [0,1] are allowed, not topology corruption.
    auto collapsed = sample();
    collapsed.id = "44444444-4444-4444-8444-444444444444";
    collapsed.base_positions.assign(4, {0, 0});
    collapsed.uvs[0] = {-1, 2};
    CHECK(doc.create_mesh(collapsed).status.ok());
    CHECK(doc.mesh_order() == std::vector<std::string>({MESH, collapsed.id}));
    indices = {123};
    CHECK(!doc.render_indices("missing", indices).ok() && indices == std::vector<uint32_t>({123}));
    CHECK(doc.get_mesh(MESH)->base_positions[0] == values[0]);

    // Clean state follows content, including edits that return to the saved value.
    doc.mark_saved();
    const auto saved_name = doc.get_mesh(MESH)->name;
    CHECK(doc.rename_mesh(MESH, "temporary").status.ok() && doc.modified());
    CHECK(doc.rename_mesh(MESH, saved_name).status.ok() && !doc.modified());
    const auto saved_source = doc;
    CHECK(doc.rename_mesh(MESH, "second save").status.ok());
    doc.mark_saved();
    const auto second_save = doc;
    doc.restore_from(saved_source);
    CHECK(doc.modified());
    doc.restore_from(second_save);
    CHECK(!doc.modified());

    // Multiple commands commit atomically and occupy one history step.
    doc.mark_saved();
    CHECK(!doc.modified());
    CHECK(doc.begin_transaction().ok());
    CHECK(doc.stage_vertex_positions({MESH, {40}, {{-20, 25}}}).ok());
    CHECK(doc.stage_vertex_positions({MESH, {20}, {{20, 25}}}).ok());
    auto transaction_revision = doc.revision();
    edit = doc.commit_transaction();
    CHECK(edit.status.ok() && edit.changes.kind == ChangeKind::positions);
    CHECK(doc.revision() == transaction_revision + 1 && doc.modified());
    CHECK(doc.get_mesh(MESH)->base_positions[0] == Vec2({-20, 25}));
    CHECK(doc.get_mesh(MESH)->base_positions[3] == Vec2({20, 25}));
    // Validation happens before any source write, even across staged commands.
    CHECK(doc.begin_transaction().ok());
    CHECK(doc.stage_vertex_positions({MESH, {40}, {{1, 2}}}).ok());
    CHECK(doc.stage_vertex_positions({MESH, {999}, {{3, 4}}}).ok());
    auto atomic_before = doc.get_mesh(MESH)->base_positions;
    transaction_revision = doc.revision();
    edit = doc.commit_transaction();
    CHECK(edit.status.code == "MISSING_VERTEX");
    CHECK(doc.get_mesh(MESH)->base_positions == atomic_before && doc.revision() == transaction_revision);

    CHECK(doc.begin_transaction().ok());
    CHECK(doc.stage_vertex_positions({MESH, {40}, {{100, 100}}}).ok());
    transaction_revision = doc.revision();
    CHECK(doc.cancel_transaction().ok());
    CHECK(doc.get_mesh(MESH)->base_positions == atomic_before && doc.revision() == transaction_revision);
    CHECK(doc.cancel_transaction().code == "NO_TRANSACTION");

    // Source restoration is explicit; the core owns no undo/redo stack.
    auto checkpoint = doc;
    CHECK(
        doc.set_vertex_positions(MESH, std::vector<VertexId>{10}, std::vector<Vec2>{{-11, -12}}).status.ok());
    auto restore_revision = doc.revision();
    doc.restore_from(checkpoint);
    CHECK(doc.revision() == restore_revision + 1);
    CHECK(doc.get_mesh(MESH)->base_positions == checkpoint.get_mesh(MESH)->base_positions);
    auto replacement = *doc.get_mesh(MESH);
    replacement.uvs[0] = {0.25, 0.5};
    CHECK(doc.replace_mesh(replacement).status.ok());
    CHECK(doc.get_mesh(MESH)->uvs[0] == Vec2({0.25, 0.5}));
    replacement.triangles[0][0] = 999;
    CHECK(!doc.replace_mesh(replacement).status.ok());
    CHECK(doc.get_mesh(MESH)->triangles == checkpoint.get_mesh(MESH)->triangles);
    auto stale_revision = doc.revision() - 1;
    std::vector<VertexPositionUpdate> stale_batch = {{MESH, {40}, {{5, 6}}}};
    auto stale_before = doc.get_mesh(MESH)->base_positions;
    CHECK(doc.apply_vertex_position_updates_at_revision(stale_batch, stale_revision).status.code ==
          "STALE_REVISION");
    CHECK(doc.get_mesh(MESH)->base_positions == stale_before);
    CHECK(doc.apply_vertex_position_updates_at_revision(stale_batch, doc.revision()).status.ok());
    CHECK(doc.get_mesh(MESH)->base_positions[0] == Vec2({5, 6}));
    std::cout << "KASANE_CORE_TESTS_OK: identity, direct data, atomic batches, cancel, restore, topology\n";
}
