use crate::utils::VeloxResult;
use polling::{Event, Events, Poller};
use std::os::fd::RawFd;
use std::sync::Arc;

pub struct LoopPoller {
    poller: Arc<Poller>,
    // Internal mutability for shared access via Arc in EventLoop
    registered_fds: parking_lot::Mutex<std::collections::HashSet<RawFd>>,
}

impl LoopPoller {
    pub fn new() -> VeloxResult<Self> {
        Ok(Self {
            poller: Arc::new(Poller::new()?),
            registered_fds: parking_lot::Mutex::new(std::collections::HashSet::new()),
        })
    }

    // API: register with specific interest
    pub fn register(&self, fd: RawFd, interest: Event) -> VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.add(&borrowed, interest)?;
        }
        self.registered_fds.lock().insert(fd);
        Ok(())
    }

    pub fn modify(&self, fd: RawFd, interest: Event) -> VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.modify(&borrowed, interest)?;
        }
        Ok(())
    }

    pub fn delete(&self, fd: RawFd) -> VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.delete(&borrowed)?;
        }
        self.registered_fds.lock().remove(&fd);
        Ok(())
    }

    pub fn notify(&self) -> VeloxResult<()> {
        self.poller.notify()?;
        Ok(())
    }

    pub fn poll(
        &self,
        events: &mut Events,
        timeout: Option<std::time::Duration>,
    ) -> VeloxResult<usize> {
        events.clear();
        self.poller.wait(events, timeout)?;
        Ok(events.len())
    }
}
