//! Godot implementation of the backend-neutral Kasane render plan.
//!
//! The public `kasane-godot` crate owns the GDExtension API and preview
//! lifecycle. This crate owns Godot rendering resources, shaders, scene-tree
//! execution, and the mesh view class used by that API.

mod backend;
mod conversions;
mod mesh_view;

pub use backend::{BackendRenderResult, GodotRenderBackend, RenderRequest};
pub use mesh_view::KasaneMeshView;
