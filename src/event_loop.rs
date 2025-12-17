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
        eprintln!("get_debug() called, returning {}", val);
        val
    }
    
    fn set_debug(&self, enabled: bool) {
        self.debug.store(enabled, Ordering::SeqCst);
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
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
        let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
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
