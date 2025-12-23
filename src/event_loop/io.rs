use crate::event_loop::VeloxLoop;
use crate::handles::IoCallback;
use crate::poller::PollerEvent;
use pyo3::prelude::*;
use std::os::fd::RawFd;
use std::sync::Arc;

impl VeloxLoop {
    pub fn add_reader_native(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::Native(callback))
    }

    pub(crate) fn add_reader_internal(&self, fd: RawFd, callback: IoCallback) -> PyResult<()> {
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_reader(fd, callback);

        let mut ev = PollerEvent::readable(fd as usize);
        if writer_exists {
            ev.writable = true;
        }

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
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_writer(fd, callback);

        let mut ev = PollerEvent::writable(fd as usize);
        if reader_exists {
            ev.readable = true;
        }

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
