mod commands;
mod masks;
mod resources;
mod surfaces;

use godot::classes::{
    back_buffer_copy::CopyMode, canvas_item::TextureFilter, sub_viewport::UpdateMode,
    BackBufferCopy, MeshInstance2D, Node2D, RenderingServer, Shader, ShaderMaterial, Sprite2D,
    SubViewport, Texture2D,
};
use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::evaluation::DrawableFrame;
use kasane_core::types::{BlendMode, Status, Vec2};
use kasane_render::{MaskKey, OFFSCREEN_BUDGET_BYTES};

use crate::conversions::{error_dict, status_to_dict, Dictionary};
use crate::mesh_view::KasaneMeshView;

struct MeshKey {
    uvs: std::sync::Arc<[Vec2]>,
    indices: std::sync::Arc<[u32]>,
    texture: Gd<Texture2D>,
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

/// Concrete Godot renderer for the backend-neutral frame plan.
///
/// `KasaneDocumentPreview` owns the Godot-facing API and lifecycle. This
/// object owns every Godot rendering resource and is the only place that
/// translates `PreparedFrame` into scene-tree and RenderingServer operations.
#[derive(Default)]
pub struct GodotRenderBackend {
    views: HashMap<String, Gd<KasaneMeshView>>,
    model_root: Option<Gd<Node2D>>,
    mesh_keys: HashMap<String, MeshKey>,
    materials: HashMap<String, Gd<ShaderMaterial>>,
    shaders: HashMap<u32, Gd<Shader>>,
    mask_shader: Option<Gd<Shader>>,
    masks: HashMap<MaskKey, MaskView>,
    mask_targets: HashMap<String, MaskKey>,
    offscreens: HashMap<String, OffscreenView>,
    destination_copies: HashMap<String, Gd<BackBufferCopy>>,
    offscreen_creations: i64,
    offscreen_resizes: i64,
    offscreen_shaders: HashMap<u32, Gd<Shader>>,
    mask_scale: f64,
    surface_target_extent: Vector2,
    surface_transform: Transform2D,
    surface_viewport: Option<Rid>,
    submission_id: u64,
}

/// Inputs crossing the Godot preview/backend boundary for one submission.
///
/// The owner is borrowed only for the duration of the submission because the
/// backend may create/reparent scene-tree resources while executing passes.
pub struct RenderRequest<'a> {
    pub owner: &'a mut Gd<Node2D>,
    pub frame: &'a DrawableFrame,
    pub textures: &'a HashMap<String, Gd<Texture2D>>,
    pub transform: Transform2D,
    pub viewport_extent: Vector2,
    pub scale: f64,
    pub submission_id: u64,
}

/// Backend output consumed by the preview's observation state machine.
pub struct BackendRenderResult {
    pub status: Dictionary,
    pub has_masks: bool,
}

impl BackendRenderResult {
    fn rejected(status: Dictionary) -> Self {
        Self {
            status,
            has_masks: false,
        }
    }

    fn submitted(status: Dictionary, has_masks: bool) -> Self {
        Self { status, has_masks }
    }
}

impl GodotRenderBackend {
    pub fn on_enter_tree(&mut self) {
        self.surface_viewport = None;
        for mask in self.masks.values_mut() {
            mask.viewport.set_update_mode(UpdateMode::ONCE);
        }
    }

    pub fn on_exit_tree(&mut self) {
        self.surface_viewport = None;
    }

    pub(super) fn has_offscreens(&self) -> bool {
        !self.offscreens.is_empty()
    }

    pub fn has_masks(&self) -> bool {
        !self.masks.is_empty()
    }

    pub fn mask_scale(&self) -> f64 {
        self.mask_scale
    }

    pub fn needs_geometry_refresh(
        &self,
        scale: f64,
        target: Vector2,
        transform: Transform2D,
        viewport: Option<Rid>,
    ) -> bool {
        (scale - self.mask_scale).abs() > 0.00001
            || (self.has_offscreens()
                && (target != self.surface_target_extent
                    || transform != self.surface_transform
                    || viewport != self.surface_viewport))
    }

    pub fn clear_views(&mut self) {
        self.mask_targets.clear();
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

    pub fn get_mesh_view(&self, mesh_id: &str) -> Option<Gd<KasaneMeshView>> {
        self.views.get(mesh_id).cloned()
    }

    pub fn get_offscreen_texture(&self, offscreen_id: &str) -> Option<Gd<Texture2D>> {
        self.offscreens
            .get(offscreen_id)
            .and_then(|view| view.viewport.get_texture())
            .map(|texture| texture.upcast())
    }

    pub fn render_stats(&self) -> Dictionary {
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

    pub fn render_frame(&mut self, request: RenderRequest<'_>) -> BackendRenderResult {
        let RenderRequest {
            owner,
            frame,
            textures,
            transform,
            viewport_extent,
            scale,
            submission_id,
        } = request;
        let plan = match resources::plan(frame, textures, transform, viewport_extent, scale) {
            Ok(plan) => plan,
            Err(status) => return BackendRenderResult::rejected(status_to_dict(&status)),
        };
        let active_offscreens = &plan.active_offscreens;
        let surface_size = Vector2i::new(plan.surface_size.width, plan.surface_size.height);
        let surface_transform = resources::to_godot_transform(plan.surface_transform);
        self.mask_scale = scale;
        self.surface_transform = transform;
        self.surface_target_extent = viewport_extent;
        self.surface_viewport = owner.get_viewport().map(|v| v.get_viewport_rid());
        self.submission_id = submission_id;
        self.ensure_model_root(owner);

        self.destination_copies.retain(|id, copy| {
            if plan.destination_reads.contains(id.as_str()) {
                true
            } else {
                copy.queue_free();
                false
            }
        });
        self.mask_targets.clear();
        self.update_surfaces(
            owner,
            frame,
            active_offscreens,
            surface_size,
            surface_transform,
        );
        for d in &frame.drawables {
            let Some(tex) = textures.get(&d.texture_asset_id).cloned() else {
                self.clear_views();
                return BackendRenderResult::rejected(error_dict(
                    "MISSING_TEXTURE",
                    &d.texture_asset_id,
                ));
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
                return BackendRenderResult::rejected(update_status);
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
                owner,
                &offscreen.id,
                if active_offscreens.contains(offscreen.id.as_str()) {
                    &offscreen.masks
                } else {
                    &[]
                },
                &plan.mask_consumers,
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

            if let Some((texture, bounds)) = self.update_mask_texture(
                owner,
                &d.id,
                &d.masks,
                &plan.mask_consumers,
                self.mask_scale,
            ) {
                material.set_shader_parameter("mask_texture", &texture);
                material.set_shader_parameter("mask_bounds", &bounds.to_variant());
            }

            view.set_material(&material);
            self.materials.insert(d.id.clone(), material);
        }

        self.masks.retain(|_, mask| {
            if mask.last_submission == self.submission_id {
                true
            } else {
                mask.viewport.queue_free();
                false
            }
        });
        self.execute_prepared_frame(owner, &plan);

        BackendRenderResult::submitted(
            status_to_dict(&kasane_core::types::Status::ok()),
            self.has_masks(),
        )
    }

    fn ensure_model_root(&mut self, owner: &mut Gd<Node2D>) {
        if self.model_root.is_none() {
            let root = Node2D::new_alloc();
            owner.add_child(&root);
            owner.move_child(&root, 0);
            self.model_root = Some(root);
        }
    }
}
