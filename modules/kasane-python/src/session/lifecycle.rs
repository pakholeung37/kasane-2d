use super::*;

#[pymethods]
impl NativeSession {
    #[new]
    fn new(
        py: Python<'_>,
        document_id: &str,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
    ) -> PyResult<Self> {
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let session =
            AuthoringSession::new(document_id, canvas).map_err(|error| sdk_failure(py, error))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            active_edit: Arc::new(AtomicBool::new(false)),
        })
    }

    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    fn with_history_limits(
        py: Python<'_>,
        document_id: &str,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
        max_steps: usize,
        max_bytes: usize,
    ) -> PyResult<Self> {
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let session = AuthoringSession::with_history_limits(
            document_id,
            canvas,
            HistoryLimits {
                max_steps,
                max_bytes,
            },
        )
        .map_err(|error| sdk_failure(py, error))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            active_edit: Arc::new(AtomicBool::new(false)),
        })
    }

    #[staticmethod]
    fn open(py: Python<'_>, path: String) -> PyResult<Self> {
        let result = py.detach(|| {
            let mut session = AuthoringSession::new(
                "00000000-0000-4000-8000-000000000001",
                Canvas::new(1.0, 1.0, Vec2::new(0.0, 0.0), 1.0),
            )?;
            session.open_project(Path::new(&path), None)?;
            Ok::<_, SdkError>(session)
        });
        Ok(Self {
            inner: Arc::new(Mutex::new(result.map_err(|error| sdk_failure(py, error))?)),
            active_edit: Arc::new(AtomicBool::new(false)),
        })
    }

    fn version(&self) -> PyResult<(u64, u64, u64)> {
        Ok(version_tuple(
            self.inner.lock().map_err(|_| poisoned())?.version(),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (document_id, width, height, origin_x, origin_y, pixels_per_unit, expected_version=None))]
    fn new_project(
        &self,
        py: Python<'_>,
        document_id: String,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
        expected_version: Option<VersionTuple>,
    ) -> PyResult<VersionTuple> {
        self.ensure_idle(py, "new_project")?;
        let active = self.active_edit.clone();
        let session = self.inner.clone();
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("new_project")));
            }
            Ok::<_, ()>(session.new_project(
                &document_id,
                canvas,
                expected_version.map(tuple_version),
            ))
        });
        match result {
            Ok(Ok(version)) => Ok(version_tuple(version)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn document_id(&self) -> PyResult<String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .document_id()
            .to_owned())
    }

    fn canvas(&self) -> PyResult<(f32, f32, f32, f32, f32)> {
        let canvas = self.inner.lock().map_err(|_| poisoned())?.canvas();
        Ok((
            canvas.width,
            canvas.height,
            canvas.origin.x,
            canvas.origin.y,
            canvas.pixels_per_unit,
        ))
    }

    fn draw_order_groups(&self) -> PyResult<Option<Vec<DrawOrderTuple>>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .draw_order_groups()
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|g| (g.owner, g.items, g.min_order, g.max_order))
                    .collect()
            }))
    }

    fn evaluation_revision(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .evaluation_revision())
    }

    fn modified(&self) -> PyResult<bool> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.modified())
    }

    fn project_path(&self) -> PyResult<Option<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .project_path()
            .map(|path| path.to_string_lossy().into_owned()))
    }
}
