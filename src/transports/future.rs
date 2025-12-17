use pyo3::prelude::*;

/// Pure Rust completed future to avoid importing asyncio.Future
#[pyclass(module = "veloxloop._veloxloop")]
pub struct CompletedFuture {
    result: Py<PyAny>,
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
