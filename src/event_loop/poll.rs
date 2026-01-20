use crate::event_loop::VeloxLoop;
use crate::handles::{Handle, IoCallback};
use crate::poller::{PlatformEvent, PollerEvent};
use crate::utils::VeloxResult;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::os::fd::RawFd;
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
        let has_callbacks = !self.callbacks.borrow().is_empty();

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
        let events = self.poller.borrow_mut().poll_native(timeout);
        self.atomic_state.set_polling(false);

        match events {
            Ok(evs) => {
                self._process_native_events(py, evs)?;
            }
            Err(e) => return Err(e),
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

        // Process Callbacks (call_soon) - lock-free drain via crossbeam
        let mut cb_batch = self.callback_buffer.borrow_mut();
        cb_batch.clear();
        self.callbacks.borrow().swap_into(&mut *cb_batch);

        for cb in cb_batch.drain(..) {
            let _ = cb.callback.bind(py).call(PyTuple::new(py, cb.args)?, None);
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

            // Clone callbacks to avoid borrow issues
            let callbacks: Vec<(RawFd, Option<Handle>, Option<Handle>)> = {
                let handles = self.handles.borrow();
                events
                    .iter()
                    .map(|ev| (ev.fd, handles.get_reader(ev.fd), handles.get_writer(ev.fd)))
                    .collect()
            };
            for (_, r_cb, w_cb) in callbacks {
                if let Some(cb) = r_cb {
                    cb.execute(py)?;
                }
                if let Some(cb) = w_cb {
                    cb.execute(py)?;
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

                #[cfg(not(target_os = "linux"))]
                {
                    // On non-Linux platforms, always register as oneshot
                    poller.register_oneshot(fd, ev)?;
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

        let mut python_callbacks: Vec<Handle> = Vec::new();

        for (fd, r_h, w_h, _has_r, _has_w) in pending.iter() {
            if let Some(h) = r_h {
                match &h.callback {
                    IoCallback::Native(cb) => {
                        let _ = cb(py);
                    } // Native first, no GIL hold
                    _ => python_callbacks.push(h.clone()), // Batch Python
                }
            }
            if let Some(h) = w_h {
                match &h.callback {
                    IoCallback::Native(cb) => {
                        let _ = cb(py);
                    }
                    _ => python_callbacks.push(h.clone()),
                }
            }

            // Re-arm the FD for io-uring (poll_add is oneshot)
            // CRITICAL: Re-check handles state AFTER callback execution since callbacks
            // may have removed themselves (e.g., oneshot sock_recv callbacks)
            let (still_has_reader, still_has_writer) = {
                let handles = self.handles.borrow();
                handles.get_states(*fd)
            };

            if still_has_reader || still_has_writer {
                let ev = PollerEvent::new(*fd as usize, still_has_reader, still_has_writer);
                let _ = self.poller.borrow_mut().rearm_oneshot(*fd, ev);
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
