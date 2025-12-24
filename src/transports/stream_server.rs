use bytes::BytesMut;
use parking_lot::Mutex;
use pyo3::prelude::*;
use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use super::TransportState;
use crate::event_loop::VeloxLoop;
use crate::streams::{StreamReader, StreamWriter};
use crate::utils::VeloxResult;

/// stream-based transport that directly integrates StreamReader/StreamWriter
/// This avoids the Protocol API overhead for stream-based communication
#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamTransport {
    fd: RawFd,
    stream: Option<TcpStream>,
    loop_: Py<VeloxLoop>,
    reader: Py<StreamReader>,
    writer: Py<StreamWriter>,
    state: TransportState,
    // Shared write buffer between StreamWriter and transport
    write_buffer: Arc<Mutex<BytesMut>>,
    // Cached write callback for registering writer (native path)
    write_callback: Arc<Mutex<Option<Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>>>>,
}

/// Native proxy for StreamWriter to trigger writes on StreamTransport
struct StreamTransportProxy {
    transport: Py<StreamTransport>,
}

impl crate::streams::StreamWriterProxy for StreamTransportProxy {
    fn trigger_write(&self, py: Python<'_>) -> PyResult<()> {
        let t = self.transport.bind(py).borrow();
        t._trigger_write(py)
    }
}
unsafe impl Send for StreamTransportProxy {}
unsafe impl Sync for StreamTransportProxy {}

#[pymethods]
impl StreamTransport {
    pub fn get_reader(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.reader.clone_ref(py).into_any())
    }

    pub fn get_writer(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.writer.clone_ref(py).into_any())
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSING)
            || self.state.contains(TransportState::CLOSED)
        {
            return Ok(());
        }

        self.state.insert(TransportState::CLOSING);

        // Mark writer as closing
        self.writer.bind(py).borrow().close()?;

        // If buffer is empty, close now
        if self.write_buffer.lock().is_empty() {
            self._force_close_internal(py)?;
        }

        Ok(())
    }

    fn force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close_internal(py)
    }

    fn _force_close_internal(&mut self, py: Python<'_>) -> PyResult<()> {
        self.state.insert(TransportState::CLOSED);
        self.state.remove(TransportState::ACTIVE);
        self.state.remove(TransportState::CLOSING);

        if let Some(stream) = self.stream.take() {
            let loop_ = self.loop_.bind(py).borrow();
            let _ = loop_.remove_reader(py, self.fd);
            let _ = loop_.remove_writer(py, self.fd);
            drop(stream);
        }
        Ok(())
    }

    fn is_closing(&self) -> bool {
        self.state.contains(TransportState::CLOSING) || self.state.contains(TransportState::CLOSED)
    }

    pub(crate) fn _read_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if self
            .state
            .intersects(TransportState::CLOSED | TransportState::READING_PAUSED)
        {
            return Ok(());
        }

        if let Some(stream) = self.stream.as_mut() {
            let reader = self.reader.bind(py).borrow();
            match reader.read_from_socket(stream) {
                Ok(0) => {
                    // Signal EOF to reader and let protocol decide when to close
                    drop(reader);
                    self.reader.bind(py).borrow().feed_eof_native(py)?;
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub(crate) fn _write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            loop {
                let mut buffer = self.write_buffer.lock();
                if !buffer.is_empty() {
                    // Try to write as much as possible
                    match stream.write(&buffer) {
                        Ok(0) => {
                            return Err(PyErr::new::<pyo3::exceptions::PyConnectionError, _>(
                                "Connection closed during write",
                            ));
                        }
                        Ok(n) => {
                            let _ = buffer.split_to(n);
                            if buffer.is_empty() {
                                self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
                                drop(buffer);

                                // Wake up drain waiters
                                self.writer.bind(py).borrow()._wakeup_drain_waiters(py)?;

                                // If closing and buffer is empty, close now
                                if self.state.contains(TransportState::CLOSING) {
                                    self._force_close_internal(py)?;
                                    // Notify StreamWriter it is closed
                                    let writer = self.writer.bind(py).borrow();
                                    *writer.closed.lock() = true;
                                }
                                break;
                            }
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Trigger write when data is added to buffer (called by StreamWriter)
    fn _trigger_write(&self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSED) {
            return Ok(());
        }

        // If we have buffered data, ensure writer callback is registered
        if !self.write_buffer.lock().is_empty() {
            // Try immediate write first
            if let Some(mut stream) = self.stream.as_ref() {
                let mut buffer = self.write_buffer.lock();
                if !buffer.is_empty() {
                    match stream.write(&buffer) {
                        Ok(n) if n > 0 => {
                            let _ = buffer.split_to(n);
                        }
                        _ => {}
                    }

                    // If still have data, register writer callback
                    if !buffer.is_empty() {
                        drop(buffer);
                        if let Some(callback) = self.write_callback.lock().as_ref() {
                            self.loop_
                                .bind(py)
                                .borrow()
                                .add_writer_native(self.fd, callback.clone())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn sendto(&self, _py: Python<'_>, data: &[u8], addr: Option<(String, u16)>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSING)
            || self.state.contains(TransportState::CLOSED)
        {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Transport is closing or closed",
            ));
        }
        // StreamTransport does not support sendto with an address, as it's a connected stream.
        // If addr is None, it's equivalent to a regular write.
        // If addr is Some, it's an error for a stream transport.
        if addr.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "sendto with address is not supported for StreamTransport",
            ));
        }

        let mut buffer = self.write_buffer.lock();
        buffer.extend_from_slice(data);
        Ok(())
    }

    fn write(&mut self, _py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSED) {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Transport is closed",
            ));
        }

        let mut buffer = self.write_buffer.lock();
        buffer.extend_from_slice(data);
        Ok(())
    }

    fn fileno(&self) -> RawFd {
        self.fd
    }

    pub fn get_fd(&self) -> RawFd {
        self.fd
    }
}

impl StreamTransport {
    pub fn new(
        py: Python<'_>,
        loop_: Py<VeloxLoop>,
        stream: TcpStream,
        reader: Py<StreamReader>,
        writer: Py<StreamWriter>,
    ) -> VeloxResult<Py<StreamTransport>> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true).expect("set_nodelay call failed"); // lower latency (disable Nagle algorithm)
        let fd = stream.as_raw_fd();

        // Use the writer's buffer directly (shared)
        let writer_obj = writer.bind(py).borrow();
        let write_buffer = writer_obj.get_buffer_arc();

        let transport = Self {
            fd,
            stream: Some(stream),
            loop_: loop_.clone_ref(py),
            reader,
            writer,
            state: TransportState::ACTIVE,
            write_buffer,
            write_callback: Arc::new(Mutex::new(None)),
        };

        let transport_py = Py::new(py, transport)?;

        // Cache the write callback (native path)
        let transport_clone = transport_py.clone_ref(py);
        let write_callback = Arc::new(move |py: Python<'_>| {
            let mut t = transport_clone.bind(py).borrow_mut();
            t._write_ready(py)
        });
        transport_py
            .bind(py)
            .borrow()
            .write_callback
            .lock()
            .replace(write_callback);

        // Set the transport proxy in the writer for native trigger_write
        let proxy = Arc::new(StreamTransportProxy {
            transport: transport_py.clone_ref(py),
        });
        transport_py
            .bind(py)
            .borrow()
            .writer
            .bind(py)
            .borrow()
            .set_proxy(proxy);

        Ok(transport_py)
    }
}

/// Server that accepts connections and creates StreamReader/StreamWriter pairs
#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamServer {
    listener: Option<TcpListener>,
    loop_: Py<VeloxLoop>,
    client_connected_cb: Py<PyAny>,
    active: bool,
    limit: usize,
}

#[pymethods]
impl StreamServer {
    pub fn sockets(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(listener) = self.listener.as_ref() {
            let addr = listener.local_addr()?;
            let addr_tuple = crate::utils::ipv6::socket_addr_to_tuple(py, addr)?;
            let list = pyo3::types::PyList::new(py, vec![addr_tuple])?;
            Ok(list.into_any().unbind())
        } else {
            Ok(pyo3::types::PyList::empty(py).into_any().unbind())
        }
    }

    pub fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        if let Some(listener) = self.listener.take() {
            let fd = listener.as_raw_fd();
            self.loop_.bind(py).borrow().remove_reader(py, fd)?;
            drop(listener);
        }
        Ok(())
    }

    pub fn get_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.loop_.clone_ref(py).into_any())
    }

    pub fn is_serving(&self) -> bool {
        self.active
    }

    pub fn wait_closed(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Return a completed future as we don't have a specific wait mechanism yet
        let fut = crate::transports::future::CompletedFuture::new(py.None());
        Ok(Py::new(py, fut)?.into_any())
    }

    pub fn _on_accept(&self, py: Python<'_>) -> PyResult<()> {
        if !self.active {
            return Ok(());
        }

        if let Some(listener) = self.listener.as_ref() {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let loop_py = self.loop_.clone_ref(py);
                    let limit = self.limit;

                    // Create StreamReader and StreamWriter
                    let reader = Py::new(py, StreamReader::new(Some(limit)))?;
                    let writer = Py::new(py, StreamWriter::new(None, None))?;

                    // Create StreamTransport
                    let _transport = StreamTransport::new(
                        py,
                        loop_py.clone_ref(py),
                        stream,
                        reader.clone_ref(py),
                        writer.clone_ref(py),
                    )?;

                    let reader_py = reader.into_any();
                    let writer_py = writer.into_any();

                    // Call the callback
                    let result = self.client_connected_cb.call1(py, (reader_py, writer_py))?;

                    // Check if the result is a coroutine and schedule it
                    if result.bind(py).hasattr("__await__")? {
                        // It's a coroutine - create a task using the Python loop wrapper
                        loop_py.call_method1(py, "create_task", (result,))?;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

impl StreamServer {
    pub fn new(
        listener: TcpListener,
        loop_: Py<VeloxLoop>,
        client_connected_cb: Py<PyAny>,
        limit: usize,
    ) -> Self {
        Self {
            listener: Some(listener),
            loop_,
            client_connected_cb,
            active: true,
            limit,
        }
    }

    pub(crate) fn get_fd(&self) -> Option<RawFd> {
        self.listener.as_ref().map(|l| l.as_raw_fd())
    }
}
