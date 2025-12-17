use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Instant, Duration};

use crate::poller::LoopPoller;
use crate::handles::IoHandles;
use crate::callbacks::{CallbackQueue, Callback};
use crate::timers::Timers;
use crate::utils::{VeloxResult, VeloxError};
use crate::transports::tcp::TcpServer;
use crate::transports::future::PendingFuture;
use crate::executor::ThreadPoolExecutor;
use std::os::fd::{AsRawFd, RawFd};

use std::sync::atomic::{AtomicBool, Ordering};

#[pyclass(subclass, module = "veloxloop._veloxloop")]
pub struct VeloxLoop {
    poller: Arc<LoopPoller>, // Arc to share with callbacks
    handles: Mutex<IoHandles>,
    callbacks: Arc<CallbackQueue>,
    timers: Mutex<Timers>,
    running: AtomicBool,
    stopped: AtomicBool,
    closed: AtomicBool,
    debug: AtomicBool,
    start_time: Instant,
    executor: Arc<Mutex<Option<ThreadPoolExecutor>>>,
    exception_handler: Arc<Mutex<Option<Py<PyAny>>>>,
    task_factory: Arc<Mutex<Option<Py<PyAny>>>>,
    async_generators: Arc<Mutex<Vec<Py<PyAny>>>>,
}

#[pymethods]
impl VeloxLoop {
    #[new]
    #[pyo3(signature = (debug=None))]
    pub fn new(debug: Option<bool>) -> VeloxResult<Self> {
        let poller = Arc::new(LoopPoller::new()?);
        let debug_val = debug.unwrap_or(false);

        Ok(Self {
            poller: poller.clone(),
            handles: Mutex::new(IoHandles::new()),
            callbacks: Arc::new(CallbackQueue::new(poller)),
            timers: Mutex::new(Timers::new()),
            running: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            debug: AtomicBool::new(debug_val),
            start_time: Instant::now(),
            executor: Arc::new(Mutex::new(None)),
            exception_handler: Arc::new(Mutex::new(None)),
            task_factory: Arc::new(Mutex::new(None)),
            async_generators: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
    
    fn run_forever(&self, py: Python<'_>) -> VeloxResult<()> {
        self.running.store(true, Ordering::SeqCst);
        self.stopped.store(false, Ordering::SeqCst);
        
        // Use polling::Events
        let mut events = polling::Events::new();
        
        while self.running.load(Ordering::SeqCst) && !self.stopped.load(Ordering::SeqCst) {
            self._run_once(py, &mut events)?;
            
            // Check if stopped after processing callbacks (future might be done)
            if self.stopped.load(Ordering::SeqCst) {
                break;
            }
            
            // Allow Python signals (Ctrl+C) to interrupt
            if let Err(e) = py.check_signals() {
                return Err(VeloxError::Python(e));
            }
        }
        
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn add_reader(&self, _py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        let poller = self.poller.clone();
        
        // Lock handles once
        let mut handles = self.handles.lock();
        let reader_exists = handles.get_reader(fd).is_some();
        let writer_exists = handles.get_writer(fd).is_some();
        
        // Add or modify
        handles.add_reader(fd, callback);
        drop(handles); // Drop lock before poller
        
        // Update Poller
        let mut ev = polling::Event::readable(fd as usize);
        if writer_exists {
            ev.writable = true; 
        }
        if !reader_exists && !writer_exists {
             poller.register(fd, ev)?;
        } else {
             poller.modify(fd, ev)?;
        }
        Ok(())
    }

    pub fn remove_reader(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        let mut handles = self.handles.lock();
        if handles.remove_reader(fd) {
            let writer_exists = handles.get_writer(fd).is_some();
            drop(handles);
            
            if writer_exists {
                // Downgrade to W only
                let ev = polling::Event::writable(fd as usize);
                self.poller.modify(fd, ev)?;
            } else {
                // Remove
                self.poller.delete(fd)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub fn add_writer(&self, _py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
        let poller = self.poller.clone();
        
        let mut handles = self.handles.lock();
        let reader_exists = handles.get_reader(fd).is_some();
        let writer_exists = handles.get_writer(fd).is_some();
        
        handles.add_writer(fd, callback);
        drop(handles);
        
        let mut ev = polling::Event::writable(fd as usize);
        if reader_exists {
            ev.readable = true;
        }
        
        if !writer_exists && !reader_exists {
            poller.register(fd, ev)?;
        } else {
            poller.modify(fd, ev)?;
        }
        Ok(())
    }

    pub fn remove_writer(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
        let mut handles = self.handles.lock();
        if handles.remove_writer(fd) {
            let reader_exists = handles.get_reader(fd).is_some();
            drop(handles);
            
            if reader_exists {
                 let ev = polling::Event::readable(fd as usize);
                 self.poller.modify(fd, ev)?;
            } else {
                 self.poller.delete(fd)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    // Callbacks
    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon(&self, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) {
        self.callbacks.push(Callback { callback, args, context });
    }
    
    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon_threadsafe(&self, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) {
        // Push naturally notifies.
        self.callbacks.push(Callback { callback, args, context });
    }
    
    #[pyo3(signature = (delay, callback, *args, context=None))]
    fn call_later(&self, delay: f64, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) -> u64 {
        let now = (self.time() * 1_000_000_000.0) as u64;
        let delay_ns = (delay * 1_000_000_000.0) as u64;
        let when = now + delay_ns;
        self.timers.lock().insert(when, callback, args, context)
    }
    
    #[pyo3(signature = (when, callback, *args, context=None))]
    fn call_at(&self, when: f64, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) -> u64 {
         let when_ns = (when * 1_000_000_000.0) as u64;
         self.timers.lock().insert(when_ns, callback, args, context)
    }

    fn _cancel_timer(&self, timer_id: u64) {
        self.timers.lock().remove(timer_id);
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    
    fn get_debug(&self) -> bool {
        let val = self.debug.load(Ordering::SeqCst);
        val
    }
    
    fn set_debug(&self, enabled: bool) {
        self.debug.store(enabled, Ordering::SeqCst);
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
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
        let mut exec_guard = self.executor.lock();
        if exec_guard.is_none() {
            *exec_guard = Some(ThreadPoolExecutor::new()?);
        }
        let executor_ref = exec_guard.as_ref().unwrap();
        
        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);
        
        // Clone the necessary objects for the spawned task
        let func_clone = func.clone_ref(py);
        let args_clone: Py<PyTuple> = args.clone().unbind();
        let callbacks = self.callbacks.clone();
        
        // Spawn the blocking task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                // Call the Python function
                let result = func_clone.call1(py, args_clone.bind(py));
                
                // Schedule callback to set the future result
                let future_ref = future_clone.clone_ref(py);
                match result {
                    Ok(val) => {
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_result").unwrap(),
                            args: vec![val],
                            context: None,
                        });
                    }
                    Err(e) => {
                        // Set exception on future
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_exception").unwrap(),
                            args: vec![exc],
                            context: None,
                        });
                    }
                }
            });
        });
        
        Ok(future.into_any())
    }

    fn set_default_executor(&self, _executor: Option<Py<PyAny>>) -> PyResult<()> {
        // For now, we only support our internal executor
        // If executor is None, we create a new default one
        let mut exec_guard = self.executor.lock();
        *exec_guard = Some(ThreadPoolExecutor::new()?);
        Ok(())
    }

    #[pyo3(signature = (host, port, family=0, type_=0, proto=0, flags=0))]
    fn getaddrinfo(
        &self,
        py: Python<'_>,
        host: Option<String>,
        port: Option<String>,
        family: i32,
        type_: i32,
        proto: i32,
        flags: i32,
    ) -> PyResult<Py<PyAny>> {
        // Get or create default executor
        let mut exec_guard = self.executor.lock();
        if exec_guard.is_none() {
            *exec_guard = Some(ThreadPoolExecutor::new()?);
        }
        let executor_ref = exec_guard.as_ref().unwrap();
        
        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);
        let callbacks = self.callbacks.clone();
        
        // Spawn the blocking DNS resolution task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                // Perform DNS resolution using libc getaddrinfo
                let result = perform_getaddrinfo(py, host, port, family, type_, proto, flags);
                
                // Schedule callback to set the future result
                let future_ref = future_clone.clone_ref(py);
                match result {
                    Ok(val) => {
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_result").unwrap(),
                            args: vec![val],
                            context: None,
                        });
                    }
                    Err(e) => {
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_exception").unwrap(),
                            args: vec![exc],
                            context: None,
                        });
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
        let mut exec_guard = self.executor.lock();
        if exec_guard.is_none() {
            *exec_guard = Some(ThreadPoolExecutor::new()?);
        }
        let executor_ref = exec_guard.as_ref().unwrap();
        
        // Extract address and port from sockaddr tuple
        let addr_str: String = sockaddr.get_item(0)?.extract()?;
        let port: u16 = sockaddr.get_item(1)?.extract()?;
        
        // Create a future to return to Python
        let future = self.create_future(py)?;
        let future_clone = future.clone_ref(py);
        let callbacks = self.callbacks.clone();
        
        // Spawn the blocking reverse DNS resolution task
        executor_ref.spawn_blocking(move || {
            let _ = Python::attach(move |py| {
                let result = perform_getnameinfo(py, &addr_str, port, flags);
                
                // Schedule callback to set the future result
                let future_ref = future_clone.clone_ref(py);
                match result {
                    Ok(val) => {
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_result").unwrap(),
                            args: vec![val],
                            context: None,
                        });
                    }
                    Err(e) => {
                        let exc: Py<PyAny> = e.value(py).clone().unbind().into();
                        callbacks.push(Callback {
                            callback: future_ref.getattr(py, "set_exception").unwrap(),
                            args: vec![exc],
                            context: None,
                        });
                    }
                }
            });
        });
        
        Ok(future.into_any())
    }

    // Exception handler methods
    fn set_exception_handler(&self, handler: Option<Py<PyAny>>) {
        let mut eh = self.exception_handler.lock();
        *eh = handler;
    }

    fn get_exception_handler(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        let eh = self.exception_handler.lock();
        eh.as_ref().map(|h| h.clone_ref(py))
    }

    fn call_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
        let handler = {
            let eh = self.exception_handler.lock();
            eh.as_ref().map(|h| h.clone_ref(py))
        };
        
        if let Some(handler) = handler {
            // Call handler with (loop, context) as per asyncio API
            // Pass None for loop and the context dict
            match handler.call(py, (py.None(), context.as_any()), None) {
                Ok(_) => {},
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
        let mut tf = self.task_factory.lock();
        *tf = factory;
    }

    fn get_task_factory(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        let tf = self.task_factory.lock();
        tf.as_ref().map(|f| f.clone_ref(py))
    }

    // Async generator tracking methods
    fn _track_async_generator(&self, agen: Py<PyAny>) {
        let mut generators = self.async_generators.lock();
        generators.push(agen);
    }

    fn _untrack_async_generator(&self, py: Python<'_>, agen: Py<PyAny>) {
        let mut generators = self.async_generators.lock();
        generators.retain(|g| !g.bind(py).is(agen.bind(py)));
    }

    fn shutdown_asyncgens(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Get all tracked async generators
        let generators = {
            let mut gen_guard = self.async_generators.lock();
            let gens: Vec<Py<PyAny>> = gen_guard.iter().map(|g| g.clone_ref(py)).collect();
            gen_guard.clear();
            gens
        };

        // If no generators to shut down, complete immediately
        if generators.is_empty() {
            let future = self.create_future(py)?;
            let set_result = future.getattr(py, "set_result")?;
            set_result.call1(py, (py.None(),))?;
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

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_connection(slf: &Bound<'_, Self>, protocol_factory: Py<PyAny>, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();
        
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr_str = format!("{}:{}", host, port);
        
        // Resolve address (blocking for now - DNS resolution is blocking in this MVP)
        // using std::net::ToSocketAddrs.
        let mut addrs = std::net::ToSocketAddrs::to_socket_addrs(&addr_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
            
        let addr = addrs.next().ok_or_else(|| PyErr::new::<pyo3::exceptions::PyOSError, _>("No address found"))?;
        
        // Use socket2 for non-blocking connect
        use socket2::{Socket, Domain, Type};
        // Use IPv6 detection from utils for better handling
        let is_ipv6 = addr.is_ipv6();
        let domain = if is_ipv6 { Domain::IPV6 } else { Domain::IPV4 };
        let socket = Socket::new(domain, Type::STREAM, None)
             .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
             
        socket.set_nonblocking(true)
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
                return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    format!("Connection failed: {}", e)
                ));
            }
        }
        
        let stream: std::net::TcpStream = socket.into();
        let fd = stream.as_raw_fd();
        
        // Create Future using Rust implementation
        let fut = self_.create_future(py)?;
        
        // Create Callback
        use crate::callbacks::AsyncConnectCallback;
        let loop_obj = slf.clone().unbind();
        let callback = AsyncConnectCallback::new(loop_obj.clone_ref(py), fut.clone_ref(py), protocol_factory, stream);
        let callback_py = Py::new(py, callback)?.into_any();
        
        // Register Writer (for Connect)
        self_.add_writer(py, fd, callback_py)?;
        
        Ok(fut.into_any())
    }    

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_server(slf: &Bound<'_, Self>, protocol_factory: Py<PyAny>, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let loop_obj = slf.clone().unbind();
        
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr = format!("{}:{}", host, port);
        
        let listener = std::net::TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;
        
        // Create Server object
        let server = TcpServer::new(listener, loop_obj.clone_ref(py), protocol_factory.clone_ref(py));
        let server_py = Py::new(py, server)?;
        
        // Register Accept Handler
        // The server needs to be polled for reading (accept).
        // TcpServer defines `_on_accept`.
        let on_accept = server_py.getattr(py, "_on_accept")?;
        
        // Get fd directly from TcpServer instead of calling Python method
        let fd = server_py.borrow(py).fd()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Server has no listener"))?;
        
        self_.add_reader(py, fd, on_accept)?;
        
        // Return Server object wrapped in completed future
        let fut = crate::transports::future::CompletedFuture::new(server_py.into_any());
        
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
        let self_ = slf.borrow();
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
        use socket2::{Socket, Domain, Type, Protocol};
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
        
        socket.set_nonblocking(true)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        
        // Set socket options
        if allow_broadcast {
            socket.set_broadcast(true)
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
                    return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                        format!("Failed to set SO_REUSEPORT: {}", std::io::Error::last_os_error())
                    ));
                }
            }
        }
        
        // Bind to local address if provided
        if let Some((host, port)) = local_addr {
            let addr_str = format!("{}:{}", host, port);
            let bind_addr: SocketAddr = addr_str
                .parse()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Invalid local address: {}", e)
                ))?;
            socket.bind(&bind_addr.into())
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    format!("Failed to bind: {}", e)
                ))?;
        }
        
        // Parse remote address if provided
        let remote_sockaddr = if let Some((host, port)) = remote_addr {
            let addr_str = format!("{}:{}", host, port);
            let addr: SocketAddr = addr_str
                .parse()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Invalid remote address: {}", e)
                ))?;
            
            // Connect to remote address (this filters incoming packets)
            socket.connect(&addr.into())
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    format!("Failed to connect: {}", e)
                ))?;
            Some(addr)
        } else {
            None
        };
        
        let udp_socket: std::net::UdpSocket = socket.into();
        
        // Create protocol instance
        let protocol = protocol_factory.call0(py)?;
        
        // Create transport
        use crate::transports::udp::UdpTransport;
        let transport = UdpTransport::new(
            loop_obj.clone_ref(py),
            udp_socket,
            protocol.clone_ref(py),
            remote_sockaddr,
        )?;
        
        let fd = transport.fd();
        let transport_py = Py::new(py, transport)?;
        
        // Call protocol.connection_made(transport)
        protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
        
        // Register read handler
        let read_ready = transport_py.getattr(py, "_read_ready")?;
        self_.add_reader(py, fd, read_ready)?;
        
        // Return (transport, protocol) tuple wrapped in completed future
        let result_tuple = PyTuple::new(py, vec![
            transport_py.into_any(),
            protocol.into_any(),
        ])?;
        
        let fut = crate::transports::future::CompletedFuture::new(result_tuple.into());
        Ok(Py::new(py, fut)?.into_any())
    }
}

impl VeloxLoop {
    fn _run_once(&self, py: Python<'_>, events: &mut polling::Events) -> VeloxResult<()> {
         // Calculate Timeout
         let has_callbacks = !self.callbacks.is_empty();
         let timeout = if has_callbacks {
             Some(Duration::from_secs(0))
         } else {
             let timers = self.timers.lock();
             if let Some(next) = timers.next_expiry() { // Removed self.timers.next_expiry() to use lock
                 let now = (self.time() * 1_000_000_000.0) as u64;
                 if next > now {
                     Some(Duration::from_nanos(next - now))
                 } else {
                     Some(Duration::from_secs(0))
                 }
             } else {
                 // Default to a small timeout to avoid blocking indefinitely
                 // This allows the loop to check for stop() calls
                 // Use 10ms for better responsiveness
                 Some(Duration::from_millis(10))
             }
         };
         
         // Poll
         match self.poller.poll(events, timeout) {
             Ok(_) => {},
             Err(e) => {
                 // Ignore EINTR?
                 return Err(e);
             }
         }
         
         // Process I/O Events
         for event in events.iter() {

             let fd = event.key as std::os::fd::RawFd;
             
             if event.readable {
                  let callback = {
                      let handles = self.handles.lock();
                      if let Some(handle) = handles.get_reader(fd) {
                          if !handle.cancelled {
                               Some(handle.callback.clone_ref(py))
                          } else {
                               None
                          }
                      } else {
                          None
                      }
                  };
                 
                 if let Some(cb) = callback {
                      if let Err(e) = cb.call0(py) {
                          e.print(py);
                      } else {
                      }
                 }
             }
             if event.writable {
                 let callback = {
                     let handles = self.handles.lock();
                     if let Some(handle) = handles.get_writer(fd) {
                         if !handle.cancelled {
                              Some(handle.callback.clone_ref(py))
                         } else {
                              None
                         }
                     } else {
                         None
                     }
                 };
                if let Some(cb) = callback {
                       if let Err(e) = cb.call0(py) {
                           e.print(py);
                       }
                  }
             }
             
             // Re-arm (Oneshot support for polling v3)
             let handles = self.handles.lock();
             let r = handles.get_reader(fd).is_some();
             let w = handles.get_writer(fd).is_some();
             drop(handles);
             
             if r || w {
                 let ev = if r && w {
                      let mut e = polling::Event::readable(fd as usize);
                      e.writable = true;
                      e
                 } else if r {
                      polling::Event::readable(fd as usize)
                 } else {
                      polling::Event::writable(fd as usize)
                 };
                 
                 // Ignore errors (e.g. if concurrently closed)
                let _ = self.poller.modify(fd, ev);
             }
         }
            // Process Timers
          let now_ns = (self.time() * 1_000_000_000.0) as u64;
          let expired = self.timers.lock().pop_expired(now_ns);
         for entry in expired {
             let _ = entry.callback.bind(py).call(PyTuple::new(py, entry.args)?, None);
         }
         
         // Process Callbacks (call_soon)
         let callbacks = self.callbacks.pop_all();
         for cb in callbacks {
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
    use std::ffi::{CString, CStr};
    use std::ptr;
    use std::mem;
    use pyo3::types::{PyList, PyString, PyInt};
    
    unsafe {
        let mut hints: libc::addrinfo = mem::zeroed();
        hints.ai_family = family;
        hints.ai_socktype = socktype;
        hints.ai_protocol = protocol;
        hints.ai_flags = flags;
        
        let c_host = host.as_ref().map(|h| CString::new(h.as_str()).ok()).flatten();
        let c_port = port.as_ref().map(|p| CString::new(p.as_str()).ok()).flatten();
        
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
                CStr::from_ptr(info.ai_canonname).to_string_lossy().to_string()
            };
            
            // Extract sockaddr
            if info.ai_family == libc::AF_INET {
                let addr = &*(info.ai_addr as *const libc::sockaddr_in);
                let ip_bytes = addr.sin_addr.s_addr.to_ne_bytes();
                let ip = format!("{}.{}.{}.{}", ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);
                let port = u16::from_be(addr.sin_port);
                
                let fam_py = PyInt::new(py, fam);
                let stype_py = PyInt::new(py, stype);
                let proto_py = PyInt::new(py, proto);
                let canon_py = PyString::new(py, &canonname);
                let ip_py = PyString::new(py, &ip);
                let port_py = PyInt::new(py, port);
                let addr_tuple = PyTuple::new(py, vec![ip_py.as_any(), port_py.as_any()])?;
                
                let tuple = PyTuple::new(py, vec![
                    fam_py.as_any(),
                    stype_py.as_any(),
                    proto_py.as_any(),
                    canon_py.as_any(),
                    addr_tuple.as_any(),
                ])?;
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
                let addr_tuple = PyTuple::new(py, vec![
                    ip_py.as_any(),
                    port_py.as_any(),
                    flowinfo_py.as_any(),
                    scope_py.as_any(),
                ])?;
                
                let tuple = PyTuple::new(py, vec![
                    fam_py.as_any(),
                    stype_py.as_any(),
                    proto_py.as_any(),
                    canon_py.as_any(),
                    addr_tuple.as_any(),
                ])?;
                py_list.append(tuple)?;
            }
            
            current = info.ai_next;
        }
        
        libc::freeaddrinfo(res);
        
        Ok(py_list.into())
    }
}

fn perform_getnameinfo(
    py: Python<'_>,
    addr: &str,
    port: u16,
    flags: i32,
) -> PyResult<Py<PyAny>> {
    use std::ffi::CStr;
    use std::mem;
    use std::net::{IpAddr, SocketAddr};
    use pyo3::types::PyString;
    
    // Parse the address
    let ip_addr: IpAddr = addr.parse()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid IP address: {}", e)))?;
    
    let sock_addr = SocketAddr::new(ip_addr, port);
    
    // Use constants directly since libc may not export them on all platforms
    const NI_MAXHOST: usize = 1025;
    const NI_MAXSERV: usize = 32;
    
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
