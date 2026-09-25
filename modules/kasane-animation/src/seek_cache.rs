//! Canonical replay checkpoints. The budget covers retained state estimates,
//! excluding the shared document and temporary transactional seek state.
use crate::{
    expression::ExpressionRuntime, motion_runtime::MotionRuntime, physics::PhysicsRuntime,
    pose::PoseRuntime, MotionSnapshot,
};
use std::{
    collections::{BTreeMap, LinkedList},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeekCacheStats {
    pub budget_bytes: usize,
    pub estimated_bytes: usize,
    pub checkpoints: usize,
    pub last_restored_time: f32,
    pub last_replayed_steps: u32,
}

#[derive(Clone)]
pub(crate) struct Checkpoint {
    pub step: u32,
    pub snapshot: MotionSnapshot,
    pub motion: MotionRuntime,
    pub expressions: ExpressionRuntime,
    pub physics: PhysicsRuntime,
    pub pose: PoseRuntime,
}

#[derive(Clone)]
pub(crate) struct SeekCache {
    entries: LinkedList<(usize, Arc<Checkpoint>)>,
    pub stats: SeekCacheStats,
}
impl Default for SeekCache {
    fn default() -> Self {
        Self {
            entries: LinkedList::new(),
            stats: SeekCacheStats {
                budget_bytes: 16 * 1024 * 1024,
                estimated_bytes: 0,
                checkpoints: 0,
                last_restored_time: 0.0,
                last_replayed_steps: 0,
            },
        }
    }
}
impl SeekCache {
    pub fn clear(&mut self) {
        self.entries = LinkedList::new();
        self.stats.estimated_bytes = 0;
        self.stats.checkpoints = 0;
        self.stats.last_restored_time = 0.0;
        self.stats.last_replayed_steps = 0;
    }
    pub fn set_budget(&mut self, bytes: usize) {
        self.clear();
        self.stats.budget_bytes = bytes;
    }
    pub fn nearest(&self, time: f32) -> Option<Arc<Checkpoint>> {
        self.entries
            .iter()
            .filter(|(_, state)| state.snapshot.time <= time)
            .max_by_key(|(_, state)| state.step)
            .map(|(_, state)| Arc::clone(state))
    }
    pub fn insert(&mut self, bytes: usize, checkpoint: impl FnOnce() -> Checkpoint) {
        if bytes > self.stats.budget_bytes {
            return;
        }
        while self.stats.estimated_bytes > self.stats.budget_bytes - bytes {
            if let Some((size, _)) = self.entries.pop_front() {
                self.stats.estimated_bytes -= size;
            }
        }
        self.entries.push_back((bytes, Arc::new(checkpoint())));
        self.stats.estimated_bytes += bytes;
        self.stats.checkpoints = self.entries.len();
    }
}

// Conservative retained-allocation accounting, including container overhead.
// BTree nodes can have spare slots; charge a full node per entry.
pub(crate) fn map_bytes(map: &BTreeMap<String, f32>) -> usize {
    map.keys().map(|key| 512 + key.capacity()).sum()
}
pub(crate) fn vec_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity() * std::mem::size_of::<T>()
}
pub(crate) fn strings_bytes(values: &Vec<String>) -> usize {
    vec_bytes(values) + values.iter().map(String::capacity).sum::<usize>()
}
