//! Session-owned preview evaluation. Published frames are immutable snapshots.
use std::sync::Arc;

use crate::{Document, DrawableFrame, FrameEvaluator, PreviewValues, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameKey {
    generation: u64,
    document_revision: u64,
    preview_revision: u64,
}

/// One instance per editing session. Call `reset` when replacing a document,
/// and `invalidate` for initialization or mutations that do not advance its revision.
#[derive(Debug, Default)]
pub struct PreviewState {
    values: PreviewValues,
    revision: u64,
    cached: Option<(FrameKey, Result<Arc<DrawableFrame>, Status>)>,
    evaluator: FrameEvaluator,
    output: DrawableFrame,
    evaluations: u64,
}

impl PreviewState {
    pub fn values(&self) -> &PreviewValues {
        &self.values
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn evaluation_count(&self) -> u64 {
        self.evaluations
    }

    pub fn invalidate(&mut self) {
        if let Some((_, Ok(frame))) = self.cached.take() {
            if let Ok(frame) = Arc::try_unwrap(frame) {
                self.output = frame;
            }
        }
    }

    pub fn reset(&mut self) {
        self.values.clear();
        self.revision += 1;
        self.invalidate();
    }

    pub fn retain_parameters(&mut self, doc: &Document) {
        let before = self.values.len();
        self.values.retain(|id, _| doc.get_parameter(id).is_some());
        if before != self.values.len() {
            self.revision += 1;
            self.invalidate();
        }
    }

    fn evaluate(
        &mut self,
        doc: &Document,
        values: &PreviewValues,
    ) -> Result<Arc<DrawableFrame>, Status> {
        self.evaluations += 1;
        let status = self.evaluator.evaluate(doc, values, &mut self.output);
        if !status.is_ok() {
            return Err(status);
        }
        Ok(Arc::new(std::mem::take(&mut self.output)))
    }

    pub fn frame(&mut self, doc: &Document, generation: u64) -> Result<Arc<DrawableFrame>, Status> {
        let key = FrameKey {
            generation,
            document_revision: doc.evaluation_revision(),
            preview_revision: self.revision,
        };
        if let Some((cached_key, result)) = &self.cached {
            if *cached_key == key {
                return result.clone();
            }
        }
        self.evaluations += 1;
        let status = self.evaluator.evaluate(doc, &self.values, &mut self.output);
        let result = if status.is_ok() {
            Ok(Arc::new(std::mem::take(&mut self.output)))
        } else {
            Err(status)
        };
        self.invalidate();
        self.cached = Some((key, result.clone()));
        result
    }

    /// Evaluate before committing. Failure preserves parameters, version and cached frame.
    pub fn replace(
        &mut self,
        doc: &Document,
        generation: u64,
        values: PreviewValues,
    ) -> Result<bool, Status> {
        if self.values == values {
            self.frame(doc, generation)?;
            return Ok(false);
        }
        let frame = self.evaluate(doc, &values)?;
        self.invalidate();
        self.values = values;
        self.revision += 1;
        self.cached = Some((
            FrameKey {
                generation,
                document_revision: doc.evaluation_revision(),
                preview_revision: self.revision,
            },
            Ok(frame),
        ));
        Ok(true)
    }
}
