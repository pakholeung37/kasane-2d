use godot::prelude::*;

mod conversions;
mod deformer_data;
mod document_bridge;
mod document_preview;
mod mesh_data;
mod project_io;
mod selection_overlay;
mod texture_store;

pub use deformer_data::KasaneDeformerData;
pub use document_bridge::KasaneDocumentBridge;
pub use document_preview::KasaneDocumentPreview;
pub use kasane_render_godot::KasaneMeshView;
pub use mesh_data::KasaneMeshData;
pub use project_io::KasaneProjectIO;
pub use selection_overlay::KasaneSelectionOverlay;
pub use texture_store::KasaneTextureStore;

struct KasaneExtension;

#[gdextension(entry_symbol = kasane_gd_library_init)]
unsafe impl ExtensionLibrary for KasaneExtension {}
