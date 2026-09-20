// SPDX-License-Identifier: MIT
#include "deformer_data.hpp"
#include "document_bridge.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/core/object.hpp>
#include <godot_cpp/classes/os.hpp>
using namespace godot;

namespace kasane_gd {
void KasaneDeformerData::_bind_methods() {
    ClassDB::bind_method(D_METHOD("is_valid"), &KasaneDeformerData::is_valid);
    ClassDB::bind_method(D_METHOD("get_id"), &KasaneDeformerData::get_id);
    ClassDB::bind_method(D_METHOD("snapshot"), &KasaneDeformerData::snapshot);
    ClassDB::bind_method(D_METHOD("get_angle_degrees"), &KasaneDeformerData::get_angle_degrees);
    ClassDB::bind_method(D_METHOD("set_angle_degrees", "value"), &KasaneDeformerData::set_angle_degrees);
    ClassDB::bind_method(D_METHOD("get_center"), &KasaneDeformerData::get_center);
    ClassDB::bind_method(D_METHOD("set_center", "value"), &KasaneDeformerData::set_center);
    ClassDB::bind_method(D_METHOD("get_control_points"), &KasaneDeformerData::get_control_points);
    ClassDB::bind_method(D_METHOD("set_control_points", "value"), &KasaneDeformerData::set_control_points);
    ClassDB::bind_method(D_METHOD("update_rotation", "center", "angle"),
                         &KasaneDeformerData::update_rotation);
    ClassDB::bind_method(D_METHOD("update_control_points", "points"),
                         &KasaneDeformerData::update_control_points);
    ClassDB::bind_method(D_METHOD("bind_to", "parent"), &KasaneDeformerData::bind_to);
    ADD_PROPERTY(PropertyInfo(Variant::STRING, "id"), "", "get_id");
    ADD_PROPERTY(PropertyInfo(Variant::FLOAT, "angle_degrees"), "set_angle_degrees", "get_angle_degrees");
    ADD_PROPERTY(PropertyInfo(Variant::VECTOR2, "center"), "set_center", "get_center");
    ADD_PROPERTY(PropertyInfo(Variant::PACKED_VECTOR2_ARRAY, "control_points"), "set_control_points",
                 "get_control_points");
}

void KasaneDeformerData::attach(uint64_t owner_id, uint64_t generation, const String &id) {
    owner_ = owner_id;
    generation_ = generation;
    id_ = id;
}

KasaneDocumentBridge *KasaneDeformerData::owner() const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return nullptr;
    auto *bridge = Object::cast_to<KasaneDocumentBridge>(ObjectDB::get_instance(owner_));
    return bridge && bridge->generation() == generation_ ? bridge : nullptr;
}

bool KasaneDeformerData::is_valid() const {
    auto *b = owner();
    return b && bool(b->get_deformer_snapshot(id_)["ok"]);
}

Dictionary KasaneDeformerData::snapshot() const {
    auto *b = owner();
    return b ? b->get_deformer_snapshot(id_) : error("STALE_HANDLE", "Document was replaced or closed.");
}

double KasaneDeformerData::get_angle_degrees() const {
    return snapshot().get("angle_degrees", 0.0);
}

Vector2 KasaneDeformerData::get_center() const {
    return snapshot().get("center", Vector2());
}

PackedVector2Array KasaneDeformerData::get_control_points() const {
    return snapshot().get("control_points", PackedVector2Array());
}

Dictionary KasaneDeformerData::update_rotation(Vector2 center, double angle) {
    auto *b = owner();
    return b ? b->set_rotation(id_, center, angle)
             : error("STALE_HANDLE", "Document was replaced or closed.");
}

Dictionary KasaneDeformerData::update_control_points(const PackedVector2Array &points) {
    auto *b = owner();
    return b ? b->set_warp_points(id_, points) : error("STALE_HANDLE", "Document was replaced or closed.");
}

Dictionary KasaneDeformerData::bind_to(const String &parent) {
    auto *b = owner();
    return b ? b->set_deform_parent(id_, parent) : error("STALE_HANDLE", "Document was replaced or closed.");
}

void KasaneDeformerData::set_angle_degrees(double angle) {
    auto r = update_rotation(get_center(), angle);
    ERR_FAIL_COND_MSG(!bool(r["ok"]), String(r["message"]));
}

void KasaneDeformerData::set_center(Vector2 center) {
    auto r = update_rotation(center, get_angle_degrees());
    ERR_FAIL_COND_MSG(!bool(r["ok"]), String(r["message"]));
}

void KasaneDeformerData::set_control_points(const PackedVector2Array &points) {
    auto r = update_control_points(points);
    ERR_FAIL_COND_MSG(!bool(r["ok"]), String(r["message"]));
}
} // namespace kasane_gd
