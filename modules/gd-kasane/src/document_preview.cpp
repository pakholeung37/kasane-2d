// SPDX-License-Identifier: MIT
#include "document_preview.hpp"
#include "project_results.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/shader.hpp>
#include <godot_cpp/classes/shader_material.hpp>
#include <godot_cpp/classes/viewport_texture.hpp>
#include <algorithm>
#include <cmath>
using namespace godot;

namespace kasane_gd {
void KasaneDocumentPreview::_bind_methods() {
    ClassDB::bind_method(D_METHOD("set_document", "document"), &KasaneDocumentPreview::set_document);
    ClassDB::bind_method(D_METHOD("get_document"), &KasaneDocumentPreview::get_document);
    ClassDB::bind_method(D_METHOD("set_texture_store", "textures"),
                         &KasaneDocumentPreview::set_texture_store);
    ClassDB::bind_method(D_METHOD("refresh"), &KasaneDocumentPreview::refresh);
    ClassDB::bind_method(D_METHOD("get_last_result"), &KasaneDocumentPreview::get_last_result);
    ClassDB::bind_method(D_METHOD("get_mesh_view", "mesh_id"), &KasaneDocumentPreview::get_mesh_view);
    ClassDB::bind_method(D_METHOD("_document_changed", "change"), &KasaneDocumentPreview::document_changed);
}

void KasaneDocumentPreview::clear_views() {
    for (const auto &[id, view] : views_)
        memdelete(view);
    views_.clear();
    for (auto *mask : masks_)
        memdelete(mask);
    masks_.clear();
}

void KasaneDocumentPreview::set_document(const Ref<KasaneDocumentBridge> &doc) {
    if (document_ == doc)
        return;
    if (document_.is_valid()) {
        document_->disconnect("changed", Callable(this, "_document_changed"));
        document_->disconnect("preview_changed", Callable(this, "refresh"));
    }
    clear_views();
    document_ = doc;
    if (document_.is_valid()) {
        document_->connect("changed", Callable(this, "_document_changed"));
        document_->connect("preview_changed", Callable(this, "refresh"));
    }
    set_process(true);
    refresh();
}

void KasaneDocumentPreview::set_texture_store(const Ref<KasaneTextureStore> &textures) {
    if (textures_ == textures)
        return;
    if (textures_.is_valid())
        textures_->disconnect("changed", Callable(this, "refresh"));
    textures_ = textures;
    if (textures_.is_valid())
        textures_->connect("changed", Callable(this, "refresh"));
    refresh();
}

void KasaneDocumentPreview::document_changed(const Dictionary &) {
    refresh();
}

Dictionary KasaneDocumentPreview::refresh() {
    auto finish = [&](Dictionary result) {
        last_result_ = result;
        return result;
    };
    if (document_.is_null()) {
        clear_views();
        return finish(error("MISSING_DOCUMENT", "Attach a Document."));
    }
    if (!document_->document_session().root().empty()) {
        auto diagnostics = diagnostic_array(document_->document_session().diagnose());
        if (!diagnostics.is_empty()) {
            clear_views();
            auto out = error("INCOMPLETE_RESOURCES", "Project resources failed verification.");
            out["diagnostics"] = diagnostics;
            return finish(out);
        }
    }
    kasane::DrawableFrame frame;
    if (auto s = document_->evaluate(frame); !s.ok())
        return finish(result(s));
    if (textures_.is_null())
        return finish(error("MISSING_TEXTURE_STORE", "Attach a texture store."));
    if (is_inside_tree()) {
        auto transform = get_global_transform_with_canvas();
        mask_scale_ = std::max(0.0001f, std::max(transform[0].length(), transform[1].length()));
    }
    std::unordered_map<std::string, KasaneMeshView *> pending;
    auto fail = [&](Dictionary error) {
        for (const auto &[id, view] : pending)
            memdelete(view);
        clear_views();
        return finish(error);
    };
    for (const auto &d : frame.drawables) {
        if (!document_->document_session().root().empty()) {
            if (auto s = textures_->resolve_asset(document_, string(d.texture_asset_id)); !s.ok())
                return fail(result(s));
        }
        auto texture = textures_->get_texture(string(d.texture_asset_id));
        const auto *asset = document_->source().get_asset(d.texture_asset_id);
        if (texture.is_null())
            return fail(
                error("MISSING_TEXTURE", "Preview texture is not loaded; source edits remain valid."));
        if (texture->get_width() != int64_t(asset->width) || texture->get_height() != int64_t(asset->height))
            return fail(
                error("RESOURCE_MISMATCH", "Preview texture dimensions differ from source metadata."));
        std::vector<kasane::Vec2> positions, uvs;
        // Convert only at the Godot presentation boundary, back to canvas pixels.
        for (auto p : d.positions)
            positions.push_back({p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                                 frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit});
        for (auto uv : d.uvs)
            uvs.push_back({uv.x, 1 - uv.y});
        PackedInt32Array indices;
        for (size_t i = 0; i < d.indices.size(); i += 3) {
            indices.push_back(d.indices[i]);
            indices.push_back(d.indices[i + 2]);
            indices.push_back(d.indices[i + 1]);
        }
        auto *view = memnew(KasaneMeshView);
        pending[d.id] = view;
        auto status = view->initialize(vectors(positions), vectors(uvs), indices, texture);
        if (!bool(status["ok"]))
            return fail(status);
        view->set_visible(d.visible && d.opacity > 0);
        view->set_texture_filter(CanvasItem::TEXTURE_FILTER_LINEAR_WITH_MIPMAPS);
    }
    clear_views();
    views_ = std::move(pending);
    // All source meshes exist before constructing masks. Masks sample texture
    // alpha independently of the source drawable's opacity and blend mode.
    std::vector<const kasane::Drawable *> ordered;
    for (const auto &d : frame.drawables)
        ordered.push_back(&d);
    std::stable_sort(ordered.begin(), ordered.end(),
                     [](auto a, auto b) { return a->render_order < b->render_order; });
    for (const auto *drawable : ordered) {
        const auto &d = *drawable;
        auto *view = views_.at(d.id);
        Ref<ShaderMaterial> material;
        material.instantiate();
        Ref<Shader> shader;
        shader.instantiate();
        String blend = d.blend_mode == kasane::BlendMode::multiplicative ? "blend_mul" : "blend_premul_alpha";
        String code = "shader_type canvas_item; render_mode " + blend + ", unshaded;\n";
        code += "uniform sampler2D main_texture : filter_linear_mipmap, repeat_disable; uniform vec3 "
                "multiply_color; uniform vec3 screen_color; uniform float opacity; uniform bool inverted; "
                "uniform bool masked; uniform sampler2D mask_texture : filter_linear, repeat_disable; "
                "uniform vec4 mask_bounds; varying vec2 point; void vertex(){point=VERTEX;}\n";
        code += "void fragment(){vec4 "
                "c=texture(main_texture,UV);c.rgb*=multiply_color;c.rgb=c.rgb+screen_color-c.rgb*screen_"
                "color;float a=c.a*opacity;if(masked){vec2 uv=(point-mask_bounds.xy)/mask_bounds.zw;float "
                "mask=0.0;if(all(greaterThanEqual(uv,vec2(0)))&&all(lessThanEqual(uv,vec2(1))))mask=texture("
                "mask_texture,uv).a;a*=inverted?1.0-mask:mask;}";
        if (d.blend_mode == kasane::BlendMode::multiplicative)
            code += "COLOR=vec4(c.rgb*a+vec3(1.0-a),1.0);}";
        else if (d.blend_mode == kasane::BlendMode::additive)
            code += "COLOR=vec4(c.rgb*a,0.0);}";
        else
            code += "COLOR=vec4(c.rgb*a,a);}";
        shader->set_code(code);
        material->set_shader(shader);
        material->set_shader_parameter("main_texture", view->get_texture());
        material->set_shader_parameter(
            "multiply_color", Vector3(d.multiply_color[0], d.multiply_color[1], d.multiply_color[2]));
        material->set_shader_parameter("screen_color",
                                       Vector3(d.screen_color[0], d.screen_color[1], d.screen_color[2]));
        material->set_shader_parameter("opacity", d.opacity);
        material->set_shader_parameter("masked", !d.masks.empty());
        material->set_shader_parameter("inverted", d.inverted_mask);
        if (!d.masks.empty()) {
            Rect2 bounds;
            bool first = true;
            for (auto &id : d.masks)
                for (auto point : views_.at(id)->get_positions_snapshot()) {
                    if (first) {
                        bounds = Rect2(point, Vector2());
                        first = false;
                    } else
                        bounds = bounds.expand(point);
                }
            bounds = bounds.grow(4);
            Vector2i size(std::max(1, int(std::ceil(bounds.size.x))),
                          std::max(1, int(std::ceil(bounds.size.y))));
            // Bound allocation while preserving normalized sampling outside the canvas.
            float scale = std::min(float(mask_scale_), 4096.0f / std::max(size.x, size.y));
            size = Vector2i(std::max(1, int(std::ceil(size.x * scale))),
                            std::max(1, int(std::ceil(size.y * scale))));
            auto *viewport = memnew(SubViewport);
            viewport->set_size(size);
            viewport->set_transparent_background(true);
            viewport->set_disable_3d(true);
            viewport->set_update_mode(SubViewport::UPDATE_ALWAYS);
            masks_.push_back(viewport);
            add_child(viewport);
            auto *root = memnew(Node2D);
            viewport->add_child(root);
            root->set_scale(Vector2(scale, scale));
            root->set_position(-bounds.position * scale);
            Ref<Shader> mask_shader;
            mask_shader.instantiate();
            mask_shader->set_code("shader_type canvas_item;render_mode blend_mix,unshaded;uniform sampler2D "
                                  "main_texture:filter_linear_mipmap,repeat_disable;void vertex(){}void "
                                  "fragment(){COLOR=vec4(0,0,0,texture(main_texture,UV).a);}");
            for (auto &id : d.masks) {
                auto *source = views_.at(id);
                auto *mask = memnew(MeshInstance2D);
                mask->set_mesh(source->get_mesh());
                Ref<ShaderMaterial> mat;
                mat.instantiate();
                mat->set_shader(mask_shader);
                mat->set_shader_parameter("main_texture", source->get_texture());
                mask->set_material(mat);
                root->add_child(mask);
            }
            material->set_shader_parameter("mask_texture", viewport->get_texture());
            material->set_shader_parameter(
                "mask_bounds", Vector4(bounds.position.x, bounds.position.y, size.x / scale, size.y / scale));
        }
        view->set_material(material);
        add_child(view);
    }
    auto status = result({});
    status["revision"] = frame.source_revision;
    return finish(status);
}

void KasaneDocumentPreview::_process(double) {
    auto transform = get_global_transform_with_canvas();
    double scale = std::max(0.0001f, std::max(transform[0].length(), transform[1].length()));
    if (std::abs(scale - mask_scale_) > 0.00001)
        refresh();
}

KasaneMeshView *KasaneDocumentPreview::get_mesh_view(const String &id) const {
    auto it = views_.find(utf8(id));
    return it == views_.end() ? nullptr : it->second;
}
} // namespace kasane_gd
