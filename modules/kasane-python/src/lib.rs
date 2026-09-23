//! CPython bridge for the Kasane authoring SDK.
mod conversion;
mod edit;
mod error;
mod handle;
mod session;

use edit::NativeEdit;
use error::SdkFailure;
use handle::NativeHandle;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use session::NativeSession;

#[pyfunction]
fn capabilities(py: Python<'_>) -> PyResult<Py<PyDict>> {
    let result = PyDict::new(py);
    result.set_item("project_io", true)?;
    result.set_item("model3_import", true)?;
    result.set_item("bare_moc3_import", true)?;
    result.set_item("moc3_export", true)?;
    result.set_item("official_core_validation", kasane_moc3::HAS_CORE_VALIDATION)?;
    result.set_item("gpu_observation", false)?;
    Ok(result.unbind())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("SdkFailure", module.py().get_type::<SdkFailure>())?;
    module.add_class::<NativeSession>()?;
    module.add_class::<NativeEdit>()?;
    module.add_class::<NativeHandle>()?;
    module.add_function(wrap_pyfunction!(capabilities, module)?)?;
    Ok(())
}
