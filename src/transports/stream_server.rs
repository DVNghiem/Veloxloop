use pyo3::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::io::{self, Write, Read};
use std::sync::Arc;
use parking_lot::Mutex;

use crate::event_loop::VeloxLoop;
use crate::streams::{StreamReader, StreamWriter};
use crate::utils::VeloxResult;

/// A high-performance stream-based transport that directly integrates StreamReader/StreamWriter
/// This avoids the Protocol API overhead for stream-based communication
#[pyclass(module = "veloxloop._veloxloop")]
pub struct StreamTransport {
    fd: RawFd,
    stream: Option<TcpStream>,
    loop_: Py<VeloxLoop>,
    reader: Py<StreamReader>,
    writer: Py<StreamWriter>,
    closing: bool,
    // Shared write buffer between StreamWriter and transport
    write_buffer: Arc<Mutex<Vec<u8>>>,
    // Cached write callback for registering writer
    write_callback: Arc<Mutex<Option<Py<PyAny>>>>,
}

#[pymethods]
impl StreamTransport {
    fn get_reader(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.reader.clone_ref(py).into_any())
    }
    
    fn get_writer(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.writer.clone_ref(py).into_any())
    }
    
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.closing {
            return Ok(());
        }
        
        self.closing = true;
        
        // Mark writer as closing
        self.writer.bind(py).borrow().close()?;
        
        // Remove from event loop
        self.loop_.bind(py).borrow().remove_reader(py, self.fd)?;
        
        // If write buffer is empty, close immediately
        if self.write_buffer.lock().is_empty() {
            self.force_close(py)?;
        }
        
        Ok(())
    }
    
    fn force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(stream) = self.stream.take() {
            self.loop_.bind(py).borrow().remove_reader(py, self.fd).ok();
            self.loop_.bind(py).borrow().remove_writer(py, self.fd).ok();
            drop(stream);
        }
        self.closing = true;
        Ok(())
    }
    
    fn is_closing(&self) -> bool {
        self.closing
    }
    
    fn _read_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            // Use large buffer for better performance
            let mut buf = [0u8; 65536];
            let mut s = stream;
            
            match Read::read(&mut s, &mut buf) {
                Ok(0) => {
                    // EOF
                    self.reader.bind(py).borrow().feed_eof(py)?;
                    self.close(py)?;
                }
                Ok(n) => {
                    // Feed data directly to StreamReader with Python context
                    self.reader.bind(py).borrow().feed_data(py, &buf[..n])?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    self.reader.bind(py).borrow().set_exception(e.to_string())?;
                    self.close(py)?;
                }
            }
        }
        Ok(())
    }
    
    fn _write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            let mut buffer = self.write_buffer.lock();
            
            if !buffer.is_empty() {
                // Try to write as much as possible
                loop {
                    match stream.write(&buffer) {
                        Ok(0) => {
                            return Err(PyErr::new::<pyo3::exceptions::PyConnectionError, _>(
                                "Connection closed during write"
                            ));
                        }
                        Ok(n) => {
                            buffer.drain(0..n);
                            if buffer.is_empty() {
                                self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
                                drop(buffer);
                                
                                // Wake up drain waiters
                                self.writer.bind(py).borrow()._wakeup_drain_waiters(py)?;
                                
                                // If closing and buffer is empty, close now
                                if self.closing {
                                    self.force_close(py)?;
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
                }
            }
        }
        Ok(())
    }
    
    /// Trigger write when data is added to buffer (called by StreamWriter)
    fn _trigger_write(&self, py: Python<'_>) -> PyResult<()> {
        // If we have buffered data, ensure writer callback is registered
        if !self.write_buffer.lock().is_empty() {
            // Try immediate write first
            if let Some(mut stream) = self.stream.as_ref() {
                let mut buffer = self.write_buffer.lock();
                if !buffer.is_empty() {
                    match stream.write(&buffer) {
                        Ok(n) if n > 0 => {
                            buffer.drain(0..n);
                        }
                        _ => {}
                    }
                    
                    // If still have data, register writer callback
                    if !buffer.is_empty() {
                        drop(buffer);
                        if let Some(callback) = self.write_callback.lock().as_ref() {
                            self.loop_.bind(py).borrow().add_writer(py, self.fd, callback.clone_ref(py))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, _py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if self.closing {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Transport is closing"
            ));
        }
        
        let mut buffer = self.write_buffer.lock();
        let was_empty = buffer.is_empty();
        
        if was_empty {
            // Try to write immediately
            if let Some(mut stream) = self.stream.as_ref() {
                match stream.write(data) {
                    Ok(n) if n == data.len() => {
                        // All written, done
                        return Ok(());
                    }
                    Ok(n) => {
                        // Partial write, buffer the rest
                        buffer.extend_from_slice(&data[n..]);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Buffer all data
                        buffer.extend_from_slice(data);
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                }
            }
            
            // Add writer if we have buffered data
            if !buffer.is_empty() {
                drop(buffer);
                // The writer callback will be triggered when socket becomes writable
                // No need to explicitly schedule it here
            }
        } else {
            // Already have buffered data, just append
            buffer.extend_from_slice(data);
        }
        
        Ok(())
    }
    
    fn fileno(&self) -> RawFd {
        self.fd
    }
    
    pub(crate) fn get_fd(&self) -> RawFd {
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
        let fd = stream.as_raw_fd();
        
        // Use the writer's buffer directly (shared)
        let write_buffer = writer.bind(py).borrow().get_buffer_arc();
        
        let transport = StreamTransport {
            fd,
            stream: Some(stream),
            loop_: loop_.clone_ref(py),
            reader: reader.clone_ref(py),
            writer: writer.clone_ref(py),
            closing: false,
            write_buffer,
            write_callback: Arc::new(Mutex::new(None)),
        };
        
        let transport_py = Py::new(py, transport)?;
        
        // Cache the write callback
        let write_ready = transport_py.getattr(py, "_write_ready")?;
        transport_py.borrow(py).write_callback.lock().replace(write_ready);
        
        // Set the transport reference in the writer
        writer.bind(py).borrow()._set_transport(transport_py.clone_ref(py).into_any());
        
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
    #[getter]
    fn sockets(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(listener) = self.listener.as_ref() {
            let fd = listener.as_raw_fd();
            let addr = listener.local_addr()?;
            let socket_wrapper = crate::transports::tcp::SocketWrapper::new_with_peer(fd, addr, addr);
            let sock_py = Py::new(py, socket_wrapper)?;
            let list = pyo3::types::PyList::new(py, &[sock_py])?;
            Ok(list.into())
        } else {
            Ok(pyo3::types::PyList::empty(py).into())
        }
    }
    
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(listener) = self.listener.as_ref() {
            let fd = listener.as_raw_fd();
            self.loop_.bind(py).borrow().remove_reader(py, fd)?;
        }
        self.active = false;
        self.listener = None;
        Ok(())
    }
    
    fn get_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.loop_.clone_ref(py).into_any())
    }
    
    fn is_serving(&self) -> bool {
        self.active
    }
    
    fn wait_closed(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let fut = crate::transports::future::CompletedFuture::new(py.None());
        Ok(Py::new(py, fut)?.into())
    }
    
    fn _on_accept(&self, py: Python<'_>) -> PyResult<()> {
        if let Some(listener) = self.listener.as_ref() {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Create StreamReader and StreamWriter
                    let reader = Py::new(py, StreamReader::new(Some(self.limit)))?;
                    let writer = Py::new(py, StreamWriter::new(Some(65536), Some(16384)))?;
                    
                    // Create StreamTransport (now returns Py<StreamTransport>)
                    let transport_py = StreamTransport::new(
                        py,
                        self.loop_.clone_ref(py),
                        stream,
                        reader.clone_ref(py),
                        writer.clone_ref(py),
                    )?;
                    
                    // Register read callback
                    let read_ready = transport_py.getattr(py, "_read_ready")?;
                    let fd = transport_py.borrow(py).fd;
                    self.loop_.bind(py).borrow().add_reader(py, fd, read_ready)?;
                    
                    // Note: write callback is cached in transport and will be registered when data is written
                    
                    // Call client_connected callback with (reader, writer) in a new task
                    let loop_py = self.loop_.clone_ref(py).into_any();
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
