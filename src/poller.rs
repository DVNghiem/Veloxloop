//! High-performance poller using io-uring on Linux
//! 
//! This module provides the core event loop polling mechanism.
//! On Linux: Uses io-uring for completion-based async IO (REQUIRED)
//! Non-Linux: Stub for future Tokio integration
//!
//! Performance features:
//! - io-uring for zero-copy, batched I/O operations
//! - Completion-based model with submit_read/submit_write for true async I/O
//! - Integrated with IoUringBackend from io_backend module
//! - Lock-free data structures via dashmap/crossbeam

#[cfg(target_os = "linux")]
use std::io;
use std::os::fd::RawFd;

#[cfg(target_os = "linux")]
use std::net::SocketAddr;

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[cfg(target_os = "linux")]
use io_uring::{opcode, types, IoUring, Probe};

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
use rustc_hash::FxHashMap;

#[cfg(not(target_os = "linux"))]
use rustc_hash::FxHashMap;

use std::time::Duration;

/// Cached event state - used for non-Linux backends
#[cfg(not(target_os = "linux"))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FdInterest {
    pub readable: bool,
    pub writable: bool,
}

#[cfg(not(target_os = "linux"))]
impl FdInterest {
    #[inline]
    pub fn new(readable: bool, writable: bool) -> Self {
        Self { readable, writable }
    }
}

/// Event type that works across platforms
#[derive(Clone, Copy)]
pub struct PollerEvent {
    pub readable: bool,
    pub writable: bool,
}

impl PollerEvent {
    /// Create a new poller event with specified interest
    #[inline]
    pub fn new(_key: usize, readable: bool, writable: bool) -> Self {
        Self { readable, writable }
    }

    /// Create an event for readable interest only
    #[inline]
    pub fn readable(_key: usize) -> Self {
        Self {
            readable: true,
            writable: false,
        }
    }

    /// Create an event for writable interest only
    #[inline]
    pub fn writable(_key: usize) -> Self {
        Self {
            readable: false,
            writable: true,
        }
    }
}

/// Platform-specific event type representing a completed IO operation
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
pub struct PlatformEvent {
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
    pub error: bool,
}
#[cfg(not(target_os = "linux"))]
#[derive(Clone, Copy)]
pub struct PlatformEvent {
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
}

/// io-uring operation token for tracking pending operations
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IoToken(pub u64);

/// Pending poll operation tracking
#[cfg(target_os = "linux")]
struct PendingPoll {
    fd: RawFd,
    #[allow(dead_code)]
    readable: bool,
    #[allow(dead_code)]
    writable: bool,
}

#[cfg(target_os = "linux")]
const SQ_SIZE: u32 = 256;
#[cfg(target_os = "linux")]
const CQ_SIZE: u32 = 512;

pub struct LoopPoller {
    /// The io-uring instance
    ring: IoUring,
    /// Token counter for operations
    token_counter: AtomicU64,
    /// Track registered FDs and their poll tokens
    fd_tokens: FxHashMap<RawFd, IoToken>,
    /// Track pending poll operations
    pending_polls: FxHashMap<u64, PendingPoll>,
    /// Eventfd for waking up the ring
    eventfd: RawFd,
    /// Token for eventfd poll
    eventfd_token: u64,
    /// Probe for checking supported operations (kept for future op support checks)
    #[allow(dead_code)]
    probe: Probe,
    pending_submissions: AtomicUsize,
    last_submit_time: parking_lot::Mutex<std::time::Instant>,
}

#[cfg(target_os = "linux")]
impl LoopPoller {
    pub fn new() -> crate::utils::VeloxResult<Self> {
        let ring = IoUring::builder()
            .setup_cqsize(CQ_SIZE)
            .build(SQ_SIZE)
            .map_err(crate::utils::VeloxError::Io)?;

        // Probe for supported operations
        let mut probe = Probe::new();
        ring.submitter()
            .register_probe(&mut probe)
            .map_err(crate::utils::VeloxError::Io)?;

        // Create eventfd for waking
        let eventfd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if eventfd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let mut poller = Self {
            ring,
            token_counter: AtomicU64::new(1),
            fd_tokens: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            pending_polls: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            eventfd,
            eventfd_token: 0,
            probe,
            pending_submissions: AtomicUsize::new(0),
            last_submit_time: parking_lot::Mutex::new(std::time::Instant::now()),
        };

        // Register eventfd for notifications
        poller.eventfd_token = poller.next_token();
        poller.submit_poll_add(eventfd, true, false, poller.eventfd_token)?;

        Ok(poller)
    }

    #[inline]
    fn next_token(&self) -> u64 {
        self.token_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Submit a poll_add operation to io-uring (queues for batch submission)
    #[inline]
    fn submit_poll_add(
        &mut self,
        fd: RawFd,
        readable: bool,
        writable: bool,
        token: u64,
    ) -> crate::utils::VeloxResult<()> {
        use crate::constants::POLLER_BATCH_THRESHOLD;

        let mut flags: u32 = 0;
        if readable {
            flags |= libc::POLLIN as u32;
        }
        if writable {
            flags |= libc::POLLOUT as u32;
        }

        let poll_e = opcode::PollAdd::new(types::Fd(fd), flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&poll_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable,
                writable,
            },
        );

        if self.pending_submissions.load(Ordering::Relaxed) >= POLLER_BATCH_THRESHOLD {
            self.flush_submissions()?;
        }
        
        Ok(())
    }

    /// Cancel a pending poll operation
    fn submit_poll_remove(&mut self, token: u64) -> crate::utils::VeloxResult<()> {
        let cancel_e = opcode::PollRemove::new(token)
            .build()
            .user_data(0); // We don't track cancellation completions

        unsafe {
            let _ = self.ring.submission().push(&cancel_e);
        }

        self.pending_polls.remove(&token);
        Ok(())
    }

    /// Register FD with specific interest
    #[inline]
    pub fn register(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        // Remove existing poll if any
        if let Some(&IoToken(old_token)) = self.fd_tokens.get(&fd) {
            self.submit_poll_remove(old_token)?;
        }

        let token = self.next_token();
        self.fd_tokens.insert(fd, IoToken(token));
        self.submit_poll_add(fd, interest.readable, interest.writable, token)?;

        Ok(())
    }

    /// Register FD with oneshot mode
    /// io-uring poll_add is inherently oneshot - completes once and needs re-arming
    #[inline]
    pub fn register_oneshot(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        // io-uring poll_add is already oneshot
        self.register(fd, interest)
    }

    /// Re-arm a oneshot FD
    #[inline]
    pub fn rearm_oneshot(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        let token = self.next_token();
        self.fd_tokens.insert(fd, IoToken(token));
        self.submit_poll_add(fd, interest.readable, interest.writable, token)?;
        Ok(())
    }

    /// Modify FD interest
    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: PollerEvent) -> crate::utils::VeloxResult<()> {
        // Cancel existing poll and submit new one
        if let Some(&IoToken(old_token)) = self.fd_tokens.get(&fd) {
            self.submit_poll_remove(old_token)?;
        }

        let token = self.next_token();
        self.fd_tokens.insert(fd, IoToken(token));
        self.submit_poll_add(fd, interest.readable, interest.writable, token)?;

        Ok(())
    }

    /// Delete FD from monitoring
    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> crate::utils::VeloxResult<()> {
        if let Some(IoToken(token)) = self.fd_tokens.remove(&fd) {
            self.submit_poll_remove(token)?;
        }
        Ok(())
    }

    /// Wake up the poller from another thread
    #[inline]
    pub fn notify(&self) -> crate::utils::VeloxResult<()> {
        let val: u64 = 1;
        unsafe {
            if libc::write(self.eventfd, &val as *const _ as *const _, 8) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }

    /// Poll for events using io-uring
    #[inline]
    pub fn poll_native(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<Vec<PlatformEvent>> {
        let should_flush = {
            let last_submit = *self.last_submit_time.lock();
            last_submit.elapsed() > Duration::from_micros(100) // 100µs batching window
        };
        
        if should_flush {
            self.flush_submissions()?;
        }

        // Use submit_and_wait with timeout
        if let Some(dur) = timeout {
            if dur > Duration::ZERO {
                let ts = types::Timespec::new()
                    .sec(dur.as_secs() as u64)
                    .nsec(dur.subsec_nanos() as u32);
                
                let timeout_e = opcode::Timeout::new(&ts).build().user_data(0);
                unsafe { let _ = self.ring.submission().push(&timeout_e); }
            }
        }
        
        let want = if timeout == Some(Duration::ZERO) { 0 } else { 1 };
        let _ = self.ring.submit_and_wait(want);

        // Collect completions first to avoid borrow issues
        let completions: Vec<(u64, i32)> = {
            let cq = self.ring.completion();
            cq.map(|cqe| (cqe.user_data(), cqe.result())).collect()
        };

        let mut events = Vec::with_capacity(completions.len());
        let mut need_rearm_eventfd = false;
        
        // Process collected completions
        for (token, result) in completions {
            // Skip timeout completions and cancellation completions
            if token == 0 {
                continue;
            }

            // Handle eventfd wakeup
            if token == self.eventfd_token {
                // Drain the eventfd
                let mut buf: u64 = 0;
                unsafe {
                    let _ = libc::read(self.eventfd, &mut buf as *mut _ as *mut _, 8);
                }
                need_rearm_eventfd = true;
                continue;
            }

            // Get the pending poll info
            if let Some(pending) = self.pending_polls.remove(&token) {
                if result >= 0 {
                    let poll_events = result as u32;
                    events.push(PlatformEvent {
                        fd: pending.fd,
                        readable: (poll_events & libc::POLLIN as u32) != 0
                            || (poll_events & libc::POLLHUP as u32) != 0,
                        writable: (poll_events & libc::POLLOUT as u32) != 0,
                        error: (poll_events & libc::POLLERR as u32) != 0,
                    });

                    // Remove the fd -> token mapping since poll completed
                    self.fd_tokens.remove(&pending.fd);
                } else if result == -libc::ECANCELED {
                    // Poll was cancelled, ignore
                } else {
                    // Error on the FD
                    events.push(PlatformEvent {
                        fd: pending.fd,
                        readable: false,
                        writable: false,
                        error: true,
                    });
                    self.fd_tokens.remove(&pending.fd);
                }
            }
        }

        // Re-arm eventfd poll after processing completions
        if need_rearm_eventfd {
            self.eventfd_token = self.next_token();
            let _ = self.submit_poll_add(self.eventfd, true, false, self.eventfd_token);
        }

        Ok(events)
    }
    /// Submit an async read operation via io-uring
    /// Returns a token to track completion
    #[inline]
    pub fn submit_read(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        offset: Option<u64>,
    ) -> crate::utils::VeloxResult<IoToken> {
        use crate::constants::POLLER_BATCH_THRESHOLD;


        let token = self.next_token();
        let off = offset.unwrap_or(u64::MAX); // -1 for current position

        let read_e = opcode::Read::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as u32)
            .offset(off)
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&read_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: true,
                writable: false,
            },
        );

        self.pending_submissions.fetch_add(1, Ordering::Relaxed);
        if self.pending_submissions.load(Ordering::Relaxed) >= POLLER_BATCH_THRESHOLD {
            self.flush_submissions()?;
        }
        Ok(IoToken(token))
    }

    #[inline]
    fn flush_submissions(&mut self) -> io::Result<()> {
        if self.pending_submissions.load(Ordering::Relaxed) > 0 {
            self.ring.submit()?;
            self.pending_submissions.store(0, Ordering::Relaxed);
            *self.last_submit_time.lock() = std::time::Instant::now();
        }
        Ok(())
    }

    /// Submit an async write operation via io-uring
    #[inline]
    pub fn submit_write(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        offset: Option<u64>,
    ) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();
        let off = offset.unwrap_or(u64::MAX);

        let write_e = opcode::Write::new(types::Fd(fd), buf.as_ptr(), buf.len() as u32)
            .offset(off)
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&write_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: false,
                writable: true,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async recv operation via io-uring
    #[inline]
    pub fn submit_recv(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();

        let recv_e = opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&recv_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: true,
                writable: false,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async send operation via io-uring
    #[inline]
    pub fn submit_send(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        flags: i32,
    ) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();

        let send_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&send_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: false,
                writable: true,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async accept operation via io-uring
    #[inline]
    pub fn submit_accept(&mut self, fd: RawFd) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();

        let accept_e = opcode::Accept::new(types::Fd(fd), std::ptr::null_mut(), std::ptr::null_mut())
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&accept_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: true,
                writable: false,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async connect operation via io-uring
    #[inline]
    pub fn submit_connect(
        &mut self,
        fd: RawFd,
        addr: SocketAddr,
    ) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();
        let sock_addr: socket2::SockAddr = addr.into();

        let connect_e = opcode::Connect::new(
            types::Fd(fd),
            sock_addr.as_ptr() as *const _,
            sock_addr.len(),
        )
        .build()
        .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&connect_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd,
                readable: false,
                writable: true,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async close operation via io-uring
    #[inline]
    pub fn submit_close(&mut self, fd: RawFd) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();

        let close_e = opcode::Close::new(types::Fd(fd))
            .build()
            .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&close_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Submit an async sendfile/splice operation via io-uring
    /// Uses splice for optimal zero-copy file transfer
    #[inline]
    pub fn submit_sendfile(
        &mut self,
        out_fd: RawFd,
        in_fd: RawFd,
        offset: u64,
        count: usize,
    ) -> crate::utils::VeloxResult<IoToken> {
        let token = self.next_token();

        let splice_e = opcode::Splice::new(
            types::Fd(in_fd),
            offset as i64,
            types::Fd(out_fd),
            -1, // Current position for output
            count as u32,
        )
        .build()
        .user_data(token);

        unsafe {
            self.ring
                .submission()
                .push(&splice_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ full"))?;
        }

        self.pending_polls.insert(
            token,
            PendingPoll {
                fd: out_fd,
                readable: false,
                writable: true,
            },
        );

        let _ = self.ring.submit();
        Ok(IoToken(token))
    }

    /// Cancel an in-flight io-uring operation
    #[inline]
    pub fn cancel_operation(&mut self, target_token: IoToken) -> crate::utils::VeloxResult<()> {
        let cancel_e = opcode::AsyncCancel::new(target_token.0)
            .build()
            .user_data(0); // Don't track cancellation completion

        unsafe {
            let _ = self.ring.submission().push(&cancel_e);
        }

        self.pending_polls.remove(&target_token.0);
        let _ = self.ring.submit();
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for LoopPoller {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.eventfd);
        }
    }
}

// ============================================================================
// Non-Linux: Stub implementation (future Tokio integration)
// ============================================================================

#[cfg(not(target_os = "linux"))]
pub struct LoopPoller {
    fd_interests: FxHashMap<RawFd, FdInterest>,
}

#[cfg(not(target_os = "linux"))]
impl LoopPoller {
    pub fn new() -> crate::utils::VeloxResult<Self> {
        Ok(Self {
            fd_interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
        })
    }

    #[inline]
    pub fn register(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        let fd_interest = FdInterest::new(interest.readable, interest.writable);
        self.fd_interests.insert(fd, fd_interest);
        Ok(())
    }

    #[inline]
    pub fn register_oneshot(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        self.register(fd, interest)
    }

    #[inline]
    pub fn rearm_oneshot(
        &mut self,
        fd: RawFd,
        interest: PollerEvent,
    ) -> crate::utils::VeloxResult<()> {
        self.modify(fd, interest)
    }

    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: PollerEvent) -> crate::utils::VeloxResult<()> {
        let new_interest = FdInterest::new(interest.readable, interest.writable);
        self.fd_interests.insert(fd, new_interest);
        Ok(())
    }

    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> crate::utils::VeloxResult<()> {
        self.fd_interests.remove(&fd);
        Ok(())
    }

    #[inline]
    pub fn notify(&self) -> crate::utils::VeloxResult<()> {
        Ok(())
    }

    #[inline]
    pub fn poll_native(
        &mut self,
        _timeout: Option<std::time::Duration>,
    ) -> crate::utils::VeloxResult<Vec<PlatformEvent>> {
        // TODO: Implement via Tokio for non-Linux platforms
        Ok(Vec::new())
    }

    #[inline]
    pub fn is_registered(&self, fd: RawFd) -> bool {
        self.fd_interests.contains_key(&fd)
    }
}
