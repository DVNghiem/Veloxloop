use pyo3::prelude::*;

/// Trait defining the common interface for Veloxloop transports.
/// This allows different implementations (TCP, UDS, etc.) to share logic.
pub trait Transport {
    fn write(&mut self, data: &[u8]) -> PyResult<()>;
    fn is_closing(&self) -> bool;
    fn close(&mut self) -> PyResult<()>;
    // Add other common methods like pause_reading, resume_reading
}

pub mod tcp;
