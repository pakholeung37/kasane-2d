// SPDX-License-Identifier: MIT
// C++ Baseline Benchmark for Kasane 2D Core
#include <kasane/document.hpp>
#include <kasane/evaluation.hpp>
#include <kasane/moc3.hpp>
#include <kasane/project_codec.hpp>

#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <string>
#include <vector>

namespace fs = std::filesystem;
using namespace kasane;
using namespace std::chrono;

static std::string make_id(uint32_t type, uint32_t n) {
    char buf[37];
    std::snprintf(buf, sizeof(buf), "%04x%04x-1111-4111-8111-111111111111", type, n);
    return std::string(buf);
}

#define CHECK_OK(expr)                                                                                          \
    do {                                                                                                        \
        auto res = (expr);                                                                                      \
        if (!res.ok()) {                                                                                        \
            std::cerr << "FAIL line " << __LINE__ << ": " #expr " failed with code " << res.code << ": "       \
                      << res.message << std::endl;                                                              \
            std::exit(1);                                                                                       \
        }                                                                                                       \
    } while (0)

#define CHECK_EDIT_OK(expr)                                                                                     \
    do {                                                                                                        \
        auto res = (expr);                                                                                      \
        if (!res.status.ok()) {                                                                                 \
            std::cerr << "FAIL line " << __LINE__ << ": " #expr " failed with code " << res.status.code << ": " \
                      << res.status.message << std::endl;                                                       \
            std::exit(1);                                                                                       \
        }                                                                                                       \
    } while (0)

static Document build_benchmark_document(std::vector<std::string> &out_params) {
    Document doc;
    std::string doc_id = make_id(0x0001, 1);
    CHECK_OK(doc.initialize(doc_id, Canvas{1280, 720, {640, 360}, 100}));

    std::string asset1 = make_id(0x0002, 1);
    std::string asset2 = make_id(0x0002, 2);
    CHECK_EDIT_OK(doc.add_asset(ImageAsset{asset1, "texture_0", "textures/0.png", 512, 512, ""}));
    CHECK_EDIT_OK(doc.add_asset(ImageAsset{asset2, "texture_1", "textures/1.png", 512, 512, ""}));

    // 4 Parameters
    out_params.clear();
    for (int i = 0; i < 4; ++i) {
        std::string pid = make_id(0x0003, i + 1);
        out_params.push_back(pid);
        CHECK_EDIT_OK(doc.create_parameter(
            Parameter{pid, "Param_" + std::to_string(i), "Parameter " + std::to_string(i), -1.0f, 1.0f, 0.0f, 6}));
    }

    // 2 Parts
    std::string part_root = make_id(0x0004, 1);
    std::string part_child = make_id(0x0004, 2);
    CHECK_EDIT_OK(doc.create_part(Part{part_root, "PartRoot", "Root Part", "", true, 0.0f}));
    CHECK_EDIT_OK(doc.create_part(Part{part_child, "PartChild", "Child Part", part_root, true, 1.0f}));

    // Transform 1: Root Rotation
    std::string rot_root = make_id(0x0005, 1);
    Transform tfm_rot;
    tfm_rot.id = rot_root;
    tfm_rot.runtime_id = "RootRotation";
    tfm_rot.name = "Root Rotation";
    tfm_rot.part_id = part_root;
    tfm_rot.kind = TransformKind::rotation;
    tfm_rot.rotation = {{640.0f, 360.0f}, 0.0f, 1.0f, false, false};
    CHECK_EDIT_OK(doc.create_transform(tfm_rot));

    // Transform 2: Child Warp (3x3 grid = 16 points)
    std::string warp_child = make_id(0x0005, 2);
    Transform tfm_warp;
    tfm_warp.id = warp_child;
    tfm_warp.runtime_id = "ChildWarp";
    tfm_warp.name = "Child Warp";
    tfm_warp.parent_id = rot_root;
    tfm_warp.part_id = part_child;
    tfm_warp.kind = TransformKind::warp;
    tfm_warp.rows = 3;
    tfm_warp.columns = 3;
    tfm_warp.quad = true;
    for (int r = 0; r <= 3; ++r) {
        for (int c = 0; c <= 3; ++c) {
            tfm_warp.points.push_back({c * 50.0f - 75.0f, r * 50.0f - 75.0f});
        }
    }
    CHECK_EDIT_OK(doc.create_transform(tfm_warp));

    // Bindings for transforms
    for (int t = 1; t <= 2; ++t) {
        std::string tid = (t == 1) ? rot_root : warp_child;
        SceneBinding sb{make_id(0x0006, t), tid, {{out_params[0], {-1.0f, 0.0f, 1.0f}}}, {}};
        for (float key : {-1.0f, 0.0f, 1.0f}) {
            SceneKeyform kf;
            kf.keys = {key};
            if (t == 1) {
                kf.rotation = tfm_rot.rotation;
                kf.rotation.angle = key * 25.0f;
                kf.rotation.scale = 1.0f + key * 0.1f;
            } else {
                kf.positions = tfm_warp.points;
                for (auto &pt : kf.positions) {
                    pt.x += key * 15.0f;
                }
            }
            sb.keyforms.push_back(kf);
        }
        CHECK_EDIT_OK(doc.create_scene_binding(sb));
    }

    // 10 Meshes (each with 4x4 vertex grid = 16 vertices, 18 triangles)
    for (int m = 0; m < 10; ++m) {
        std::string mesh_id = make_id(0x0007, m + 1);
        Mesh mesh;
        mesh.id = mesh_id;
        mesh.runtime_id = "Mesh_" + std::to_string(m);
        mesh.name = "Mesh " + std::to_string(m);
        mesh.texture_asset_id = (m % 2 == 0) ? asset1 : asset2;
        mesh.part_id = part_child;
        mesh.deformer_id = warp_child;
        mesh.draw_order = float(m);

        // 4x4 vertices = 16 vertices
        VertexId vid = 0;
        for (int r = 0; r < 4; ++r) {
            for (int c = 0; c < 4; ++c) {
                mesh.vertex_ids.push_back(vid++);
                mesh.base_positions.push_back({c * 20.0f - 30.0f, r * 20.0f - 30.0f});
                mesh.uvs.push_back({c / 3.0f, r / 3.0f});
            }
        }
        // Triangles for 3x3 quad cells = 18 triangles
        for (int r = 0; r < 3; ++r) {
            for (int c = 0; c < 3; ++c) {
                VertexId v0 = VertexId(r * 4 + c);
                VertexId v1 = VertexId(r * 4 + c + 1);
                VertexId v2 = VertexId((r + 1) * 4 + c);
                VertexId v3 = VertexId((r + 1) * 4 + c + 1);
                mesh.triangles.push_back({{v0, v1, v2}});
                mesh.triangles.push_back({{v1, v3, v2}});
            }
        }

        CHECK_EDIT_OK(doc.create_mesh(mesh));

        // Binding on param 1
        MeshBinding mb{make_id(0x0008, m + 1), mesh_id, {{out_params[1], {-1.0f, 0.0f, 1.0f}}}, {}};
        for (float key : {-1.0f, 0.0f, 1.0f}) {
            MeshKeyform mkf;
            mkf.keys = {key};
            mkf.positions = mesh.base_positions;
            for (size_t vi = 0; vi < mkf.positions.size(); ++vi) {
                mkf.positions[vi].x += key * 10.0f * (float(vi % 4) / 3.0f);
                mkf.positions[vi].y += key * 5.0f;
            }
            mb.keyforms.push_back(mkf);
        }
        CHECK_EDIT_OK(doc.create_binding(mb));
    }

    return doc;
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    std::cout << "=== Kasane 2D C++ Baseline Benchmark ===" << std::endl;

    std::vector<std::string> param_ids;
    Document doc = build_benchmark_document(param_ids);
    std::cout << "Document constructed successfully:" << std::endl;
    std::cout << "  - Parameters: " << doc.parameter_order().size() << std::endl;
    std::cout << "  - Transforms: " << doc.transform_order().size() << std::endl;
    std::cout << "  - Meshes:     " << doc.mesh_order().size() << std::endl;

    size_t total_vertices = 0;
    for (const auto &mid : doc.mesh_order()) {
        total_vertices += doc.get_mesh(mid)->base_positions.size();
    }
    std::cout << "  - Total vertices: " << total_vertices << std::endl;

    // -------------------------------------------------------------
    // Benchmark 1: Evaluation Throughput
    // -------------------------------------------------------------
    std::cout << "\n[1/3] Benchmarking Evaluation Throughput (5,000 frames)..." << std::endl;
    DrawableFrame frame;
    PreviewValues preview;

    // Warmup
    for (int i = 0; i < 100; ++i) {
        float val = std::sin(i * 0.1f);
        preview[param_ids[0]] = val;
        preview[param_ids[1]] = -val;
        CHECK_OK(evaluate_frame(doc, preview, frame));
    }

    const int eval_iterations = 5000;
    auto eval_start = high_resolution_clock::now();
    for (int i = 0; i < eval_iterations; ++i) {
        float val = std::sin(i * 0.05f);
        preview[param_ids[0]] = val;
        preview[param_ids[1]] = -val * 0.8f;
        evaluate_frame(doc, preview, frame);
    }
    auto eval_end = high_resolution_clock::now();
    double eval_total_ms = duration<double, std::milli>(eval_end - eval_start).count();
    double eval_mean_us = (eval_total_ms * 1000.0) / eval_iterations;
    double eval_fps = (eval_iterations / eval_total_ms) * 1000.0;
    double vertices_per_sec = eval_fps * total_vertices;

    std::cout << "  Total time:     " << eval_total_ms << " ms" << std::endl;
    std::cout << "  Mean frame:     " << eval_mean_us << " us (" << eval_fps << " FPS)" << std::endl;
    std::cout << "  Vertex rate:    " << (vertices_per_sec / 1e6) << " M vertices/sec" << std::endl;

    // -------------------------------------------------------------
    // Benchmark 2: MOC3 5.0 Serialization Throughput
    // -------------------------------------------------------------
    std::cout << "\n[2/3] Benchmarking MOC3 5.0 Serialization (200 iterations)..." << std::endl;
    Moc3Artifact artifact;

    // Warmup
    for (int i = 0; i < 10; ++i) {
        CHECK_OK(encode_moc3(doc, artifact));
    }

    const int moc3_iterations = 200;
    auto moc3_start = high_resolution_clock::now();
    for (int i = 0; i < moc3_iterations; ++i) {
        encode_moc3(doc, artifact);
    }
    auto moc3_end = high_resolution_clock::now();
    double moc3_total_ms = duration<double, std::milli>(moc3_end - moc3_start).count();
    double moc3_mean_us = (moc3_total_ms * 1000.0) / moc3_iterations;
    size_t moc3_bytes = artifact.bytes.size();

    std::cout << "  Total time:     " << moc3_total_ms << " ms" << std::endl;
    std::cout << "  Mean serialize: " << moc3_mean_us << " us" << std::endl;
    std::cout << "  Artifact size:  " << moc3_bytes << " bytes" << std::endl;

    // -------------------------------------------------------------
    // Benchmark 3: Project JSON Codec Throughput
    // -------------------------------------------------------------
    std::cout << "\n[3/3] Benchmarking Project JSON Codec (500 iterations)..." << std::endl;
    std::string json_str;
    CHECK_OK(encode_project(doc, json_str));

    // Warmup
    for (int i = 0; i < 10; ++i) {
        std::string s;
        encode_project(doc, s);
        Document d2;
        decode_project(json_str, d2);
    }

    const int codec_iterations = 500;
    auto enc_start = high_resolution_clock::now();
    for (int i = 0; i < codec_iterations; ++i) {
        std::string s;
        encode_project(doc, s);
    }
    auto enc_end = high_resolution_clock::now();
    double enc_total_ms = duration<double, std::milli>(enc_end - enc_start).count();
    double enc_mean_us = (enc_total_ms * 1000.0) / codec_iterations;

    auto dec_start = high_resolution_clock::now();
    for (int i = 0; i < codec_iterations; ++i) {
        Document d2;
        decode_project(json_str, d2);
    }
    auto dec_end = high_resolution_clock::now();
    double dec_total_ms = duration<double, std::milli>(dec_end - dec_start).count();
    double dec_mean_us = (dec_total_ms * 1000.0) / codec_iterations;

    std::cout << "  JSON length:    " << json_str.size() << " bytes" << std::endl;
    std::cout << "  Mean encode:    " << enc_mean_us << " us" << std::endl;
    std::cout << "  Mean decode:    " << dec_mean_us << " us" << std::endl;

    // -------------------------------------------------------------
    // Export Baseline JSON
    // -------------------------------------------------------------
    fs::path out_dir = "target/benchmarks";
    fs::create_directories(out_dir);
    fs::path out_file = out_dir / "cpp_baseline.json";

    std::ofstream out(out_file);
    out << std::setprecision(6) << std::fixed;
    out << "{\n";
    out << "  \"version\": 1,\n";
    out << "  \"system\": {\n";
    out << "    \"model\": \"Kasane 2D Benchmark Standard Model\",\n";
    out << "    \"num_parameters\": " << doc.parameter_order().size() << ",\n";
    out << "    \"num_transforms\": " << doc.transform_order().size() << ",\n";
    out << "    \"num_meshes\": " << doc.mesh_order().size() << ",\n";
    out << "    \"total_vertices\": " << total_vertices << "\n";
    out << "  },\n";
    out << "  \"benchmarks\": {\n";
    out << "    \"evaluation\": {\n";
    out << "      \"iterations\": " << eval_iterations << ",\n";
    out << "      \"total_ms\": " << eval_total_ms << ",\n";
    out << "      \"mean_us\": " << eval_mean_us << ",\n";
    out << "      \"fps\": " << eval_fps << ",\n";
    out << "      \"vertices_per_sec\": " << vertices_per_sec << "\n";
    out << "    },\n";
    out << "    \"moc3_export\": {\n";
    out << "      \"iterations\": " << moc3_iterations << ",\n";
    out << "      \"total_ms\": " << moc3_total_ms << ",\n";
    out << "      \"mean_us\": " << moc3_mean_us << ",\n";
    out << "      \"bytes\": " << moc3_bytes << "\n";
    out << "    },\n";
    out << "    \"project_codec\": {\n";
    out << "      \"iterations\": " << codec_iterations << ",\n";
    out << "      \"json_bytes\": " << json_str.size() << ",\n";
    out << "      \"encode_mean_us\": " << enc_mean_us << ",\n";
    out << "      \"decode_mean_us\": " << dec_mean_us << "\n";
    out << "    }\n";
    out << "  }\n";
    out << "}\n";

    std::cout << "\nBaseline successfully saved to: " << out_file.string() << std::endl;
    return 0;
}
