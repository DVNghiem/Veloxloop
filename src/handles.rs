use pyo3::prelude::*;
use std::os::fd::RawFd;
use std::sync::Arc;

pub use crate::concurrent::ConcurrentIntMap;

pub enum IoCallback {
    Python(Py<PyAny>),
    Native(Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>),
    // Specialized handlers for common transports - direct dispatch without dynamic dispatch
    TcpRead(Py<crate::transports::tcp::TcpTransport>),
    TcpWrite(Py<crate::transports::tcp::TcpTransport>),
}

impl Clone for IoCallback {
    fn clone(&self) -> Self {
        match self {
            IoCallback::Python(cb) => Python::attach(|py| IoCallback::Python(cb.clone_ref(py))),
            IoCallback::Native(cb) => IoCallback::Native(cb.clone()),
            IoCallback::TcpRead(cb) => Python::attach(|py| IoCallback::TcpRead(cb.clone_ref(py))),
            IoCallback::TcpWrite(cb) => Python::attach(|py| IoCallback::TcpWrite(cb.clone_ref(py))),
        }
    }
}

#[derive(Clone)]
pub struct Handle {
    pub callback: IoCallback,
    pub cancelled: bool,
}

impl Handle {
    /// Execute the callback - inlined for performance
    #[inline(always)]
    pub fn execute(&self, py: Python<'_>) -> PyResult<()> {
        if self.cancelled {
            return Ok(());
        }
        match &self.callback {
            IoCallback::Python(cb) => {
                cb.call0(py)?;
                Ok(())
            }
            IoCallback::Native(cb) => cb(py),
            IoCallback::TcpRead(tcp) => {
                crate::transports::tcp::TcpTransport::_read_ready(tcp.bind(py))
            }
            IoCallback::TcpWrite(tcp) => {
                let tcp_bound = tcp.bind(py);
                crate::transports::tcp::TcpTransport::_write_ready(&mut *tcp_bound.borrow_mut(), py)
            }
        }
    }
}

/// Lock-free I/O handle storage using DashMap for true concurrent access
/// Uses ConcurrentIntMap (wrapping DashMap) for zero-lock contention in multi-threaded scenarios
pub struct IoHandles {
    // Maps FD to (Reader, Writer) - lock-free concurrent map
    pub(crate) map: ConcurrentIntMap<(Option<Handle>, Option<Handle>)>,
}

impl IoHandles {
    pub fn new() -> Self {
        Self {
            map: ConcurrentIntMap::with_capacity(256),
        }
    }

    #[inline]
    pub fn get_states(&self, fd: RawFd) -> (bool, bool) {
        if let Some(pair) = self.map.get(&fd) {
            (pair.0.is_some(), pair.1.is_some())
        } else {
            (false, false)
        }
    }

    #[inline]
    pub fn get_state_owned(&self, fd: RawFd) -> Option<(Option<Handle>, Option<Handle>)> {
        self.map.get(&fd).map(|pair| (pair.0.clone(), pair.1.clone()))
    }

    #[inline]
    pub fn add_reader(&mut self, fd: RawFd, callback: IoCallback) {
        use dashmap::mapref::entry::Entry;
        match self.map.entry(fd) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().0 = Some(Handle {
                    callback,
                    cancelled: false,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert((Some(Handle {
                    callback,
                    cancelled: false,
                }), None));
            }
        }
    }

    #[inline]
    pub fn remove_reader(&mut self, fd: RawFd) -> bool {
        if let Some(mut pair) = self.map.get_mut(&fd) {
            if pair.0.is_some() {
                pair.0 = None;
                let should_remove = pair.1.is_none();
                drop(pair); // Release lock before remove
                if should_remove {
                    self.map.remove(&fd);
                }
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn add_writer(&mut self, fd: RawFd, callback: IoCallback) {
        use dashmap::mapref::entry::Entry;
        match self.map.entry(fd) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().1 = Some(Handle {
                    callback,
                    cancelled: false,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert((None, Some(Handle {
                    callback,
                    cancelled: false,
                })));
            }
        }
    }

    #[inline]
    pub fn remove_writer(&mut self, fd: RawFd) -> bool {
        if let Some(mut pair) = self.map.get_mut(&fd) {
            if pair.1.is_some() {
                pair.1 = None;
                let should_remove = pair.0.is_none();
                drop(pair); // Release lock before remove
                if should_remove {
                    self.map.remove(&fd);
                }
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn get_reader(&self, fd: RawFd) -> Option<Handle> {
        self.map.get(&fd).and_then(|v| v.0.clone())
    }

    #[inline]
    pub fn get_writer(&self, fd: RawFd) -> Option<Handle> {
        self.map.get(&fd).and_then(|v| v.1.clone())
    }
}
