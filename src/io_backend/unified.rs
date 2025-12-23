//! Unified poller that abstracts over different IO backends.
//!
//! This module provides a unified interface that automatically selects
//! the best available backend:
//! - io-uring on Linux (default, highest performance)
//! - polling crate on macOS/BSD/Windows (future native implementations)
//!
//! The design allows for both completion-based and readiness-based
//! event processing through a common API.

use std::os::fd::RawFd;
use std::time::Duration;

use crate::io_backend::{
    BackendType, Interest, IoBackend, IoEvent, OpCompletion, OpToken,
};
use crate::utils::VeloxResult;

/// Unified poller that can use different backends
pub struct UnifiedPoller {
    backend: Box<dyn IoBackend>,
    backend_type: BackendType,
}

impl UnifiedPoller {
    /// Create a new unified poller with the best available backend
    pub fn new() -> VeloxResult<Self> {
        let backend = crate::io_backend::create_backend()?;
        let backend_type = backend.backend_type();
        
        Ok(Self {
            backend,
            backend_type,
        })
    }

    /// Get the backend type being used
    #[inline]
    pub fn backend_type(&self) -> BackendType {
        self.backend_type
    }

    /// Check if using a completion-based backend (io-uring, IOCP)
    #[inline]
    pub fn is_completion_based(&self) -> bool {
        self.backend_type.is_completion_based()
    }

    // ==================== FD Registration ====================

    /// Register a file descriptor for monitoring
    #[inline]
    pub fn register(&mut self, fd: RawFd, interest: Interest) -> VeloxResult<()> {
        self.backend.register(fd, interest)?;
        Ok(())
    }

    /// Register for readability
    #[inline]
    pub fn register_readable(&mut self, fd: RawFd) -> VeloxResult<()> {
        self.register(fd, Interest::readable())
    }

    /// Register for writability
    #[inline]
    pub fn register_writable(&mut self, fd: RawFd) -> VeloxResult<()> {
        self.register(fd, Interest::writable())
    }

    /// Modify the interest for a registered file descriptor
    #[inline]
    pub fn modify(&mut self, fd: RawFd, interest: Interest) -> VeloxResult<()> {
        self.backend.modify(fd, interest)?;
        Ok(())
    }

    /// Unregister a file descriptor
    #[inline]
    pub fn unregister(&mut self, fd: RawFd) -> VeloxResult<()> {
        self.backend.unregister(fd)?;
        Ok(())
    }

    /// Check if a file descriptor is registered
    #[inline]
    pub fn is_registered(&self, fd: RawFd) -> bool {
        self.backend.is_registered(fd)
    }

    // ==================== Event Polling ====================

    /// Poll for events with an optional timeout
    #[inline]
    pub fn poll(&mut self, timeout: Option<Duration>) -> VeloxResult<Vec<IoEvent>> {
        Ok(self.backend.poll(timeout)?)
    }

    /// Wake up the poller from another thread
    #[inline]
    pub fn notify(&self) -> VeloxResult<()> {
        self.backend.notify()?;
        Ok(())
    }

    // ==================== Async Operations ====================

    /// Submit a read operation (completion-based backends only)
    #[inline]
    pub fn submit_read(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        offset: Option<u64>,
    ) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_read(fd, buf, offset)?)
    }

    /// Submit a write operation (completion-based backends only)
    #[inline]
    pub fn submit_write(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        offset: Option<u64>,
    ) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_write(fd, buf, offset)?)
    }

    /// Submit a recv operation
    #[inline]
    pub fn submit_recv(
        &mut self,
        fd: RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_recv(fd, buf, flags)?)
    }

    /// Submit a send operation
    #[inline]
    pub fn submit_send(
        &mut self,
        fd: RawFd,
        buf: &[u8],
        flags: i32,
    ) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_send(fd, buf, flags)?)
    }

    /// Submit an accept operation
    #[inline]
    pub fn submit_accept(&mut self, fd: RawFd) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_accept(fd)?)
    }

    /// Submit a connect operation
    #[inline]
    pub fn submit_connect(
        &mut self,
        fd: RawFd,
        addr: std::net::SocketAddr,
    ) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_connect(fd, addr)?)
    }

    /// Submit a close operation
    #[inline]
    pub fn submit_close(&mut self, fd: RawFd) -> VeloxResult<OpToken> {
        Ok(self.backend.submit_close(fd)?)
    }

    /// Cancel a pending operation
    #[inline]
    pub fn cancel(&mut self, token: OpToken) -> VeloxResult<()> {
        self.backend.cancel(token)?;
        Ok(())
    }

    /// Get completed operations (completion-based backends)
    #[inline]
    pub fn get_completions(&mut self) -> Vec<OpCompletion> {
        self.backend.get_completions()
    }

    /// Get the number of pending operations
    #[inline]
    pub fn pending_ops(&self) -> usize {
        self.backend.pending_ops()
    }
}

// Legacy compatibility layer for existing code
// This allows gradual migration from the old poller to the new unified poller

impl UnifiedPoller {
    /// Legacy method for deleting (same as unregister)
    #[inline]
    pub fn delete(&mut self, fd: RawFd) -> VeloxResult<()> {
        self.unregister(fd)
    }
}

unsafe impl Send for UnifiedPoller {}
unsafe impl Sync for UnifiedPoller {}
