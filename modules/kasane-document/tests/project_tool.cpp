// SPDX-License-Identifier: MIT
// Small native acceptance driver, deliberately independent of Godot and the renderer.
#include <kasane/project.hpp>
#include <kasane/project_codec.hpp>
#include <kasane/evaluation.hpp>
#include <nlohmann/json.hpp>
#include <iostream>
using namespace kasane;
using namespace kasane::io;
using Json = nlohmann::json;

static void require(const Status &s) {
    if (!s.ok())
        throw std::runtime_error(s.code + ": " + s.message);
}

static Json points(const std::vector<Vec2> &values) {
    Json out = Json::array();
    for (auto v : values)
        out.push_back({v.x, v.y});
    return out;
}

static void write_json(FileSystem &files, const fs::path &p, const Json &j) {
    auto s = j.dump(2) + "\n";
    write_new(files, p, {reinterpret_cast<const uint8_t *>(s.data()), s.size()});
}

int main(int argc, char **argv) {
    try {
        if (argc != 4)
            throw std::runtime_error("Usage: project_tool copy|inspect|edit|save|hold-lock PROJECT OUTPUT");
        std::string mode = argv[1];
        auto source = local_path(path_from_utf8(argv[2]));
        auto output = local_path(path_from_utf8(argv[3]));
        NativeFileSystem files;
        if (mode == "hold-lock") {
            auto lock = files.lock(source);
            write_json(files, output, Json{{"locked", true}});
            std::cin.get();
            return 0;
        }
        DocumentSession session;
        require(session.open(source).status);
        if (mode == "save") {
            require(session.save(output).status);
            return 0;
        }
        files.create_directories(output);
        auto &doc = session.document();
        if (mode == "edit") {
            auto binding = *doc.get_binding(doc.binding_order().front());
            auto form = binding.keyforms.front();
            for (auto &p : form.positions)
                p.x += 0.1f;
            require(doc.set_mesh_keyform(binding.id, form).status);
            binding = *doc.get_binding(binding.id);
            binding.axes[0].parameter_id = doc.parameter_order().back();
            require(doc.replace_binding(binding).status);
            Parameter unused{"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "Unused", "temporary", -1, 1, 0, 6};
            require(doc.create_parameter(unused).status);
            require(doc.erase_object(unused.id).status);
        }
        if (mode == "copy" || mode == "edit")
            require(session.save(output / "project").status);
        else if (mode != "inspect")
            throw std::runtime_error("Unknown native tool mode");
        std::string document;
        require(encode_project(doc, document));
        write_json(files, output / "source.json", Json::parse(document));
        Json samples = Json::array();
        for (float x : {-1.f, -0.5f, 0.f, 0.5f, 1.f})
            for (float y : {-1.f, -0.5f, 0.f, 0.5f, 1.f})
                for (float z : {-1.f, 0.f, 1.f}) {
                    const auto &parameters = doc.parameter_order();
                    if (parameters.size() != 3)
                        throw std::runtime_error("Acceptance model requires three parameters");
                    DrawableFrame frame;
                    require(evaluate_frame(doc, {{parameters[0], x}, {parameters[1], y}, {parameters[2], z}},
                                           frame));
                    Json drawables = Json::array();
                    for (auto &d : frame.drawables)
                        drawables.push_back({{"id", d.id},
                                             {"runtime_id", d.runtime_id},
                                             {"positions", points(d.positions)},
                                             {"uvs", points(d.uvs)},
                                             {"indices", d.indices},
                                             {"texture_asset_id", d.texture_asset_id},
                                             {"texture_slot", d.texture_slot},
                                             {"draw_order", d.draw_order},
                                             {"render_order", d.render_order},
                                             {"opacity", d.opacity},
                                             {"enabled", d.enabled},
                                             {"visible", d.visible},
                                             {"double_sided", d.double_sided},
                                             {"inverted_mask", d.inverted_mask},
                                             {"blend_mode", int(d.blend_mode)},
                                             {"masks", d.masks},
                                             {"multiply_color", d.multiply_color},
                                             {"screen_color", d.screen_color}});
                    samples.push_back({{"parameters", {x, y, z}}, {"drawables", drawables}});
                }
        write_json(files, output / "samples.json", samples);
        require(session.export_package(output / "runtime").status);
        write_json(files, output / "report.json",
                   {{"status", "passed"},
                    {"samples", samples.size()},
                    {"source", path_text(source)},
                    {"manifest", path_text(session.manifest())},
                    {"godot_required", false}});
        return 0;
    } catch (const std::exception &e) {
        std::cerr << e.what() << "\n";
        return 1;
    }
}
