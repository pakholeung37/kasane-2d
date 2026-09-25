//! CPU animation state. The authoring document is copied once; advancing a
//! preview never edits it or the owning SDK session.

mod motion;
mod replay;
mod seek_cache;
pub use motion::{sample_motion_curve, CompiledCurve};
pub use seek_cache::SeekCacheStats;
mod motion_preview;
mod motion_runtime;
mod physics;
pub use physics::PhysicsPreview;
mod pose;
pub use motion_preview::{MotionEventFired, MotionPreview, MotionSnapshot};

mod expression;
mod expression_preview;
pub use expression_preview::{ExpressionPreview, RuntimeSnapshot};
use thiserror::Error;

/// Scheduling, replay-limit and evaluation failures in detached animation previews.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AnimationError {
    /// The requested expression UUID does not exist.
    #[error("expression {0} does not exist")]
    MissingExpression(String),
    /// The requested motion UUID does not exist.
    #[error("motion {0} does not exist")]
    MissingMotion(String),
    /// No registration exists at the exact group name and zero-based index.
    #[error("motion registration {group}[{index}] does not exist")]
    MissingMotionEntry {
        /// Requested model3 motion group name.
        group: String,
        /// Requested zero-based entry index.
        index: usize,
    },
    /// The motion contains an unresolved imported target.
    #[error("motion {motion_id} has unresolved {category} target {runtime_id}")]
    UnresolvedMotionTarget {
        /// Source motion UUID.
        motion_id: String,
        /// Imported target category.
        category: String,
        /// Unresolved runtime ID from the source asset.
        runtime_id: String,
    },
    /// The expression contains an unresolved imported parameter.
    #[error("expression {expression_id} has unresolved parameter {runtime_id}")]
    UnresolvedParameter {
        /// Source expression UUID.
        expression_id: String,
        /// Unresolved runtime ID from the source asset.
        runtime_id: String,
    },
    /// A time/delta is invalid, or a Physics/input parameter value is nonfinite.
    #[error("invalid preview time or delta")]
    InvalidTime,
    /// An activation/input time precedes the current preview time.
    #[error("cannot insert an activation before the current preview time")]
    PastActivation,
    /// The requested time multiplied by 60 exceeds one million replay steps.
    #[error("seek would require more than one million replay steps")]
    SeekLimit,
    /// The estimated motion event batch exceeds one million events.
    #[error("advance would produce more than one million motion events")]
    EventLimit,
    /// The seek progress callback returned false; playback and cache remain unchanged.
    #[error("animation seek was cancelled")]
    SeekCancelled,
    /// Core evaluation failed, or a parameter UUID/value is invalid for the operation.
    #[error("geometry evaluation failed: {0}")]
    Evaluation(String),
}
