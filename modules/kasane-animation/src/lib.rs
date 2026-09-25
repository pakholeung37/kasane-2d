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

#[derive(Debug, Clone, PartialEq, Error)]
pub enum AnimationError {
    #[error("expression {0} does not exist")]
    MissingExpression(String),
    #[error("motion {0} does not exist")]
    MissingMotion(String),
    #[error("motion registration {group}[{index}] does not exist")]
    MissingMotionEntry { group: String, index: usize },
    #[error("motion {motion_id} has unresolved {category} target {runtime_id}")]
    UnresolvedMotionTarget {
        motion_id: String,
        category: String,
        runtime_id: String,
    },
    #[error("expression {expression_id} has unresolved parameter {runtime_id}")]
    UnresolvedParameter {
        expression_id: String,
        runtime_id: String,
    },
    #[error("invalid preview time or delta")]
    InvalidTime,
    #[error("cannot insert an activation before the current preview time")]
    PastActivation,
    #[error("seek would require more than one million replay steps")]
    SeekLimit,
    #[error("advance would produce more than one million motion events")]
    EventLimit,
    #[error("animation seek was cancelled")]
    SeekCancelled,
    #[error("geometry evaluation failed: {0}")]
    Evaluation(String),
}
