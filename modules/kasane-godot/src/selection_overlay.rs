use godot::classes::Node2D;
use godot::prelude::*;

use crate::document_preview::KasaneDocumentPreview;

/// Drawn in preview-local coordinates; add it as a child of the preview.
/// It never changes the model mesh, material, or source Document.
#[derive(GodotClass)]
#[class(init, base=Node2D)]
pub struct KasaneSelectionOverlay {
    base: Base<Node2D>,
    preview: Option<Gd<KasaneDocumentPreview>>,
    selected_id: GString,
    show_vertices: bool,
}

#[godot_api]
impl INode2D for KasaneSelectionOverlay {
    fn process(&mut self, _delta: f64) {
        if self.preview.is_some() && !self.selected_id.is_empty() {
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(preview) = &self.preview else {
            return;
        };
        if self.selected_id.is_empty() {
            return;
        }
        let Some(view) = preview.bind().get_mesh_view(self.selected_id.clone()) else {
            return;
        };
        let points = view.bind().get_positions_snapshot();
        if points.is_empty() {
            return;
        }

        let mut bounds = Rect2::new(points[0], Vector2::ZERO);
        for i in 1..points.len() {
            bounds = bounds.expand(points[i]);
        }
        let color = Color::from_rgba(0.1, 0.8, 1.0, 0.95);
        self.base_mut()
            .draw_rect_ex(bounds.grow(3.0), color)
            .filled(false)
            .width(1.5)
            .done();
        let pivot = Vector2::new(
            bounds.position.x + bounds.size.x * 0.5,
            bounds.position.y - 3.0,
        );
        let handle = pivot + Vector2::new(0.0, -20.0);
        self.base_mut().draw_line(pivot, handle, color);
        self.base_mut().draw_circle(handle, 4.0, color);
        if self.show_vertices {
            for i in 0..points.len() {
                self.base_mut().draw_circle(points[i], 2.5, color);
            }
        }
    }
}

#[godot_api]
impl KasaneSelectionOverlay {
    #[func]
    pub fn set_preview(&mut self, preview: Option<Gd<KasaneDocumentPreview>>) {
        self.preview = preview;
        self.base_mut().queue_redraw();
    }

    #[func]
    pub fn select(&mut self, id: GString) {
        self.selected_id = id;
        self.base_mut().queue_redraw();
    }

    #[func]
    pub fn set_show_vertices(&mut self, show: bool) {
        self.show_vertices = show;
        self.base_mut().queue_redraw();
    }
}
