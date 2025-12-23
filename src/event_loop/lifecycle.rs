use crate::event_loop::VeloxLoop;
use crate::event_loop::poll::PlatformEvents;
use crate::utils::{VeloxError, VeloxResult};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

impl VeloxLoop {
    pub fn run_forever(&self, py: Python<'_>) -> VeloxResult<()> {
        // Set state using both RefCell (for compatibility) and atomic (for hot paths)
        {
            let mut state = self.state.borrow_mut();
            state.running = true;
            state.stopped = false;
        }
        self.atomic_state.set_running(true);
        self.atomic_state.set_stopped(false);

        let mut events = PlatformEvents::new();

        loop {
            // Use atomic state for hot path check (lock-free)
            if !self.atomic_state.is_running() || self.atomic_state.is_stopped() {
                break;
            }

            self._run_once(py, &mut events)?;

            // Check stopped after run_once (callbacks may have called stop())
            // Use atomic for lock-free check
            if self.atomic_state.is_stopped() {
                break;
            }

            // Check Python signals (Ctrl+C)
            if let Err(e) = py.check_signals() {
                return Err(VeloxError::Python(e));
            }
        }

        self.state.borrow_mut().running = false;
        self.atomic_state.set_running(false);
        Ok(())
    }

    pub fn stop(&self) {
        let mut state = self.state.borrow_mut();
        state.stopped = true;
        state.running = false;
        // Update atomic state for lock-free access
        self.atomic_state.set_stopped(true);
        self.atomic_state.set_running(false);
    }

    pub fn is_running(&self) -> bool {
        // Use atomic for lock-free check
        self.atomic_state.is_running()
    }

    pub fn is_closed(&self) -> bool {
        // Use atomic for lock-free check
        self.atomic_state.is_closed()
    }

    pub fn get_debug(&self) -> bool {
        self.state.borrow().debug
    }

    pub fn set_debug(&self, enabled: bool) {
        self.state.borrow_mut().debug = enabled;
    }

    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        state.running = false;
        // Update atomic state
        self.atomic_state.set_closed(true);
        self.atomic_state.set_running(false);
    }

    // Exception handler methods
    pub fn set_exception_handler(&self, handler: Option<Py<PyAny>>) {
        *self.exception_handler.borrow_mut() = handler;
    }

    pub fn get_exception_handler(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.exception_handler
            .borrow()
            .as_ref()
            .map(|h| h.clone_ref(py))
    }

    pub fn call_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
        let handler = self
            .exception_handler
            .borrow()
            .as_ref()
            .map(|h| h.clone_ref(py));

        if let Some(handler) = handler {
            match handler.call(py, (py.None(), context.as_any()), None) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error in custom exception handler:");
                    e.print_and_set_sys_last_vars(py);
                    let message = context.bind(py).get_item("message")?;
                    if let Some(msg) = message {
                        eprintln!("Exception in event loop: {}", msg);
                    }
                }
            }
        } else {
            let message = context.bind(py).get_item("message")?;
            if let Some(msg) = message {
                eprintln!("{}", msg);
            }
            let exception = context.bind(py).get_item("exception")?;
            if let Some(exc) = exception {
                if let Ok(traceback_module) = py.import("traceback") {
                    if let Ok(print_exception) = traceback_module.getattr("print_exception") {
                        let _ = print_exception.call1((exc,));
                    }
                } else {
                    let py_err = PyErr::from_value(exc.unbind().clone_ref(py).into_bound(py));
                    py_err.print_and_set_sys_last_vars(py);
                }
            }
        }
        Ok(())
    }

    pub fn default_exception_handler(&self, py: Python<'_>, context: Py<PyDict>) -> PyResult<()> {
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
    pub fn set_task_factory(&self, factory: Option<Py<PyAny>>) {
        *self.task_factory.borrow_mut() = factory;
    }

    pub fn get_task_factory(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.task_factory.borrow().as_ref().map(|f| f.clone_ref(py))
    }

    // Async generator tracking methods
    pub fn _track_async_generator(&self, agen: Py<PyAny>) {
        self.async_generators.borrow_mut().push(agen);
    }

    pub fn _untrack_async_generator(&self, py: Python<'_>, agen: Py<PyAny>) {
        self.async_generators
            .borrow_mut()
            .retain(|g| !g.bind(py).is(agen.bind(py)));
    }

    pub fn shutdown_asyncgens(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let generators = {
            let mut gen_guard = self.async_generators.borrow_mut();
            let gens: Vec<Py<PyAny>> = gen_guard.iter().map(|g| g.clone_ref(py)).collect();
            gen_guard.clear();
            gens
        };

        if generators.is_empty() {
            let future = self.create_future(py)?;
            future.bind(py).borrow().set_result(py, py.None())?;
            return Ok(future.into_any());
        }

        let mut close_coros = Vec::new();
        for generator in generators {
            if let Ok(aclose) = generator.getattr(py, "aclose") {
                if let Ok(coro) = aclose.call0(py) {
                    close_coros.push(coro);
                }
            }
        }

        let asyncio = py.import("asyncio")?;
        let gather = asyncio.getattr("gather")?;

        let coros_tuple = PyTuple::new(py, &close_coros)?;
        let close_task = gather.call1(coros_tuple)?;

        Ok(close_task.unbind())
    }
}
