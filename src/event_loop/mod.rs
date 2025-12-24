use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::os::fd::RawFd;
use std::time::Instant;

use crate::callbacks::{Callback, CallbackQueue};
use crate::executor::ThreadPoolExecutor;
use crate::handles::{Handle, IoHandles};
use crate::poller::LoopPoller;
use crate::timers::Timers;
use crate::transports::future::PendingFuture;
use crate::utils::VeloxResult;

mod callbacks;
mod executor;
mod io;
mod lifecycle;
mod network;
mod poll;

/// Atomic state flags for lock-free state checking in hot paths.
/// These replace the RefCell<HotState> booleans for frequently checked state.
/// Using atomics eliminates RefCell borrow overhead in the critical event loop.
/// Now uses AtomicFlag from the concurrent module for lock-free operations.
pub struct AtomicState {
    pub running: crate::concurrent::AtomicFlag,
    pub stopped: crate::concurrent::AtomicFlag,
    pub closed: crate::concurrent::AtomicFlag,
    pub is_polling: crate::concurrent::AtomicFlag,
}

impl AtomicState {
    pub fn new() -> Self {
        Self {
            running: crate::concurrent::AtomicFlag::new(false),
            stopped: crate::concurrent::AtomicFlag::new(false),
            closed: crate::concurrent::AtomicFlag::new(false),
            is_polling: crate::concurrent::AtomicFlag::new(false),
        }
    }

    #[inline(always)]
    pub fn is_running(&self) -> bool {
        self.running.is_set()
    }

    #[inline(always)]
    pub fn set_running(&self, val: bool) {
        if val { self.running.set(); } else { self.running.clear(); }
    }

    #[inline(always)]
    pub fn is_stopped(&self) -> bool {
        self.stopped.is_set()
    }

    #[inline(always)]
    pub fn set_stopped(&self, val: bool) {
        if val { self.stopped.set(); } else { self.stopped.clear(); }
    }

    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        self.closed.is_set()
    }

    #[inline(always)]
    pub fn set_closed(&self, val: bool) {
        if val { self.closed.set(); } else { self.closed.clear(); }
    }

    #[inline(always)]
    pub fn is_polling(&self) -> bool {
        self.is_polling.is_set()
    }

    #[inline(always)]
    pub fn set_polling(&self, val: bool) {
        if val { self.is_polling.set(); } else { self.is_polling.clear(); }
    }
}

/// Fast-path state for the event loop (non-atomic, RefCell-protected)
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
    pub(crate) poller: RefCell<LoopPoller>,
    pub(crate) handles: RefCell<IoHandles>,
    pub(crate) callbacks: RefCell<CallbackQueue>,
    pub(crate) timers: RefCell<Timers>,
    pub(crate) state: RefCell<HotState>,
    /// Atomic state for lock-free hot path checks (duplicates key state vars)
    pub(crate) atomic_state: AtomicState,
    pub(crate) start_time: Instant,
    pub(crate) executor: RefCell<Option<ThreadPoolExecutor>>,
    pub(crate) exception_handler: RefCell<Option<Py<PyAny>>>,
    pub(crate) task_factory: RefCell<Option<Py<PyAny>>>,
    pub(crate) async_generators: RefCell<Vec<Py<PyAny>>>,
    pub(crate) callback_buffer: RefCell<Vec<Callback>>,
    pub(crate) pending_ios: RefCell<Vec<(RawFd, Option<Handle>, Option<Handle>, bool, bool)>>,
    /// Track FDs registered with EPOLLONESHOT that are currently disabled (fired once)
    #[cfg(target_os = "linux")]
    pub(crate) oneshot_disabled: RefCell<FxHashSet<RawFd>>,
    /// Atomic counter for tracking I/O operations (lock-free)
    pub(crate) io_op_counter: crate::concurrent::AtomicCounter,
}

unsafe impl Send for VeloxLoop {}
unsafe impl Sync for VeloxLoop {}

impl VeloxLoop {
    pub fn time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    /// Get the current I/O operation count (lock-free)
    pub fn io_operations(&self) -> u64 {
        self.io_op_counter.get()
    }

    /// Increment I/O operation counter (lock-free)
    #[inline]
    pub(crate) fn track_io_operation(&self) -> u64 {
        self.io_op_counter.increment()
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
            atomic_state: AtomicState::new(),
            start_time: Instant::now(),
            executor: RefCell::new(None),
            exception_handler: RefCell::new(None),
            task_factory: RefCell::new(None),
            async_generators: RefCell::new(Vec::new()),
            callback_buffer: RefCell::new(Vec::with_capacity(1024)),
            pending_ios: RefCell::new(Vec::with_capacity(128)),
            #[cfg(target_os = "linux")]
            oneshot_disabled: RefCell::new(FxHashSet::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            io_op_counter: crate::concurrent::AtomicCounter::new(0),
        })
    }

    #[pyo3(name = "time")]
    pub fn py_time(&self) -> f64 {
        self.time()
    }

    // Lifecycle methods
    #[pyo3(name = "run_forever")]
    pub fn py_run_forever(&self, py: Python<'_>) -> PyResult<()> {
        self.run_forever(py).map_err(|e| e.into())
    }

    #[pyo3(name = "_run_once")]
    pub fn py_run_once(&self, py: Python<'_>) -> PyResult<()> {
        let mut events = poll::PlatformEvents::new();
        self._run_once(py, &mut events).map_err(|e| e.into())
    }

    #[pyo3(name = "stop")]
    pub fn py_stop(&self) {
        self.stop()
    }

    #[pyo3(name = "close")]
    pub fn py_close(&self) {
        self.close()
    }

    #[pyo3(name = "is_running")]
    pub fn py_is_running(&self) -> bool {
        self.is_running()
    }

    #[pyo3(name = "is_closed")]
    pub fn py_is_closed(&self) -> bool {
        self.is_closed()
    }

    #[pyo3(name = "get_debug")]
    pub fn py_get_debug(&self) -> bool {
        self.get_debug()
    }

    #[pyo3(name = "set_debug")]
    pub fn py_set_debug(&self, enabled: bool) {
        self.set_debug(enabled)
    }

    /// Get the number of I/O operations tracked by this event loop
    #[pyo3(name = "io_operations")]
    pub fn py_io_operations(&self) -> u64 {
        self.io_operations()
    }

    // I/O methods
    #[pyo3(name = "add_reader", signature = (fd, callback))]
    pub fn py_add_reader(&self, py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        self.add_reader(py, fd, callback)
    }

    #[pyo3(name = "remove_reader")]
    pub fn py_remove_reader(&self, py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        self.remove_reader(py, fd)
    }

    #[pyo3(name = "add_writer", signature = (fd, callback))]
    pub fn py_add_writer(&self, py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        self.add_writer(py, fd, callback)
    }

    #[pyo3(name = "remove_writer")]
    pub fn py_remove_writer(&self, py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        self.remove_writer(py, fd)
    }

    // Callback/Timer methods
    #[pyo3(name = "call_soon", signature = (callback, *args, context=None))]
    pub fn py_call_soon(
        &self,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) {
        self.call_soon(callback, args, context)
    }

    #[pyo3(name = "call_soon_threadsafe", signature = (callback, *args, context=None))]
    pub fn py_call_soon_threadsafe(
        &self,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) {
        self.call_soon_threadsafe(callback, args, context)
    }

    #[pyo3(name = "call_later", signature = (delay, callback, *args, context=None))]
    pub fn py_call_later(
        &self,
        delay: f64,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) -> u64 {
        self.call_later(delay, callback, args, context)
    }

    #[pyo3(name = "call_at", signature = (when, callback, *args, context=None))]
    pub fn py_call_at(
        &self,
        when: f64,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) -> u64 {
        self.call_at(when, callback, args, context)
    }

    #[pyo3(name = "_cancel_timer")]
    pub fn py_cancel_timer(&self, timer_id: u64) {
        self._cancel_timer(timer_id)
    }

    #[pyo3(name = "create_future")]
    pub fn py_create_future(&self, py: Python<'_>) -> PyResult<Py<PendingFuture>> {
        self.create_future(py)
    }

    // Network methods
    #[pyo3(name = "sock_connect")]
    pub fn py_sock_connect(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        address: Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        Self::sock_connect(slf, sock, address)
    }

    #[pyo3(name = "sock_accept")]
    pub fn py_sock_accept(slf: &Bound<'_, Self>, sock: Py<PyAny>) -> PyResult<Py<PyAny>> {
        Self::sock_accept(slf, sock)
    }

    #[pyo3(name = "sock_recv")]
    pub fn py_sock_recv(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        nbytes: usize,
    ) -> PyResult<Py<PyAny>> {
        Self::sock_recv(slf, sock, nbytes)
    }

    #[pyo3(name = "sendfile", signature = (transport, file, offset=0, count=None, *, _fallback=true))]
    pub fn py_sendfile(
        slf: &Bound<'_, Self>,
        transport: Py<PyAny>,
        file: Py<PyAny>,
        offset: i64,
        count: Option<usize>,
        _fallback: bool,
    ) -> PyResult<Py<PyAny>> {
        Self::sendfile(slf, transport, file, offset, count, _fallback)
    }

    #[pyo3(name = "sock_sendall")]
    pub fn py_sock_sendall(
        slf: &Bound<'_, Self>,
        sock: Py<PyAny>,
        data: &[u8],
    ) -> PyResult<Py<PyAny>> {
        Self::sock_sendall(slf, sock, data)
    }

    #[pyo3(name = "create_connection", signature = (protocol_factory, host=None, port=None, **_kwargs))]
    pub fn py_create_connection(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Self::create_connection(slf, protocol_factory, host, port, _kwargs)
    }

    #[pyo3(name = "create_server", signature = (protocol_factory, host=None, port=None, **_kwargs))]
    pub fn py_create_server(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Self::create_server(slf, protocol_factory, host, port, _kwargs)
    }

    #[pyo3(name = "start_server", signature = (client_connected_cb, host=None, port=None, limit=None, **_kwargs))]
    pub fn py_start_server(
        slf: &Bound<'_, Self>,
        client_connected_cb: Py<PyAny>,
        host: Option<&str>,
        port: Option<u16>,
        limit: Option<usize>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Self::start_server(slf, client_connected_cb, host, port, limit, _kwargs)
    }

    #[pyo3(name = "open_connection", signature = (host, port, limit=None, **_kwargs))]
    pub fn py_open_connection(
        slf: &Bound<'_, Self>,
        host: &str,
        port: u16,
        limit: Option<usize>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Self::open_connection(slf, host, port, limit, _kwargs)
    }

    #[pyo3(name = "create_datagram_endpoint", signature = (protocol_factory, local_addr=None, remote_addr=None, **kwargs))]
    pub fn py_create_datagram_endpoint(
        slf: &Bound<'_, Self>,
        protocol_factory: Py<PyAny>,
        local_addr: Option<(String, u16)>,
        remote_addr: Option<(String, u16)>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Self::create_datagram_endpoint(slf, protocol_factory, local_addr, remote_addr, kwargs)
    }

    // Executor methods
    #[pyo3(name = "run_in_executor", signature = (_executor, func, *args))]
    pub fn py_run_in_executor(
        &self,
        py: Python<'_>,
        _executor: Option<Py<PyAny>>,
        func: Py<PyAny>,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        self.run_in_executor(py, _executor, func, args)
    }

    #[pyo3(name = "set_default_executor")]
    pub fn py_set_default_executor(&self, _executor: Option<Py<PyAny>>) -> PyResult<()> {
        self.set_default_executor(_executor)
    }

    #[pyo3(name = "getaddrinfo", signature = (host, port, *, family=0, r#type=0, proto=0, flags=0))]
    pub fn py_getaddrinfo(
        &self,
        py: Python<'_>,
        host: Option<Bound<'_, PyAny>>,
        port: Option<Bound<'_, PyAny>>,
        family: i32,
        r#type: i32,
        proto: i32,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        self.getaddrinfo(py, host, port, family, r#type, proto, flags)
    }

    #[pyo3(name = "getnameinfo", signature = (sockaddr, flags=0))]
    pub fn py_getnameinfo(
        &self,
        py: Python<'_>,
        sockaddr: Bound<'_, PyTuple>,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        self.getnameinfo(py, sockaddr, flags)
    }

    // Exception handler methods
    #[pyo3(name = "set_exception_handler")]
    pub fn py_set_exception_handler(&self, handler: Option<Py<PyAny>>) {
        self.set_exception_handler(handler)
    }

    #[pyo3(name = "get_exception_handler")]
    pub fn py_get_exception_handler(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.get_exception_handler(py)
    }

    #[pyo3(name = "default_exception_handler")]
    pub fn py_default_exception_handler(
        &self,
        py: Python<'_>,
        context: Py<PyDict>,
    ) -> PyResult<()> {
        self.default_exception_handler(py, context)
    }

    #[pyo3(name = "call_exception_handler")]
    pub fn py_call_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
        self.call_exception_handler(py, context)
    }

    // Task factory methods
    #[pyo3(name = "set_task_factory")]
    pub fn py_set_task_factory(&self, factory: Option<Py<PyAny>>) {
        self.set_task_factory(factory)
    }

    #[pyo3(name = "get_task_factory")]
    pub fn py_get_task_factory(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.get_task_factory(py)
    }

    // Async generator methods
    #[pyo3(name = "_track_async_generator")]
    pub fn py_track_async_generator(&self, agen: Py<PyAny>) {
        self._track_async_generator(agen)
    }

    #[pyo3(name = "_untrack_async_generator")]
    pub fn py_untrack_async_generator(&self, py: Python<'_>, agen: Py<PyAny>) {
        self._untrack_async_generator(py, agen)
    }

    #[pyo3(name = "shutdown_asyncgens")]
    pub fn py_shutdown_asyncgens(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.shutdown_asyncgens(py)
    }

    /// Get the number of active tasks in the executor
    #[pyo3(name = "get_executor_active_tasks")]
    pub fn py_get_executor_active_tasks(&self) -> usize {
        if let Some(executor) = self.executor.borrow().as_ref() {
            executor.active_tasks()
        } else {
            0
        }
    }

    /// Get the number of worker threads in the executor
    #[pyo3(name = "get_executor_num_workers")]
    pub fn py_get_executor_num_workers(&self) -> usize {
        if let Some(executor) = self.executor.borrow().as_ref() {
            executor.num_workers()
        } else {
            0
        }
    }
}
