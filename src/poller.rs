//! Optimized poller implementation
//! Uses direct epoll on Linux for maximum performance (level-triggered mode)
//! Falls back to the `polling` crate on other platforms

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

/// Platform-specific event type
#[cfg(target_os = "linux")]
pub type PlatformEvent = crate::epoll::Event;

#[cfg(not(target_os = "linux"))]
#[derive(Clone, Copy)]
pub struct PlatformEvent {
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
}

/// High-performance poller
/// On Linux: Uses direct epoll with level-triggered mode (no re-arming needed)
/// On other platforms: Uses the polling crate
pub struct LoopPoller {
    #[cfg(target_os = "linux")]
    epoll: crate::epoll::Epoll,
    
    #[cfg(not(target_os = "linux"))]
    poller: polling::Poller,
    #[cfg(not(target_os = "linux"))]
    fd_interests: FxHashMap<RawFd, FdInterest>,
}

impl LoopPoller {
    pub fn new() -> crate::utils::VeloxResult<Self> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                epoll: crate::epoll::Epoll::new()?,
            })
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                poller: polling::Poller::new()?,
                fd_interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            })
        }
    }

    /// Register FD with specific interest
    #[inline]
    pub fn register(
        &mut self,
        fd: RawFd,
        interest: polling::Event,
    ) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            let interest = crate::epoll::Interest::new(interest.readable, interest.writable);
            self.epoll.add(fd, interest)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            let fd_interest = FdInterest::new(interest.readable, interest.writable);
            unsafe {
                let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
                self.poller.add(&borrowed, interest)?;
            }
            self.fd_interests.insert(fd, fd_interest);
        }
        Ok(())
    }

    /// Register FD with oneshot mode (Linux: EPOLLONESHOT, auto-disarms after one event)
    /// This is optimized for sock_recv/sock_sendall where we expect one event per call.
    /// Re-arming is done via rearm_oneshot() instead of delete+add.
    #[inline]
    pub fn register_oneshot(
        &mut self,
        fd: RawFd,
        interest: polling::Event,
    ) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            let interest = crate::epoll::Interest::oneshot(interest.readable, interest.writable);
            self.epoll.add(fd, interest)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // Non-Linux: just use regular register (polling crate is oneshot by default)
            self.register(fd, interest)?;
        }
        Ok(())
    }

    /// Re-arm a oneshot FD. This is cheaper than delete + add.
    /// On Linux: Uses EPOLL_CTL_MOD to re-enable the FD
    /// On other platforms: Uses modify()
    #[inline]
    pub fn rearm_oneshot(
        &mut self,
        fd: RawFd,
        interest: polling::Event,
    ) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            // For oneshot, we always need to re-arm even if interest is the same
            let int = crate::epoll::Interest::oneshot(interest.readable, interest.writable);
            let events = int.to_epoll_events();
            let mut event = libc::epoll_event {
                events,
                u64: fd as u64,
            };

            if unsafe { libc::epoll_ctl(self.epoll.epfd(), libc::EPOLL_CTL_MOD, fd, &mut event) } < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            self.modify(fd, interest)
        }
    }

    /// Modify FD interest
    /// On Linux: Only issues syscall if interest actually changed (level-triggered)
    /// On other platforms: Always re-arms (oneshot mode)
    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: polling::Event) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            let interest = crate::epoll::Interest::new(interest.readable, interest.writable);
            self.epoll.modify(fd, interest)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            let new_interest = FdInterest::new(interest.readable, interest.writable);
            unsafe {
                let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
                self.poller.modify(&borrowed, interest)?;
            }
            self.fd_interests.insert(fd, new_interest);
        }
        Ok(())
    }

    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            self.epoll.delete(fd)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            unsafe {
                let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
                self.poller.delete(&borrowed)?;
            }
            self.fd_interests.remove(&fd);
        }
        Ok(())
    }

    #[inline]
    pub fn notify(&self) -> crate::utils::VeloxResult<()> {
        #[cfg(target_os = "linux")]
        {
            self.epoll.notify()?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            self.poller.notify()?;
        }
        Ok(())
    }

    /// Poll for events - Linux-optimized path
    #[cfg(target_os = "linux")]
    #[inline]
    pub fn poll_native(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<Vec<PlatformEvent>> {
        Ok(self.epoll.wait(timeout)?)
    }

    /// Poll for events - fallback for polling crate compatibility
    #[inline]
    pub fn poll(
        &mut self,
        events: &mut polling::Events,
        timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<usize> {
        #[cfg(target_os = "linux")]
        {
            // For compatibility, convert to polling::Events
            events.clear();
            let native_events = self.epoll.wait(timeout)?;
            // Note: This path is only used for compatibility
            // The optimized path uses poll_native directly
            Ok(native_events.len())
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            events.clear();
            self.poller.wait(events, timeout)?;
            Ok(events.len())
        }
    }

    /// Check if FD is registered
    #[inline]
    #[allow(dead_code)]
    pub fn is_registered(&self, fd: RawFd) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.epoll.is_registered(fd)
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            self.fd_interests.contains_key(&fd)
        }
    }
}
