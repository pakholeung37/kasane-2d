use godot::classes::{
    canvas_item::TextureFilter, sub_viewport::UpdateMode, MeshInstance2D, Node2D, Shader,
    ShaderMaterial, SubViewport,
};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::{BlendMode, Status};

use crate::conversions::{error_dict, status_to_dict, Array, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;
use crate::mesh_view::KasaneMeshView;
use crate::texture_store::KasaneTextureStore;

#[derive(GodotClass)]
#[class(init, base=Node2D)]
pub struct KasaneDocumentPreview {
    base: Base<Node2D>,
    document: Option<Gd<KasaneDocumentBridge>>,
    textures: Option<Gd<KasaneTextureStore>>,
    views: HashMap<String, Gd<KasaneMeshView>>,
    masks: Vec<Gd<SubViewport>>,
    mask_scale: f64,
    last_result: Dictionary,
}

#[godot_api]
impl INode2D for KasaneDocumentPreview {
    fn process(&mut self, _delta: f64) {
        let transform = self.base().get_global_transform_with_canvas();
        let x_len = (transform.a.x * transform.a.x + transform.a.y * transform.a.y).sqrt();
        let y_len = (transform.b.x * transform.b.x + transform.b.y * transform.b.y).sqrt();
        let scale = (x_len.max(y_len) as f64).max(0.0001);
        if (scale - self.mask_scale).abs() > 0.00001 {
            self.refresh();
        }
    }
}

#[godot_api]
impl KasaneDocumentPreview {
    fn clear_views(&mut self) {
        for (_, mut view) in self.views.drain() {
            view.queue_free();
        }
        for mut mask in self.masks.drain(..) {
            mask.queue_free();
        }
    }

    #[func]
    pub fn set_document(&mut self, doc: Option<Gd<KasaneDocumentBridge>>) {
        if self.document == doc {
            return;
        }
        if let Some(mut old_doc) = self.document.take() {
            let changed_callable = self.to_gd().callable("_document_changed");
            let refresh_callable = self.to_gd().callable("refresh");
            old_doc.disconnect("changed", &changed_callable);
            old_doc.disconnect("preview_changed", &refresh_callable);
        }
        self.clear_views();
        self.document = doc.clone();
        if let Some(mut new_doc) = doc {
            let changed_callable = self.to_gd().callable("_document_changed");
            let refresh_callable = self.to_gd().callable("refresh");
            new_doc.connect("changed", &changed_callable);
            new_doc.connect("preview_changed", &refresh_callable);
        }
        self.base_mut().set_process(true);
        self.refresh();
    }

    #[func]
    pub fn get_document(&self) -> Option<Gd<KasaneDocumentBridge>> {
        self.document.clone()
    }

    #[func]
    pub fn set_texture_store(&mut self, textures: Option<Gd<KasaneTextureStore>>) {
        if self.textures == textures {
            return;
        }
        if let Some(mut old_tex) = self.textures.take() {
            let refresh_callable = self.to_gd().callable("refresh");
            old_tex.disconnect("changed", &refresh_callable);
        }
        self.textures = textures.clone();
        if let Some(mut new_tex) = textures {
            let refresh_callable = self.to_gd().callable("refresh");
            new_tex.connect("changed", &refresh_callable);
        }
        self.refresh();
    }

    #[func]
    pub fn _document_changed(&mut self, _change: Dictionary) {
        self.refresh();
    }

    #[func]
    pub fn get_last_result(&self) -> Dictionary {
        self.last_result.clone()
    }

    #[func]
    pub fn get_mesh_view(&self, mesh_id: GString) -> Option<Gd<KasaneMeshView>> {
        self.views.get(&mesh_id.to_string()).cloned()
    }

    #[func]
    pub fn refresh(&mut self) -> Dictionary {
        let Some(doc) = self.document.clone() else {
            self.clear_views();
            let res = error_dict("MISSING_DOCUMENT", "Attach a Document.");
            self.last_result = res.clone();
            return res;
        };

        if !doc.bind().session().root().as_os_str().is_empty() {
            let diags = doc.bind().session().diagnose();
            if !diags.is_empty() {
                self.clear_views();
                let mut out = error_dict(
                    "INCOMPLETE_RESOURCES",
                    "Project resources failed verification.",
                );
                let mut diag_arr = Array::new();
                for d in &diags {
                    let mut item = Dictionary::new();
                    item.set("asset_id", d.asset_id.as_str());
                    item.set("code", d.code.as_str());
                    item.set("message", d.message.as_str());
                    diag_arr.push(&item);
                }
                out.set("diagnostics", &diag_arr);
                self.last_result = out.clone();
                return out;
            }
        }

        let mut frame = DrawableFrame::default();
        let status = doc.bind().evaluate(&mut frame);
        if !status.is_ok() {
            self.clear_views();
            let out = status_to_dict(&status);
            self.last_result = out.clone();
            return out;
        }

        let Some(mut textures) = self.textures.clone() else {
            let out = error_dict("MISSING_TEXTURE_STORE", "Attach a texture store.");
            self.last_result = out.clone();
            return out;
        };

        if self.base().is_inside_tree() {
            let transform = self.base().get_global_transform_with_canvas();
            let x_len = (transform.a.x * transform.a.x + transform.a.y * transform.a.y).sqrt();
            let y_len = (transform.b.x * transform.b.x + transform.b.y * transform.b.y).sqrt();
            self.mask_scale = (x_len.max(y_len) as f64).max(0.0001);
        }

        let mut pending: HashMap<String, Gd<KasaneMeshView>> = HashMap::new();

        for d in &frame.drawables {
            if !doc.bind().session().root().as_os_str().is_empty() {
                let s = textures.bind_mut().resolve_asset(
                    Some(doc.clone()),
                    GString::from(d.texture_asset_id.as_str()),
                );
                if !s.is_ok() {
                    for (_, mut v) in pending.drain() {
                        v.queue_free();
                    }
                    self.clear_views();
                    let out = status_to_dict(&s);
                    self.last_result = out.clone();
                    return out;
                }
            }

            let texture = textures
                .bind()
                .get_texture(GString::from(d.texture_asset_id.as_str()));
            let doc_bind = doc.bind();
            let asset = doc_bind.session().document().get_asset(&d.texture_asset_id);
            let Some(tex) = texture else {
                drop(doc_bind);
                for (_, mut v) in pending.drain() {
                    v.queue_free();
                }
                self.clear_views();
                let out = error_dict(
                    "MISSING_TEXTURE",
                    "Preview texture is not loaded; source edits remain valid.",
                );
                self.last_result = out.clone();
                return out;
            };
            let Some(a) = asset else {
                drop(doc_bind);
                for (_, mut v) in pending.drain() {
                    v.queue_free();
                }
                self.clear_views();
                let out = error_dict("MISSING_ASSET", &d.texture_asset_id);
                self.last_result = out.clone();
                return out;
            };

            if tex.get_width() != a.width as i32 || tex.get_height() != a.height as i32 {
                drop(doc_bind);
                for (_, mut v) in pending.drain() {
                    v.queue_free();
                }
                self.clear_views();
                let out = error_dict(
                    "RESOURCE_MISMATCH",
                    "Preview texture dimensions differ from source metadata.",
                );
                self.last_result = out.clone();
                return out;
            }
            drop(doc_bind);

            let mut positions = PackedVector2Array::new();
            positions.resize(d.positions.len());
            for (i, p) in d.positions.iter().enumerate() {
                let px = p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x;
                let py = frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit;
                positions[i] = Vector2::new(px, py);
            }

            let mut uvs = PackedVector2Array::new();
            uvs.resize(d.uvs.len());
            for (i, uv) in d.uvs.iter().enumerate() {
                uvs[i] = Vector2::new(uv.x, 1.0 - uv.y);
            }

            let mut indices = PackedInt32Array::new();
            indices.resize(d.indices.len());
            for i in (0..d.indices.len()).step_by(3) {
                indices[i] = d.indices[i] as i32;
                indices[i + 1] = d.indices[i + 2] as i32;
                indices[i + 2] = d.indices[i + 1] as i32;
            }

            let mut view = KasaneMeshView::new_alloc();
            let init_status = view
                .bind_mut()
                .initialize(positions, uvs, indices, Some(tex));
            let ok = init_status
                .get("ok")
                .and_then(|v| v.try_to::<bool>().ok())
                .unwrap_or(false);
            if !ok {
                for (_, mut v) in pending.drain() {
                    v.queue_free();
                }
                self.clear_views();
                self.last_result = init_status.clone();
                return init_status;
            }
            view.set_visible(d.visible && d.opacity > 0.0);
            view.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
            pending.insert(d.id.clone(), view);
        }

        self.clear_views();
        self.views = pending;

        let mut ordered: Vec<&kasane_core::evaluation::Drawable> = frame.drawables.iter().collect();
        ordered.sort_by_key(|a| a.render_order);

        for drawable in ordered {
            let d = drawable;
            let mut view = self.views.get(&d.id).unwrap().clone();
            let mut material = ShaderMaterial::new_gd();
            let mut shader = Shader::new_gd();
            let blend = if d.blend_mode == BlendMode::Multiplicative {
                "blend_mul"
            } else {
                "blend_premul_alpha"
            };
            let mut code = format!(
                "shader_type canvas_item; render_mode {}, unshaded;\n",
                blend
            );
            code += "uniform sampler2D main_texture : filter_linear_mipmap, repeat_disable; uniform vec3 multiply_color; uniform vec3 screen_color; uniform float opacity; uniform bool inverted; uniform bool masked; uniform sampler2D mask_texture : filter_linear, repeat_disable; uniform vec4 mask_bounds; varying vec2 point; void vertex(){point=VERTEX;}\n";
            code += "void fragment(){vec4 c=texture(main_texture,UV);c.rgb*=multiply_color;c.rgb=c.rgb+screen_color-c.rgb*screen_color;float a=c.a*opacity;if(masked){vec2 uv=(point-mask_bounds.xy)/mask_bounds.zw;float mask=0.0;if(all(greaterThanEqual(uv,vec2(0)))&&all(lessThanEqual(uv,vec2(1))))mask=texture(mask_texture,uv).a;a*=inverted?1.0-mask:mask;}";
            if d.blend_mode == BlendMode::Multiplicative {
                code += "COLOR=vec4(c.rgb*a+vec3(1.0-a),1.0);}";
            } else if d.blend_mode == BlendMode::Additive {
                code += "COLOR=vec4(c.rgb*a,0.0);}";
            } else {
                code += "COLOR=vec4(c.rgb*a,a);}";
            }
            shader.set_code(&GString::from(code.as_str()));
            material.set_shader(&shader);
            let tex = view.get_texture();
            if let Some(t) = tex {
                material.set_shader_parameter("main_texture", &t.to_variant());
            }
            material.set_shader_parameter(
                "multiply_color",
                &Vector3::new(
                    d.multiply_color[0],
                    d.multiply_color[1],
                    d.multiply_color[2],
                )
                .to_variant(),
            );
            material.set_shader_parameter(
                "screen_color",
                &Vector3::new(d.screen_color[0], d.screen_color[1], d.screen_color[2]).to_variant(),
            );
            material.set_shader_parameter("opacity", &d.opacity.to_variant());
            material.set_shader_parameter("masked", &(!d.masks.is_empty()).to_variant());
            material.set_shader_parameter("inverted", &d.inverted_mask.to_variant());

            if !d.masks.is_empty() {
                let mut bounds = Rect2::default();
                let mut first = true;
                for mask_id in &d.masks {
                    if let Some(src) = self.views.get(mask_id) {
                        let pts = src.bind().get_positions_snapshot();
                        for i in 0..pts.len() {
                            let pt = pts[i];
                            if first {
                                bounds = Rect2::new(pt, Vector2::ZERO);
                                first = false;
                            } else {
                                bounds = bounds.expand(pt);
                            }
                        }
                    }
                }
                bounds = bounds.grow(4.0);
                let size_x = 1.max(bounds.size.x.ceil() as i32);
                let size_y = 1.max(bounds.size.y.ceil() as i32);
                let max_dim = size_x.max(size_y) as f32;
                let scale = (self.mask_scale as f32).min(4096.0 / max_dim);
                let final_sx = 1.max((size_x as f32 * scale).ceil() as i32);
                let final_sy = 1.max((size_y as f32 * scale).ceil() as i32);

                let mut viewport = SubViewport::new_alloc();
                viewport.set_size(Vector2i::new(final_sx, final_sy));
                viewport.set_transparent_background(true);
                viewport.set_disable_3d(true);
                viewport.set_update_mode(UpdateMode::ALWAYS);
                self.masks.push(viewport.clone());
                self.base_mut().add_child(&viewport);

                let mut root = Node2D::new_alloc();
                viewport.add_child(&root);
                root.set_scale(Vector2::new(scale, scale));
                root.set_position(-bounds.position * scale);

                let mut mask_shader = Shader::new_gd();
                mask_shader.set_code(&GString::from(
                    "shader_type canvas_item;render_mode blend_mix,unshaded;uniform sampler2D main_texture:filter_linear_mipmap,repeat_disable;void vertex(){}void fragment(){COLOR=vec4(0,0,0,texture(main_texture,UV).a);}",
                ));

                for mask_id in &d.masks {
                    if let Some(source) = self.views.get(mask_id) {
                        let mut mask_mesh = MeshInstance2D::new_alloc();
                        if let Some(m) = source.get_mesh() {
                            mask_mesh.set_mesh(&m);
                        }
                        let mut mat = ShaderMaterial::new_gd();
                        mat.set_shader(&mask_shader);
                        if let Some(st) = source.get_texture() {
                            mat.set_shader_parameter("main_texture", &st.to_variant());
                        }
                        mask_mesh.set_material(&mat);
                        root.add_child(&mask_mesh);
                    }
                }

                if let Some(vp_tex) = viewport.get_texture() {
                    material.set_shader_parameter("mask_texture", &vp_tex.to_variant());
                }
                material.set_shader_parameter(
                    "mask_bounds",
                    &Vector4::new(
                        bounds.position.x,
                        bounds.position.y,
                        final_sx as f32 / scale,
                        final_sy as f32 / scale,
                    )
                    .to_variant(),
                );
            }

            view.set_material(&material);
            self.base_mut().add_child(&view);
        }

        let mut res = status_to_dict(&Status::ok());
        res.set("revision", frame.source_revision as i64);
        self.last_result = res.clone();
        res
    }
}
