//! Engine-independent Rust authoring SDK with project open/save and checkpoint history.
mod assets;
mod diagnostics;
mod edit;
mod project_io;
mod session;
mod types;

pub use assets::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
};
pub use diagnostics::{GeometryBounds, GeometryChecks, GeometryDiagnostic, GeometryDiagnosticKind};
pub use edit::EditSession;
pub use kasane_animation::{
    AnimationError, ExpressionPreview, MotionEventFired, MotionPreview, MotionSnapshot,
    PhysicsPreview, RuntimeSnapshot, SeekCacheStats,
};
pub use kasane_project::{
    FileSystem, ImportReport, NativeFileSystem, ProjectResult, PsdImportReport, ResourceDiagnostic,
};
pub use session::AuthoringSession;
pub use types::{
    EditReceipt, GeometrySnapshot, HistoryLimits, HistoryState, ImportReceipt, MeshProperties,
    ObjectHandle, ObjectKind, PsdImportReceipt, SaveReceipt, SdkError, SourceSpace,
    TopologyReplacement, Version,
};

mod geometry;
pub use geometry::{rectangle_grid_geometry, MeshGeometry};
