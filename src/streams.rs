use crate::{
    constants::{DEFAULT_HIGH, DEFAULT_LIMIT, DEFAULT_LOW},
    transports::future::PendingFuture,
};
use memchr::memchr;
use parking_lot::Mutex;
use pyo3::IntoPyObjectExt;
#[allow(unused)]
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::Arc;

#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamReader {
    inner: Arc<Mutex<StreamReaderInner>>,
    /// Maximum buffer size before pausing
    limit: usize,
}

struct StreamReaderInner {
    buffer: Vec<u8>,
    eof: bool,
    exception: Option<String>,
    waiters: Vec<(WaiterType, Py<PendingFuture>)>,
}

impl StreamReaderInner {
    fn feed_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    fn feed_eof(&mut self) {
        self.eof = true;
    }
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
        Self {
            inner: Arc::new(Mutex::new(StreamReaderInner {
                buffer: Vec::with_capacity(DEFAULT_LIMIT),
                eof: false,
                exception: None,
                waiters: Vec::new(),
            })),
            limit: limit.unwrap_or(DEFAULT_LIMIT),
        }
    }

    /// Feed data into the buffer and wake up waiters
    pub fn feed_data(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if data.is_empty() {
            return Ok(());
        }

        self.feed_data_native(py, data)
    }

    /// Feed data into the buffer from Rust and wake up waiters
    pub fn feed_data_native(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if data.is_empty() {
            return Ok(());
        }

        {
            let mut inner = self.inner.lock();
            inner.feed_data(data);
        }

        // Try to satisfy waiting futures
        self._wakeup_waiters(py)?;
        Ok(())
    }

    /// Signal EOF and wake up all waiters
    pub fn feed_eof(&self, py: Python<'_>) -> PyResult<()> {
        self.feed_eof_native(py)
    }

    /// Signal EOF from Rust and wake up all waiters
    pub fn feed_eof_native(&self, py: Python<'_>) -> PyResult<()> {
        {
            let mut inner = self.inner.lock();
            inner.feed_eof();
        }
        self._wakeup_waiters(py)?;
        Ok(())
    }

    /// Internal method to wake up waiting futures
    pub(crate) fn _wakeup_waiters(&self, py: Python<'_>) -> PyResult<()> {
        // Collect satisfied futures to avoid holding the lock while calling Python code
        let mut ready_waiters = Vec::new();
        let mut error_waiters = Vec::new();

        {
            let mut inner_guard = self.inner.lock();
            let inner = &mut *inner_guard;

            // Check for exception first
            if let Some(exc_msg) = &inner.exception {
                // All waiters get error
                for (_, future) in inner.waiters.drain(..) {
                    error_waiters.push((future, exc_msg.clone()));
                }
            } else {
                // Split borrows to allow independent access to buffer and waiters
                let eof = inner.eof;
                let buffer = &mut inner.buffer;
                let waiters = &mut inner.waiters;

                let mut i = 0;
                while i < waiters.len() {
                    let should_remove = {
                        let waiter_type = &waiters[i].0;
                        match waiter_type {
                            WaiterType::ReadLine => Self::_try_readuntil_inner(buffer, eof, b"\n"),
                            WaiterType::ReadUntil(sep) => {
                                Self::_try_readuntil_inner(buffer, eof, sep)
                            }
                            WaiterType::ReadExactly(n) => {
                                Self::_try_readexactly_inner(buffer, eof, *n)
                            }
                        }?
                    };

                    if let Some(data) = should_remove {
                        let (_, future) = waiters.remove(i);
                        ready_waiters.push((future, data));
                    } else {
                        i += 1;
                    }
                }
            }
        }

        // Dispatch results outside lock
        for (future, data) in ready_waiters {
            let bytes = PyBytes::new(py, &data);
            future.bind(py).borrow().set_result(py, bytes.into())?;
        }

        for (future, msg) in error_waiters {
            // Correctly create exception object
            let exc = pyo3::exceptions::PyRuntimeError::new_err(msg).into_py_any(py)?;
            future.bind(py).borrow().set_exception(py, exc)?;
        }

        Ok(())
    }

    fn _try_readline(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self._try_readuntil(py, b"\n")
    }

    fn _try_readuntil(&self, py: Python<'_>, separator: &[u8]) -> PyResult<Option<Py<PyAny>>> {
        let mut inner = self.inner.lock();
        if let Some(msg) = &inner.exception {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(msg.clone()));
        }
        let eof = inner.eof;
        if let Some(data) = Self::_try_readuntil_inner(&mut inner.buffer, eof, separator)? {
            let bytes = PyBytes::new(py, &data);
            Ok(Some(bytes.into()))
        } else {
            Ok(None)
        }
    }

    fn _try_readexactly(&self, py: Python<'_>, n: usize) -> PyResult<Option<Py<PyAny>>> {
        let mut inner = self.inner.lock();
        if let Some(msg) = &inner.exception {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(msg.clone()));
        }
        let eof = inner.eof;
        if let Some(data) = Self::_try_readexactly_inner(&mut inner.buffer, eof, n)? {
            let bytes = PyBytes::new(py, &data);
            Ok(Some(bytes.into()))
        } else {
            Ok(None)
        }
    }

    /// Set an exception message to be raised on next read
    pub fn set_exception(&self, message: String) -> PyResult<()> {
        let mut inner = self.inner.lock();
        inner.exception = Some(message);
        Ok(())
    }

    /// Get the current exception message (if any)
    pub fn exception(&self) -> Option<String> {
        self.inner.lock().exception.clone()
    }

    /// Check if at EOF
    pub fn at_eof(&self) -> bool {
        let inner = self.inner.lock();
        inner.eof && inner.buffer.is_empty()
    }

    /// Read up to n bytes
    #[pyo3(signature = (n=-1))]
    pub fn read(&self, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        let mut inner = self.inner.lock();

        // Check for exception
        if let Some(exc_msg) = inner.exception.take() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(exc_msg));
        }

        if n < 0 {
            // Read all available data
            let data = inner.buffer.drain(..).collect::<Vec<u8>>();
            let bytes = PyBytes::new(py, &data);
            return Ok(bytes.into());
        }

        let n = n as usize;
        let available = inner.buffer.len().min(n);
        let data = inner.buffer.drain(..available).collect::<Vec<u8>>();
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
                self.inner
                    .lock()
                    .waiters
                    .push((WaiterType::ReadExactly(n), future.clone_ref(py)));
                Ok(future.into_any())
            }
        }
    }

    /// Read until delimiter is found (async - returns a future)
    #[pyo3(signature = (separator=b"\n".as_slice()))]
    pub fn readuntil(&self, py: Python<'_>, separator: &[u8]) -> PyResult<Py<PyAny>> {
        if separator.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Separator cannot be empty",
            ));
        }

        // Try to get data immediately
        match self._try_readuntil(py, separator)? {
            Some(data) => Ok(data),
            None => {
                // Create a pending future
                let future = Py::new(py, PendingFuture::new())?;
                self.inner.lock().waiters.push((
                    WaiterType::ReadUntil(separator.to_vec()),
                    future.clone_ref(py),
                ));
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
                self.inner
                    .lock()
                    .waiters
                    .push((WaiterType::ReadLine, future.clone_ref(py)));
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
        self.inner.lock().buffer.len()
    }

    fn __repr__(&self) -> String {
        let inner = self.inner.lock();
        format!(
            "<StreamReader buffer_len={} eof={}>",
            inner.buffer.len(),
            inner.eof
        )
    }
}

impl StreamReader {
    // Helper method for readuntil logic operating on raw buffer
    fn _try_readuntil_inner(
        buffer: &mut Vec<u8>,
        eof: bool,
        separator: &[u8],
    ) -> PyResult<Option<Vec<u8>>> {
        let pos = if separator.len() == 1 {
            memchr(separator[0], &buffer)
        } else {
            buffer
                .windows(separator.len())
                .position(|window| window == separator)
        };

        if let Some(pos) = pos {
            let end = pos + separator.len();
            let data = buffer.drain(..end).collect();
            return Ok(Some(data));
        }

        if eof {
            if buffer.is_empty() {
                return Ok(Some(Vec::new()));
            }
            let data = buffer.drain(..).collect();
            return Ok(Some(data));
        }

        Ok(None)
    }

    // Helper for readexactly logic
    fn _try_readexactly_inner(
        buffer: &mut Vec<u8>,
        eof: bool,
        n: usize,
    ) -> PyResult<Option<Vec<u8>>> {
        if buffer.len() >= n {
            let data = buffer.drain(..n).collect();
            return Ok(Some(data));
        }

        if eof {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Not enough data: expected {}, got {}",
                n,
                buffer.len()
            )));
        }

        Ok(None)
    }

    // Zero-copy read from socket
    pub(crate) fn read_from_socket(
        &self,
        py: Python<'_>,
        stream: &mut std::net::TcpStream,
    ) -> std::io::Result<usize> {
        // Reserve internal buffer and read directly
        let mut inner = self.inner.lock();
        let buffer = &mut inner.buffer;

        // Try to read into spare capacity or reserve more
        // We want to read a reasonable chunk, e.g. 64KB
        buffer.reserve(DEFAULT_LIMIT);

        // Unsafe access to unused capacity to avoid initialization cost
        let len = buffer.len();
        let cap = buffer.capacity();
        let spare =
            unsafe { std::slice::from_raw_parts_mut(buffer.as_mut_ptr().add(len), cap - len) };

        let n = std::io::Read::read(stream, spare)?;

        if n > 0 {
            unsafe { buffer.set_len(len + n) };
        }

        // Early return if no data read (EOF is handled differently or n=0)
        if n == 0 {
            return Ok(0);
        }

        // Drop lock before waking waiters to allow them to proceed
        drop(inner);

        // Try to wakeup waiters
        if let Err(e) = self._wakeup_waiters(py) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ));
        }

        Ok(n)
    }
}

/// Trait for transport to trigger write flush from StreamWriter without Python
pub trait StreamWriterProxy: Send + Sync {
    fn trigger_write(&self, py: Python<'_>) -> PyResult<()>;
}

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
    /// Transport reference for triggering writes (legacy Python path)
    transport: Arc<Mutex<Option<Py<PyAny>>>>,
    /// Native transport proxy for triggering writes (optimized path)
    proxy: Arc<Mutex<Option<Arc<dyn StreamWriterProxy>>>>,
}

#[pymethods]
impl StreamWriter {
    #[new]
    #[pyo3(signature = (high_water=None, low_water=None))]
    pub fn new(high_water: Option<usize>, low_water: Option<usize>) -> Self {
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
            proxy: Arc::new(Mutex::new(None)),
        }
    }

    /// Internal method to set the transport (Python path)
    pub fn _set_transport(&self, transport: Py<PyAny>) {
        *self.transport.lock() = Some(transport);
    }

    /// Write data to the buffer and trigger transport write
    pub fn write(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if *self.closed.lock() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Writer is closed",
            ));
        }

        if *self.closing.lock() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Writer is closing",
            ));
        }

        // Add data to buffer
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(data);
        drop(buffer);

        // Trigger transport to write
        if let Some(proxy) = self.proxy.lock().as_ref() {
            proxy.trigger_write(py)?;
        } else if let Some(transport) = self.transport.lock().as_ref() {
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
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Already closed"));
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
        format!(
            "<StreamWriter buffer_size={} closing={}>",
            buffer_size, is_closing
        )
    }
}

// Impl block outside of pymethods for Rust-only methods
impl StreamWriter {
    /// Internal method to set the native proxy (Rust path)
    pub fn set_proxy(&self, proxy: Arc<dyn StreamWriterProxy>) {
        *self.proxy.lock() = Some(proxy);
    }

    /// Get the buffer Arc for sharing with transport (Rust-only method)
    pub(crate) fn get_buffer_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        self.buffer.clone()
    }
}
