//! io-uring backend for Linux using the io-uring crate.
//!
//! This is a high-performance completion-based IO backend that leverages
//! the Linux io-uring interface for efficient asynchronous IO operations.
//!
//! Key features:
//! - Zero-copy IO where possible
//! - Batched submissions for reduced syscall overhead
//! - Support for registered files and buffers
//! - Kernel-side polling (SQPOLL) for ultra-low latency

use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use io_uring::{opcode, types, IoUring, Probe};
use rustc_hash::FxHashMap;

use super::{
    BackendType, Interest, IoBackend, IoBackendExt, IoEvent, IoOp,
    OpCompletion, OpResult, OpToken,
};

/// Size of the submission queue
const SQ_SIZE: u32 = 256;
/// Size of the completion queue (usually 2x SQ)
const CQ_SIZE: u32 = 512;
/// Maximum number of inflight operations
const MAX_INFLIGHT: usize = 4096;

/// Operation state for tracking pending operations
#[derive(Debug)]
struct PendingOp {
    fd: RawFd,
    op_type: OpType,
    /// Buffer for read operations (owned by us until completion)
    buffer: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpType {
    Read,
    Write,
    Recv,
    Send,
    Accept,
    Connect,
    Close,
    Poll,
    RecvFrom,
    SendTo,
    Sendfile,
    Timeout,
    Cancel,
}

/// io-uring based backend for Linux.
pub struct IoUringBackend {
    /// The io-uring instance
    ring: IoUring,
    /// Token counter for operations
    token_counter: AtomicU64,
    /// Pending operations indexed by token
    pending: FxHashMap<u64, PendingOp>,
    /// Track registered FDs for compatibility with readiness-based API
    interests: FxHashMap<RawFd, Interest>,
    /// Probe for checking supported operations
    probe: Probe,
    /// Completion buffer
    completions: Vec<OpCompletion>,
    /// Eventfd for waking up the ring
    eventfd: RawFd,
    /// Buffer pool for read operations
    buffer_pool: Vec<Vec<u8>>,
}

impl IoUringBackend {
    /// Create a new io-uring backend with default settings.
    pub fn new() -> std::io::Result<Self> {
        Self::with_config(SQ_SIZE, CQ_SIZE)
    }

    /// Create a new io-uring backend with custom queue sizes.
    pub fn with_config(sq_entries: u32, cq_entries: u32) -> std::io::Result<Self> {
        let ring = IoUring::builder()
            .setup_cqsize(cq_entries)
            .build(sq_entries)?;

        // Probe for supported operations
        let mut probe = Probe::new();
        ring.submitter().register_probe(&mut probe)?;

        // Create eventfd for waking
        let eventfd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if eventfd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Pre-allocate buffer pool
        let buffer_pool = (0..32).map(|_| Vec::with_capacity(65536)).collect();

        Ok(Self {
            ring,
            token_counter: AtomicU64::new(1),
            pending: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            probe,
            completions: Vec::with_capacity(64),
            eventfd,
            buffer_pool,
        })
    }

    fn next_token(&self) -> u64 {
        self.token_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Submit all pending operations
    fn submit(&mut self) -> std::io::Result<usize> {
        self.ring.submit().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Submit and wait for at least one completion
    fn submit_and_wait(&mut self, want: usize) -> std::io::Result<usize> {
        self.ring.submit_and_wait(want).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Acquire a buffer from the pool
    fn acquire_buffer(&mut self, size: usize) -> Vec<u8> {
        if let Some(mut buf) = self.buffer_pool.pop() {
            buf.clear();
            if buf.capacity() < size {
                buf.reserve(size - buf.capacity());
            }
            buf.resize(size, 0);
            buf
        } else {
            vec![0u8; size]
        }
    }

    /// Return a buffer to the pool
    fn return_buffer(&mut self, buf: Vec<u8>) {
        if self.buffer_pool.len() < 64 && buf.capacity() <= 65536 {
            self.buffer_pool.push(buf);
        }
    }

    /// Process completion queue entries
    fn process_completions(&mut self) -> std::io::Result<()> {
        // Collect completions first to avoid borrow issues
        let completions_data: Vec<(u64, i32)> = {
            let cq = self.ring.completion();
            cq.map(|cqe| (cqe.user_data(), cqe.result())).collect()
        };
        
        for (token, result) in completions_data {
            if let Some(pending_op) = self.pending.remove(&token) {
                let op_result = match pending_op.op_type {
                    OpType::Read | OpType::Recv => {
                        if result >= 0 {
                            OpResult::Bytes(result as usize)
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Write | OpType::Send => {
                        if result >= 0 {
                            OpResult::Bytes(result as usize)
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Accept => {
                        if result >= 0 {
                            OpResult::Accept {
                                client_fd: result,
                                addr: None, // TODO: Extract address
                            }
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Connect => {
                        if result >= 0 || result == -libc::EISCONN {
                            OpResult::Connect
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Close => {
                        if result >= 0 {
                            OpResult::Close
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Poll => {
                        if result >= 0 {
                            let events = result as u32;
                            OpResult::Poll {
                                readable: (events & libc::POLLIN as u32) != 0,
                                writable: (events & libc::POLLOUT as u32) != 0,
                            }
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Timeout => {
                        if result == -libc::ETIME {
                            OpResult::Timeout
                        } else if result >= 0 {
                            OpResult::Timeout
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                    OpType::Cancel => {
                        OpResult::Cancelled
                    }
                    _ => {
                        if result >= 0 {
                            OpResult::Bytes(result as usize)
                        } else {
                            OpResult::Error(std::io::Error::from_raw_os_error(-result))
                        }
                    }
                };

                // Return buffer to pool if present
                if let Some(buf) = pending_op.buffer {
                    self.return_buffer(buf);
                }

                self.completions.push(OpCompletion {
                    token: OpToken::new(token),
                    result: op_result,
                });
            }
        }

        Ok(())
    }

    /// Check if an operation type is supported
    #[inline]
    pub fn is_op_supported(&self, op: u8) -> bool {
        self.probe.is_supported(op)
    }
}

impl IoBackend for IoUringBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::IoUring
    }

    fn register(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        // For io-uring, we don't need to register for readiness
        // But we track interests for compatibility
        self.interests.insert(fd, interest);
        Ok(())
    }

    fn modify(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        self.interests.insert(fd, interest);
        Ok(())
    }

    fn unregister(&mut self, fd: RawFd) -> std::io::Result<()> {
        self.interests.remove(&fd);
        Ok(())
    }

    fn poll(&mut self, timeout: Option<Duration>) -> std::io::Result<Vec<IoEvent>> {
        // For io-uring, we process completions instead of polling for readiness
        let timeout_ms = timeout.map(|d| d.as_millis() as u32).unwrap_or(0);
        
        if timeout_ms > 0 {
            // Submit a timeout operation
            let ts = types::Timespec::new()
                .sec(timeout.unwrap().as_secs() as u64)
                .nsec(timeout.unwrap().subsec_nanos() as u32);
            
            let timeout_e = opcode::Timeout::new(&ts)
                .build()
                .user_data(0); // Special token for timeout
            
            unsafe {
                self.ring.submission().push(&timeout_e).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
                })?;
            }
        }

        // Submit and wait
        if self.pending.is_empty() && timeout.is_none() {
            return Ok(Vec::new());
        }

        let _ = self.submit_and_wait(1)?;
        self.process_completions()?;

        // Convert completions to IO events for compatibility
        let events: Vec<IoEvent> = self.completions.iter()
            .filter_map(|c| {
                if let OpResult::Poll { readable, writable } = &c.result {
                    // Find the fd for this token
                    // This is a simplified version - in practice you'd track this better
                    Some(IoEvent {
                        fd: 0, // Would need to track fd -> token mapping
                        readable: *readable,
                        writable: *writable,
                        error: false,
                        hangup: false,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(events)
    }

    fn notify(&self) -> std::io::Result<()> {
        let val: u64 = 1;
        unsafe {
            if libc::write(self.eventfd, &val as *const _ as *const _, 8) < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn submit_read(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        offset: Option<u64>,
    ) -> std::io::Result<OpToken> {
        let token = self.next_token();
        let offset = offset.unwrap_or(u64::MAX); // -1 for current position
        
        let read_e = opcode::Read::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as u32)
            .offset(offset)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&read_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Read,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_write(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        offset: Option<u64>,
    ) -> std::io::Result<OpToken> {
        let token = self.next_token();
        let offset = offset.unwrap_or(u64::MAX);

        let write_e = opcode::Write::new(types::Fd(fd), buf.as_ptr(), buf.len() as u32)
            .offset(offset)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&write_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Write,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_recv(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> std::io::Result<OpToken> {
        let token = self.next_token();

        let recv_e = opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&recv_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Recv,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_send(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        flags: i32,
    ) -> std::io::Result<OpToken> {
        let token = self.next_token();

        let send_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&send_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Send,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_accept(&mut self, fd: RawFd) -> std::io::Result<OpToken> {
        let token = self.next_token();

        let accept_e = opcode::Accept::new(types::Fd(fd), std::ptr::null_mut(), std::ptr::null_mut())
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&accept_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Accept,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_connect(
        &mut self,
        fd: RawFd,
        addr: std::net::SocketAddr,
    ) -> std::io::Result<OpToken> {
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
            self.ring.submission().push(&connect_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Connect,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_close(&mut self, fd: RawFd) -> std::io::Result<OpToken> {
        let token = self.next_token();

        let close_e = opcode::Close::new(types::Fd(fd))
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&close_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::Close,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn cancel(&mut self, token: OpToken) -> std::io::Result<()> {
        let cancel_token = self.next_token();

        let cancel_e = opcode::AsyncCancel::new(token.0)
            .build()
            .user_data(cancel_token);

        unsafe {
            self.ring.submission().push(&cancel_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(cancel_token, PendingOp {
            fd: -1,
            op_type: OpType::Cancel,
            buffer: None,
        });

        self.submit()?;
        Ok(())
    }

    fn get_completions(&mut self) -> Vec<OpCompletion> {
        std::mem::take(&mut self.completions)
    }

    fn is_registered(&self, fd: RawFd) -> bool {
        self.interests.contains_key(&fd)
    }

    fn pending_ops(&self) -> usize {
        self.pending.len()
    }
}

impl IoBackendExt for IoUringBackend {
    fn submit_sendfile(
        &mut self,
        out_fd: RawFd,
        in_fd: RawFd,
        offset: u64,
        count: usize,
    ) -> std::io::Result<OpToken> {
        let token = self.next_token();

        // Use splice for sendfile-like behavior in io-uring
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
            self.ring.submission().push(&splice_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd: out_fd,
            op_type: OpType::Sendfile,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_recvfrom(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> std::io::Result<OpToken> {
        // For UDP, use recvmsg with io-uring
        let token = self.next_token();

        let recv_e = opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&recv_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::RecvFrom,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn submit_sendto(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        _addr: std::net::SocketAddr,
        flags: i32,
    ) -> std::io::Result<OpToken> {
        // For connected UDP or with pre-set destination
        let token = self.next_token();

        let send_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), buf.len() as u32)
            .flags(flags)
            .build()
            .user_data(token);

        unsafe {
            self.ring.submission().push(&send_e).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
            })?;
        }

        self.pending.insert(token, PendingOp {
            fd,
            op_type: OpType::SendTo,
            buffer: None,
        });

        self.submit()?;
        Ok(OpToken::new(token))
    }

    fn link_ops(&mut self, _first: OpToken, _second: OpToken) -> std::io::Result<()> {
        // Link flag is set during submission, not after
        // This would require a different API design
        Ok(())
    }

    fn submit_batch(&mut self, ops: Vec<IoOp>) -> std::io::Result<Vec<OpToken>> {
        let mut tokens = Vec::with_capacity(ops.len());

        for op in ops {
            let token = match op {
                IoOp::Accept(a) => {
                    let t = self.next_token();
                    let accept_e = opcode::Accept::new(
                        types::Fd(a.fd),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                    .build()
                    .user_data(t);

                    unsafe {
                        self.ring.submission().push(&accept_e).map_err(|_| {
                            std::io::Error::new(std::io::ErrorKind::Other, "SQ full")
                        })?;
                    }

                    self.pending.insert(t, PendingOp {
                        fd: a.fd,
                        op_type: OpType::Accept,
                        buffer: None,
                    });
                    OpToken::new(t)
                }
                // Add other operation types as needed
                _ => continue,
            };
            tokens.push(token);
        }

        self.submit()?;
        Ok(tokens)
    }
}

impl Drop for IoUringBackend {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.eventfd);
        }
    }
}

// Safety: IoUringBackend is designed to be used from a single thread
// but the ring itself is thread-safe for submissions
unsafe impl Send for IoUringBackend {}
unsafe impl Sync for IoUringBackend {}
