use super::*;

type MaskSourceSnapshot = (
    String,
    Option<Gd<godot::classes::Mesh>>,
    Option<Gd<Texture2D>>,
    PackedVector2Array,
);

impl GodotRenderBackend {
    pub(super) fn update_mask_texture(
        &mut self,
        owner: &mut Gd<Node2D>,
        target_id: &str,
        mask_ids: &[String],
        mask_consumers: &HashMap<String, String>,
        requested_scale: f64,
    ) -> Option<(Variant, Vector4)> {
        if mask_ids.is_empty() {
            return None;
        }

        // Share within a consumer viewport. Different offscreen consumers need
        // separate dependency edges so every mask is ready in the same frame.
        let consumer = mask_consumers
            .get(target_id)
            .map(String::as_str)
            .unwrap_or("");
        let key = MaskKey::new(mask_ids, requested_scale, consumer);
        self.mask_targets.insert(target_id.to_owned(), key.clone());
        if let Some(mask) = self.masks.get(&key) {
            if mask.last_submission == self.submission_id {
                return Some((mask.viewport.get_texture()?.to_variant(), mask.bounds));
            }
        }
        let sources: Vec<MaskSourceSnapshot> = mask_ids
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
        let scale = (requested_scale as f32).min(4096.0 / max_dim);
        let final_sx = 1.max((size_x as f32 * scale).ceil() as i32);
        let final_sy = 1.max((size_y as f32 * scale).ceil() as i32);

        if !self.masks.contains_key(&key) {
            let mut viewport = SubViewport::new_alloc();
            viewport.set_transparent_background(true);
            viewport.set_disable_3d(true);
            viewport.set_update_mode(UpdateMode::ONCE);
            owner.add_child(&viewport);
            let root = Node2D::new_alloc();
            viewport.add_child(&root);
            self.masks.insert(
                key.clone(),
                MaskView {
                    last_submission: u64::MAX,
                    bounds: Vector4::ZERO,
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
                shader.set_code(&GString::from(include_str!("../../shaders/mask.gdshader")));
                shader
            })
            .clone();
        let mask = self.masks.get_mut(&key).unwrap();
        mask.last_submission = self.submission_id;
        mask.viewport.set_update_mode(UpdateMode::ONCE);
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
        mask.bounds = Vector4::new(
            bounds.position.x,
            bounds.position.y,
            final_sx as f32 / scale,
            final_sy as f32 / scale,
        );
        Some((texture, mask.bounds))
    }
}
