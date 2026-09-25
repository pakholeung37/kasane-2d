//! Optional wgpu observer binding. GPU work runs after the session lock is released.
use std::collections::HashMap;
use std::sync::Mutex;

use kasane_sdk_observe::{
    CanvasRoi, ObservationError, ObservationInput, ObservedFrame, Observer, ObserverConfig,
    RenderRequest, ResolvedObservation,
};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::animation::NativeMotionPreview;
use crate::conversion::{version_tuple, VersionTuple};
use crate::error::poisoned;
use crate::session::NativeSession;

create_exception!(
    _native,
    ObservationFailure,
    PyException,
    "GPU observation failure with code and optional texture asset ID."
);

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

type RenderMappingTuple = (
    (f32, f32, f32, f32),
    (f32, f32, f32, f32),
    (f32, f32, f32, f32),
);

fn frame_tuple(py: Python<'_>, frame: ObservedFrame) -> PyResult<NativeFrameTuple> {
    let png = frame
        .png_bytes()
        .map_err(|error| observation_failure(py, error))?;
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

#[pyclass]
pub(crate) struct NativeCapturedScene {
    inner: ResolvedObservation,
}

#[pymethods]
impl NativeCapturedScene {
    #[getter]
    fn capture_id(&self) -> &str {
        self.inner.capture_id()
    }

    #[getter]
    fn scene_digest(&self) -> &str {
        self.inner.scene_digest()
    }

    fn authoring_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.input().authoring())
            .map_err(|error| PyException::new_err(error.to_string()))
    }

    fn source_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.input().source())
            .map_err(|error| PyException::new_err(error.to_string()))
    }

    fn metadata_json(&self) -> PyResult<String> {
        let input = self.inner.input();
        let version = input.version();
        serde_json::to_string(&serde_json::json!({
            "capture_id": self.inner.capture_id(),
            "scene_digest": self.inner.scene_digest(),
            "document_id": input.document_id(),
            "version": [version.session_id, version.generation, version.revision],
            "evaluation_revision": input.evaluation_revision(),
            "snapshot_clone_ns": input.snapshot_clone_ns(),
            "source_revision": input.frame().source_revision,
            "requested": input.requested(),
            "textures": self.inner.textures().iter().map(|texture| &texture.asset).collect::<Vec<_>>(),
        }))
        .map_err(|error| PyException::new_err(error.to_string()))
    }

    fn evaluated_frame_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.input().frame())
            .map_err(|error| PyException::new_err(error.to_string()))
    }

    #[staticmethod]
    fn open_scene(py: Python<'_>, absolute_directory: &str) -> PyResult<Self> {
        let scene = py
            .detach(|| ResolvedObservation::open_scene(std::path::Path::new(absolute_directory)))
            .map_err(|error| observation_failure(py, error))?;
        Ok(Self { inner: scene })
    }

    fn save_scene(&self, py: Python<'_>, absolute_directory: &str) -> PyResult<()> {
        py.detach(|| {
            self.inner
                .save_scene(std::path::Path::new(absolute_directory))
        })
        .map_err(|error| observation_failure(py, error))
    }

    fn render(
        &self,
        py: Python<'_>,
        observer: &NativeObserver,
        width: u32,
        height: u32,
        roi: (f32, f32, f32, f32),
        padding_canvas: f32,
    ) -> PyResult<(NativeFrameTuple, RenderMappingTuple, String)> {
        let request = RenderRequest {
            width,
            height,
            roi: CanvasRoi {
                x0: roi.0,
                y0: roi.1,
                x1: roi.2,
                y1: roi.3,
            },
            padding_canvas,
        };
        let render_digest = self
            .inner
            .render_digest(request)
            .map_err(|error| observation_failure(py, error))?;
        let frame = py.detach(|| {
            observer
                .inner
                .lock()
                .map_err(|_| None)?
                .render(&self.inner, request)
                .map_err(Some)
        });
        let frame = match frame {
            Ok(frame) => frame,
            Err(Some(error)) => return Err(observation_failure(py, error)),
            Err(None) => return Err(poisoned()),
        };
        let mapping = frame
            .explicit_view
            .expect("explicit render has an ROI mapping");
        let roi_tuple = |roi: CanvasRoi| (roi.x0, roi.y0, roi.x1, roi.y1);
        Ok((
            frame_tuple(py, frame)?,
            (
                roi_tuple(mapping.requested_roi),
                roi_tuple(mapping.padded_roi),
                roi_tuple(mapping.visible_roi),
            ),
            render_digest,
        ))
    }
}

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

    #[getter]
    fn texture_mipmaps(&self) -> bool {
        cfg!(feature = "framework-texture-filtering")
    }

    fn set_fit_long_side(&self, py: Python<'_>, value: f32) -> PyResult<()> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .set_fit_long_side(value)
            .map_err(|error| observation_failure(py, error))
    }

    fn capture_scene(
        &self,
        py: Python<'_>,
        session: &NativeSession,
        values: HashMap<String, f32>,
    ) -> PyResult<NativeCapturedScene> {
        let session = session.inner.clone();
        let result = py.detach(|| {
            let snapshot = {
                let session = session.lock().map_err(|_| None)?;
                session.read_snapshot()
            };
            let requested =
                ObservationInput::resolve_requested(&snapshot, &values).map_err(Some)?;
            let input =
                ObservationInput::capture_from_snapshot(&snapshot, &requested).map_err(Some)?;
            ResolvedObservation::capture(input).map_err(Some)
        });
        match result {
            Ok(inner) => Ok(NativeCapturedScene { inner }),
            Err(Some(error)) => Err(observation_failure(py, error)),
            Err(None) => Err(poisoned()),
        }
    }

    fn capture_scenes(
        &self,
        py: Python<'_>,
        session: &NativeSession,
        samples: Vec<HashMap<String, f32>>,
    ) -> PyResult<Vec<Py<NativeCapturedScene>>> {
        let session = session.inner.clone();
        let result = py.detach(|| {
            let snapshot = {
                let session = session.lock().map_err(|_| None)?;
                session.read_snapshot()
            };
            let requested = samples
                .iter()
                .map(|values| ObservationInput::resolve_requested(&snapshot, values))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Some)?;
            let inputs = ObservationInput::capture_samples(&snapshot, &requested).map_err(Some)?;
            ResolvedObservation::capture_many(inputs).map_err(Some)
        });
        let scenes = match result {
            Ok(scenes) => scenes,
            Err(Some(error)) => return Err(observation_failure(py, error)),
            Err(None) => return Err(poisoned()),
        };
        scenes
            .into_iter()
            .map(|inner| Py::new(py, NativeCapturedScene { inner }))
            .collect()
    }

    fn capture_animation_scene(
        &self,
        py: Python<'_>,
        session: &NativeSession,
        preview: &NativeMotionPreview,
        apply_model_opacity: bool,
    ) -> PyResult<NativeCapturedScene> {
        let session = session.inner.clone();
        let result = py.detach(|| {
            let snapshot = {
                let session = session.lock().map_err(|_| None)?;
                session.read_snapshot()
            };
            let input = ObservationInput::capture_motion_from_snapshot(
                &snapshot,
                preview.inner(),
                apply_model_opacity,
            )
            .map_err(Some)?;
            ResolvedObservation::capture(input).map_err(Some)
        });
        match result {
            Ok(inner) => Ok(NativeCapturedScene { inner }),
            Err(Some(error)) => Err(observation_failure(py, error)),
            Err(None) => Err(poisoned()),
        }
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
            observer.observe(&input).map_err(Some)
        });
        let frame = match result {
            Ok(frame) => frame,
            Err(Some(error)) => return Err(observation_failure(py, error)),
            Err(None) => return Err(poisoned()),
        };
        frame_tuple(py, frame)
    }
}
