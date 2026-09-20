// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include "model_conversion.hpp"
#include <godot_cpp/classes/os.hpp>
using namespace godot;

namespace kasane_gd {
#define MAIN_THREAD()                                                                                        \
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())            \
    return error("WRONG_THREAD", "Document requires the main thread.")

kasane::Status KasaneDocumentBridge::evaluate(kasane::DrawableFrame &out) const {
    return kasane::evaluate_frame(session_.document(), preview_values_, out);
}

Dictionary KasaneDocumentBridge::get_frame() const {
    MAIN_THREAD();
    kasane::DrawableFrame frame;
    auto out = result(evaluate(frame));
    if (!bool(out["ok"]))
        return out;
    out["revision"] = frame.source_revision;
    out["coordinate_units"] = "runtime";
    Array drawables, parameters;
    for (const auto &d : frame.drawables) {
        Dictionary item;
        item["id"] = string(d.id);
        item["runtime_id"] = string(d.runtime_id);
        item["positions"] = vectors(d.positions);
        item["uvs"] = vectors(d.uvs);
        item["indices"] = ids(d.indices);
        item["texture_asset_id"] = string(d.texture_asset_id);
        item["texture_slot"] = d.texture_slot;
        item["draw_order"] = d.draw_order;
        item["render_order"] = d.render_order;
        item["opacity"] = d.opacity;
        item["enabled"] = d.enabled;
        item["visible"] = d.visible;
        item["double_sided"] = d.double_sided;
        item["inverted_mask"] = d.inverted_mask;
        item["blend_mode"] = int(d.blend_mode);
        PackedStringArray masks;
        for (const auto &id : d.masks)
            masks.push_back(string(id));
        item["masks"] = masks;
        item["multiply_color"] =
            Color(d.multiply_color[0], d.multiply_color[1], d.multiply_color[2], d.multiply_color[3]);
        item["screen_color"] =
            Color(d.screen_color[0], d.screen_color[1], d.screen_color[2], d.screen_color[3]);
        drawables.push_back(item);
    }
    for (const auto &p : frame.parameters) {
        Dictionary value;
        value["id"] = string(p.id);
        value["requested"] = p.requested;
        value["value"] = p.value;
        value["clamped"] = p.clamped;
        parameters.push_back(value);
    }
    out["drawables"] = drawables;
    out["parameters"] = parameters;
    return out;
}

Dictionary KasaneDocumentBridge::set_preview_values(const Dictionary &values) {
    MAIN_THREAD();
    kasane::PreviewValues next;
    Array keys = values.keys();
    for (int64_t i = 0; i < keys.size(); ++i) {
        const auto value = values[keys[i]];
        if (keys[i].get_type() != Variant::STRING ||
            (value.get_type() != Variant::INT && value.get_type() != Variant::FLOAT))
            return error("INVALID_FIELD", "Preview requires parameter IDs and numeric values.");
        next[utf8(keys[i])] = float(value);
    }
    kasane::DrawableFrame frame;
    auto status = kasane::evaluate_frame(session_.document(), next, frame);
    if (!status.ok())
        return result(status);
    preview_values_ = std::move(next);
    emit_signal("preview_changed");
    return get_frame();
}

Dictionary KasaneDocumentBridge::create_parameter(const Dictionary &d) {
    MAIN_THREAD();
    kasane::Parameter p;
    if (auto s = parameter_from_dictionary(d, p); !s.ok())
        return result(s);
    return apply(session_.document().create_parameter(std::move(p)));
}

Dictionary KasaneDocumentBridge::write_binding(const Dictionary &d, bool replace) {
    MAIN_THREAD();
    kasane::MeshBinding b;
    if (auto s = binding_from_dictionary(d, b); !s.ok())
        return result(s);
    return apply(replace ? session_.document().replace_binding(std::move(b))
                         : session_.document().create_binding(std::move(b)));
}

Dictionary KasaneDocumentBridge::set_mesh_keyform(const String &id, const PackedFloat32Array &keys,
                                                  const PackedVector2Array &positions) {
    MAIN_THREAD();
    kasane::MeshKeyform form;
    form.positions = vectors(positions);
    for (int64_t i = 0; i < keys.size(); ++i)
        form.keys.push_back(keys[i]);
    if (auto b = session_.document().get_binding(utf8(id)))
        for (const auto &old : b->keyforms)
            if (old.keys == form.keys) {
                form.appearance = old.appearance;
                form.draw_order = old.draw_order;
                break;
            }
    return apply(session_.document().set_mesh_keyform(utf8(id), std::move(form)));
}

Dictionary KasaneDocumentBridge::erase_object(const String &id) {
    MAIN_THREAD();
    return apply(session_.document().erase_object(utf8(id)));
}

Dictionary KasaneDocumentBridge::write_part(const Dictionary &d, bool replace) {
    MAIN_THREAD();
    kasane::Part p;
    if (auto s = part_from_dictionary(d, p); !s.ok())
        return result(s);
    return apply(replace ? session_.document().replace_part(std::move(p))
                         : session_.document().create_part(std::move(p)));
}

Dictionary KasaneDocumentBridge::write_transform(const Dictionary &d, bool replace) {
    MAIN_THREAD();
    kasane::Transform t;
    if (auto s = transform_from_dictionary(d, t); !s.ok())
        return result(s);
    return apply(replace ? session_.document().replace_transform(std::move(t))
                         : session_.document().create_transform(std::move(t)));
}

Dictionary KasaneDocumentBridge::write_scene_binding(const Dictionary &d, bool replace) {
    MAIN_THREAD();
    kasane::SceneBinding b;
    if (auto s = scene_binding_from_dictionary(d, b); !s.ok())
        return result(s);
    return apply(replace ? session_.document().replace_scene_binding(std::move(b))
                         : session_.document().create_scene_binding(std::move(b)));
}

Dictionary KasaneDocumentBridge::set_mesh_properties(const String &id, const Dictionary &d) {
    MAIN_THREAD();
    auto old = session_.document().get_mesh(utf8(id));
    if (!old)
        return result(kasane::Status::error("MISSING_MESH", utf8(id)));
    auto m = *old;
    m.masks.clear();
    m.draw_order.reset();
    if (auto s = mesh_properties_from_dictionary(d, m); !s.ok())
        return result(s);
    return apply(session_.document().replace_mesh(std::move(m)));
}

#undef MAIN_THREAD
} // namespace kasane_gd
