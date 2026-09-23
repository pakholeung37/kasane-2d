use super::resources::ViewLayout;
use super::*;
use kasane_render::{ScenePlan, TargetId, TargetItem};

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

    pub(super) fn update_dependencies(
        &self,
        owner: &Gd<Node2D>,
        scene: &ScenePlan,
        layout: &ViewLayout,
    ) {
        for target in scene.targets().iter().skip(1) {
            let view = &self.offscreens[&target.id];
            RenderingServer::singleton().viewport_set_parent_viewport(
                view.viewport.get_viewport_rid(),
                self.target_viewport(scene, target.parent, owner),
            );
        }
        for key in layout.masks.keys() {
            RenderingServer::singleton().viewport_set_parent_viewport(
                self.masks[key].viewport.get_viewport_rid(),
                self.target_viewport(scene, key.consumer, owner),
            );
        }
    }

    /// Scene-tree order is local to a target. Dependencies are registered
    /// separately; no backend-neutral command has Godot's assembly timing.
    pub(super) fn sync_target_order(&mut self, owner: &mut Gd<Node2D>, scene: &ScenePlan) {
        self.next_order.clear();
        for target in scene.targets() {
            let parent = if target.id.is_empty() {
                self.model_root.as_ref().unwrap().instance_id()
            } else {
                self.offscreens[&target.id].root.instance_id()
            };
            self.next_order.push((parent, parent, false));
            for item in &target.items {
                let (node, reads) = match item {
                    TargetItem::Draw(mesh) => {
                        let mesh = &scene.meshes()[mesh.0];
                        (self.views[&mesh.id].instance_id(), mesh.reads_destination)
                    }
                    TargetItem::Composite(child) => {
                        let child = &scene.targets()[child.0];
                        (
                            self.offscreens[&child.id].composite.instance_id(),
                            child.reads_destination,
                        )
                    }
                };
                self.next_order.push((parent, node, reads));
            }
        }
        if self.next_order == self.order {
            return;
        }
        std::mem::swap(&mut self.order, &mut self.next_order);
        self.order_syncs += 1;
        for target in scene.targets() {
            let mut parent: Gd<Node> = if target.id.is_empty() {
                self.model_root.as_ref().unwrap().clone().upcast()
            } else {
                self.offscreens[&target.id].root.clone().upcast()
            };
            for item in &target.items {
                let (id, reads, mut node) = match item {
                    TargetItem::Draw(mesh) => {
                        let mesh = &scene.meshes()[mesh.0];
                        (
                            mesh.id.as_str(),
                            mesh.reads_destination,
                            self.views[&mesh.id].clone().upcast::<Node>(),
                        )
                    }
                    TargetItem::Composite(child) => {
                        let child = &scene.targets()[child.0];
                        (
                            child.id.as_str(),
                            child.reads_destination,
                            self.offscreens[&child.id]
                                .composite
                                .clone()
                                .upcast::<Node>(),
                        )
                    }
                };
                if node.get_parent() != Some(parent.clone()) {
                    node.reparent_ex(&parent)
                        .keep_global_transform(false)
                        .done();
                }
                if reads {
                    self.place_destination_copy(id, &mut parent);
                }
                parent.move_child(&node, -1);
            }
        }
        if scene.targets().len() > 1 {
            let model_root = self.model_root.as_ref().unwrap().clone();
            owner.move_child(&model_root, -1);
        }
    }

    fn target_viewport(&self, scene: &ScenePlan, target: TargetId, owner: &Gd<Node2D>) -> Rid {
        if target == TargetId::MAIN {
            owner
                .get_viewport()
                .map(|v| v.get_viewport_rid())
                .unwrap_or(Rid::Invalid)
        } else {
            self.offscreens[&scene.targets()[target.0].id]
                .viewport
                .get_viewport_rid()
        }
    }
}
