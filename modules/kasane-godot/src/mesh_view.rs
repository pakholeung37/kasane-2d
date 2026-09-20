use godot::classes::{
    canvas_item::{TextureFilter, TextureRepeat},
    mesh::{ArrayFormat, ArrayType, PrimitiveType},
    ArrayMesh, MeshInstance2D, Texture2D,
};
use godot::prelude::*;

use kasane_core::geometry::{validate_positions, validate_render_mesh};
use kasane_core::types::Vec2;

use crate::conversions::{
    error_dict, is_main_thread, packed_to_vectors, status_to_dict, vectors_to_packed, Array,
    Dictionary,
};

#[derive(GodotClass)]
#[class(init, base=MeshInstance2D)]
pub struct KasaneMeshView {
    base: Base<MeshInstance2D>,
    surface: Option<Gd<ArrayMesh>>,
    positions: Vec<Vec2>,
    uploads: u64,
    creations: u64,
}

#[godot_api]
impl KasaneMeshView {
    #[func]
    pub fn initialize(
        &mut self,
        positions: PackedVector2Array,
        uvs: PackedVector2Array,
        indices: PackedInt32Array,
        texture: Option<Gd<Texture2D>>,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Rendering calls require the main thread.");
        }
        let Some(tex) = texture else {
            return error_dict("INVALID_TEXTURE", "Supply a loaded Texture2D.");
        };
        if tex.get_width() <= 0 || tex.get_height() <= 0 {
            return error_dict("INVALID_TEXTURE", "Supply a loaded Texture2D.");
        }
        let next = packed_to_vectors(&positions);
        let next_uvs = packed_to_vectors(&uvs);
        let mut dense = Vec::with_capacity(indices.len());
        for i in 0..indices.len() {
            let idx = indices[i];
            if idx < 0 {
                return error_dict("INVALID_INDEX", "Indices cannot be negative.");
            }
            dense.push(idx as u32);
        }
        let status = validate_render_mesh(&next, &next_uvs, &dense);
        if !status.is_ok() {
            return status_to_dict(&status);
        }

        let mut next_surface = ArrayMesh::new_gd();
        let mut gpu_positions = PackedVector3Array::new();
        gpu_positions.resize(next.len());
        for (i, p) in next.iter().enumerate() {
            gpu_positions[i] = Vector3::new(p.x, p.y, 0.0);
        }
        let mut arrays = Array::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &gpu_positions.to_variant(),
        );
        arrays.set(ArrayType::TEX_UV.ord() as usize, &uvs.to_variant());
        arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());

        next_surface
            .add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays)
            .flags(ArrayFormat::FLAG_USE_DYNAMIC_UPDATE)
            .done();

        self.surface = Some(next_surface.clone());
        self.positions = next;
        self.base_mut().set_mesh(&next_surface);
        self.base_mut().set_texture(&tex);
        self.base_mut().set_texture_filter(TextureFilter::NEAREST);
        self.base_mut().set_texture_repeat(TextureRepeat::DISABLED);
        self.update_bounds();
        self.creations += 1;
        status_to_dict(&kasane_core::types::Status::ok())
    }

    #[func]
    pub fn update_positions(&mut self, positions: PackedVector2Array) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Rendering calls require the main thread.");
        }
        if self.surface.is_none() {
            return error_dict("NOT_INITIALIZED", "Initialize the mesh first.");
        }
        if positions.len() != self.positions.len() {
            return error_dict(
                "INVALID_LENGTH",
                "Position updates must preserve topology and vertex count.",
            );
        }
        let next = packed_to_vectors(&positions);
        let status = validate_positions(&next);
        if !status.is_ok() {
            return status_to_dict(&status);
        }
        let mut upload = PackedByteArray::new();
        upload.resize(next.len() * 3 * 4);
        for (i, p) in next.iter().enumerate() {
            let offset = i * 12;
            let x_bytes = p.x.to_ne_bytes();
            let y_bytes = p.y.to_ne_bytes();
            let z_bytes = 0.0f32.to_ne_bytes();
            for b in 0..4 {
                upload[offset + b] = x_bytes[b];
                upload[offset + 4 + b] = y_bytes[b];
                upload[offset + 8 + b] = z_bytes[b];
            }
        }
        self.surface
            .as_mut()
            .unwrap()
            .surface_update_vertex_region(0, 0, &upload);
        self.positions = next;
        self.update_bounds();
        self.uploads += 1;
        status_to_dict(&kasane_core::types::Status::ok())
    }

    fn update_bounds(&mut self) {
        if self.positions.is_empty() {
            return;
        }
        let mut left = self.positions[0].x;
        let mut right = left;
        let mut top = self.positions[0].y;
        let mut bottom = top;
        for p in &self.positions {
            left = left.min(p.x);
            right = right.max(p.x);
            top = top.min(p.y);
            bottom = bottom.max(p.y);
        }
        if let Some(surf) = self.surface.as_mut() {
            surf.set_custom_aabb(Aabb::new(
                Vector3::new(left, top, -0.5),
                Vector3::new(right - left, bottom - top, 1.0),
            ));
        }
    }

    #[func]
    pub fn clear(&mut self) {
        if !is_main_thread() {
            godot_error!("Rendering calls require the main thread.");
            return;
        }
        self.surface = None;
        self.positions.clear();
    }

    #[func]
    pub fn get_render_stats(&self) -> Dictionary {
        let mut d = Dictionary::new();
        d.set("uploads", self.uploads as i64);
        d.set("creations", self.creations as i64);
        d
    }

    #[func]
    pub fn get_positions_snapshot(&self) -> PackedVector2Array {
        vectors_to_packed(&self.positions)
    }
}
