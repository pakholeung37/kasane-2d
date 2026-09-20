// SPDX-License-Identifier: MIT
#include <kasane/project.hpp>
#include <openssl/evp.h>
#include <png.h>

namespace kasane {
std::string content_sha256(std::span<const uint8_t> bytes) {
    unsigned char digest[EVP_MAX_MD_SIZE];
    unsigned int length = 0;
    if (EVP_Digest(bytes.data(), bytes.size(), digest, &length, EVP_sha256(), nullptr) != 1 || length != 32)
        throw std::runtime_error("SHA-256 calculation failed");
    const char hex[] = "0123456789abcdef";
    std::string result;
    for (unsigned int i = 0; i < length; ++i) {
        result += hex[digest[i] >> 4];
        result += hex[digest[i] & 15];
    }
    return result;
}

Status decode_png(std::span<const uint8_t> bytes, AssetData &output) {
    png_image image{};
    image.version = PNG_IMAGE_VERSION;
    if (!png_image_begin_read_from_memory(&image, bytes.data(), bytes.size())) {
        png_image_free(&image);
        return Status::error("INVALID_PNG", "PNG header cannot be decoded");
    }
    if (uint64_t(image.width) * image.height > 268435456) {
        png_image_free(&image);
        return Status::error("CAPACITY", "PNG exceeds 1 GiB decoded");
    }
    image.format = PNG_FORMAT_RGBA;
    AssetData data;
    data.width = image.width;
    data.height = image.height;
    data.rgba.resize(PNG_IMAGE_SIZE(image));
    bool decoded = png_image_finish_read(&image, nullptr, data.rgba.data(), 0, nullptr);
    png_image_free(&image);
    if (!decoded)
        return Status::error("INVALID_PNG", "PNG data cannot be decoded");
    data.bytes.assign(bytes.begin(), bytes.end());
    data.sha256 = content_sha256(bytes);
    output = std::move(data);
    return {};
}

Status read_project_asset(io::FileSystem &filesystem, const io::fs::path &root, const ImageAsset &asset,
                          AssetData &output) {
    try {
        auto path = io::resolve_asset(filesystem, root, asset.source);
        auto bytes = filesystem.read(path);
        AssetData data;
        if (auto s = decode_png(bytes, data); !s.ok()) {
            s.message = asset.id + ": " + s.message;
            return s;
        }
        if (data.width != asset.width || data.height != asset.height)
            return Status::error("RESOURCE_DIMENSIONS", asset.id + ": PNG dimensions differ from metadata");
        if (!asset.sha256.empty() && data.sha256 != asset.sha256)
            return Status::error("RESOURCE_HASH", asset.id + ": PNG differs from saved SHA-256");
        output = std::move(data);
        return {};
    } catch (const io::Error &e) {
        return Status::error("RESOURCE_IO", asset.id + ": " + e.what());
    }
}
} // namespace kasane
