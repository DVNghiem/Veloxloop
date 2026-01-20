use crate::constants::{NI_MAXHOST, NI_MAXSERV};
use crate::event_loop::VeloxLoop;
use crate::executor::ThreadPoolExecutor;
use std::ffi::{CStr, CString};
use std::mem;
use std::net::{IpAddr, SocketAddr};
use std::ptr;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt, PyList, PyString, PyTuple};

impl VeloxLoop {
    pub fn run_in_executor(
        &self,
        py: Python<'_>,
        _executor: Option<Py<PyAny>>,
        func: Py<PyAny>,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        let func_clone = func.clone_ref(py);
        let args_clone: Py<PyTuple> = args.clone().unbind();

        // Use spawn for fire-and-forget task execution
        executor_ref.spawn(move || {
            let _ = Python::attach(move |py| {
                let result = func_clone.call1(py, args_clone.bind(py));

                match result {
                    Ok(val) => {
                        let _ = future_clone.bind(py).borrow().set_result(py, val);
                    }
                    Err(e) => {
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        let _ = future_clone.bind(py).borrow().set_exception(py, exc);
                    }
                }
            });
        });

        Ok(future.into_any())
    }

    /// Run a blocking function synchronously in the executor and wait for result
    /// This uses TaskHandle::join() to wait for completion
    pub fn run_in_executor_sync<F, R>(&self, func: F) -> PyResult<Option<R>>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        // Use spawn_blocking which returns TaskHandle
        let handle = executor_ref.spawn_blocking(func);

        // Use TaskHandle::join() to wait for result
        Ok(handle.join())
    }

    pub fn set_default_executor(&self, _executor: Option<Py<PyAny>>) -> PyResult<()> {
        *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        Ok(())
    }

    pub fn getaddrinfo(
        &self,
        py: Python<'_>,
        host: Option<Bound<'_, PyAny>>,
        port: Option<Bound<'_, PyAny>>,
        family: i32,
        r#type: i32,
        proto: i32,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        let host_str = match host {
            Some(h) => {
                if let Ok(s) = h.cast::<PyString>() {
                    Some(s.to_string())
                } else if let Ok(b) = h.cast::<PyBytes>() {
                    Some(String::from_utf8_lossy(b.as_bytes()).to_string())
                } else {
                    return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        "host must be str, bytes, or None",
                    ));
                }
            }
            None => None,
        };

        let port_str = match port {
            Some(p) => {
                if let Ok(s) = p.cast::<PyString>() {
                    Some(s.to_string())
                } else if let Ok(b) = p.cast::<PyBytes>() {
                    Some(String::from_utf8_lossy(b.as_bytes()).to_string())
                } else if let Ok(i) = p.extract::<i32>() {
                    Some(i.to_string())
                } else {
                    return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        "port must be str, bytes, int, or None",
                    ));
                }
            }
            None => None,
        };

        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                let result =
                    perform_getaddrinfo(py, host_str, port_str, family, r#type, proto, flags);

                match result {
                    Ok(val) => {
                        let _ = future_clone.bind(py).borrow().set_result(py, val);
                    }
                    Err(e) => {
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        let _ = future_clone.bind(py).borrow().set_exception(py, exc);
                    }
                }
            });
        });

        Ok(future.into_any())
    }

    pub fn getnameinfo(
        &self,
        py: Python<'_>,
        sockaddr: Bound<'_, PyTuple>,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        let addr_str: String = sockaddr.get_item(0)?.extract()?;
        let port: u16 = sockaddr.get_item(1)?.extract()?;

        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                let result = perform_getnameinfo(py, &addr_str, port, flags);

                match result {
                    Ok(val) => {
                        let _ = future_clone.bind(py).borrow().set_result(py, val);
                    }
                    Err(e) => {
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        let _ = future_clone.bind(py).borrow().set_exception(py, exc);
                    }
                }
            });
        });

        Ok(future.into_any())
    }
}

#[cfg(unix)]
fn perform_getaddrinfo(
    py: Python<'_>,
    host: Option<String>,
    port: Option<String>,
    family: i32,
    socktype: i32,
    protocol: i32,
    flags: i32,
) -> PyResult<Py<PyAny>> {
    unsafe {
        let mut hints: libc::addrinfo = mem::zeroed();
        hints.ai_family = family;
        hints.ai_socktype = socktype;
        hints.ai_protocol = protocol;
        hints.ai_flags = flags;

        let c_host = host
            .as_ref()
            .map(|h| CString::new(h.as_str()).ok())
            .flatten();
        let c_port = port
            .as_ref()
            .map(|p| CString::new(p.as_str()).ok())
            .flatten();

        let host_ptr = c_host.as_ref().map_or(ptr::null(), |s| s.as_ptr());
        let port_ptr = c_port.as_ref().map_or(ptr::null(), |s| s.as_ptr());

        let mut res: *mut libc::addrinfo = ptr::null_mut();
        let ret = libc::getaddrinfo(host_ptr, port_ptr, &hints, &mut res);

        if ret != 0 {
            let error_msg = if ret == libc::EAI_SYSTEM {
                format!("getaddrinfo failed: {}", std::io::Error::last_os_error())
            } else {
                let err_str = libc::gai_strerror(ret);
                let c_str = CStr::from_ptr(err_str);
                format!("getaddrinfo failed: {}", c_str.to_string_lossy())
            };
            return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(error_msg));
        }

        let py_list = PyList::empty(py);
        let mut current = res;

        while !current.is_null() {
            let info = &*current;

            let fam = info.ai_family;
            let stype = info.ai_socktype;
            let proto = info.ai_protocol;

            let canonname = if info.ai_canonname.is_null() {
                String::new()
            } else {
                CStr::from_ptr(info.ai_canonname)
                    .to_string_lossy()
                    .to_string()
            };

            if info.ai_family == libc::AF_INET {
                let addr = &*(info.ai_addr as *const libc::sockaddr_in);
                let ip_bytes = addr.sin_addr.s_addr.to_ne_bytes();
                let ip = format!(
                    "{}.{}.{}.{}",
                    ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]
                );
                let port = u16::from_be(addr.sin_port);

                let fam_py = PyInt::new(py, fam);
                let stype_py = PyInt::new(py, stype);
                let proto_py = PyInt::new(py, proto);
                let canon_py = PyString::new(py, &canonname);
                let ip_py = PyString::new(py, &ip);
                let port_py = PyInt::new(py, port);
                let addr_tuple = PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?;

                let tuple = PyTuple::new(
                    py,
                    vec![
                        fam_py.as_any(),
                        stype_py.as_any(),
                        proto_py.as_any(),
                        canon_py.as_any(),
                        addr_tuple.as_any(),
                    ],
                )?;
                py_list.append(tuple)?;
            } else if info.ai_family == libc::AF_INET6 {
                let addr = &*(info.ai_addr as *const libc::sockaddr_in6);
                let ip_bytes = addr.sin6_addr.s6_addr;
                let ip = format!(
                    "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
                    u16::from_be_bytes([ip_bytes[0], ip_bytes[1]]),
                    u16::from_be_bytes([ip_bytes[2], ip_bytes[3]]),
                    u16::from_be_bytes([ip_bytes[4], ip_bytes[5]]),
                    u16::from_be_bytes([ip_bytes[6], ip_bytes[7]]),
                    u16::from_be_bytes([ip_bytes[8], ip_bytes[9]]),
                    u16::from_be_bytes([ip_bytes[10], ip_bytes[11]]),
                    u16::from_be_bytes([ip_bytes[12], ip_bytes[13]]),
                    u16::from_be_bytes([ip_bytes[14], ip_bytes[15]]),
                );
                let port = u16::from_be(addr.sin6_port);
                let flowinfo = addr.sin6_flowinfo;
                let scope_id = addr.sin6_scope_id;

                let fam_py = PyInt::new(py, fam);
                let stype_py = PyInt::new(py, stype);
                let proto_py = PyInt::new(py, proto);
                let canon_py = PyString::new(py, &canonname);
                let ip_py = PyString::new(py, &ip);
                let port_py = PyInt::new(py, port);
                let flowinfo_py = PyInt::new(py, flowinfo);
                let scope_py = PyInt::new(py, scope_id);
                let addr_tuple = PyTuple::new(
                    py,
                    vec![
                        ip_py.as_any(),
                        port_py.as_any(),
                        flowinfo_py.as_any(),
                        scope_py.as_any(),
                    ],
                )?;

                let tuple = PyTuple::new(
                    py,
                    vec![
                        fam_py.as_any(),
                        stype_py.as_any(),
                        proto_py.as_any(),
                        canon_py.as_any(),
                        addr_tuple.as_any(),
                    ],
                )?;
                py_list.append(tuple)?;
            }

            current = info.ai_next;
        }

        libc::freeaddrinfo(res);

        Ok(py_list.into())
    }
}

#[cfg(windows)]
fn perform_getaddrinfo(
    py: Python<'_>,
    host: Option<String>,
    port: Option<String>,
    _family: i32,
    _socktype: i32,
    _protocol: i32,
    _flags: i32,
) -> PyResult<Py<PyAny>> {
    // Use std::net for cross-platform DNS resolution
    let host_str = host.as_deref().unwrap_or("localhost");
    let port_str = port.as_deref().unwrap_or("0");
    let addr_str = format!("{}:{}", host_str, port_str);

    use std::net::ToSocketAddrs;
    let addrs: Vec<_> = addr_str
        .to_socket_addrs()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("getaddrinfo failed: {}", e))
        })?
        .collect();

    let py_list = PyList::empty(py);
    for addr in addrs {
        let (ip_str, port) = match addr {
            SocketAddr::V4(v4) => (v4.ip().to_string(), v4.port()),
            SocketAddr::V6(v6) => (v6.ip().to_string(), v6.port()),
        };

        let family = if addr.is_ipv4() { 2 } else { 23 }; // AF_INET=2, AF_INET6=23
        let fam_py = PyInt::new(py, family);
        let stype_py = PyInt::new(py, 1); // SOCK_STREAM
        let proto_py = PyInt::new(py, 6); // IPPROTO_TCP
        let canon_py = PyString::new(py, "");
        let ip_py = PyString::new(py, &ip_str);
        let port_py = PyInt::new(py, port);
        let addr_tuple = PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?;

        let tuple = PyTuple::new(
            py,
            vec![
                fam_py.as_any(),
                stype_py.as_any(),
                proto_py.as_any(),
                canon_py.as_any(),
                addr_tuple.as_any(),
            ],
        )?;
        py_list.append(tuple)?;
    }

    Ok(py_list.into())
}

#[cfg(unix)]
fn perform_getnameinfo(py: Python<'_>, addr: &str, port: u16, flags: i32) -> PyResult<Py<PyAny>> {
    let ip_addr: IpAddr = addr.parse().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid IP address: {}", e))
    })?;

    let sock_addr = SocketAddr::new(ip_addr, port);

    unsafe {
        let mut host = vec![0u8; NI_MAXHOST];
        let mut serv = vec![0u8; NI_MAXSERV];

        let ret = match sock_addr {
            SocketAddr::V4(v4_addr) => {
                let mut sa: libc::sockaddr_in = mem::zeroed();
                sa.sin_family = libc::AF_INET as _;
                sa.sin_port = v4_addr.port().to_be();
                sa.sin_addr.s_addr = u32::from_ne_bytes(v4_addr.ip().octets());

                libc::getnameinfo(
                    &sa as *const libc::sockaddr_in as *const libc::sockaddr,
                    mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                    host.as_mut_ptr() as *mut libc::c_char,
                    host.len() as libc::socklen_t,
                    serv.as_mut_ptr() as *mut libc::c_char,
                    serv.len() as libc::socklen_t,
                    flags,
                )
            }
            SocketAddr::V6(v6_addr) => {
                let mut sa: libc::sockaddr_in6 = mem::zeroed();
                sa.sin6_family = libc::AF_INET6 as _;
                sa.sin6_port = v6_addr.port().to_be();
                sa.sin6_addr.s6_addr = v6_addr.ip().octets();
                sa.sin6_flowinfo = v6_addr.flowinfo();
                sa.sin6_scope_id = v6_addr.scope_id();

                libc::getnameinfo(
                    &sa as *const libc::sockaddr_in6 as *const libc::sockaddr,
                    mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
                    host.as_mut_ptr() as *mut libc::c_char,
                    host.len() as libc::socklen_t,
                    serv.as_mut_ptr() as *mut libc::c_char,
                    serv.len() as libc::socklen_t,
                    flags,
                )
            }
        };

        if ret != 0 {
            let error_msg = if ret == libc::EAI_SYSTEM {
                format!("getnameinfo failed: {}", std::io::Error::last_os_error())
            } else {
                let err_str = libc::gai_strerror(ret);
                let c_str = CStr::from_ptr(err_str);
                format!("getnameinfo failed: {}", c_str.to_string_lossy())
            };
            return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(error_msg));
        }

        let hostname = CStr::from_ptr(host.as_ptr() as *const libc::c_char)
            .to_string_lossy()
            .to_string();
        let servname = CStr::from_ptr(serv.as_ptr() as *const libc::c_char)
            .to_string_lossy()
            .to_string();

        let host_py = PyString::new(py, &hostname);
        let serv_py = PyString::new(py, &servname);
        let result_tuple = PyTuple::new(py, vec![host_py.as_any(), serv_py.as_any()])?;

        Ok(result_tuple.into())
    }
}

#[cfg(windows)]
fn perform_getnameinfo(py: Python<'_>, addr: &str, port: u16, _flags: i32) -> PyResult<Py<PyAny>> {
    // Simple implementation using std::net for Windows
    // In a production system, you might want to use Windows-specific APIs
    let host_py = PyString::new(py, addr);
    let serv_py = PyString::new(py, &port.to_string());
    let result_tuple = PyTuple::new(py, vec![host_py.as_any(), serv_py.as_any()])?;
    Ok(result_tuple.into())
}
