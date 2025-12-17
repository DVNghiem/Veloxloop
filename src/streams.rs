use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::Arc;
use parking_lot::Mutex;

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
        }
    }

    /// Feed data into the buffer
    pub fn feed_data(&self, data: &[u8]) -> PyResult<()> {
        if data.is_empty() {
            return Ok(());
        }
        
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(data);
        Ok(())
    }

    /// Signal EOF
    pub fn feed_eof(&self) -> PyResult<()> {
        *self.eof.lock() = true;
        Ok(())
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

    /// Read exactly n bytes (or raise error if not enough data)
    pub fn readexactly(&self, py: Python<'_>, n: usize) -> PyResult<Py<PyAny>> {
        let buffer = self.buffer.lock();
        
        if buffer.len() < n {
            if *self.eof.lock() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    format!("Not enough data: expected {}, got {}", n, buffer.len())
                ));
            }
            // Not enough data yet, return empty bytes
            drop(buffer);
            return Ok(PyBytes::new(py, &[]).into());
        }
        
        drop(buffer);
        self.read(py, n as isize)
    }

    /// Read until delimiter is found
    pub fn readuntil(&self, py: Python<'_>, separator: &[u8]) -> PyResult<Py<PyAny>> {
        if separator.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Separator cannot be empty"
            ));
        }
        
        let mut buffer = self.buffer.lock();
        
        // Find the separator
        if let Some(pos) = buffer.windows(separator.len())
            .position(|window| window == separator)
        {
            let end = pos + separator.len();
            let data = buffer.drain(..end).collect::<Vec<u8>>();
            let bytes = PyBytes::new(py, &data);
            return Ok(bytes.into());
        }
        
        // Separator not found
        if *self.eof.lock() {
            // Return all data if at EOF
            let data = buffer.drain(..).collect::<Vec<u8>>();
            let bytes = PyBytes::new(py, &data);
            return Ok(bytes.into());
        }
        
        // Not found and not at EOF - return empty
        Ok(PyBytes::new(py, &[]).into())
    }

    /// Read one line (until \n)
    pub fn readline(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.readuntil(py, b"\n")
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
    /// Internal write buffer
    buffer: Arc<Mutex<Vec<u8>>>,
    /// Closed flag
    closed: Arc<Mutex<bool>>,
    /// Closing flag
    closing: Arc<Mutex<bool>>,
    /// High water mark for flow control
    high_water: usize,
    /// Low water mark for flow control
    low_water: usize,
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
        }
    }

    /// Write data to the buffer
    pub fn write(&self, data: &[u8]) -> PyResult<()> {
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
        
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(data);
        Ok(())
    }

    /// Write multiple lines
    pub fn writelines(&self, lines: Vec<Vec<u8>>) -> PyResult<()> {
        for line in lines {
            self.write(&line)?;
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
