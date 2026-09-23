use super::*;

impl GodotRenderBackend {
    pub(super) fn update_surfaces(
        &mut self,
        owner: &mut Gd<Node2D>,
        frame: &DrawableFrame,
        scene: &kasane_render::ScenePlan,
        surface_size: Vector2i,
    ) {
        let stale_offscreens: Vec<String> = self
            .offscreens
            .keys()
            .filter(|id| scene.target_id(id).is_none())
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
        }
        if !frame.offscreens.is_empty() {
            for offscreen in &frame.offscreens {
                let active = scene.targets()[scene.target_id(&offscreen.id).unwrap().0].active;
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
                            include_str!("../../shaders/offscreen_normal.gdshader").to_owned()
                        } else {
                            include_str!("../../shaders/offscreen.gdshader").replace(
                                "__BLEND_FUNCTIONS__",
                                include_str!("../../shaders/blend_functions.gdshaderinc"),
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
                    owner.add_child(&viewport);
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
    }
    pub(super) fn update_surface_layout(
        &mut self,
        scene: &kasane_render::ScenePlan,
        layout: &super::resources::ViewLayout,
    ) {
        let inverse = layout.surface_transform.affine_inverse();
        let mapping = Basis::from_cols(
            Vector3::new(inverse.a.x, inverse.a.y, 0.0),
            Vector3::new(inverse.b.x, inverse.b.y, 0.0),
            Vector3::new(inverse.origin.x, inverse.origin.y, 1.0),
        );
        for target in scene.targets().iter().skip(1) {
            let view = self.offscreens.get_mut(&target.id).unwrap();
            let size = if target.active {
                layout.surface_size
            } else {
                Vector2i::new(2, 2)
            };
            if view.viewport.get_size() != size {
                view.viewport.set_size(size);
                self.offscreen_resizes += 1;
            }
            view.viewport.set_update_mode(if target.active {
                UpdateMode::ALWAYS
            } else {
                UpdateMode::DISABLED
            });
            view.material
                .set_shader_parameter("texture_to_model", &mapping.to_variant());
            view.root.set_transform(layout.surface_transform);
            view.composite.set_transform(inverse);
            view.composite.set_region_rect(Rect2::new(
                Vector2::ZERO,
                Vector2::new(layout.surface_size.x as f32, layout.surface_size.y as f32),
            ));
            view.composite.set_visible(target.active);
        }
    }
}
