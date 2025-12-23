use crate::callbacks::Callback;
use crate::event_loop::VeloxLoop;
use crate::transports::future::PendingFuture;
use pyo3::prelude::*;

impl VeloxLoop {
    /// Schedule a callback to be called on the next iteration (lock-free).
    /// Uses crossbeam-channel internally for efficient MPMC queue operations.
    pub fn call_soon(&self, callback: Py<PyAny>, args: Vec<Py<PyAny>>, context: Option<Py<PyAny>>) {
        self.callbacks.borrow().push(Callback {
            callback,
            args,
            context,
        });
    }

    /// Schedule a callback from another thread (lock-free, thread-safe).
    /// Uses crossbeam-channel internally - safe to call from any thread.
    pub fn call_soon_threadsafe(
        &self,
        callback: Py<PyAny>,
        args: Vec<Py<PyAny>>,
        context: Option<Py<PyAny>>,
    ) {
        // Lock-free push via crossbeam channel
        self.callbacks.borrow().push(Callback {
            callback,
            args,
            context,
        });
        // Use atomic state for lock-free polling check
        if self.atomic_state.is_polling() {
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
