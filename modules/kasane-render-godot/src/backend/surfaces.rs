use super::*;

impl GodotRenderBackend {
    pub(super) fn update_surfaces(
        &mut self,
        owner: &mut Gd<Node2D>,
        frame: &DrawableFrame,
        active_offscreens: &std::collections::HashSet<&str>,
        surface_size: Vector2i,
        surface_transform: Transform2D,
    ) {
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
    }
}
