#[allow(unused)]
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::Arc;
use parking_lot::Mutex;
use memchr::memchr;
use crate::transports::future::PendingFuture;

#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamReader {
    /// Internal buffer for read data
    buffer: Arc<Mutex<Vec<u8>>>,
    /// Maximum buffer size before pausing
    limit: usize,
    /// EOF flag
    eof: Arc<Mutex<bool>>,
    /// Exception to raise on next read (if any)
    exception: Arc<Mutex<Option<String>>>,
    /// Pending futures waiting for data
    waiters: Arc<Mutex<Vec<(WaiterType, Py<PendingFuture>)>>>,
}

#[derive(Clone)]
enum WaiterType {
    ReadLine,
    ReadUntil(Vec<u8>),
    ReadExactly(usize),
}

#[pymethods]
impl StreamReader {
    #[new]
    #[pyo3(signature = (limit=None))]
    pub fn new(limit: Option<usize>) -> Self {
        const DEFAULT_LIMIT: usize = 64 * 1024; // 64 KB default
        
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            limit: limit.unwrap_or(DEFAULT_LIMIT),
            eof: Arc::new(Mutex::new(false)),
            exception: Arc::new(Mutex::new(None)),
            waiters: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Feed data into the buffer and wake up waiters
    pub fn feed_data(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if data.is_empty() {
            return Ok(());
        }
        
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(data);
        drop(buffer);
        
        // Try to satisfy waiting futures
        self._wakeup_waiters(py)?;
        Ok(())
    }

    /// Signal EOF and wake up all waiters
    pub fn feed_eof(&self, py: Python<'_>) -> PyResult<()> {
        *self.eof.lock() = true;
        self._wakeup_waiters(py)?;
        Ok(())
    }
    
    /// Internal method to wake up waiting futures
    fn _wakeup_waiters(&self, py: Python<'_>) -> PyResult<()> {
        let mut waiters = self.waiters.lock();
        let mut i = 0;
        
        while i < waiters.len() {
            let (waiter_type, future) = &waiters[i];
            
            // Try to fulfill this waiter
            let result = match waiter_type {
                WaiterType::ReadLine => self._try_readline(py),
                WaiterType::ReadUntil(sep) => self._try_readuntil(py, sep),
                WaiterType::ReadExactly(n) => self._try_readexactly(py, *n),
            };
            
            match result {
                Ok(Some(data)) => {
                    // We have data, set the future result
                    future.bind(py).borrow().set_result(py, data)?;
                    waiters.remove(i);
                    // Don't increment i since we removed an element
                }
                Ok(None) => {
                    // Not ready yet
                    i += 1;
                }
                Err(e) => {
                    // Error occurred, set exception on future
                    let exc = e.value(py).clone().into_any().unbind();
                    future.bind(py).borrow().set_exception(py, exc)?;
                    waiters.remove(i);
                }
            }
        }
        
        Ok(())
    }
    
    /// Try to read a line without blocking (returns None if not available)
    fn _try_readline(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self._try_readuntil(py, b"\n")
    }
    
    /// Try to read until separator without blocking
    /// Optimized with SIMD-accelerated search for single-byte separators
    fn _try_readuntil(&self, py: Python<'_>, separator: &[u8]) -> PyResult<Option<Py<PyAny>>> {
        // Check for exception first
        if let Some(exc_msg) = self.exception.lock().as_ref() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(exc_msg.clone()));
        }
        
        let mut buffer = self.buffer.lock();
        
        // Find the separator - use SIMD-accelerated memchr for single-byte (common case: newline)
        let pos = if separator.len() == 1 {
            memchr(separator[0], &buffer)
        } else {
            // Multi-byte separator: use windows iterator
            buffer.windows(separator.len())
                .position(|window| window == separator)
        };
        
        if let Some(pos) = pos {
            let end = pos + separator.len();
            // Create PyBytes directly from slice to avoid intermediate Vec allocation
            let bytes = PyBytes::new(py, &buffer[..end]);
            buffer.drain(..end);
            return Ok(Some(bytes.into()));
        }
        
        // Separator not found - check EOF while still holding buffer lock
        let is_eof = *self.eof.lock();
        if is_eof {
            // Return all data if at EOF
            if buffer.is_empty() {
                return Ok(Some(PyBytes::new(py, &[]).into()));
            }
            let bytes = PyBytes::new(py, &buffer);
            buffer.clear();
            return Ok(Some(bytes.into()));
        }
        
        // Not ready yet
        Ok(None)
    }
    
    /// Try to read exactly n bytes without blocking
    fn _try_readexactly(&self, py: Python<'_>, n: usize) -> PyResult<Option<Py<PyAny>>> {
        // Check for exception first
        if let Some(exc_msg) = self.exception.lock().as_ref() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(exc_msg.clone()));
        }
        
        let mut buffer = self.buffer.lock();
        
        if buffer.len() >= n {
            // Create PyBytes directly from slice to avoid intermediate allocation
            let bytes = PyBytes::new(py, &buffer[..n]);
            buffer.drain(..n);
            return Ok(Some(bytes.into()));
        }
        
        // Check EOF while still holding buffer lock
        let is_eof = *self.eof.lock();
        if is_eof {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Not enough data: expected {}, got {}", n, buffer.len())
            ));
        }
        
        // Not ready yet
        Ok(None)
    }

    /// Set an exception message to be raised on next read
    pub fn set_exception(&self, message: String) -> PyResult<()> {
        *self.exception.lock() = Some(message);
        Ok(())
    }

    /// Get the current exception message (if any)
    pub fn exception(&self) -> Option<String> {
        self.exception.lock().clone()
    }

    /// Check if at EOF
    pub fn at_eof(&self) -> bool {
        *self.eof.lock() && self.buffer.lock().is_empty()
    }

    /// Read up to n bytes
    #[pyo3(signature = (n=-1))]
    pub fn read(&self, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        // Check for exception
        if let Some(exc_msg) = self.exception.lock().take() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(exc_msg));
        }
        
        let mut buffer = self.buffer.lock();
        
        if n < 0 {
            // Read all available data
            let data = buffer.drain(..).collect::<Vec<u8>>();
            let bytes = PyBytes::new(py, &data);
            return Ok(bytes.into());
        }
        
        let n = n as usize;
        let available = buffer.len().min(n);
        let data = buffer.drain(..available).collect::<Vec<u8>>();
        let bytes = PyBytes::new(py, &data);
        
        Ok(bytes.into())
    }

    /// Read exactly n bytes (async - returns a future)
    pub fn readexactly(&self, py: Python<'_>, n: usize) -> PyResult<Py<PyAny>> {
        // Try to get data immediately
        match self._try_readexactly(py, n)? {
            Some(data) => Ok(data),
            None => {
                // Create a pending future
                let future = Py::new(py, PendingFuture::new())?;
                self.waiters.lock().push((WaiterType::ReadExactly(n), future.clone_ref(py)));
                Ok(future.into_any())
            }
        }
    }

    /// Read until delimiter is found (async - returns a future)
    #[pyo3(signature = (separator=b"\n".as_slice()))]
    pub fn readuntil(&self, py: Python<'_>, separator: &[u8]) -> PyResult<Py<PyAny>> {
        if separator.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Separator cannot be empty"
            ));
        }
        
        // Try to get data immediately
        match self._try_readuntil(py, separator)? {
            Some(data) => Ok(data),
            None => {
                // Create a pending future
                let future = Py::new(py, PendingFuture::new())?;
                self.waiters.lock().push((WaiterType::ReadUntil(separator.to_vec()), future.clone_ref(py)));
                Ok(future.into_any())
            }
        }
    }

    /// Read one line (until \n) - async, returns a future
    pub fn readline(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Try to get data immediately
        match self._try_readline(py)? {
            Some(data) => Ok(data),
            None => {
                // Create a pending future
                let future = Py::new(py, PendingFuture::new())?;
                self.waiters.lock().push((WaiterType::ReadLine, future.clone_ref(py)));
                Ok(future.into_any())
            }
        }
    }

    /// Get the buffer size limit
    pub fn get_limit(&self) -> usize {
        self.limit
    }

    /// Get current buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.lock().len()
    }

    fn __repr__(&self) -> String {
        let buffer_len = self.buffer.lock().len();
        let eof = *self.eof.lock();
        format!("<StreamReader buffer_len={} eof={}>", buffer_len, eof)
    }
}

// Provides high-level async I/O operations for writing data.
#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamWriter {
    /// Internal write buffer (shared with transport)
    buffer: Arc<Mutex<Vec<u8>>>,
    /// Closed flag
    closed: Arc<Mutex<bool>>,
    /// Closing flag
    closing: Arc<Mutex<bool>>,
    /// High water mark for flow control
    high_water: usize,
    /// Low water mark for flow control
    low_water: usize,
    /// Drain waiters - futures waiting for buffer to drain
    drain_waiters: Arc<Mutex<Vec<Py<PendingFuture>>>>,
    /// Transport reference for triggering writes
    transport: Arc<Mutex<Option<Py<PyAny>>>>,
}

#[pymethods]
impl StreamWriter {
    #[new]
    #[pyo3(signature = (high_water=None, low_water=None))]
    pub fn new(high_water: Option<usize>, low_water: Option<usize>) -> Self {
        const DEFAULT_HIGH: usize = 64 * 1024; // 64 KB
        const DEFAULT_LOW: usize = 16 * 1024;  // 16 KB
        
        let high = high_water.unwrap_or(DEFAULT_HIGH);
        let low = low_water.unwrap_or(DEFAULT_LOW);
        
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(Mutex::new(false)),
            closing: Arc::new(Mutex::new(false)),
            high_water: high,
            low_water: low,
            drain_waiters: Arc::new(Mutex::new(Vec::new())),
            transport: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Internal method to set the transport
    pub fn _set_transport(&self, transport: Py<PyAny>) {
        *self.transport.lock() = Some(transport);
    }

    /// Write data to the buffer and trigger transport write
    pub fn write(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if *self.closed.lock() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Writer is closed"
            ));
        }
        
        if *self.closing.lock() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Writer is closing"
            ));
        }
        
        // Add data to buffer
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(data);
        drop(buffer);
        
        // Trigger transport to write
        if let Some(transport) = self.transport.lock().as_ref() {
            transport.call_method1(py, "_trigger_write", ())?;
        }
        
        Ok(())
    }
    
    /// Wait for the write buffer to drain below the low water mark
    pub fn drain(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // If already below low water mark, return completed future
        if self.is_drained() {
            let fut = crate::transports::future::CompletedFuture::new(py.None());
            return Ok(Py::new(py, fut)?.into_any());
        }
        
        // Create a pending future
        let future = Py::new(py, PendingFuture::new())?;
        self.drain_waiters.lock().push(future.clone_ref(py));
        Ok(future.into_any())
    }
    
    /// Internal method to wake up drain waiters when buffer is drained
    pub fn _wakeup_drain_waiters(&self, py: Python<'_>) -> PyResult<()> {
        if self.is_drained() {
            let mut waiters = self.drain_waiters.lock();
            for future in waiters.drain(..) {
                future.bind(py).borrow().set_result(py, py.None())?;
            }
        }
        Ok(())
    }

    /// Write multiple lines
    pub fn writelines(&self, py: Python<'_>, lines: Vec<Vec<u8>>) -> PyResult<()> {
        for line in lines {
            self.write(py, &line)?;
        }
        Ok(())
    }

    /// Mark the writer as closing
    pub fn close(&self) -> PyResult<()> {
        *self.closing.lock() = true;
        Ok(())
    }

    /// Check if transport is closing
    pub fn is_closing(&self) -> bool {
        *self.closing.lock() || *self.closed.lock()
    }

    /// Check if the buffer needs draining (above high water mark)
    pub fn needs_drain(&self) -> bool {
        self.buffer.lock().len() > self.high_water
    }

    /// Get the current write buffer size
    pub fn get_write_buffer_size(&self) -> usize {
        self.buffer.lock().len()
    }

    /// Clear the buffer (simulate drain completion)
    pub fn _clear_buffer(&self) -> Vec<u8> {
        let mut buffer = self.buffer.lock();
        buffer.drain(..).collect()
    }

    /// Check if buffer is below low water mark
    pub fn is_drained(&self) -> bool {
        self.buffer.lock().len() <= self.low_water
    }

    /// Check if can write EOF
    pub fn can_write_eof(&self) -> bool {
        !*self.closed.lock()
    }

    /// Write EOF (mark as closed)
    pub fn write_eof(&self) -> PyResult<()> {
        if *self.closed.lock() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Already closed"
            ));
        }
        
        *self.closed.lock() = true;
        *self.closing.lock() = true;
        Ok(())
    }

    /// Get high water mark
    pub fn get_high_water(&self) -> usize {
        self.high_water
    }

    /// Get low water mark
    pub fn get_low_water(&self) -> usize {
        self.low_water
    }

    fn __repr__(&self) -> String {
        let is_closing = self.is_closing();
        let buffer_size = self.get_write_buffer_size();
        format!("<StreamWriter buffer_size={} closing={}>", buffer_size, is_closing)
    }
}

// Impl block outside of pymethods for Rust-only methods
impl StreamWriter {
    /// Get the buffer Arc for sharing with transport (Rust-only method)
    pub(crate) fn get_buffer_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        self.buffer.clone()
    }
}
