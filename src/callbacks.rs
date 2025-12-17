use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::collections::VecDeque;
use parking_lot::Mutex;
use std::sync::Arc;
use std::os::fd::{AsRawFd, RawFd};

use crate::poller::LoopPoller;
use crate::event_loop::VeloxLoop;
use crate::transports::tcp::TcpTransport;
use crate::transports::ssl::{SSLContext, SSLTransport};
use crate::transports::future::PendingFuture;

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
        // We always notify for now to ensure loop wakes up
        // Optimization: only notify if loop is sleeping? 
        // Poller::notify is cheap on Linux (eventfd write), verifying calling it is safe.
        // But LoopPoller struct I made wraps `Arc<Poller>`.
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
                            // Create Transport (SSL or regular TCP)
                            let transport_result: PyResult<(Py<PyAny>, Py<PyAny>)> = if let Some(ssl_ctx) = &self.ssl_context {
                                // Create SSL transport
                                let ssl_transport = SSLTransport::new_client(
                                    self.loop_.clone_ref(py),
                                    stream,
                                    protocol.clone_ref(py),
                                    ssl_ctx.clone_ref(py),
                                    self.server_hostname.clone(),
                                    py,
                                )?;
                                let transport_py = Py::new(py, ssl_transport)?;
                                
                                // Add reader for SSL handshake and data
                                let read_ready = transport_py.getattr(py, "_read_ready")?;
                                loop_ref.borrow().add_reader(py, fd, read_ready)?;
                                
                                // Add writer for SSL handshake
                                let write_ready = transport_py.getattr(py, "_write_ready")?;
                                loop_ref.borrow().add_writer(py, fd, write_ready)?;
                                
                                Ok((transport_py.into_any(), protocol.clone_ref(py)))
                            } else {
                                // Create regular TCP transport
                                let transport = TcpTransport::new(
                                    self.loop_.clone_ref(py),
                                    stream,
                                    protocol.clone_ref(py),
                                )?;
                                let transport_py = Py::new(py, transport)?;
                                
                                // connection_made
                                protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
                                
                                // Add reader
                                let read_ready = transport_py.getattr(py, "_read_ready")?;
                                loop_ref.borrow().add_reader(py, fd, read_ready)?;
                                
                                Ok((transport_py.into_any(), protocol.clone_ref(py)))
                            };
                            
                            match transport_result {
                                Ok((transport_py, protocol)) => {
                                    // Set result: (transport, protocol)
                                    let res = PyTuple::new(py, &[transport_py, protocol])?.into_any();
                                    self.future.bind(py).borrow().set_result(res.unbind())?;
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
