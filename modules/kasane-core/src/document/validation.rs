use crate::types::{Appearance, RotationPose, Status};

pub fn valid_uuid(id: &str) -> bool {
    if id.len() != 36 {
        return false;
    }
    let mut nonzero = false;
    for (i, b) in id.bytes().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            if b != b'-' {
                return false;
            }
        } else {
            if !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b) {
                return false;
            }
            nonzero |= b != b'0';
        }
    }
    nonzero
}

pub fn validate_appearance(a: &Appearance, id: &str) -> Status {
    // MOC3 keyform opacity can exceed 1 (e.g. authored 1.00005).
    // Core interpolates it without clamping; retain that value for roundtrips.
    if !a.opacity.is_finite() || a.opacity < 0.0 {
        return Status::error("INVALID_OPACITY", id);
    }
    for &c in a.multiply.iter().chain(a.screen.iter()) {
        if !c.is_finite() || c < 0.0 || c > 1.0 {
            return Status::error("INVALID_COLOR", id);
        }
    }
    Status::ok()
}

pub fn validate_draw_order(order: f32, id: &str) -> Status {
    if !order.is_finite() || order < -32768.0 || order > 32767.0 {
        return Status::error(
            "INVALID_DRAW_ORDER",
            format!("{}: supported order range -32768..32767", id),
        );
    }
    Status::ok()
}

pub fn pose_valid(p: &RotationPose, id: &str) -> Status {
    if !p.origin.x.is_finite()
        || !p.origin.y.is_finite()
        || !p.angle.is_finite()
        || !p.scale.is_finite()
        || p.scale < 0.0
    {
        return Status::error("INVALID_ROTATION", id);
    }
    Status::ok()
}
