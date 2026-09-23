//! Structured SDK failures exposed to Python.
use crate::conversion::version_tuple;
use kasane_sdk::SdkError;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError};
use pyo3::prelude::*;

create_exception!(_native, SdkFailure, PyException);

pub(crate) fn sdk_failure(py: Python<'_>, error: SdkError) -> PyErr {
    let failure = SdkFailure::new_err(error.message.to_string());
    let value = failure.value(py);
    let _ = value.setattr("code", error.code.to_string());
    let _ = value.setattr("operation", error.operation);
    let _ = value.setattr("object_ids", error.object_ids);
    let _ = value.setattr("field_path", error.field_path.map(|path| path.to_string()));
    let _ = value.setattr(
        "expected_version",
        error
            .expected_version
            .map(|version| version_tuple(*version)),
    );
    let _ = value.setattr(
        "actual_version",
        error.actual_version.map(|version| version_tuple(*version)),
    );
    let _ = value.setattr("referrers", error.referrers.into_vec());
    failure
}

pub(crate) fn edit_failure(py: Python<'_>, code: &str, operation: &str, message: &str) -> PyErr {
    let failure = SdkFailure::new_err(message.to_owned());
    let value = failure.value(py);
    let _ = value.setattr("code", code);
    let _ = value.setattr("operation", operation);
    let _ = value.setattr("object_ids", Vec::<String>::new());
    let _ = value.setattr("field_path", py.None());
    let _ = value.setattr("expected_version", py.None());
    let _ = value.setattr("actual_version", py.None());
    let _ = value.setattr("referrers", Vec::<String>::new());
    failure
}

pub(crate) fn poisoned() -> PyErr {
    PyRuntimeError::new_err("Kasane session lock was poisoned")
}
