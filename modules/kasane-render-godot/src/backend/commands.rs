use super::*;
use kasane_render::{PreparedFrame, RenderPass};

impl GodotRenderBackend {
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

    pub(super) fn execute_prepared_frame(
        &mut self,
        owner: &mut Gd<Node2D>,
        prepared: &PreparedFrame<'_>,
    ) {
        let mut stack: Vec<&str> = Vec::new();
        let mut has_offscreens = false;
        for pass in &prepared.passes {
            match pass {
                RenderPass::Main => {
                    debug_assert!(stack.is_empty());
                }
                RenderPass::Offscreen { id, parent } => {
                    let id = *id;
                    let parent = *parent;
                    has_offscreens = true;
                    debug_assert_eq!(stack.last().copied(), parent);
                    if let Some(view) = self.offscreens.get(id) {
                        // Scene sibling order does not describe render-target
                        // dependencies. Register the actual consumer viewport so
                        // children finish before parents in this same frame.
                        RenderingServer::singleton().viewport_set_parent_viewport(
                            view.viewport.get_viewport_rid(),
                            self.parent_viewport_rid(parent, owner),
                        );
                    }
                    stack.push(id);
                }
                RenderPass::Mask { target, consumer } => {
                    if let Some(mask) = self
                        .mask_targets
                        .get(*target)
                        .and_then(|key| self.masks.get(key))
                    {
                        RenderingServer::singleton().viewport_set_parent_viewport(
                            mask.viewport.get_viewport_rid(),
                            self.parent_viewport_rid(*consumer, owner),
                        );
                    }
                }
                RenderPass::Composite { id, parent } => {
                    let id = *id;
                    let parent_id = *parent;
                    let Some(view) = self.offscreens.get(id) else {
                        continue;
                    };
                    let mut composite = view.composite.clone();
                    let texture = view.viewport.get_texture();
                    let mut parent = self.parent_node(parent_id);
                    if composite.get_parent() != Some(parent.clone()) {
                        composite
                            .reparent_ex(&parent)
                            .keep_global_transform(false)
                            .done();
                    }
                    if let Some(texture) = texture {
                        composite.set_texture(&texture);
                    }
                    if prepared.destination_reads.contains(id) {
                        self.place_destination_copy(id, &mut parent);
                    }
                    parent.move_child(&composite, -1);
                }
                RenderPass::Draw(item) => {
                    let mut view = self.views[item.drawable_id].clone();
                    let mut parent = self.parent_node(stack.last().copied());
                    if view.get_parent() != Some(parent.clone()) {
                        view.reparent_ex(&parent)
                            .keep_global_transform(false)
                            .done();
                    }
                    if prepared.destination_reads.contains(item.drawable_id) {
                        self.place_destination_copy(item.drawable_id, &mut parent);
                    }
                    parent.move_child(&view, -1);
                }
                RenderPass::EndOffscreen { id } => {
                    debug_assert_eq!(stack.last().copied(), Some(*id));
                    stack.pop();
                }
            }
        }
        debug_assert!(stack.is_empty());
        if has_offscreens {
            let model_root = self.model_root.as_ref().unwrap().clone();
            owner.move_child(&model_root, -1);
        }
    }

    fn parent_viewport_rid(&self, parent: Option<&str>, owner: &Gd<Node2D>) -> Rid {
        parent
            .and_then(|id| self.offscreens.get(id))
            .map(|view| view.viewport.get_viewport_rid())
            .or_else(|| {
                owner
                    .get_viewport()
                    .map(|viewport| viewport.get_viewport_rid())
            })
            .unwrap_or(Rid::Invalid)
    }

    fn parent_node(&self, parent: Option<&str>) -> Gd<Node> {
        parent
            .and_then(|id| self.offscreens.get(id))
            .map(|view| view.root.clone().upcast())
            .unwrap_or_else(|| self.model_root.as_ref().unwrap().clone().upcast())
    }
}
