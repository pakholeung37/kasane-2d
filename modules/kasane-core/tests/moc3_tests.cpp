// SPDX-License-Identifier: MIT
#include <kasane/moc3.hpp>
#include <kasane/package.hpp>
#include <png.h>
#include <kasane/evaluation.hpp>
#ifdef KASANE_OFFICIAL_CORE
#include <Live2DCubismCore.h>
#else
#include <PurismCore.h>
#if PSM_COMPAT_VERSION < 0x06000000L
#define csmGetRenderOrders csmGetDrawableRenderOrders
#endif
#endif
#include <cmath>
#include <algorithm>
#include <cstring>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <memory>
#include <random>
#include <stdexcept>
#include <source_location>
#include <sstream>
#include <iomanip>
using namespace kasane;
#define CHECK(x)                                                                                             \
    do {                                                                                                     \
        if (!(x))                                                                                            \
            throw std::runtime_error(std::string("line ") + std::to_string(__LINE__) + ": " + #x);           \
    } while (0)
static double max_error = 0;
static std::string context = "fixture";
static std::vector<std::string> comparisons, samples;
static unsigned sample = 0;

static void near(float actual, float expected, float ppu = 1,
                 std::source_location loc = std::source_location::current()) {
    CHECK(std::isfinite(actual));
    CHECK(std::isfinite(expected));
    double e = std::abs(double(actual) - expected);
    max_error = std::max(max_error, e);
    std::ostringstream row;
    row << std::setprecision(17) << "{\"object\":" << std::quoted(context) << ",\"check_line\":" << loc.line()
        << ",\"expected\":" << expected << ",\"actual\":" << actual << ",\"absolute_error\":" << e
        << ",\"pixel_error\":" << e * ppu << "}";
    comparisons.push_back(row.str());
    if (e > 1e-5 + 1e-5 * std::max(std::abs(actual), std::abs(expected)) || e * ppu > 0.05)
        throw std::runtime_error(context + " line " + std::to_string(loc.line()) + ": expected " +
                                 std::to_string(expected) + ", actual " + std::to_string(actual));
}

static std::string id(int n) {
    return std::to_string(n) + "1111111-1111-4111-8111-111111111111";
}

static Document fixture() {
    Document doc;
    CHECK(doc.initialize(id(1), {640, 480, {271, 193}, 100}).ok());
    CHECK(doc.add_asset({id(2), "A", "textures/0.png", 8, 8}).status.ok());
    CHECK(doc.add_asset({id(3), "B", "textures/1.png", 8, 8}).status.ok());
    Mesh m{id(4),
           "Asymmetric quad",
           id(2),
           {91, 8, 77, 12},
           {{101, 43}, {328, 65}, {365, 274}, {87, 291}},
           {{0, 0}, {1, 0.1f}, {0.9f, 1}, {0, 0.8f}},
           {{{91, 8, 77}}, {{91, 77, 12}}},
           "ArtMeshA"};
    CHECK(doc.create_mesh(m).status.ok());
    m = {id(5),
         "Triangle",
         id(3),
         {70, 13, 99},
         {{350, 117}, {542, 133}, {468, 373}},
         {{0.15f, 0.05f}, {0.95f, 0.2f}, {0.6f, 0.9f}},
         {{{70, 13, 99}}},
         "ArtMeshB"};
    CHECK(doc.create_mesh(m).status.ok());
    return doc;
}

static void verify(const Document &doc, const Moc3Artifact &a, const PreviewValues &preview = {}) {
    context = "sample-" + std::to_string(sample++) + "/canvas";
    const auto sample_context = context;
    DrawableFrame frame;
    CHECK(evaluate_frame(doc, preview, frame).ok());
    std::ostringstream sample_row;
    sample_row << std::setprecision(17) << "{\"sample\":" << std::quoted(sample_context)
               << ",\"parameters\":[";
    for (size_t i = 0; i < frame.parameters.size(); ++i) {
        const auto &p = frame.parameters[i];
        sample_row << (i ? "," : "") << "{\"id\":" << std::quoted(p.id) << ",\"requested\":" << p.requested
                   << ",\"actual\":" << p.value << "}";
    }
    sample_row << "]}";
    samples.push_back(sample_row.str());
    const auto &expected = frame.drawables;
    using Buffer = std::unique_ptr<void, decltype(&std::free)>;
    Buffer moc_mem(std::aligned_alloc(64, (a.bytes.size() + 63) & ~size_t(63)), &std::free);
    CHECK(moc_mem);
    std::memcpy(moc_mem.get(), a.bytes.data(), a.bytes.size());
    CHECK(csmHasMocConsistency(moc_mem.get(), unsigned(a.bytes.size())));
    auto *moc = csmReviveMocInPlace(moc_mem.get(), unsigned(a.bytes.size()));
    CHECK(moc);
    auto size = csmGetSizeofModel(moc);
    CHECK(size);
    Buffer model_mem(std::aligned_alloc(16, (size + 15) & ~size_t(15)), &std::free);
    CHECK(model_mem);
    auto *model = csmInitializeModelInPlace(moc, model_mem.get(), size);
    CHECK(model);
    CHECK(csmGetParameterCount(model) == int(doc.parameter_order().size()));
    for (size_t i = 0; i < doc.parameter_order().size(); ++i) {
        const auto &p = *doc.get_parameter(doc.parameter_order()[i]);
        CHECK(p.runtime_id == csmGetParameterIds(model)[i]);
        near(csmGetParameterMinimumValues(model)[i], p.minimum);
        near(csmGetParameterMaximumValues(model)[i], p.maximum);
        near(csmGetParameterDefaultValues(model)[i], p.default_value);
        csmGetParameterValues(model)[i] = frame.parameters[i].value;
    }
    csmUpdateModel(model);
    auto parts = doc.sorted_parts();
    CHECK(csmGetPartCount(model) == int(parts.size()));
    for (size_t i = 0; i < parts.size(); ++i) {
        auto p = doc.get_part(parts[i]);
        CHECK(p->runtime_id == csmGetPartIds(model)[i]);
        int parent = p->parent_id.empty()
                         ? -1
                         : int(std::find(parts.begin(), parts.end(), p->parent_id) - parts.begin());
        CHECK(csmGetPartParentPartIndices(model)[i] == parent);
    }
    CHECK(csmGetDrawableCount(model) == int(expected.size()));
    csmVector2 canvas, origin;
    float ppu;
    csmReadCanvasInfo(model, &canvas, &origin, &ppu);
    near(canvas.X, doc.canvas().width);
    near(canvas.Y, doc.canvas().height);
    near(origin.X, doc.canvas().origin.x);
    near(origin.Y, doc.canvas().height - doc.canvas().origin.y);
    near(ppu, doc.canvas().pixels_per_unit);
    for (size_t i = 0; i < expected.size(); ++i) {
        const auto &d = expected[i];
        context = sample_context + "/" + d.id;
        CHECK(d.runtime_id == csmGetDrawableIds(model)[i]);
        CHECK(csmGetDrawableTextureIndices(model)[i] == d.texture_slot);
        CHECK(csmGetDrawableVertexCounts(model)[i] == int(d.positions.size()));
        CHECK(csmGetDrawableIndexCounts(model)[i] == int(d.indices.size()));
        if (d.enabled)
            CHECK(csmGetDrawableDrawOrders(model)[i] == d.draw_order);
        CHECK(csmGetRenderOrders(model)[i] == d.render_order);
        CHECK(csmGetDrawableMaskCounts(model)[i] == int(d.masks.size()));
        for (size_t k = 0; k < d.masks.size(); ++k)
            CHECK(csmGetDrawableMasks(model)[i][k] ==
                  int(std::find(doc.mesh_order().begin(), doc.mesh_order().end(), d.masks[k]) -
                      doc.mesh_order().begin()));
        const auto &part = doc.get_mesh(d.id)->part_id;
        CHECK(csmGetDrawableParentPartIndices(model)[i] ==
              (part.empty() ? -1 : int(std::find(parts.begin(), parts.end(), part) - parts.begin())));
        CHECK(csmGetDrawableConstantFlags(model)[i] == ((d.double_sided ? 4 : 0) | (d.inverted_mask ? 8 : 0) |
                                                        (d.blend_mode == BlendMode::additive         ? 1
                                                         : d.blend_mode == BlendMode::multiplicative ? 2
                                                                                                     : 0)));
        CHECK(bool(csmGetDrawableDynamicFlags(model)[i] & 1) == d.visible);
        if (d.enabled)
            near(csmGetDrawableOpacities(model)[i], d.opacity);
        auto mul = csmGetDrawableMultiplyColors(model)[i], scr = csmGetDrawableScreenColors(model)[i];
        if (d.enabled) {
            near(mul.X, d.multiply_color[0]);
            near(mul.Y, d.multiply_color[1]);
            near(mul.Z, d.multiply_color[2]);
            near(mul.W, 1);
            near(scr.X, d.screen_color[0]);
            near(scr.Y, d.screen_color[1]);
            near(scr.Z, d.screen_color[2]);
            near(scr.W, 1);
        }
        for (size_t j = 0; j < d.positions.size(); ++j) {
            auto p = csmGetDrawableVertexPositions(model)[i][j], uv = csmGetDrawableVertexUvs(model)[i][j];
            if (d.visible) {
                near(p.X, d.positions[j].x, ppu);
                near(p.Y, d.positions[j].y, ppu);
            }
            near(uv.X, d.uvs[j].x);
            near(uv.Y, d.uvs[j].y);
        }
        for (size_t j = 0; j < d.indices.size(); ++j)
            CHECK(csmGetDrawableIndices(model)[i][j] == d.indices[j]);
    }
}

static Moc3Artifact encode(const Document &d) {
    Moc3Artifact a;
    auto s = encode_moc3(d, a);
    if (!s.ok())
        throw std::runtime_error(s.code + ": " + s.message);
    return a;
}

static Document animated_fixture(unsigned dimensions) {
    auto doc = fixture();
    MeshBinding b{id(9), id(4)};
    for (unsigned a = 0; a < dimensions; ++a) {
        // Deliberately reverse parameter creation order versus binding axes.
        unsigned index = dimensions - 1 - a;
        CHECK(doc.create_parameter({id(6 + index), "Param" + std::to_string(index), "parameter", -1, 1, 0, 6})
                  .status.ok());
    }
    const std::vector<float> keys =
        dimensions == 3 ? std::vector<float>{-1, 1} : std::vector<float>{-1, 0, 1};
    for (unsigned a = 0; a < dimensions; ++a)
        b.axes.push_back({id(6 + a), keys});
    size_t total = 1;
    for (unsigned a = 0; a < dimensions; ++a)
        total *= keys.size();
    for (size_t i = 0; i < total; ++i) {
        MeshKeyform f;
        size_t cursor = i;
        float x = 0, y = 0, z = 0;
        for (unsigned a = 0; a < dimensions; ++a) {
            f.keys.push_back(keys[cursor % keys.size()]);
            cursor /= keys.size();
        }
        x = f.keys[0];
        if (dimensions > 1)
            y = f.keys[1];
        if (dimensions > 2)
            z = f.keys[2];
        f.positions = doc.get_mesh(id(4))->base_positions;
        for (size_t v = 0; v < f.positions.size(); ++v) {
            f.positions[v].x += 11 * x + 3 * y + 7 * z + float(v) * x * y * 2;
            f.positions[v].y += 5 * x - 13 * y + 2 * z + float(v) * x * z * 3;
        }
        b.keyforms.push_back(f);
    }
    std::reverse(b.keyforms.begin(), b.keyforms.end());
    CHECK(doc.create_binding(b).status.ok());
    return doc;
}

static void verify_keyforms(const std::filesystem::path &output) {
    for (unsigned dimensions = 1; dimensions <= 3; ++dimensions) {
        auto doc = animated_fixture(dimensions);
        auto artifact = encode(doc);
        const std::vector<float> values =
            dimensions == 3 ? std::vector<float>{-1, 0, 1} : std::vector<float>{-1, -0.5f, 0, 0.5f, 1};
        size_t total = 1;
        for (unsigned a = 0; a < dimensions; ++a)
            total *= values.size();
        doc.mark_saved();
        const auto revision = doc.revision();
        for (size_t i = 0; i < total; ++i) {
            PreviewValues preview;
            size_t cursor = i;
            for (unsigned a = 0; a < dimensions; ++a) {
                preview[id(6 + a)] = values[cursor % values.size()];
                cursor /= values.size();
            }
            verify(doc, artifact, preview);
        }
        CHECK(doc.revision() == revision && !doc.modified() && encode(doc).bytes == artifact.bytes);
        verify(doc, artifact, {{id(6), -2}});
        verify(doc, artifact, {{id(6), 2}});
        DrawableFrame clamped;
        CHECK(evaluate_frame(doc, {{id(6), 2}}, clamped).ok());
        auto cp = std::find_if(clamped.parameters.begin(), clamped.parameters.end(),
                               [](auto &p) { return p.id == id(6); });
        CHECK(cp != clamped.parameters.end() && cp->clamped && cp->value == 1);
        if (!output.empty()) {
            auto path = output / ("parameter-" + std::to_string(dimensions) + "d.moc3");
            std::ofstream file(path, std::ios::binary);
            file.write(reinterpret_cast<const char *>(artifact.bytes.data()), artifact.bytes.size());
            CHECK(file.good());
        }
    }
    auto doc = animated_fixture(1);
    const auto old = encode(doc);
    auto binding = *doc.get_binding(id(9));
    auto middle = binding.keyforms[1];
    middle.positions[1].x += 34;
    CHECK(doc.set_mesh_keyform(id(9), middle).status.ok());
    auto edited = encode(doc);
    CHECK(edited.bytes != old.bytes);
    verify(doc, edited, {{id(6), 0}});
    verify(doc, edited, {{id(6), -0.5f}});
    verify(doc, edited, {{id(6), 0.5f}});
    DrawableFrame frame;
    CHECK(evaluate_frame(doc, {}, frame).ok());
    near(frame.drawables[0].positions[1].x, (328 + 34 - 271) / 100.0f);
    auto revision = doc.revision();
    auto bytes = encode(doc).bytes;
    binding.keyforms.pop_back();
    CHECK(doc.replace_binding(binding).status.code == "INCOMPLETE_KEYFORMS");
    binding = *doc.get_binding(id(9));
    binding.keyforms[1].keys = binding.keyforms[0].keys;
    CHECK(doc.replace_binding(binding).status.code == "DUPLICATE_KEYFORM");
    binding = *doc.get_binding(id(9));
    binding.axes[0].keys[1] = -1;
    CHECK(doc.replace_binding(binding).status.code == "INVALID_KEYS");
    binding = *doc.get_binding(id(9));
    binding.keyforms[0].positions[0].x = NAN;
    CHECK(doc.replace_binding(binding).status.code == "NON_FINITE");
    auto p = *doc.get_parameter(id(6));
    p.minimum = 0;
    CHECK(!doc.replace_parameter(p).status.ok());
    CHECK(doc.erase_object(id(6)).referrers == std::vector<std::string>{id(9)});
    CHECK(doc.revision() == revision && encode(doc).bytes == bytes);
    auto mesh = *doc.get_mesh(id(4));
    mesh.vertex_ids.pop_back();
    mesh.base_positions.pop_back();
    mesh.uvs.pop_back();
    mesh.triangles.pop_back();
    CHECK(doc.replace_mesh(mesh).status.code == "KEYFORMS_REQUIRED");
    auto forms = doc.get_binding(id(9))->keyforms;
    for (auto &f : forms)
        f.positions.pop_back();
    std::vector<VertexMapping> mapping{{91, 91}, {8, 8}, {77, 77}};
    CHECK(!doc.replace_mesh_with_keyforms(mesh, std::span<const VertexMapping>(mapping).first(2), forms)
               .status.ok());
    CHECK(doc.replace_mesh_with_keyforms(mesh, mapping, forms).status.ok());
    verify(doc, encode(doc), {{id(6), 0.5f}});
    CHECK(doc.create_parameter({id(0), "ParamOther", "other", -1, 1, 0}).status.ok());
    binding = *doc.get_binding(id(9));
    binding.axes[0].parameter_id = id(0);
    CHECK(doc.replace_binding(binding).status.ok());
    CHECK(doc.erase_object(id(6)).status.ok());
    verify(doc, encode(doc), {{id(0), 0.5f}});
    CHECK(doc.erase_object(id(9)).status.ok());
    CHECK(doc.erase_object(id(4)).status.ok());
    CHECK(doc.erase_object(id(2)).status.ok());
    verify(doc, encode(doc));
    // A one-key axis is valid; pad only the unreachable runtime gather span.
    doc = animated_fixture(1);
    binding = *doc.get_binding(id(9));
    binding.axes[0].keys = {0};
    binding.keyforms = {binding.keyforms[1]};
    CHECK(doc.replace_binding(binding).status.ok());
    verify(doc, encode(doc));
    verify(doc, encode(doc), {{id(6), 0.5f}});
}

static std::string sid(int n) {
    char value[37];
    std::snprintf(value, sizeof(value), "%08x-2222-4222-8222-222222222222", n);
    return value;
}

static Document scene_fixture(bool warp_root, bool quad) {
    auto doc = fixture();
    CHECK(doc.create_parameter({id(6), "ParamScene", "scene", -1, 1, 0}).status.ok());
    CHECK(doc.create_part({sid(1), "PartRoot", "root", "", true, 12}).status.ok());
    CHECK(doc.create_part({sid(2), "PartChild", "child", sid(1), true, -3}).status.ok());
    Transform root;
    root.id = sid(3);
    root.runtime_id = "RootTransform";
    root.part_id = sid(1);
    root.kind = warp_root ? TransformKind::warp : TransformKind::rotation;
    root.rotation = {{312, 207}, 13, 1.17f, true, false};
    root.base_angle = 7;
    root.rows = root.columns = 2;
    root.quad = quad;
    root.points = {{100, 390}, {290, 380}, {500, 370}, {90, 215}, {300, 200},
                   {520, 195}, {70, 40},   {305, 35},  {530, 20}};
    root.appearance = {0.83f, {0.9f, 0.8f, 0.95f}, {0.1f, 0.2f, 0.05f}};
    CHECK(doc.create_transform(root).status.ok());
    Transform child;
    child.id = sid(4);
    child.runtime_id = "ChildTransform";
    child.parent_id = root.id;
    child.part_id = sid(2);
    child.kind = warp_root ? TransformKind::rotation : TransformKind::warp;
    child.rotation = {{0.37f, 0.63f}, -24, 0.86f, false, true};
    child.base_angle = -9;
    child.rows = child.columns = 2;
    child.quad = quad;
    child.points = {{-1, -1},     {0.1f, -1.1f}, {1.2f, -1.0f}, {-1.1f, 0},  {0.15f, 0.2f},
                    {1.3f, 0.1f}, {-0.9f, 1.2f}, {0, 1.1f},     {1.1f, 1.4f}};
    child.appearance = {0.77f, {0.8f, 1, 0.9f}, {0.05f, 0.1f, 0.2f}};
    CHECK(doc.create_transform(child).status.ok());
    for (auto t : {root, child}) {
        SceneBinding b{sid(t.id == root.id ? 5 : 6), t.id, {{id(6), {-1, 0, 1}}}, {}};
        for (float key : {-1.f, 0.f, 1.f}) {
            SceneKeyform f;
            f.keys = {key};
            f.rotation = t.rotation;
            f.rotation.angle += key * 17;
            f.rotation.scale += key * 0.13f;
            f.rotation.origin.x += key * (t.parent_id.empty() ? 11 : 0.07f);
            f.appearance = t.appearance;
            f.appearance.opacity += key * 0.05f;
            if (t.kind == TransformKind::warp) {
                f.positions = t.points;
                for (size_t i = 0; i < f.positions.size(); ++i) {
                    f.positions[i].x += key * float(i % 3) * (t.parent_id.empty() ? 7 : 0.09f);
                    f.positions[i].y += key * float(i / 3) * (t.parent_id.empty() ? 3 : 0.03f);
                }
            }
            b.keyforms.push_back(f);
        }
        CHECK(doc.create_scene_binding(b).status.ok());
    }
    auto mesh = *doc.get_mesh(id(4));
    mesh.part_id = sid(2);
    mesh.deformer_id = child.id;
    mesh.base_positions = {{-0.25f, 0.15f}, {1.23f, 0.31f}, {3.1f, 1.2f}, {-2.2f, 0.83f}};
    mesh.draw_order = 18;
    mesh.appearance = {0.71f, {0.7f, 0.85f, 0.9f}, {0.1f, 0.05f, 0.17f}};
    mesh.masks = {id(5)};
    mesh.inverted_mask = warp_root;
    mesh.blend_mode = warp_root ? BlendMode::multiplicative : BlendMode::additive;
    CHECK(doc.replace_mesh(mesh).status.ok());
    auto other = *doc.get_mesh(id(5));
    other.draw_order = 25;
    CHECK(doc.replace_mesh(other).status.ok());
    SceneBinding part_binding{sid(7), sid(1), {{id(6), {-1, 0, 1}}}, {}};
    for (float key : {-1.f, 0.f, 1.f}) {
        SceneKeyform f;
        f.keys = {key};
        f.draw_order = key * 20 + 20;
        part_binding.keyforms.push_back(f);
    }
    CHECK(doc.create_scene_binding(part_binding).status.ok());
    return doc;
}

static void verify_scene(const std::filesystem::path &output) {
    for (bool warp_root : {false, true})
        for (bool quad : {false, true}) {
            auto doc = scene_fixture(warp_root, quad);
            auto artifact = encode(doc);
            for (float p : {-1.f, -0.5f, 0.f, 0.5f, 1.f})
                verify(doc, artifact, {{id(6), p}});
            if (!output.empty()) {
                auto name = std::string("nested-") + (warp_root ? "warp-rotation" : "rotation-warp") +
                            (quad ? "-quad" : "-triangle") + ".moc3";
                std::ofstream file(output / name, std::ios::binary);
                file.write(reinterpret_cast<const char *>(artifact.bytes.data()), artifact.bytes.size());
                CHECK(file.good());
            }
            auto rev = doc.revision();
            auto t = *doc.get_transform(sid(3));
            t.parent_id = sid(4);
            CHECK(doc.replace_transform(t).status.code == "RELATION_CYCLE");
            auto p = *doc.get_part(sid(1));
            p.parent_id = sid(2);
            CHECK(doc.replace_part(p).status.code == "RELATION_CYCLE");
            CHECK(!doc.erase_object(sid(3)).referrers.empty());
            CHECK(!doc.erase_object(id(5)).referrers.empty());
            CHECK(doc.revision() == rev);
            auto b = *doc.get_scene_binding(sid(6));
            auto f = b.keyforms[1];
            f.rotation.angle += 8;
            if (!f.positions.empty())
                f.positions[0].x += 0.1f;
            CHECK(doc.set_scene_keyform(b.id, f).status.ok());
            verify(doc, encode(doc));
            b = *doc.get_scene_binding(sid(6));
            b.keyforms.pop_back();
            rev = doc.revision();
            CHECK(doc.replace_scene_binding(b).status.code == "INCOMPLETE_KEYFORMS");
            CHECK(doc.revision() == rev);
            auto missing = *doc.get_mesh(id(4));
            missing.deformer_id = sid(999);
            CHECK(doc.replace_mesh(missing).status.code == "MISSING_TRANSFORM");
            missing = *doc.get_mesh(id(4));
            missing.masks.push_back(id(4));
            CHECK(!doc.replace_mesh(missing).status.ok());
            missing = *doc.get_mesh(id(4));
            missing.appearance.multiply[1] = NAN;
            CHECK(!doc.replace_mesh(missing).status.ok());
            auto bad_transform = *doc.get_transform(sid(3));
            bad_transform.runtime_id = std::string(64, 'x');
            auto unrepresentable = doc;
            CHECK(unrepresentable.replace_transform(bad_transform).status.ok());
            Moc3Artifact unchanged = artifact;
            CHECK(encode_moc3(unrepresentable, unchanged).code == "UNREPRESENTABLE_ID");
            CHECK(unchanged.bytes == artifact.bytes);
            auto moved_canvas = doc;
            auto canvas = moved_canvas.canvas();
            canvas.origin.x += 13;
            CHECK(moved_canvas.replace_canvas(canvas).status.ok());
            verify(moved_canvas, encode(moved_canvas));
            // Disabled hierarchy must agree on flags and immutable source metadata.
            auto disabled = doc;
            auto part = *disabled.get_part(sid(1));
            part.enabled = false;
            CHECK(disabled.replace_part(part).status.ok());
            verify(disabled, encode(disabled));
            // Explicit unbinding/deletion cannot leave hidden source dependencies.
            CHECK(doc.erase_object(sid(6)).status.ok());
            auto m = *doc.get_mesh(id(4));
            m.deformer_id = sid(3);
            CHECK(doc.replace_mesh(m).status.ok());
            CHECK(doc.erase_object(sid(4)).status.ok());
            verify(doc, encode(doc));
        }
}

static Mesh rectangle(int number, const std::string &asset, float x, float y, float w, float h) {
    return {sid(number),
            "rect",
            asset,
            {1, 2, 3, 4},
            {{x, y}, {x + w, y}, {x + w, y + h}, {x, y + h}},
            {{0, 0}, {1, 0}, {1, 1}, {0, 1}},
            {{{1, 2, 3}}, {{1, 3, 4}}},
            "Mesh" + std::to_string(number)};
}

static Document gpu_fixture() {
    Document doc;
    CHECK(doc.initialize(id(1), {640, 480, {271, 193}, 100}).ok());
    CHECK(doc.add_asset({id(2), "gradient", "textures/0.png", 8, 8}).status.ok());
    CHECK(doc.add_asset({id(3), "solid", "textures/1.png", 8, 8}).status.ok());
    CHECK(doc.create_parameter({id(6), "ParamMask", "mask", -1, 1, 0}).status.ok());
    CHECK(doc.create_part({sid(100), "Foreground", "foreground", "", true, 100}).status.ok());
    auto bg = rectangle(101, id(3), 20, 20, 600, 440);
    bg.draw_order = 0;
    CHECK(doc.create_mesh(bg).status.ok());
    for (int row = 0; row < 3; ++row)
        for (int col = 0; col < 3; ++col) {
            int cell = row * 3 + col;
            float x = 40 + 190 * col, y = 40 + 135 * row;
            auto mask = rectangle(200 + cell, id(3), x, y, 80, 100);
            mask.appearance.opacity = 0;
            mask.draw_order = 1;
            CHECK(doc.create_mesh(mask).status.ok());
            MeshBinding b{sid(400 + cell), mask.id, {{id(6), {-1, 0, 1}}}, {}};
            for (float key : {-1.f, 0.f, 1.f}) {
                MeshKeyform f;
                f.keys = {key};
                f.positions = mask.base_positions;
                for (auto &p : f.positions)
                    p.x += key * 20;
                f.appearance.opacity = 0;
                b.keyforms.push_back(f);
            }
            CHECK(doc.create_binding(b).status.ok());
            auto m = rectangle(300 + cell, id(2), x, y, 160, 100);
            m.part_id = sid(100);
            m.draw_order = float(10 + cell);
            m.blend_mode = BlendMode(col);
            m.appearance = {0.65f, {0.8f, 0.7f, 0.9f}, {0.1f, 0.05f, 0.15f}};
            if (row)
                m.masks = {mask.id};
            m.inverted_mask = row == 2;
            CHECK(doc.create_mesh(m).status.ok());
        }
    return doc;
}

static void write_fixture_png(const std::filesystem::path &path, int slot) {
    png_image image{};
    image.version = PNG_IMAGE_VERSION;
    image.width = image.height = 8;
    image.format = PNG_FORMAT_RGBA;
    std::vector<uint8_t> pixels;
    for (int y = 0; y < 8; ++y)
        for (int x = 0; x < 8; ++x) {
            pixels.push_back(slot ? 64 : x * 31);
            pixels.push_back(slot ? 128 : y * 31);
            pixels.push_back(slot ? 192 : 240);
            pixels.push_back(255);
        }
    CHECK(png_image_write_to_file(&image, path.string().c_str(), 0, pixels.data(), 0, nullptr));
}

static void write_gpu_source(const Document &doc, const std::filesystem::path &path) {
    std::ofstream o(path);
    o << std::setprecision(9);
    auto q = [&](const std::string &v) { o << std::quoted(v); };
    auto positions = [&](const std::vector<Vec2> &p) {
        o << "[";
        for (size_t i = 0; i < p.size(); ++i)
            o << (i ? "," : "") << "[" << p[i].x << "," << p[i].y << "]";
        o << "]";
    };
    auto appearance = [&](const Appearance &a) {
        o << "{\"opacity\":" << a.opacity << ",\"multiply\":[" << a.multiply[0] << "," << a.multiply[1] << ","
          << a.multiply[2] << "],\"screen\":[" << a.screen[0] << "," << a.screen[1] << "," << a.screen[2]
          << "]}";
    };
    o << "{\"format\":\"kasane-project\",\"format_version\":5,\"document\":{\"id\":";
    q(doc.id());
    o << ",\"canvas\":[640,480],\"canvas_origin\":[271,193],\"pixels_per_unit\":100,\"assets\":[";
    for (size_t i = 0; i < doc.asset_order().size(); ++i) {
        auto &a = *doc.get_asset(doc.asset_order()[i]);
        o << (i ? "," : "") << "{\"id\":";
        q(a.id);
        o << ",\"name\":";
        q(a.name);
        o << ",\"source\":";
        q(a.source);
        o << ",\"width\":8,\"height\":8}";
    }
    o << "],\"parts\":[{\"id\":";
    q(sid(100));
    o << ",\"runtime_id\":\"Foreground\",\"name\":\"foreground\",\"parent_id\":\"\",\"enabled\":true,\"draw_"
         "order\":100}],\"transforms\":[],\"scene_bindings\":[],\"deformers\":[],\"deformation_links\":[],"
         "\"organization_links\":[],\"meshes\":[";
    for (size_t i = 0; i < doc.mesh_order().size(); ++i) {
        auto &m = *doc.get_mesh(doc.mesh_order()[i]);
        o << (i ? "," : "") << "{\"id\":";
        q(m.id);
        o << ",\"runtime_id\":";
        q(m.runtime_id);
        o << ",\"name\":";
        q(m.name);
        o << ",\"texture_asset_id\":";
        q(m.texture_asset_id);
        o << ",\"vertex_ids\":[1,2,3,4],\"base_positions\":";
        positions(m.base_positions);
        o << ",\"uvs\":";
        positions(m.uvs);
        o << ",\"triangles\":[1,2,3,1,3,4],\"properties\":{\"part_id\":";
        q(m.part_id);
        o << ",\"deformer_id\":\"\",\"appearance\":";
        appearance(m.appearance);
        o << ",\"draw_order\":" << m.draw_order.value_or(float(i)) << ",\"blend_mode\":" << int(m.blend_mode)
          << ",\"enabled\":true,\"double_sided\":true,\"inverted_mask\":"
          << (m.inverted_mask ? "true" : "false") << ",\"masks\":[";
        for (size_t k = 0; k < m.masks.size(); ++k) {
            if (k)
                o << ",";
            q(m.masks[k]);
        }
        o << "]}}";
    }
    o << "],\"parameters\":[{\"id\":";
    q(id(6));
    o << ",\"runtime_id\":\"ParamMask\",\"name\":\"mask\",\"minimum\":-1,\"maximum\":1,\"default_value\":0}],"
         "\"bindings\":[";
    for (size_t i = 0; i < doc.binding_order().size(); ++i) {
        auto &b = *doc.get_binding(doc.binding_order()[i]);
        o << (i ? "," : "") << "{\"id\":";
        q(b.id);
        o << ",\"mesh_id\":";
        q(b.mesh_id);
        o << ",\"axes\":[{\"parameter_id\":";
        q(id(6));
        o << ",\"keys\":[-1,0,1]}],\"keyforms\":[";
        for (size_t k = 0; k < b.keyforms.size(); ++k) {
            auto &f = b.keyforms[k];
            o << (k ? "," : "") << "{\"keys\":[" << f.keys[0] << "],\"positions\":";
            positions(f.positions);
            o << ",\"appearance\":";
            appearance(f.appearance);
            o << "}";
        }
        o << "]}";
    }
    o << "]}}\n";
    CHECK(o.good());
}

static void verify_package(const std::filesystem::path &output) {
    auto doc = gpu_fixture();
    auto artifact = encode(doc);
    for (float p : {-1.f, 0.f, 1.f})
        verify(doc, artifact, {{id(6), p}});
    auto base = output / "publication";
    if (output.empty()) {
        std::random_device random;
        bool claimed = false;
        for (unsigned attempt = 0; attempt < 100; ++attempt) {
            base = std::filesystem::temp_directory_path() /
                   ("kasane-package-" + std::to_string(random()) + "-" + std::to_string(attempt));
            if (std::filesystem::create_directory(base)) {
                claimed = true;
                break;
            }
        }
        CHECK(claimed);
    }
    std::filesystem::create_directories(base / "assets/textures");
    for (int i = 0; i < 2; ++i)
        write_fixture_png(base / "assets/textures" / (std::to_string(i) + ".png"), i);
    PackageOptions options{base / "assets", base / "gpu-package", [&](const Moc3Artifact &a) {
                               verify(doc, a);
                               return Status{};
                           }};
    CHECK(publish_package(doc, options).ok());
    auto read = [&]() {
        std::ifstream f(options.destination / "model.moc3", std::ios::binary);
        return std::vector<uint8_t>(std::istreambuf_iterator<char>(f), {});
    };
    auto bytes = read();
    CHECK(bytes == artifact.bytes);
    write_gpu_source(doc, base / "gpu-source.json");
    auto broken = *doc.get_asset(id(2));
    broken.source = "missing.png";
    CHECK(doc.replace_asset(broken).status.ok());
    CHECK(publish_package(doc, options).code == "MISSING_TEXTURE");
    CHECK(read() == bytes);
    broken.source = "textures/0.png";
    broken.width = 9;
    CHECK(doc.replace_asset(broken).status.ok());
    CHECK(publish_package(doc, options).code == "RESOURCE_MISMATCH");
    CHECK(read() == bytes);
    broken.width = 8;
    CHECK(doc.replace_asset(broken).status.ok());
    options.validate = [](const auto &) {
        return Status::error("TEST_REJECTION", "injected runtime rejection");
    };
    CHECK(publish_package(doc, options).code == "TEST_REJECTION");
    CHECK(read() == bytes);
    options.validate = {};
    CHECK(publish_package(doc, options).code == "MISSING_VALIDATOR");
    options.validate = [](const auto &) { return Status{}; };
    std::ofstream bad(base / "assets/textures/0.png", std::ios::binary);
    bad << "broken PNG";
    bad.close();
    CHECK(publish_package(doc, options).code == "INVALID_PNG");
    CHECK(read() == bytes);
    write_fixture_png(base / "assets/textures/0.png", 0);
    std::ofstream blocker(base / "blocker");
    blocker << "not a directory";
    blocker.close();
    options.destination = base / "blocker/output";
    CHECK(!publish_package(doc, options).ok());
    if (output.empty())
        std::filesystem::remove_all(base);
}

static void reject_malformed(const Moc3Artifact &a) {
    using Buffer = std::unique_ptr<void, decltype(&std::free)>;
    Buffer memory(std::aligned_alloc(64, (a.bytes.size() + 63) & ~size_t(63)), &std::free);
    CHECK(memory);
    auto reset = [&] { std::memcpy(memory.get(), a.bytes.data(), a.bytes.size()); };
    reset();
    CHECK(!csmHasMocConsistency(memory.get(), 64));
    reset();
    static_cast<uint8_t *>(memory.get())[4] = 255;
    CHECK(!csmHasMocConsistency(memory.get(), unsigned(a.bytes.size())));
    auto read32 = [&](size_t at) {
        uint32_t v = 0;
        for (int k = 0; k < 4; ++k)
            v |= uint32_t(a.bytes[at + k]) << (k * 8);
        return v;
    };
    reset(); // ArtMesh count cannot extend beyond its section buffers.
    auto counts = read32(64);
    for (int k = 0; k < 4; ++k)
        static_cast<uint8_t *>(memory.get())[counts + 4 * 4 + k] = 0x7f;
    CHECK(!csmHasMocConsistency(memory.get(), unsigned(a.bytes.size())));
    reset(); // count_info offset is beyond the provided buffer.
    for (unsigned i = 0; i < 4; ++i)
        static_cast<uint8_t *>(memory.get())[64 + i] = 0x7f;
    CHECK(!csmHasMocConsistency(memory.get(), unsigned(a.bytes.size())));
}

int main(int argc, char **argv) {
    try {
        csmSetLogFunction([](const char *s) { std::cerr << s; });
        auto doc = fixture();
        auto initial = encode(doc);
        verify(doc, initial);
        reject_malformed(initial);
        DrawableFrame initial_frame;
        CHECK(evaluate_frame(doc, {}, initial_frame).ok());
        const auto &snapshot = initial_frame.drawables;
        // Independent expected values keep the encoder and evaluator from
        // passing together with a shared coordinate/slot/winding mistake.
        near(snapshot[0].positions[0].x, -1.7f);
        near(snapshot[0].positions[0].y, 1.5f);
        near(snapshot[1].uvs[0].y, 0.95f);
        CHECK(snapshot[0].indices == std::vector<uint32_t>({0, 2, 1, 0, 3, 2}));
        CHECK(snapshot[0].texture_slot == 0 && snapshot[1].texture_slot == 1);
        CHECK(initial.bytes[4] == 5 && initial.bytes[5] == 0);
        CHECK(doc.rename_mesh(id(4), "renamed").status.ok());
        CHECK(encode(doc).bytes == initial.bytes);
        VertexId v = 8;
        Vec2 p{338, 69};
        CHECK(doc.set_vertex_positions(id(4), {&v, 1}, {&p, 1}).status.ok());
        auto edited = encode(doc);
        CHECK(edited.bytes != initial.bytes);
        verify(doc, edited);
        DrawableFrame after_frame;
        CHECK(evaluate_frame(doc, {}, after_frame).ok());
        const auto &after = after_frame.drawables;
        CHECK(after[1].positions == snapshot[1].positions);
        auto mesh = *doc.get_mesh(id(4));
        mesh.vertex_ids = {2, 6, 18};
        mesh.base_positions = {{100, 40}, {330, 70}, {350, 290}};
        mesh.uvs = {{0, 0}, {1, 0}, {0.8f, 1}};
        mesh.triangles = {{{2, 6, 18}}};
        CHECK(doc.replace_mesh(mesh).status.ok());
        verify(doc, encode(doc));
        // Failure preserves both output and Document state.
        mesh.runtime_id = std::string(64, 'x');
        CHECK(doc.replace_mesh(mesh).status.ok());
        auto before = doc.revision();
        auto sentinel = initial;
        auto invalid = encode_moc3(doc, sentinel);
        CHECK(invalid.code == "UNREPRESENTABLE_ID" && invalid.message.find(id(4)) != std::string::npos);
        CHECK(sentinel.bytes == initial.bytes && doc.revision() == before);
        mesh.runtime_id = "bad\nID";
        CHECK(doc.replace_mesh(mesh).status.ok());
        CHECK(!encode_moc3(doc, sentinel).ok());
        mesh.runtime_id = "ArtMeshB";
        before = doc.revision();
        CHECK(!doc.replace_mesh(mesh).status.ok());
        CHECK(doc.revision() == before);
        doc = fixture();
        CHECK(doc.begin_transaction().ok());
        CHECK(encode_moc3(doc, sentinel).code == "TRANSACTION_ACTIVE");
        CHECK(doc.cancel_transaction().ok());
        Deformer def;
        def.id = id(6);
        CHECK(doc.create_deformer(def).status.ok());
        CHECK(encode_moc3(doc, sentinel).code == "UNSUPPORTED_FEATURE");
        doc = fixture();
        CHECK(doc.set_parent(id(4), id(5), true).status.ok());
        CHECK(encode_moc3(doc, sentinel).code == "UNSUPPORTED_FEATURE");
        Document bad;
        CHECK(!bad.initialize(id(1), {640, 480, {0, 0}, 0}).ok());
        CHECK(!bad.initialize(id(1), {640, 480, {NAN, 0}, 100}).ok());
        const auto output = argc > 1 ? std::filesystem::path(argv[1]) : std::filesystem::path{};
        if (!output.empty())
            std::filesystem::create_directories(output);
        verify_keyforms(output);
        verify_scene(output);
        verify_package(output);
        if (argc > 1) {
            auto dir = std::filesystem::path(argv[1]);
            std::filesystem::create_directories(dir);
            std::ofstream moc(dir / "model.moc3", std::ios::binary);
            moc.write(reinterpret_cast<const char *>(initial.bytes.data()), initial.bytes.size());
            CHECK(moc.good());
            std::ofstream json(dir / "model.model3.json");
            json << initial.model3_json;
            CHECK(json.good());
            std::ofstream evidence(dir / "comparisons.json");
            evidence << "[\n";
            for (size_t i = 0; i < comparisons.size(); ++i)
                evidence << (i ? ",\n" : "") << comparisons[i];
            evidence << "\n]\n";
            CHECK(evidence.good());
            std::ofstream sampling(dir / "samples.json");
            sampling << "[\n";
            for (size_t i = 0; i < samples.size(); ++i)
                sampling << (i ? ",\n" : "") << samples[i];
            sampling << "\n]\n";
            CHECK(sampling.good());
        }
        std::cout << "{\"status\":\"passed\",\"case\":\"m1-document-export-regressions\",\"core_version\":"
                  << csmGetVersion() << ",\"max_error\":" << max_error << "}\n";
        return 0;
    } catch (const std::exception &e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}
