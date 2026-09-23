use godot::classes::{Engine, RenderingServer, SubViewport, Texture2D, Viewport, Window};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::Status;
use kasane_preview::{AssetResolver, LoadedTextureInfo, PreviewResources, ResourceFailure};
use kasane_project::DocumentSession;

use crate::conversions::{error_dict, status_to_dict, Array, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;
use crate::texture_store::KasaneTextureStore;

use kasane_render_godot::{GodotRenderBackend, KasaneMeshView, RenderRequest};

/// Godot-facing preview component.
///
/// The component owns the public Godot API, document/texture subscriptions and
/// frame-observation state. Rendering resources and execution live in
/// `GodotRenderBackend`, so the API layer does not depend on Godot rendering
/// objects beyond the node lifecycle and public texture handles.
#[derive(GodotClass)]
#[class(init, base=Node2D)]
pub struct KasaneDocumentPreview {
    base: Base<Node2D>,
    document: Option<Gd<KasaneDocumentBridge>>,
    textures: Option<Gd<KasaneTextureStore>>,
    backend: GodotRenderBackend,
    last_result: Dictionary,
    submission_id: u64,
    submitted_frame: i32,
    completed_submission: u64,
    drawing_submission: Option<(u64, Rid)>,
    pending_draws: i32,
    runtime_frame: Option<DrawableFrame>,
    resources: PreviewResources,
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
        self.backend.on_enter_tree();
        self.pending_draws = if self.backend.has_masks() { 2 } else { 1 };
    }

    fn exit_tree(&mut self) {
        for (signal, method) in [
            ("frame_pre_draw", "_on_frame_pre_draw"),
            ("frame_post_draw", "_on_frame_post_draw"),
        ] {
            let callback = self.to_gd().callable(method);
            RenderingServer::singleton().disconnect(signal, &callback);
        }
        self.backend.on_exit_tree();
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
        let viewport = self.base().get_viewport();
        let viewport_rid = viewport.as_ref().map(|v| v.get_viewport_rid());
        if self
            .backend
            .needs_geometry_refresh(scale, target, transform, viewport_rid)
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
        self.backend.clear_views();
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
        self.resources.reset();
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
        let reload_assets = self
            .document
            .as_ref()
            .is_none_or(|doc| self.resources.needs_reload(doc.bind().session()));
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
        self.backend.render_stats()
    }

    #[func]
    pub fn get_mesh_view(&self, mesh_id: GString) -> Option<Gd<KasaneMeshView>> {
        self.backend.get_mesh_view(&mesh_id.to_string())
    }

    #[func]
    pub fn get_offscreen_texture(&self, offscreen_id: GString) -> Option<Gd<Texture2D>> {
        self.backend
            .get_offscreen_texture(&offscreen_id.to_string())
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

        let resource_check = {
            let doc_bind = doc.bind();
            let mut resolver = GodotTextureResolver {
                textures: &mut textures,
            };
            self.resources
                .verify_frame(doc_bind.session(), &frame, &mut resolver, reload_assets)
        };
        if let Err(failure) = resource_check {
            self.clear_views();
            let out = resource_failure_to_dict(&failure);
            self.last_result = out.clone();
            return out;
        }

        let mut resolved = HashMap::new();
        for d in &frame.drawables {
            // Shared atlas assets must be validated/decoded once per refresh, not
            // once per mesh. Explicit refresh still detects disk edits.
            if resolved.contains_key(&d.texture_asset_id) {
                continue;
            }
            let texture = textures
                .bind()
                .get_texture(GString::from(d.texture_asset_id.as_str()));
            let Some(tex) = texture else {
                self.clear_views();
                let out = error_dict(
                    "MISSING_TEXTURE",
                    "Preview texture is not loaded; source edits remain valid.",
                );
                self.last_result = out.clone();
                return out;
            };
            resolved.insert(d.texture_asset_id.clone(), tex);
        }
        let result = self.render_frame(&frame, &resolved);
        if reload_assets && result.get("ok").and_then(|v| v.try_to::<bool>().ok()) == Some(true) {
            self.resources.mark_verified(doc.bind().session());
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
        self.drawing_submission = None;
        let transform = self.base().get_global_transform_with_canvas();
        let scale = if self.base().is_inside_tree() {
            let x = transform.a.length();
            let y = transform.b.length();
            (x.max(y) as f64).max(0.0001)
        } else {
            self.backend.mask_scale()
        };
        let viewport_extent = self
            .base()
            .get_viewport()
            .map(|v| v.get_visible_rect().size)
            .unwrap_or(Vector2::new(2048.0, 2048.0));
        let submission_id = self.submission_id;
        let mut owner = self.to_gd().upcast::<Node2D>();
        let outcome = self.backend.render_frame(RenderRequest {
            owner: &mut owner,
            frame,
            textures,
            transform,
            viewport_extent,
            scale,
            submission_id,
        });
        let mut result = outcome.status;
        let ok = result
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if ok {
            self.submission_id = self.submission_id.wrapping_add(1);
            result.set("submission_id", self.submission_id as i64);
            result.set("revision", frame.source_revision as i64);
            self.submitted_frame = Engine::singleton().get_frames_drawn();
            self.pending_draws = if outcome.has_masks { 2 } else { 1 };
        } else {
            self.pending_draws = 0;
            self.completed_submission = 0;
        }
        self.last_result = result.clone();
        result
    }
}

struct GodotTextureResolver<'a> {
    textures: &'a mut Gd<KasaneTextureStore>,
}

impl AssetResolver for GodotTextureResolver<'_> {
    fn texture_info(&self, asset_id: &str) -> Option<LoadedTextureInfo> {
        self.textures
            .bind()
            .get_texture(GString::from(asset_id))
            .map(|texture| LoadedTextureInfo {
                width: texture.get_width().max(0) as u32,
                height: texture.get_height().max(0) as u32,
            })
    }

    fn resolve_asset(&mut self, session: &DocumentSession, asset_id: &str) -> Status {
        self.textures
            .bind_mut()
            .resolve_asset_from_session(session, asset_id)
    }
}

fn resource_failure_to_dict(failure: &ResourceFailure) -> Dictionary {
    let mut out = status_to_dict(&failure.status);
    if !failure.diagnostics.is_empty() {
        let mut diagnostics = Array::new();
        for diagnostic in &failure.diagnostics {
            let mut item = Dictionary::new();
            item.set("asset_id", diagnostic.asset_id.as_str());
            item.set("code", diagnostic.code.as_str());
            item.set("message", diagnostic.message.as_str());
            diagnostics.push(&item);
        }
        out.set("diagnostics", &diagnostics);
    }
    out
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
