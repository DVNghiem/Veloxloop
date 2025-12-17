use pyo3::prelude::*;
use parking_lot::Mutex;

/// Pure Rust completed future to avoid importing asyncio.Future
#[pyclass(module = "veloxloop._veloxloop")]
pub struct CompletedFuture {
    result: Py<PyAny>,
}

/// Pure Rust pending future that can be resolved later
#[pyclass(module = "veloxloop._veloxloop")]
pub struct PendingFuture {
    result: Mutex<Option<Py<PyAny>>>,
    exception: Mutex<Option<PyErr>>,
    callbacks: Mutex<Vec<Py<PyAny>>>,
}

#[pymethods]
impl PendingFuture {
    #[new]
    pub fn new() -> Self {
        Self {
            result: Mutex::new(None),
            exception: Mutex::new(None),
            callbacks: Mutex::new(Vec::new()),
        }
    }

    fn __await__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        // Check if we have a result or exception
        if let Some(exc) = self.exception.lock().as_ref() {
            return Err(exc.clone_ref(py));
        }
        
        if let Some(result) = self.result.lock().as_ref() {
            // Raise StopIteration with result
            return Err(pyo3::exceptions::PyStopIteration::new_err((result.clone_ref(py),)));
        }
        
        // Not ready yet, yield None
        Ok(Some(py.None()))
    }
    
    fn result(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(exc) = self.exception.lock().as_ref() {
            return Err(exc.clone_ref(py));
        }
        
        if let Some(result) = self.result.lock().as_ref() {
            return Ok(result.clone_ref(py));
        }
        
        Err(pyo3::exceptions::PyValueError::new_err("Future is not done"))
    }
    
    fn done(&self) -> bool {
        self.result.lock().is_some() || self.exception.lock().is_some()
    }
    
    pub fn set_result(&self, py: Python<'_>, result: Py<PyAny>) -> PyResult<()> {
        if self.done() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Future already done"));
        }
        *self.result.lock() = Some(result);
        
        // Call all done callbacks
        let callbacks = std::mem::take(&mut *self.callbacks.lock());
        for callback in callbacks {
            let _ = callback.call1(py, (py.None(),)); // Pass self as argument, but we use None for simplicity
        }
        
        Ok(())
    }
    
    pub fn set_exception(&self, py: Python<'_>, exception: Py<PyAny>) -> PyResult<()> {
        if self.done() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Future already done"));
        }
        
        // Convert Python exception to PyErr
        let err = PyErr::from_value(exception.into_bound(py));
        *self.exception.lock() = Some(err);
        
        // Call all done callbacks
        let callbacks = std::mem::take(&mut *self.callbacks.lock());
        for callback in callbacks {
            let _ = callback.call1(py, (py.None(),));
        }
        
        Ok(())
    }
    
    pub fn add_done_callback(&self, callback: Py<PyAny>) -> PyResult<()> {
        if self.done() {
            // If already done, call callback immediately in a safe way
            // We can't call it here because we don't have a Python context
            // So we add it to the list and it will be called on next access
            // Or we can require Python context
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot add callback to completed future - feature not yet implemented"
            ));
        }
        self.callbacks.lock().push(callback);
        Ok(())
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
        Err(pyo3::exceptions::PyStopIteration::new_err((self.result.clone_ref(py),)))
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
