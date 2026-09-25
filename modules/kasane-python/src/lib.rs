//! CPython bridge for the Kasane authoring SDK.
mod animation;
mod conversion;
mod edit;
mod error;
mod geometry;
mod handle;
#[cfg(feature = "observe")]
mod observe;
mod session;

use animation::{NativeExpressionPreview, NativeMotionPreview, NativePhysicsPreview};
use edit::NativeEdit;
use error::SdkFailure;
use handle::NativeHandle;
#[cfg(feature = "observe")]
use observe::{NativeObserver, ObservationFailure};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use session::NativeSession;

/// Report available project IO, validation, and GPU observation capabilities.
#[pyfunction]
fn capabilities(py: Python<'_>) -> PyResult<Py<PyDict>> {
    let result = PyDict::new(py);
    result.set_item("project_io", true)?;
    result.set_item("model3_import", true)?;
    result.set_item("bare_moc3_import", true)?;
    result.set_item("psd_import", true)?;
    result.set_item("moc3_export", true)?;
    result.set_item("purism_core_validation", kasane_moc3::HAS_CORE_VALIDATION)?;
    result.set_item("official_core_validation", false)?;
    #[cfg(feature = "observe")]
    let gpu_observation = kasane_sdk_observe::Observer::new(kasane_sdk_observe::ObserverConfig {
        width: 1,
        height: 1,
        fit_long_side: 1.0,
    })
    .is_ok();
    #[cfg(not(feature = "observe"))]
    let gpu_observation = false;
    result.set_item("gpu_observation", gpu_observation)?;
    Ok(result.unbind())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("SdkFailure", module.py().get_type::<SdkFailure>())?;
    module.add_class::<NativeSession>()?;
    module.add_class::<NativeEdit>()?;
    module.add_class::<NativeExpressionPreview>()?;
    module.add_class::<NativeMotionPreview>()?;
    module.add_class::<NativePhysicsPreview>()?;
    module.add_class::<NativeHandle>()?;
    #[cfg(feature = "observe")]
    {
        module.add(
            "ObservationFailure",
            module.py().get_type::<ObservationFailure>(),
        )?;
        module.add_class::<NativeObserver>()?;
    }
    module.add_function(wrap_pyfunction!(capabilities, module)?)?;
    module.add_function(wrap_pyfunction!(geometry::rectangle_grid_geometry, module)?)?;
    Ok(())
}
