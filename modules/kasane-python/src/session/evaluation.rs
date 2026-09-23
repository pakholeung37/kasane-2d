use super::*;

#[pymethods]
impl NativeSession {
    fn evaluate(&self, py: Python<'_>, values: HashMap<String, f32>) -> PyResult<EvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.evaluate(&values))
        });
        match result {
            Ok(Ok(frame)) => Ok(frame_tuple(&frame)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn evaluate_snapshot(
        &self,
        py: Python<'_>,
        values: HashMap<String, f32>,
    ) -> PyResult<FullEvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(
                session
                    .evaluate(&values)
                    .map(|frame| full_frame_tuple(&frame, session.version())),
            )
        });
        match result {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn preview_values(&self) -> PyResult<HashMap<String, f32>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_values()
            .clone())
    }

    fn preview_revision(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_revision())
    }

    fn preview_evaluation_count(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_evaluation_count())
    }

    fn preview_frame(&self, py: Python<'_>) -> PyResult<EvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.preview_frame().map(|frame| frame_tuple(&frame)))
        });
        match result {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn preview_snapshot(&self, py: Python<'_>) -> PyResult<FullEvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            let version = session.version();
            Ok::<_, ()>(
                session
                    .preview_frame()
                    .map(|frame| full_frame_tuple(&frame, version)),
            )
        });
        match result {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn set_preview_values(&self, py: Python<'_>, values: HashMap<String, f32>) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.set_preview_values(values))
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn set_preview_parameter(&self, py: Python<'_>, id: String, value: f32) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.set_preview_parameter(&id, value))
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn reset_preview_values(&self, py: Python<'_>) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.reset_preview_values())
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }
}
