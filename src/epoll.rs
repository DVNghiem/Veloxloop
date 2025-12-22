//! Direct epoll implementation for maximum performance on Linux.
//! Uses level-triggered mode to avoid re-arming FDs after each event.
//! This eliminates the syscall overhead of the `polling` crate's oneshot mode.

use rustc_hash::FxHashMap;
use std::os::fd::RawFd;
use std::time::Duration;

/// Event interests for a file descriptor
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Interest {
    pub readable: bool,
    pub writable: bool,
    /// If true, use EPOLLONESHOT (auto-disarm after one event)
    /// This is useful for one-shot operations like sock_recv
    pub oneshot: bool,
}

impl Interest {
    #[inline(always)]
    pub fn new(readable: bool, writable: bool) -> Self {
        Self { readable, writable, oneshot: false }
    }
    
    #[inline(always)]
    pub fn oneshot(readable: bool, writable: bool) -> Self {
        Self { readable, writable, oneshot: true }
    }

    #[inline(always)]
    pub fn to_epoll_events(self) -> u32 {
        let mut events = 0u32;
        if self.readable {
            events |= libc::EPOLLIN as u32;
        }
        if self.writable {
            events |= libc::EPOLLOUT as u32;
        }
        if self.oneshot {
            events |= libc::EPOLLONESHOT as u32;
        }
        events
    }
}

/// A ready event from epoll
#[derive(Clone, Copy)]
pub struct Event {
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
}

/// Direct epoll wrapper for Linux - level-triggered mode
/// This avoids the re-arming overhead of oneshot mode
pub struct Epoll {
    epfd: RawFd,
    /// Track registered FDs and their interests
    interests: FxHashMap<RawFd, Interest>,
    /// Track FDs that are registered but disabled (oneshot fired, waiting for re-arm)
    disabled_oneshot: FxHashMap<RawFd, Interest>,
    /// Pre-allocated event buffer for epoll_wait
    raw_events: Vec<libc::epoll_event>,
    /// Event pipe for waking up the poller
    wake_read: RawFd,
    wake_write: RawFd,
}

impl Epoll {
    /// Create a new epoll instance
    pub fn new() -> std::io::Result<Self> {
        let epfd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if epfd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Create wake pipe
        let mut fds = [0i32; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } < 0 {
            unsafe { libc::close(epfd) };
            return Err(std::io::Error::last_os_error());
        }

        let wake_read = fds[0];
        let wake_write = fds[1];

        // Register wake pipe for reading
        let mut event = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: wake_read as u64,
        };
        if unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, wake_read, &mut event) } < 0 {
            unsafe {
                libc::close(epfd);
                libc::close(wake_read);
                libc::close(wake_write);
            }
            return Err(std::io::Error::last_os_error());
        }

        Ok(Self {
            epfd,
            interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            disabled_oneshot: FxHashMap::with_capacity_and_hasher(64, Default::default()),
            raw_events: vec![
                libc::epoll_event { events: 0, u64: 0 };
                1024
            ],
            wake_read,
            wake_write,
        })
    }

    /// Register a new FD with the given interest
    #[inline]
    pub fn add(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        let events = interest.to_epoll_events();
        let mut event = libc::epoll_event {
            events,
            u64: fd as u64,
        };

        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut event) } < 0 {
            return Err(std::io::Error::last_os_error());
        }

        self.interests.insert(fd, interest);
        Ok(())
    }

    /// Modify an existing FD's interest
    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        // Skip syscall if interest unchanged (level-triggered mode benefit!)
        if let Some(&current) = self.interests.get(&fd) {
            if current == interest {
                return Ok(());
            }
        }

        let events = interest.to_epoll_events();
        let mut event = libc::epoll_event {
            events,
            u64: fd as u64,
        };

        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_MOD, fd, &mut event) } < 0 {
            return Err(std::io::Error::last_os_error());
        }

        self.interests.insert(fd, interest);
        Ok(())
    }

    /// Remove an FD from epoll
    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> std::io::Result<()> {
        // epoll_ctl with EPOLL_CTL_DEL doesn't need event pointer since Linux 2.6.9
        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) } < 0
        {
            let err = std::io::Error::last_os_error();
            // Ignore ENOENT - FD might not be registered
            if err.raw_os_error() != Some(libc::ENOENT) {
                return Err(err);
            }
        }
        self.interests.remove(&fd);
        Ok(())
    }

    /// Wake up the poller
    #[inline]
    pub fn notify(&self) -> std::io::Result<()> {
        let buf = [1u8; 1];
        loop {
            let n = unsafe { libc::write(self.wake_write, buf.as_ptr() as *const _, 1) };
            if n >= 0 {
                return Ok(());
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                // EAGAIN is fine - pipe is full, meaning we've already notified
                if err.raw_os_error() == Some(libc::EAGAIN) {
                    return Ok(());
                }
                return Err(err);
            }
        }
    }

    /// Wait for events with optional timeout
    /// Returns a slice of ready events
    #[inline]
    pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<Vec<Event>> {
        let timeout_ms = match timeout {
            Some(d) => d.as_millis() as i32,
            None => -1, // Infinite
        };

        loop {
            let n = unsafe {
                libc::epoll_wait(
                    self.epfd,
                    self.raw_events.as_mut_ptr(),
                    self.raw_events.len() as i32,
                    timeout_ms,
                )
            };

            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }

            let mut events = Vec::with_capacity(n as usize);
            for i in 0..n as usize {
                let raw = &self.raw_events[i];
                let fd = raw.u64 as RawFd;

                // Handle wake pipe
                if fd == self.wake_read {
                    // Drain the pipe
                    let mut buf = [0u8; 64];
                    while unsafe { libc::read(self.wake_read, buf.as_mut_ptr() as *mut _, 64) } > 0
                    {
                    }
                    continue;
                }

                events.push(Event {
                    fd,
                    readable: (raw.events & libc::EPOLLIN as u32) != 0,
                    writable: (raw.events & libc::EPOLLOUT as u32) != 0,
                });
            }

            return Ok(events);
        }
    }

    /// Get the raw epoll file descriptor (for direct access in performance-critical paths)
    #[inline(always)]
    pub fn epfd(&self) -> RawFd {
        self.epfd
    }

    /// Check if FD is registered
    #[inline]
    pub fn is_registered(&self, fd: RawFd) -> bool {
        self.interests.contains_key(&fd)
    }

    /// Get current interest for an FD
    #[inline]
    #[allow(dead_code)]
    pub fn get_interest(&self, fd: RawFd) -> Option<Interest> {
        self.interests.get(&fd).copied()
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.epfd);
            libc::close(self.wake_read);
            libc::close(self.wake_write);
        }
    }
}

// Safety: Epoll file descriptors are thread-safe
unsafe impl Send for Epoll {}
unsafe impl Sync for Epoll {}
