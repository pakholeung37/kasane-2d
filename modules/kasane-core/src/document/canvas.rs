use super::*;
use crate::draw_order::{validate_groups, DrawOrderGroup};
use crate::types::{Canvas, ChangeKind, EditResult, Status};

impl Document {
    pub fn replace_canvas(&mut self, canvas: Canvas) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &self.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &self.id));
        }
        let mut candidate = Document::new();
        let s = candidate.initialize(self.id.clone(), canvas);
        if !s.is_ok() {
            return self.failed(s);
        }
        self.canvas = canvas;
        let meshes = self.mesh_order.clone();
        let doc_id = self.id.clone();
        self.changed(ChangeKind::Structure, meshes, vec![doc_id])
    }

    pub fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.draw_order_groups.as_deref()
    }

    pub fn replace_draw_order_groups(&mut self, groups: Vec<DrawOrderGroup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let groups = match validate_groups(self, &groups) {
            Ok(groups) => groups,
            Err(status) => return self.failed(status),
        };
        self.draw_order_groups = Some(groups);
        self.changed(ChangeKind::Structure, self.mesh_order.clone(), Vec::new())
    }
}
