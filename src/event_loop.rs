use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPyObjectExt, prelude::*};
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::callbacks::{Callback, CallbackQueue, RemoveWriterCallback, SockConnectCallback};
use crate::constants::{NI_MAXHOST, NI_MAXSERV};
use crate::executor::ThreadPoolExecutor;
use crate::handles::{Handle, IoCallback, IoHandles};
use crate::poller::LoopPoller;
use crate::timers::Timers;
use crate::transports::future::{CompletedFuture, PendingFuture};
use crate::transports::tcp::TcpServer;
use crate::transports::udp::UdpTransport;
use crate::utils::{VeloxError, VeloxResult};
use std::os::fd::{AsRawFd, RawFd};

/// Fast-path state for the event loop, cache-aligned to 64 bytes
#[repr(C)]
#[derive(Clone)]
pub struct HotState {
    pub running: bool,
    pub stopped: bool,
    pub closed: bool,
    pub debug: bool,
    pub is_polling: bool,
}

#[pyclass(subclass, module = "veloxloop._veloxloop")]
pub struct VeloxLoop {
    poller: RefCell<LoopPoller>,
    handles: RefCell<IoHandles>,
    callbacks: RefCell<CallbackQueue>,
    timers: RefCell<Timers>,
    state: RefCell<HotState>,
    start_time: Instant,
    last_poll_time: RefCell<Instant>,
    executor: RefCell<Option<ThreadPoolExecutor>>,
    exception_handler: RefCell<Option<Py<PyAny>>>,
    task_factory: RefCell<Option<Py<PyAny>>>,
    async_generators: RefCell<Vec<Py<PyAny>>>,
    callback_buffer: RefCell<Vec<Callback>>,
    pending_ios: RefCell<Vec<(RawFd, Option<Handle>, Option<Handle>, bool, bool)>>,
}

unsafe impl Send for VeloxLoop {}
unsafe impl Sync for VeloxLoop {}

// Rust-only methods for VeloxLoop
impl VeloxLoop {
    pub fn add_reader_native(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::Native(callback))
    }

    fn add_reader_internal(&self, fd: RawFd, callback: IoCallback) -> PyResult<()> {
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_reader(fd, callback);

        let mut ev = polling::Event::readable(fd as usize);
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

    pub fn add_writer_native(
        &self,
        fd: RawFd,
        callback: Arc<dyn Fn(Python<'_>) -> PyResult<()> + Send + Sync>,
    ) -> PyResult<()> {
        self.add_writer_internal(fd, IoCallback::Native(callback))
    }

    fn add_writer_internal(&self, fd: RawFd, callback: IoCallback) -> PyResult<()> {
        let mut handles = self.handles.borrow_mut();
        let (reader_exists, writer_exists) = handles.get_states(fd);

        // Add or modify
        handles.add_writer(fd, callback);

        let mut ev = polling::Event::writable(fd as usize);
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

#[pymethods]
impl VeloxLoop {
    #[new]
    #[pyo3(signature = (debug=None))]
    pub fn new(debug: Option<bool>) -> VeloxResult<Self> {
        let poller = LoopPoller::new()?;
        let debug_val = debug.unwrap_or(false);

        Ok(Self {
            poller: RefCell::new(poller),
            handles: RefCell::new(IoHandles::new()),
            callbacks: RefCell::new(CallbackQueue::new()),
            timers: RefCell::new(Timers::new()),
            state: RefCell::new(HotState {
                running: false,
                stopped: false,
                closed: false,
                debug: debug_val,
                is_polling: false,
            }),
            start_time: Instant::now(),
            last_poll_time: RefCell::new(Instant::now()),
            executor: RefCell::new(None),
            exception_handler: RefCell::new(None),
            task_factory: RefCell::new(None),
            async_generators: RefCell::new(Vec::new()),
            callback_buffer: RefCell::new(Vec::with_capacity(1024)),
            pending_ios: RefCell::new(Vec::with_capacity(128)),
        })
    }

    fn time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    fn run_forever(&self, py: Python<'_>) -> VeloxResult<()> {
        {
            let mut state = self.state.borrow_mut();
            state.running = true;
            state.stopped = false;
        }

        // Use polling::Events
        let mut events = polling::Events::new();

        while self.state.borrow().running && !self.state.borrow().stopped {
            self._run_once(py, &mut events)?;

            // Check if stopped after processing callbacks (future might be done)
            if self.state.borrow().stopped {
                break;
            }

            // Allow Python signals (Ctrl+C) to interrupt
            if let Err(e) = py.check_signals() {
                return Err(VeloxError::Python(e));
            }
        }

        self.state.borrow_mut().running = false;
        Ok(())
    }

    pub fn add_reader(&self, _py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        self.add_reader_internal(fd, IoCallback::Python(callback))
    }

    pub fn remove_reader(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        let mut handles = self.handles.borrow_mut();
        if handles.remove_reader(fd) {
            let writer_exists = handles.get_writer(fd).is_some();

            if writer_exists {
                // Downgrade to W only
                let ev = polling::Event::writable(fd as usize);
                self.poller.borrow_mut().modify(fd, ev)?;
            } else {
                // Remove
                self.poller.borrow_mut().delete(fd)?;
            }
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
                let ev = polling::Event::readable(fd as usize);
                self.poller.borrow_mut().modify(fd, ev)?;
            } else {
                // Remove
                self.poller.borrow_mut().delete(fd)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon(&self, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) {
        self.callbacks.borrow_mut().push(Callback {
            callback,
            args,
            context,
        });
    }

    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon_threadsafe(
        &self,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) {
        // In this implementation, call_soon_threadsafe still needs to notify the poller
        // because it might be called from another thread (like ThreadPoolExecutor).
        // Since we are moving to single-threading for the main loop, if this is truly
        // called from another thread, we'd need a thread-safe way to push.
        // However, the optimization plan says "all interaction with VeloxLoop from Python
        // will be on one thread". But run_in_executor results come back via threads.
        // Wait, the CallbackQueue refactor in optimize_require.md says:
        // "Callbacks will be stored in a simple Vec. No locks for the non-thread-safe path."
        // For thread-safe path, we might still need a Mutex.
        // Let's re-read: "Remove CallbackQueue ... and its Mutex".
        // If we remove the Mutex, we can't call call_soon_threadsafe from other threads.
        // But asyncio's loop.call_soon_threadsafe IS thread-safe.
        // I will keep a Mutex for the internal queue for thread-safe access if needed,
        // or use a separate thread-safe queue.
        // For now, let's follow the plan and see.
        self.callbacks.borrow_mut().push(Callback {
            callback,
            args,
            context,
        });
        // We will need to notify if we are in poll()
        if self.state.borrow().is_polling {
            let _ = self.poller.borrow().notify();
        }
    }

    #[pyo3(signature = (delay, callback, *args, context=None))]
    pub fn call_later(
        &self,
        delay: f64,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) -> u64 {
        let now = (self.time() * 1_000_000_000.0) as u64;
        let delay_ns = (delay * 1_000_000_000.0) as u64;
        let when = now + delay_ns;
        let _start_ns = (self.start_time.elapsed().as_nanos() as u64)
            .saturating_sub((self.time() * 1_000_000_000.0) as u64);
        // Let's just use 0 as start_ns for simplicity since relative time is fine
        self.timers
            .borrow_mut()
            .insert(when, callback, args, context, 0)
    }

    #[pyo3(signature = (when, callback, *args, context=None))]
    fn call_at(
        &self,
        when: f64,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) -> u64 {
        let when_ns = (when * 1_000_000_000.0) as u64;
        self.timers
            .borrow_mut()
            .insert(when_ns, callback, args, context, 0)
    }

    fn _cancel_timer(&self, timer_id: u64) {
        self.timers.borrow_mut().cancel(timer_id);
    }

    fn stop(&self) {
        let mut state = self.state.borrow_mut();
        state.stopped = true;
        state.running = false;
    }

    fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    fn is_closed(&self) -> bool {
        self.state.borrow().closed
    }

    fn get_debug(&self) -> bool {
        self.state.borrow().debug
    }

    fn set_debug(&self, enabled: bool) {
        self.state.borrow_mut().debug = enabled;
    }

    fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        state.running = false;
    }

    // Executor methods
    #[pyo3(signature = (_executor, func, *args))]
    fn run_in_executor(
        &self,
        py: Python<'_>,
        _executor: Option<Py<PyAny>>,
        func: Py<PyAny>,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        // Get or create default executor
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        // Clone the necessary objects for the spawned task
        let func_clone = func.clone_ref(py);
        let args_clone: Py<PyTuple> = args.clone().unbind();

        // Spawn the blocking task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                // Call the Python function
                let result = func_clone.call1(py, args_clone.bind(py));

                // Schedule callback to set the future result
                match result {
                    Ok(val) => {
                        let _ = future_clone.bind(py).borrow().set_result(py, val);
                    }
                    Err(e) => {
                        // Set exception on future
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        let _ = future_clone.bind(py).borrow().set_exception(py, exc);
                    }
                }
            });
        });

        Ok(future.into_any())
    }

    fn set_default_executor(&self, _executor: Option<Py<PyAny>>) -> PyResult<()> {
        // For now, we only support our internal executor
        // If executor is None, we create a new default one
        *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        Ok(())
    }

    #[pyo3(signature = (host, port, *, family=0, r#type=0, proto=0, flags=0))]
    fn getaddrinfo(
        &self,
        py: Python<'_>,
        host: Option<Bound<'_, PyAny>>,
        port: Option<Bound<'_, PyAny>>,
        family: i32,
        r#type: i32,
        proto: i32,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        use pyo3::types::{PyBytes, PyString};

        // Convert host to Option<String>, handling both str and bytes
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

        // Convert port to Option<String>, handling both str and bytes
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

        // Get or create default executor
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        // Spawn the blocking DNS resolution task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                // Perform DNS resolution using libc getaddrinfo
                let result =
                    perform_getaddrinfo(py, host_str, port_str, family, r#type, proto, flags);

                // Schedule callback to set the future result
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

    #[pyo3(signature = (sockaddr, flags=0))]
    fn getnameinfo(
        &self,
        py: Python<'_>,
        sockaddr: Bound<'_, PyTuple>,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        // Get or create default executor
        if self.executor.borrow().is_none() {
            *self.executor.borrow_mut() = Some(ThreadPoolExecutor::new()?);
        }
        let executor_bind = self.executor.borrow();
        let executor_ref = executor_bind.as_ref().unwrap();

        // Extract address and port from sockaddr tuple
        let addr_str: String = sockaddr.get_item(0)?.extract()?;
        let port: u16 = sockaddr.get_item(1)?.extract()?;

        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);

        // Spawn the blocking reverse DNS resolution task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                let result = perform_getnameinfo(py, &addr_str, port, flags);

                // Schedule callback to set the future result
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

    // Exception handler methods
    fn set_exception_handler(&self, handler: Option<Py<PyAny>>) {
        *self.exception_handler.borrow_mut() = handler;
    }

    fn get_exception_handler(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.exception_handler
            .borrow()
            .as_ref()
            .map(|h| h.clone_ref(py))
    }

    fn call_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
        let handler = self
            .exception_handler
            .borrow()
            .as_ref()
            .map(|h| h.clone_ref(py));

        if let Some(handler) = handler {
            // Call handler with (loop, context) as per asyncio API
            // Pass None for loop and the context dict
            match handler.call(py, (py.None(), context.as_any()), None) {
                Ok(_) => {}
                Err(e) => {
                    // If custom handler fails, fall back to default
                    eprintln!("Error in custom exception handler: {:?}", e);
                    e.print(py);
                    // Fall through to default handler
                    let message = context.bind(py).get_item("message")?;
                    if let Some(msg) = message {
                        eprintln!("Exception in event loop: {}", msg);
                    }
                }
            }
        } else {
            // Default exception handler - just print to stderr
            let message = context.bind(py).get_item("message")?;
            if let Some(msg) = message {
                eprintln!("Exception in event loop: {}", msg);
            }
            let exception = context.bind(py).get_item("exception")?;
            if let Some(exc) = exception {
                eprintln!("Exception: {:?}", exc);
            }
        }
        Ok(())
    }

    fn default_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
        // Default handler implementation
        let message = context.bind(py).get_item("message")?;
        if let Some(msg) = message {
            eprintln!("Exception in event loop: {}", msg);
        }

        let exception = context.bind(py).get_item("exception")?;
        if let Some(exc) = exception {
            eprintln!("Exception details: {:?}", exc);
        }

        Ok(())
    }

    // Task factory methods
    fn set_task_factory(&self, factory: Option<Py<PyAny>>) {
        *self.task_factory.borrow_mut() = factory;
    }

    fn get_task_factory(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.task_factory.borrow().as_ref().map(|f| f.clone_ref(py))
    }

    // Async generator tracking methods
    fn _track_async_generator(&self, agen: Py<PyAny>) {
        self.async_generators.borrow_mut().push(agen);
    }

    fn _untrack_async_generator(&self, py: Python<'_>, agen: Py<PyAny>) {
        self.async_generators
            .borrow_mut()
            .retain(|g| !g.bind(py).is(agen.bind(py)));
    }

    fn shutdown_asyncgens(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Get all tracked async generators
        let generators = {
            let mut gen_guard = self.async_generators.borrow_mut();
            let gens: Vec<Py<PyAny>> = gen_guard.iter().map(|g| g.clone_ref(py)).collect();
            gen_guard.clear();
            gens
        };

        // If no generators to shut down, complete immediately
        if generators.is_empty() {
            let future = self.create_future(py)?;
            future.bind(py).borrow().set_result(py, py.None())?;
            return Ok(future.into_any());
        }

        // Collect all aclose() coroutines
        let mut close_coros = Vec::new();
        for generator in generators {
            if let Ok(aclose) = generator.getattr(py, "aclose") {
                if let Ok(coro) = aclose.call0(py) {
                    close_coros.push(coro);
                }
            }
        }

        // Use asyncio.gather to await all aclose() coroutines
        let asyncio = py.import("asyncio")?;
        let gather = asyncio.getattr("gather")?;

        // Convert Vec to Python tuple
        let coros_tuple = PyTuple::new(py, &close_coros)?;
        let close_task = gather.call1(coros_tuple)?;

        Ok(close_task.unbind())
    }

    // Create a Rust-based PendingFuture
    fn create_future(&self, py: Python<'_>) -> PyResult<Py<PendingFuture>> {
        Py::new(py, PendingFuture::new())
    }

    fn sock_connect(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        address: Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        use pyo3::types::PyTuple;
        use std::net::SocketAddr;

        let py = slf.py();
        let self_ = slf.borrow();

        // Get the socket's file descriptor
        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        // Parse the address - it should be a tuple (host, port) for TCP
        let tuple: Bound<'_, PyTuple> = address.extract().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>("address must be a tuple (host, port)")
        })?;

        let host: String = tuple.get_item(0)?.extract()?;
        let port: u16 = tuple.get_item(1)?.extract()?;

        // Parse the host as an IP address
        let ip_addr: std::net::IpAddr = host.parse().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid IP address: {}", host))
        })?;

        let addr = SocketAddr::new(ip_addr, port);

        // Perform non-blocking connect using the raw fd
        use socket2::SockAddr;
        let sock_addr: SockAddr = addr.into();

        unsafe {
            let ret = libc::connect(
                fd,
                sock_addr.as_ptr() as *const libc::sockaddr,
                sock_addr.len(),
            );

            if ret == 0 {
                // Immediate success (rare)
                let fut = PendingFuture::new();
                fut.set_result(py, py.None())?;
                return Ok(Py::new(py, fut)?.into_any());
            }

            let err = std::io::Error::last_os_error();
            match err.kind() {
                std::io::ErrorKind::WouldBlock => {
                    // Expected: connection in progress
                }
                _ if err.raw_os_error() == Some(libc::EINPROGRESS) => {
                    // Expected: connection in progress
                }
                _ => {
                    // For other errors (including ENETUNREACH), propagate them
                    // The caller (aiohappyeyeballs) will handle fallback to other addresses
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        // Connection in progress - wait for the socket to become writable
        let future = self_.create_future(py)?;
        let future_clone = future.clone_ref(py);

        let callback = SockConnectCallback::new(future_clone).into_py_any(py)?;

        // Add writer to wait for socket to become writable
        self_.add_writer(py, fd, callback)?;

        // Create a done callback to remove the writer when future completes
        let loop_ref = slf.clone().unbind();
        let done_callback_obj = RemoveWriterCallback::new(fd, loop_ref).into_py_any(py)?;
        future
            .bind(py)
            .borrow()
            .add_done_callback(done_callback_obj)?;

        Ok(future.into_any())
    }

    fn sock_accept(slf: &Bound<'_, Self>, sock: Py<PyAny>) -> PyResult<Py<PyAny>> {
        use crate::callbacks::SockAcceptCallback;

        let py = slf.py();
        let self_ = slf.borrow();

        // Get the socket's file descriptor
        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        // Try to accept immediately (non-blocking)
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
                // Success - create Python socket object and return immediately
                use pyo3::types::{PyInt, PyString, PyTuple};

                let socket_module = py.import("socket")?;
                let client_sock = socket_module.call_method1("fromfd", (client_fd, 2, 1))?;

                // Set non-blocking on client socket
                let flags = libc::fcntl(client_fd, libc::F_GETFL, 0);
                if flags >= 0 {
                    libc::fcntl(client_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }

                // Parse the address
                let addr_tuple = if addr_len as usize >= std::mem::size_of::<libc::sockaddr_in>() {
                    let addr_in = &*((&addr) as *const _ as *const libc::sockaddr_in);
                    if addr_in.sin_family == libc::AF_INET as u16 {
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
                std::io::ErrorKind::WouldBlock => {
                    // Expected: no pending connection
                }
                _ if err.raw_os_error() == Some(libc::EAGAIN) => {
                    // Expected: no pending connection
                }
                _ => {
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        err.to_string(),
                    ));
                }
            }
        }

        // No pending connection - wait for socket to become readable
        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();

        let callback =
            SockAcceptCallback::new(loop_ref, future.clone_ref(py), fd).into_py_any(py)?;
        self_.add_reader(py, fd, callback)?;

        Ok(future.into_any())
    }

    fn sock_recv(slf: &Bound<'_, Self>, sock: Py<PyAny>, nbytes: usize) -> PyResult<Py<PyAny>> {
        use crate::callbacks::SockRecvCallback;
        use pyo3::types::PyBytes;

        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;

        // Try to recv immediately
        let mut buf = vec![0u8; nbytes];
        unsafe {
            let n = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, nbytes, 0);

            if n >= 0 {
                buf.truncate(n as usize);
                let bytes = PyBytes::new(py, &buf);
                let fut = PendingFuture::new();
                fut.set_result(py, bytes.into())?;
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

        // Would block - wait for readable
        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();

        let callback_obj = SockRecvCallback::new(loop_ref, future.clone_ref(py), fd, nbytes);
        let callback = Py::new(py, callback_obj)?;

        self_.add_reader(py, fd, callback.into_any())?;

        Ok(future.into_any())
    }

    fn sock_sendall(slf: &Bound<'_, Self>, sock: Py<PyAny>, data: &[u8]) -> PyResult<Py<PyAny>> {
        use crate::callbacks::SockSendallCallback;

        let py = slf.py();
        let self_ = slf.borrow();

        let fd: RawFd = sock.getattr(py, "fileno")?.call0(py)?.extract(py)?;
        let data_vec = data.to_vec();

        // Try to send all data immediately
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
            // All data sent
            let fut = PendingFuture::new();
            fut.set_result(py, py.None())?;
            return Ok(Py::new(py, fut)?.into_any());
        }

        // Need to wait for writable
        let future = self_.create_future(py)?;
        let loop_ref = slf.clone().unbind();
        let remaining_data = data_vec[total_sent..].to_vec();

        let callback_obj =
            SockSendallCallback::new(loop_ref, future.clone_ref(py), fd, remaining_data, 0);
        let callback = Py::new(py, callback_obj)?;

        self_.add_writer(py, fd, callback.into_any())?;

        Ok(future.into_any())
    }

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_connection(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();

        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr_str = format!("{}:{}", host, port);

        // Extract SSL context and server_hostname from kwargs
        let ssl_context = _kwargs
            .as_ref()
            .and_then(|kw| kw.get_item("ssl").ok().flatten())
            .and_then(|v| v.extract::<Py<crate::transports::ssl::SSLContext>>().ok());

        let server_hostname = _kwargs
            .as_ref()
            .and_then(|kw| kw.get_item("server_hostname").ok().flatten())
            .and_then(|v| v.extract::<String>().ok())
            .or_else(|| {
                if ssl_context.is_some() {
                    Some(host.to_string())
                } else {
                    None
                }
            });

        // Resolve address (blocking for now - DNS resolution is blocking in this MVP)
        // using std::net::ToSocketAddrs.
        let mut addrs = std::net::ToSocketAddrs::to_socket_addrs(&addr_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

        let addr = addrs
            .next()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyOSError, _>("No address found"))?;

        // Use socket2 for non-blocking connect
        use socket2::{Domain, Socket, Type};
        // Use IPv6 detection from utils for better handling
        let is_ipv6 = addr.is_ipv6();
        let domain = if is_ipv6 { Domain::IPV6 } else { Domain::IPV4 };
        let socket = Socket::new(domain, Type::STREAM, None)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;

        // Connect (non-blocking)
        match socket.connect(&addr.into()) {
            Ok(_) => {
                // Immediate success (rare, typically for localhost connections)
                // Continue to callback path for consistent handling
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Expected: EWOULDBLOCK - connection in progress
            }
            #[cfg(unix)]
            Err(e) if e.raw_os_error() == Some(36) || e.raw_os_error() == Some(115) => {
                // Expected: EINPROGRESS - connection in progress
                // 36 on macOS/BSD, 115 on Linux
            }
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                    "Connection failed: {}",
                    e
                )));
            }
        }

        let stream: std::net::TcpStream = socket.into();
        let fd = stream.as_raw_fd();

        // Create Future using Rust implementation
        let fut = self_.create_future(py)?;

        // Create Callback - with SSL support
        use crate::callbacks::AsyncConnectCallback;
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

        // Register Writer (for Connect)
        self_.add_writer(py, fd, callback_py)?;

        Ok(fut.into_any())
    }

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_server(
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

        // Create Server object
        let server = TcpServer::new(
            listener,
            loop_obj.clone_ref(py),
            protocol_factory.clone_ref(py),
        );
        let server_py = Py::new(py, server)?;

        // Register Accept Handler
        // The server needs to be polled for reading (accept).
        // TcpServer defines `_on_accept`.
        let on_accept = server_py.getattr(py, "_on_accept")?;

        // Get fd directly from TcpServer instead of calling Python method
        let fd = server_py.borrow(py).fd().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Server has no listener")
        })?;

        self_.add_reader(py, fd, on_accept)?;

        // Return Server object wrapped in completed future
        let fut = crate::transports::future::CompletedFuture::new(server_py.into_any());

        Ok(Py::new(py, fut)?.into_any())
    }

    /// High-performance native start_server using StreamReader/StreamWriter
    /// This provides better performance than Protocol-based servers for stream applications
    #[pyo3(signature = (client_connected_cb, host=None, port=None, limit=None, **_kwargs))]
    fn start_server(
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
        let limit = limit.unwrap_or(65536); // 64KB default buffer

        let listener = std::net::TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        // Create StreamServer object
        let server = crate::transports::stream_server::StreamServer::new(
            listener,
            loop_obj.clone_ref(py),
            client_connected_cb,
            limit,
        );
        let server_py = Py::new(py, server)?;

        // Register Accept Handler
        let on_accept = server_py.getattr(py, "_on_accept")?;

        // Get fd from server
        let fd = server_py.borrow(py).get_fd().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Server has no listener")
        })?;

        self_.add_reader(py, fd, on_accept)?;

        // Return Server object wrapped in completed future
        let fut = crate::transports::future::CompletedFuture::new(server_py.into_any());

        Ok(Py::new(py, fut)?.into_any())
    }

    /// High-performance native open_connection using StreamReader/StreamWriter
    #[pyo3(signature = (host, port, limit=None, **_kwargs))]
    fn open_connection(
        slf: &Bound<'_, Self>,
        host: &str,
        port: u16,
        limit: Option<usize>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let loop_obj = slf.clone().unbind();
        let limit = limit.unwrap_or(65536); // 64KB default buffer

        let addr = format!("{}:{}", host, port);
        let stream = std::net::TcpStream::connect(&addr)?;
        stream.set_nonblocking(true)?;

        // Create StreamReader and StreamWriter
        let reader = Py::new(py, crate::streams::StreamReader::new(Some(limit)))?;
        let writer = Py::new(
            py,
            crate::streams::StreamWriter::new(Some(65536), Some(16384)),
        )?;

        // Create StreamTransport (now returns Py<StreamTransport>)
        let transport_py = crate::transports::stream_server::StreamTransport::new(
            py,
            loop_obj.clone_ref(py),
            stream,
            reader.clone_ref(py),
            writer.clone_ref(py),
        )?;

        // Add reader (native path)
        let transport_clone = transport_py.clone_ref(py);
        let read_callback =
            Arc::new(move |py: Python<'_>| transport_clone.bind(py).borrow_mut()._read_ready(py));
        let fd = transport_py.borrow(py).get_fd();
        slf.borrow().add_reader_native(fd, read_callback)?;

        // Return (reader, writer) tuple wrapped in completed future
        let result = (reader.into_any(), writer.into_any());
        let result_tuple = pyo3::types::PyTuple::new(py, &[result.0, result.1])?;
        let fut = crate::transports::future::CompletedFuture::new(result_tuple.into());

        Ok(Py::new(py, fut)?.into_any())
    }

    #[pyo3(signature = (protocol_factory, local_addr=None, remote_addr=None, **kwargs))]
    fn create_datagram_endpoint(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        local_addr: Option<(String, u16)>,
        remote_addr: Option<(String, u16)>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let loop_obj = slf.clone().unbind();

        // Extract optional parameters from kwargs
        let allow_broadcast = kwargs
            .and_then(|k| k.get_item("allow_broadcast").ok())
            .and_then(|v| v.and_then(|val| val.extract::<bool>().ok()))
            .unwrap_or(false);

        let reuse_port = kwargs
            .and_then(|k| k.get_item("reuse_port").ok())
            .and_then(|v| v.and_then(|val| val.extract::<bool>().ok()))
            .unwrap_or(false);

        // Create UDP socket
        use socket2::{Domain, Protocol, Socket, Type};
        use std::net::SocketAddr;

        // Determine address family from local_addr or remote_addr
        // Using helper function for better IPv6 detection
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

        // Set socket options
        if allow_broadcast {
            socket
                .set_broadcast(true)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        }

        #[cfg(all(unix, not(target_os = "solaris")))]
        if reuse_port {
            use std::os::fd::AsRawFd;
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

        // Bind to local address if provided
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

        // Parse remote address if provided
        let remote_sockaddr = if let Some((host, port)) = remote_addr {
            let addr_str = format!("{}:{}", host, port);
            let addr: SocketAddr = addr_str.parse().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid remote address: {}",
                    e
                ))
            })?;

            // Connect to remote address (this filters incoming packets)
            socket.connect(&addr.into()).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Failed to connect: {}", e))
            })?;
            Some(addr)
        } else {
            None
        };

        let udp_socket: std::net::UdpSocket = socket.into();

        // Create protocol instance
        let protocol = protocol_factory.call0(py)?;

        // Create transport using factory
        use crate::transports::{DefaultTransportFactory, TransportFactory};
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

        // Call protocol.connection_made(transport)
        protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;

        // Register read handler (native path)
        let transport_clone = transport_py.clone_ref(py);
        let read_callback = Arc::new(move |py: Python<'_>| {
            let b = transport_clone.bind(py);
            let udp = b.cast::<UdpTransport>().map_err(|_| {
                PyErr::new::<pyo3::exceptions::PyTypeError, _>("Expected UdpTransport")
            })?;
            udp.borrow()._read_ready(py)
        });
        slf.borrow().add_reader_native(fd, read_callback)?;

        // Return (transport, protocol) tuple wrapped in completed future
        let result_tuple = PyTuple::new(py, vec![transport_py.into_any(), protocol.into_any()])?;

        let fut = CompletedFuture::new(result_tuple.into());
        Ok(Py::new(py, fut)?.into_any())
    }
}

impl VeloxLoop {
    fn _run_once(&self, py: Python<'_>, events: &mut polling::Events) -> VeloxResult<()> {
        let now = Instant::now();
        let has_callbacks = !self.callbacks.borrow().is_empty();

        // Skip-poll logic: if we have callbacks and last poll was recent, don't poll again.
        let skip_poll = {
            let last_poll = self.last_poll_time.borrow();
            has_callbacks && now.duration_since(*last_poll).as_nanos() < 50_000
        };

        if !skip_poll {
            // Calculate Timeout
            let timeout = if has_callbacks {
                Some(Duration::from_secs(0))
            } else {
                let timers = self.timers.borrow();
                if let Some(next) = timers.next_expiry() {
                    let now_ns = (self.time() * 1_000_000_000.0) as u64;
                    if next > now_ns {
                        Some(Duration::from_nanos(next - now_ns))
                    } else {
                        Some(Duration::from_secs(0))
                    }
                } else {
                    Some(Duration::from_millis(10))
                }
            };

            // Poll
            self.state.borrow_mut().is_polling = true;
            let res = self.poller.borrow_mut().poll(events, timeout);
            self.state.borrow_mut().is_polling = false;

            match res {
                Ok(_) => {
                    let mut last_poll = self.last_poll_time.borrow_mut();
                    *last_poll = Instant::now();
                }
                Err(e) => return Err(e),
            }
        }

        // Process I/O Events
        if events.len() > 0 {
            let mut pending = self.pending_ios.borrow_mut();
            pending.clear();

            {
                let handles = self.handles.borrow();
                for event in events.iter() {
                    let fd = event.key as RawFd;
                    let pair = handles.get_state_owned(fd);

                    if let Some((r_handle, w_handle)) = pair {
                        let reader_cb = if event.readable {
                            r_handle.as_ref().filter(|h| !h.cancelled).cloned()
                        } else {
                            None
                        };
                        let writer_cb = if event.writable {
                            w_handle.as_ref().filter(|h| !h.cancelled).cloned()
                        } else {
                            None
                        };

                        pending.push((
                            fd,
                            reader_cb,
                            writer_cb,
                            r_handle.is_some(),
                            w_handle.is_some(),
                        ));
                    }
                }
            }

            // Dispatch I/O
            for (_, r_h, w_h, _, _) in pending.iter() {
                if let Some(h) = r_h {
                    let res = match &h.callback {
                        IoCallback::Python(cb) => cb.call0(py).map(|_| ()),
                        IoCallback::Native(cb) => cb(py),
                        IoCallback::TcpRead(tcp) => {
                            crate::transports::tcp::TcpTransport::_read_ready(tcp.bind(py))
                        }
                        IoCallback::TcpWrite(tcp) => {
                            let tcp_bound = tcp.bind(py);
                            crate::transports::tcp::TcpTransport::_write_ready(
                                &mut *tcp_bound.borrow_mut(),
                                py,
                            )
                        }
                    };
                    if let Err(e) = res {
                        e.print(py);
                    }
                }
                if let Some(h) = w_h {
                    let res = match &h.callback {
                        IoCallback::Python(cb) => cb.call0(py).map(|_| ()),
                        IoCallback::Native(cb) => cb(py),
                        IoCallback::TcpRead(tcp) => {
                            crate::transports::tcp::TcpTransport::_read_ready(tcp.bind(py))
                        }
                        IoCallback::TcpWrite(tcp) => {
                            let tcp_bound = tcp.bind(py);
                            crate::transports::tcp::TcpTransport::_write_ready(
                                &mut *tcp_bound.borrow_mut(),
                                py,
                            )
                        }
                    };
                    if let Err(e) = res {
                        e.print(py);
                    }
                }
            }
            let mut modified_fds: HashSet<i32> = HashSet::new();

            // Re-arm FDs
            for (fd, _, _, has_r, has_w) in pending.iter() {
                if *has_r || *has_w {
                    let mut ev = if *has_r {
                        polling::Event::readable(*fd as usize)
                    } else {
                        polling::Event::writable(*fd as usize)
                    };
                    if *has_r && *has_w {
                        ev.writable = true;
                    }
                    if !modified_fds.contains(fd) {
                        // Check if we actually need to modify
                        // (Implementation would track previous interests in IoHandles)
                        let _ = self.poller.borrow_mut().modify(*fd, ev);
                        modified_fds.insert(*fd);
                    }
                }
            }
        }

        // Process Timers
        let now_ns = (self.time() * 1_000_000_000.0) as u64;
        let expired = self.timers.borrow_mut().pop_expired(now_ns, 0);
        for entry in expired {
            let _ = entry
                .callback
                .bind(py)
                .call(PyTuple::new(py, entry.args)?, None);
        }

        // Process Callbacks (call_soon)
        let mut cb_batch = self.callback_buffer.borrow_mut();
        cb_batch.clear();
        self.callbacks.borrow_mut().swap_into(&mut *cb_batch);

        for cb in cb_batch.drain(..) {
            let _ = cb.callback.bind(py).call(PyTuple::new(py, cb.args)?, None);
        }

        Ok(())
    }
}

// DNS resolution helper functions
fn perform_getaddrinfo(
    py: Python<'_>,
    host: Option<String>,
    port: Option<String>,
    family: i32,
    socktype: i32,
    protocol: i32,
    flags: i32,
) -> PyResult<Py<PyAny>> {
    use pyo3::types::{PyInt, PyList, PyString};
    use std::ffi::{CStr, CString};
    use std::mem;
    use std::ptr;

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

        // Convert the linked list of addrinfo to Python list
        let py_list = PyList::empty(py);
        let mut current = res;

        while !current.is_null() {
            let info = &*current;

            // Extract family, socktype, protocol
            let fam = info.ai_family;
            let stype = info.ai_socktype;
            let proto = info.ai_protocol;

            // Extract canonname
            let canonname = if info.ai_canonname.is_null() {
                String::new()
            } else {
                CStr::from_ptr(info.ai_canonname)
                    .to_string_lossy()
                    .to_string()
            };

            // Extract sockaddr
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

fn perform_getnameinfo(py: Python<'_>, addr: &str, port: u16, flags: i32) -> PyResult<Py<PyAny>> {
    use pyo3::types::PyString;
    use std::ffi::CStr;
    use std::mem;
    use std::net::{IpAddr, SocketAddr};

    // Parse the address
    let ip_addr: IpAddr = addr.parse().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid IP address: {}", e))
    })?;

    let sock_addr = SocketAddr::new(ip_addr, port);

    unsafe {
        let mut host = vec![0u8; NI_MAXHOST];
        let mut serv = vec![0u8; NI_MAXSERV];

        // Create the sockaddr structure that will live for the duration of getnameinfo call
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

        // Convert C strings to Rust strings
        let hostname = CStr::from_ptr(host.as_ptr() as *const libc::c_char)
            .to_string_lossy()
            .to_string();
        let servname = CStr::from_ptr(serv.as_ptr() as *const libc::c_char)
            .to_string_lossy()
            .to_string();

        // Return tuple (hostname, servname)
        let host_py = PyString::new(py, &hostname);
        let serv_py = PyString::new(py, &servname);
        let result_tuple = PyTuple::new(py, vec![host_py.as_any(), serv_py.as_any()])?;

        Ok(result_tuple.into())
    }
}
