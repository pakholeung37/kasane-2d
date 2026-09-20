// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <cmath>
#include <cstdlib>
#include <iostream>
#include <limits>
using namespace kasane;
#define CHECK(...)                                                                                           \
    do {                                                                                                     \
        if (!(__VA_ARGS__)) {                                                                                \
            std::cerr << "FAIL " << __LINE__ << ": " << #__VA_ARGS__ << '\n';                                \
            std::exit(1);                                                                                    \
        }                                                                                                    \
    } while (0)
const std::string DOC = "11111111-1111-4111-8111-111111111111",
                  ASSET = "22222222-2222-4222-8222-222222222222",
                  MESH = "33333333-3333-4333-8333-333333333333", ROT = "44444444-4444-4444-8444-444444444444",
                  WARP = "55555555-5555-4555-8555-555555555555",
                  OTHER = "66666666-6666-4666-8666-666666666666";

bool near(Vec2 a, Vec2 b) {
    return std::abs(a.x - b.x) < 0.0001f && std::abs(a.y - b.y) < 0.0001f;
}

int main() {
    Document doc;
    CHECK(doc.initialize(DOC, {100, 100}).ok());
    CHECK(doc.add_asset({ASSET, "image", "res://image", 32, 32}).status.ok());
    Mesh mesh{MESH,
              "mesh",
              ASSET,
              {0, 1, 2, 3},
              {{0, 0}, {10, 0}, {10, 10}, {5, 5}},
              {{0, 0}, {1, 0}, {1, 1}, {.5, .5}},
              {{{0, 1, 2}, {0, 2, 3}}}};
    CHECK(doc.create_mesh(mesh).status.ok());
    mesh.id = OTHER;
    CHECK(doc.create_mesh(mesh).status.ok());
    Deformer r;
    r.id = ROT;
    r.name = "rotation";
    r.angle_degrees = 90;
    CHECK(doc.create_deformer(r).status.ok());
    CHECK(!doc.create_deformer(r).status.ok());
    CHECK(doc.set_parent(MESH, ROT).status.ok());
    std::vector<Vec2> out;
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok());
    CHECK(near(out[1], {0, 10}) && near(out[2], {-10, 10}));
    CHECK(doc.get_mesh(MESH)->base_positions == mesh.base_positions);
    CHECK(doc.set_rotation(ROT, {5, 5}, 180).changes.mesh_ids == std::vector<std::string>{MESH});
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok() && near(out[0], {10, 10}));
    CHECK(doc.set_rotation(ROT, {0, 0}, 90).status.ok());
    Deformer w;
    w.id = WARP;
    w.name = "warp";
    w.kind = DeformerKind::warp;
    w.size = {10, 10};
    CHECK(doc.create_deformer(w).status.ok());
    CHECK(doc.set_parent(MESH, WARP).status.ok());
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok() && out == mesh.base_positions);
    auto points = doc.get_deformer(WARP)->control_points;
    points[3] = {20, 20};
    CHECK(doc.set_warp_points(WARP, points).status.ok());
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok());
    CHECK(near(out[2], {20, 20}) && near(out[3], {7.5, 7.5}));
    CHECK(doc.set_parent(WARP, ROT).status.ok());
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok() && near(out[3], {-7.5, 7.5}));
    CHECK(doc.affected_meshes(ROT) == std::vector<std::string>{MESH});
    CHECK(doc.set_parent(OTHER, ROT, true).changes.mesh_ids.empty());
    CHECK(doc.evaluate_legacy_mesh(OTHER, out).ok() && out == mesh.base_positions);
    auto rev = doc.revision();
    CHECK(doc.set_parent(ROT, WARP).status.code == "PARENT_CYCLE" && doc.revision() == rev);
    CHECK(doc.set_parent(ROT, ROT).status.code == "PARENT_CYCLE");
    CHECK(doc.set_parent(ROT, OTHER).status.code == "INVALID_PARENT");
    CHECK(doc.set_parent(ROT, OTHER, true).status.code == "PARENT_CYCLE");
    CHECK(doc.set_parent(MESH, "").status.ok());
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok() && out == mesh.base_positions);
    CHECK(!doc.set_warp_points(WARP, std::vector<Vec2>{{0, 0}}).status.ok());
    points[0].x = std::numeric_limits<float>::infinity();
    CHECK(!doc.set_warp_points(WARP, points).status.ok());
    CHECK(!doc.set_rotation(ROT, {0, 0}, std::numeric_limits<float>::quiet_NaN()).status.ok());
    // Boundary extrapolation preserves an affine transform, including outside rest bounds.
    points = {{3, 4}, {13, 4}, {3, 14}, {13, 14}};
    CHECK(doc.set_warp_points(WARP, points).status.ok());
    CHECK(doc.set_parent(WARP, "").status.ok());
    CHECK(doc.set_parent(MESH, WARP).status.ok());
    CHECK(doc.set_vertex_positions(MESH, std::vector<VertexId>{0}, std::vector<Vec2>{{-5, 20}}).status.ok());
    CHECK(doc.evaluate_legacy_mesh(MESH, out).ok() && near(out[0], {-2, 24}));
    // Multi-cell center and shared-boundary continuity.
    Document grid = doc;
    Deformer multi = w;
    multi.id = "77777777-7777-4777-8777-777777777777";
    multi.columns = 2;
    multi.rows = 2;
    CHECK(grid.create_deformer(multi).status.ok());
    CHECK(grid.set_parent(MESH, multi.id).status.ok());
    auto cp = grid.get_deformer(multi.id)->control_points;
    cp[4] = {6, 7};
    CHECK(grid.set_warp_points(multi.id, cp).status.ok());
    CHECK(grid.evaluate_legacy_mesh(MESH, out).ok() && near(out[3], {6, 7}));
    // Whole subtree depth is checked, not only the reparented node.
    Document deep;
    CHECK(deep.initialize(DOC, {100, 100}).ok());
    std::string previous;
    for (int i = 0; i < 17; ++i) {
        Deformer d;
        d.id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaa" + std::string(1, "0123456789abcdef"[i / 16]) +
               std::string(1, "0123456789abcdef"[i % 16]);
        CHECK(deep.create_deformer(d).status.ok());
        CHECK(deep.set_parent(d.id, previous).status.ok());
        previous = d.id;
    }
    CHECK(deep.add_asset({ASSET, "image", "res://image", 32, 32}).status.ok());
    mesh.id = MESH;
    CHECK(deep.create_mesh(mesh).status.ok());
    CHECK(deep.set_parent(MESH, previous).status.code == "PARENT_DEPTH");
    Deformer extra;
    extra.id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    CHECK(deep.create_deformer(extra).status.ok());
    CHECK(deep.set_parent("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaa00", extra.id).status.code == "PARENT_DEPTH");
    // Numeric overflow is reported without publishing partial evaluated output.
    auto enormous = doc.get_deformer(WARP)->control_points;
    enormous[3] = {std::numeric_limits<float>::max(), std::numeric_limits<float>::max()};
    CHECK(doc.set_warp_points(WARP, enormous).status.ok());
    CHECK(doc.set_vertex_positions(MESH, std::vector<VertexId>{0}, std::vector<Vec2>{{20, 20}}).status.ok());
    out = {{123, 456}};
    CHECK(doc.evaluate_legacy_mesh(MESH, out).code == "EVALUATION_OVERFLOW");
    CHECK(out == std::vector<Vec2>{{123, 456}});
    std::cout << "KASANE_DEFORMER_TESTS_OK\n";
}
