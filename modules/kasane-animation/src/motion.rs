//! Stateless motion curve sampling. Queue state and fades are separate.
use kasane_core::document::{MotionPoint, MotionSegment, MotionTrack};

/// Immutable runtime curve, independent of JSON representation and metadata.
#[derive(Debug, Clone)]
pub struct CompiledCurve {
    /// Initial point, with time in seconds and an unweighted channel value.
    pub initial: MotionPoint,
    /// Shared ordered curve segments; source tracks must satisfy document validation.
    pub segments: std::sync::Arc<Vec<MotionSegment>>,
}
impl From<&MotionTrack> for CompiledCurve {
    fn from(track: &MotionTrack) -> Self {
        Self {
            initial: track.initial,
            segments: track.segments.clone(),
        }
    }
}

/// Sample the same four segment kinds used by Cubism MotionBehavior V2.
/// `restricted_beziers` selects the file's time-linear or Cardano path.
/// `time` is a finite clip-local time in seconds. This low-level sampler assumes
/// validated points; it does not apply looping, fades, parameter clamping or events.
pub fn sample_motion_curve(curve: &CompiledCurve, time: f32, restricted_beziers: bool) -> f32 {
    if curve.segments.is_empty() {
        return curve.initial.value;
    }
    let mut start = curve.initial;
    for segment in curve.segments.iter() {
        let end = segment.end();
        if time < end.time {
            return match segment {
                MotionSegment::Linear { .. } => {
                    let t = ((time - start.time) / (end.time - start.time)).max(0.0);
                    lerp(start.value, end.value, t)
                }
                MotionSegment::Bezier {
                    control1, control2, ..
                } => {
                    let t = if restricted_beziers {
                        ((time - start.time) / (end.time - start.time)).max(0.0)
                    } else {
                        let a = end.time - 3.0 * control2.time + 3.0 * control1.time - start.time;
                        let b = 3.0 * control2.time - 6.0 * control1.time + 3.0 * start.time;
                        let c = 3.0 * control1.time - 3.0 * start.time;
                        cardano_bezier(a, b, c, start.time - time)
                    };
                    bezier_value(start, *control1, *control2, end, t)
                }
                MotionSegment::Stepped { .. } => start.value,
                MotionSegment::InverseStepped { .. } => end.value,
            };
        }
        start = end;
    }
    start.value
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn bezier_value(
    start: MotionPoint,
    first: MotionPoint,
    second: MotionPoint,
    end: MotionPoint,
    t: f32,
) -> f32 {
    let a = lerp(start.value, first.value, t);
    let b = lerp(first.value, second.value, t);
    let c = lerp(second.value, end.value, t);
    lerp(lerp(a, b, t), lerp(b, c, t), t)
}

fn quadratic(a: f32, b: f32, c: f32) -> f32 {
    if a.abs() < 0.00001 {
        if b.abs() < 0.00001 {
            -c
        } else {
            -c / b
        }
    } else {
        -(b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
    }
}

fn cardano_bezier(a: f32, b: f32, c: f32, d: f32) -> f32 {
    if a.abs() < 0.00001 {
        return quadratic(b, c, d).clamp(0.0, 1.0);
    }
    let ba = b / a;
    let ca = c / a;
    let da = d / a;
    let p = (3.0 * ca - ba * ba) / 3.0;
    let p3 = p / 3.0;
    let q = (2.0 * ba * ba * ba - 9.0 * ba * ca + 27.0 * da) / 27.0;
    let q2 = q / 2.0;
    let discriminant = q2 * q2 + p3 * p3 * p3;
    let near_center = |root: f32| (root - 0.5).abs() < 0.51;
    if discriminant < 0.0 {
        let r = (-p3 * -p3 * -p3).sqrt();
        let angle = (-q / (2.0 * r)).clamp(-1.0, 1.0).acos();
        let scale = 2.0 * r.cbrt();
        let root1 = scale * (angle / 3.0).cos() - ba / 3.0;
        if near_center(root1) {
            return root1.clamp(0.0, 1.0);
        }
        let root2 = scale * ((angle + 2.0 * std::f32::consts::PI) / 3.0).cos() - ba / 3.0;
        if near_center(root2) {
            return root2.clamp(0.0, 1.0);
        }
        return (scale * ((angle + 4.0 * std::f32::consts::PI) / 3.0).cos() - ba / 3.0)
            .clamp(0.0, 1.0);
    }
    if discriminant == 0.0 {
        let u = if q2 < 0.0 { (-q2).cbrt() } else { -q2.cbrt() };
        let root = 2.0 * u - ba / 3.0;
        return if near_center(root) {
            root
        } else {
            -u - ba / 3.0
        }
        .clamp(0.0, 1.0);
    }
    let sd = discriminant.sqrt();
    ((sd - q2).cbrt() - (sd + q2).cbrt() - ba / 3.0).clamp(0.0, 1.0)
}
