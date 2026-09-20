// SPDX-License-Identifier: MIT
#include <kasane/evaluation.hpp>
#include <cmath>
#include <iostream>
#include <stdexcept>
using namespace kasane;
#define CHECK(x)                                                                                             \
    do {                                                                                                     \
        if (!(x))                                                                                            \
            throw std::runtime_error(std::string("line ") + std::to_string(__LINE__) + ": " + #x);           \
    } while (0)

static std::string id(int n) {
    return std::to_string(n) + "1111111-1111-4111-8111-111111111111";
}

int main() {
    try {
        Document d;
        CHECK(d.initialize(id(1), {100, 100, {50, 50}, 100}).ok());
        CHECK(d.add_asset({id(2), "asset", "memory://unloaded", 8, 8}).status.ok());
        Mesh m{id(3),
               "mesh",
               id(2),
               {9, 1, 7},
               {{10, 10}, {40, 10}, {20, 40}},
               {{0, 0}, {1, 0}, {0.5f, 1}},
               {{{9, 1, 7}}},
               "Mesh"};
        CHECK(d.create_mesh(m).status.ok());
        auto e = d.create_parameter({id(4), "Param", "parameter", -1, 1, 0, 6});
        CHECK(e.status.ok() && e.changes.object_ids == std::vector<std::string>{id(4)});
        MeshBinding b{id(5), id(3), {{id(4), {-1, 0, 1}}}, {}};
        for (float key : {1.0f, 0.0f, -1.0f}) {
            auto positions = m.base_positions;
            for (auto &p : positions)
                p.x += key * 20;
            b.keyforms.push_back({{key}, positions});
        }
        CHECK(d.create_binding(b).status.ok());
        CHECK(d.get_binding(id(5))->keyforms[0].keys[0] == -1);
        d.mark_saved();
        auto rev = d.revision();
        DrawableFrame frame;
        CHECK(evaluate_frame(d, {{id(4), 0.5f}}, frame).ok());
        CHECK(std::abs(frame.drawables[0].positions[0].x + 0.3f) < 1e-6f);
        CHECK(d.revision() == rev && !d.modified());
        const auto preserved = frame.drawables[0].positions;
        CHECK(!evaluate_frame(d, {{id(4), NAN}}, frame).ok());
        CHECK(frame.drawables[0].positions == preserved);
        CHECK(!evaluate_frame(d, {{id(6), 0}}, frame).ok());
        CHECK(evaluate_frame(d, {{id(4), 10}}, frame).ok());
        CHECK(frame.parameters[0].clamped && frame.parameters[0].value == 1);
        VertexId vertex = 9;
        Vec2 changed_base{999, 10};
        CHECK(d.set_vertex_positions(id(3), {&vertex, 1}, {&changed_base, 1}).status.ok());
        CHECK(evaluate_frame(d, {{id(4), 0.5f}}, frame).ok());
        CHECK(frame.drawables[0].positions == preserved);
        auto invalid = *d.get_binding(id(5));
        invalid.axes[0].keys = {-1, 0, 0};
        rev = d.revision();
        CHECK(!d.replace_binding(invalid).status.ok() && d.revision() == rev);
        invalid = *d.get_binding(id(5));
        invalid.keyforms[1].positions.pop_back();
        CHECK(d.replace_binding(invalid).status.code == "INVALID_LENGTH");
        CHECK(d.erase_object(id(2)).referrers == std::vector<std::string>{id(3)});
        CHECK(d.erase_object(id(3)).referrers == std::vector<std::string>{id(5)});
        CHECK(d.erase_object(id(4)).referrers == std::vector<std::string>{id(5)});
        CHECK(d.begin_transaction().ok());
        CHECK(!d.erase_object(id(5)).status.ok());
        CHECK(d.cancel_transaction().ok());
        CHECK(d.erase_object(id(5)).status.ok());
        CHECK(evaluate_frame(d, {}, frame).ok());
        CHECK(std::abs(frame.drawables[0].positions[0].x - 9.49f) < 1e-5f);
        CHECK(d.erase_object(id(4)).status.ok());
        CHECK(d.erase_object(id(3)).status.ok());
        CHECK(d.erase_object(id(2)).status.ok());
        CHECK(evaluate_frame(d, {}, frame).ok() && frame.drawables.empty());
        std::cout << "keyform data/editing/evaluation checks passed\n";
        return 0;
    } catch (const std::exception &e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}
