use crate::callbacks::Callback;
use crate::event_loop::VeloxLoop;
use crate::transports::future::PendingFuture;
use pyo3::prelude::*;

impl VeloxLoop {
    pub fn call_soon(&self, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) {
        self.callbacks.borrow_mut().push(Callback {
            callback,
            args,
            context,
        });
    }

    pub fn call_soon_threadsafe(
        &self,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) {
        self.callbacks.borrow_mut().push(Callback {
            callback,
            args,
            context,
        });
        if self.state.borrow().is_polling {
            let _ = self.poller.borrow().notify();
        }
    }

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
        self.timers
            .borrow_mut()
            .insert(when, callback, args, context, 0)
    }

    pub fn call_at(
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

    pub fn _cancel_timer(&self, timer_id: u64) {
        self.timers.borrow_mut().cancel(timer_id);
    }

    // Create a Rust-based PendingFuture
    pub fn create_future(&self, py: Python<'_>) -> PyResult<Py<PendingFuture>> {
        Py::new(py, PendingFuture::new())
    }
}
