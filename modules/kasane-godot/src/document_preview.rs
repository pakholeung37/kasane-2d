mod commands;
mod masks;
mod resources;
mod surfaces;
use resources::OFFSCREEN_BUDGET_BYTES;

use godot::classes::{
    back_buffer_copy::CopyMode, canvas_item::TextureFilter, sub_viewport::UpdateMode,
    BackBufferCopy, Engine, MeshInstance2D, Node, Node2D, RenderingServer, Shader, ShaderMaterial,
    Sprite2D, SubViewport, Texture2D, Viewport, Window,
};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::evaluation::{DrawableFrame, RenderCommand};
use kasane_core::types::{BlendMode, Status, Vec2};

use crate::conversions::{error_dict, status_to_dict, Array, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;
use crate::mesh_view::KasaneMeshView;
use crate::render_frame_validation::validate_frame;
use crate::texture_store::KasaneTextureStore;

struct MeshKey {
    uvs: std::sync::Arc<[Vec2]>,
    indices: std::sync::Arc<[u32]>,
    texture: Gd<Texture2D>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MaskKey {
    sources: Vec<String>,
    scale_bits: u64,
    consumer: String,
}

impl MaskKey {
    fn new(sources: &[String], scale: f64, consumer: &str) -> Self {
        Self {
            sources: sources.to_vec(),
            scale_bits: scale.to_bits(),
            consumer: consumer.to_owned(),
        }
    }
}

struct MaskView {
    last_submission: u64,
    bounds: Vector4,
    viewport: Gd<SubViewport>,
    root: Gd<Node2D>,
    sources: HashMap<String, Gd<MeshInstance2D>>,
    materials: HashMap<String, Gd<ShaderMaterial>>,
}

struct OffscreenView {
    viewport: Gd<SubViewport>,
    root: Gd<Node2D>,
    composite: Gd<Sprite2D>,
    material: Gd<ShaderMaterial>,
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
    shaders: HashMap<u32, Gd<Shader>>,
    mask_shader: Option<Gd<Shader>>,
    masks: HashMap<MaskKey, MaskView>,
    mask_targets: HashMap<String, MaskKey>,
    mask_consumers: HashMap<String, String>,
    offscreens: HashMap<String, OffscreenView>,
    destination_copies: HashMap<String, Gd<BackBufferCopy>>,
    offscreen_creations: i64,
    offscreen_resizes: i64,
    offscreen_shaders: HashMap<u32, Gd<Shader>>,
    mask_scale: f64,
    surface_target_extent: Vector2,
    surface_transform: Transform2D,
    surface_viewport: Option<Rid>,
    last_result: Dictionary,
    submission_id: u64,
    submitted_frame: i32,
    completed_submission: u64,
    drawing_submission: Option<(u64, Rid)>,
    pending_draws: i32,
    runtime_frame: Option<DrawableFrame>,
    verified_assets: HashMap<String, kasane_core::ImageAsset>,
    verified_manifest: String,
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
        self.surface_viewport = None;
        for mask in self.masks.values_mut() {
            mask.viewport.set_update_mode(UpdateMode::ONCE);
        }
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
        let target = self
            .base()
            .get_viewport()
            .map(|v| v.get_visible_rect().size)
            .unwrap_or(Vector2::new(2048.0, 2048.0));
        if (scale - self.mask_scale).abs() > 0.00001
            || (!self.offscreens.is_empty()
                && (target != self.surface_target_extent
                    || transform != self.surface_transform
                    || self.base().get_viewport().map(|v| v.get_viewport_rid())
                        != self.surface_viewport))
        {
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
        self.mask_targets.clear();
        self.mask_consumers.clear();
        for (_, mut copy) in self.destination_copies.drain() {
            copy.queue_free();
        }
        for (_, mut view) in self.views.drain() {
            view.queue_free();
        }
        for (_, mut mask) in self.masks.drain() {
            mask.viewport.queue_free();
        }
        for (_, mut offscreen) in self.offscreens.drain() {
            offscreen.composite.queue_free();
            offscreen.viewport.queue_free();
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
    pub fn _document_changed(&mut self, change: Dictionary) {
        let kind = change
            .get("change_kind")
            .and_then(|v| v.try_to::<GString>().ok());
        if kind
            .as_ref()
            .is_some_and(|k| *k == "metadata" || *k == "none")
        {
            return;
        }
        if kind.as_ref().is_some_and(|k| *k == "positions") {
            self.refresh_inner(false);
            return;
        }
        let reload_assets = self.document.as_ref().is_none_or(|doc| {
            let doc = doc.bind();
            let session = doc.session();
            let source = session.document();
            self.verified_manifest != session.manifest().to_string_lossy()
                || source.asset_order().len() != self.verified_assets.len()
                || source
                    .asset_order()
                    .iter()
                    .any(|id| source.get_asset(id) != self.verified_assets.get(id))
        });
        self.refresh_inner(reload_assets);
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
        stats.set("offscreen_groups", self.offscreens.len() as i64);
        let pixels: i64 = self
            .offscreens
            .values()
            .map(|view| {
                let size = view.viewport.get_size();
                i64::from(size.x) * i64::from(size.y)
            })
            .sum();
        stats.set("offscreen_color_bytes", pixels * 4);
        // Reserve a second RGBA8 surface for destination reads. This is an
        // attachment estimate, not total driver/device memory (masks excluded).
        stats.set("offscreen_reserved_bytes", pixels * 8);
        stats.set("offscreen_budget_bytes", OFFSCREEN_BUDGET_BYTES);
        stats.set("offscreen_creations", self.offscreen_creations);
        stats.set(
            "active_offscreens",
            self.offscreens
                .values()
                .filter(|v| v.composite.is_visible())
                .count() as i64,
        );
        stats.set(
            "gpu_texture_bytes",
            RenderingServer::singleton().get_rendering_info(
                godot::classes::rendering_server::RenderingInfo::TEXTURE_MEM_USED,
            ) as i64,
        );
        stats.set(
            "gpu_video_bytes",
            RenderingServer::singleton()
                .get_rendering_info(godot::classes::rendering_server::RenderingInfo::VIDEO_MEM_USED)
                as i64,
        );
        stats.set("offscreen_resizes", self.offscreen_resizes);
        stats.set("destination_copies", self.destination_copies.len() as i64);
        let mask_pixels: i64 = self
            .masks
            .values()
            .map(|view| {
                let size = view.viewport.get_size();
                i64::from(size.x) * i64::from(size.y)
            })
            .sum();
        stats.set("mask_color_bytes", mask_pixels * 4);
        stats.set("materials", self.materials.len() as i64);
        stats.set(
            "shaders",
            self.shaders.len() as i64
                + i64::from(self.mask_shader.is_some())
                + self.offscreen_shaders.len() as i64,
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
    pub fn get_offscreen_texture(&self, offscreen_id: GString) -> Option<Gd<Texture2D>> {
        self.offscreens
            .get(&offscreen_id.to_string())
            .and_then(|view| view.viewport.get_texture())
            .map(|texture| texture.upcast())
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

        let frame = match doc.bind().evaluated_frame() {
            Ok(frame) => frame,
            Err(status) => {
                self.clear_views();
                let out = status_to_dict(&status);
                self.last_result = out.clone();
                return out;
            }
        };

        let Some(mut textures) = self.textures.clone() else {
            self.clear_views();
            let out = error_dict("MISSING_TEXTURE_STORE", "Attach a texture store.");
            self.last_result = out.clone();
            return out;
        };

        if reload_assets && !doc.bind().session().root().as_os_str().is_empty() {
            let used: std::collections::HashSet<&str> = frame
                .drawables
                .iter()
                .map(|d| d.texture_asset_id.as_str())
                .collect();
            let ids = doc.bind().session().document().asset_order().to_vec();
            let mut diagnostics = Array::new();
            for id in ids {
                let status = if used.contains(id.as_str()) {
                    textures
                        .bind_mut()
                        .resolve_asset(Some(doc.clone()), GString::from(id.as_str()))
                } else {
                    match doc.bind().session().read_asset(&id) {
                        Ok(_) => Status::ok(),
                        Err(status) => status,
                    }
                };
                if !status.is_ok() {
                    let mut item = Dictionary::new();
                    item.set("asset_id", id.as_str());
                    item.set("code", status.code.as_str());
                    item.set("message", status.message.as_str());
                    diagnostics.push(&item);
                }
            }
            if !diagnostics.is_empty() {
                self.clear_views();
                let mut out = error_dict(
                    "INCOMPLETE_RESOURCES",
                    "Project resources failed verification.",
                );
                out.set("diagnostics", &diagnostics);
                self.last_result = out.clone();
                return out;
            }
        }

        let mut resolved = HashMap::new();
        for d in &frame.drawables {
            // Shared atlas assets must be validated/decoded once per refresh, not
            // once per mesh. Explicit refresh still detects disk edits.
            if resolved.contains_key(&d.texture_asset_id) {
                continue;
            }
            let missing_texture = textures
                .bind()
                .get_texture(GString::from(d.texture_asset_id.as_str()))
                .is_none();
            if missing_texture
                && !reload_assets
                && !doc.bind().session().root().as_os_str().is_empty()
            {
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
        let result = self.render_frame(&frame, &resolved);
        if reload_assets && result.get("ok").and_then(|v| v.try_to::<bool>().ok()) == Some(true) {
            let doc = doc.bind();
            let source = doc.session().document();
            self.verified_assets = source
                .asset_order()
                .iter()
                .map(|id| (id.clone(), source.get_asset(id).unwrap().clone()))
                .collect();
            self.verified_manifest = doc.session().manifest().to_string_lossy().into_owned();
        }
        result
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
        let transform = self.base().get_global_transform_with_canvas();
        let scale = if self.base().is_inside_tree() {
            let x = transform.a.length();
            let y = transform.b.length();
            (x.max(y) as f64).max(0.0001)
        } else {
            self.mask_scale
        };
        let viewport_extent = self
            .base()
            .get_viewport()
            .map(|v| v.get_visible_rect().size)
            .unwrap_or(Vector2::new(2048.0, 2048.0));
        let plan = match resources::plan(frame, textures, transform, viewport_extent, scale) {
            Ok(plan) => plan,
            Err(status) => {
                self.pending_draws = 0;
                self.completed_submission = 0;
                self.last_result = status_to_dict(&status);
                return self.last_result.clone();
            }
        };
        let active_offscreens = plan.active;
        let surface_size = plan.size;
        let surface_transform = plan.transform;
        self.mask_scale = scale;
        self.surface_transform = transform;
        self.surface_target_extent = viewport_extent;
        self.surface_viewport = self.base().get_viewport().map(|v| v.get_viewport_rid());
        if self.model_root.is_none() {
            let root = Node2D::new_alloc();
            self.base_mut().add_child(&root);
            self.base_mut().move_child(&root, 0);
            self.model_root = Some(root);
        }

        let required_copies: std::collections::HashSet<&str> = frame
            .offscreens
            .iter()
            .filter(|o| o.blend_mode != 0)
            .map(|o| o.id.as_str())
            .chain(
                frame
                    .drawables
                    .iter()
                    .filter(|d| d.raw_blend_mode.is_some())
                    .map(|d| d.id.as_str()),
            )
            .collect();
        self.destination_copies.retain(|id, copy| {
            if required_copies.contains(id.as_str()) {
                true
            } else {
                copy.queue_free();
                false
            }
        });
        self.mask_targets.clear();
        self.mask_consumers = plan.mask_consumers;
        self.update_surfaces(frame, &active_offscreens, surface_size, surface_transform);
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

            let reuse = self.mesh_keys.get(&d.id).is_some_and(|key| {
                std::sync::Arc::ptr_eq(&key.indices, &d.indices)
                    && std::sync::Arc::ptr_eq(&key.uvs, &d.uvs)
                    && key.texture == tex
            });
            let mut view = self
                .views
                .get(&d.id)
                .cloned()
                .unwrap_or_else(KasaneMeshView::new_alloc);
            let update_status = if reuse {
                view.bind_mut().update_positions(positions)
            } else {
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
        }

        for offscreen in &frame.offscreens {
            let mask_data = self.update_mask_texture(
                &offscreen.id,
                if active_offscreens.contains(offscreen.id.as_str()) {
                    &offscreen.masks
                } else {
                    &[]
                },
                self.mask_scale.max(1.0),
            );
            let material = &mut self.offscreens.get_mut(&offscreen.id).unwrap().material;
            if let Some((texture, bounds)) = mask_data {
                material.set_shader_parameter("mask_texture", &texture);
                material.set_shader_parameter("mask_bounds", &bounds.to_variant());
            }
        }

        let mut ordered: Vec<&kasane_core::evaluation::Drawable> = frame.drawables.iter().collect();
        ordered.sort_by_key(|a| a.render_order);

        for drawable in ordered {
            let d = drawable;
            let mut view = self.views.get(&d.id).unwrap().clone();
            let blend_key = match d.raw_blend_mode {
                Some(_) => 3,
                None => match d.blend_mode {
                    BlendMode::Normal => 0,
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                },
            };
            let shader = self
                .shaders
                .entry(blend_key)
                .or_insert_with(|| {
                    let code = if blend_key == 3 {
                        include_str!("../shaders/drawable_extended.gdshader").replace(
                            "__BLEND_FUNCTIONS__",
                            include_str!("../shaders/blend_functions.gdshaderinc"),
                        )
                    } else {
                        let (blend, output) = match blend_key {
                            1 => ("blend_premul_alpha", "COLOR = vec4(c.rgb * a, 0.0);"),
                            2 => ("blend_mul", "COLOR = vec4(c.rgb * a + vec3(1.0 - a), 1.0);"),
                            _ => ("blend_premul_alpha", "COLOR = vec4(c.rgb * a, a);"),
                        };
                        include_str!("../shaders/drawable.gdshader")
                            .replace("__BLEND__", blend)
                            .replace("__OUTPUT__", output)
                    };
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
            if let Some(raw_blend_mode) = d.raw_blend_mode {
                material.set_shader_parameter(
                    "color_blend_mode",
                    &i64::from(raw_blend_mode & 0xff).to_variant(),
                );
                material.set_shader_parameter(
                    "alpha_blend_mode",
                    &i64::from((raw_blend_mode >> 8) & 0xff).to_variant(),
                );
            }
            material.set_shader_parameter("masked", &(!d.masks.is_empty()).to_variant());
            material.set_shader_parameter("inverted", &d.inverted_mask.to_variant());

            if let Some((texture, bounds)) =
                self.update_mask_texture(&d.id, &d.masks, self.mask_scale)
            {
                material.set_shader_parameter("mask_texture", &texture);
                material.set_shader_parameter("mask_bounds", &bounds.to_variant());
            }

            view.set_material(&material);
            self.materials.insert(d.id.clone(), material);
            if frame.offscreens.is_empty() {
                let mut parent = self.model_root.as_ref().unwrap().clone().upcast::<Node>();
                if required_copies.contains(d.id.as_str()) {
                    self.place_destination_copy(&d.id, &mut parent);
                }
                if view.get_parent() != Some(parent.clone()) {
                    view.reparent_ex(&parent)
                        .keep_global_transform(false)
                        .done();
                }
                parent.move_child(&view, -1);
            }
        }

        self.masks.retain(|_, mask| {
            if mask.last_submission == self.submission_id {
                true
            } else {
                mask.viewport.queue_free();
                false
            }
        });
        self.execute_render_plan(frame, &required_copies);

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

// Empty/disabled subtrees keep their reusable nodes but release large render
// targets and stop updating. A later visible frame reuses these same nodes.
