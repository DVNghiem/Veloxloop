use crate::event_loop::VeloxLoop;
use crate::utils::VeloxResult;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::time::Duration;

impl VeloxLoop {
    /// single iteration of the event loop
    #[inline(always)]
    pub(crate) fn _run_once(
        &self,
        py: Python<'_>,
        _events: &mut polling::Events,
    ) -> VeloxResult<()> {
        let has_callbacks = !self.callbacks.borrow().is_empty();

        // Calculate timeout
        let timeout = if has_callbacks {
            Some(Duration::ZERO)
        } else {
            let timers = self.timers.borrow();
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

        // Poll - minimize state mutations
        self.state.borrow_mut().is_polling = true;

        // Use native epoll on Linux for maximum performance
        #[cfg(target_os = "linux")]
        {
            let events = self.poller.borrow_mut().poll_native(timeout);
            self.state.borrow_mut().is_polling = false;

            match events {
                Ok(evs) => {
                    self._process_native_events(py, evs)?;
                }
                Err(e) => return Err(e),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let res = self.poller.borrow_mut().poll(_events, timeout);
            self.state.borrow_mut().is_polling = false;

            match res {
                Ok(_) => {
                    self._process_polling_events(py, _events)?;
                }
                Err(e) => return Err(e),
            }
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

        // Process Callbacks (call_soon)
        let mut cb_batch = self.callback_buffer.borrow_mut();
        cb_batch.clear();
        self.callbacks.borrow_mut().swap_into(&mut *cb_batch);

        for cb in cb_batch.drain(..) {
            let _ = cb.callback.bind(py).call(PyTuple::new(py, cb.args)?, None);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[inline(always)]
    fn _process_native_events(
        &self,
        py: Python<'_>,
        events: Vec<crate::epoll::Event>,
    ) -> VeloxResult<()> {
        if events.is_empty() {
            return Ok(());
        }

        if events.len() == 1 {
            let event = &events[0];
            let fd = event.fd;

            let (read_cb, write_cb) = {
                let handles = self.handles.borrow();
                if let Some((r_handle, w_handle)) = handles.map.get(&fd) {
                    let r = if event.readable {
                        r_handle.as_ref().filter(|h| !h.cancelled).cloned()
                    } else {
                        None
                    };
                    let w = if event.writable {
                        w_handle.as_ref().filter(|h| !h.cancelled).cloned()
                    } else {
                        None
                    };
                    (r, w)
                } else {
                    (None, None)
                }
            };

            if let Some(h) = read_cb {
                if let Err(e) = h.execute(py) {
                    e.print(py);
                }
            }
            if let Some(h) = write_cb {
                if let Err(e) = h.execute(py) {
                    e.print(py);
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

        for (_, r_h, w_h, _, _) in pending.iter() {
            if let Some(h) = r_h {
                if let Err(e) = h.execute(py) {
                    e.print(py);
                }
            }
            if let Some(h) = w_h {
                if let Err(e) = h.execute(py) {
                    e.print(py);
                }
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    fn _process_polling_events(&self, py: Python<'_>, events: &polling::Events) -> VeloxResult<()> {
        let event_count = events.len();
        if event_count == 0 {
            return Ok(());
        }

        let mut pending = self.pending_ios.borrow_mut();
        pending.clear();

        let capacity = pending.capacity();
        if capacity < event_count {
            pending.reserve(event_count - capacity);
        }

        {
            let handles = self.handles.borrow();
            for event in events.iter() {
                let fd = event.key as std::os::fd::RawFd;
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

        for (_, r_h, w_h, _, _) in pending.iter() {
            if let Some(h) = r_h {
                if let Err(e) = h.execute(py) {
                    e.print(py);
                }
            }
            if let Some(h) = w_h {
                if let Err(e) = h.execute(py) {
                    e.print(py);
                }
            }
        }

        {
            let mut poller = self.poller.borrow_mut();
            for (fd, _, _, has_r, has_w) in pending.iter() {
                if *has_r || *has_w {
                    let mut ev = if *has_r {
                        polling::Event::readable(*fd as usize)
                    } else {
                        polling::Event::writable(*fd as usize)
                    };
                    if *has_r && *has_w {
                        ev.writable = true;
                    }
                    let _ = poller.modify(*fd, ev);
                }
            }
        }

        Ok(())
    }
}
