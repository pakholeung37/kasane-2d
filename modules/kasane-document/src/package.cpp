// SPDX-License-Identifier: MIT
#include <kasane/package.hpp>
#include <png.h>
#include <kasane/filesystem.hpp>
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

} // namespace

Status publish_package(const Document &doc, const PackageOptions &options) {
    namespace fs = std::filesystem;
    if (!options.validate)
        return Status::error("MISSING_VALIDATOR",
                             "Package publication requires an explicit runtime validation gate");
    Moc3Artifact artifact;
    if (auto s = encode_moc3(doc, artifact); !s.ok())
        return s;
    auto filesystem = options.filesystem ? options.filesystem : io::native_filesystem();
    fs::path stage, backup, destination;
    std::unique_ptr<io::Lock> lock;
    bool moved = false, published = false;
    try {
        destination = io::local_path(options.destination);
        if (options.destination.empty() || destination == destination.root_path() ||
            destination.filename().empty())
            return Status::error("INVALID_DESTINATION", options.destination.string());
        if (filesystem->info(destination).exists &&
            (!filesystem->info(destination).directory || filesystem->info(destination).symlink))
            return Status::error("INVALID_DESTINATION", destination.string());
        std::vector<std::vector<uint8_t>> textures;
        for (const auto &slot : artifact.textures) {
            auto source = fs::path(slot.source);
            if (source.is_relative())
                source = options.asset_root / source;
            io::Bytes bytes;
            try {
                bytes = filesystem->read(io::local_path(source));
            } catch (const io::Error &e) {
                return Status::error("MISSING_TEXTURE", slot.asset_id + ": " + e.what());
            }
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
        filesystem->create_directories(destination.parent_path());
        lock = filesystem->lock(destination.parent_path());
        stage = destination.parent_path() / (".kasane-package-" + io::unique_name());
        filesystem->create_directory_new(stage);
        filesystem->create_directories(stage / "textures");
        io::write_new(*filesystem, stage / "model.moc3", artifact.bytes);
        auto write_text = [&](const fs::path &path, const std::string &text) {
            io::write_new(*filesystem, path,
                          std::span(reinterpret_cast<const uint8_t *>(text.data()), text.size()));
        };
        write_text(stage / "model.model3.json", artifact.model3_json);
        std::ostringstream report;
        report << "{\"status\":\"passed\",\"moc_version\":5,\"source_revision\":" << doc.revision()
               << ",\"textures\":[";
        for (size_t i = 0; i < textures.size(); ++i) {
            auto &slot = artifact.textures[i];
            io::write_new(*filesystem, stage / slot.package_path, textures[i]);
            report << (i ? "," : "") << "{\"asset_id\":" << json_string(slot.asset_id)
                   << ",\"source\":" << json_string(slot.source)
                   << ",\"path\":" << json_string(slot.package_path) << ",\"width\":" << slot.width
                   << ",\"height\":" << slot.height << "}";
        }
        report << "]}\n";
        auto text = report.str();
        write_text(stage / "export-report.json", text);
        filesystem->sync_directory(stage / "textures");
        filesystem->sync_directory(stage);
        backup = stage;
        backup += "-previous";
        if (filesystem->info(destination).exists) {
            filesystem->move_new(destination, backup);
            moved = true;
        }
        filesystem->move_new(stage, destination);
        published = true;
        try {
            filesystem->sync_directory(destination.parent_path());
        } catch (const io::Error &e) {
            if (options.publication_warnings)
                options.publication_warnings->push_back(
                    std::string("Published, but directory synchronization failed: ") + e.what());
        }
        if (moved) {
            try {
                filesystem->remove(backup);
            } catch (const io::Error &) {
            }
        }
        return {};
    } catch (const std::exception &e) {
        if (moved && !published) {
            try {
                filesystem->move_new(backup, destination);
            } catch (const io::Error &) {
                return Status::error("ROLLBACK_FAILED", std::string(e.what()) +
                                                            "; previous package retained at " +
                                                            backup.string());
            }
        }
        if (!stage.empty() && !published)
            try {
                filesystem->remove(stage);
            } catch (const io::Error &) {
            }
        return Status::error("PACKAGE_IO", e.what());
    }
}
} // namespace kasane
