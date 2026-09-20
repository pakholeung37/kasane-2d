// SPDX-License-Identifier: MIT
#include <kasane/project.hpp>
#include <kasane/project_codec.hpp>
#include <kasane/package.hpp>
#include <kasane/evaluation.hpp>
#include <nlohmann/json.hpp>
#include <png.h>
#include <iostream>
#include <functional>
#include <source_location>
using namespace kasane;
using namespace kasane::io;
using Json = nlohmann::json;
static Json checks = Json::array();

static void check(bool value, const std::string &label,
                  std::source_location location = std::source_location::current()) {
    checks.push_back({{"name", label},
                      {"expected", true},
                      {"actual", value},
                      {"status", value ? "passed" : "failed"},
                      {"line", location.line()}});
    if (!value)
        throw std::runtime_error(label + " at line " + std::to_string(location.line()));
}

static void ok(const Status &s, const std::string &label) {
    check(s.ok(), label + ": " + s.code + " " + s.message);
}

static void ok(const ProjectResult &r, const std::string &label) {
    ok(r.status, label);
}

static Bytes bytes(const std::string &s) {
    return {s.begin(), s.end()};
}

class FaultFileSystem final : public FileSystem {
  public:
    NativeFileSystem native;
    std::string operation, fragment;
    int occurrence = 1, seen = 0;
    bool rollback_failure = false;

    void arm(std::string op, int nth = 1, std::string match = "") {
        operation = std::move(op);
        fragment = std::move(match);
        occurrence = nth;
        seen = 0;
    }

    void hit(const std::string &op, const fs::path &path) {
        if (op == operation && path_text(path).find(fragment) != std::string::npos && ++seen == occurrence)
            throw Error(
                op, path,
                std::make_error_code(op == "write" ? std::errc::no_space_on_device : std::errc::io_error));
    }

    class FaultWriter final : public Writer {
        FaultFileSystem &owner_;
        std::unique_ptr<Writer> next_;
        fs::path path_;

      public:
        FaultWriter(FaultFileSystem &owner, std::unique_ptr<Writer> next, fs::path p)
            : owner_(owner), next_(std::move(next)), path_(std::move(p)) {}

        void write(std::span<const uint8_t> b) override {
            auto middle = b.size() / 2;
            next_->write(b.first(middle));
            owner_.hit("write", path_);
            next_->write(b.subspan(middle));
        }

        void sync() override {
            owner_.hit("sync_file", path_);
            next_->sync();
        }

        void close() override {
            owner_.hit("close", path_);
            next_->close();
        }
    };

    Bytes read(const fs::path &p) override {
        hit("read", p);
        return native.read(p);
    }

    FileInfo info(const fs::path &p) override {
        hit("stat", p);
        return native.info(p);
    }

    fs::path canonical(const fs::path &p) override {
        hit("canonical", p);
        return native.canonical(p);
    }

    void create_directories(const fs::path &p) override {
        hit("mkdir", p);
        native.create_directories(p);
    }

    void create_directory_new(const fs::path &p) override {
        hit("mkdir_new", p);
        native.create_directory_new(p);
    }

    std::unique_ptr<Writer> create_file_new(const fs::path &p) override {
        hit("create_new", p);
        return std::make_unique<FaultWriter>(*this, native.create_file_new(p), p);
    }

    void replace_file(const fs::path &a, const fs::path &b) override {
        hit("replace_file", b);
        native.replace_file(a, b);
    }

    void move_new(const fs::path &a, const fs::path &b) override {
        if (rollback_failure && path_text(a).ends_with("-previous"))
            throw Error("rollback", b, std::make_error_code(std::errc::io_error));
        hit("move_new", b);
        native.move_new(a, b);
    }

    void remove(const fs::path &p) override {
        hit("remove", p);
        native.remove(p);
    }

    void sync_directory(const fs::path &p) override {
        hit("sync_directory", p);
        native.sync_directory(p);
    }

    std::unique_ptr<Lock> lock(const fs::path &p) override {
        hit("lock", p);
        return native.lock(p);
    }
};

static std::string first_mesh(const DocumentSession &s) {
    return s.document().mesh_order().front();
}

static void compare_model(const Document &a, const Document &b) {
    check(a.same_content(b), "all source fields and object order equal");
    Moc3Artifact left, right;
    ok(encode_moc3(a, left), "encode original");
    ok(encode_moc3(b, right), "encode reopened");
    check(left.bytes == right.bytes, "MOC3 identical after native roundtrip");
    size_t samples = 0;
    for (float x : {-1.f, -0.5f, 0.f, 0.5f, 1.f})
        for (float y : {-1.f, -0.5f, 0.f, 0.5f, 1.f})
            for (float z : {-1.f, 0.f, 1.f}) {
                PreviewValues values;
                const auto &parameters = a.parameter_order();
                if (!parameters.empty())
                    values[parameters[0]] = x;
                if (parameters.size() > 1)
                    values[parameters[1]] = y;
                if (parameters.size() > 2)
                    values[parameters[2]] = z;
                DrawableFrame before, after;
                ok(evaluate_frame(a, values, before), "evaluate original");
                ok(evaluate_frame(b, values, after), "evaluate reopened");
                check(before.drawables.size() == after.drawables.size(), "drawable count");
                for (size_t i = 0; i < before.drawables.size(); ++i) {
                    const auto &l = before.drawables[i], &r = after.drawables[i];
                    check(l.positions == r.positions && l.uvs == r.uvs && l.indices == r.indices &&
                              l.opacity == r.opacity && l.draw_order == r.draw_order &&
                              l.render_order == r.render_order && l.multiply_color == r.multiply_color &&
                              l.screen_color == r.screen_color && l.masks == r.masks &&
                              l.enabled == r.enabled && l.visible == r.visible,
                          "native sampled drawable equality");
                }
                ++samples;
            }
    check(samples == 75, "75 independent parameter samples");
}

static void create_png(FileSystem &filesystem, const fs::path &p) {
    png_image image{};
    image.version = PNG_IMAGE_VERSION;
    image.width = 8;
    image.height = 8;
    image.format = PNG_FORMAT_RGBA;
    Bytes pixels(8 * 8 * 4, 123);
    size_t size = 0;
    check(png_image_write_to_memory(&image, nullptr, &size, 0, pixels.data(), 0, nullptr),
          "PNG fixture size");
    Bytes encoded(size);
    check(png_image_write_to_memory(&image, encoded.data(), &size, 0, pixels.data(), 0, nullptr),
          "PNG fixture encoding");
    encoded.resize(size);
    write_new(filesystem, p, encoded);
}

static void roundtrip(const fs::path &sample, const fs::path &root) {
    DocumentSession source;
    ok(source.open(sample), "open compatible M2 sample without Godot");
    check(source.diagnose().empty(), "sample assets verified");
    auto before = source.document();
    std::string encoded;
    ok(encode_project(before, encoded), "native JSON encode");
    Document decoded;
    ok(decode_project(encoded, decoded), "native JSON decode");
    compare_model(before, decoded);
    ok(source.save(root / "original"), "native save-as");
    NativeFileSystem files;
    files.move_new(root / "original", root / "moved");
    DocumentSession moved;
    ok(moved.open(root / "moved"), "moved directory opens");
    compare_model(before, moved.document());
    check(!moved.document().modified(), "open resets saved baseline");
    auto old = moved.document().get_mesh(first_mesh(moved))->name;
    ok(moved.document().rename_mesh(first_mesh(moved), "new name").status, "edit reopened document");
    check(moved.document().modified(), "editing marks modified");
    ok(moved.document().rename_mesh(first_mesh(moved), old).status, "restore source content");
    check(!moved.document().modified(), "content restoration clean");
    for (const auto &bad : {std::string("res://project.json"), std::string("user://project.json"),
                            std::string("relative/project.json")})
        check(!moved.save(path_from_utf8(bad)).status.ok(), "reject non-local path " + bad);
    auto manifest = Json::parse(encoded);
    std::vector<Json> malformed;
    auto j = manifest;
    j["format_version"] = 99;
    malformed.push_back(j);
    j = manifest;
    j["document"]["assets"][0]["source"] = "../escape.png";
    malformed.push_back(j);
    j = manifest;
    j["document"]["parameters"][0]["decimal_places"] = 1.5;
    malformed.push_back(j);
    j = manifest;
    j["document"]["meshes"][0]["vertex_ids"][0] = true;
    malformed.push_back(j);
    j = manifest;
    j["document"]["bindings"][0]["keyforms"].erase(0);
    malformed.push_back(j);
    j = manifest;
    j["document"]["parts"][1]["parent_id"] = j["document"]["parts"][0]["id"];
    malformed.push_back(j);
    for (auto &bad : malformed) {
        check(!decode_project(bad.dump(), decoded).ok(), "reject malformed native JSON");
        check(before.same_content(decoded), "decode failure preserves output");
    }
    check(!decode_project("{\"format\":\"a\",\"format\":\"b\"}", decoded).ok(),
          "duplicate JSON key rejected");
    DocumentSession other;
    ok(other.open(root / "moved"), "second editor opens");
    ok(moved.document().rename_mesh(first_mesh(moved), "editor A").status, "edit A");
    ok(moved.save(root / "moved"), "save A");
    ok(other.document().rename_mesh(first_mesh(other), "editor B").status, "edit B");
    check(other.save(root / "moved").status.code == "PROJECT_CONFLICT", "stale editor cannot overwrite A");
    check(other.document().modified(), "conflict preserves B edits");
    ok(other.save(root / "recovered-B"), "conflict can save as");
    auto lock = files.lock(root / "moved");
    check(!moved.save(root / "moved").status.ok(), "second exclusive lock rejected");
    lock.reset();
    auto asset = *moved.document().get_asset(moved.document().asset_order()[0]);
    auto asset_file = root / "moved" / asset.source;
    auto backup = files.read(asset_file);
    files.remove(asset_file);
    DocumentSession missing;
    auto result = missing.open(root / "moved");
    ok(result, "source opens with missing PNG");
    check(!result.resources_complete() && result.diagnostics[0].asset_id == asset.id,
          "native resource diagnostic contains asset ID");
    check(!missing.save(root / "moved").status.ok(), "missing resource blocks save");
    write_new(files, asset_file, backup);
    for (const auto &kind : {"hash", "dimensions", "corrupt"}) {
        auto bad_asset = asset;
        std::string expected;
        if (std::string(kind) == "hash") {
            bad_asset.sha256 = std::string(64, '0');
            expected = "RESOURCE_HASH";
        } else if (std::string(kind) == "dimensions") {
            ++bad_asset.width;
            expected = "RESOURCE_DIMENSIONS";
        } else {
            files.remove(asset_file);
            write_new(files, asset_file, bytes("broken PNG"));
            expected = "INVALID_PNG";
        }
        AssetData data;
        check(read_project_asset(files, root / "moved", bad_asset, data).code == expected,
              std::string("resource diagnostic ") + kind);
        if (std::string(kind) == "corrupt") {
            files.remove(asset_file);
            write_new(files, asset_file, backup);
        }
    }
    create_png(files, root / "different.png");
    auto unchanged = moved.document();
    check(!moved.relocate_asset(asset.id, root / "different.png").status.ok(),
          "relocation rejects different content");
    check(moved.document().same_content(unchanged), "failed relocation preserves source");
    ok(moved.replace_asset(asset.id, root / "different.png").status, "explicit replacement accepts new PNG");
    ok(moved.save(root / "replacement-project"), "replacement saves into portable project");
    DocumentSession replaced;
    ok(replaced.open(root / "replacement-project"), "replacement reopens");
    check(replaced.diagnose().empty(), "replacement metadata agrees with bytes");
#ifndef _WIN32
    fs::create_symlink(sample / "project.kasane.json", root / "linked.json");
    check(!moved.open(root / "linked.json").status.ok(), "symlink manifest open rejected");
    check(!moved.save(root / "linked.json").status.ok(), "symlink manifest save rejected");
#endif
}

static void fault_tests(const fs::path &sample, const fs::path &root) {
    auto fs = std::make_shared<FaultFileSystem>();
    DocumentSession session(fs);
    ok(session.open(sample), "fault session open");
    ok(session.save(root / "faults"), "fault fixture save");
    const auto manifest = session.manifest();
    auto old = fs->read(manifest);
    auto original = session.document();
    create_png(*fs, root / "replacement.png");
    ok(session.replace_asset(session.document().asset_order()[0], root / "replacement.png").status,
       "explicit asset replacement before failure");
    auto unsaved = session.document();
    for (const auto &op : {"read", "mkdir_new", "create_new", "write", "sync_file", "close", "sync_directory",
                           "replace_file"}) {
        fs->arm(op);
        auto result = session.save(root / "faults");
        fs->arm("");
        check(!result.status.ok(), std::string("injected failure ") + op);
        check(fs->read(manifest) == old, std::string("old manifest preserved after ") + op);
        check(session.document().same_content(unsaved) && session.document().modified(),
              "failed save preserves source and dirty state");
        DocumentSession reopened;
        ok(reopened.open(root / "faults"), "old project reopens after injected failure");
        check(reopened.diagnose().empty(), "old textures preserved");
    }
    // A late directory-sync failure occurs after commit: report success with a durability warning.
    fs->arm("sync_directory", 3);
    auto committed = session.save(root / "faults");
    fs->arm("");
    ok(committed, "committed save remains success after final sync failure");
    check(committed.published && !committed.durable && !committed.warnings.empty(),
          "durability uncertainty explicit");
    check(!session.document().modified(), "committed manifest updates saved baseline");
    DocumentSession verify;
    ok(verify.open(root / "faults"), "committed manifest readable");
    check(verify.document().same_content(session.document()), "committed source agrees with disk");
    // Fail at a manifest write, after all new assets have been published.
    ok(session.document().rename_mesh(first_mesh(session), "another edit").status,
       "edit before manifest write fault");
    auto committed_bytes = fs->read(manifest);
    for (const auto &op : {"write", "sync_file", "close"}) {
        fs->arm(op, 1, "manifest.json");
        check(!session.save(root / "faults").status.ok(), std::string("manifest ") + op + " failure");
        fs->arm("");
        check(fs->read(manifest) == committed_bytes, "failed manifest write retains prior commit");
    }
    // Package publication uses the same injectable filesystem and exercises rollback.
    ok(session.export_package(root / "runtime"), "native runtime package export");
    auto moc = fs->read(root / "runtime/model.moc3");
    fs->arm("move_new", 2);
    auto rolled = session.export_package(root / "runtime");
    fs->arm("");
    check(!rolled.status.ok() && fs->read(root / "runtime/model.moc3") == moc,
          "package rename failure restores old directory");
    fs->arm("move_new", 2);
    fs->rollback_failure = true;
    auto broken = session.export_package(root / "runtime");
    fs->arm("");
    fs->rollback_failure = false;
    check(broken.status.code == "ROLLBACK_FAILED", "rollback failure reports retained backup");
    check(broken.status.message.find("previous package retained at") != std::string::npos,
          "recovery location is actionable");
    bool found = false;
    for (const auto &entry : fs::directory_iterator(root))
        if (entry.path().filename().string().ends_with("-previous"))
            found |= fs->read(entry.path() / "model.moc3") == moc;
    check(found, "rollback failure still retains original package bytes");
}

int main(int argc, char **argv) {
    auto filesystem = native_filesystem();
    fs::path root;
    try {
        root = argc > 1 ? io::local_path(path_from_utf8(argv[1]))
                        : fs::temp_directory_path() / ("kasane-native-test-" + unique_name());
        filesystem->create_directories(root);
        auto sample = path_from_utf8(KASANE_SAMPLE_DIR);
        roundtrip(sample, root);
        fault_tests(sample, root);
        auto report =
            Json({{"status", "passed"}, {"checks", checks}, {"godot_required", false}}).dump(2) + "\n";
        write_new(*filesystem, root / "report.json", bytes(report));
        std::cout << checks.size() << " native project checks passed: " << path_text(root / "report.json")
                  << "\n";
        return 0;
    } catch (const std::exception &e) {
        std::cerr << e.what() << "\n";
        if (!root.empty())
            try {
                write_new(
                    *filesystem, root / "failed.json",
                    bytes(Json({{"status", "failed"}, {"error", e.what()}, {"checks", checks}}).dump(2)));
            } catch (...) {
            }
        return 1;
    }
}
