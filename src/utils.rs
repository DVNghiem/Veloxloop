use pyo3::prelude::*;
use pyo3::exceptions::{PyOSError, PyValueError, PyRuntimeError};
use std::io;

pub type VeloxResult<T> = Result<T, VeloxError>;

#[derive(thiserror::Error, Debug)]
pub enum VeloxError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Python Error: {0}")]
    Python(#[from] PyErr),
    #[error("Value Error: {0}")]
    ValueError(String),
    #[error("Runtime Error: {0}")]
    RuntimeError(String),
}

impl From<VeloxError> for PyErr {
    fn from(err: VeloxError) -> PyErr {
        match err {
            VeloxError::Io(e) => PyOSError::new_err(e.to_string()),
            VeloxError::Python(e) => e,
            VeloxError::ValueError(s) => PyValueError::new_err(s),
            VeloxError::RuntimeError(s) => PyRuntimeError::new_err(s),
        }
    }
}
