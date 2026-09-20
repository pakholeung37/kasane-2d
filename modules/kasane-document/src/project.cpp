// SPDX-License-Identifier: MIT
#include <kasane/project.hpp>
#include <kasane/project_codec.hpp>
#include <kasane/package.hpp>

namespace kasane {
namespace {
ProjectResult failed(std::string code, std::string message) {
    ProjectResult r;
    r.status = Status::error(std::move(code), std::move(message));
    return r;
}

ProjectResult io_failure(const io::Error &e) {
    return failed(e.operation == "lock" ? "PROJECT_BUSY" : "PROJECT_IO", e.what());
}

Status editable(const Document &d) {
    if (d.transaction_active())
        return Status::error("TRANSACTION_ACTIVE", "Commit or cancel first");
    return {};
}

io::Bytes bytes_of(const std::string &text) {
    return {text.begin(), text.end()};
}

std::string text_of(const io::Bytes &bytes) {
    return {bytes.begin(), bytes.end()};
}

void check_target(io::FileSystem &filesystem, const io::fs::path &path,
                  const std::optional<std::string> &expected) {
    const auto info = filesystem.info(path);
    if (info.symlink || info.directory)
        throw io::Error("manifest", path, std::make_error_code(std::errc::invalid_argument));
    if (expected) {
        if (!info.exists || content_sha256(filesystem.read(path)) != *expected)
            throw std::runtime_error(
                "PROJECT_CONFLICT: on-disk manifest changed; reopen or save to a new path");
    } else if (info.exists)
        throw std::runtime_error(
            "DESTINATION_EXISTS: refusing to overwrite a project that was not opened by this session");
}
} // namespace

io::fs::path project_manifest(const io::fs::path &path) {
    auto p = io::local_path(path);
    return p.extension() == ".json" ? p : p / "project.kasane.json";
}

std::vector<ResourceDiagnostic> DocumentStore::diagnose(const Document &document, const io::fs::path &root) {
    std::vector<ResourceDiagnostic> result;
    for (const auto &id : document.asset_order()) {
        AssetData bytes;
        if (auto s = read_project_asset(*filesystem_, root, *document.get_asset(id), bytes); !s.ok())
            result.push_back({id, s.code, s.message});
    }
    return result;
}

ProjectResult DocumentStore::open(const io::fs::path &path, DocumentSnapshot &output) {
    try {
        auto manifest = project_manifest(path);
        if (filesystem_->info(manifest).symlink)
            return failed("INVALID_PATH", "Project manifest cannot be a symlink");
        manifest = filesystem_->canonical(manifest);
        auto bytes = filesystem_->read(manifest);
        DocumentSnapshot next;
        next.manifest = manifest;
        next.manifest_sha256 = content_sha256(bytes);
        if (auto s = decode_project(text_of(bytes), next.document); !s.ok()) {
            ProjectResult r;
            r.status = s;
            return r;
        }
        ProjectResult result;
        result.diagnostics = diagnose(next.document, manifest.parent_path());
        output = std::move(next);
        return result;
    } catch (const io::Error &e) {
        return io_failure(e);
    } catch (const std::exception &e) {
        return failed("OPEN_FAILED", e.what());
    }
}

ProjectResult DocumentStore::save(const Document &document, const io::fs::path &source_root,
                                  const io::fs::path &path, const std::optional<std::string> &expected,
                                  DocumentSnapshot &output) {
    try {
        if (auto s = editable(document); !s.ok()) {
            ProjectResult r;
            r.status = s;
            return r;
        }
        std::string preflight;
        if (auto s = encode_project(document, preflight); !s.ok()) {
            ProjectResult r;
            r.status = s;
            return r;
        }
        auto requested = project_manifest(path);
        if (filesystem_->info(requested).symlink)
            return failed("INVALID_PATH", "Project manifest cannot be a symlink");
        const auto manifest = filesystem_->canonical(requested), root = manifest.parent_path();
        filesystem_->create_directories(root);
        auto lock = filesystem_->lock(root);
        check_target(*filesystem_, manifest, expected);
        if (filesystem_->info(root / "assets").symlink)
            return failed("INVALID_PATH", "Assets directory cannot be a symlink");
        filesystem_->create_directories(root / "assets");
        io::TemporaryDirectory stage(*filesystem_, root);
        Document candidate = document;
        for (const auto &id : candidate.asset_order()) {
            auto asset = *candidate.get_asset(id);
            AssetData data;
            if (auto s = read_project_asset(*filesystem_, source_root, asset, data); !s.ok()) {
                ProjectResult r;
                r.status = s;
                return r;
            }
            auto name = data.sha256 + ".png";
            auto target = root / "assets" / name;
            auto info = filesystem_->info(target);
            if (info.exists && (info.symlink || info.directory ||
                                content_sha256(filesystem_->read(target)) != data.sha256)) {
                // Never overwrite even a corrupt old asset. A fresh explicit replacement gets a new name.
                name = data.sha256 + "-" + io::unique_name() + ".png";
                target = root / "assets" / name;
                info = {};
            }
            if (!info.exists)
                io::write_new(*filesystem_, target, data.bytes);
            asset.source = "assets/" + name;
            asset.sha256 = data.sha256;
            if (auto edit = candidate.replace_asset(std::move(asset)); !edit.status.ok()) {
                ProjectResult r;
                r.status = edit.status;
                return r;
            }
        }
        filesystem_->sync_directory(root / "assets");
        filesystem_->sync_directory(root); // Ensure assets/ exists durably before referencing it.
        std::string text;
        if (auto s = encode_project(candidate, text); !s.ok()) {
            ProjectResult r;
            r.status = s;
            return r;
        }
        auto temporary = stage.path / "manifest.json";
        io::write_new(*filesystem_, temporary, bytes_of(text));
        check_target(*filesystem_, manifest, expected);
        // Allocate all committed state before the commit point; no reported precommit failure after rename.
        candidate.mark_saved();
        DocumentSnapshot next{std::move(candidate), manifest, content_sha256(bytes_of(text))};
        ProjectResult result;
        result.published = true;
        filesystem_->replace_file(temporary, manifest);
        output = std::move(next);
        try {
            filesystem_->sync_directory(root);
        } catch (const std::exception &e) {
            result.durable = false;
            result.warnings.push_back(std::string("Published, but directory synchronization failed: ") +
                                      e.what());
        }
        return result;
    } catch (const io::Error &e) {
        return io_failure(e);
    } catch (const std::exception &e) {
        std::string message = e.what();
        return failed(message.starts_with("PROJECT_CONFLICT:")     ? "PROJECT_CONFLICT"
                      : message.starts_with("DESTINATION_EXISTS:") ? "DESTINATION_EXISTS"
                                                                   : "SAVE_FAILED",
                      message);
    }
}

ProjectResult DocumentSession::open(const io::fs::path &path) {
    if (auto s = editable(document_); !s.ok()) {
        ProjectResult r;
        r.status = s;
        return r;
    }
    DocumentSnapshot next;
    auto result = store_.open(path, next);
    if (result.status.ok()) {
        document_.restore_from(next.document);
        document_.mark_saved();
        manifest_ = std::move(next.manifest);
        manifest_sha256_ = std::move(next.manifest_sha256);
    }
    return result;
}

ProjectResult DocumentSession::save(const io::fs::path &path) {
    try {
        auto requested = project_manifest(path);
        if (filesystem_->info(requested).symlink)
            return failed("INVALID_PATH", "Project manifest cannot be a symlink");
        auto target = filesystem_->canonical(requested);
        std::optional<std::string> expected;
        if (!manifest_.empty() && target == manifest_)
            expected = manifest_sha256_;
        DocumentSnapshot next;
        auto result = store_.save(document_, root(), target, expected, next);
        if (result.status.ok()) {
            document_ = std::move(next.document);
            manifest_ = std::move(next.manifest);
            manifest_sha256_ = std::move(next.manifest_sha256);
        }
        return result;
    } catch (const io::Error &e) {
        return io_failure(e);
    }
}

std::vector<ResourceDiagnostic> DocumentSession::diagnose() const {
    return DocumentStore(filesystem_).diagnose(document_, root());
}

Status DocumentSession::read_asset(const std::string &id, AssetData &output) const {
    auto asset = document_.get_asset(id);
    if (!asset)
        return Status::error("MISSING_ASSET", id);
    return read_project_asset(*filesystem_, root(), *asset, output);
}

EditResult DocumentSession::relocate_asset(const std::string &id, const io::fs::path &path) {
    if (auto s = editable(document_); !s.ok())
        return {s, {}, {}};
    auto existing = document_.get_asset(id);
    if (!existing)
        return {Status::error("MISSING_ASSET", id), {}, {}};
    try {
        auto asset = *existing;
        asset.source = io::path_text(io::local_path(path));
        AssetData data;
        if (auto s = read_project_asset(*filesystem_, {}, asset, data); !s.ok())
            return {s, {}, {}};
        asset.sha256 = data.sha256;
        return document_.replace_asset(std::move(asset));
    } catch (const io::Error &e) {
        return {Status::error("RESOURCE_IO", e.what()), {}, {}};
    }
}

EditResult DocumentSession::replace_asset(const std::string &id, const io::fs::path &path) {
    if (auto s = editable(document_); !s.ok())
        return {s, {}, {}};
    auto existing = document_.get_asset(id);
    if (!existing)
        return {Status::error("MISSING_ASSET", id), {}, {}};
    try {
        auto source = io::local_path(path);
        AssetData data;
        if (auto s = decode_png(filesystem_->read(source), data); !s.ok())
            return {s, {}, {}};
        auto asset = *existing;
        asset.source = io::path_text(source);
        asset.sha256 = data.sha256;
        asset.width = data.width;
        asset.height = data.height;
        return document_.replace_asset(std::move(asset));
    } catch (const io::Error &e) {
        return {Status::error("RESOURCE_IO", e.what()), {}, {}};
    }
}

ProjectResult DocumentSession::export_package(const io::fs::path &path) {
    if (auto s = editable(document_); !s.ok()) {
        ProjectResult r;
        r.status = s;
        return r;
    }
    try {
        auto destination = filesystem_->canonical(io::local_path(path));
        auto contains = [&](const io::fs::path &source) {
            auto relative = filesystem_->canonical(source).lexically_relative(destination);
            return !relative.empty() && *relative.begin() != "..";
        };
        if (!root().empty() && contains(root()))
            return failed("INVALID_DESTINATION", "Export would replace the source project");
        for (const auto &id : document_.asset_order())
            if (contains(io::resolve_asset(*filesystem_, root(), document_.get_asset(id)->source)))
                return failed("INVALID_DESTINATION", "Export would replace a source asset");
        filesystem_->create_directories(destination.parent_path());
        io::TemporaryDirectory stage(*filesystem_, destination.parent_path());
        auto candidate = document_;
        for (const auto &id : candidate.asset_order()) {
            AssetData data;
            if (auto s = read_asset(id, data); !s.ok()) {
                ProjectResult r;
                r.status = s;
                return r;
            }
            auto target = stage.path / (id + ".png");
            io::write_new(*filesystem_, target, data.bytes);
            auto asset = *candidate.get_asset(id);
            asset.source = io::path_text(target);
            candidate.replace_asset(std::move(asset));
        }
        ProjectResult result;
        PackageOptions options;
        options.destination = destination;
        options.filesystem = filesystem_;
        options.publication_warnings = &result.warnings;
        options.validate = [](const Moc3Artifact &a) {
            return a.bytes.empty() ? Status::error("EMPTY_MOC3", "Encoder returned no bytes") : Status{};
        };
        result.status = publish_package(candidate, options);
        result.published = result.status.ok();
        result.durable = result.warnings.empty();
        return result;
    } catch (const io::Error &e) {
        return io_failure(e);
    } catch (const std::exception &e) {
        return failed("EXPORT_FAILED", e.what());
    }
}
} // namespace kasane
