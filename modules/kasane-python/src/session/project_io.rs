use super::*;

#[pymethods]
impl NativeSession {
    #[pyo3(signature = (path, destination, expected_version=None))]
    fn import_psd(
        &self,
        py: Python<'_>,
        path: String,
        destination: String,
        expected_version: Option<VersionTuple>,
    ) -> PyResult<PsdImportTuple> {
        self.ensure_idle(py, "import_psd")?;
        let active = self.active_edit.clone();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("import_psd")));
            }
            Ok::<_, ()>(session.import_psd(
                Path::new(&path),
                Path::new(&destination),
                expected_version.map(tuple_version),
            ))
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                version_tuple(receipt.after),
                receipt.manifest.to_string_lossy().into_owned(),
                receipt.report.width,
                receipt.report.height,
                receipt.report.raster_layers,
                receipt.report.groups,
                receipt.project.durable,
                receipt.project.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (path, expected_version=None))]
    fn import_model3(
        &self,
        py: Python<'_>,
        path: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<ImportTuple> {
        self.ensure_idle(py, "import_model3")?;
        let active = self.active_edit.clone();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("import_model3")));
            }
            Ok::<_, ()>(
                session.import_model3(Path::new(&path), expected_version.map(tuple_version)),
            )
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                version_tuple(receipt.after),
                receipt.report.moc_version,
                receipt
                    .project
                    .diagnostics
                    .into_iter()
                    .map(|d| (d.asset_id, d.code, d.message))
                    .collect(),
                receipt.project.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (path, texture_map, expected_version=None))]
    fn import_bare_moc3(
        &self,
        py: Python<'_>,
        path: String,
        texture_map: HashMap<usize, String>,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<ImportTuple> {
        self.ensure_idle(py, "import_bare_moc3")?;
        let active = self.active_edit.clone();
        let texture_map: HashMap<_, _> = texture_map
            .into_iter()
            .map(|(slot, path)| (slot, PathBuf::from(path)))
            .collect();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("import_bare_moc3")));
            }
            Ok::<_, ()>(session.import_bare_moc3(
                Path::new(&path),
                &texture_map,
                expected_version.map(tuple_version),
            ))
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                version_tuple(receipt.after),
                receipt.report.moc_version,
                receipt
                    .project
                    .diagnostics
                    .into_iter()
                    .map(|d| (d.asset_id, d.code, d.message))
                    .collect(),
                receipt.project.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (destination, expected_version=None))]
    fn export_package(
        &self,
        py: Python<'_>,
        destination: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<(bool, bool, Vec<String>)> {
        self.ensure_idle(py, "export_package")?;
        let active = self.active_edit.clone();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("export_package")));
            }
            Ok::<_, ()>(
                session
                    .export_package(Path::new(&destination), expected_version.map(tuple_version)),
            )
        });
        match result {
            Ok(Ok(publication)) => Ok((
                publication.published,
                publication.durable,
                publication.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (path, expected_version=None))]
    fn save(
        &self,
        py: Python<'_>,
        path: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<(String, bool, Vec<String>, Vec<String>)> {
        self.ensure_idle(py, "save")?;
        let active = self.active_edit.clone();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            if active.load(Ordering::Acquire) {
                return Ok(Err(active_edit_error("save")));
            }
            Ok::<_, ()>(session.save_project(Path::new(&path), expected_version.map(tuple_version)))
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                receipt.manifest.to_string_lossy().into_owned(),
                receipt.durable,
                receipt.warnings,
                receipt.history_warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }
}
