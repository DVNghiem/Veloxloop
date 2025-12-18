use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt, PyString, PyTuple};
use std::collections::VecDeque;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use crate::event_loop::VeloxLoop;
use crate::poller::LoopPoller;
use crate::transports::future::PendingFuture;
use crate::transports::ssl::SSLContext;
use crate::transports::{DefaultTransportFactory, TransportFactory};

pub struct Callback {
    pub callback: Py<PyAny>,
    pub args: Vec<Py<PyAny>>, // Minimal args, usually Context + Args

    #[allow(dead_code)] // For future use
    pub context: Option<Py<PyAny>>,
}

pub struct CallbackQueue {
    queue: Mutex<VecDeque<Callback>>,
    poller: Arc<LoopPoller>, // Needed to wake up the loop
}

impl CallbackQueue {
    pub fn new(poller: Arc<LoopPoller>) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            poller,
        }
    }

    pub fn push(&self, callback: Callback) {
        let mut q = self.queue.lock();
        q.push_back(callback);
        //  always notify for now to ensure loop wakes up
        // Poller::notify is cheap on Linux (eventfd write), verifying calling it is safe.
        // Let's ensure LoopPoller exposes notify.
        let _ = self.poller.notify();
    }

    pub fn pop_all(&self) -> VecDeque<Callback> {
        let mut q = self.queue.lock();
        std::mem::take(&mut *q)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

/// Callback for async TCP connection establishment
#[pyclass(module = "veloxloop._veloxloop")]
pub struct AsyncConnectCallback {
    loop_: Py<VeloxLoop>,
    future: Py<PendingFuture>,
    protocol_factory: Py<PyAny>,
    stream: Option<std::net::TcpStream>,
    fd: RawFd,
    ssl_context: Option<Py<SSLContext>>,
    server_hostname: Option<String>,
}

#[pymethods]
impl AsyncConnectCallback {
    fn __call__(&mut self, py: Python<'_>) -> PyResult<()> {
        let fd = self.fd;

        // Unregister writer (ourselves) using VeloxLoop directly
        let loop_ref = self.loop_.bind(py);
        loop_ref.borrow().remove_writer(py, fd)?;

        // Take stream
        if let Some(stream) = self.stream.take() {
            // Check error
            let res = stream.take_error();
            match res {
                Ok(None) => {
                    // Connected! Create protocol
                    let protocol_res = self.protocol_factory.call0(py);
                    match protocol_res {
                        Ok(protocol) => {
                            // Use the transport factory to create transports
                            let factory = DefaultTransportFactory;
                            let loop_py = self.loop_.clone_ref(py).into_any();

                            let transport_result: PyResult<(Py<PyAny>, Py<PyAny>)> =
                                if let Some(ssl_ctx) = &self.ssl_context {
                                    // Create SSL transport using factory
                                    let transport_py = factory.create_ssl(
                                        py,
                                        loop_py,
                                        stream,
                                        protocol.clone_ref(py),
                                        ssl_ctx.clone_ref(py).into_any(),
                                        self.server_hostname.clone(),
                                        true, // is_client
                                    )?;

                                    // Add reader for SSL handshake and data
                                    let read_ready = transport_py.getattr(py, "_read_ready")?;
                                    loop_ref.borrow().add_reader(py, fd, read_ready)?;

                                    // Add writer for SSL handshake
                                    let write_ready = transport_py.getattr(py, "_write_ready")?;
                                    loop_ref.borrow().add_writer(py, fd, write_ready)?;

                                    Ok((transport_py, protocol.clone_ref(py)))
                                } else {
                                    // Create regular TCP transport using factory
                                    let transport_py = factory.create_tcp(
                                        py,
                                        loop_py,
                                        stream,
                                        protocol.clone_ref(py),
                                    )?;

                                    // connection_made
                                    protocol.call_method1(
                                        py,
                                        "connection_made",
                                        (transport_py.clone_ref(py),),
                                    )?;

                                    // Add reader
                                    let read_ready = transport_py.getattr(py, "_read_ready")?;
                                    loop_ref.borrow().add_reader(py, fd, read_ready)?;

                                    Ok((transport_py, protocol.clone_ref(py)))
                                };

                            match transport_result {
                                Ok((transport_py, protocol)) => {
                                    // Set result: (transport, protocol)
                                    let res =
                                        PyTuple::new(py, &[transport_py, protocol])?.into_any();
                                    self.future.bind(py).borrow().set_result(py, res.unbind())?;
                                }
                                Err(e) => {
                                    let exc_val = e.value(py).as_any().clone().unbind();
                                    self.future.bind(py).borrow().set_exception(py, exc_val)?;
                                }
                            }
                        }
                        Err(e) => {
                            let exc_val = e.value(py).as_any().clone().unbind();
                            self.future.bind(py).borrow().set_exception(py, exc_val)?;
                        }
                    }
                }
                Ok(Some(e)) | Err(e) => {
                    // Error connecting
                    let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string());
                    let exc_val = py_err.value(py).as_any().clone().unbind();
                    self.future.bind(py).borrow().set_exception(py, exc_val)?;
                }
            }
        }
        Ok(())
    }
}

impl AsyncConnectCallback {
    pub fn new(
        loop_: Py<VeloxLoop>,
        future: Py<PendingFuture>,
        protocol_factory: Py<PyAny>,
        stream: std::net::TcpStream,
    ) -> Self {
        let fd = stream.as_raw_fd();
        Self {
            loop_,
            future,
            protocol_factory,
            stream: Some(stream),
            fd,
            ssl_context: None,
            server_hostname: None,
        }
    }

    pub fn new_with_ssl(
        loop_: Py<VeloxLoop>,
        future: Py<PendingFuture>,
        protocol_factory: Py<PyAny>,
        stream: std::net::TcpStream,
        ssl_context: Option<Py<SSLContext>>,
        server_hostname: Option<String>,
    ) -> Self {
        let fd = stream.as_raw_fd();
        Self {
            loop_,
            future,
            protocol_factory,
            stream: Some(stream),
            fd,
            ssl_context,
            server_hostname,
        }
    }
}

/// Callback for sock_accept
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SockAcceptCallback {
    future: Py<PendingFuture>,
    loop_: Py<VeloxLoop>,
    fd: RawFd,
}

#[pymethods]
impl SockAcceptCallback {
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        // Try to accept when readable
        unsafe {
            let mut addr: libc::sockaddr_storage = std::mem::zeroed();
            let mut addr_len: libc::socklen_t =
                std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

            let client_fd = libc::accept(
                self.fd,
                &mut addr as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            );

            if client_fd >= 0 {
                // Set non-blocking on client socket
                let flags = libc::fcntl(client_fd, libc::F_GETFL, 0);
                if flags >= 0 {
                    libc::fcntl(client_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }

                // Create Python socket object using socket.fromfd()
                let socket_module = py.import("socket")?;
                let py_socket = socket_module.call_method1("fromfd", (client_fd, 2, 1))?; // AF_INET=2, SOCK_STREAM=1

                // Parse address
                let addr_tuple = if addr_len as usize >= std::mem::size_of::<libc::sockaddr_in>() {
                    let addr_in = &*((&addr) as *const _ as *const libc::sockaddr_in);
                    if addr_in.sin_family == libc::AF_INET as u16 {
                        let ip = u32::from_be(addr_in.sin_addr.s_addr);
                        let port = u16::from_be(addr_in.sin_port);
                        let ip_str = format!(
                            "{}.{}.{}.{}",
                            (ip >> 24) & 0xff,
                            (ip >> 16) & 0xff,
                            (ip >> 8) & 0xff,
                            ip & 0xff
                        );
                        let ip_py = PyString::new(py, &ip_str);
                        let port_py = PyInt::new(py, port);
                        PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?
                    } else {
                        let ip_py = PyString::new(py, "");
                        let port_py = PyInt::new(py, 0);
                        PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?
                    }
                } else {
                    let ip_py = PyString::new(py, "");
                    let port_py = PyInt::new(py, 0);
                    PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?
                };

                // Return tuple (socket, address)
                let result = PyTuple::new(py, vec![py_socket.as_any(), addr_tuple.as_any()])?;

                self.future
                    .bind(py)
                    .borrow()
                    .set_result(py, result.into())?;
                self.loop_.bind(py).borrow().remove_reader(py, self.fd)?;
            } else {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock
                    && err.raw_os_error() != Some(libc::EAGAIN)
                {
                    let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(err.to_string());
                    let exc_val = py_err.value(py).as_any().clone().unbind();
                    self.future.bind(py).borrow().set_exception(py, exc_val)?;
                    self.loop_.bind(py).borrow().remove_reader(py, self.fd)?;
                }
            }
        }
        Ok(())
    }
}

impl SockAcceptCallback {
    pub fn new(loop_: Py<VeloxLoop>, future: Py<PendingFuture>, fd: RawFd) -> Self {
        Self { future, loop_, fd }
    }
}

/// Callback for sock_recv
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SockRecvCallback {
    future: Py<PendingFuture>,
    loop_: Py<VeloxLoop>,
    fd: RawFd,
    nbytes: usize,
}

#[pymethods]
impl SockRecvCallback {
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        let mut buf = vec![0u8; self.nbytes];
        unsafe {
            let n = libc::recv(
                self.fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                self.nbytes,
                0,
            );

            if n >= 0 {
                buf.truncate(n as usize);
                let bytes = PyBytes::new(py, &buf);
                self.future.bind(py).borrow().set_result(py, bytes.into())?;
                self.loop_.bind(py).borrow().remove_reader(py, self.fd)?;
            } else {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock
                    && err.raw_os_error() != Some(libc::EAGAIN)
                {
                    let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(err.to_string());
                    let exc_val = py_err.value(py).as_any().clone().unbind();
                    self.future.bind(py).borrow().set_exception(py, exc_val)?;
                    self.loop_.bind(py).borrow().remove_reader(py, self.fd)?;
                }
            }
        }
        Ok(())
    }
}

impl SockRecvCallback {
    pub fn new(loop_: Py<VeloxLoop>, future: Py<PendingFuture>, fd: RawFd, nbytes: usize) -> Self {
        Self {
            future,
            loop_,
            fd,
            nbytes,
        }
    }
}

/// Callback for sock_sendall
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SockSendallCallback {
    future: Py<PendingFuture>,
    loop_: Py<VeloxLoop>,
    fd: RawFd,
    data: Vec<u8>,
    sent: usize,
}

#[pymethods]
impl SockSendallCallback {
    fn __call__(&mut self, py: Python<'_>) -> PyResult<()> {
        while self.sent < self.data.len() {
            unsafe {
                let n = libc::send(
                    self.fd,
                    self.data[self.sent..].as_ptr() as *const libc::c_void,
                    self.data.len() - self.sent,
                    0,
                );

                if n > 0 {
                    self.sent += n as usize;
                } else {
                    let err = std::io::Error::last_os_error();
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock => return Ok(()),
                        _ if err.raw_os_error() == Some(libc::EAGAIN) => return Ok(()),
                        _ => {
                            let py_err =
                                PyErr::new::<pyo3::exceptions::PyOSError, _>(err.to_string());
                            let exc_val = py_err.value(py).as_any().clone().unbind();
                            self.future.bind(py).borrow().set_exception(py, exc_val)?;
                            self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
                            return Ok(());
                        }
                    }
                }
            }
        }

        // All sent
        self.future.bind(py).borrow().set_result(py, py.None())?;
        self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
        Ok(())
    }
}

impl SockSendallCallback {
    pub fn new(
        loop_: Py<VeloxLoop>,
        future: Py<PendingFuture>,
        fd: RawFd,
        data: Vec<u8>,
        sent: usize,
    ) -> Self {
        Self {
            future,
            loop_,
            fd,
            data,
            sent,
        }
    }
}

#[pyclass]
pub struct SockConnectCallback {
    future: Py<PendingFuture>,
}

#[pymethods]
impl SockConnectCallback {
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        self.future
            .bind(py)
            .call_method1("set_result", (py.None(),))?;
        Ok(())
    }
}

impl SockConnectCallback {
    pub fn new(future: Py<PendingFuture>) -> Self {
        Self { future }
    }
}

#[pyclass]
pub struct RemoveWriterCallback {
    fd: RawFd,
    loop_: Py<VeloxLoop>,
}

impl RemoveWriterCallback {
    pub fn new(fd: RawFd, loop_: Py<VeloxLoop>) -> Self {
        Self { fd, loop_ }
    }
}

#[pymethods]
impl RemoveWriterCallback {
    fn __call__(&self, py: Python<'_>, _fut: Py<PyAny>) -> PyResult<()> {
        // Remove writer through the event loop's remove_writer method
        self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
        Ok(())
    }
}
