//! Detached Expression preview exposed to Python.
use crate::conversion::{frame_tuple, EvaluationTuple};
use crate::error::sdk_failure;
use kasane_sdk::{AnimationError, ExpressionPreview, MotionPreview, PhysicsPreview, SdkError};
use pyo3::prelude::*;

#[pyclass]
pub(crate) struct NativeExpressionPreview {
    inner: ExpressionPreview,
}

impl NativeExpressionPreview {
    pub(crate) fn new(inner: ExpressionPreview) -> Self {
        Self { inner }
    }
}

fn failure(py: Python<'_>, error: AnimationError, operation: &'static str) -> PyErr {
    let code = match error {
        AnimationError::MissingExpression(_) => "MISSING_EXPRESSION",
        AnimationError::MissingMotion(_) => "MISSING_MOTION",
        AnimationError::MissingMotionEntry { .. } => "MISSING_MOTION_ENTRY",
        AnimationError::UnresolvedMotionTarget { .. } => "UNRESOLVED_MOTION_TARGET",
        AnimationError::UnresolvedParameter { .. } => "UNRESOLVED_PARAMETER",
        AnimationError::InvalidTime => "INVALID_TIME",
        AnimationError::PastActivation => "PAST_ACTIVATION",
        AnimationError::SeekLimit => "SEEK_LIMIT",
        AnimationError::EventLimit => "EVENT_LIMIT",
        AnimationError::SeekCancelled => "SEEK_CANCELLED",
        AnimationError::Evaluation(_) => "EVALUATION_FAILED",
    };
    sdk_failure(
        py,
        SdkError {
            code: code.into(),
            message: error.to_string().into(),
            operation,
            object_ids: Vec::new(),
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        },
    )
}

#[pyclass]
pub(crate) struct NativePhysicsPreview {
    inner: PhysicsPreview,
}

impl NativePhysicsPreview {
    pub(crate) fn new(inner: PhysicsPreview) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl NativePhysicsPreview {
    fn document_revision(&self) -> u64 {
        self.inner.document_revision()
    }
    fn parameters(&self) -> Vec<(String, f32)> {
        self.inner
            .parameters()
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect()
    }
    fn diagnostics(&self) -> Vec<String> {
        self.inner.diagnostics().to_vec()
    }
    fn set_parameter(&mut self, py: Python<'_>, id: &str, value: f32) -> PyResult<()> {
        self.inner
            .set_parameter(id, value)
            .map_err(|error| failure(py, error, "set_parameter"))
    }
    fn advance(&mut self, py: Python<'_>, dt: f32) -> PyResult<Vec<(String, f32)>> {
        self.inner
            .advance(dt)
            .map(|values| {
                values
                    .iter()
                    .map(|(id, value)| (id.clone(), *value))
                    .collect()
            })
            .map_err(|error| failure(py, error, "advance"))
    }
    fn stabilize(&mut self) -> Vec<(String, f32)> {
        self.inner
            .stabilize()
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }
}

#[pyclass]
pub(crate) struct NativeMotionPreview {
    inner: MotionPreview,
}

impl NativeMotionPreview {
    pub(crate) fn new(inner: MotionPreview) -> Self {
        Self { inner }
    }

    #[cfg(feature = "observe")]
    pub(crate) fn inner(&self) -> &MotionPreview {
        &self.inner
    }
}

type MotionPreviewTuple = (
    f32,
    Vec<(String, f32)>,
    Vec<(String, f32)>,
    Vec<(String, f32)>,
    f32,
    Vec<String>,
    Vec<String>,
    Vec<(String, String, String)>,
    Vec<String>,
);

fn motion_snapshot(preview: &MotionPreview) -> MotionPreviewTuple {
    let state = preview.snapshot();
    (
        state.time,
        state
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect(),
        state
            .part_opacity_channels
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect(),
        state
            .part_opacities
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect(),
        state.model_opacity,
        state.active_motions.clone(),
        state.active_expressions.clone(),
        state
            .fired_events
            .iter()
            .map(|event| {
                (
                    event.motion_id.clone(),
                    event.event_id.clone(),
                    event.value.clone(),
                )
            })
            .collect(),
        state.coverage.clone(),
    )
}

#[pymethods]
impl NativeMotionPreview {
    fn schedule_motion_entry(
        &mut self,
        py: Python<'_>,
        group: &str,
        index: usize,
        time: f32,
    ) -> PyResult<()> {
        self.inner
            .schedule_motion_entry(group, index, time)
            .map_err(|error| failure(py, error, "schedule_motion_entry"))
    }
    fn set_seek_cache_budget(&mut self, bytes: usize) {
        self.inner.set_seek_cache_budget(bytes);
    }
    fn clear_seek_cache(&mut self) {
        self.inner.clear_seek_cache();
    }
    fn seek_cache_stats(&self) -> (usize, usize, usize, f32, u32) {
        let s = self.inner.seek_cache_stats();
        (
            s.budget_bytes,
            s.estimated_bytes,
            s.checkpoints,
            s.last_restored_time,
            s.last_replayed_steps,
        )
    }
    fn document_revision(&self) -> u64 {
        self.inner.document_revision()
    }
    fn snapshot(&self) -> MotionPreviewTuple {
        motion_snapshot(&self.inner)
    }
    fn operation_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.operation())
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }
    fn set_base_parameter(&mut self, py: Python<'_>, id: &str, value: f32) -> PyResult<()> {
        self.inner
            .set_base_parameter(id, value)
            .map_err(|error| failure(py, error, "set_base_parameter"))
    }
    fn schedule_motion(&mut self, py: Python<'_>, id: &str, time: f32) -> PyResult<()> {
        self.inner
            .schedule_motion(id, time)
            .map_err(|error| failure(py, error, "schedule_motion"))
    }
    fn schedule_expression(&mut self, py: Python<'_>, id: &str, time: f32) -> PyResult<()> {
        self.inner
            .schedule_expression(id, time)
            .map_err(|error| failure(py, error, "schedule_expression"))
    }
    fn schedule_parameter_input(
        &mut self,
        py: Python<'_>,
        id: &str,
        time: f32,
        value: f32,
    ) -> PyResult<()> {
        self.inner
            .schedule_parameter_input(id, time, value)
            .map_err(|error| failure(py, error, "schedule_parameter_input"))
    }
    fn stabilize_physics(&mut self) -> MotionPreviewTuple {
        self.inner.stabilize_physics();
        motion_snapshot(&self.inner)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }
    fn advance(&mut self, py: Python<'_>, dt: f32) -> PyResult<MotionPreviewTuple> {
        self.inner
            .advance(dt)
            .map_err(|error| failure(py, error, "advance"))?;
        Ok(motion_snapshot(&self.inner))
    }
    fn seek(&mut self, py: Python<'_>, time: f32) -> PyResult<MotionPreviewTuple> {
        self.inner
            .seek(time)
            .map_err(|error| failure(py, error, "seek"))?;
        Ok(motion_snapshot(&self.inner))
    }
    fn seek_with_progress(
        &mut self,
        py: Python<'_>,
        time: f32,
        progress: &Bound<'_, pyo3::types::PyAny>,
    ) -> PyResult<MotionPreviewTuple> {
        let mut callback_error = None;
        let result = self.inner.seek_with_progress(time, |completed, total| {
            match progress
                .call1((completed, total))
                .and_then(|value| value.extract::<bool>())
            {
                Ok(continue_seek) => continue_seek,
                Err(error) => {
                    callback_error = Some(error);
                    false
                }
            }
        });
        if let Some(error) = callback_error {
            return Err(error);
        }
        result.map_err(|error| failure(py, error, "seek_with_progress"))?;
        Ok(motion_snapshot(&self.inner))
    }
    fn frame(&self, py: Python<'_>) -> PyResult<EvaluationTuple> {
        self.inner
            .evaluate_drawables()
            .map(|frame| frame_tuple(&frame))
            .map_err(|error| failure(py, error, "frame"))
    }
}

type PreviewTuple = (f32, Vec<(String, f32)>, Vec<String>);

fn snapshot(preview: &ExpressionPreview) -> PreviewTuple {
    let state = preview.snapshot();
    (
        state.time,
        state
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect(),
        state.active_expressions.clone(),
    )
}

#[pymethods]
impl NativeExpressionPreview {
    fn document_revision(&self) -> u64 {
        self.inner.document_revision()
    }

    fn snapshot(&self) -> PreviewTuple {
        snapshot(&self.inner)
    }

    fn set_base_parameter(&mut self, py: Python<'_>, id: &str, value: f32) -> PyResult<()> {
        self.inner
            .set_base_parameter(id, value)
            .map_err(|error| failure(py, error, "set_base_parameter"))
    }

    fn schedule_expression(&mut self, py: Python<'_>, id: &str, time: f32) -> PyResult<()> {
        self.inner
            .schedule_expression(id, time)
            .map_err(|error| failure(py, error, "schedule_expression"))
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn advance(&mut self, py: Python<'_>, dt: f32) -> PyResult<PreviewTuple> {
        self.inner
            .advance(dt)
            .map_err(|error| failure(py, error, "advance"))?;
        Ok(snapshot(&self.inner))
    }

    fn seek(&mut self, py: Python<'_>, time: f32) -> PyResult<PreviewTuple> {
        self.inner
            .seek(time)
            .map_err(|error| failure(py, error, "seek"))?;
        Ok(snapshot(&self.inner))
    }

    fn frame(&self, py: Python<'_>) -> PyResult<EvaluationTuple> {
        self.inner
            .evaluate_drawables()
            .map(|frame| frame_tuple(&frame))
            .map_err(|error| failure(py, error, "frame"))
    }
}
