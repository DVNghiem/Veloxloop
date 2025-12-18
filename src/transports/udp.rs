use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt, PyString, PyTuple};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};

use crate::event_loop::VeloxLoop;
use crate::transports::{DatagramTransport, Transport};
use crate::utils::VeloxResult;

/// Pure Rust UDP socket wrapper to avoid importing Python's socket module
#[pyclass(module = "veloxloop._veloxloop")]
pub struct UdpSocketWrapper {
    fd: RawFd,
    addr: SocketAddr,
}

#[pymethods]
impl UdpSocketWrapper {
    fn getsockname(&self) -> PyResult<(String, u16)> {
        Ok((self.addr.ip().to_string(), self.addr.port()))
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
}

impl UdpSocketWrapper {
    fn new(fd: RawFd, addr: SocketAddr) -> Self {
        Self { fd, addr }
    }
}

/// UDP/Datagram Transport implementation
#[pyclass(module = "veloxloop._veloxloop")]
pub struct UdpTransport {
    fd: RawFd,
    socket: Mutex<Option<UdpSocket>>,
    protocol: Py<PyAny>,
    loop_: Py<VeloxLoop>,
    closing: bool,
    local_addr: Option<SocketAddr>,
    remote_addr: Option<SocketAddr>,
    allow_broadcast: bool,
}

impl crate::transports::Transport for UdpTransport {
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match name {
            "socket" => {
                if let Some(ref addr) = self.local_addr {
                    let socket_wrapper = UdpSocketWrapper::new(self.fd, *addr);
                    return Ok(Py::new(py, socket_wrapper)?.into_any());
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "sockname" => {
                if let Some(addr) = self.local_addr {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "peername" => {
                if let Some(addr) = self.remote_addr {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            _ => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn is_closing(&self) -> bool {
        self.closing
    }

    fn get_fd(&self) -> RawFd {
        self.fd
    }
}

// Implement DatagramTransport trait for UdpTransport
impl crate::transports::DatagramTransport for UdpTransport {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.closing {
            return Ok(());
        }

        self.closing = true;
        self._force_close(py)?;
        Ok(())
    }

    fn sendto(&self, _py: Python<'_>, data: &[u8], addr: Option<(String, u16)>) -> PyResult<()> {
        if self.closing {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Cannot send on closing transport",
            ));
        }

        let socket_guard = self.socket.lock();
        if let Some(socket) = socket_guard.as_ref() {
            let result = if let Some((ip, port)) = addr {
                let target_addr: std::net::SocketAddr =
                    format!("{}:{}", ip, port).parse().map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Invalid address: {}",
                            e
                        ))
                    })?;
                socket.send_to(data, target_addr)
            } else if let Some(remote) = self.remote_addr {
                socket.send_to(data, remote)
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "No address specified and socket is not connected",
                ));
            };

            match result {
                Ok(_) => Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    Err(PyErr::new::<pyo3::exceptions::PyBlockingIOError, _>(
                        "Socket buffer full",
                    ))
                }
                Err(e) => Err(e.into()),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Socket is closed",
            ))
        }
    }

    fn abort(&mut self, py: Python<'_>) -> PyResult<()> {
        self.close(py)
    }

    fn read_ready(&self, py: Python<'_>) -> PyResult<()> {
        let socket_guard = self.socket.lock();

        if let Some(socket) = socket_guard.as_ref() {
            let mut buf = [0u8; 65536];

            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    drop(socket_guard);

                    let py_data = PyBytes::new(py, &buf[..n]);
                    let py_addr = crate::utils::ipv6::socket_addr_to_tuple(py, addr)?;

                    self.protocol
                        .call_method1(py, "datagram_received", (py_data, py_addr))?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    drop(socket_guard);
                    let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string());
                    self.protocol
                        .call_method1(py, "error_received", (py_err,))?;
                }
            }
        }
        Ok(())
    }
}

#[pymethods]
impl UdpTransport {
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

    fn is_closing(&self) -> bool {
        // Delegate to trait implementation
        Transport::is_closing(self)
    }

    fn fileno(&self) -> RawFd {
        // Delegate to trait implementation
        Transport::get_fd(self)
    }

    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        {
            let self_ = slf.borrow();
            if self_.closing {
                return Ok(());
            }
        }

        {
            let mut self_ = slf.borrow_mut();
            // Delegate to trait implementation
            DatagramTransport::close(&mut *self_, py)?;
        }
        Ok(())
    }

    fn _force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        let fd = self.fd;

        let loop_ = self.loop_.bind(py).borrow();
        loop_.remove_reader(py, fd)?;
        drop(loop_);

        *self.socket.lock() = None;

        // Notify Protocol
        let _ = self
            .protocol
            .call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    /// Send data to the remote peer (for connected sockets)
    #[pyo3(signature = (data, addr=None))]
    fn sendto(
        slf: &Bound<'_, Self>,
        data: &Bound<'_, PyBytes>,
        addr: Option<(String, u16)>,
    ) -> PyResult<()> {
        let bytes = data.as_bytes();
        let self_ = slf.borrow();

        if self_.closing {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Cannot send on closing transport",
            ));
        }

        let socket_guard = self_.socket.lock();
        if let Some(socket) = socket_guard.as_ref() {
            let result = if let Some((ip, port)) = addr {
                // sendto() with explicit address
                let target_addr: SocketAddr = format!("{}:{}", ip, port).parse().map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                        "Invalid address: {}",
                        e
                    ))
                })?;
                socket.send_to(bytes, target_addr)
            } else if let Some(remote) = self_.remote_addr {
                // Connected socket - send to remote_addr
                socket.send_to(bytes, remote)
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "No address specified and socket is not connected",
                ));
            };

            match result {
                Ok(_) => Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // For UDP, we could buffer or just drop
                    // asyncio UDP doesn't buffer writes, it may raise BlockingIOError
                    Err(PyErr::new::<pyo3::exceptions::PyBlockingIOError, _>(
                        "Socket buffer full",
                    ))
                }
                Err(e) => Err(e.into()),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Socket is closed",
            ))
        }
    }

    /// Abort the transport immediately
    fn abort(slf: &Bound<'_, Self>) -> PyResult<()> {
        Self::close(slf)
    }

    /// Internal callback called by loop when readable
    fn _read_ready(&self, py: Python<'_>) -> PyResult<()> {
        let socket_guard = self.socket.lock();

        if let Some(socket) = socket_guard.as_ref() {
            let mut buf = [0u8; 65536]; // Max UDP packet size

            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    drop(socket_guard); // Release lock before calling Python

                    let data = PyBytes::new(py, &buf[..n]);
                    let addr_tuple = PyTuple::new(
                        py,
                        vec![
                            PyString::new(py, &addr.ip().to_string()).as_any(),
                            PyInt::new(py, addr.port()).as_any(),
                        ],
                    )?;

                    // Call protocol.datagram_received(data, addr)
                    let _ = self
                        .protocol
                        .call_method1(py, "datagram_received", (data, addr_tuple));
                    Ok(())
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data available
                    Ok(())
                }
                Err(e) => {
                    drop(socket_guard); // Release lock before calling Python

                    // Call protocol.error_received(exc)
                    let exc = pyo3::exceptions::PyOSError::new_err(e.to_string());
                    let exc_obj = exc.value(py);
                    let _ = self.protocol.call_method1(py, "error_received", (exc_obj,));
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }

    /// Set broadcast option on the socket
    fn set_broadcast(&mut self, enable: bool) -> PyResult<()> {
        let socket_guard = self.socket.lock();
        if let Some(socket) = socket_guard.as_ref() {
            socket.set_broadcast(enable)?;
            drop(socket_guard);
            self.allow_broadcast = enable;
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Socket is closed",
            ))
        }
    }

    /// Get the loop associated with this transport
    fn get_loop(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.loop_.clone_ref(py).into_any())
    }
}

impl UdpTransport {
    /// Create a new UDP transport
    pub fn new(
        loop_: Py<VeloxLoop>,
        socket: UdpSocket,
        protocol: Py<PyAny>,
        remote_addr: Option<SocketAddr>,
    ) -> VeloxResult<Self> {
        socket.set_nonblocking(true)?;
        let fd = socket.as_raw_fd();
        let local_addr = socket.local_addr().ok();

        Ok(Self {
            fd,
            socket: Mutex::new(Some(socket)),
            protocol,
            loop_,
            closing: false,
            local_addr,
            remote_addr,
            allow_broadcast: false,
        })
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}
