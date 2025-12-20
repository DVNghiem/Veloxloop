use papaya::{HashMap, Operation};
use pyo3::prelude::*;
use std::os::fd::RawFd;
use std::sync::Arc;

pub enum IoCallback {
    Python(Py<PyAny>),
    Native(Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>),
    // Specialized handlers for common transports
    TcpRead(Py<crate::transports::tcp::TcpTransport>),
    TcpWrite(Py<crate::transports::tcp::TcpTransport>),
}

impl Clone for IoCallback {
    fn clone(&self) -> Self {
        match self {
            IoCallback::Python(cb) => {
                Python::attach(|py| IoCallback::Python(cb.clone_ref(py)))
            }
            IoCallback::Native(cb) => IoCallback::Native(cb.clone()),
            IoCallback::TcpRead(cb) => {
                Python::attach(|py| IoCallback::TcpRead(cb.clone_ref(py)))
            }
            IoCallback::TcpWrite(cb) => {
                Python::attach(|py| IoCallback::TcpWrite(cb.clone_ref(py)))
            }
        }
    }
}

pub struct Handle {
    pub callback: IoCallback,
    pub cancelled: bool,
}

pub struct IoHandles {
    // Maps FD to (Reader, Writer)
    pub(crate) map: HashMap<RawFd, (Option<Arc<Handle>>, Option<Arc<Handle>>)>,
}

impl IoHandles {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn add_reader(&self, fd: RawFd, callback: IoCallback) {
        let pin = self.map.guard();
        self.map.compute(
            fd,
            |prev| {
                let mut pair = prev.map(|(_k, v)| v.clone()).unwrap_or((None, None));
                pair.0 = Some(Arc::new(Handle {
                    callback: callback.clone(),
                    cancelled: false,
                }));
                Operation::<_, ()>::Insert(pair)
            },
            &pin,
        );
    }

    pub fn remove_reader(&self, fd: RawFd) -> bool {
        let pin = self.map.guard();
        let mut existed = false;
        self.map.compute(
            fd,
            |prev| {
                if let Some((_k, v)) = prev {
                    let mut pair = v.clone();
                    if pair.0.is_some() {
                        existed = true;
                        pair.0 = None;
                        if pair.1.is_none() {
                            return Operation::Remove;
                        }
                        return Operation::<_, ()>::Insert(pair);
                    }
                }
                Operation::Abort(())
            },
            &pin,
        );
        existed
    }

    pub fn add_writer(&self, fd: RawFd, callback: IoCallback) {
        let pin = self.map.guard();
        self.map.compute(
            fd,
            |prev| {
                let mut pair = prev.map(|(_k, v)| v.clone()).unwrap_or((None, None));
                pair.1 = Some(Arc::new(Handle {
                    callback: callback.clone(),
                    cancelled: false,
                }));
                Operation::<_, ()>::Insert(pair)
            },
            &pin,
        );
    }

    pub fn remove_writer(&self, fd: RawFd) -> bool {
        let pin = self.map.guard();
        let mut existed = false;
        self.map.compute(
            fd,
            |prev| {
                if let Some((_k, v)) = prev {
                    let mut pair = v.clone();
                    if pair.1.is_some() {
                        existed = true;
                        pair.1 = None;
                        if pair.0.is_none() {
                            return Operation::Remove;
                        }
                        return Operation::<_, ()>::Insert(pair);
                    }
                }
                Operation::Abort(())
            },
            &pin,
        );
        existed
    }

    pub fn get_reader(&self, fd: RawFd, pin: &impl papaya::Guard) -> Option<Arc<Handle>> {
        self.map.get(&fd, pin).and_then(|v| v.0.as_ref().cloned())
    }

    pub fn get_writer(&self, fd: RawFd, pin: &impl papaya::Guard) -> Option<Arc<Handle>> {
        self.map.get(&fd, pin).and_then(|v| v.1.as_ref().cloned())
    }

    pub fn get_pair(&self, fd: RawFd, pin: &impl papaya::Guard) -> Option<(Option<Arc<Handle>>, Option<Arc<Handle>>)> {
        self.map.get(&fd, pin).map(|v| ((*v).0.clone(), (*v).1.clone()))
    }
}
