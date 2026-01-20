use crate::callbacks::{
    AsyncConnectCallback, RemoveWriterCallback, SendfileCallback, SockAcceptCallback,
    SockConnectCallback,
};
use crate::constants::STACK_BUF_SIZE;
use crate::event_loop::VeloxLoop;
use crate::transports::future::{CompletedFuture, PendingFuture};
use crate::transports::tcp::TcpServer;
use crate::transports::udp::UdpTransport;
use crate::transports::{DefaultTransportFactory, TransportFactory};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyInt, PyString, PyTuple};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use pyo3::IntoPyObjectExt;

impl VeloxLoop {
    pub fn sock_connect(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        address: Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        let tuple: Bound<'_, PyTuple> = address.extract().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>("address must be a tuple (host, port)")
        })?;

        let host: String = tuple.get_item(0)?.extract()?;
        let port: u16 = tuple.get_item(1)?.extract()?;

        let ip_addr: std::net::IpAddr = host.parse().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid IP address: {}", host))
        })?;

        let addr = SocketAddr::new(ip_addr, port);

        let sock_addr: SockAddr = addr.into();

        unsafe {
            let ret = libc::connect(
                fd,
                sock_addr.as_ptr() as *const libc::sockaddr,
                sock_addr.len(),
            );

            if ret == 0 {
                let fut = PendingFuture::new();
                fut.set_result(py, py.None())?;
                return Ok(Py::new(py, fut)?.into_any());
            }

            let err = std::io::Error::last_os_error();
            match err.kind() {
                std::io::ErrorKind::WouldBlock => {}
                _ if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
                _ => {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        let future = self_.create_future(py)?;
        let future_clone = future.clone_ref(py);

        let callback = SockConnectCallback::new(future_clone).into_py_any(py)?;

        self_.add_writer(py, fd, callback)?;

        let loop_ref = slf.clone().unbind();
        let done_callback_obj = RemoveWriterCallback::new(fd, loop_ref).into_py_any(py)?;
        future
            .bind(py)
            .borrow()
            .add_done_callback(done_callback_obj)?;

        Ok(future.into_any())
    }

    pub fn sock_accept(slf: &Bound<'_, Self>, sock: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        unsafe {
            let mut addr: libc::sockaddr_storage = std::mem::zeroed();
            let mut addr_len: libc::socklen_t =
                std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

            let client_fd = libc::accept(
                fd,
                &mut addr as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            );

            if client_fd >= 0 {
                let socket_module = py.import("socket")?;
                let client_sock = socket_module.call_method1("fromfd", (client_fd, 2, 1))?;

                let flags = libc::fcntl(client_fd, libc::F_GETFL, 0);
                if flags >= 0 {
                    libc::fcntl(client_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }

                let addr_tuple = if addr_len as usize >= std::mem::size_of::<libc::sockaddr_in>() {
                    let addr_in = &*((&addr) as *const _ as *const libc::sockaddr_in);
                    #[cfg(any(
                        target_os = "macos",
                        target_os = "ios",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "netbsd"
                    ))]
                    let is_ipv4 = addr_in.sin_family == libc::AF_INET as u8;
                    #[cfg(not(any(
                        target_os = "macos",
                        target_os = "ios",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "netbsd"
                    )))]
                    let is_ipv4 = addr_in.sin_family == libc::AF_INET as u16;

                    if is_ipv4 {
                        let ip = u32::from_be(addr_in.sin_addr.s_addr);
                        let ip_str = format!(
                            "{}.{}.{}.{}",
                            (ip >> 24) & 0xff,
                            (ip >> 16) & 0xff,
                            (ip >> 8) & 0xff,
                            ip & 0xff
                        );
                        let ip_py = PyString::new(py, &ip_str);
                        let port_py = PyInt::new(py, u16::from_be(addr_in.sin_port));
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

                let result = PyTuple::new(py, vec![client_sock.as_any(), addr_tuple.as_any()])?;

                let fut = PendingFuture::new();
                fut.set_result(py, result.into())?;
                return Ok(Py::new(py, fut)?.into_any());
            }

            let err = std::io::Error::last_os_error();
            match err.kind() {
                std::io::ErrorKind::WouldBlock => {}
                _ if err.raw_os_error() == Some(libc::EAGAIN) => {}
                _ => {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();

        let callback =
            SockAcceptCallback::new(loop_ref, future.clone_ref(py), fd).into_py_any(py)?;
        self_.add_reader(py, fd, callback)?;

        Ok(future.into_any())
    }

    #[inline(always)]
    pub fn sock_recv(slf: &Bound<'_, Self>, sock: Py<PyAny>, nbytes: usize) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        if fd < 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                "Invalid file descriptor",
            ));
        }

        if nbytes <= STACK_BUF_SIZE {
            let mut buf = [0u8; STACK_BUF_SIZE];
            unsafe {
                let n = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, nbytes, 0);

                if n > 0 {
                    let bytes = PyBytes::new(py, &buf[..n as usize]);
                    let fut = CompletedFuture::new(bytes.into_any().unbind());
                    return Ok(Py::new(py, fut)?.into_any());
                } else if n == 0 {
                    let bytes = PyBytes::new(py, &[]);
                    let fut = CompletedFuture::new(bytes.into_any().unbind());
                    return Ok(Py::new(py, fut)?.into_any());
                }

                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock
                    && err.raw_os_error() != Some(libc::EAGAIN)
                {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        } else {
            let mut buf = vec![0u8; nbytes];
            unsafe {
                let n = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, nbytes, 0);

                if n > 0 {
                    buf.truncate(n as usize);
                    let bytes = PyBytes::new(py, &buf);
                    let fut = CompletedFuture::new(bytes.into_any().unbind());
                    return Ok(Py::new(py, fut)?.into_any());
                } else if n == 0 {
                    let bytes = PyBytes::new(py, &[]);
                    let fut = CompletedFuture::new(bytes.into_any().unbind());
                    return Ok(Py::new(py, fut)?.into_any());
                }

                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock
                    && err.raw_os_error() != Some(libc::EAGAIN)
                {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();
        let future_clone = future.clone_ref(py);

        #[cfg(target_os = "linux")]
        {
            let native_callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync> =
                Arc::new(move |py: Python<'_>| {
                    loop_ref.bind(py).borrow().mark_oneshot_disabled(fd);

                    let mut buf = [0u8; 65536];
                    let read_size = nbytes.min(65536);

                    let n = unsafe {
                        libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, read_size, 0)
                    };

                    if n > 0 {
                        let bytes = pyo3::types::PyBytes::new(py, &buf[..n as usize]);
                        let _ = future_clone.bind(py).borrow().set_result(py, bytes.into());
                    } else if n == 0 {
                        let bytes = pyo3::types::PyBytes::new(py, &[]);
                        let _ = future_clone.bind(py).borrow().set_result(py, bytes.into());
                    } else {
                        let err = std::io::Error::last_os_error();
                        if err.kind() != std::io::ErrorKind::WouldBlock
                            && err.raw_os_error() != Some(libc::EAGAIN)
                            && err.raw_os_error() != Some(libc::EBADF)
                        {
                            let py_err =
                                PyErr::new::<pyo3::exceptions::PyOSError, _>(err.to_string());
                            let exc_val = py_err.value(py).as_any().clone().unbind();
                            let _ = future_clone.bind(py).borrow().set_exception(py, exc_val);
                        } else if err.raw_os_error() == Some(libc::EBADF) {
                            let bytes = pyo3::types::PyBytes::new(py, &[]);
                            let _ = future_clone.bind(py).borrow().set_result(py, bytes.into());
                        }
                    }
                    Ok(())
                });

            self_.add_reader_oneshot(fd, native_callback)?;
        }

        #[cfg(not(target_os = "linux"))]
        {
            let handled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let handled_clone = handled.clone();

            let native_callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync> =
                Arc::new(move |py: Python<'_>| {
                    if handled_clone.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        return Ok(());
                    }

                    let mut buf = [0u8; 65536];
                    let read_size = nbytes.min(65536);

                    let n = unsafe {
                        libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, read_size, 0)
                    };

                    let _ = loop_ref.bind(py).borrow().remove_reader(py, fd);

                    if n > 0 {
                        let bytes = pyo3::types::PyBytes::new(py, &buf[..n as usize]);
                        let _ = future_clone.bind(py).borrow().set_result(py, bytes.into());
                    } else if n == 0 {
                        let bytes = pyo3::types::PyBytes::new(py, &[]);
                        let _ = future_clone.bind(py).borrow().set_result(py, bytes.into());
                    } else {
                        let err = std::io::Error::last_os_error();
                        if err.kind() != std::io::ErrorKind::WouldBlock
                            && err.raw_os_error() != Some(libc::EAGAIN)
                        {
                            let py_err =
                                PyErr::new::<pyo3::exceptions::PyOSError, _>(err.to_string());
                            let exc_val = py_err.value(py).as_any().clone().unbind();
                            let _ = future_clone.bind(py).borrow().set_exception(py, exc_val);
                        }
                    }
                    Ok(())
                });

            self_.add_reader_native(fd, native_callback)?;
        }

        Ok(future.into_any())
    }

    pub fn sendfile(
        slf: &Bound<'_, Self>,
        transport: Py<PyAny>,
        file: Py<PyAny>,
        offset: i64,
        count: Option<usize>,
        _fallback: bool,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let out_fd: RawFd = if let Ok(fd) = transport
            .getattr(py, "fileno")?
            .call0(py)?
            .extract::<RawFd>(py)
        {
            fd
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "transport must have a fileno() method",
            ));
        };

        let in_fd: RawFd =
            if let Ok(fd) = file.getattr(py, "fileno")?.call0(py)?.extract::<RawFd>(py) {
                fd
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "file must have a fileno() method",
                ));
            };

        let total_count = match count {
            Some(c) => c,
            None => unsafe {
                let mut stat: libc::stat = std::mem::zeroed();
                if libc::fstat(in_fd, &mut stat) == 0 {
                    (stat.st_size as i64 - offset).max(0) as usize
                } else {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        "failed to get file size",
                    ));
                }
            },
        };

        if total_count == 0 {
            let fut = PendingFuture::new();
            fut.set_result(py, py.None())?;
            return Ok(Py::new(py, fut)?.into_any());
        }

        let mut current_sent = 0;
        unsafe {
            let mut off = offset as libc::off_t;

            #[cfg(target_os = "linux")]
            let n = libc::sendfile(out_fd, in_fd, &mut off, total_count);

            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
            let n = {
                let mut len = total_count as libc::off_t;
                let result = libc::sendfile(in_fd, out_fd, off, &mut len, std::ptr::null_mut(), 0);
                if result == 0 { len as isize } else { -1 }
            };

            #[cfg(not(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "ios",
                target_os = "freebsd"
            )))]
            let n = {
                // Fallback for platforms without sendfile (e.g., Windows)
                let mut buf = [0u8; 8192];
                let to_read = total_count.min(8192);
                let read_result = libc::read(in_fd, buf.as_mut_ptr() as *mut libc::c_void, to_read);
                if read_result > 0 {
                    libc::write(
                        out_fd,
                        buf.as_ptr() as *const libc::c_void,
                        read_result as usize,
                    )
                } else {
                    read_result
                }
            };
            if n > 0 {
                current_sent = n as usize;
                if current_sent >= total_count {
                    let fut = PendingFuture::new();
                    fut.set_result(py, py.None())?;
                    return Ok(Py::new(py, fut)?.into_any());
                }
            } else if n == 0 {
                let fut = PendingFuture::new();
                fut.set_result(py, py.None())?;
                return Ok(Py::new(py, fut)?.into_any());
            } else {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock
                    && err.raw_os_error() != Some(libc::EAGAIN)
                {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();

        let callback = SendfileCallback::new(
            loop_ref,
            future.clone_ref(py),
            out_fd,
            in_fd,
            Some(offset),
            total_count,
            current_sent,
        );

        let callback_py = Py::new(py, callback)?;
        self_.add_writer(py, out_fd, callback_py.into_any())?;

        Ok(future.into_any())
    }

    pub fn sock_sendall(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        data: &[u8],
    ) -> PyResult<Py<PyAny>> {
        use std::sync::Mutex;

        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;
        let data_vec = data.to_vec();

        let mut total_sent = 0;
        while total_sent < data_vec.len() {
            unsafe {
                let n = libc::send(
                    fd,
                    data_vec[total_sent..].as_ptr() as *const libc::c_void,
                    data_vec.len() - total_sent,
                    0,
                );

                if n > 0 {
                    total_sent += n as usize;
                } else {
                    let err = std::io::Error::last_os_error();
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock => break,
                        _ if err.raw_os_error() == Some(libc::EAGAIN) => break,
                        _ => {
                            return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                                err.to_string(),
                            ));
                        }
                    }
                }
            }
        }

        if total_sent == data_vec.len() {
            let fut = CompletedFuture::new(py.None());
            return Ok(Py::new(py, fut)?.into_any());
        }

        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();
        let remaining_data = Arc::new(Mutex::new(data_vec[total_sent..].to_vec()));
        let sent_counter = Arc::new(Mutex::new(0usize));
        let future_clone = future.clone_ref(py);

        let native_callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync> =
            Arc::new(move |py: Python<'_>| {
                let mut sent = sent_counter.lock().unwrap();
                let data = remaining_data.lock().unwrap();

                while *sent < data.len() {
                    unsafe {
                        let n = libc::send(
                            fd,
                            data[*sent..].as_ptr() as *const libc::c_void,
                            data.len() - *sent,
                            0,
                        );

                        if n > 0 {
                            *sent += n as usize;
                        } else {
                            let err = std::io::Error::last_os_error();
                            match err.kind() {
                                std::io::ErrorKind::WouldBlock => return Ok(()),
                                _ if err.raw_os_error() == Some(libc::EAGAIN) => return Ok(()),
                                _ => {
                                    let py_err = PyErr::new::<pyo3::exceptions::PyOSError, _>(
                                        err.to_string(),
                                    );
                                    let exc_val = py_err.value(py).as_any().clone().unbind();
                                    future_clone.bind(py).borrow().set_exception(py, exc_val)?;
                                    loop_ref.bind(py).borrow().remove_writer(py, fd)?;
                                    return Ok(());
                                }
                            }
                        }
                    }
                }

                future_clone.bind(py).borrow().set_result(py, py.None())?;
                loop_ref.bind(py).borrow().remove_writer(py, fd)?;
                Ok(())
            });

        self_.add_writer_native(fd, native_callback)?;

        Ok(future.into_any())
    }

    pub fn create_connection(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let ssl_context = _kwargs
            .as_ref()
            .and_then(|kw| kw.get_item("ssl").ok().flatten())
            .and_then(|v| v.extract::<Py<crate::transports::ssl::SSLContext>>().ok());

        // Check if a pre-existing socket is provided
        let sock_obj = _kwargs
            .as_ref()
            .and_then(|kw| kw.get_item("sock").ok().flatten());

        let (stream, fd) = if let Some(sock) = sock_obj {
            // Use the provided socket
            let fd = sock.call_method0("fileno")?.extract::<RawFd>()?;

            // Duplicate the file descriptor so we don't steal it from Python
            use std::os::unix::io::FromRawFd;
            let dup_fd = unsafe { libc::dup(fd) };
            if dup_fd < 0 {
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    "Failed to duplicate file descriptor",
                ));
            }
            let stream = unsafe { std::net::TcpStream::from_raw_fd(dup_fd) };

            // Set nonblocking mode
            stream
                .set_nonblocking(true)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

            (stream, dup_fd)
        } else {
            // Create a new socket as before
            let host = host.unwrap_or("127.0.0.1");
            let port = port.unwrap_or(0);
            let addr_str = format!("{}:{}", host, port);

            let mut addrs = std::net::ToSocketAddrs::to_socket_addrs(&addr_str)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

            let addr = addrs
                .next()
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyOSError, _>("No address found"))?;

            let is_ipv6 = addr.is_ipv6();
            let domain = if is_ipv6 { Domain::IPV6 } else { Domain::IPV4 };
            let socket = Socket::new(domain, Type::STREAM, None)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

            socket
                .set_nonblocking(true)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

            match socket.connect(&addr.into()) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                #[cfg(unix)]
                Err(e) if e.raw_os_error() == Some(36) || e.raw_os_error() == Some(115) => {}
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Connection failed: {}",
                        e
                    )));
                }
            }

            let stream: std::net::TcpStream = socket.into();
            let fd = stream.as_raw_fd();

            (stream, fd)
        };

        let server_hostname = _kwargs
            .as_ref()
            .and_then(|kw| kw.get_item("server_hostname").ok().flatten())
            .and_then(|v| v.extract::<String>().ok())
            .or_else(|| {
                if ssl_context.is_some() {
                    host.map(|h| h.to_string())
                } else {
                    None
                }
            });

        let fut = self_.create_future(py)?;

        let loop_obj = slf.clone().unbind();
        let callback = AsyncConnectCallback::new_with_ssl(
            loop_obj.clone_ref(py),
            fut.clone_ref(py),
            protocol_factory,
            stream,
            ssl_context,
            server_hostname,
        );
        let callback_py = Py::new(py, callback)?.into_any();

        self_.add_writer(py, fd, callback_py)?;

        Ok(fut.into_any())
    }

    pub fn create_server(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let loop_obj = slf.clone().unbind();

        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr = format!("{}:{}", host, port);

        let listener = std::net::TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        let server = TcpServer::new(
            listener,
            loop_obj.clone_ref(py),
            protocol_factory.clone_ref(py),
        );
        let server_py = Py::new(py, server)?;

        let on_accept = server_py.getattr(py, "_on_accept")?;

        let fd = server_py.borrow(py).fd().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Server has no listener")
        })?;

        self_.add_reader(py, fd, on_accept)?;

        let fut = crate::transports::future::CompletedFuture::new(server_py.into_any());

        Ok(Py::new(py, fut)?.into_any())
    }

    pub fn start_server(
        slf: &Bound<'_, Self>,
        client_connected_cb: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        limit: Option<usize>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let loop_obj = slf.clone().unbind();

        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr = format!("{}:{}", host, port);
        let limit = limit.unwrap_or(65536);

        let listener = std::net::TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        let server = crate::transports::stream_server::StreamServer::new(
            listener,
            loop_obj.clone_ref(py),
            client_connected_cb,
            limit,
        );
        let server_py = Py::new(py, server)?;

        let on_accept = server_py.getattr(py, "_on_accept")?;

        let fd = server_py.borrow(py).get_fd().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Server has no listener")
        })?;

        self_.add_reader(py, fd, on_accept)?;

        let fut = crate::transports::future::CompletedFuture::new(server_py.into_any());

        Ok(Py::new(py, fut)?.into_any())
    }

    pub fn open_connection(
        slf: &Bound<'_, Self>,
        host: &str,
        port: u16,
        limit: Option<usize>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let loop_obj = slf.clone().unbind();
        let limit = limit.unwrap_or(65536);

        let addr = format!("{}:{}", host, port);
        let stream = std::net::TcpStream::connect(&addr)?;
        stream.set_nonblocking(true)?;

        let reader = Py::new(py, crate::streams::StreamReader::new(Some(limit)))?;
        let writer = Py::new(
            py,
            crate::streams::StreamWriter::new(Some(65536), Some(16384)),
        )?;

        let transport_py = crate::transports::stream_server::StreamTransport::new(
            py,
            loop_obj.clone_ref(py),
            stream,
            reader.clone_ref(py),
            writer.clone_ref(py),
        )?;

        let transport_clone = transport_py.clone_ref(py);
        let read_callback =
            Arc::new(move |py: Python<'_>| transport_clone.bind(py).borrow_mut()._read_ready(py));
        let fd = transport_py.borrow(py).get_fd();
        slf.borrow().add_reader_native(fd, read_callback)?;

        let result = (reader.into_any(), writer.into_any());
        let result_tuple = pyo3::types::PyTuple::new(py, &[result.0, result.1])?;
        let fut = crate::transports::future::CompletedFuture::new(result_tuple.into());

        Ok(Py::new(py, fut)?.into_any())
    }

    pub fn create_datagram_endpoint(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        local_addr: Option<(String, u16)>,
        remote_addr: Option<(String, u16)>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let loop_obj = slf.clone().unbind();

        let allow_broadcast = kwargs
            .and_then(|k| k.get_item("allow_broadcast").ok().flatten())
            .and_then(|v| v.extract::<bool>().ok())
            .unwrap_or(false);

        let reuse_port = kwargs
            .and_then(|k| k.get_item("reuse_port").ok().flatten())
            .and_then(|v| v.extract::<bool>().ok())
            .unwrap_or(false);

        let is_ipv6 = if let Some((ref host, _)) = local_addr {
            crate::utils::ipv6::is_ipv6_string(host)
        } else if let Some((ref host, _)) = remote_addr {
            crate::utils::ipv6::is_ipv6_string(host)
        } else {
            false
        };

        let domain = if is_ipv6 { Domain::IPV6 } else { Domain::IPV4 };
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

        if allow_broadcast {
            socket
                .set_broadcast(true)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        #[cfg(all(unix, not(target_os = "solaris")))]
        if reuse_port {
            let fd = socket.as_raw_fd();
            unsafe {
                let optval: libc::c_int = 1;
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

        if let Some((host, port)) = local_addr {
            let addr_str = format!("{}:{}", host, port);
            let bind_addr: SocketAddr = addr_str.parse().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid local address: {}",
                    e
                ))
            })?;
            socket.bind(&bind_addr.into()).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Failed to bind: {}", e))
            })?;
        }

        let remote_sockaddr = if let Some((host, port)) = remote_addr {
            let addr_str = format!("{}:{}", host, port);
            let addr: SocketAddr = addr_str.parse().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid remote address: {}",
                    e
                ))
            })?;

            socket.connect(&addr.into()).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Failed to connect: {}", e))
            })?;
            Some(addr)
        } else {
            None
        };

        let udp_socket: std::net::UdpSocket = socket.into();

        let protocol = protocol_factory.call0(py)?;

        let factory = DefaultTransportFactory;
        let loop_py = loop_obj.clone_ref(py).into_any();

        let transport_py = factory.create_udp(
            py,
            loop_py,
            udp_socket,
            protocol.clone_ref(py),
            remote_sockaddr,
            allow_broadcast,
        )?;

        let fd = transport_py
            .getattr(py, "fileno")?
            .call0(py)?
            .extract::<i32>(py)?;

        protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;

        let transport_clone = transport_py.clone_ref(py);
        let read_callback = Arc::new(move |py: Python<'_>| {
            let b = transport_clone.bind(py);
            let udp = b.cast::<UdpTransport>().map_err(|_| {
                PyErr::new::<pyo3::exceptions::PyTypeError, _>("Expected UdpTransport")
            })?;
            udp.borrow()._read_ready(py)
        });
        slf.borrow().add_reader_native(fd, read_callback)?;

        let result_tuple = PyTuple::new(py, vec![transport_py.into_any(), protocol.into_any()])?;

        let fut = CompletedFuture::new(result_tuple.into());
        Ok(Py::new(py, fut)?.into_any())
    }
}
