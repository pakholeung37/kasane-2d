// SPDX-License-Identifier: MIT
// Read-only M2 oracle: evaluate an exported MOC3 in one provider per process.
#ifdef KASANE_OFFICIAL_CORE
#include <Live2DCubismCore.h>
#else
#include <PurismCore.h>
#if PSM_COMPAT_VERSION < 0x06000000L
#define csmGetRenderOrders csmGetDrawableRenderOrders
#endif
#endif
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <vector>

int main(int argc, char **argv) {
    try {
        if (argc != 2)
            throw std::runtime_error("Expected a model.moc3 path; parameter samples on stdin");
        std::ifstream file(argv[1], std::ios::binary);
        std::vector<char> bytes(std::istreambuf_iterator<char>(file), {});
        if (bytes.empty())
            throw std::runtime_error("Empty MOC3");
        using Buffer = std::unique_ptr<void, decltype(&std::free)>;
        Buffer moc_memory(std::aligned_alloc(64, (bytes.size() + 63) & ~size_t(63)), &std::free);
        if (!moc_memory)
            throw std::runtime_error("Allocation failed");
        std::memcpy(moc_memory.get(), bytes.data(), bytes.size());
        if (!csmHasMocConsistency(moc_memory.get(), unsigned(bytes.size())))
            throw std::runtime_error("Invalid MOC3");
        auto moc = csmReviveMocInPlace(moc_memory.get(), unsigned(bytes.size()));
        if (!moc)
            throw std::runtime_error("Cannot revive MOC3");
        auto size = csmGetSizeofModel(moc);
        Buffer model_memory(std::aligned_alloc(16, (size + 15) & ~size_t(15)), &std::free);
        if (!size || !model_memory)
            throw std::runtime_error("Model allocation failed");
        auto model = csmInitializeModelInPlace(moc, model_memory.get(), size);
        if (!model)
            throw std::runtime_error("Cannot initialize model");
        std::cout << std::setprecision(17);
        int samples;
        if (!(std::cin >> samples) || samples <= 0)
            throw std::runtime_error("Missing samples");
        int offscreen_count = csmGetOffscreenCount(model);
        std::cout << "{\"core_version\":" << csmGetVersion()
                  << ",\"offscreen_count\":" << offscreen_count;
        if (offscreen_count > 0) {
            auto part_offs = csmGetPartOffscreenIndices(model);
            int part_count = csmGetPartCount(model);
            std::cout << ",\"part_offscreen_indices\":[";
            for (int p = 0; p < part_count; ++p) {
                std::cout << (p ? "," : "") << (part_offs ? part_offs[p] : -1);
            }
            std::cout << "]";
        }
        std::cout << ",\"samples\":[";
        std::vector<std::string> offscreen_records;
        for (int sample = 0; sample < samples; ++sample) {
            for (int i = 0; i < csmGetParameterCount(model); ++i)
                if (!(std::cin >> csmGetParameterValues(model)[i]))
                    throw std::runtime_error("Missing parameter");
            csmUpdateModel(model);
            std::cout << (sample ? "," : "") << "[";
            for (int i = 0; i < csmGetDrawableCount(model); ++i) {
                auto flags = csmGetDrawableConstantFlags(model)[i];
                std::cout << (i ? "," : "") << "{\"runtime_id\":" << std::quoted(csmGetDrawableIds(model)[i])
                          << ",\"texture_slot\":" << csmGetDrawableTextureIndices(model)[i]
                          << ",\"draw_order\":" << csmGetDrawableDrawOrders(model)[i]
                          << ",\"render_order\":" << csmGetRenderOrders(model)[i]
                          << ",\"opacity\":" << csmGetDrawableOpacities(model)[i]
                          << ",\"visible\":" << ((csmGetDrawableDynamicFlags(model)[i] & csmIsVisible) ? "true" : "false")
                          << ",\"double_sided\":" << ((flags & csmIsDoubleSided) ? "true" : "false")
                          << ",\"inverted_mask\":" << ((flags & csmIsInvertedMask) ? "true" : "false")
                          << ",\"blend_mode\":"
                          << ((flags & csmBlendAdditive)         ? 1
                              : (flags & csmBlendMultiplicative) ? 2
                                                                 : 0);
                auto vectors = [&](const char *key, const csmVector2 *values) {
                    std::cout << ",\"" << key << "\":[";
                    for (int k = 0; k < csmGetDrawableVertexCounts(model)[i]; ++k)
                        std::cout << (k ? "," : "") << "[" << values[k].X << "," << values[k].Y << "]";
                    std::cout << "]";
                };
                vectors("positions", csmGetDrawableVertexPositions(model)[i]);
                vectors("uvs", csmGetDrawableVertexUvs(model)[i]);
                std::cout << ",\"indices\":[";
                for (int k = 0; k < csmGetDrawableIndexCounts(model)[i]; ++k)
                    std::cout << (k ? "," : "") << csmGetDrawableIndices(model)[i][k];
                std::cout << "],\"mask_indices\":[";
                for (int k = 0; k < csmGetDrawableMaskCounts(model)[i]; ++k)
                    std::cout << (k ? "," : "") << csmGetDrawableMasks(model)[i][k];
                auto color = [&](const char *key, const csmVector4 &v) {
                    std::cout << "],\"" << key << "\":[" << v.X << "," << v.Y << "," << v.Z << "," << v.W;
                };
                color("multiply_color", csmGetDrawableMultiplyColors(model)[i]);
                color("screen_color", csmGetDrawableScreenColors(model)[i]);
                std::cout << "]}";
            }
            std::cout << "]";
            if (offscreen_count > 0) {
                std::ostringstream oss;
                oss << std::setprecision(17) << "[";
                for (int o = 0; o < offscreen_count; ++o) {
                    auto b_mode = csmGetOffscreenBlendModes(model)[o];
                    auto op = csmGetOffscreenOpacities(model)[o];
                    auto owner = csmGetOffscreenOwnerIndices(model)[o];
                    auto mul = csmGetOffscreenMultiplyColors(model)[o];
                    auto scr = csmGetOffscreenScreenColors(model)[o];
                    auto flags = csmGetOffscreenConstantFlags(model)[o];
                    int m_count = csmGetOffscreenMaskCounts(model)[o];
                    auto masks = csmGetOffscreenMasks(model)[o];
                    oss << (o ? "," : "") << "{\"index\":" << o
                        << ",\"owner_index\":" << owner
                        << ",\"blend_mode\":" << b_mode
                        << ",\"opacity\":" << op
                        << ",\"flags\":" << (int)flags
                        << ",\"multiply_color\":[" << mul.X << "," << mul.Y << "," << mul.Z << "," << mul.W << "]"
                        << ",\"screen_color\":[" << scr.X << "," << scr.Y << "," << scr.Z << "," << scr.W << "]"
                        << ",\"mask_indices\":[";
                    for (int k = 0; k < m_count; ++k) {
                        oss << (k ? "," : "") << masks[k];
                    }
                    oss << "]}";
                }
                oss << "]";
                offscreen_records.push_back(oss.str());
            }
        }
        std::cout << "]";
        if (offscreen_count > 0) {
            std::cout << ",\"offscreen_samples\":[";
            for (size_t s = 0; s < offscreen_records.size(); ++s) {
                std::cout << (s ? "," : "") << offscreen_records[s];
            }
            std::cout << "]";
        }
        std::cout << "}\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
