use crate::event_loop::VeloxLoop;
use pyo3::prelude::*;
use std::cell::RefCell;

thread_local! {
    static CURRENT_LOOP: RefCell<Option<Py<PyAny>>> = RefCell::new(None);
}

#[pyclass(module = "veloxloop", subclass)]
pub struct VeloxLoopPolicy {}

#[pymethods]
impl VeloxLoopPolicy {
    #[new]
    fn new() -> Self {
        Self {}
    }

    fn get_event_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        CURRENT_LOOP.with(|cell| {
            if let Some(loop_) = cell.borrow().as_ref() {
                Ok(loop_.clone_ref(py))
            } else {
                Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "There is no current event loop in thread 'VeloxLoopPolicy'.",
                ))
            }
        })
    }

    fn set_event_loop(&self, loop_: Option<Py<PyAny>>) -> PyResult<()> {
        CURRENT_LOOP.with(|cell| {
            *cell.borrow_mut() = loop_;
        });
        Ok(())
    }

    fn new_event_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let loop_instance = VeloxLoop::new(None)?;
        Ok(Py::new(py, loop_instance)?.into())
    }
}
