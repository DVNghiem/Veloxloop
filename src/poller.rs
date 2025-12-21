use std::os::fd::RawFd;

pub struct LoopPoller {
    poller: polling::Poller,
    registered_fds: std::collections::HashSet<RawFd>,
}

impl LoopPoller {
    pub fn new() -> crate::utils::VeloxResult<Self> {
        Ok(Self {
            poller: polling::Poller::new()?,
            registered_fds: std::collections::HashSet::new(),
        })
    }

    // API: register with specific interest
    pub fn register(
        &mut self,
        fd: RawFd,
        interest: polling::Event,
    ) -> crate::utils::VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.add(&borrowed, interest)?;
        }
        self.registered_fds.insert(fd);
        Ok(())
    }

    pub fn modify(&mut self, fd: RawFd, interest: polling::Event) -> crate::utils::VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.modify(&borrowed, interest)?;
        }
        Ok(())
    }

    pub fn delete(&mut self, fd: RawFd) -> crate::utils::VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.delete(&borrowed)?;
        }
        self.registered_fds.remove(&fd);
        Ok(())
    }

    pub fn notify(&self) -> crate::utils::VeloxResult<()> {
        self.poller.notify()?;
        Ok(())
    }

    pub fn poll(
        &mut self,
        events: &mut polling::Events,
        timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<usize> {
        events.clear();
        self.poller.wait(events, timeout)?;
        Ok(events.len())
    }
}
