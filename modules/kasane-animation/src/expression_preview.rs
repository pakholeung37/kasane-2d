use crate::{expression::ExpressionRuntime, AnimationError};
use kasane_core::{evaluate_frame, Document, DrawableFrame, PreviewValues};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshot {
    pub time: f32,
    /// Parameter UUIDs and their final, clamped values.
    pub parameters: BTreeMap<String, f32>,
    pub active_expressions: Vec<String>,
}

/// A deterministic Expression-stage preview. `seek` replays the same fixed
/// steps from the initial state, including activation order.
#[derive(Clone)]
pub struct ExpressionPreview {
    document: Arc<Document>,
    base: BTreeMap<String, f32>,
    snapshot: RuntimeSnapshot,
    stage: ExpressionRuntime,
}

impl ExpressionPreview {
    pub fn new(document: &Document) -> Self {
        let base = document
            .parameter_order()
            .iter()
            .map(|id| {
                let parameter = document
                    .get_parameter(id)
                    .expect("ordered parameter exists");
                (id.clone(), parameter.default_value)
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            document: Arc::new(document.clone()),
            snapshot: RuntimeSnapshot {
                time: 0.0,
                parameters: base.clone(),
                active_expressions: Vec::new(),
            },
            base,
            stage: ExpressionRuntime::default(),
        }
    }

    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }

    pub fn snapshot(&self) -> &RuntimeSnapshot {
        &self.snapshot
    }

    pub fn set_base_parameter(&mut self, id: &str, value: f32) -> Result<(), AnimationError> {
        let parameter = self.document.get_parameter(id);
        if !value.is_finite() || parameter.is_none() {
            return Err(AnimationError::Evaluation(format!(
                "invalid parameter {id}"
            )));
        }
        let parameter = parameter.expect("checked above");
        self.base
            .insert(id.into(), value.clamp(parameter.minimum, parameter.maximum));
        self.reset();
        Ok(())
    }

    pub fn schedule_expression(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.stage
            .schedule(&self.document, self.snapshot.time, id, time)
    }

    pub fn reset(&mut self) {
        self.snapshot = RuntimeSnapshot {
            time: 0.0,
            parameters: self.base.clone(),
            active_expressions: Vec::new(),
        };
        self.stage.reset();
    }

    pub fn advance(&mut self, dt: f32) -> Result<&RuntimeSnapshot, AnimationError> {
        if !dt.is_finite() || dt < 0.0 || !((self.snapshot.time + dt).is_finite()) {
            return Err(AnimationError::InvalidTime);
        }
        let time = self.snapshot.time + dt;
        self.snapshot.parameters.clone_from(&self.base);
        self.stage
            .evaluate(&self.document, time, &mut self.snapshot.parameters);
        self.snapshot.time = time;
        self.snapshot.active_expressions = self.stage.active_ids();
        Ok(&self.snapshot)
    }

    /// Replay at 60 Hz, with a final short step. The schedule and base inputs
    /// stay fixed, so repeated seeks produce identical snapshots.
    pub fn seek(&mut self, time: f32) -> Result<&RuntimeSnapshot, AnimationError> {
        let steps = crate::replay::ReplaySteps::new(time)?;
        self.reset();
        for next in steps {
            self.advance((next - self.snapshot.time).max(0.0))?;
        }
        Ok(&self.snapshot)
    }

    pub fn evaluate_drawables(&self) -> Result<DrawableFrame, AnimationError> {
        let values: PreviewValues = self
            .snapshot
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(&self.document, &values, &mut frame);
        if status.is_ok() {
            Ok(frame)
        } else {
            Err(AnimationError::Evaluation(format!(
                "{}: {}",
                status.code, status.message
            )))
        }
    }
}
