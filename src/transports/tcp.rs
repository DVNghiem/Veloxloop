use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;
use parking_lot::Mutex;
use std::io::{Read, Write, self};

use crate::utils::{VeloxResult, VeloxError};
use crate::event_loop::VeloxLoop;
use super::Transport;

// Pure Rust socket wrapper to avoid importing Python's socket module
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SocketWrapper {
    fd: RawFd,
    addr: SocketAddr,
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct AsyncConnectCallback {
    loop_: Py<PyAny>,
    future: Py<PyAny>,
    protocol_factory: Py<PyAny>,
    stream: Option<std::net::TcpStream>,
    fd: RawFd,
    // We need to keep stream alive? Yes.
}

#[pymethods]
impl AsyncConnectCallback {
    fn __call__(&mut self, py: Python<'_>) -> PyResult<()> {
        let loop_ = self.loop_.clone_ref(py);
        let fd = self.fd;
        
        // Unregister writer (ourselves)
        // Unregister writer (ourselves)
        if let Ok(bound) = loop_.bind(py).downcast::<VeloxLoop>() {
             bound.borrow().remove_writer(py, fd)?;
        } else {
             loop_.call_method1(py, "remove_writer", (fd,))?;
        }
        
        // Take stream
        if let Some(stream) = self.stream.take() {
            // Check error
            let res = stream.take_error();
             match res {
                 Ok(None) => {
                     eprintln!("DEBUG: Connection success fd={}", fd);
                     // Connected!
                     // Create protocol
                     let protocol_res = self.protocol_factory.call0(py);
                     match protocol_res {
                         Ok(protocol) => {
                              // Create Transport
                              let transport_res = TcpTransport::new(py, loop_.clone_ref(py), stream, protocol.clone_ref(py));
                              match transport_res {
                                  Ok(transport) => {
                                      let transport_py = Py::new(py, transport)?;
                                      // connection_made
                                      if let Err(e) = protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),)) {
                                          self.future.call_method1(py, "set_exception", (e,))?;
                                          return Ok(());
                                      }
                                      
                                      eprintln!("DEBUG: calling add_reader for fd={}", fd);
                                      // Add reader
                                      // We can use transport_py object
                                      let read_ready = transport_py.getattr(py, "_read_ready")?;
                                      if let Ok(bound) = loop_.bind(py).downcast::<VeloxLoop>() {
                                           bound.borrow().add_reader(py, fd, read_ready)?;
                                      } else {
                                           loop_.call_method1(py, "add_reader", (fd, read_ready))?;
                                      }
                                      
                                      // Set result: (transport, protocol)
                                      let res = PyTuple::new(py, &[transport_py.into_any(), protocol])?;
                                      self.future.call_method1(py, "set_result", (res,))?;
                                  }
                                  Err(e) => {
                                      // Convert VeloxError to PyErr
                                      let py_err: PyErr = e.into();
                                      self.future.call_method1(py, "set_exception", (py_err,))?;
                                  }
                              }
                         }
                         Err(e) => {
                             self.future.call_method1(py, "set_exception", (e,))?;
                         }
                     }
                 }
                 Ok(Some(e)) | Err(e) => {
                     // Error connecting
                     let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string());
                     self.future.call_method1(py, "set_exception", (py_err,))?;
                 }
             }
        }
        Ok(())
    }
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

// Pure Rust completed future to avoid importing asyncio.Future
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
    
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
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


#[pyclass(module = "veloxloop._veloxloop")]
pub struct TcpServer {
    listener: Option<std::net::TcpListener>,
    loop_: Py<PyAny>,
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
            if let Ok(bound) = self.loop_.bind(py).downcast::<VeloxLoop>() {
                 bound.borrow().remove_reader(py, fd)?;
            } else {
                 let _ = self.loop_.call_method1(py, "remove_reader", (fd,));
            }
        }
        self.active = false;
        self.listener = None;
        Ok(())
    }
    
    fn get_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.loop_.clone_ref(py))
    }
    
    fn is_serving(&self) -> bool {
         self.active
    }
    
    fn fd(&self) -> Option<RawFd> {
        self.listener.as_ref().map(|l| l.as_raw_fd())
    }
    
    // wait_closed is async. We return a completed future-like object
    fn wait_closed(&self, py: Python<'_>) -> PyResult<PyObject> {
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
                     let transport = TcpTransport::new(py, self.loop_.clone_ref(py), stream, protocol.clone_ref(py))?;
                     let transport_py = Py::new(py, transport)?;
                     
                     // Connection made
                     protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
                     // Start reading
                     let read_ready = transport_py.getattr(py, "_read_ready")?;
                     let fd = transport_py.borrow(py).fd;
                     if let Ok(bound) = self.loop_.bind(py).downcast::<VeloxLoop>() {
                          bound.borrow().add_reader(py, fd, read_ready)?;
                     } else {
                          self.loop_.call_method1(py, "add_reader", (fd, read_ready))?;
                     }
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
    loop_: Py<PyAny>,
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
             self_.loop_.call_method1(py, "add_writer", (fd, method))?;
        }
        Ok(())
    }
    
    fn _force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        let fd = self.fd;
        
        if let Ok(bound) = self.loop_.bind(py).downcast::<VeloxLoop>() {
             let loop_ = bound.borrow();
             loop_.remove_reader(py, fd)?;
             loop_.remove_writer(py, fd)?;
        } else {
             let _ = self.loop_.call_method1(py, "remove_reader", (fd,));
             let _ = self.loop_.call_method1(py, "remove_writer", (fd,));
        }
        
        self.stream = None; 
        
        // Notify Protocol
        let _ = self.protocol.call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn write(slf: &Bound<'_, Self>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let bytes = data.as_bytes();
        let py = slf.py();

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
                              if let Ok(bound) = self.loop_.bind(py).downcast::<VeloxLoop>() {
                                   bound.borrow().remove_writer(py, fd)?;
                              } else {
                                   self.loop_.call_method1(py, "remove_writer", (fd,))?;
                              }
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


impl AsyncConnectCallback {
    pub fn new(loop_: Py<PyAny>, future: Py<PyAny>, protocol_factory: Py<PyAny>, stream: std::net::TcpStream) -> Self {
        let fd = stream.as_raw_fd();
        Self {
            loop_,
            future,
            protocol_factory,
            stream: Some(stream),
            fd,
        }
    }
}

impl TcpServer {
    pub fn new(listener: std::net::TcpListener, loop_: Py<PyAny>, protocol_factory: Py<PyAny>) -> Self {
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
    pub fn new(py: Python<'_>, loop_: Py<PyAny>, stream: std::net::TcpStream, protocol: Py<PyAny>) -> VeloxResult<Self> {
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
        if let Ok(bound) = self.loop_.bind(py).downcast::<VeloxLoop>() {
             bound.borrow().add_writer(py, self.fd, method.unbind())?;
        } else {
             self.loop_.call_method1(py, "add_writer", (self.fd, method))?;
        }
        Ok(())
    }
}
