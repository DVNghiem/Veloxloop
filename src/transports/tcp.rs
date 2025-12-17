use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::net::{TcpStream, SocketAddr};
use std::os::fd::{AsRawFd, RawFd};
use std::io::{Write, self};
use parking_lot::Mutex;

use crate::utils::VeloxResult;
use crate::event_loop::VeloxLoop;
use super::future::{CompletedFuture, PendingFuture};
use crate::transports::{TransportFactory, DefaultTransportFactory, StreamTransport, Transport};

// Pure Rust socket wrapper to avoid importing Python's socket module
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SocketWrapper {
    fd: RawFd,
    addr: SocketAddr,
    peer_addr: Option<SocketAddr>,
}

#[pymethods]
impl SocketWrapper {
    fn getsockname(&self) -> PyResult<(String, u16)> {
        Ok((self.addr.ip().to_string(), self.addr.port()))
    }
    
    fn getpeername(&self) -> PyResult<(String, u16)> {
        if let Some(peer) = self.peer_addr {
            Ok((peer.ip().to_string(), peer.port()))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                "Transport endpoint is not connected"
            ))
        }
    }
    
    #[getter]
    fn family(&self) -> i32 {
        match self.addr {
            SocketAddr::V4(_) => libc::AF_INET,
            SocketAddr::V6(_) => libc::AF_INET6,
        }
    }
    
    fn fileno(&self) -> RawFd {
        self.fd
    }

    /// Get IPv6-specific information (flowinfo and scope_id for IPv6 addresses)
    fn get_ipv6_info(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.addr {
            SocketAddr::V6(addr) => {
                let flowinfo = addr.flowinfo();
                let scope_id = addr.scope_id();
                
                let info = pyo3::types::PyDict::new(py);
                info.set_item("flowinfo", flowinfo)?;
                info.set_item("scope_id", scope_id)?;
                
                Ok(Some(info.into()))
            }
            SocketAddr::V4(_) => Ok(None),
        }
    }

    /// Set socket options
    /// This is a simplified implementation that supports common options
    #[cfg(unix)]
    fn setsockopt(&self, level: i32, optname: i32, value: i32) -> PyResult<()> {
        use libc::setsockopt;
        
        unsafe {
            let optval = value as libc::c_int;
            let ret = setsockopt(
                self.fd,
                level,
                optname,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of_val(&optval) as libc::socklen_t,
            );
            if ret != 0 {
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    format!(
                        "Failed to set socket option: {}",
                        std::io::Error::last_os_error()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Set socket options (Windows version)
    #[cfg(windows)]
    fn setsockopt(&self, level: i32, optname: i32, value: i32) -> PyResult<()> {
        use winapi::um::winsock2::setsockopt;
        
        unsafe {
            let optval = value as i32;
            let ret = setsockopt(
                self.fd as usize,
                level,
                optname,
                &optval as *const _ as *const i8,
                std::mem::size_of_val(&optval) as i32,
            );
            if ret != 0 {
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    format!(
                        "Failed to set socket option: {}",
                        std::io::Error::last_os_error()
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl SocketWrapper {
    pub(crate) fn new(fd: RawFd, addr: SocketAddr) -> Self {
        Self { fd, addr, peer_addr: None }
    }
    
    pub(crate) fn new_with_peer(fd: RawFd, addr: SocketAddr, peer_addr: SocketAddr) -> Self {
        Self { fd, addr, peer_addr: Some(peer_addr) }
    }
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct TcpServer {
    listener: Option<std::net::TcpListener>,
    loop_: Py<VeloxLoop>,
    protocol_factory: Py<PyAny>,
    active: bool,
    serve_forever_future: Mutex<Option<Py<PendingFuture>>>,
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
        
        // Resolve serve_forever future if it exists
        if let Some(future) = self.serve_forever_future.lock().as_ref() {
            future.bind(py).borrow().set_result(py, py.None())?;
        }
        
        Ok(())
    }
    
    fn get_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.loop_.clone_ref(py).into_any())
    }
    
    fn is_serving(&self) -> bool {
         self.active
    }
    
    pub fn fd(&self) -> Option<RawFd> {
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
                     // Create Transport using factory
                     let factory = DefaultTransportFactory;
                     let loop_py = self.loop_.clone_ref(py).into_any();
                     
                     let transport_py = factory.create_tcp(
                         py,
                         loop_py,
                         stream,
                         protocol.clone_ref(py),
                     )?;
                     
                     // Connection made
                     protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
                     // Start reading
                     let read_ready = transport_py.getattr(py, "_read_ready")?;
                     let fd = transport_py.getattr(py, "fileno")?.call0(py)?.extract::<i32>(py)?;
                     self.loop_.bind(py).borrow().add_reader(py, fd, read_ready)?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Set SO_REUSEADDR option on the server socket
    fn set_reuse_address(&self, enabled: bool) -> PyResult<()> {
        if let Some(listener) = self.listener.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, SOL_SOCKET, SO_REUSEADDR};
            
            let fd = listener.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if enabled { 1 } else { 0 };
                let ret = setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_REUSEADDR,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set SO_REUSEADDR: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set SO_REUSEPORT option on the server socket (Unix only, not Solaris)
    #[cfg(all(unix, not(target_os = "solaris")))]
    fn set_reuse_port(&self, enabled: bool) -> PyResult<()> {
        if let Some(listener) = self.listener.as_ref() {
            use std::os::unix::io::AsRawFd;
            
            let fd = listener.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if enabled { 1 } else { 0 };
                let ret = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEPORT,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set SO_REUSEPORT: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }
    /// Serve forever - runs the server until explicitly closed
    /// This method implements asyncio.Server.serve_forever() behavior
    fn serve_forever(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Create a PendingFuture that will be resolved when close() is called
        let future = Py::new(py, PendingFuture::new())?;
        *self.serve_forever_future.lock() = Some(future.clone_ref(py));
        
        Ok(future.into_any())
    }
    
    /// Start serving - begin accepting connections
    fn start_serving(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut self_ = slf.borrow_mut();
        
        if !self_.active {
            self_.active = true;
            if let Some(listener) = self_.listener.as_ref() {
                let fd = listener.as_raw_fd();
                // Register the accept callback
                drop(self_); // Drop the mutable borrow before calling getattr
                let on_accept = slf.getattr("_on_accept")?;
                let loop_ = slf.borrow().loop_.clone_ref(py);
                loop_.bind(py).borrow().add_reader(py, fd, on_accept.unbind())?;
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
    reading_paused: bool,
    // Buffer for outgoing data? Asyncio transports buffer if socket is full.
    // For MVP we might BLOCK or fail if full? No, we must buffer.
    write_buffer: Vec<u8>,
    // Write buffer limits (high water mark, low water mark)
    write_buffer_high: usize,
    write_buffer_low: usize,
}


// Implement Transport trait for TcpTransport
impl crate::transports::Transport for TcpTransport {
    fn get_extra_info(&self, py: Python<'_>, name: &str, default: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        // Delegate to the pymethods implementation
        match name {
            "peername" => {
                if let Some(stream) = self.stream.as_ref() {
                    if let Ok(addr) = stream.peer_addr() {
                        return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                    }
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "sockname" => {
                if let Some(stream) = self.stream.as_ref() {
                    if let Ok(addr) = stream.local_addr() {
                        return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                    }
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "socket" => {
                if let Some(stream) = self.stream.as_ref() {
                    let fd = stream.as_raw_fd();
                    if let (Ok(addr), Ok(peer_addr)) = (stream.local_addr(), stream.peer_addr()) {
                        let socket_wrapper = SocketWrapper::new_with_peer(fd, addr, peer_addr);
                        return Ok(Py::new(py, socket_wrapper)?.into_any());
                    } else if let Ok(addr) = stream.local_addr() {
                        let socket_wrapper = SocketWrapper::new(fd, addr);
                        return Ok(Py::new(py, socket_wrapper)?.into_any());
                    }
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            _ => Ok(default.unwrap_or_else(|| py.None()))
        }
    }
    
    fn is_closing(&self) -> bool {
        self.closing
    }
    
    fn get_fd(&self) -> RawFd {
        self.fd
    }
    
}

// Implement StreamTransport trait for TcpTransport
impl crate::transports::StreamTransport for TcpTransport {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.closing {
            return Ok(());
        }
        
        self.closing = true;
        
        if self.write_buffer.is_empty() {
            self.force_close(py)?;
        } else {
            // Writer will be added to flush buffer
        }
        Ok(())
    }
    
    fn force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close(py)
    }
    
    fn write(&mut self, _py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            match stream.write(data) {
                Ok(n) if n == data.len() => {
                    // All written
                }
                Ok(n) => {
                    // Partial write
                    self.write_buffer.extend_from_slice(&data[n..]);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.write_buffer.extend_from_slice(data);
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
    
    fn write_eof(&mut self) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            stream.shutdown(std::net::Shutdown::Write)?;
        }
        Ok(())
    }
    
    fn get_write_buffer_size(&self) -> usize {
        self.write_buffer.len()
    }
    
    fn set_write_buffer_limits(&mut self, py: Python<'_>, high: Option<usize>, low: Option<usize>) -> PyResult<()> {
        const DEFAULT_HIGH: usize = 64 * 1024;
        
        let high_limit = high.unwrap_or(DEFAULT_HIGH);
        let low_limit = low.unwrap_or_else(|| {
            if high_limit == 0 {
                0
            } else {
                high_limit / 4
            }
        });
        
        // Special case: high=0 means disable flow control (both should be 0)
        // Otherwise, validate that low < high
        if high_limit > 0 && low_limit >= high_limit {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "low must be less than high"
            ));
        }
        
        self.write_buffer_high = high_limit;
        self.write_buffer_low = low_limit;
        
        if high_limit > 0 && self.write_buffer.len() > self.write_buffer_high {
            let _ = self.protocol.call_method0(py, "pause_writing");
        }
        
        Ok(())
    }
    
    fn read_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            let mut buf = [0u8; 65536]; // Increased from 4KB to 64KB for better large message performance
            let mut s = stream;
            match std::io::Read::read(&mut s, &mut buf) {
                Ok(0) => {
                    // EOF
                    if let Ok(res) = self.protocol.call_method0(py, "eof_received") {
                        if let Ok(keep_open) = res.extract::<bool>(py) {
                            if !keep_open {
                                self.close(py)?;
                            }
                        } else {
                            self.close(py)?;
                        }
                    } else {
                        self.close(py)?;
                    }
                }
                Ok(n) => {
                    let py_data = PyBytes::new(py, &buf[..n]);
                    self.protocol.call_method1(py, "data_received", (py_data,))?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    
    fn write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            if !self.write_buffer.is_empty() {
                // Try to write as much as possible in one go
                loop {
                    match stream.write(&self.write_buffer) {
                        Ok(0) => {
                            // Connection closed
                            return Err(PyErr::new::<pyo3::exceptions::PyConnectionError, _>(
                                "Connection closed during write"
                            ));
                        }
                        Ok(n) => {
                            self.write_buffer.drain(0..n);
                            if self.write_buffer.is_empty() {
                                let fd = self.fd;
                                self.loop_.bind(py).borrow().remove_writer(py, fd)?;
                                break;
                            }
                            // Continue writing remaining data
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            // Can't write more now, will retry on next write_ready
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
}

#[pymethods]
impl TcpTransport {
    #[pyo3(signature = (name, default=None))]
    fn get_extra_info(&self, py: Python<'_>, name: &str, default: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        // Delegate to trait implementation
        Transport::get_extra_info(self, py, name, default)
    }

    fn get_write_buffer_size(&self) -> usize {
        // Delegate to trait implementation
        StreamTransport::get_write_buffer_size(self)
    }
    
    #[pyo3(signature = (high=None, low=None))]
    fn set_write_buffer_limits(&mut self, py: Python<'_>, high: Option<usize>, low: Option<usize>) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::set_write_buffer_limits(self, py, high, low)
    }
    
    fn write_eof(&mut self) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::write_eof(self)
    }

    fn is_closing(&self) -> bool {
        // Delegate to trait implementation
        Transport::is_closing(self)
    }
    
    fn fileno(&self) -> RawFd {
        // Delegate to trait implementation
        Transport::get_fd(self)
    }

    fn pause_reading(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let (should_remove, fd, loop_obj) = {
            let mut self_ = slf.borrow_mut();
            
            if !self_.reading_paused {
                self_.reading_paused = true;
                let fd = self_.fd;
                let loop_obj = self_.loop_.clone_ref(py);
                (true, fd, loop_obj)
            } else {
                return Ok(());
            }
        }; // Drop mutable borrow before calling into loop
        
        if should_remove {
            let loop_ = loop_obj.bind(py).borrow();
            loop_.remove_reader(py, fd)?;
        }
        Ok(())
    }

    fn resume_reading(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut self_ = slf.borrow_mut();
        
        if self_.reading_paused {
            self_.reading_paused = false;
            let fd = self_.fd;
            drop(self_); // Drop borrow before calling getattr
            let method = slf.getattr("_read_ready")?;
            let self_ = slf.borrow();
            let loop_ = self_.loop_.bind(py).borrow();
            loop_.add_reader(py, fd, method.unbind())?;
        }
        Ok(())
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
        
        {
             let mut self_ = slf.borrow_mut();
             // Delegate to trait implementation
             StreamTransport::close(&mut *self_, py)?;
             needs_writer = !self_.write_buffer.is_empty();
        }
        
        if needs_writer {
             // Ensure writer is active to flush buffer
             let self_ = slf.borrow();
             let fd = self_.fd;
             let method = slf.getattr("_write_ready")?;
             self_.loop_.bind(py).borrow().add_writer(py, fd, method.unbind())?;
        }
        Ok(())
    }
    
    fn abort(&mut self, py: Python<'_>) -> PyResult<()> {
        // Immediate close without flushing
        self._force_close(py)
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
        
        // Delegate to trait implementation
        StreamTransport::write(&mut *self_, slf.py(), bytes)?;
        
        // Register writer if needed
        if !self_.write_buffer.is_empty() {
            drop(self_);
            slf.borrow().add_writer(slf)?;
        }
        Ok(())
    }
    
    // Internal callback called by loop when writable
    fn _write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::write_ready(self, py)
    }

    fn _read_ready(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        
        // Read data and prepare callbacks without holding the mutable borrow
        enum Action {
            Data(Vec<u8>),
            Eof,
            WouldBlock,
            Error(io::Error),
        }
        
        let action = {
            let self_ = slf.borrow();
            if let Some(stream) = self_.stream.as_ref() {
                let mut buf = [0u8; 4096];
                let mut s = stream;
                match std::io::Read::read(&mut s, &mut buf) {
                    Ok(0) => Action::Eof,
                    Ok(n) => Action::Data(buf[..n].to_vec()),
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Action::WouldBlock,
                    Err(e) => Action::Error(e),
                }
            } else {
                Action::WouldBlock
            }
        }; // Drop borrow before calling Python callbacks
        
        match action {
            Action::Data(data) => {
                let py_data = PyBytes::new(py, &data);
                let protocol = slf.borrow().protocol.clone_ref(py);
                protocol.call_method1(py, "data_received", (py_data,))?;
            }
            Action::Eof => {
                let protocol = slf.borrow().protocol.clone_ref(py);
                if let Ok(res) = protocol.call_method0(py, "eof_received") {
                    if let Ok(keep_open) = res.extract::<bool>(py) {
                        if !keep_open {
                            Self::close(slf)?;
                        }
                    } else {
                        Self::close(slf)?;
                    }
                } else {
                    Self::close(slf)?;
                }
            }
            Action::WouldBlock => {}
            Action::Error(e) => return Err(e.into()),
        }
        
        Ok(())
    }

    /// Set TCP_NODELAY option on the socket
    fn set_tcp_nodelay(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, IPPROTO_TCP, TCP_NODELAY};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if enabled { 1 } else { 0 };
                let ret = setsockopt(
                    fd,
                    IPPROTO_TCP,
                    TCP_NODELAY,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set TCP_NODELAY: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set SO_KEEPALIVE option on the socket
    fn set_keepalive(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, SOL_SOCKET, SO_KEEPALIVE};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if enabled { 1 } else { 0 };
                let ret = setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_KEEPALIVE,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set SO_KEEPALIVE: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set SO_REUSEADDR option on the socket
    fn set_reuse_address(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, SOL_SOCKET, SO_REUSEADDR};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if enabled { 1 } else { 0 };
                let ret = setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_REUSEADDR,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set SO_REUSEADDR: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive time (idle time before first probe in seconds)
    #[cfg(target_os = "linux")]
    fn set_keepalive_time(&self, seconds: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, IPPROTO_TCP};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval = seconds as libc::c_int;
                let ret = setsockopt(
                    fd,
                    IPPROTO_TCP,
                    libc::TCP_KEEPIDLE,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set TCP_KEEPIDLE: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive interval between probes (in seconds)
    #[cfg(target_os = "linux")]
    fn set_keepalive_interval(&self, seconds: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, IPPROTO_TCP};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval = seconds as libc::c_int;
                let ret = setsockopt(
                    fd,
                    IPPROTO_TCP,
                    libc::TCP_KEEPINTVL,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set TCP_KEEPINTVL: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive probe count
    #[cfg(target_os = "linux")]
    fn set_keepalive_count(&self, count: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use std::os::unix::io::AsRawFd;
            use libc::{setsockopt, IPPROTO_TCP};
            
            let fd = stream.as_raw_fd();
            unsafe {
                let optval = count as libc::c_int;
                let ret = setsockopt(
                    fd,
                    IPPROTO_TCP,
                    libc::TCP_KEEPCNT,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if ret != 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set TCP_KEEPCNT: {}", std::io::Error::last_os_error()),
                    ));
                }
            }
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
            serve_forever_future: Mutex::new(None),
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
        
        // Default buffer limits from asyncio
        const DEFAULT_HIGH: usize = 64 * 1024;  // 64 KB
        const DEFAULT_LOW: usize = 16 * 1024;   // 16 KB (64KB / 4)
        
        let transport = Self {
            fd,
            stream: Some(stream),
            protocol,
            loop_,
            closing: false,
            reading_paused: false,
            write_buffer: Vec::new(),
            write_buffer_high: DEFAULT_HIGH,
            write_buffer_low: DEFAULT_LOW,
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
