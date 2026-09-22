// SPDX-FileCopyrightText: 2025-2026 undoredo contributors
// SPDX-FileCopyrightText: 2026 Kasane 2D Contributors
// SPDX-License-Identifier: MIT
// Adapted from undoredo 0.15.1 (70306463634a5f2723333a8b2c30acaf98bac041).
// First-write recording and reversible edits are specialized for Kasane below.
//! Bounded, engine-independent history. Entries own only replaced field values.
use crate::{ChangeKind, ChangeSet, Document, EditResult, Status, Vec2, VertexId};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Field {
    MeshName(String),
    Vertex(String, VertexId),
}
#[derive(Debug)]
pub(crate) enum Value {
    Name(String),
    Position(Vec2),
}
#[derive(Debug, Default)]
pub struct EditDelta {
    pub(crate) values: BTreeMap<Field, Value>,
}
impl EditDelta {
    pub(crate) fn name(id: &str, before: String) -> Self {
        Self {
            values: BTreeMap::from([(Field::MeshName(id.into()), Value::Name(before))]),
        }
    }
    pub(crate) fn vertex(&mut self, id: &str, vertex: VertexId, before: Vec2) {
        self.values
            .entry(Field::Vertex(id.into(), vertex))
            .or_insert(Value::Position(before));
    }
    pub(crate) fn merge(&mut self, other: Self) {
        // Keep the value before the FIRST write. The current document owns the latest value.
        for (key, value) in other.values {
            self.values.entry(key).or_insert(value);
        }
    }
    pub fn estimated_bytes(&self) -> usize {
        self.values
            .iter()
            .map(|(key, value)| {
                let id = match key {
                    Field::MeshName(id) | Field::Vertex(id, _) => id,
                };
                // Include an allowance for tree links, not just serialized payload size.
                std::mem::size_of::<(Field, Value)>()
                    + 64
                    + id.capacity()
                    + match value {
                        Value::Name(s) => s.capacity(),
                        Value::Position(_) => 0,
                    }
            })
            .sum()
    }
}

/// A mutation receipt is never part of a cloned document or saved content.
#[derive(Debug, Default)]
pub(crate) struct Receipt(pub Option<EditDelta>);
impl Clone for Receipt {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[derive(Debug)]
struct Action {
    label: String,
    delta: EditDelta,
}
impl Action {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.label.capacity() + self.delta.estimated_bytes()
    }
}

#[derive(Debug)]
pub struct History {
    done: VecDeque<Action>,
    undone: Vec<Action>,
    pending: Option<Action>,
    expected_revision: Option<u64>,
    max_steps: usize,
    max_bytes: usize,
    notice: Option<&'static str>,
}
impl Default for History {
    fn default() -> Self {
        Self::with_limits(50, 64 * 1024 * 1024)
    }
}
impl History {
    pub fn with_limits(max_steps: usize, max_bytes: usize) -> Self {
        Self {
            done: VecDeque::new(),
            undone: Vec::new(),
            pending: None,
            expected_revision: None,
            max_steps,
            max_bytes,
            notice: None,
        }
    }
    pub fn undo_len(&self) -> usize {
        self.done.len()
    }
    pub fn redo_len(&self) -> usize {
        self.undone.len()
    }
    pub fn active(&self) -> bool {
        self.pending.is_some()
    }
    pub fn estimated_bytes(&self) -> usize {
        self.done
            .iter()
            .chain(&self.undone)
            .chain(self.pending.iter())
            .map(Action::bytes)
            .sum()
    }
    pub fn notice(&self) -> Option<&'static str> {
        self.notice
    }
    pub fn clear(&mut self, revision: u64, notice: Option<&'static str>) {
        self.done.clear();
        self.undone.clear();
        self.pending = None;
        self.expected_revision = Some(revision);
        self.notice = notice;
    }
    /// Saving rewrites resource locations, not the name/position fields recorded here.
    pub fn saved(&mut self, before_revision: u64, after_revision: u64) {
        self.sync(before_revision);
        self.expected_revision = Some(after_revision);
    }
    fn sync(&mut self, revision: u64) {
        if self.expected_revision.is_some_and(|r| r != revision) {
            self.clear(revision, Some("HISTORY_EXTERNAL_EDIT"));
        }
        self.expected_revision = Some(revision);
    }
    /// Call once after every successful document mutation, before notifying consumers.
    /// Edits not implemented by the recorder form an explicit history barrier.
    pub fn record(&mut self, doc: &mut Document, edit: &EditResult) {
        if !edit.status.is_ok() || edit.changes.kind == ChangeKind::None {
            return;
        }
        if edit.changes.revision != doc.revision() {
            self.clear(doc.revision(), Some("HISTORY_EXTERNAL_EDIT"));
            return;
        }
        self.sync(doc.revision().saturating_sub(1));
        self.expected_revision = Some(doc.revision());
        let Some(delta) = doc.take_edit_delta() else {
            self.clear(doc.revision(), Some("HISTORY_UNSUPPORTED_EDIT"));
            return;
        };
        self.notice = None;
        if let Some(action) = &mut self.pending {
            action.delta.merge(delta);
            doc.remove_unchanged_history_fields(&mut action.delta);
            // Evict old entries before abandoning an oversized in-progress action.
            self.enforce_budget();
        } else {
            self.undone.clear();
            self.done.push_back(Action {
                label: "Edit".into(),
                delta,
            });
            self.enforce_budget();
        }
    }
    fn enforce_budget(&mut self) {
        if self.max_steps == 0
            || self
                .pending
                .as_ref()
                .is_some_and(|a| a.bytes() > self.max_bytes)
            || self.done.back().is_some_and(|a| a.bytes() > self.max_bytes)
        {
            self.clear(
                self.expected_revision.unwrap_or(0),
                Some("HISTORY_LIMIT_EXCEEDED"),
            );
            return;
        }
        while self.done.len() > self.max_steps || self.estimated_bytes() > self.max_bytes {
            if self.done.pop_front().is_none() {
                // A pending action and retained redo data may together exceed the budget.
                self.undone.clear();
                break;
            }
        }
    }
    pub fn begin(&mut self, doc: &Document, label: String) -> Status {
        self.sync(doc.revision());
        if doc.transaction_active() {
            return Status::error("TRANSACTION_ACTIVE", "Commit or cancel vertex edits first");
        }
        if self.active() {
            return Status::error("ACTION_ACTIVE", "Finish the active action first");
        }
        if label.capacity() + std::mem::size_of::<Action>() > self.max_bytes || self.max_steps == 0
        {
            return Status::error(
                "HISTORY_LIMIT_EXCEEDED",
                "Action exceeds the configured history budget",
            );
        }
        self.pending = Some(Action {
            label,
            delta: EditDelta::default(),
        });
        self.notice = None;
        Status::ok()
    }
    pub fn end(&mut self, doc: &Document) -> Status {
        self.sync(doc.revision());
        if doc.transaction_active() {
            return Status::error("TRANSACTION_ACTIVE", "Commit or cancel vertex edits first");
        }
        let Some(mut action) = self.pending.take() else {
            return Status::error("NO_ACTION", "No action is active");
        };
        doc.remove_unchanged_history_fields(&mut action.delta);
        if !action.delta.values.is_empty() {
            self.undone.clear();
            self.done.push_back(action);
            self.enforce_budget();
        }
        Status::ok()
    }
    pub fn cancel(&mut self, doc: &mut Document) -> EditResult {
        self.sync(doc.revision());
        if doc.transaction_active() {
            return failure(doc, "TRANSACTION_ACTIVE");
        }
        let Some(action) = &mut self.pending else {
            return failure(doc, "NO_ACTION");
        };
        let result = doc.replay_history(&mut action.delta);
        if result.status.is_ok() {
            self.pending = None;
            self.expected_revision = Some(doc.revision());
        }
        result
    }
    pub fn undo(&mut self, doc: &mut Document) -> EditResult {
        self.replay(doc, false)
    }
    pub fn redo(&mut self, doc: &mut Document) -> EditResult {
        self.replay(doc, true)
    }
    fn replay(&mut self, doc: &mut Document, redo: bool) -> EditResult {
        self.sync(doc.revision());
        if self.active() {
            return failure(doc, "ACTION_ACTIVE");
        }
        if doc.transaction_active() {
            return failure(doc, "TRANSACTION_ACTIVE");
        }
        let action = if redo {
            self.undone.last_mut()
        } else {
            self.done.back_mut()
        };
        let Some(action) = action else {
            return failure(doc, if redo { "NO_REDO" } else { "NO_UNDO" });
        };
        let result = doc.replay_history(&mut action.delta);
        if result.status.is_ok() {
            if redo {
                self.done.push_back(self.undone.pop().unwrap());
            } else {
                self.undone.push(self.done.pop_back().unwrap());
            }
            self.expected_revision = Some(doc.revision());
            self.notice = None;
            self.enforce_budget();
        }
        result
    }
}
fn failure(doc: &Document, code: &str) -> EditResult {
    EditResult {
        status: Status::error(code, "History operation is unavailable"),
        changes: ChangeSet {
            revision: doc.revision(),
            ..Default::default()
        },
        referrers: Vec::new(),
    }
}
