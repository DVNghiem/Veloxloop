use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Instant, Duration};
use polling::Event;

use crate::poller::LoopPoller;
use crate::handles::{IoHandles, HandleType};
use crate::callbacks::{CallbackQueue, Callback};
use crate::timers::Timers;
use crate::utils::{VeloxResult, VeloxError};
use crate::transports::tcp::{TcpTransport, TcpServer};
use std::net::{TcpListener, SocketAddr};
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};

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
    // We need to track time, asyncio uses loop.time() which is monotonic float seconds.
    // Rust Instant is opaque. We'll verify against a baseline.
    start_time: Instant,
}

#[pymethods]
impl VeloxLoop {
    #[new]
    #[pyo3(signature = (debug=None))] // Updated signature for Option<bool>
    pub fn new(debug: Option<bool>) -> VeloxResult<Self> { // Made pub and changed debug type
        let poller = Arc::new(LoopPoller::new()?);

        Ok(Self {
            poller: poller.clone(),
            handles: Mutex::new(IoHandles::new()),
            callbacks: Arc::new(CallbackQueue::new(poller)),
            timers: Mutex::new(Timers::new()),
            running: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            debug: AtomicBool::new(debug.unwrap_or(false)),
            start_time: Instant::now(),
        })
    }

    fn time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
    
    fn _check_closed(&self) -> VeloxResult<()> {
        // Simple check
        Ok(())
    }

    // Lifecycle
    fn run_forever(&self, py: Python<'_>) -> VeloxResult<()> {
        self.running.store(true, Ordering::SeqCst);
        self.stopped.store(false, Ordering::SeqCst);
        
        // Use polling::Events
        let mut events = polling::Events::new();
        
        while self.running.load(Ordering::SeqCst) && !self.stopped.load(Ordering::SeqCst) {
            // 1. Run scheduled callbacks
            // We call the internal run_once which is NOT a pymethod
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

    // Basic I/O methods

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
             unsafe { poller.register(fd, ev)?; }
        } else {
             unsafe { poller.modify(fd, ev)?; }
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
                 unsafe { self.poller.modify(fd, ev)?; }
            } else {
                 // Remove
                 unsafe { self.poller.delete(fd)?; }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub fn add_writer(&self, py: Python<'_>, fd: RawFd, callback: Py<PyAny>) -> PyResult<()> {
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
             unsafe { poller.register(fd, ev)?; }
        } else {
             unsafe { poller.modify(fd, ev)?; }
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
                 unsafe { self.poller.modify(fd, ev)?; }
            } else {
                 unsafe { self.poller.delete(fd)?; }
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
         self.callbacks.push(Callback { callback, args, context });
         // Push naturally notifies.
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

    // Lifecycle methods
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

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
    }

    // --- Networking ---
    
    // internal _accept_connection
    // callback called when listener is ready
    fn _accept_connection(&self) -> PyResult<()> {
         // Placeholder or remove
        Ok(())
    }

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_connection(slf: &Bound<'_, Self>, protocol_factory: Py<PyAny>, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
        let py = slf.py();
        // let self_ = slf.borrow(); // We don't need borrow of self anymore for this logic (mostly)
        
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
             
        // Connect
        match socket.connect(&addr.into()) {
            Ok(_) => {
                // Immediate success?
                // Should still go through callback path to be consistent or handle here?
                // Let's assume generic path handle it via EINPROGRESS usually.
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.raw_os_error() == Some(115) => {
                // Expected (115 is EINPROGRESS on Linux)
            }
            Err(e) => return Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string())),
        }
        
        let stream: std::net::TcpStream = socket.into();
        let fd = stream.as_raw_fd();
        
        // Create Future
        // Use self.create_future() via Python wrapper to avoid importing asyncio here
        let fut = slf.call_method0("create_future")?;
        
        // Create Callback
        use crate::transports::tcp::AsyncConnectCallback;
        let loop_obj = slf.clone().unbind();
        let fut_obj = fut.clone().unbind(); 
        let callback = AsyncConnectCallback::new(loop_obj.clone_ref(py).into_any(), fut_obj.clone_ref(py), protocol_factory, stream);
        let callback_py = Py::new(py, callback)?;
        
        // Register Writer (for Connect)
        // We need to call add_writer on the loop.
        // slf is &Bound<Self>. We can call Python method "add_writer" on it (exposed via base implementation inheritance if it worked completely via python, but here slf IS the VeloxLoop instance).
        // Since VeloxLoop has add_writer pymethod, we can call it.
        slf.call_method1("add_writer", (fd, callback_py))?;
        
        Ok(fut.into())
    }    

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_server(slf: &Bound<'_, Self>, protocol_factory: Py<PyAny>, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
        let py = slf.py();
        let loop_obj = slf.clone().unbind();
        
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        let addr = format!("{}:{}", host, port);
        
        // 1. Bind
        let listener = std::net::TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;
        
        // 2. Create Server object
        let server = TcpServer::new(listener, loop_obj.clone_ref(py).into_any(), protocol_factory.clone_ref(py));
        let server = Py::new(py, server)?;
        
        // 3. Register Accept Handler
        // The server needs to be polled for reading (accept).
        // TcpServer defines `_on_accept`.
        let on_accept = server.getattr(py, "_on_accept")?;
        
        // Use self.add_reader directly?
        // We need fd from listener. We can get it from server.
        let fd = server.call_method0(py, "fd")?.extract::<i32>(py)?;
        
        // self.add_reader(fd, on_accept)
        let self_ = slf.borrow();
        self_.add_reader(py, fd, on_accept)?;
        
        // 4. Return Server object wrapped in completed future
        let fut = crate::transports::tcp::CompletedFuture::new(server.into());
        
        Ok(Py::new(py, fut)?.into())
    }
}

impl VeloxLoop {
    fn get_loop_obj(&self, _py: Python<'_>) -> PyResult<Py<PyAny>> {
        // This is hard without `slf`.
        // We will assume the user passes the loop or we fix signature.
        Err(VeloxError::RuntimeError("Creating connection requires bound loop".into()).into())
    }

    fn _run_once(&self, py: Python<'_>, events: &mut polling::Events) -> VeloxResult<()> {
         // 1. Calculate Timeout
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
         
         // 2. Poll
         match self.poller.poll(events, timeout) {
             Ok(_) => {},
             Err(e) => {
                 // Ignore EINTR?
                 return Err(e);
             }
         }
         
         // 3. Process I/O Events
         for event in events.iter() {

             let fd = event.key as std::os::fd::RawFd;
             
             if event.readable {
                  let callback = {
                      let handles = self.handles.lock();
                      if let Some(handle) = handles.get_reader(fd) {
                          if !handle.cancelled {
                               // eprintln!("DEBUG: Found readable callback for fd={}", fd);
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
                          // eprintln!("DEBUG: callback executed successfully");
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
                 unsafe { let _ = self.poller.modify(fd, ev); }
             }
         }
                  // 4. Process Timers
          let now_ns = (self.time() * 1_000_000_000.0) as u64;
          let expired = self.timers.lock().pop_expired(now_ns);
         for entry in expired {
             let _ = entry.callback.bind(py).call(PyTuple::new(py, entry.args)?, None);
         }
         
         // 5. Process Callbacks (call_soon)
         let callbacks = self.callbacks.pop_all();
         for cb in callbacks {
             let _ = cb.callback.bind(py).call(PyTuple::new(py, cb.args)?, None);
         }
         
         Ok(())
    }
}
