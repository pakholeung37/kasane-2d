// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/document.hpp>
#include <kasane/filesystem.hpp>
#include <optional>

namespace kasane {
struct AssetData {
    io::Bytes bytes, rgba;
    uint32_t width = 0, height = 0;
    std::string sha256;
};

struct ResourceDiagnostic {
    std::string asset_id, code, message;
};

struct ProjectResult {
    Status status;
    std::vector<ResourceDiagnostic> diagnostics;
    std::vector<std::string> warnings;
    bool published = false;
    bool durable = true;

    bool resources_complete() const { return diagnostics.empty(); }
};

std::string content_sha256(std::span<const uint8_t>);
Status decode_png(std::span<const uint8_t>, AssetData &);
Status read_project_asset(io::FileSystem &, const io::fs::path &root, const ImageAsset &, AssetData &);
io::fs::path project_manifest(const io::fs::path &);

struct DocumentSnapshot {
    Document document;
    io::fs::path manifest;
    std::string manifest_sha256;
};

// Stateless persistence service. It does not own editing state or Godot objects.
class DocumentStore {
    std::shared_ptr<io::FileSystem> filesystem_;

  public:
    explicit DocumentStore(std::shared_ptr<io::FileSystem> filesystem) : filesystem_(std::move(filesystem)) {}

    ProjectResult open(const io::fs::path &, DocumentSnapshot &);
    ProjectResult save(const Document &, const io::fs::path &source_root, const io::fs::path &destination,
                       const std::optional<std::string> &expected_manifest, DocumentSnapshot &);
    std::vector<ResourceDiagnostic> diagnose(const Document &, const io::fs::path &);
};

// Concrete editing context. Source snapshots/UndoRedo remain Document responsibilities.
class DocumentSession {
    std::shared_ptr<io::FileSystem> filesystem_;
    DocumentStore store_;
    Document document_;
    io::fs::path manifest_;
    std::string manifest_sha256_;

  public:
    explicit DocumentSession(std::shared_ptr<io::FileSystem> filesystem = io::native_filesystem())
        : filesystem_(std::move(filesystem)), store_(filesystem_) {}

    Document &document() { return document_; }

    const Document &document() const { return document_; }

    io::fs::path root() const { return manifest_.parent_path(); }

    const io::fs::path &manifest() const { return manifest_; }

    ProjectResult open(const io::fs::path &);
    ProjectResult save(const io::fs::path &);
    std::vector<ResourceDiagnostic> diagnose() const;
    Status read_asset(const std::string &, AssetData &) const;
    EditResult relocate_asset(const std::string &, const io::fs::path &);
    EditResult replace_asset(const std::string &, const io::fs::path &);
    ProjectResult export_package(const io::fs::path &);
};
} // namespace kasane
