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
    masks: HashMap<String, MaskView>,
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
            if resolved.contains_key(&d.texture_asset_id) {
                continue;
            }
            let missing_texture = textures
                .bind()
                .get_texture(GString::from(d.texture_asset_id.as_str()))
                .is_none();
            if (reload_assets || missing_texture)
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

    fn update_mask_texture(
        &mut self,
        target_id: &str,
        mask_ids: &[String],
    ) -> Option<(Variant, Vector4)> {
        if mask_ids.is_empty() {
            if let Some(mut mask) = self.masks.remove(target_id) {
                mask.viewport.queue_free();
            }
            return None;
        }

        let sources: Vec<(
            String,
            Option<Gd<godot::classes::Mesh>>,
            Option<Gd<Texture2D>>,
            PackedVector2Array,
        )> = mask_ids
            .iter()
            .filter_map(|id| {
                self.views.get(id).map(|view| {
                    (
                        id.clone(),
                        view.get_mesh(),
                        view.get_texture(),
                        view.bind().get_positions_snapshot(),
                    )
                })
            })
            .collect();
        let mut bounds = Rect2::default();
        let mut first = true;
        for (_, _, _, points) in &sources {
            for i in 0..points.len() {
                let point = points[i];
                if first {
                    bounds = Rect2::new(point, Vector2::ZERO);
                    first = false;
                } else {
                    bounds = bounds.expand(point);
                }
            }
        }
        bounds = bounds.grow(4.0);
        let size_x = 1.max(bounds.size.x.ceil() as i32);
        let size_y = 1.max(bounds.size.y.ceil() as i32);
        let max_dim = size_x.max(size_y) as f32;
        // Tiny fitted previews still need enough mask samples for pupil edges.
        // Preserve at least model-pixel density, within the shared mask budget.
        let scale = (self.mask_scale as f32).max(1.0).min(4096.0 / max_dim);
        let final_sx = 1.max((size_x as f32 * scale).ceil() as i32);
        let final_sy = 1.max((size_y as f32 * scale).ceil() as i32);

        if !self.masks.contains_key(target_id) {
            let mut viewport = SubViewport::new_alloc();
            viewport.set_transparent_background(true);
            viewport.set_disable_3d(true);
            viewport.set_update_mode(UpdateMode::ALWAYS);
            self.base_mut().add_child(&viewport);
            let root = Node2D::new_alloc();
            viewport.add_child(&root);
            self.masks.insert(
                target_id.to_owned(),
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
        let mask = self.masks.get_mut(target_id).unwrap();
        mask.viewport.set_size(Vector2i::new(final_sx, final_sy));
        mask.root.set_scale(Vector2::new(scale, scale));
        mask.root.set_position(-bounds.position * scale);
        let stale: Vec<String> = mask
            .sources
            .keys()
            .filter(|id| !mask_ids.contains(id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(mut mesh) = mask.sources.remove(&id) {
                mesh.queue_free();
            }
            mask.materials.remove(&id);
        }
        for (id, source_mesh, source_texture, _) in sources {
            if !mask.sources.contains_key(&id) {
                let mesh = MeshInstance2D::new_alloc();
                mask.root.add_child(&mesh);
                mask.sources.insert(id.clone(), mesh);
            }
            let mesh = mask.sources.get_mut(&id).unwrap();
            mesh.set_mesh(source_mesh.as_ref());
            let material = mask.materials.entry(id).or_insert_with(|| {
                let mut material = ShaderMaterial::new_gd();
                material.set_shader(&mask_shader);
                material
            });
            if let Some(source_texture) = source_texture {
                material.set_shader_parameter("main_texture", &source_texture.to_variant());
            }
            mesh.set_material(&*material);
        }
        let texture = mask.viewport.get_texture()?.to_variant();
        Some((
            texture,
            Vector4::new(
                bounds.position.x,
                bounds.position.y,
                final_sx as f32 / scale,
                final_sy as f32 / scale,
            ),
        ))
    }

    // Godot's automatic screen copy is only taken for the first screen-reading
    // item in a canvas. Every subsequent extended blend needs the destination
    // produced by all preceding commands, including normal composites.
    fn place_destination_copy(&mut self, id: &str, parent: &mut Gd<Node>) {
        let copy = self
            .destination_copies
            .entry(id.to_owned())
            .or_insert_with(|| {
                let mut copy = BackBufferCopy::new_alloc();
                copy.set_copy_mode(CopyMode::VIEWPORT);
                parent.add_child(&copy);
                copy
            });
        if copy.get_parent() != Some(parent.clone()) {
            copy.reparent_ex(&*parent)
                .keep_global_transform(false)
                .done();
        }
        parent.move_child(&*copy, -1);
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
        if self.base().is_inside_tree() {
            let transform = self.base().get_global_transform_with_canvas();
            let x_len = (transform.a.x * transform.a.x + transform.a.y * transform.a.y).sqrt();
            let y_len = (transform.b.x * transform.b.x + transform.b.y * transform.b.y).sqrt();
            self.mask_scale = (x_len.max(y_len) as f64).max(0.0001);
        }
        let viewport_extent = self
            .base()
            .get_viewport()
            .map(|v| v.get_visible_rect().size)
            .unwrap_or(Vector2::new(2048.0, 2048.0));
        self.surface_target_extent = viewport_extent;
        self.surface_viewport = self.base().get_viewport().map(|v| v.get_viewport_rid());
        self.surface_transform = self.base().get_global_transform_with_canvas();
        let active_offscreens = active_offscreens(frame);
        let (surface_size, surface_transform) = match offscreen_layout(
            Vector2::new(frame.canvas.width, frame.canvas.height),
            self.surface_transform,
            viewport_extent,
            active_offscreens.len(),
        ) {
            Ok(size) => size,
            Err(status) => {
                self.pending_draws = 0;
                self.completed_submission = 0;
                self.last_result = status_to_dict(&status);
                return self.last_result.clone();
            }
        };
        let surface_bytes = i64::from(surface_size.x)
            * i64::from(surface_size.y)
            * 8
            * active_offscreens.len() as i64;
        let mask_bytes: i64 = frame
            .drawables
            .iter()
            .map(|d| mask_reserved_bytes(frame, &d.masks, self.mask_scale))
            .chain(
                frame
                    .offscreens
                    .iter()
                    .filter(|o| active_offscreens.contains(o.id.as_str()))
                    .map(|o| mask_reserved_bytes(frame, &o.masks, self.mask_scale.max(1.0))),
            )
            .sum();
        if surface_bytes + mask_bytes > OFFSCREEN_BUDGET_BYTES {
            self.pending_draws = 0;
            self.completed_submission = 0;
            self.last_result = error_dict(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "Offscreen and mask attachments exceed the shared 512 MiB budget.",
            );
            return self.last_result.clone();
        }
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
        let stale_offscreens: Vec<String> = self
            .offscreens
            .keys()
            .filter(|id| {
                !frame
                    .offscreens
                    .iter()
                    .any(|offscreen| &offscreen.id == *id)
            })
            .cloned()
            .collect();
        for id in stale_offscreens {
            if let Some(mut view) = self.offscreens.remove(&id) {
                // Surviving meshes/composites may still be children of this
                // removed surface. Detach before deferred deletion owns them.
                let parent = self.model_root.as_ref().unwrap().clone().upcast::<Node>();
                for mut child in view.root.get_children().iter_shared() {
                    child
                        .reparent_ex(&parent)
                        .keep_global_transform(false)
                        .done();
                }
                view.composite.queue_free();
                view.viewport.queue_free();
            }
            if let Some(mut mask) = self.masks.remove(&id) {
                mask.viewport.queue_free();
            }
        }
        if !frame.offscreens.is_empty() {
            let texture_to_model = surface_transform.affine_inverse();
            for offscreen in &frame.offscreens {
                let active = active_offscreens.contains(offscreen.id.as_str());
                let allocation_size = if active {
                    surface_size
                } else {
                    Vector2i::new(2, 2)
                };
                let shader = self
                    .offscreen_shaders
                    .entry(offscreen.blend_mode)
                    .or_insert_with(|| {
                        let code = if offscreen.blend_mode == 0 {
                            include_str!("../shaders/offscreen_normal.gdshader").to_owned()
                        } else {
                            include_str!("../shaders/offscreen.gdshader").replace(
                                "__BLEND_FUNCTIONS__",
                                include_str!("../shaders/blend_functions.gdshaderinc"),
                            )
                        };
                        let mut shader = Shader::new_gd();
                        shader.set_code(&GString::from(code.as_str()));
                        shader
                    })
                    .clone();
                if !self.offscreens.contains_key(&offscreen.id) {
                    let mut viewport = SubViewport::new_alloc();
                    viewport.set_size(allocation_size);
                    viewport.set_transparent_background(true);
                    viewport.set_disable_3d(true);
                    viewport.set_update_mode(UpdateMode::ALWAYS);
                    self.base_mut().add_child(&viewport);
                    let root = Node2D::new_alloc();
                    viewport.add_child(&root);
                    let mut composite = Sprite2D::new_alloc();
                    composite.set_centered(false);
                    composite.set_region_enabled(true);
                    composite.set_region_rect(Rect2::new(
                        Vector2::ZERO,
                        Vector2::new(surface_size.x as f32, surface_size.y as f32),
                    ));
                    if let Some(texture) = viewport.get_texture() {
                        composite.set_texture(&texture);
                    }
                    let mut material = ShaderMaterial::new_gd();
                    material.set_shader(&shader);
                    if let Some(texture) = viewport.get_texture() {
                        material.set_shader_parameter("main_texture", &texture.to_variant());
                    }
                    composite.set_material(&material);
                    self.model_root.as_mut().unwrap().add_child(&composite);
                    self.offscreen_creations += 1;
                    self.offscreens.insert(
                        offscreen.id.clone(),
                        OffscreenView {
                            viewport,
                            root,
                            composite,
                            material,
                        },
                    );
                }
                let view = self.offscreens.get_mut(&offscreen.id).unwrap();
                if view.material.get_shader() != Some(shader.clone()) {
                    view.material.set_shader(&shader);
                }
                if view.viewport.get_size() != allocation_size {
                    view.viewport.set_size(allocation_size);
                    self.offscreen_resizes += 1;
                }
                view.viewport.set_update_mode(if active {
                    UpdateMode::ALWAYS
                } else {
                    UpdateMode::DISABLED
                });
                // Shader mat3 uniforms require Basis, not Transform2D (mat2).
                // Passing Transform2D silently loses the affine mapping; masks
                // then work at identity scale but disappear in fitted previews.
                let mask_mapping = Basis::from_cols(
                    Vector3::new(texture_to_model.a.x, texture_to_model.a.y, 0.0),
                    Vector3::new(texture_to_model.b.x, texture_to_model.b.y, 0.0),
                    Vector3::new(texture_to_model.origin.x, texture_to_model.origin.y, 1.0),
                );
                view.material
                    .set_shader_parameter("texture_to_model", &mask_mapping.to_variant());
                view.root.set_transform(surface_transform);
                view.composite.set_transform(texture_to_model);
                view.composite.set_region_rect(Rect2::new(
                    Vector2::ZERO,
                    Vector2::new(surface_size.x as f32, surface_size.y as f32),
                ));
                view.composite.set_visible(active);
                view.material.set_shader_parameter(
                    "color_blend_mode",
                    &i64::from(offscreen.blend_mode & 0xff).to_variant(),
                );
                view.material.set_shader_parameter(
                    "alpha_blend_mode",
                    &i64::from((offscreen.blend_mode >> 8) & 0xff).to_variant(),
                );
                view.material.set_shader_parameter(
                    "multiply_color",
                    &Vector3::new(
                        offscreen.multiply_color[0],
                        offscreen.multiply_color[1],
                        offscreen.multiply_color[2],
                    )
                    .to_variant(),
                );
                view.material.set_shader_parameter(
                    "screen_color",
                    &Vector3::new(
                        offscreen.screen_color[0],
                        offscreen.screen_color[1],
                        offscreen.screen_color[2],
                    )
                    .to_variant(),
                );
                view.material
                    .set_shader_parameter("opacity", &offscreen.opacity.to_variant());
                view.material
                    .set_shader_parameter("enabled", &offscreen.enabled.to_variant());
                view.material
                    .set_shader_parameter("masked", &(!offscreen.masks.is_empty()).to_variant());
                view.material
                    .set_shader_parameter("inverted", &(offscreen.flags & 8 != 0).to_variant());
            }
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

        for offscreen in &frame.offscreens {
            let mask_data = self.update_mask_texture(
                &offscreen.id,
                if active_offscreens.contains(offscreen.id.as_str()) {
                    &offscreen.masks
                } else {
                    &[]
                },
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
                        mesh.set_mesh(source.get_mesh().as_ref());
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

        if !frame.offscreens.is_empty() {
            let mut stack: Vec<String> = Vec::new();
            for command in &frame.render_plan {
                match command {
                    RenderCommand::BeginOffscreen { offscreen_id } => {
                        // Scene sibling order does not describe render-target
                        // dependencies. Register the actual consumer viewport so
                        // children finish before parents in this same frame.
                        let parent_rid = stack
                            .last()
                            .map(|id| self.offscreens[id].viewport.get_viewport_rid())
                            .unwrap_or_else(|| {
                                self.base()
                                    .get_viewport()
                                    .map(|v| v.get_viewport_rid())
                                    .unwrap_or(Rid::Invalid)
                            });
                        RenderingServer::singleton().viewport_set_parent_viewport(
                            self.offscreens[offscreen_id].viewport.get_viewport_rid(),
                            parent_rid,
                        );
                        if let Some(mask) = self.masks.get(offscreen_id) {
                            RenderingServer::singleton().viewport_set_parent_viewport(
                                mask.viewport.get_viewport_rid(),
                                parent_rid,
                            );
                        }
                        let mut composite = self.offscreens[offscreen_id].composite.clone();
                        let texture = self.offscreens[offscreen_id].viewport.get_texture();
                        let mut parent: Gd<Node> = if let Some(parent_id) = stack.last() {
                            self.offscreens[parent_id].root.clone().upcast()
                        } else {
                            self.model_root.as_ref().unwrap().clone().upcast()
                        };
                        if composite.get_parent() != Some(parent.clone()) {
                            composite
                                .reparent_ex(&parent)
                                .keep_global_transform(false)
                                .done();
                        }
                        if let Some(texture) = texture {
                            composite.set_texture(&texture);
                        }
                        if required_copies.contains(offscreen_id.as_str()) {
                            self.place_destination_copy(offscreen_id, &mut parent);
                        }
                        parent.move_child(&composite, -1);
                        stack.push(offscreen_id.clone());
                    }
                    RenderCommand::DrawMesh { mesh_id } => {
                        if let Some(mask) = self.masks.get(mesh_id) {
                            let parent_rid = stack
                                .last()
                                .map(|id| self.offscreens[id].viewport.get_viewport_rid())
                                .unwrap_or_else(|| {
                                    self.base()
                                        .get_viewport()
                                        .map(|v| v.get_viewport_rid())
                                        .unwrap_or(Rid::Invalid)
                                });
                            RenderingServer::singleton().viewport_set_parent_viewport(
                                mask.viewport.get_viewport_rid(),
                                parent_rid,
                            );
                        }
                        let mut view = self.views[mesh_id].clone();
                        let mut parent: Gd<Node> = if let Some(parent_id) = stack.last() {
                            self.offscreens[parent_id].root.clone().upcast()
                        } else {
                            self.model_root.as_ref().unwrap().clone().upcast()
                        };
                        if view.get_parent() != Some(parent.clone()) {
                            view.reparent_ex(&parent)
                                .keep_global_transform(false)
                                .done();
                        }
                        if required_copies.contains(mesh_id.as_str()) {
                            self.place_destination_copy(mesh_id, &mut parent);
                        }
                        parent.move_child(&view, -1);
                    }
                    RenderCommand::EndOffscreen { offscreen_id } => {
                        debug_assert_eq!(stack.last(), Some(offscreen_id));
                        stack.pop();
                    }
                }
            }
            let model_root = self.model_root.as_ref().unwrap().clone();
            self.base_mut().move_child(&model_root, -1);
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

// Empty/disabled subtrees keep their reusable nodes but release large render
// targets and stop updating. A later visible frame reuses these same nodes.
fn active_offscreens(frame: &DrawableFrame) -> std::collections::HashSet<&str> {
    let groups: HashMap<&str, _> = frame
        .offscreens
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();
    let meshes: HashMap<&str, _> = frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut stack = Vec::new();
    let mut active = std::collections::HashSet::new();
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => stack.push(offscreen_id.as_str()),
            RenderCommand::EndOffscreen { .. } => {
                stack.pop();
            }
            RenderCommand::DrawMesh { mesh_id } => {
                let d = meshes[mesh_id.as_str()];
                if d.visible
                    && d.opacity > 0.0
                    && !d.indices.is_empty()
                    && stack
                        .iter()
                        .all(|id| groups[id].enabled && groups[id].opacity > 0.0)
                {
                    active.extend(stack.iter().copied());
                }
            }
        }
    }
    active
}

fn mask_reserved_bytes(frame: &DrawableFrame, masks: &[String], scale: f64) -> i64 {
    if masks.is_empty() {
        return 0;
    }
    let mut bounds: Option<Rect2> = None;
    for id in masks {
        if let Some(mesh) = frame.drawables.iter().find(|d| &d.id == id) {
            for p in &mesh.positions {
                let point = Vector2::new(
                    p.x * frame.canvas.pixels_per_unit + frame.canvas.origin.x,
                    frame.canvas.origin.y - p.y * frame.canvas.pixels_per_unit,
                );
                bounds = Some(
                    bounds
                        .map(|b| b.expand(point))
                        .unwrap_or(Rect2::new(point, Vector2::ZERO)),
                );
            }
        }
    }
    let bounds = bounds.unwrap_or_default().grow(4.0);
    let w = bounds.size.x.ceil().max(1.0) as f64;
    let h = bounds.size.y.ceil().max(1.0) as f64;
    let scale = scale.min(4096.0 / w.max(h));
    // Same dimensions as both mask allocation paths, including Godot's minimum.
    (w * scale).ceil().max(2.0) as i64 * (h * scale).ceil().max(2.0) as i64 * 4
}

// Keep surfaces at preview resolution, bounded by the target viewport. Budget
// validation happens before any GPU allocation; never silently drop a group.
const OFFSCREEN_BUDGET_BYTES: i64 = 512 * 1024 * 1024;
fn offscreen_layout(
    canvas: Vector2,
    transform: Transform2D,
    target: Vector2,
    count: usize,
) -> Result<(Vector2i, Transform2D), Status> {
    if count == 0 {
        return Ok((Vector2i::new(2, 2), Transform2D::IDENTITY));
    }
    if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    // Crop to the visible canvas and snap BOTH edges to physical pixels. All
    // nested surfaces share this grid, so compositing never resamples a model
    // at fractional offsets (which otherwise blurs small Editor previews).
    let bounds = (transform * Rect2::new(Vector2::ZERO, canvas))
        .intersect(Rect2::new(Vector2::ZERO, target))
        .unwrap_or_default();
    let origin = bounds.position.floor();
    let extent = bounds.end().ceil() - origin;
    let size = Vector2i::new((extent.x as i32).max(2), (extent.y as i32).max(2));
    let bytes = i64::from(size.x) * i64::from(size.y) * 8;
    if size.x > 4096 || size.y > 4096 || count > (OFFSCREEN_BUDGET_BYTES / bytes) as usize {
        return Err(Status::error(
            "OFFSCREEN_BUDGET_EXCEEDED",
            format!(
                "{count} surfaces at {}x{} exceed the {} byte color/destination budget",
                size.x, size.y, OFFSCREEN_BUDGET_BYTES
            ),
        ));
    }
    let mut root = transform;
    root.origin -= origin;
    Ok((size, root))
}

#[cfg(test)]
mod surface_budget_tests {
    use super::*;
    #[test]
    fn fractional_translation_preserves_the_screen_pixel_grid() {
        let transform = Transform2D::IDENTITY.scaled(Vector2::splat(0.25));
        let mut transform = transform;
        transform.origin = Vector2::new(20.25, 30.75);
        let (size, root) = offscreen_layout(
            Vector2::new(5200.0, 7000.0),
            transform,
            Vector2::splat(2048.0),
            24,
        )
        .unwrap();
        assert_eq!(size, Vector2i::new(1301, 1751));
        assert_eq!(root.origin, Vector2::new(0.25, 0.75));
        let placement = transform * root.affine_inverse();
        assert_eq!(placement.origin, Vector2::new(20.0, 30.0));
        assert_eq!(placement.a, Vector2::RIGHT);
    }
    #[test]
    fn zoom_crops_to_target_and_excess_surfaces_fail_before_allocation() {
        let (size, _) = offscreen_layout(
            Vector2::new(5200.0, 7000.0),
            Transform2D::IDENTITY.scaled(Vector2::splat(100.0)),
            Vector2::new(1024.0, 768.0),
            24,
        )
        .unwrap();
        assert_eq!(size, Vector2i::new(1024, 768));
        assert!(offscreen_layout(
            Vector2::splat(4096.0),
            Transform2D::IDENTITY,
            Vector2::splat(4096.0),
            5
        )
        .is_err());
    }
}
