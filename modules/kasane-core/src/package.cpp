// SPDX-License-Identifier: MIT
#include <kasane/package.hpp>
#include <png.h>
#include <fstream>
#include <random>
#include <sstream>

namespace kasane {
namespace {
std::string json_string(const std::string &value) {
    std::string s = "\"";
    const char *hex = "0123456789abcdef";
    for (unsigned char c : value) {
        if (c == '"' || c == '\\') {
            s += '\\';
            s += char(c);
        } else if (c < 32) {
            s += "\\u00";
            s += hex[c >> 4];
            s += hex[c & 15];
        } else
            s += char(c);
    }
    return s + '"';
}

void write(const std::filesystem::path &p, const void *data, size_t count) {
    std::ofstream file(p, std::ios::binary);
    file.write(static_cast<const char *>(data), std::streamsize(count));
    file.close();
    if (!file)
        throw std::runtime_error("write failed: " + p.string());
}
} // namespace

Status publish_package(const Document &doc, const PackageOptions &options) {
    namespace fs = std::filesystem;
    if (!options.validate)
        return Status::error("MISSING_VALIDATOR",
                             "Package publication requires an explicit runtime validation gate");
    Moc3Artifact artifact;
    if (auto s = encode_moc3(doc, artifact); !s.ok())
        return s;
    fs::path stage, backup, destination;
    bool moved = false, published = false;
    try {
        destination = fs::absolute(options.destination).lexically_normal();
        if (options.destination.empty() || destination == destination.root_path() ||
            destination.filename().empty())
            return Status::error("INVALID_DESTINATION", options.destination.string());
        if (fs::exists(destination) && (!fs::is_directory(destination) || fs::is_symlink(destination)))
            return Status::error("INVALID_DESTINATION", destination.string());
        std::vector<std::vector<uint8_t>> textures;
        for (const auto &slot : artifact.textures) {
            auto source = fs::path(slot.source);
            if (source.is_relative())
                source = options.asset_root / source;
            std::ifstream file(source, std::ios::binary);
            if (!file)
                return Status::error("MISSING_TEXTURE", slot.asset_id + ": " + source.string());
            auto bytes = std::vector<uint8_t>(std::istreambuf_iterator<char>(file), {});
            png_image image{};
            image.version = PNG_IMAGE_VERSION;
            if (!png_image_begin_read_from_memory(&image, bytes.data(), bytes.size())) {
                png_image_free(&image);
                return Status::error("INVALID_PNG", slot.asset_id);
            }
            if (image.width != slot.width || image.height != slot.height) {
                png_image_free(&image);
                return Status::error("RESOURCE_MISMATCH",
                                     slot.asset_id + ": PNG dimensions differ from metadata");
            }
            image.format = PNG_FORMAT_RGBA;
            // Resource boundary: cap decoded data, never allocate from unchecked input.
            if (uint64_t(image.width) * image.height > 268435456) {
                png_image_free(&image);
                return Status::error("CAPACITY", slot.asset_id + ": PNG exceeds 1 GiB decoded");
            }
            std::vector<uint8_t> pixels(PNG_IMAGE_SIZE(image));
            bool ok = png_image_finish_read(&image, nullptr, pixels.data(), 0, nullptr);
            png_image_free(&image);
            if (!ok)
                return Status::error("INVALID_PNG", slot.asset_id + ": incomplete PNG data");
            textures.push_back(std::move(bytes));
        }
        if (auto s = options.validate(artifact); !s.ok())
            return s;
        fs::create_directories(destination.parent_path());
        std::random_device rng;
        for (int attempt = 0; attempt < 32; ++attempt) {
            auto candidate = destination.parent_path() /
                             ("." + destination.filename().string() + "-stage-" + std::to_string(rng()));
            if (fs::create_directory(candidate)) {
                stage = candidate;
                break;
            }
        }
        if (stage.empty())
            throw std::runtime_error("Cannot allocate staging directory");
        fs::create_directory(stage / "textures");
        write(stage / "model.moc3", artifact.bytes.data(), artifact.bytes.size());
        write(stage / "model.model3.json", artifact.model3_json.data(), artifact.model3_json.size());
        std::ostringstream report;
        report << "{\"status\":\"passed\",\"moc_version\":5,\"source_revision\":" << doc.revision()
               << ",\"textures\":[";
        for (size_t i = 0; i < textures.size(); ++i) {
            auto &slot = artifact.textures[i];
            write(stage / slot.package_path, textures[i].data(), textures[i].size());
            report << (i ? "," : "") << "{\"asset_id\":" << json_string(slot.asset_id)
                   << ",\"source\":" << json_string(slot.source)
                   << ",\"path\":" << json_string(slot.package_path) << ",\"width\":" << slot.width
                   << ",\"height\":" << slot.height << "}";
        }
        report << "]}\n";
        auto text = report.str();
        write(stage / "export-report.json", text.data(), text.size());
        backup = stage;
        backup += "-previous";
        if (fs::exists(destination)) {
            fs::rename(destination, backup);
            moved = true;
        }
        fs::rename(stage, destination);
        published = true;
        if (moved) {
            std::error_code ignored;
            fs::remove_all(backup, ignored);
        }
        return {};
    } catch (const std::exception &e) {
        std::error_code ec;
        if (moved && !published) {
            fs::rename(backup, destination, ec);
            if (ec)
                return Status::error("ROLLBACK_FAILED", std::string(e.what()) +
                                                            "; previous package retained at " +
                                                            backup.string());
        }
        if (!stage.empty() && !published)
            fs::remove_all(stage, ec);
        return Status::error("PACKAGE_IO", e.what());
    }
}
} // namespace kasane
