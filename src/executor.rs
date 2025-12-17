use pyo3::prelude::*;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// A thread pool executor for running blocking code
pub struct ThreadPoolExecutor {
    runtime: Arc<Runtime>,
}

impl ThreadPoolExecutor {
    /// Create a new thread pool executor
    pub fn new() -> PyResult<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to create thread pool: {}", e)
            ))?;
        
        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Spawn a blocking task on the thread pool
    pub fn spawn_blocking<F, R>(&self, f: F) -> tokio::task::JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.runtime.spawn_blocking(f)
    }
}

impl Default for ThreadPoolExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default thread pool executor")
    }
}
