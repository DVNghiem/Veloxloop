use crate::event_loop::VeloxLoop;
use crate::handles::{Handle, IoCallback};
use crate::poller::{PlatformEvent, PollerEvent};
use crate::utils::VeloxResult;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::time::Duration;

/// Platform events - on all platforms we use native events
pub(crate) struct PlatformEvents;

impl PlatformEvents {
    pub fn new() -> Self {
        Self
    }
}

impl VeloxLoop {
    /// single iteration of the event loop
    #[inline(always)]
    pub(crate) fn _run_once(
        &self,
        py: Python<'_>,
        _events: &mut PlatformEvents,
    ) -> VeloxResult<()> {
        let has_callbacks = !self.callbacks.is_empty();

        // Calculate timeout
        let timeout = if has_callbacks {
            Some(Duration::ZERO)
        } else {
            let mut timers = self.timers.borrow_mut();
            if let Some(next) = timers.next_expiry() {
                let now_ns = (self.time() * 1_000_000_000.0) as u64;
                if next > now_ns {
                    Some(Duration::from_nanos(next - now_ns))
                } else {
                    Some(Duration::ZERO)
                }
            } else {
                // Default poll timeout when no timers
                Some(Duration::from_millis(10))
            }
        };

        // Poll - use atomic state for lock-free polling flag
        self.atomic_state.set_polling(true);

        // Use io-uring based polling on Linux
        // Release GIL during blocking poll to allow other threads to run
        let events = py.detach(|| self.poller.borrow_mut().poll_native(timeout));
        self.atomic_state.set_polling(false);

        match events {
            Ok(evs) => {
                self._process_native_events(py, evs)?;
            }
            Err(e) => return Err(e),
        }

        // Process Timers - use C API for callback invocation (no PyTuple allocation)
        let now_ns = (self.time() * 1_000_000_000.0) as u64;
        let expired = self.timers.borrow_mut().pop_expired(now_ns, 0);
        for entry in expired {
            // Use C API: avoids PyTuple::new() overhead and trait dispatch
            unsafe {
                crate::ffi_utils::call_callback_ignore_err(
                    entry.callback.as_ptr(),
                    &entry.args,
                );
            }
        }

        // Process Callbacks (call_soon) - lock-free drain via crossbeam
        let mut cb_batch = self.callback_buffer.borrow_mut();
        cb_batch.clear();
        self.callbacks.swap_into(&mut *cb_batch);

        for cb in cb_batch.drain(..) {
            // Use C API: for 0-arg case uses PyObject_CallNoArgs (no tuple at all)
            unsafe {
                if let Err(e) = crate::ffi_utils::call_callback(py, cb.callback.as_ptr(), &cb.args) {
                    let context = PyDict::new(py);
                    context.set_item("message", "Exception in callback")?;
                    context.set_item("exception", e.value(py))?;
                    self.call_exception_handler(py, context.unbind())?;
                }
            }
        }

        Ok(())
    }

    /// Process io-uring completion events
    #[inline(always)]
    fn _process_native_events(
        &self,
        py: Python<'_>,
        events: Vec<PlatformEvent>,
    ) -> VeloxResult<()> {
        if events.is_empty() {
            return Ok(());
        }

        if events.len() == 1 {
            let event = &events[0];
            let fd = event.fd;

            // Handle error events - unregister the FD if there's an error
            #[cfg(target_os = "linux")]
            if event.error {
                // On error, remove both reader and writer
                let mut handles = self.handles.borrow_mut();
                handles.remove_reader(fd);
                handles.remove_writer(fd);
                let _ = self.poller.borrow_mut().delete(fd);
                return Ok(());
            }

            // Clone callbacks to avoid borrow issues - direct extraction, no Vec needed
            let (r_cb, w_cb) = {
                let handles = self.handles.borrow();
                (handles.get_reader(fd), handles.get_writer(fd))
            };
            if let Some(cb) = r_cb {
                cb.execute(py)?;
            }
            if let Some(cb) = w_cb {
                cb.execute(py)?;
            }
            // Re-arm the FD for io-uring (poll_add is oneshot)
            // may have removed themselves (e.g., oneshot sock_recv callbacks)
            let (still_has_reader, still_has_writer) = {
                let handles = self.handles.borrow();
                handles.get_states(fd)
            };

            if still_has_reader || still_has_writer {
                let ev = PollerEvent::new(fd as usize, still_has_reader, still_has_writer);
                let mut poller = self.poller.borrow_mut();

                // Check FD state: is it already registered or not
                #[cfg(target_os = "linux")]
                {
                    if self.oneshot_disabled.borrow().contains(&fd) {
                        poller.rearm_oneshot(fd, ev)?;
                    } else {
                        // FD is new or has been removed → needs to be registered again
                        poller.register_oneshot(fd, ev)?;
                    }
                }
            }

            return Ok(());
        }

        let mut pending = self.pending_ios.borrow_mut();
        pending.clear();

        let event_count = events.len();
        let capacity = pending.capacity();
        if capacity < event_count {
            pending.reserve(event_count - capacity);
        }

        {
            let handles = self.handles.borrow();
            for event in events.iter() {
                let fd = event.fd;
                if let Some((r_handle, w_handle)) = handles.get_state_owned(fd) {
                    // Save is_some state before filter() consumes the Option
                    let has_reader = r_handle.is_some();
                    let has_writer = w_handle.is_some();

                    // Use .filter() on owned Option<Handle> - avoids second clone
                    // that was previously done by .as_ref().filter().cloned()
                    let reader_cb = if event.readable {
                        r_handle.filter(|h| !h.cancelled)
                    } else {
                        None
                    };
                    let writer_cb = if event.writable {
                        w_handle.filter(|h| !h.cancelled)
                    } else {
                        None
                    };

                    pending.push((
                        fd,
                        reader_cb,
                        writer_cb,
                        has_reader,
                        has_writer,
                    ));
                }
            }
        }

        let mut python_callbacks: Vec<Handle> = Vec::new();

        // Use drain() to consume pending_ios, moving handles instead of cloning
        for (fd, r_h, w_h, _has_r, _has_w) in pending.drain(..) {
            if let Some(h) = r_h {
                match &h.callback {
                    IoCallback::Native(cb) => {
                        let _ = cb(py);
                    } // Native first, no GIL hold
                    _ => python_callbacks.push(h), // Move instead of clone
                }
            }
            if let Some(h) = w_h {
                match &h.callback {
                    IoCallback::Native(cb) => {
                        let _ = cb(py);
                    }
                    _ => python_callbacks.push(h), // Move instead of clone
                }
            }

            // Re-arm the FD for io-uring (poll_add is oneshot)
            // CRITICAL: Re-check handles state AFTER callback execution since callbacks
            // may have removed themselves (e.g., oneshot sock_recv callbacks)
            let (still_has_reader, still_has_writer) = {
                let handles = self.handles.borrow();
                handles.get_states(fd)
            };

            if still_has_reader || still_has_writer {
                let ev = PollerEvent::new(fd as usize, still_has_reader, still_has_writer);
                let _ = self.poller.borrow_mut().rearm_oneshot(fd, ev);
            }
        }
        // Execute batched Python callbacks at end (one GIL hold)
        for cb in python_callbacks {
            if let Err(e) = cb.execute(py) {
                e.print(py);
            }
        }

        Ok(())
    }
}
