//! Opaque object identity for Python callers.
use crate::conversion::object_kind_name;
use kasane_sdk::ObjectHandle;
use pyo3::prelude::*;

/// Opaque object identity; validate it after edits with Session.resolve_handle.
#[pyclass(name = "ObjectHandle")]
pub(crate) struct NativeHandle {
    pub(crate) inner: ObjectHandle,
}

#[pymethods]
impl NativeHandle {
    /// Canonical ID of the referenced object.
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }

    /// Kind of the referenced SDK object.
    #[getter]
    fn kind(&self) -> &'static str {
        object_kind_name(self.inner.kind())
    }
}
