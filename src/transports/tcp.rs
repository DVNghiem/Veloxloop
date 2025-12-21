use bytes::BytesMut;
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use super::future::{CompletedFuture, PendingFuture};
use super::{StreamTransport, Transport, TransportFactory, TransportState};
use crate::constants::{DEFAULT_HIGH, DEFAULT_LOW};
use crate::event_loop::VeloxLoop;
use crate::transports::DefaultTransportFactory;

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
                "Transport endpoint is not connected",
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
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                    "Failed to set socket option: {}",
                    std::io::Error::last_os_error()
                )));
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
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                    "Failed to set socket option: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        Ok(())
    }
}

impl SocketWrapper {
    pub(crate) fn new(fd: RawFd, addr: SocketAddr) -> Self {
        Self {
            fd,
            addr,
            peer_addr: None,
        }
    }

    pub(crate) fn new_with_peer(fd: RawFd, addr: SocketAddr, peer_addr: SocketAddr) -> Self {
        Self {
            fd,
            addr,
            peer_addr: Some(peer_addr),
        }
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

    fn __aexit__(
        &mut self,
        py: Python<'_>,
        _exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _exc_tb: Py<PyAny>,
    ) -> PyResult<Py<PyAny>> {
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

                    let transport_py =
                        factory.create_tcp(py, loop_py, stream, protocol.clone_ref(py))?;

                    // Connection made
                    protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;

                    // Attempt to link StreamReader for direct path if it's a StreamReaderProtocol
                    if let Ok(reader_attr) = protocol.getattr(py, "_reader") {
                        if let Ok(reader) =
                            reader_attr.extract::<Py<crate::streams::StreamReader>>(py)
                        {
                            if let Ok(tcp_transport) = transport_py.extract::<Py<TcpTransport>>(py)
                            {
                                tcp_transport.bind(py).borrow_mut()._link_reader(reader);
                            }
                        }
                    }
                    // Start reading (native path)
                    let transport_clone = transport_py.extract::<Py<TcpTransport>>(py)?;
                    let fd = transport_clone.bind(py).borrow().fd;
                    self.loop_
                        .bind(py)
                        .borrow()
                        .add_tcp_reader(fd, transport_clone)?;
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
            use libc::{SO_REUSEADDR, SOL_SOCKET, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set SO_REUSEADDR: {}",
                        std::io::Error::last_os_error()
                    )));
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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set SO_REUSEPORT: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }
    /// Serve forever - runs the server until explicitly closed
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
                // Register the accept callback (native path)
                let slf_clone = slf.clone().unbind();
                let on_accept =
                    Arc::new(move |py: Python<'_>| slf_clone.bind(py).borrow()._on_accept(py));
                let loop_ = slf.borrow().loop_.clone_ref(py);
                loop_.bind(py).borrow().add_reader_native(fd, on_accept)?;
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
    state: TransportState,
    // Buffer for outgoing data
    write_buffer: BytesMut,
    // Write buffer limits (high water mark, low water mark)
    write_buffer_high: usize,
    write_buffer_low: usize,
    // Direct path to reader
    reader: Option<Py<crate::streams::StreamReader>>,
}

// Implement Transport trait for TcpTransport
impl crate::transports::Transport for TcpTransport {
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
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
            _ => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn is_closing(&self) -> bool {
        self.state.contains(TransportState::CLOSING) || self.state.contains(TransportState::CLOSED)
    }

    fn get_fd(&self) -> RawFd {
        self.fd
    }
}

// Implement StreamTransport trait for TcpTransport
impl crate::transports::StreamTransport for TcpTransport {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSING)
            || self.state.contains(TransportState::CLOSED)
        {
            return Ok(());
        }
        self.state.insert(TransportState::CLOSING);

        if self.write_buffer.is_empty() {
            self.force_close(py)?;
        } else {
            // Writer will be added to flush buffer
        }
        Ok(())
    }

    fn force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close_internal(py)
    }

    fn write(&mut self, _py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if let Some(mut stream) = self.stream.as_ref() {
            match stream.write(data) {
                Ok(n) if n == data.len() => {
                    // All written
                }
                Ok(n) => {
                    // Partial write - use extend for VecDeque
                    self.write_buffer.extend(&data[n..]);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.write_buffer.extend(data);
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

    fn set_write_buffer_limits(
        &mut self,
        py: Python<'_>,
        high: Option<usize>,
        low: Option<usize>,
    ) -> PyResult<()> {
        let high_limit = high.unwrap_or(DEFAULT_HIGH);
        let low_limit = low.unwrap_or_else(|| if high_limit == 0 { 0 } else { high_limit / 4 });

        // Special case: high=0 means disable flow control (both should be 0)
        // Otherwise, validate that low < high
        if high_limit > 0 && low_limit >= high_limit {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "low must be less than high",
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
        if let Some(reader_py) = &self.reader {
            if let Some(stream) = self.stream.as_mut() {
                let reader = reader_py.bind(py).borrow();
                // Read directly using StreamReader's optimized method
                match reader.read_from_socket(py, stream) {
                    Ok(0) => {
                        let _ = reader.feed_eof_native(py);
                        reader._wakeup_waiters(py)?;
                        self.close(py)?;
                    }
                    Ok(_) => {
                        // Data already in buffer via read_from_socket
                        reader._wakeup_waiters(py)?;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
            return Ok(());
        }

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
                    self.protocol
                        .call_method1(py, "data_received", (py_data,))?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut should_finalize = false;
        if let Some(stream) = self.stream.as_mut() {
            // Try to write as much as possible in one go
            while !self.write_buffer.is_empty() {
                let data = &self.write_buffer[..];

                match stream.write(data) {
                    Ok(0) => {
                        return Err(PyErr::new::<pyo3::exceptions::PyConnectionError, _>(
                            "Connection closed during write",
                        ));
                    }
                    Ok(n) => {
                        let _ = self.write_buffer.split_to(n);
                        if self.write_buffer.is_empty() {
                            let fd = self.fd;
                            self.loop_.bind(py).borrow().remove_writer(py, fd)?;

                            // If we are in CLOSING state and buffer is empty, finalize closure
                            if self.state.contains(TransportState::CLOSING) {
                                should_finalize = true;
                                break;
                            }
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

        if should_finalize {
            self._force_close_internal(py)?;
            let protocol = self.protocol.clone_ref(py);
            let _ = protocol.call_method1(py, "connection_lost", (py.None(),));
        }

        Ok(())
    }
}

#[pymethods]
impl TcpTransport {
    #[pyo3(signature = (name, default=None))]
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Delegate to trait implementation
        Transport::get_extra_info(self, py, name, default)
    }

    fn get_write_buffer_size(&self) -> usize {
        // Delegate to trait implementation
        StreamTransport::get_write_buffer_size(self)
    }

    #[pyo3(signature = (high=None, low=None))]
    fn set_write_buffer_limits(
        &mut self,
        py: Python<'_>,
        high: Option<usize>,
        low: Option<usize>,
    ) -> PyResult<()> {
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

            if !self_.state.contains(TransportState::READING_PAUSED) {
                self_.state.insert(TransportState::READING_PAUSED);
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

        if self_.state.contains(TransportState::READING_PAUSED) {
            self_.state.remove(TransportState::READING_PAUSED);
            let fd = self_.fd;
            let loop_obj = self_.loop_.clone_ref(py);
            drop(self_); // Drop borrow before calling into loop

            loop_obj
                .bind(py)
                .borrow()
                .add_tcp_reader(fd, slf.clone().unbind())?;
        }
        Ok(())
    }

    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut protocol = None;
        let mut needs_writer = false;

        {
            let mut self_ = slf.borrow_mut();
            if self_.state.contains(TransportState::CLOSING)
                || self_.state.contains(TransportState::CLOSED)
            {
                return Ok(());
            }

            self_.state.insert(TransportState::CLOSING);

            if self_.write_buffer.is_empty() {
                self_._force_close_internal(py)?;
                protocol = Some(self_.protocol.clone_ref(py));
            } else {
                needs_writer = true;
            }
        }

        // Notify protocol after dropping borrow
        if let Some(proto) = protocol {
            let _ = proto.call_method1(py, "connection_lost", (py.None(),));
        }

        if needs_writer {
            // Ensure writer is active to flush buffer
            let self_ = slf.borrow();
            let fd = self_.fd;
            self_
                .loop_
                .bind(py)
                .borrow()
                .add_tcp_writer(fd, slf.clone().unbind())?;
        }
        Ok(())
    }

    fn abort(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let protocol = {
            let mut self_ = slf.borrow_mut();
            self_._force_close_internal(py)?;
            self_.protocol.clone_ref(py)
        };
        let _ = protocol.call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn _force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close_internal(py)?;
        let _ = self
            .protocol
            .call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn _force_close_internal(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSED) {
            return Ok(());
        }

        let fd = self.fd;
        self.state.insert(TransportState::CLOSED);
        self.state.remove(TransportState::ACTIVE);
        self.state.remove(TransportState::CLOSING);

        let loop_ = self.loop_.bind(py).borrow();
        let _ = loop_.remove_reader(py, fd);
        let _ = loop_.remove_writer(py, fd);
        drop(loop_);

        self.stream = None;
        self.reader = None;
        Ok(())
    }

    /// Trigger write when data is added to buffer (called by StreamWriter)
    fn _trigger_write(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut self_ = slf.borrow_mut();

        if self_.state.contains(TransportState::CLOSING)
            || self_.state.contains(TransportState::CLOSED)
            || self_.stream.is_none()
        {
            return Ok(());
        }

        if !self_.write_buffer.is_empty() {
            // Try immediate write first
            let res = self_._write_ready(py);

            // If still have data, ensure writer callback is registered
            if !self_.write_buffer.is_empty() {
                let fd = self_.fd;
                let loop_ = self_.loop_.clone_ref(py);
                drop(self_); // Drop borrow before calling into loop
                loop_
                    .bind(py)
                    .borrow()
                    .add_tcp_writer(fd, slf.clone().unbind())?;
            }
            res
        } else {
            Ok(())
        }
    }

    /// Link a StreamReader specialized for direct Rust-level data feeding
    pub(crate) fn _link_reader(&mut self, reader: Py<crate::streams::StreamReader>) {
        self.reader = Some(reader);
    }

    fn write(slf: &Bound<'_, Self>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let bytes = data.as_bytes();
        let mut self_ = slf.borrow_mut();

        // Delegate to trait implementation
        StreamTransport::write(&mut *self_, slf.py(), bytes)?;

        // Register writer if needed
        if !self_.write_buffer.is_empty() {
            let fd = self_.fd;
            let loop_ = self_.loop_.clone_ref(slf.py());
            drop(self_);
            loop_
                .bind(slf.py())
                .borrow()
                .add_tcp_writer(fd, slf.clone().unbind())?;
        }
        Ok(())
    }

    // Internal callback called by loop when writable
    pub(crate) fn _write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::write_ready(self, py)
    }

    pub(crate) fn _read_ready(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut batch = BytesMut::new();
        let mut eof = false;

        loop {
            let self_ = slf.borrow();
            if self_.state.intersects(
                TransportState::CLOSING | TransportState::CLOSED | TransportState::READING_PAUSED,
            ) {
                break;
            }

            if let Some(mut stream) = self_.stream.as_ref() {
                let mut buf = [0u8; 65536];
                match stream.read(&mut buf) {
                    Ok(0) => {
                        eof = true;
                        break;
                    }
                    Ok(n) => {
                        batch.extend_from_slice(&buf[..n]);
                        // If we already have a lot of data, dispatch it to avoid excessive memory usage
                        if batch.len() >= 128 * 1024 {
                            let data = batch.split().to_vec();
                            drop(self_); // Drop borrow before calling out
                            Self::_dispatch_batch(slf, py, data)?;
                        } else {
                            // Continue reading more from socket
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e.into()),
                }
            } else {
                break;
            }
        }

        // Dispatch remaining data in batch
        if !batch.is_empty() {
            Self::_dispatch_batch(slf, py, batch.to_vec())?;
        }

        if eof {
            // EOF handling: call eof_received and only close if it returns false
            let protocol = {
                let self_ = slf.borrow();
                self_.protocol.clone_ref(py)
            };

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

        Ok(())
    }

    fn _dispatch_batch(slf: &Bound<'_, Self>, py: Python<'_>, data: Vec<u8>) -> PyResult<()> {
        let self_ = slf.borrow();
        if let Some(reader) = self_.reader.as_ref().map(|r| r.clone_ref(py)) {
            drop(self_);
            reader.bind(py).borrow().feed_data_native(py, &data)
        } else {
            let py_data = PyBytes::new(py, &data);
            let protocol = self_.protocol.clone_ref(py);
            drop(self_);
            protocol.call_method1(py, "data_received", (py_data,))?;
            Ok(())
        }
    }

    /// Set TCP_NODELAY option on the socket
    fn set_tcp_nodelay(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{IPPROTO_TCP, TCP_NODELAY, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set TCP_NODELAY: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Set SO_KEEPALIVE option on the socket
    fn set_keepalive(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{SO_KEEPALIVE, SOL_SOCKET, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set SO_KEEPALIVE: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Set SO_REUSEADDR option on the socket
    fn set_reuse_address(&self, enabled: bool) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{SO_REUSEADDR, SOL_SOCKET, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set SO_REUSEADDR: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive time (idle time before first probe in seconds)
    #[cfg(target_os = "linux")]
    fn set_keepalive_time(&self, seconds: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{IPPROTO_TCP, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set TCP_KEEPIDLE: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive interval between probes (in seconds)
    #[cfg(target_os = "linux")]
    fn set_keepalive_interval(&self, seconds: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{IPPROTO_TCP, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set TCP_KEEPINTVL: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Set TCP keep-alive probe count
    #[cfg(target_os = "linux")]
    fn set_keepalive_count(&self, count: u32) -> PyResult<()> {
        if let Some(stream) = self.stream.as_ref() {
            use libc::{IPPROTO_TCP, setsockopt};
            use std::os::unix::io::AsRawFd;

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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Failed to set TCP_KEEPCNT: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }
        Ok(())
    }
}

impl TcpServer {
    pub fn new(
        listener: std::net::TcpListener,
        loop_: Py<VeloxLoop>,
        protocol_factory: Py<PyAny>,
    ) -> Self {
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
    pub fn new(
        loop_: Py<VeloxLoop>,
        stream: std::net::TcpStream,
        protocol: Py<PyAny>,
    ) -> PyResult<Self> {
        stream.set_nonblocking(true)?;
        let fd = stream.as_raw_fd();

        Ok(Self {
            fd,
            stream: Some(stream),
            protocol,
            loop_,
            state: TransportState::ACTIVE,
            write_buffer: BytesMut::with_capacity(65536),
            write_buffer_high: DEFAULT_HIGH,
            write_buffer_low: DEFAULT_LOW,
            reader: None,
        })
    }
}
