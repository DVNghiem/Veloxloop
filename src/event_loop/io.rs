use crate::event_loop::VeloxLoop;
use crate::handles::IoCallback;
use crate::poller::PollerEvent;
use pyo3::prelude::*;
use std::os::fd::RawFd;
use std::sync::Arc;

#[cfg(target_os = "linux")]
use crate::poller::IoToken;

impl VeloxLoop {
    pub fn add_reader_native(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::Native(callback))
    }

    pub(crate) fn add_reader_internal(&self, fd: RawFd, callback: IoCallback) -> PyResult<()> {
        // Track I/O operation
        self.track_io_operation();
        
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_reader(fd, callback);

        // Use PollerEvent::new for combined readable + writable interest
        let ev = PollerEvent::new(fd as usize, true, writer_exists);

        if reader_exists || writer_exists {
            self.poller.borrow_mut().modify(fd, ev)?;
        } else {
            self.poller.borrow_mut().register(fd, ev)?;
        }
        Ok(())
    }

    /// Add a reader with oneshot mode (Linux only optimization).
    #[cfg(target_os = "linux")]
    pub fn add_reader_oneshot(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        let mut handles = self.handles.borrow_mut();
        handles.add_reader(fd, IoCallback::Native(callback));
        drop(handles);

        let ev = PollerEvent::readable(fd as usize);

        // Check if this FD is in the disabled-oneshot set
        let in_oneshot_set = self.oneshot_disabled.borrow_mut().remove(&fd);

        let mut poller = self.poller.borrow_mut();
        if in_oneshot_set {
            // FD is registered but disabled - rearm with MOD (1 syscall)
            if let Err(e) = poller.rearm_oneshot(fd, ev) {
                let err_msg = e.to_string();
                if err_msg.contains("No such file or directory") || err_msg.contains("os error 2") {
                    poller.register_oneshot(fd, ev)?;
                } else {
                    return Err(e.into());
                }
            }
        } else {
            // FD not registered - register with oneshot (1 syscall)
            poller.register_oneshot(fd, ev)?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn mark_oneshot_disabled(&self, fd: RawFd) {
        // Remove from handles since callback has fired
        self.handles.borrow_mut().remove_reader(fd);
        // Track that this FD is still registered but disabled
        self.oneshot_disabled.borrow_mut().insert(fd);
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub fn cleanup_oneshot(&self, fd: RawFd) -> PyResult<()> {
        if self.oneshot_disabled.borrow_mut().remove(&fd) {
            // FD was in disabled state - need to delete it
            self.poller.borrow_mut().delete(fd)?;
        }
        Ok(())
    }

    pub fn add_writer_native(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        self.add_writer_internal(fd, IoCallback::Native(callback))
    }

    pub(crate) fn add_writer_internal(&self, fd: RawFd, callback: IoCallback) -> PyResult<()> {
        // Track I/O operation
        self.track_io_operation();
        
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_writer(fd, callback);

        // Use PollerEvent::new for combined readable + writable interest
        let ev = PollerEvent::new(fd as usize, reader_exists, true);

        if reader_exists || writer_exists {
            self.poller.borrow_mut().modify(fd, ev)?;
        } else {
            self.poller.borrow_mut().register(fd, ev)?;
        }
        Ok(())
    }

    pub fn add_tcp_reader(
        &self,
        fd: RawFd,
        transport: Py<crate::transports::tcp::TcpTransport>,
    ) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::TcpRead(transport))
    }

    pub fn add_tcp_writer(
        &self,
        fd: RawFd,
        transport: Py<crate::transports::tcp::TcpTransport>,
    ) -> PyResult<()> {
        self.add_writer_internal(fd, IoCallback::TcpWrite(transport))
    }
}

impl VeloxLoop {
    pub fn add_reader(&self, _py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::Python(callback))
    }

    pub fn remove_reader(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        let mut handles = self.handles.borrow_mut();
        if handles.remove_reader(fd) {
            let writer_exists = handles.get_writer(fd).is_some();

            if writer_exists {
                // Downgrade to W only
                let ev = PollerEvent::writable(fd as usize);
                self.poller.borrow_mut().modify(fd, ev)?;
            } else {
                // Remove
                self.poller.borrow_mut().delete(fd)?;
            }
            #[cfg(target_os = "linux")]
            self.oneshot_disabled.borrow_mut().remove(&fd);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn add_writer(&self, _py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        self.add_writer_internal(fd, IoCallback::Python(callback))
    }

    pub fn remove_writer(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        let mut handles = self.handles.borrow_mut();
        if handles.remove_writer(fd) {
            let reader_exists = handles.get_reader(fd).is_some();

            if reader_exists {
                // Downgrade to R only
                let ev = PollerEvent::readable(fd as usize);
                self.poller.borrow_mut().modify(fd, ev)?;
            } else {
                // Remove
                self.poller.borrow_mut().delete(fd)?;
            }
            #[cfg(target_os = "linux")]
            self.oneshot_disabled.borrow_mut().remove(&fd);

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(target_os = "linux")]
impl VeloxLoop {
    /// Submit an async read operation via io-uring for true zero-copy I/O
    /// Returns a token to track completion. The operation completes in the
    /// kernel without additional syscalls.
    #[inline]
    pub fn submit_async_read(
        &self,
        fd: RawFd,
        buf: &mut [u8],
        offset: Option<u64>,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_read(fd, buf, offset)
            .map_err(|e| e.into())
    }

    /// Submit an async write operation via io-uring
    #[inline]
    pub fn submit_async_write(
        &self,
        fd: RawFd,
        buf: &[u8],
        offset: Option<u64>,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_write(fd, buf, offset)
            .map_err(|e| e.into())
    }

    /// Submit an async recv operation via io-uring
    #[inline]
    pub fn submit_async_recv(
        &self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_recv(fd, buf, flags)
            .map_err(|e| e.into())
    }

    /// Submit an async send operation via io-uring
    #[inline]
    pub fn submit_async_send(
        &self,
        fd: RawFd,
        buf: &[u8],
        flags: i32,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_send(fd, buf, flags)
            .map_err(|e| e.into())
    }

    /// Submit an async accept operation via io-uring
    #[inline]
    pub fn submit_async_accept(&self, fd: RawFd) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_accept(fd)
            .map_err(|e| e.into())
    }

    /// Submit an async connect operation via io-uring
    #[inline]
    pub fn submit_async_connect(
        &self,
        fd: RawFd,
        addr: std::net::SocketAddr,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_connect(fd, addr)
            .map_err(|e| e.into())
    }

    /// Submit an async close operation via io-uring
    #[inline]
    pub fn submit_async_close(&self, fd: RawFd) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_close(fd)
            .map_err(|e| e.into())
    }

    /// Submit an async sendfile/splice operation via io-uring
    /// Uses kernel-side zero-copy file transfer
    #[inline]
    pub fn submit_async_sendfile(
        &self,
        out_fd: RawFd,
        in_fd: RawFd,
        offset: u64,
        count: usize,
    ) -> PyResult<IoToken> {
        self.poller
            .borrow_mut()
            .submit_sendfile(out_fd, in_fd, offset, count)
            .map_err(|e| e.into())
    }

    /// Cancel an in-flight io-uring operation
    #[inline]
    pub fn cancel_async_operation(&self, token: IoToken) -> PyResult<()> {
        self.poller
            .borrow_mut()
            .cancel_operation(token)
            .map_err(|e| e.into())
    }
}