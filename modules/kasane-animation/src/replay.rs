//! Absolute time grid shared by standalone and combined previews.
use crate::AnimationError;

pub(crate) struct ReplaySteps {
    time: f32,
    next: u32,
    total: u32,
}
impl ReplaySteps {
    pub(crate) fn new(time: f32) -> Result<Self, AnimationError> {
        if !time.is_finite() || time < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        if time * 60.0 > 1_000_000.0 {
            return Err(AnimationError::SeekLimit);
        }
        let whole = (time * 60.0).floor() as u32;
        let total = if time == 0.0 {
            1
        } else {
            whole + u32::from(time > whole as f32 / 60.0)
        };
        Ok(Self {
            time,
            next: 1,
            total,
        })
    }
    pub(crate) fn resume_after(&mut self, completed: u32) {
        self.next = completed + 1;
    }
    pub(crate) fn total(&self) -> u32 {
        self.total
    }
}
impl Iterator for ReplaySteps {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.next > self.total {
            return None;
        }
        let time = if self.next == self.total {
            self.time
        } else {
            self.next as f32 / 60.0
        };
        self.next += 1;
        Some(time)
    }
}
