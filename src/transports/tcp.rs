use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::net::{TcpStream, SocketAddr};
use std::os::fd::{AsRawFd, RawFd};
use std::io::{Write, self};

use crate::utils::VeloxResult;
use crate::event_loop::VeloxLoop;
use super::future::CompletedFuture;

// Pure Rust socket wrapper to avoid importing Python's socket module
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SocketWrapper {
    fd: RawFd,
    addr: SocketAddr,
}

#[pymethods]
impl SocketWrapper {
    fn getsockname(&self) -> PyResult<(String, u16)> {
        Ok((self.addr.ip().to_string(), self.addr.port()))
    }
    
    fn fileno(&self) -> RawFd {
        self.fd
    }
}

impl SocketWrapper {
    fn new(fd: RawFd, addr: SocketAddr) -> Self {
        Self { fd, addr }
    }
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct TcpServer {
    listener: Option<std::net::TcpListener>,
    loop_: Py<VeloxLoop>,
    protocol_factory: Py<PyAny>,
    active: bool,
}

#[pymethods]
impl TcpServer {
    #[getter]
    fn sockets(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Return a list containing a socket wrapper
        if let Some(listener) = self.listener.as_ref() {
            let fd = listener.as_raw_fd();
            let addr = listener.local_addr()?;
            let socket_wrapper = SocketWrapper::new(fd, addr);
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
    
    fn fd(&self) -> Option<RawFd> {
        self.listener.as_ref().map(|l| l.as_raw_fd())
    }
    
    // wait_closed is async. We return a completed future-like object
    fn wait_closed(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
         // Create a simple completed future wrapper
         let fut = CompletedFuture::new(py.None());
         Ok(Py::new(py, fut)?.into())
    }
    
    fn __aenter__<'py>(slf: Bound<'py, Self>) -> PyResult<Py<PyAny>> {
        // Async context manager protocol - return a completed future with self
        let py = slf.py();
        let server_obj = slf.clone().unbind();
        let fut = CompletedFuture::new(server_obj.into());
        Ok(Py::new(py, fut)?.into())
    }
    
    fn __aexit__(&mut self, py: Python<'_>, _exc_type: Py<PyAny>, _exc_val: Py<PyAny>, _exc_tb: Py<PyAny>) -> PyResult<Py<PyAny>> {
        // Close the server when exiting context
        self.close(py)?;
        // Return a completed future with None
        let fut = CompletedFuture::new(py.None());
        Ok(Py::new(py, fut)?.into())
    }
    
    fn _on_accept(&self, py: Python<'_>) -> PyResult<()> {
        // Accept
        // We need mutable access or interior mutability? TcpListener accept takes &self.
        if let Some(listener) = self.listener.as_ref() {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Create protocol
                     let protocol = self.protocol_factory.call0(py)?;
                     // Create Transport
                     let transport = TcpTransport::new(self.loop_.clone_ref(py), stream, protocol.clone_ref(py))?;
                     let transport_py = Py::new(py, transport)?;
                     
                     // Connection made
                     protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
                     // Start reading
                     let read_ready = transport_py.getattr(py, "_read_ready")?;
                     let fd = transport_py.borrow(py).fd;
                     self.loop_.bind(py).borrow().add_reader(py, fd, read_ready)?;
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
    protocol: Py<PyAny>,
    loop_: Py<VeloxLoop>,
    closing: bool,
    // Buffer for outgoing data? Asyncio transports buffer if socket is full.
    // For MVP we might BLOCK or fail if full? No, we must buffer.
    write_buffer: Vec<u8>,
}

#[pymethods]
impl TcpTransport {
    fn get_extra_info(&self, _name: &str) -> PyResult<Option<String>> {
        // Implement peername, sockname etc.
        Ok(None)
    }

    fn get_write_buffer_size(&self) -> usize {
        self.write_buffer.len()
    }
    
    fn set_write_buffer_limits(&self, _high: Option<usize>, _low: Option<usize>) -> PyResult<()> {
        Ok(())
    }
    
    fn write_eof(&mut self) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            stream.shutdown(std::net::Shutdown::Write)?;
        }
        Ok(())
    }

    fn is_closing(&self) -> bool {
        self.closing
    }

    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        {
             let self_ = slf.borrow();
             if self_.closing {
                 return Ok(());
             }
        }
        
        let needs_writer;
        let should_force_close;
        
        {
             let mut self_ = slf.borrow_mut();
             self_.closing = true;
             should_force_close = self_.write_buffer.is_empty();
             needs_writer = !self_.write_buffer.is_empty();
        }
        
        if should_force_close {
             let mut self_ = slf.borrow_mut();
             self_._force_close(py)?;
        } else if needs_writer {
             // Ensure writer is active to flush buffer
             // Logic similar to write()
             let self_ = slf.borrow();
             let fd = self_.fd;
             let method = slf.getattr("_write_ready")?;
             self_.loop_.bind(py).borrow().add_writer(py, fd, method.unbind())?;
        }
        Ok(())
    }
    
    fn _force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        let fd = self.fd;
        
        let loop_ = self.loop_.bind(py).borrow();
        loop_.remove_reader(py, fd)?;
        loop_.remove_writer(py, fd)?;
        drop(loop_);
        
        self.stream = None; 
        
        // Notify Protocol
        let _ = self.protocol.call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn write(slf: &Bound<'_, Self>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let bytes = data.as_bytes();

        let mut self_ = slf.borrow_mut();
        
        // Try writing directly
        if let Some(mut stream) = self_.stream.as_ref() {
             // Non-blocking write
             match stream.write(bytes) {
                 Ok(n) if n == bytes.len() => {
                     // All written
                 }
                 Ok(n) => {
                     // Partial write
                     self_.write_buffer.extend_from_slice(&bytes[n..]);
                     // Register writer
                     drop(self_); // Drop mutable borrow
                     slf.borrow().add_writer(slf)?;
                 }
                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                     self_.write_buffer.extend_from_slice(bytes);
                     // Register writer
                     drop(self_);
                     slf.borrow().add_writer(slf)?;
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
                              self.loop_.bind(py).borrow().remove_writer(py, fd)?;
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

    fn _read_ready(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        
        let (read_result, protocol) = {
             let self_ = slf.borrow();
             let protocol = self_.protocol.clone_ref(py);
             
             if let Some(stream) = self_.stream.as_ref() {
                  let mut buf = [0u8; 4096];
                  // Read using &TcpStream impl of Read (which works for shared reference)
                  
                  // Use io::Read logic
                  let mut s = stream; // s is &TcpStream
                  match std::io::Read::read(&mut s, &mut buf) {
                      Ok(0) => (Ok(None), protocol),
                      Ok(n) => (Ok(Some(Vec::from(&buf[..n]))), protocol),
                      Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                      Err(e) => (Err(e), protocol),
                  }
             } else {
                  return Ok(());
             }
        };
        
        match read_result {
             Ok(Some(data)) => {
                  let py_data = PyBytes::new(py, &data);
                  protocol.call_method1(py, "data_received", (py_data,))?;
             }
             Ok(None) => {
                  if let Ok(res) = protocol.call_method0(py, "eof_received") {
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
             Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}

impl TcpServer {
    pub fn new(listener: std::net::TcpListener, loop_: Py<VeloxLoop>, protocol_factory: Py<PyAny>) -> Self {
        Self {
            listener: Some(listener),
            loop_,
            protocol_factory,
            active: true,
        }
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
    pub fn new(loop_: Py<VeloxLoop>, stream: std::net::TcpStream, protocol: Py<PyAny>) -> VeloxResult<Self> {
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
    
    fn add_writer(&self, slf: &Bound<'_, Self>) -> PyResult<()> {
        let method = slf.getattr("_write_ready")?;
        let py = slf.py();
        self.loop_.bind(py).borrow().add_writer(py, self.fd, method.unbind())?;
        Ok(())
    }
}
