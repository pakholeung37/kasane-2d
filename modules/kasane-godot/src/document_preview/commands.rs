use super::*;

impl KasaneDocumentPreview {
    // Godot's automatic screen copy is only taken for the first screen-reading
    // item in a canvas. Every subsequent extended blend needs the destination
    // produced by all preceding commands, including normal composites.
    pub(super) fn place_destination_copy(&mut self, id: &str, parent: &mut Gd<Node>) {
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

    pub(super) fn execute_render_plan(
        &mut self,
        frame: &DrawableFrame,
        required_copies: &std::collections::HashSet<&str>,
    ) {
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
    }
}
