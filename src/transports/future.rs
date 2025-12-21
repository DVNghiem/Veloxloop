use parking_lot::Mutex;
use pyo3::prelude::*;

enum FutureState {
    Pending,
    Finished(Py<PyAny>),
    Error(PyErr),
    Cancelled,
}

/// Pure Rust completed future to avoid importing asyncio.Future
#[pyclass(module = "veloxloop._veloxloop")]
pub struct CompletedFuture {
    result: Py<PyAny>,
}

/// Pure Rust pending future that can be resolved later
#[pyclass(module = "veloxloop._veloxloop")]
pub struct PendingFuture {
    state: Mutex<(FutureState, Vec<Py<PyAny>>)>,
}

#[pymethods]
impl PendingFuture {
    #[new]
    pub fn new() -> Self {
        Self {
            state: Mutex::new((FutureState::Pending, Vec::new())),
        }
    }

    fn __await__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let lock = self.state.lock();
        match &lock.0 {
            FutureState::Finished(result) => Err(pyo3::exceptions::PyStopIteration::new_err((
                result.clone_ref(py),
            ))),
            FutureState::Error(err) => Err(err.clone_ref(py)),
            FutureState::Cancelled => Err(pyo3::exceptions::PyRuntimeError::new_err("Cancelled")),
            FutureState::Pending => Ok(Some(py.None())),
        }
    }

    fn result(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let lock = self.state.lock();
        match &lock.0 {
            FutureState::Finished(res) => Ok(res.clone_ref(py)),
            FutureState::Error(err) => Err(err.clone_ref(py)),
            FutureState::Cancelled => Err(pyo3::exceptions::PyRuntimeError::new_err("Cancelled")),
            FutureState::Pending => Err(pyo3::exceptions::PyValueError::new_err(
                "Future is not done",
            )),
        }
    }

    fn done(&self) -> bool {
        !matches!(self.state.lock().0, FutureState::Pending)
    }

    pub fn set_result(&self, py: Python<'_>, result: Py<PyAny>) -> PyResult<()> {
        let mut lock = self.state.lock();
        if !matches!(lock.0, FutureState::Pending) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Future already done",
            ));
        }
        lock.0 = FutureState::Finished(result);

        // Call all done callbacks
        let callbacks = std::mem::take(&mut lock.1);
        drop(lock); // Drop lock before Python calls
        for callback in callbacks {
            let _ = callback.call1(py, (py.None(),));
        }

        Ok(())
    }

    pub fn set_exception(&self, py: Python<'_>, exception: Py<PyAny>) -> PyResult<()> {
        let mut lock = self.state.lock();
        if !matches!(lock.0, FutureState::Pending) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Future already done",
            ));
        }

        let err = PyErr::from_value(exception.into_bound(py));
        lock.0 = FutureState::Error(err);

        let callbacks = std::mem::take(&mut lock.1);
        drop(lock);
        for callback in callbacks {
            let _ = callback.call1(py, (py.None(),));
        }

        Ok(())
    }

    pub fn add_done_callback(&self, callback: Py<PyAny>) -> PyResult<()> {
        let mut lock = self.state.lock();
        if !matches!(lock.0, FutureState::Pending) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot add callback to completed future",
            ));
        }
        lock.1.push(callback);
        Ok(())
    }

    pub fn cancel(&self, py: Python<'_>) -> PyResult<bool> {
        let mut lock = self.state.lock();
        if !matches!(lock.0, FutureState::Pending) {
            return Ok(false);
        }
        lock.0 = FutureState::Cancelled;
        let callbacks = std::mem::take(&mut lock.1);
        drop(lock);
        for callback in callbacks {
            let _ = callback.call1(py, (py.None(),));
        }
        Ok(true)
    }
}

#[pymethods]
impl CompletedFuture {
    fn __await__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        // Return self as an iterator - already completed
        slf
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        // Iterator is exhausted, raise StopIteration with result
        Err(pyo3::exceptions::PyStopIteration::new_err((self
            .result
            .clone_ref(py),)))
    }

    fn result(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.result.clone_ref(py))
    }

    fn done(&self) -> bool {
        true
    }
}

impl CompletedFuture {
    pub fn new(result: Py<PyAny>) -> Self {
        Self { result }
    }
}
