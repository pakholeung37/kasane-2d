// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include <godot_cpp/classes/os.hpp>
using namespace godot;

namespace kasane_gd {
#define MAIN_THREAD()                                                                                        \
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())            \
    return error("WRONG_THREAD", "Document requires the main thread.")

Dictionary KasaneDocumentBridge::create_rotation(const String &id, const String &name, Vector2 center,
                                                 double angle) {
    MAIN_THREAD();
    kasane::Deformer d;
    d.id = utf8(id);
    d.name = utf8(name);
    d.center = {float(center.x), float(center.y)};
    d.angle_degrees = float(angle);
    return apply(session_.document().create_deformer(std::move(d)));
}

Dictionary KasaneDocumentBridge::create_warp(const String &id, const String &name, Vector2 origin,
                                             Vector2 size, int64_t columns, int64_t rows) {
    MAIN_THREAD();
    if (columns < 1 || rows < 1 || columns > 16 || rows > 16)
        return error("INVALID_WARP", "Warp supports 1–16 cells per axis.");
    kasane::Deformer d;
    d.id = utf8(id);
    d.name = utf8(name);
    d.kind = kasane::DeformerKind::warp;
    d.origin = {float(origin.x), float(origin.y)};
    d.size = {float(size.x), float(size.y)};
    d.columns = uint32_t(columns);
    d.rows = uint32_t(rows);
    return apply(session_.document().create_deformer(std::move(d)));
}

Dictionary KasaneDocumentBridge::set_rotation(const String &id, Vector2 center, double angle) {
    MAIN_THREAD();
    return apply(
        session_.document().set_rotation(utf8(id), {float(center.x), float(center.y)}, float(angle)));
}

Dictionary KasaneDocumentBridge::set_warp_points(const String &id, const PackedVector2Array &points) {
    MAIN_THREAD();
    return apply(session_.document().set_warp_points(utf8(id), vectors(points)));
}

Dictionary KasaneDocumentBridge::set_deform_parent(const String &id, const String &parent) {
    MAIN_THREAD();
    return apply(session_.document().set_parent(utf8(id), utf8(parent)));
}

Dictionary KasaneDocumentBridge::set_organization_parent(const String &id, const String &parent) {
    MAIN_THREAD();
    return apply(session_.document().set_parent(utf8(id), utf8(parent), true));
}

Dictionary KasaneDocumentBridge::get_deformer_snapshot(const String &id) const {
    MAIN_THREAD();
    const auto *d = session_.document().get_deformer(utf8(id));
    if (!d)
        return error("MISSING_DEFORMER", "Deformer does not exist.");
    auto out = result({});
    out["id"] = id;
    out["name"] = string(d->name);
    out["kind"] = d->kind == kasane::DeformerKind::rotation ? "rotation" : "warp";
    out["deform_parent"] = string(session_.document().parent_of(utf8(id)));
    out["organization_parent"] = string(session_.document().parent_of(utf8(id), true));
    out["revision"] = session_.document().revision();
    if (d->kind == kasane::DeformerKind::rotation) {
        out["center"] = Vector2(d->center.x, d->center.y);
        out["angle_degrees"] = d->angle_degrees;
    } else {
        out["origin"] = Vector2(d->origin.x, d->origin.y);
        out["size"] = Vector2(d->size.x, d->size.y);
        out["columns"] = d->columns;
        out["rows"] = d->rows;
        out["control_points"] = vectors(d->control_points);
    }
    return out;
}

Ref<KasaneDeformerData> KasaneDocumentBridge::get_deformer(const String &id) const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id() ||
        !session_.document().get_deformer(utf8(id)))
        return {};
    Ref<KasaneDeformerData> handle;
    handle.instantiate();
    handle->attach(get_instance_id(), generation_, id);
    return handle;
}

Dictionary KasaneDocumentBridge::evaluate_mesh(const String &id) const {
    MAIN_THREAD();
    kasane::DrawableFrame frame;
    auto status = evaluate(frame);
    if (!status.ok())
        return result(status);
    for (const auto &drawable : frame.drawables)
        if (drawable.id == utf8(id)) {
            auto out = result({});
            out["positions"] = vectors(drawable.positions);
            out["coordinate_units"] = "runtime";
            out["revision"] = frame.source_revision;
            return out;
        }
    return error("MISSING_MESH", "Mesh does not exist.");
}

#undef MAIN_THREAD
} // namespace kasane_gd
