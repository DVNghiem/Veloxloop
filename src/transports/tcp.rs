use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;
use parking_lot::Mutex;
use std::io::{Read, Write, self};

use crate::utils::{VeloxResult, VeloxError};
use crate::event_loop::VeloxLoop;
use super::Transport;

#[pyclass(module = "veloxloop._veloxloop")]
pub struct TcpServer {
    listener: Option<std::net::TcpListener>,
    pub sockets: Vec<PyObject>, // List of socket objects (optional)
    loop_: PyObject,
    protocol_factory: PyObject,
    active: bool,
}

#[pymethods]
impl TcpServer {
    fn close(&mut self) -> PyResult<()> {
        self.active = false;
        self.listener = None;
        Ok(())
    }
    
    fn get_loop(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.loop_.clone_ref(py))
    }
    
    fn is_serving(&self) -> bool {
         self.active
    }
    
    // wait_closed is async. We return a Future.
    fn wait_closed(&self, py: Python<'_>) -> PyResult<PyObject> {
         let asyncio = py.import("asyncio")?;
         let fut = asyncio.call_method0("Future")?;
         fut.call_method1("set_result", (py.None(),))?;
         Ok(fut.into())
    }
    
    fn _on_accept(&self, slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        // Accept
        // We need mutable access or interior mutability? TcpListener accept takes &self.
        if let Some(listener) = self.listener.as_ref() {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Create protocol
                     let protocol = self.protocol_factory.call0(py)?;
                     // Create Transport
                     let transport = TcpTransport::new(py, self.loop_.clone_ref(py), stream, protocol.clone_ref(py))?;
                     let transport_py = Py::new(py, transport)?;
                     
                     // Connection made
                     protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
                     
                     // Start reading
                     let read_ready = transport_py.getattr(py, "_read_ready")?;
                     let fd = transport_py.borrow(py).fd;
                     self.loop_.call_method1(py, "add_reader", (fd, read_ready))?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct TcpTransport {
    fd: RawFd,
    stream: Option<std::net::TcpStream>,
    protocol: PyObject,
    loop_: PyObject,
    closing: bool,
    // Buffer for outgoing data? Asyncio transports buffer if socket is full.
    // For MVP we might BLOCK or fail if full? No, we must buffer.
    write_buffer: Vec<u8>,
}

#[pymethods]
impl TcpTransport {
    fn get_extra_info(&self, name: &str) -> PyResult<Option<String>> {
        // Implement peername, sockname etc.
        Ok(None)
    }

    fn is_closing(&self) -> bool {
        self.closing
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        self.closing = true;
        // Verify with loop to remove reader/writer?
        // In Python asyncio, transport.close() schedules cleanup.
        // We should call loop.remove_reader(fd) etc.
        let fd = self.fd;
        let _ = self.loop_.call_method1(py, "remove_reader", (fd,));
        let _ = self.loop_.call_method1(py, "remove_writer", (fd,));
        self.stream = None; // Drop closes socket
        Ok(())
    }

    fn write(&mut self, slf: &Bound<'_, Self>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let bytes = data.as_bytes();
        let py = slf.py();
        // Try writing directly
        if let Some(mut stream) = self.stream.as_ref() {
             // Non-blocking write
             match stream.write(bytes) {
                 Ok(n) if n == bytes.len() => {
                     // All written
                 }
                 Ok(n) => {
                     // Partial write
                     self.write_buffer.extend_from_slice(&bytes[n..]);
                     // Register writer
                     self.add_writer(slf)?;
                 }
                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                     self.write_buffer.extend_from_slice(bytes);
                     self.add_writer(slf)?;
                 }
                 Err(e) => {
                     // Report error to protocol?
                     return Err(e.into());
                 }
             }
        }
        Ok(())
    }
    
    // Internal callback called by loop when writable
    fn _write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            if !self.write_buffer.is_empty() {
                match stream.write(&self.write_buffer) {
                    Ok(n) => {
                         self.write_buffer.drain(0..n);
                         if self.write_buffer.is_empty() {
                             let fd = self.fd;
                             self.loop_.call_method1(py, "remove_writer", (fd,))?;
                         }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Still waiting
                    }
                    Err(e) => {
                         // Error, maybe close?
                         // self.close(py)?; 
                         return Err(e.into());
                    }
                }
            }
        }
        Ok(())
    }

    fn _read_ready(&self, slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        if let Some(mut stream) = self.stream.as_ref() {
             let mut buf = [0u8; 4096];
             match stream.read(&mut buf) {
                 Ok(0) => {
                     // EOF
                     // self.protocol.eof_received()
                     if let Ok(res) = self.protocol.call_method0(py, "eof_received") {
                          if let Ok(keep_open) = res.extract::<bool>(py) {
                              if !keep_open {
                                  slf.call_method0("close")?;
                              }
                          } else {
                              slf.call_method0("close")?;
                          }
                     } else {
                          slf.call_method0("close")?;
                     }
                 }
                 Ok(n) => {
                     let data = PyBytes::new(py, &buf[..n]);
                     self.protocol.call_method1(py, "data_received", (data,))?;
                 }
                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                     // Spurious wakeup
                 }
                 Err(e) => {
                     // Error
                     return Err(e.into());
                 }
             }
        }
        Ok(())
    }
}


impl TcpServer {
    pub fn new(listener: std::net::TcpListener, loop_: PyObject, protocol_factory: PyObject) -> Self {
        Self {
            listener: Some(listener),
            sockets: Vec::new(),
            loop_,
            protocol_factory,
            active: true,
        }
    }
    
    pub fn fd(&self) -> Option<RawFd> {
        self.listener.as_ref().map(|l| l.as_raw_fd())
    }
    
    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        if let Some(l) = self.listener.as_ref() {
            l.accept()
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Closed"))
        }
    }
}

impl TcpTransport {
    pub fn new(py: Python<'_>, loop_: PyObject, stream: std::net::TcpStream, protocol: PyObject) -> VeloxResult<Self> {
        stream.set_nonblocking(true)?;
        let fd = stream.as_raw_fd();
        
        let transport = Self {
            fd,
            stream: Some(stream),
            protocol,
            loop_,
            closing: false,
            write_buffer: Vec::new(),
        };
        Ok(transport)
    }
    
    // We need a helper to start reading because `new` returns Self (not PyObject yet).
    // The caller (create_connection) should call `transport.start_reading()`.
    // Or we expose `start_reading` as pymethod?
    // Actually asyncio transports start reading immediately.
    // CALLER `create_connection` creates the PyObject. It should call `add_reader`.
    
    // Let's add a method `maybe_start_reading` that `create_connection` calls?
    // Or just make `_read_ready` public so `create_connection` can use it?
    // `_read_ready` is pymethod.


    fn add_writer(&self, slf: &Bound<'_, Self>) -> PyResult<()> {
        // Get bound method "self._write_ready"
        let method = slf.getattr("_write_ready")?;
        self.loop_.call_method1(slf.py(), "add_writer", (self.fd, method))?;
        Ok(())
    }
}
