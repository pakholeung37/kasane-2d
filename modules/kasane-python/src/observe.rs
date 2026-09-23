//! Optional wgpu observer binding. GPU work runs after the session lock is released.
use std::collections::HashMap;
use std::sync::Mutex;

use kasane_sdk_observe::{ObservationError, ObservationInput, Observer, ObserverConfig};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::conversion::{version_tuple, VersionTuple};
use crate::error::poisoned;
use crate::session::NativeSession;

create_exception!(_native, ObservationFailure, PyException);

fn observation_failure(py: Python<'_>, error: ObservationError) -> PyErr {
    let failure = ObservationFailure::new_err(error.message);
    let value = failure.value(py);
    let _ = value.setattr("code", error.code);
    let _ = value.setattr("asset_id", error.asset_id);
    failure
}

type NativeFrameTuple = (
    (
        VersionTuple,
        String,
        u64,
        String,
        u64,
        Vec<(String, f32, f32, bool)>,
        (f32, f32, f32, f32, f32),
        f32,
        (f32, f32),
        Vec<(String, bool, Option<(u32, u32, u32, u32)>)>,
    ),
    u32,
    u32,
    Py<PyBytes>,
    Py<PyBytes>,
    Vec<(String, String, u64)>,
    String,
    String,
);

#[pyclass]
pub(crate) struct NativeObserver {
    inner: Mutex<Observer>,
}

#[pymethods]
impl NativeObserver {
    #[new]
    fn new(py: Python<'_>, width: u32, height: u32, fit_long_side: f32) -> PyResult<Self> {
        let config = ObserverConfig {
            width,
            height,
            fit_long_side,
        };
        let observer = py
            .detach(|| Observer::new(config))
            .map_err(|error| observation_failure(py, error))?;
        Ok(Self {
            inner: Mutex::new(observer),
        })
    }

    fn set_fit_long_side(&self, py: Python<'_>, value: f32) -> PyResult<()> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .set_fit_long_side(value)
            .map_err(|error| observation_failure(py, error))
    }

    fn observe(
        &self,
        py: Python<'_>,
        session: &NativeSession,
        values: HashMap<String, f32>,
    ) -> PyResult<NativeFrameTuple> {
        let session = session.inner.clone();
        let result = py.detach(|| {
            let input = {
                let session = session.lock().map_err(|_| None)?;
                ObservationInput::capture(&session, &values).map_err(Some)?
            };
            let mut observer = self.inner.lock().map_err(|_| None)?;
            let frame = observer.observe(&input).map_err(Some)?;
            let png = frame.png_bytes().map_err(Some)?;
            Ok((frame, png))
        });
        let (frame, png) = match result {
            Ok(value) => value,
            Err(Some(error)) => return Err(observation_failure(py, error)),
            Err(None) => return Err(poisoned()),
        };
        Ok((
            (
                version_tuple(frame.version),
                frame.input_sha256,
                frame.evaluation_revision,
                frame.document_id,
                frame.source_revision,
                frame.parameters,
                frame.canvas,
                frame.view_scale,
                frame.view_offset,
                frame
                    .drawable_bounds
                    .into_iter()
                    .map(|item| (item.id, item.visible, item.bounds))
                    .collect(),
            ),
            frame.width,
            frame.height,
            PyBytes::new(py, &frame.rgba).unbind(),
            PyBytes::new(py, &png).unbind(),
            frame
                .texture_revisions
                .into_iter()
                .map(|texture| (texture.asset_id, texture.sha256, texture.revision))
                .collect(),
            frame.adapter_name,
            frame.adapter_backend,
        ))
    }
}
