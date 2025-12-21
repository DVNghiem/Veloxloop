use rustc_hash::FxHashMap;
use std::os::fd::RawFd;

/// Cached event state to avoid unnecessary modify() calls
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FdInterest {
    pub readable: bool,
    pub writable: bool,
}

impl FdInterest {
    #[inline]
    pub fn new(readable: bool, writable: bool) -> Self {
        Self { readable, writable }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn to_event(&self, fd: RawFd) -> polling::Event {
        let mut ev = polling::Event::none(fd as usize);
        ev.readable = self.readable;
        ev.writable = self.writable;
        ev
    }
}

pub struct LoopPoller {
    poller: polling::Poller,
    /// Track current interest for each FD to avoid redundant modify() calls
    /// Uses FxHashMap for faster integer key hashing
    fd_interests: FxHashMap<RawFd, FdInterest>,
}

impl LoopPoller {
    pub fn new() -> crate::utils::VeloxResult<Self> {
        Ok(Self {
            poller: polling::Poller::new()?,
            fd_interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
        })
    }

    /// Register FD with specific interest - optimized with interest tracking
    #[inline]
    pub fn register(
        &mut self,
        fd: RawFd,
        interest: polling::Event,
    ) -> crate::utils::VeloxResult<()> {
        let fd_interest = FdInterest::new(interest.readable, interest.writable);
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.add(&borrowed, interest)?;
        }
        self.fd_interests.insert(fd, fd_interest);
        Ok(())
    }

    /// Modify FD interest - MUST be called after each event due to oneshot mode
    /// Note: polling crate uses oneshot/edge-triggered mode internally,
    /// so we MUST re-arm after each event regardless of whether interest changed
    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: polling::Event) -> crate::utils::VeloxResult<()> {
        let new_interest = FdInterest::new(interest.readable, interest.writable);
        
        // Always call modify() due to oneshot mode - each event disables the FD
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.modify(&borrowed, interest)?;
        }
        self.fd_interests.insert(fd, new_interest);
        Ok(())
    }

    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> crate::utils::VeloxResult<()> {
        unsafe {
            let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
            self.poller.delete(&borrowed)?;
        }
        self.fd_interests.remove(&fd);
        Ok(())
    }

    #[inline]
    pub fn notify(&self) -> crate::utils::VeloxResult<()> {
        self.poller.notify()?;
        Ok(())
    }

    /// Optimized poll with pre-cleared events buffer
    #[inline]
    pub fn poll(
        &mut self,
        events: &mut polling::Events,
        timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<usize> {
        events.clear();
        self.poller.wait(events, timeout)?;
        Ok(events.len())
    }

    /// Get current interest for an FD (for re-arming optimization)
    #[inline]
    #[allow(dead_code)]
    pub fn get_interest(&self, fd: RawFd) -> Option<FdInterest> {
        self.fd_interests.get(&fd).copied()
    }

    /// Check if FD is registered
    #[inline]
    #[allow(dead_code)]
    pub fn is_registered(&self, fd: RawFd) -> bool {
        self.fd_interests.contains_key(&fd)
    }
}
