use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};

use super::TransportState;
use crate::event_loop::VeloxLoop;
use crate::utils::VeloxResult;

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
    state: TransportState,
    local_addr: Option<SocketAddr>,
    remote_addr: Option<SocketAddr>,
}

impl crate::transports::Transport for UdpTransport {
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match name {
            "addr" => {
                if let Some(addr) = self.local_addr {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?.into_any());
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "sockname" => {
                if let Some(addr) = self.local_addr {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?.into_any());
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "peername" => {
                if let Some(addr) = self.remote_addr {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?.into_any());
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "socket" => {
                if let Some(addr) = self.local_addr {
                    let socket_wrapper = UdpSocketWrapper::new(self.fd, addr);
                    return Ok(Py::new(py, socket_wrapper)?.into_any());
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

#[pymethods]
impl UdpTransport {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.is_closing() {
            return Ok(());
        }
        self.state.insert(TransportState::CLOSING);
        self.abort(py)
    }

    fn abort(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSED) {
            return Ok(());
        }
        self.state.insert(TransportState::CLOSED);
        self.state.remove(TransportState::ACTIVE);
        self.state.remove(TransportState::CLOSING);

        if let Some(socket) = self.socket.lock().take() {
            let loop_ = self.loop_.bind(py).borrow();
            let _ = loop_.remove_reader(py, self.fd);
            drop(socket);
        }

        let protocol = self.protocol.clone_ref(py);
        let _ = protocol.call_method1(py, "connection_lost", (py.None(),));

        Ok(())
    }

    #[pyo3(signature = (data, addr=None))]
    fn sendto(&self, _py: Python<'_>, data: &[u8], addr: Option<(String, u16)>) -> PyResult<()> {
        if self.is_closing() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Transport is closing or closed",
            ));
        }

        let socket_guard = self.socket.lock();
        if let Some(socket) = socket_guard.as_ref() {
            match addr {
                Some((host, port)) => {
                    let target_addr = format!("{}:{}", host, port);
                    socket.send_to(data, target_addr)?;
                }
                None => {
                    if let Some(_remote) = self.remote_addr {
                        socket.send(data)?;
                    } else {
                        return Err(pyo3::exceptions::PyValueError::new_err(
                            "Sendto requires an address for unconnected sockets",
                        ));
                    }
                }
            }
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Socket is closed",
            ))
        }
    }

    fn get_write_buffer_size(&self) -> usize {
        0 // UDP has no write buffer in this implementation
    }

    fn is_closing(&self) -> bool {
        self.state.contains(TransportState::CLOSING) || self.state.contains(TransportState::CLOSED)
    }

    fn fileno(&self) -> RawFd {
        self.fd
    }

    fn get_loop(&self, py: Python<'_>) -> Py<VeloxLoop> {
        self.loop_.clone_ref(py)
    }

    #[pyo3(signature = (name, default=None))]
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> Option<Py<PyAny>> {
        match name {
            "socket" => {
                let socket_guard = self.socket.lock();
                if let Some(socket) = socket_guard.as_ref() {
                    let fd = socket.as_raw_fd();
                    let addr = socket
                        .local_addr()
                        .unwrap_or(self.local_addr.unwrap_or("0.0.0.0:0".parse().unwrap()));
                    let socket_wrapper = crate::transports::udp::UdpSocketWrapper { fd, addr };
                    Py::new(py, socket_wrapper).ok().map(|s| s.into_any())
                } else {
                    default
                }
            }
            "sockname" => {
                if let Some(addr) = self.local_addr {
                    crate::utils::ipv6::socket_addr_to_tuple(py, addr)
                        .ok()
                        .map(|t| t.into_any())
                } else {
                    default
                }
            }
            "peername" => {
                if let Some(addr) = self.remote_addr {
                    crate::utils::ipv6::socket_addr_to_tuple(py, addr)
                        .ok()
                        .map(|t| t.into_any())
                } else {
                    default
                }
            }
            _ => default,
        }
    }
}

impl UdpTransport {
    pub(crate) fn _read_ready(&self, py: Python<'_>) -> PyResult<()> {
        if self.is_closing() {
            return Ok(());
        }

        let socket_guard = self.socket.lock();
        if let Some(socket) = socket_guard.as_ref() {
            let mut buf = [0u8; 65536];
            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let data = PyBytes::new(py, &buf[..n]);
                    let addr_tuple = crate::utils::ipv6::socket_addr_to_tuple(py, addr)?;
                    let protocol = self.protocol.clone_ref(py);
                    drop(socket_guard);
                    protocol.call_method1(py, "datagram_received", (data, addr_tuple))?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data available
                }
                Err(e) => {
                    drop(socket_guard);
                    let protocol = self.protocol.clone_ref(py);
                    let _ = protocol.call_method1(py, "error_received", (e.to_string(),));
                }
            }
        }
        Ok(())
    }

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
            state: TransportState::ACTIVE,
            local_addr,
            remote_addr,
        })
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}
