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
        
        // Use polling::Events
        let mut events = polling::Events::new();
        
        while self.running.load(Ordering::SeqCst) {
            // 1. Run scheduled callbacks
            // We call the internal run_once which is NOT a pymethod
            self._run_once(py, &mut events)?;
            
            // Allow Python signals (Ctrl+C) to interrupt
            if let Err(e) = py.check_signals() {
                return Err(VeloxError::Python(e));
            }
        }
        Ok(())
    }

    // Basic I/O methods

    fn add_reader(&self, py: Python<'_>, fd: RawFd, callback: PyObject) -> PyResult<()> {
        let poller = self.poller.clone();
        
        // Lock handles once
        let mut handles = self.handles.lock();
        let reader_exists = handles.get_reader(fd).is_some();
        let writer_exists = handles.get_writer(fd).is_some();
        
        // Add or modify
        handles.add_reader(fd, callback);
        drop(handles); // Drop lock before poller (avoid potential deadlock though unlikely here)
        
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

    fn remove_reader(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
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
    
    fn add_writer(&self, py: Python<'_>, fd: RawFd, callback: PyObject) -> PyResult<()> {
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

    fn remove_writer(&self, _py: Python<'_>, fd: RawFd) -> PyResult<bool> {
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
    // Callbacks
    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon(&self, callback: PyObject, args: Vec<PyObject>, context: Option<PyObject>) {
        self.callbacks.push(Callback { callback, args, context });
    }
    
    #[pyo3(signature = (callback, *args, context=None))]
    fn call_soon_threadsafe(&self, callback: PyObject, args: Vec<PyObject>, context: Option<PyObject>) {
         self.callbacks.push(Callback { callback, args, context });
         // Push naturally notifies.
    }
    
    #[pyo3(signature = (delay, callback, *args, context=None))]
    fn call_later(&self, delay: f64, callback: PyObject, args: Vec<PyObject>, context: Option<PyObject>) -> u64 {
        let now = (self.time() * 1_000_000_000.0) as u64;
        let delay_ns = (delay * 1_000_000_000.0) as u64;
        let when = now + delay_ns;
        self.timers.lock().insert(when, callback, args, context)
    }
    
    #[pyo3(signature = (when, callback, *args, context=None))]
    fn call_at(&self, when: f64, callback: PyObject, args: Vec<PyObject>, context: Option<PyObject>) -> u64 {
         let when_ns = (when * 1_000_000_000.0) as u64;
         self.timers.lock().insert(when_ns, callback, args, context)
    }

    fn _cancel_timer(&self, timer_id: u64) {
        self.timers.lock().remove(timer_id);
    }

    // --- Networking ---
    
    // internal _accept_connection
    // callback called when listener is ready
    fn _accept_connection(&self) -> PyResult<()> {
         // Placeholder or remove
        Ok(())
    }

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_connection(slf: &Bound<'_, Self>, protocol_factory: PyObject, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
        let py = slf.py();
        let self_ = slf.borrow(); // Use borrow for mutable class
        
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(0);
        
        // 1. Resolve (blocking for now)
        let addr: String = format!("{}:{}", host, port);
        // Connect
        let stream = std::net::TcpStream::connect(&addr)?;
        stream.set_nonblocking(true)?;
        
        // 2. Create Protocol
        let protocol = protocol_factory.call0(py)?;
        
        // 3. Create Transport
        let loop_obj = slf.clone().unbind();
        // Clone loop_obj ref for transport
        let loop_ref = loop_obj.clone_ref(py);
        
        // Get FD before move
        let fd = stream.as_raw_fd();
        
        // transport uses loop_ref (PyObject)
        let transport = TcpTransport::new(py, loop_ref.into_any(), stream, protocol.clone_ref(py))?;
        let transport_py = Py::new(py, transport)?;
        
        // 4. Tie together
        protocol.call_method1(py, "connection_made", (transport_py.clone_ref(py),))?;
        
        // 5. Register Reader
        let read_ready = transport_py.getattr(py, "_read_ready")?;
        // self_.add_reader(py, fd, read_ready)?; // Use self_ to call internal method?
        // Wait, remove usage of loop_obj.call_method1("add_reader").
        // We can call Rust method directly if we have access.
        // But add_reader is defined on &VeloxLoop (self_).
        // self_.add_reader(py, fd, read_ready)?; 
        // NOTE: add_reader updates implementation state.
        self_.add_reader(py, fd, read_ready)?; 
        

        
        // Wrap in Future? 
        // For now return tuple to satisfy signature if user awaits it? No, await expects awaitable.
        // We return a "Real" future?
        // Or just return the result and let Python crash if it tries to await a tuple?
        // User asked for "asyncio.Future".
        // Use `asyncio.Future` from Python.
        let asyncio = py.import("asyncio")?;
        let fut = asyncio.call_method0("Future")?;
        let res = PyTuple::new(py, &[transport_py.clone_ref(py).into_any(), protocol])?;
        fut.call_method1("set_result", (res,))?;
        
        Ok(fut.into())
    }

    #[pyo3(signature = (protocol_factory, host=None, port=None, **_kwargs))]
    fn create_server(slf: &Bound<'_, Self>, protocol_factory: PyObject, host: Option<&str>, port: Option<u16>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
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
        
        // 4. Return Server object (wrapped in Future?)
        // asyncio.start_server returns a coroutine that returns a Server object.
        // We replicate that.
        let asyncio = py.import("asyncio")?;
        let fut = asyncio.call_method0("Future")?;
        fut.call_method1("set_result", (server,))?;
        
        Ok(fut.into())
    }
}

impl VeloxLoop {
    fn get_loop_obj(&self, _py: Python<'_>) -> PyResult<PyObject> {
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
                 None
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
                              Some(handle.callback.clone_ref(py))
                         } else {
                              None
                         }
                     } else {
                         None
                     }
                 };
                 
                 if let Some(cb) = callback {
                      let _ = cb.call0(py); 
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
                      let _ = cb.call0(py);
                 }
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
