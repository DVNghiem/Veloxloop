use pyo3::prelude::*;
use std::collections::HashMap;
use std::os::fd::RawFd;

pub enum IoCallback {
    Python(Py<PyAny>),
    Native(std::sync::Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>),
    // Specialized handlers for common transports
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

pub struct IoHandles {
    // Maps FD to (Reader, Writer)
    pub(crate) map: HashMap<RawFd, (Option<Handle>, Option<Handle>)>,
}

impl IoHandles {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get_states(&self, fd: RawFd) -> (bool, bool) {
        if let Some((r, w)) = self.map.get(&fd) {
            (r.is_some(), w.is_some())
        } else {
            (false, false)
        }
    }

    pub fn get_state_owned(&self, fd: RawFd) -> Option<(Option<Handle>, Option<Handle>)> {
        self.map.get(&fd).map(|(r, w)| (r.clone(), w.clone()))
    }

    pub fn add_reader(&mut self, fd: RawFd, callback: IoCallback) {
        let pair = self.map.entry(fd).or_insert((None, None));
        pair.0 = Some(Handle {
            callback,
            cancelled: false,
        });
    }

    pub fn remove_reader(&mut self, fd: RawFd) -> bool {
        if let Some(pair) = self.map.get_mut(&fd) {
            if pair.0.is_some() {
                pair.0 = None;
                if pair.1.is_none() {
                    self.map.remove(&fd);
                }
                return true;
            }
        }
        false
    }

    pub fn add_writer(&mut self, fd: RawFd, callback: IoCallback) {
        let pair = self.map.entry(fd).or_insert((None, None));
        pair.1 = Some(Handle {
            callback,
            cancelled: false,
        });
    }

    pub fn remove_writer(&mut self, fd: RawFd) -> bool {
        if let Some(pair) = self.map.get_mut(&fd) {
            if pair.1.is_some() {
                pair.1 = None;
                if pair.0.is_none() {
                    self.map.remove(&fd);
                }
                return true;
            }
        }
        false
    }

    pub fn get_reader(&self, fd: RawFd) -> Option<&Handle> {
        self.map.get(&fd).and_then(|v| v.0.as_ref())
    }

    pub fn get_writer(&self, fd: RawFd) -> Option<&Handle> {
        self.map.get(&fd).and_then(|v| v.1.as_ref())
    }
}
