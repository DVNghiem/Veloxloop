use crate::buffer_pool::BufferPool;
use crate::{
    constants::{DEFAULT_HIGH, DEFAULT_LIMIT, DEFAULT_LOW},
    transports::future::PendingFuture,
};
use bytes::BytesMut;
use memchr::memchr;
use parking_lot::Mutex;
use pyo3::IntoPyObjectExt;
use pyo3::ffi;
#[allow(unused)]
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;
use std::io::{self, Read};
use std::sync::Arc;

thread_local! {
    static TEMP_READ_BUF: RefCell<Vec<u8>> = RefCell::new(vec![0; 131072]);
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamReader {
    pub(crate) inner: RefCell<StreamReaderInner>,
    /// Maximum buffer size before pausing
    pub(crate) limit: usize,
}

// Safety: StreamReader is only used in single-threaded Python context
// PyO3 requires Send+Sync for #[pyclass], but we never actually send across threads
unsafe impl Send for StreamReader {}
unsafe impl Sync for StreamReader {}

pub(crate) struct StreamReaderInner {
    pub(crate) buffer: BytesMut,
    pub(crate) eof: bool,
    pub(crate) exception: Option<String>,
    pub(crate) waiters: Vec<(WaiterType, Py<PendingFuture>)>,
}

impl StreamReaderInner {
    fn feed_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    fn feed_eof(&mut self) {
        self.eof = true;
    }
}

impl Drop for StreamReaderInner {
    fn drop(&mut self) {
        let buf = std::mem::replace(&mut self.buffer, BytesMut::new());
        BufferPool::release(buf);
    }
}

#[derive(Clone)]
pub(crate) enum WaiterType {
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
            inner: RefCell::new(StreamReaderInner {
                buffer: BufferPool::acquire(),
                eof: false,
                exception: None,
                waiters: Vec::new(),
            }),
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
            let mut inner = self.inner.borrow_mut();
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
            let mut inner = self.inner.borrow_mut();
            inner.feed_eof();
        }
        self._wakeup_waiters(py)?;
        Ok(())
    }

    /// Internal method to wake up waiting futures
    pub(crate) fn _wakeup_waiters(&self, py: Python<'_>) -> PyResult<()> {
        // Collect satisfied futures to avoid holding the borrow while calling Python code
        let mut ready_waiters = Vec::new();
        let mut error_waiters = Vec::new();

        {
            let mut inner_guard = self.inner.borrow_mut();
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
        let mut inner = self.inner.borrow_mut();
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
        let mut inner = self.inner.borrow_mut();
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
        let mut inner = self.inner.borrow_mut();
        inner.exception = Some(message);
        Ok(())
    }

    /// Get the current exception message (if any)
    pub fn exception(&self) -> Option<String> {
        self.inner.borrow().exception.clone()
    }

    /// Check if at EOF
    pub fn at_eof(&self) -> bool {
        let inner = self.inner.borrow();
        inner.eof && inner.buffer.is_empty()
    }

    /// Read up to n bytes synchronously from buffer
    /// Returns immediately with available data (does not wait for more data)
    #[pyo3(signature = (n=-1))]
    pub fn read(&self, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        let mut inner = self.inner.borrow_mut();

        // Check for exception
        if let Some(exc_msg) = inner.exception.take() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(exc_msg));
        }

        if n < 0 {
            // Read all available data
            let data = inner.buffer.split().to_vec();
            let bytes = PyBytes::new(py, &data);
            return Ok(bytes.into());
        }

        let n = n as usize;
        let available = inner.buffer.len().min(n);
        let data = inner.buffer.split_to(available).to_vec();
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
                    .borrow_mut()
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
                self.inner.borrow_mut().waiters.push((
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
                    .borrow_mut()
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
        self.inner.borrow().buffer.len()
    }

    fn __repr__(&self) -> String {
        let inner = self.inner.borrow();
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
        buffer: &mut BytesMut,
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
            let data = buffer.split_to(end).to_vec();
            return Ok(Some(data));
        }

        if eof {
            if buffer.is_empty() {
                return Ok(Some(Vec::new()));
            }
            let data = buffer.split().to_vec();
            return Ok(Some(data));
        }

        Ok(None)
    }

    // Helper for readexactly logic
    fn _try_readexactly_inner(
        buffer: &mut BytesMut,
        eof: bool,
        n: usize,
    ) -> PyResult<Option<Vec<u8>>> {
        if buffer.len() >= n {
            let data = buffer.split_to(n).to_vec();
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

    /// Optimized zero-copy read from socket
    /// Uses larger buffer (128KB) for better large message performance
    pub(crate) fn read_from_socket(
        &self,
        stream: &mut std::net::TcpStream,
    ) -> std::io::Result<usize> {
        TEMP_READ_BUF.with(|buf_cell| {
            let mut temp = buf_cell.borrow_mut();
            let mut total = 0;
            
            loop {
                match stream.read(&mut temp[..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        self.inner.borrow_mut().buffer.extend_from_slice(&temp[..n]);
                        if n < 131072 { break; }  // Partial read
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e),
                }
            }
            Ok(total)
        })
    }
}

/// Trait for transport to trigger write flush from StreamWriter without Python
pub trait StreamWriterProxy: Send + Sync {
    fn trigger_write(&self, py: Python<'_>) -> PyResult<()>;
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamWriter {
    /// Internal write buffer (shared with transport)
    pub(crate) buffer: Arc<Mutex<BytesMut>>,
    /// Closed flag
    pub(crate) closed: Arc<Mutex<bool>>,
    /// Closing flag
    pub(crate) closing: Arc<Mutex<bool>>,
    /// High water mark for flow control
    pub(crate) high_water: usize,
    /// Low water mark for flow control
    pub(crate) low_water: usize,
    /// Drain waiters - futures waiting for buffer to drain
    pub(crate) drain_waiters: Arc<Mutex<Vec<Py<PendingFuture>>>>,
    /// Transport reference for triggering writes (legacy Python path)
    pub(crate) transport: Arc<Mutex<Option<Py<PyAny>>>>,
    /// Native transport proxy for triggering writes (optimized path)
    pub(crate) proxy: Arc<Mutex<Option<Arc<dyn StreamWriterProxy>>>>,
}

#[pymethods]
impl StreamWriter {
    #[new]
    #[pyo3(signature = (high_water=None, low_water=None))]
    pub fn new(high_water: Option<usize>, low_water: Option<usize>) -> Self {
        let high = high_water.unwrap_or(DEFAULT_HIGH);
        let low = low_water.unwrap_or(DEFAULT_LOW);

        Self {
            buffer: Arc::new(Mutex::new(BytesMut::with_capacity(high))),
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
        buffer.split().to_vec()
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

#[pyclass(module = "veloxloop._veloxloop")]
pub struct VeloxBuffer {
    data: Option<BytesMut>,
}

#[pymethods]
impl VeloxBuffer {
    #[new]
    fn new() -> Self {
        Self { data: None }
    }

    fn __len__(&self) -> usize {
        self.data.as_ref().map(|d| d.len()).unwrap_or(0)
    }

    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        _flags: i32,
    ) -> PyResult<()> {
        let self_ = slf.borrow();
        let data = self_.data.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyBufferError::new_err("Buffer is empty or released")
        })?;

        if view.is_null() {
            return Err(pyo3::exceptions::PyBufferError::new_err("Null buffer view"));
        }

        let slice = &data[..];

        unsafe {
            (*view).obj = slf.as_ptr();
            ffi::Py_XINCREF((*view).obj);
            (*view).buf = slice.as_ptr() as *mut _;
            (*view).len = slice.len() as ffi::Py_ssize_t;
            (*view).readonly = 1;
            (*view).itemsize = 1;
            (*view).format = std::ptr::null_mut();
            (*view).ndim = 1;
            (*view).shape = &mut (*view).len;
            (*view).strides = &mut (*view).itemsize;
            (*view).suboffsets = std::ptr::null_mut();
            (*view).internal = std::ptr::null_mut();
        }

        Ok(())
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        if let Some(data) = &self.data {
            Ok(PyBytes::new(py, data))
        } else {
            Ok(PyBytes::new(py, &[]))
        }
    }

    fn release(&mut self) {
        if let Some(buf) = self.data.take() {
            BufferPool::release(buf);
        }
    }
}

impl Drop for VeloxBuffer {
    fn drop(&mut self) {
        if let Some(buf) = self.data.take() {
            BufferPool::release(buf);
        }
    }
}

impl VeloxBuffer {
    pub fn from_bytes_mut(buf: BytesMut) -> Self {
        Self { data: Some(buf) }
    }
}

// Impl block outside of pymethods for Rust-only methods
impl StreamWriter {
    /// Internal method to set the native proxy (Rust path)
    pub fn set_proxy(&self, proxy: Arc<dyn StreamWriterProxy>) {
        *self.proxy.lock() = Some(proxy);
    }

    /// Get the buffer Arc for sharing with transport (Rust-only method)
    pub(crate) fn get_buffer_arc(&self) -> Arc<Mutex<BytesMut>> {
        self.buffer.clone()
    }
}
