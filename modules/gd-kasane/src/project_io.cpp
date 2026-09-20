// SPDX-License-Identifier: MIT
#include "project_io.hpp"
#include "project_results.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/os.hpp>
using namespace godot;

namespace kasane_gd {
void KasaneProjectIO::_bind_methods() {
    ClassDB::bind_method(D_METHOD("save_project", "document", "path"), &KasaneProjectIO::save_project);
    ClassDB::bind_method(D_METHOD("open_project", "document", "path"), &KasaneProjectIO::open_project);
    ClassDB::bind_method(D_METHOD("diagnose_resources", "document"), &KasaneProjectIO::diagnose_resources);
    ClassDB::bind_method(D_METHOD("relocate_asset", "document", "asset_id", "path"),
                         &KasaneProjectIO::relocate_asset);
    ClassDB::bind_method(D_METHOD("replace_asset", "document", "asset_id", "path"),
                         &KasaneProjectIO::replace_asset);
    ClassDB::bind_method(D_METHOD("export_package", "document", "path"), &KasaneProjectIO::export_package);
}

namespace {
kasane::Status ready(const Ref<KasaneDocumentBridge> &owner) {
    if (owner.is_null())
        return kasane::Status::error("MISSING_DOCUMENT", "Provide a Document.");
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return kasane::Status::error("WRONG_THREAD", "Document binding requires the main thread.");
    return {};
}
} // namespace

Dictionary KasaneProjectIO::save_project(const Ref<KasaneDocumentBridge> &owner, const String &path) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    auto saved = owner->session_.save(kasane::io::path_from_utf8(utf8(path)));
    auto out = project_result(saved);
    if (saved.status.ok()) {
        out["path"] = string(kasane::io::path_text(owner->session_.manifest()));
        owner->emit_signal("changed", out);
    }
    return out;
}

Dictionary KasaneProjectIO::open_project(const Ref<KasaneDocumentBridge> &owner, const String &path) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    auto opened = owner->session_.open(kasane::io::path_from_utf8(utf8(path)));
    auto out = project_result(opened);
    if (opened.status.ok()) {
        ++owner->generation_;
        owner->preview_values_.clear();
        out["path"] = string(kasane::io::path_text(owner->session_.manifest()));
        out["revision"] = owner->source().revision();
        owner->emit_signal("changed", out);
    }
    return out;
}

Dictionary KasaneProjectIO::diagnose_resources(const Ref<KasaneDocumentBridge> &owner) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    kasane::ProjectResult report;
    report.diagnostics = owner->session_.diagnose();
    return project_result(report);
}

Dictionary KasaneProjectIO::relocate_asset(const Ref<KasaneDocumentBridge> &owner, const String &id,
                                           const String &path) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    return owner->apply(owner->session_.relocate_asset(utf8(id), kasane::io::path_from_utf8(utf8(path))));
}

Dictionary KasaneProjectIO::replace_asset(const Ref<KasaneDocumentBridge> &owner, const String &id,
                                          const String &path) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    return owner->apply(owner->session_.replace_asset(utf8(id), kasane::io::path_from_utf8(utf8(path))));
}

Dictionary KasaneProjectIO::export_package(const Ref<KasaneDocumentBridge> &owner, const String &path) {
    if (auto s = ready(owner); !s.ok())
        return result(s);
    return project_result(owner->session_.export_package(kasane::io::path_from_utf8(utf8(path))));
}
} // namespace kasane_gd
