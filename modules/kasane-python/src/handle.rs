//! Opaque object identity for Python callers.
use crate::conversion::object_kind_name;
use kasane_sdk::ObjectHandle;
use pyo3::prelude::*;

#[pyclass(name = "ObjectHandle")]
pub(crate) struct NativeHandle {
    pub(crate) inner: ObjectHandle,
}

#[pymethods]
impl NativeHandle {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }

    #[getter]
    fn kind(&self) -> &'static str {
        object_kind_name(self.inner.kind())
    }
}
