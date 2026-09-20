// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/document.hpp>

namespace kasane {
struct TextureSlot {
    std::string asset_id, source, package_path;
    uint32_t width, height;
};

struct Moc3Artifact {
    std::vector<uint8_t> bytes;
    std::string model3_json;
    std::vector<TextureSlot> textures;
};

// MOC3 v5 meshes, Parts and nested Rotation/Warp with ordinary Keyforms. Unsupported
// semantics are rejected; output is unchanged on failure.
Status encode_moc3(const Document &, Moc3Artifact &);
} // namespace kasane
