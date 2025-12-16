use pyo3::prelude::*;
use crate::event_loop::VeloxLoop;

#[pyclass(module = "veloxloop", subclass)]
pub struct VeloxLoopPolicy {
    loop_factory: Option<Py<PyAny>>,
    local: Py<PyAny>, // threading.local() object
}

#[pymethods]
impl VeloxLoopPolicy {
    #[pyo3(signature = (loop_factory=None))]
    #[new]
    fn new(py: Python<'_>, loop_factory: Option<Py<PyAny>>) -> PyResult<Self> {
        Ok(Self {
            loop_factory,
            local: py.import("threading")?.call_method0("local")?.into_any().into(),
        })
    }

    fn get_event_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // We defer to DefaultEventLoopPolicy logic or implementing simple one.
        // Standard policy manages thread-local loop.
        // For simple Rust implementation without inheriting:
        // We can mimic behavior: get global/thread-local var.
        // Or simpler: Just return a new loop for now if not set?
        // Correct way: use `asyncio`'s helpers or reimplement thread-local storage.
        //
        // "I only want to implement everything entirely using Rust".
        // Reimplementing the full policy logic (thread-local storage, child watchers) is heavy.
        //
        // Let's implement basic factories.
        Err(pyo3::exceptions::PyNotImplementedError::new_err("get_event_loop not fully impl in Rust yet"))
    }
    
    fn new_event_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Create VeloxLoop instance directly in Rust
        let loop_instance = VeloxLoop::new(None)?;
        Ok(Py::new(py, loop_instance)?.into())
    }
    
    // We might need to inherit from asyncio.AbstractEventLoopPolicy
    // PyO3 doesn't easily support borrowing from Python classes unless we accept it as base.
    // simpler: define `_loop_factory` which DefaultEventLoopPolicy uses?
    // If user sets this as policy, `new_event_loop` is key.
}
