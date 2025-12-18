use pyo3::prelude::*;
use socket2::Socket;
use std::net::TcpStream;

/// Socket options configuration
/// Supports common socket options like SO_KEEPALIVE, TCP_NODELAY, SO_REUSEADDR, etc.
#[derive(Debug, Clone, Default)]
pub struct InnerSocketOptions {
    pub tcp_nodelay: Option<bool>,
    pub keepalive: Option<bool>,
    pub keepalive_time: Option<u32>, // TCP_KEEP_IDLE on Unix, TCP_KEEPIDLE
    pub keepalive_interval: Option<u32>, // TCP_KEEP_INTVL on Unix, TCP_KEEPINTVL
    pub keepalive_count: Option<u32>, // TCP_KEEP_CNT on Unix, TCP_KEEPCNT
    pub so_reuseaddr: Option<bool>,
    pub so_reuseport: Option<bool>,
    pub so_rcvbuf: Option<usize>,
    pub so_sndbuf: Option<usize>,
}

impl InnerSocketOptions {
    /// Create a new InnerSocketOptions with all options unset
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply socket options to a socket2::Socket
    pub fn apply(&self, socket: &Socket) -> PyResult<()> {
        if let Some(nodelay) = self.tcp_nodelay {
            socket
                .set_tcp_nodelay(nodelay)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        if let Some(reuse_addr) = self.so_reuseaddr {
            socket
                .set_reuse_address(reuse_addr)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        if let Some(rcvbuf) = self.so_rcvbuf {
            socket
                .set_recv_buffer_size(rcvbuf)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        if let Some(sndbuf) = self.so_sndbuf {
            socket
                .set_send_buffer_size(sndbuf)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        self.apply_keepalive(socket)?;
        self.apply_reuseport(socket)?;

        Ok(())
    }

    /// Apply SO_KEEPALIVE and related TCP keep-alive options
    #[cfg(unix)]
    fn apply_keepalive(&self, socket: &Socket) -> PyResult<()> {
        use libc::{IPPROTO_TCP, SO_KEEPALIVE, SOL_SOCKET, setsockopt};
        use std::os::unix::io::AsRawFd;

        let fd = socket.as_raw_fd();

        if let Some(keepalive) = self.keepalive {
            let optval: libc::c_int = if keepalive { 1 } else { 0 };
            unsafe {
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

        #[cfg(target_os = "linux")]
        {
            if let Some(keep_idle) = self.keepalive_time {
                unsafe {
                    let optval = keep_idle as libc::c_int;
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

            if let Some(keep_intvl) = self.keepalive_interval {
                unsafe {
                    let optval = keep_intvl as libc::c_int;
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

            if let Some(keep_cnt) = self.keepalive_count {
                unsafe {
                    let optval = keep_cnt as libc::c_int;
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
        }

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            if let Some(keep_idle) = self.keepalive_time {
                unsafe {
                    let optval = keep_idle as libc::c_int;
                    let ret = setsockopt(
                        fd,
                        IPPROTO_TCP,
                        libc::TCP_KEEPALIVE,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                    if ret != 0 {
                        return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                            "Failed to set TCP_KEEPALIVE: {}",
                            std::io::Error::last_os_error()
                        )));
                    }
                }
            }

            if let Some(keep_intvl) = self.keepalive_interval {
                unsafe {
                    let optval = keep_intvl as libc::c_int;
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

            if let Some(keep_cnt) = self.keepalive_count {
                unsafe {
                    let optval = keep_cnt as libc::c_int;
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
        }

        Ok(())
    }

    #[cfg(not(unix))]
    fn apply_keepalive(&self, _socket: &Socket) -> PyResult<()> {
        // Keepalive is Unix-specific for now
        Ok(())
    }

    /// Apply SO_REUSEPORT option
    #[cfg(all(unix, not(target_os = "solaris")))]
    fn apply_reuseport(&self, socket: &Socket) -> PyResult<()> {
        use std::os::unix::io::AsRawFd;

        if let Some(reuseport) = self.so_reuseport {
            let fd = socket.as_raw_fd();
            unsafe {
                let optval: libc::c_int = if reuseport { 1 } else { 0 };
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

    #[cfg(not(all(unix, not(target_os = "solaris"))))]
    fn apply_reuseport(&self, _socket: &Socket) -> PyResult<()> {
        // SO_REUSEPORT is not available on this platform
        Ok(())
    }

    /// Apply socket options to a raw TcpStream
    pub fn apply_to_stream(&self, stream: &TcpStream) -> PyResult<()> {
        #[cfg(unix)]
        {
            use libc::{IPPROTO_TCP, SO_KEEPALIVE, SOL_SOCKET, setsockopt};
            use std::os::unix::io::AsRawFd;

            let fd = stream.as_raw_fd();

            if let Some(nodelay) = self.tcp_nodelay {
                unsafe {
                    let optval: libc::c_int = if nodelay { 1 } else { 0 };
                    let ret = setsockopt(
                        fd,
                        IPPROTO_TCP,
                        libc::TCP_NODELAY,
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

            if let Some(keepalive) = self.keepalive {
                unsafe {
                    let optval: libc::c_int = if keepalive { 1 } else { 0 };
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

            #[cfg(target_os = "linux")]
            {
                if let Some(keep_idle) = self.keepalive_time {
                    unsafe {
                        let optval = keep_idle as libc::c_int;
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

                if let Some(keep_intvl) = self.keepalive_interval {
                    unsafe {
                        let optval = keep_intvl as libc::c_int;
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

                if let Some(keep_cnt) = self.keepalive_count {
                    unsafe {
                        let optval = keep_cnt as libc::c_int;
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
            }

            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                if let Some(keep_idle) = self.keepalive_time {
                    unsafe {
                        let optval = keep_idle as libc::c_int;
                        let ret = setsockopt(
                            fd,
                            IPPROTO_TCP,
                            libc::TCP_KEEPALIVE,
                            &optval as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&optval) as libc::socklen_t,
                        );
                        if ret != 0 {
                            return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                                "Failed to set TCP_KEEPALIVE: {}",
                                std::io::Error::last_os_error()
                            )));
                        }
                    }
                }

                if let Some(keep_intvl) = self.keepalive_interval {
                    unsafe {
                        let optval = keep_intvl as libc::c_int;
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

                if let Some(keep_cnt) = self.keepalive_count {
                    unsafe {
                        let optval = keep_cnt as libc::c_int;
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
            }

            if let Some(reuseport) = self.so_reuseport {
                unsafe {
                    let optval: libc::c_int = if reuseport { 1 } else { 0 };
                    let ret = libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_REUSEPORT,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                    if ret != 0 && cfg!(not(target_os = "solaris")) {
                        return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                            "Failed to set SO_REUSEPORT: {}",
                            std::io::Error::last_os_error()
                        )));
                    }
                }
            }

            if let Some(reuse_addr) = self.so_reuseaddr {
                unsafe {
                    let optval: libc::c_int = if reuse_addr { 1 } else { 0 };
                    let ret = setsockopt(
                        fd,
                        SOL_SOCKET,
                        libc::SO_REUSEADDR,
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

            if let Some(rcvbuf) = self.so_rcvbuf {
                unsafe {
                    let optval = rcvbuf as libc::c_int;
                    let ret = setsockopt(
                        fd,
                        SOL_SOCKET,
                        libc::SO_RCVBUF,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                    if ret != 0 {
                        return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                            "Failed to set SO_RCVBUF: {}",
                            std::io::Error::last_os_error()
                        )));
                    }
                }
            }

            if let Some(sndbuf) = self.so_sndbuf {
                unsafe {
                    let optval = sndbuf as libc::c_int;
                    let ret = setsockopt(
                        fd,
                        SOL_SOCKET,
                        libc::SO_SNDBUF,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                    if ret != 0 {
                        return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                            "Failed to set SO_SNDBUF: {}",
                            std::io::Error::last_os_error()
                        )));
                    }
                }
            }
        }

        #[cfg(not(unix))]
        {
            // For non-Unix platforms, just ignore for now
            let _ = self;
        }

        Ok(())
    }
}

#[pyclass(module = "veloxloop._veloxloop")]
pub struct SocketOptions {
    pub inner: InnerSocketOptions,
}

#[pymethods]
impl SocketOptions {
    /// Create a new SocketOptions instance
    #[new]
    fn new() -> Self {
        Self {
            inner: InnerSocketOptions::new(),
        }
    }

    /// Set TCP_NODELAY option
    /// If enabled, disables Nagle's algorithm (sends data immediately)
    fn set_tcp_nodelay(&mut self, enabled: bool) -> PyResult<()> {
        self.inner.tcp_nodelay = Some(enabled);
        Ok(())
    }

    /// Get TCP_NODELAY option
    fn get_tcp_nodelay(&self) -> Option<bool> {
        self.inner.tcp_nodelay
    }

    /// Set SO_KEEPALIVE option
    /// If enabled, enables TCP keep-alive
    fn set_keepalive(&mut self, enabled: bool) -> PyResult<()> {
        self.inner.keepalive = Some(enabled);
        Ok(())
    }

    /// Get SO_KEEPALIVE option
    fn get_keepalive(&self) -> Option<bool> {
        self.inner.keepalive
    }

    /// Set TCP keep-alive idle time (in seconds)
    /// Time before first keep-alive probe after no activity (Linux: TCP_KEEPIDLE, macOS: TCP_KEEPALIVE)
    fn set_keepalive_time(&mut self, seconds: u32) -> PyResult<()> {
        self.inner.keepalive_time = Some(seconds);
        Ok(())
    }

    /// Get TCP keep-alive idle time
    fn get_keepalive_time(&self) -> Option<u32> {
        self.inner.keepalive_time
    }

    /// Set TCP keep-alive probe interval (in seconds)
    /// Interval between successive keep-alive probes (Linux: TCP_KEEPINTVL, macOS: TCP_KEEPINTVL)
    fn set_keepalive_interval(&mut self, seconds: u32) -> PyResult<()> {
        self.inner.keepalive_interval = Some(seconds);
        Ok(())
    }

    /// Get TCP keep-alive probe interval
    fn get_keepalive_interval(&self) -> Option<u32> {
        self.inner.keepalive_interval
    }

    /// Set TCP keep-alive probe count
    /// Number of unacknowledged probes before connection is closed (Linux: TCP_KEEPCNT, macOS: TCP_KEEPCNT)
    fn set_keepalive_count(&mut self, count: u32) -> PyResult<()> {
        self.inner.keepalive_count = Some(count);
        Ok(())
    }

    /// Get TCP keep-alive probe count
    fn get_keepalive_count(&self) -> Option<u32> {
        self.inner.keepalive_count
    }

    /// Set SO_REUSEADDR option
    /// If enabled, allows reusing addresses in TIME_WAIT state
    fn set_reuse_address(&mut self, enabled: bool) -> PyResult<()> {
        self.inner.so_reuseaddr = Some(enabled);
        Ok(())
    }

    /// Get SO_REUSEADDR option
    fn get_reuse_address(&self) -> Option<bool> {
        self.inner.so_reuseaddr
    }

    /// Set SO_REUSEPORT option
    /// If enabled, allows multiple sockets to bind to the same port (Unix only, not Solaris)
    fn set_reuse_port(&mut self, enabled: bool) -> PyResult<()> {
        self.inner.so_reuseport = Some(enabled);
        Ok(())
    }

    /// Get SO_REUSEPORT option
    fn get_reuse_port(&self) -> Option<bool> {
        self.inner.so_reuseport
    }

    /// Set SO_RCVBUF option
    /// Receive buffer size in bytes
    fn set_recv_buffer_size(&mut self, size: usize) -> PyResult<()> {
        self.inner.so_rcvbuf = Some(size);
        Ok(())
    }

    /// Get SO_RCVBUF option
    fn get_recv_buffer_size(&self) -> Option<usize> {
        self.inner.so_rcvbuf
    }

    /// Set SO_SNDBUF option
    /// Send buffer size in bytes
    fn set_send_buffer_size(&mut self, size: usize) -> PyResult<()> {
        self.inner.so_sndbuf = Some(size);
        Ok(())
    }

    /// Get SO_SNDBUF option
    fn get_send_buffer_size(&self) -> Option<usize> {
        self.inner.so_sndbuf
    }

    /// Reset all options to None
    fn reset(&mut self) -> PyResult<()> {
        self.inner = InnerSocketOptions::new();
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!(
            "SocketOptions(tcp_nodelay={:?}, keepalive={:?}, keepalive_time={:?}, keepalive_interval={:?}, keepalive_count={:?}, reuse_address={:?}, reuse_port={:?}, rcvbuf={:?}, sndbuf={:?})",
            self.inner.tcp_nodelay,
            self.inner.keepalive,
            self.inner.keepalive_time,
            self.inner.keepalive_interval,
            self.inner.keepalive_count,
            self.inner.so_reuseaddr,
            self.inner.so_reuseport,
            self.inner.so_rcvbuf,
            self.inner.so_sndbuf,
        )
    }
}
