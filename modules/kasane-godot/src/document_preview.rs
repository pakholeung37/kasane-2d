use godot::classes::{
    canvas_item::TextureFilter, sub_viewport::UpdateMode, Engine, MeshInstance2D, Node2D,
    RenderingServer, Shader, ShaderMaterial, SubViewport, Texture2D, Viewport, Window,
};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::{BlendMode, Status, Vec2};

use crate::conversions::{error_dict, status_to_dict, Array, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;
use crate::mesh_view::KasaneMeshView;
use crate::render_frame_validation::validate_frame;
use crate::texture_store::KasaneTextureStore;

struct MeshKey {
    uvs: Vec<Vec2>,
    indices: Vec<u32>,
    texture: Gd<Texture2D>,
}

struct MaskView {
    viewport: Gd<SubViewport>,
    root: Gd<Node2D>,
    sources: HashMap<String, Gd<MeshInstance2D>>,
    materials: HashMap<String, Gd<ShaderMaterial>>,
}

#[derive(GodotClass)]
#[class(init, base=Node2D)]
pub struct KasaneDocumentPreview {
    base: Base<Node2D>,
    document: Option<Gd<KasaneDocumentBridge>>,
    textures: Option<Gd<KasaneTextureStore>>,
    views: HashMap<String, Gd<KasaneMeshView>>,
    model_root: Option<Gd<Node2D>>,
    mesh_keys: HashMap<String, MeshKey>,
    materials: HashMap<String, Gd<ShaderMaterial>>,
    shaders: HashMap<u8, Gd<Shader>>,
    mask_shader: Option<Gd<Shader>>,
    masks: HashMap<String, MaskView>,
    mask_scale: f64,
    last_result: Dictionary,
    submission_id: u64,
    submitted_frame: i32,
    completed_submission: u64,
    drawing_submission: Option<(u64, Rid)>,
    pending_draws: i32,
    runtime_frame: Option<DrawableFrame>,
    runtime_textures: HashMap<String, Gd<Texture2D>>,
}

#[godot_api]
impl INode2D for KasaneDocumentPreview {
    fn enter_tree(&mut self) {
        for (signal, method) in [
            ("frame_pre_draw", "_on_frame_pre_draw"),
            ("frame_post_draw", "_on_frame_post_draw"),
        ] {
            let callback = self.to_gd().callable(method);
            RenderingServer::singleton().connect(signal, &callback);
        }
        // A completed image from the previous viewport cannot certify the new one.
        self.completed_submission = 0;
        self.pending_draws = if self.masks.is_empty() { 1 } else { 2 };
    }

    fn exit_tree(&mut self) {
        for (signal, method) in [
            ("frame_pre_draw", "_on_frame_pre_draw"),
            ("frame_post_draw", "_on_frame_post_draw"),
        ] {
            let callback = self.to_gd().callable(method);
            RenderingServer::singleton().disconnect(signal, &callback);
        }
        self.drawing_submission = None;
        self.completed_submission = 0;
    }

    fn process(&mut self, _delta: f64) {
        let transform = self.base().get_global_transform_with_canvas();
        let x_len = (transform.a.x * transform.a.x + transform.a.y * transform.a.y).sqrt();
        let y_len = (transform.b.x * transform.b.x + transform.b.y * transform.b.y).sqrt();
        let scale = (x_len.max(y_len) as f64).max(0.0001);
        if (scale - self.mask_scale).abs() > 0.00001 {
            if self.document.is_some() {
                self.refresh_geometry();
            } else if let Some(frame) = self.runtime_frame.clone() {
                let textures = self.runtime_textures.clone();
                self.render_frame(&frame, &textures);
            }
        }
    }
}

#[godot_api]
impl KasaneDocumentPreview {
    fn clear_views(&mut self) {
        for (_, mut view) in self.views.drain() {
            view.queue_free();
        }
        for (_, mut mask) in self.masks.drain() {
            mask.viewport.queue_free();
        }
        self.mesh_keys.clear();
        self.materials.clear();
    }

    #[func]
    pub fn set_document(&mut self, doc: Option<Gd<KasaneDocumentBridge>>) {
        if self.document == doc {
            return;
        }
        if let Some(mut old_doc) = self.document.take() {
            let changed_callable = self.to_gd().callable("_document_changed");
            let refresh_callable = self.to_gd().callable("refresh_geometry");
            old_doc.disconnect("changed", &changed_callable);
            old_doc.disconnect("preview_changed", &refresh_callable);
        }
        self.clear_views();
        self.runtime_frame = None;
        self.runtime_textures.clear();
        self.document = doc.clone();
        if let Some(mut new_doc) = doc {
            let changed_callable = self.to_gd().callable("_document_changed");
            let refresh_callable = self.to_gd().callable("refresh_geometry");
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
    pub fn get_observation_state(&self) -> Dictionary {
        let mut state = self.last_result.clone();
        let ok = state
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        let drawn = Engine::singleton().get_frames_drawn();
        state.set(
            "ready",
            ok && self.base().is_visible_in_tree()
                && self.pending_draws == 0
                && self.completed_submission == self.submission_id,
        );
        state.set("submitted_frame", self.submitted_frame);
        state.set("drawn_frame", drawn);
        state
    }

    #[func]
    pub fn _on_frame_pre_draw(&mut self) {
        self.drawing_submission = None;
        if self.pending_draws > 0 && self.base().is_visible_in_tree() {
            if let Some(viewport) = self.base().get_viewport() {
                if viewport_will_draw(&viewport) {
                    self.drawing_submission =
                        Some((self.submission_id, viewport.get_viewport_rid()));
                }
            }
        }
    }

    #[func]
    pub fn _on_frame_post_draw(&mut self) {
        let Some((submission, viewport_rid)) = self.drawing_submission.take() else {
            return;
        };
        if self.pending_draws > 0
            && submission == self.submission_id
            && self.base().is_visible_in_tree()
            && self
                .base()
                .get_viewport()
                .is_some_and(|v| v.get_viewport_rid() == viewport_rid)
        {
            self.pending_draws -= 1;
            if self.pending_draws == 0 {
                self.completed_submission = submission;
            }
        }
    }

    #[func]
    pub fn get_render_stats(&self) -> Dictionary {
        let mut stats = Dictionary::new();
        stats.set("mesh_views", self.views.len() as i64);
        stats.set("mask_viewports", self.masks.len() as i64);
        stats.set("materials", self.materials.len() as i64);
        stats.set(
            "shaders",
            self.shaders.len() as i64 + i64::from(self.mask_shader.is_some()),
        );
        let mut uploads = 0i64;
        let mut creations = 0i64;
        for view in self.views.values() {
            let s = view.bind().get_render_stats();
            uploads += s
                .get("uploads")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(0);
            creations += s
                .get("creations")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(0);
        }
        stats.set("uploads", uploads);
        stats.set("creations", creations);
        stats
    }

    #[func]
    pub fn get_mesh_view(&self, mesh_id: GString) -> Option<Gd<KasaneMeshView>> {
        self.views.get(&mesh_id.to_string()).cloned()
    }

    #[func]
    pub fn refresh(&mut self) -> Dictionary {
        self.refresh_inner(true)
    }

    /// Preview values and camera changes cannot mutate asset data. Explicit
    /// refresh/document/texture changes still revalidate resources from disk.
    #[func]
    pub fn refresh_geometry(&mut self) -> Dictionary {
        self.refresh_inner(false)
    }

    fn refresh_inner(&mut self, reload_assets: bool) -> Dictionary {
        let Some(doc) = self.document.clone() else {
            self.clear_views();
            let res = error_dict("MISSING_DOCUMENT", "Attach a Document.");
            self.last_result = res.clone();
            return res;
        };

        if reload_assets && !doc.bind().session().root().as_os_str().is_empty() {
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
            self.clear_views();
            let out = error_dict("MISSING_TEXTURE_STORE", "Attach a texture store.");
            self.last_result = out.clone();
            return out;
        };

        let mut resolved = HashMap::new();
        for d in &frame.drawables {
            // Shared atlas assets must be validated/decoded once per refresh, not
            // once per mesh. Explicit refresh still detects disk edits.
            if resolved.contains_key(&d.texture_asset_id) { continue; }
            let missing_texture = textures.bind().get_texture(GString::from(d.texture_asset_id.as_str())).is_none();
            if (reload_assets || missing_texture) && !doc.bind().session().root().as_os_str().is_empty() {
                let s = textures.bind_mut().resolve_asset(
                    Some(doc.clone()),
                    GString::from(d.texture_asset_id.as_str()),
                );
                if !s.is_ok() {
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
                self.clear_views();
                let out = error_dict("MISSING_ASSET", &d.texture_asset_id);
                self.last_result = out.clone();
                return out;
            };

            if tex.get_width() != a.width as i32 || tex.get_height() != a.height as i32 {
                drop(doc_bind);
                self.clear_views();
                let out = error_dict(
                    "RESOURCE_MISMATCH",
                    "Preview texture dimensions differ from source metadata.",
                );
                self.last_result = out.clone();
                return out;
            }
            drop(doc_bind);
            resolved.insert(d.texture_asset_id.clone(), tex);
        }
        self.render_frame(&frame, &resolved)
    }

    /// Shared Rust drawing contract. A runtime model can submit an evaluated
    /// frame and its textures without constructing an editable Document.
    pub fn submit_frame(
        &mut self,
        frame: &DrawableFrame,
        textures: &HashMap<String, Gd<Texture2D>>,
    ) -> Dictionary {
        let result = self.render_frame(frame, textures);
        if result.get("ok").and_then(|v| v.try_to::<bool>().ok()) == Some(true) {
            self.runtime_frame = Some(frame.clone());
            self.runtime_textures = textures.clone();
            self.base_mut().set_process(true);
        } else {
            self.runtime_frame = None;
            self.runtime_textures.clear();
        }
        result
    }

    fn render_frame(
        &mut self,
        frame: &DrawableFrame,
        textures: &HashMap<String, Gd<Texture2D>>,
    ) -> Dictionary {
        // Validate the entire submission before indexing triangles or changing resources.
        self.drawing_submission = None;
        let mut status = validate_frame(frame);
        if status.is_ok() {
            for drawable in &frame.drawables {
                match textures.get(&drawable.texture_asset_id) {
                    None => {
                        status = Status::error("MISSING_TEXTURE", &drawable.texture_asset_id);
                        break;
                    }
                    Some(texture) if texture.get_width() <= 0 || texture.get_height() <= 0 => {
                        status = Status::error("INVALID_TEXTURE", &drawable.texture_asset_id);
                        break;
                    }
                    _ => {}
                }
            }
        }
        if !status.is_ok() {
            self.pending_draws = 0;
            self.completed_submission = 0;
            self.last_result = status_to_dict(&status);
            return self.last_result.clone();
        }
        if self.model_root.is_none() {
            let root = Node2D::new_alloc();
            self.base_mut().add_child(&root);
            self.base_mut().move_child(&root, 0);
            self.model_root = Some(root);
        }
        if self.base().is_inside_tree() {
            let transform = self.base().get_global_transform_with_canvas();
            let x_len = (transform.a.x * transform.a.x + transform.a.y * transform.a.y).sqrt();
            let y_len = (transform.b.x * transform.b.x + transform.b.y * transform.b.y).sqrt();
            self.mask_scale = (x_len.max(y_len) as f64).max(0.0001);
        }
        for d in &frame.drawables {
            let Some(tex) = textures.get(&d.texture_asset_id).cloned() else {
                self.clear_views();
                let out = error_dict("MISSING_TEXTURE", &d.texture_asset_id);
                self.last_result = out.clone();
                return out;
            };
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

            let reuse = self.mesh_keys.get(&d.id).is_some_and(|key| {
                key.indices == d.indices && key.uvs == d.uvs && key.texture == tex
            });
            let mut view = self
                .views
                .get(&d.id)
                .cloned()
                .unwrap_or_else(KasaneMeshView::new_alloc);
            let update_status = if reuse {
                view.bind_mut().update_positions(positions)
            } else {
                view.bind_mut()
                    .initialize(positions, uvs, indices, Some(tex.clone()))
            };
            let ok = update_status
                .get("ok")
                .and_then(|v| v.try_to::<bool>().ok())
                .unwrap_or(false);
            if !ok {
                if !self.views.contains_key(&d.id) {
                    view.queue_free();
                }
                self.clear_views();
                self.last_result = update_status.clone();
                return update_status;
            }
            if !reuse {
                self.mesh_keys.insert(
                    d.id.clone(),
                    MeshKey {
                        uvs: d.uvs.clone(),
                        indices: d.indices.clone(),
                        texture: tex,
                    },
                );
            }
            view.set_visible(d.visible && d.opacity > 0.0 && !d.indices.is_empty());
            view.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
            if !self.views.contains_key(&d.id) {
                self.model_root.as_mut().unwrap().add_child(&view);
                self.views.insert(d.id.clone(), view);
            }
        }

        let removed: Vec<String> = self
            .views
            .keys()
            .filter(|id| !frame.drawables.iter().any(|d| &d.id == *id))
            .cloned()
            .collect();
        for id in removed {
            if let Some(mut view) = self.views.remove(&id) {
                view.queue_free();
            }
            self.mesh_keys.remove(&id);
            self.materials.remove(&id);
            if let Some(mut mask) = self.masks.remove(&id) {
                mask.viewport.queue_free();
            }
        }

        let mut ordered: Vec<&kasane_core::evaluation::Drawable> = frame.drawables.iter().collect();
        ordered.sort_by_key(|a| a.render_order);

        for drawable in ordered {
            let d = drawable;
            let mut view = self.views.get(&d.id).unwrap().clone();
            let blend_key = match d.blend_mode {
                BlendMode::Normal => 0,
                BlendMode::Additive => 1,
                BlendMode::Multiplicative => 2,
            };
            let shader = self
                .shaders
                .entry(blend_key)
                .or_insert_with(|| {
                    let (blend, output) = match blend_key {
                        1 => ("blend_premul_alpha", "COLOR = vec4(c.rgb * a, 0.0);"),
                        2 => ("blend_mul", "COLOR = vec4(c.rgb * a + vec3(1.0 - a), 1.0);"),
                        _ => ("blend_premul_alpha", "COLOR = vec4(c.rgb * a, a);"),
                    };
                    let code = include_str!("../shaders/drawable.gdshader")
                        .replace("__BLEND__", blend)
                        .replace("__OUTPUT__", output);
                    let mut shader = Shader::new_gd();
                    shader.set_code(&GString::from(code.as_str()));
                    shader
                })
                .clone();
            let mut material = self
                .materials
                .get(&d.id)
                .cloned()
                .unwrap_or_else(ShaderMaterial::new_gd);
            if material.get_shader() != Some(shader.clone()) {
                material.set_shader(&shader);
            }
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

                if !self.masks.contains_key(&d.id) {
                    let mut viewport = SubViewport::new_alloc();
                    viewport.set_transparent_background(true);
                    viewport.set_disable_3d(true);
                    viewport.set_update_mode(UpdateMode::ALWAYS);
                    self.base_mut().add_child(&viewport);
                    let root = Node2D::new_alloc();
                    viewport.add_child(&root);
                    self.masks.insert(
                        d.id.clone(),
                        MaskView {
                            viewport,
                            root,
                            sources: HashMap::new(),
                            materials: HashMap::new(),
                        },
                    );
                }
                let mask_shader = self
                    .mask_shader
                    .get_or_insert_with(|| {
                        let mut shader = Shader::new_gd();
                        shader.set_code(&GString::from(include_str!("../shaders/mask.gdshader")));
                        shader
                    })
                    .clone();
                let mask = self.masks.get_mut(&d.id).unwrap();
                mask.viewport.set_size(Vector2i::new(final_sx, final_sy));
                mask.root.set_scale(Vector2::new(scale, scale));
                mask.root.set_position(-bounds.position * scale);
                let stale: Vec<String> = mask
                    .sources
                    .keys()
                    .filter(|id| !d.masks.contains(id))
                    .cloned()
                    .collect();
                for id in stale {
                    if let Some(mut mesh) = mask.sources.remove(&id) {
                        mesh.queue_free();
                    }
                    mask.materials.remove(&id);
                }
                for mask_id in &d.masks {
                    if let Some(source) = self.views.get(mask_id) {
                        if !mask.sources.contains_key(mask_id) {
                            let mesh = MeshInstance2D::new_alloc();
                            mask.root.add_child(&mesh);
                            mask.sources.insert(mask_id.clone(), mesh);
                        }
                        let mesh = mask.sources.get_mut(mask_id).unwrap();
                        if let Some(m) = source.get_mesh() {
                            mesh.set_mesh(&m);
                        }
                        let mat = mask.materials.entry(mask_id.clone()).or_insert_with(|| {
                            let mut mat = ShaderMaterial::new_gd();
                            mat.set_shader(&mask_shader);
                            mat
                        });
                        if let Some(st) = source.get_texture() {
                            mat.set_shader_parameter("main_texture", &st.to_variant());
                        }
                        mesh.set_material(&*mat);
                    }
                }

                if let Some(vp_tex) = mask.viewport.get_texture() {
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
            } else if let Some(mut mask) = self.masks.remove(&d.id) {
                mask.viewport.queue_free();
            }

            view.set_material(&material);
            self.materials.insert(d.id.clone(), material);
            self.model_root.as_mut().unwrap().move_child(&view, -1);
        }

        let mut res = status_to_dict(&Status::ok());
        self.submission_id = self.submission_id.wrapping_add(1);
        res.set("submission_id", self.submission_id as i64);
        res.set("revision", frame.source_revision as i64);
        self.submitted_frame = Engine::singleton().get_frames_drawn();
        self.pending_draws = if self.masks.is_empty() { 1 } else { 2 };
        self.last_result = res.clone();
        res
    }
}

// Godot does not expose whether a WHEN_VISIBLE viewport texture was consumed.
// Do not infer that from CanvasItem visibility: screenshot callers must use an
// explicit update mode. Read the RenderingServer mode: SubViewport caches ONCE
// even after the server has consumed it and changed its actual mode to DISABLED.
fn viewport_will_draw(viewport: &Gd<Viewport>) -> bool {
    if !viewport.is_inside_tree() {
        return false;
    }
    if let Ok(sub) = viewport.clone().try_cast::<SubViewport>() {
        let size = sub.get_size();
        return size.x > 1
            && size.y > 1
            && matches!(
                RenderingServer::singleton().viewport_get_update_mode(sub.get_viewport_rid()),
                godot::classes::rendering_server::ViewportUpdateMode::ALWAYS
                    | godot::classes::rendering_server::ViewportUpdateMode::ONCE
            );
    }
    if let Ok(window) = viewport.clone().try_cast::<Window>() {
        return window.is_visible()
            && window.get_mode() != godot::classes::window::Mode::MINIMIZED
            && window.get_size().x > 1
            && window.get_size().y > 1;
    }
    false
}
