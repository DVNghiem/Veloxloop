//! Traits defining the IO backend interface.
//!
//! These traits provide a unified interface for both completion-based (io-uring, IOCP)
//! and readiness-based (epoll, kqueue) IO backends.

use std::os::fd::RawFd;
use std::time::Duration;

use super::{Interest, IoEvent, OpCompletion, OpToken, BackendType};

/// Main trait for IO backends.
/// 
/// This trait provides a unified interface that works for both:
/// - Completion-based backends (io-uring, IOCP): Operations are submitted and
///   completion events are retrieved.
/// - Readiness-based backends (epoll, kqueue): FDs are registered for interest
///   and readiness events are retrieved.
pub trait IoBackend: Send + Sync {
    /// Get the backend type
    fn backend_type(&self) -> BackendType;

    /// Check if this is a completion-based backend
    #[inline]
    fn is_completion_based(&self) -> bool {
        self.backend_type().is_completion_based()
    }

    // ==================== FD Registration (Readiness-based) ====================

    /// Register a file descriptor for monitoring.
    /// For completion-based backends, this may be a no-op or used for
    /// maintaining internal state.
    fn register(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()>;

    /// Modify the interest for a registered file descriptor.
    fn modify(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()>;

    /// Unregister a file descriptor.
    fn unregister(&mut self, fd: RawFd) -> std::io::Result<()>;

    // ==================== Event Polling ====================

    /// Wait for events with an optional timeout.
    /// Returns readiness events for readiness-based backends,
    /// or completion events for completion-based backends.
    fn poll(&mut self, timeout: Option<Duration>) -> std::io::Result<Vec<IoEvent>>;

    /// Wake up the poller from another thread.
    fn notify(&self) -> std::io::Result<()>;

    // ==================== Async Operations (Completion-based) ====================

    /// Submit a read operation.
    /// Returns a token to track the operation.
    /// For readiness-based backends, this may register for readability.
    fn submit_read(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        offset: Option<u64>,
    ) -> std::io::Result<OpToken>;

    /// Submit a write operation.
    /// For readiness-based backends, this may register for writability.
    fn submit_write(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        offset: Option<u64>,
    ) -> std::io::Result<OpToken>;

    /// Submit a recv operation.
    fn submit_recv(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> std::io::Result<OpToken>;

    /// Submit a send operation.
    fn submit_send(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        flags: i32,
    ) -> std::io::Result<OpToken>;

    /// Submit an accept operation.
    fn submit_accept(&mut self, fd: RawFd) -> std::io::Result<OpToken>;

    /// Submit a connect operation.
    fn submit_connect(
        &mut self,
        fd: RawFd,
        addr: std::net::SocketAddr,
    ) -> std::io::Result<OpToken>;

    /// Submit a close operation.
    /// For io-uring, this uses IORING_OP_CLOSE.
    /// For readiness-based backends, this calls close() directly.
    fn submit_close(&mut self, fd: RawFd) -> std::io::Result<OpToken>;

    /// Cancel a pending operation.
    fn cancel(&mut self, token: OpToken) -> std::io::Result<()>;

    // ==================== Completion Retrieval ====================

    /// Get completed operations.
    /// For readiness-based backends, this returns empty as completions
    /// are handled via the poll() interface.
    fn get_completions(&mut self) -> Vec<OpCompletion>;

    // ==================== Query Methods ====================

    /// Check if a file descriptor is registered.
    fn is_registered(&self, fd: RawFd) -> bool;

    /// Get the number of pending operations.
    fn pending_ops(&self) -> usize;
}

/// Extension trait for backends that support advanced features.
pub trait IoBackendExt: IoBackend {
    /// Submit a sendfile operation.
    fn submit_sendfile(
        &mut self,
        out_fd: RawFd,
        in_fd: RawFd,
        offset: u64,
        count: usize,
    ) -> std::io::Result<OpToken>;

    /// Submit a recvfrom operation (UDP).
    fn submit_recvfrom(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> std::io::Result<OpToken>;

    /// Submit a sendto operation (UDP).
    fn submit_sendto(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        addr: std::net::SocketAddr,
        flags: i32,
    ) -> std::io::Result<OpToken>;

    /// Link operations together (io-uring specific).
    /// The second operation only executes if the first succeeds.
    fn link_ops(&mut self, first: OpToken, second: OpToken) -> std::io::Result<()>;

    /// Submit a batch of operations atomically.
    fn submit_batch(&mut self, ops: Vec<super::IoOp>) -> std::io::Result<Vec<OpToken>>;
}

/// Builder pattern for creating backends with specific configuration.
#[derive(Debug, Clone, Default)]
pub struct BackendConfig {
    /// Number of submission queue entries (io-uring)
    pub sq_entries: Option<u32>,
    /// Number of completion queue entries (io-uring)
    pub cq_entries: Option<u32>,
    /// Enable kernel polling (io-uring SQPOLL)
    pub kernel_poll: bool,
    /// Enable submission queue polling (io-uring)
    pub sq_poll: bool,
    /// Idle timeout for SQ polling thread (milliseconds)
    pub sq_poll_idle: Option<u32>,
    /// Enable registered files (io-uring)
    pub registered_files: bool,
    /// Maximum number of registered files
    pub max_registered_files: Option<u32>,
    /// Enable registered buffers (io-uring)
    pub registered_buffers: bool,
    /// Maximum number of registered buffers
    pub max_registered_buffers: Option<u32>,
}

impl BackendConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sq_entries(mut self, entries: u32) -> Self {
        self.sq_entries = Some(entries);
        self
    }

    pub fn with_cq_entries(mut self, entries: u32) -> Self {
        self.cq_entries = Some(entries);
        self
    }

    pub fn with_kernel_poll(mut self, enabled: bool) -> Self {
        self.kernel_poll = enabled;
        self
    }

    pub fn with_sq_poll(mut self, enabled: bool) -> Self {
        self.sq_poll = enabled;
        self
    }
}
