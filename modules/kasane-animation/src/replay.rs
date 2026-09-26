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
        // Every cold replay includes the zero-time evaluation. Otherwise the
        // first activation starts a frame late relative to seek(0) + advance.
        let total = 1 + whole + u32::from(time > whole as f32 / 60.0);
        Ok(Self {
            time,
            next: 0,
            total,
        })
    }
    pub(crate) fn resume_after(&mut self, completed: u32) {
        self.next = completed;
    }
    pub(crate) fn total(&self) -> u32 {
        self.total
    }
}
impl Iterator for ReplaySteps {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.next >= self.total {
            return None;
        }
        let time = if self.next + 1 == self.total {
            self.time
        } else {
            self.next as f32 / 60.0
        };
        self.next += 1;
        Some(time)
    }
}
