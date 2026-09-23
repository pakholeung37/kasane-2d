use super::resources::{MaskInstanceKey, ViewLayout};
use super::*;
use kasane_render::ScenePlan;

impl GodotRenderBackend {
    /// Sync raw source bindings only on model submissions. View-only updates
    /// reuse the meshes already attached to surviving mask viewports.
    pub(super) fn update_masks(
        &mut self,
        owner: &mut Gd<Node2D>,
        scene: &ScenePlan,
        layout: &ViewLayout,
        content_changed: bool,
    ) {
        if layout.masks.is_empty() {
            for (_, mut mask) in self.masks.drain() {
                mask.viewport.queue_free();
            }
            return;
        }
        let shader = self
            .mask_shader
            .get_or_insert_with(|| {
                let mut shader = Shader::new_gd();
                shader.set_code(&GString::from(include_str!("../../shaders/mask.gdshader")));
                shader
            })
            .clone();
        for (&key, allocation) in &layout.masks {
            let created = !self.masks.contains_key(&key);
            if created {
                let mut viewport = SubViewport::new_alloc();
                viewport.set_transparent_background(true);
                viewport.set_disable_3d(true);
                owner.add_child(&viewport);
                let root = Node2D::new_alloc();
                viewport.add_child(&root);
                self.masks.insert(
                    key,
                    MaskView {
                        bounds: Vector4::ZERO,
                        viewport,
                        root,
                        sources: HashMap::new(),
                        materials: HashMap::new(),
                    },
                );
                self.mask_creations += 1;
            }
            let mask = self.masks.get_mut(&key).unwrap();
            let changed = mask.viewport.get_size() != allocation.size
                || mask.bounds != allocation.bounds
                || mask.root.get_scale() != Vector2::new(allocation.scale, allocation.scale);
            if created || content_changed || changed {
                mask.viewport.set_update_mode(UpdateMode::ONCE);
            }
            if mask.viewport.get_size() != allocation.size {
                mask.viewport.set_size(allocation.size);
            }
            mask.root
                .set_scale(Vector2::new(allocation.scale, allocation.scale));
            mask.root.set_position(
                -Vector2::new(allocation.bounds.x, allocation.bounds.y) * allocation.scale,
            );
            mask.bounds = allocation.bounds;
            if created || content_changed {
                let source_ids = &scene.masks()[key.mask.0].sources;
                mask.sources.retain(|id, mesh| {
                    if source_ids
                        .iter()
                        .any(|source| scene.meshes()[source.0].id == *id)
                    {
                        true
                    } else {
                        mesh.queue_free();
                        mask.materials.remove(id);
                        false
                    }
                });
                for source in source_ids {
                    let id = &scene.meshes()[source.0].id;
                    let view = &self.views[id];
                    let mesh = mask.sources.entry(id.clone()).or_insert_with(|| {
                        let mesh = MeshInstance2D::new_alloc();
                        mask.root.add_child(&mesh);
                        mesh
                    });
                    mesh.set_mesh(view.get_mesh().as_ref());
                    let material = mask.materials.entry(id.clone()).or_insert_with(|| {
                        let mut material = ShaderMaterial::new_gd();
                        material.set_shader(&shader);
                        material
                    });
                    if let Some(texture) = view.get_texture() {
                        material.set_shader_parameter("main_texture", &texture.to_variant());
                    }
                    mesh.set_material(&*material);
                }
            }
        }
        self.masks.retain(|key, mask| {
            if layout.masks.contains_key(key) {
                true
            } else {
                mask.viewport.queue_free();
                false
            }
        });
    }

    pub(super) fn mask_binding(&self, key: Option<MaskInstanceKey>) -> Option<(Variant, Vector4)> {
        let mask = self.masks.get(&key?)?;
        Some((mask.viewport.get_texture()?.to_variant(), mask.bounds))
    }

    pub(super) fn bind_masks(&mut self, scene: &ScenePlan, layout: &ViewLayout) {
        for (mesh, key) in scene.meshes().iter().zip(&layout.mesh_masks) {
            if let Some((texture, bounds)) = self.mask_binding(*key) {
                let material = self.materials.get_mut(&mesh.id).unwrap();
                material.set_shader_parameter("mask_texture", &texture);
                material.set_shader_parameter("mask_bounds", &bounds.to_variant());
            }
        }
        for (target, key) in scene.targets().iter().zip(&layout.target_masks).skip(1) {
            if let Some((texture, bounds)) = self.mask_binding(*key) {
                let material = &mut self.offscreens.get_mut(&target.id).unwrap().material;
                material.set_shader_parameter("mask_texture", &texture);
                material.set_shader_parameter("mask_bounds", &bounds.to_variant());
            }
        }
    }
}
