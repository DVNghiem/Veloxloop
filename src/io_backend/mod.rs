//! Platform-agnostic IO backend abstraction layer.
//! 
//! This module provides a unified interface for different IO backends:
//! - **Linux**: io-uring (highest performance, default on Linux)
//! - **macOS**: kqueue via polling crate (future native implementation)
//! - **Windows**: IOCP via polling crate (future native implementation)
//! - **Other**: polling crate fallback
//!
//! The abstraction is designed to support completion-based (io-uring, IOCP) and
//! readiness-based (epoll, kqueue) models through a unified async interface.

// Allow dead code until full integration is complete
#![allow(dead_code)]

mod ops;
mod traits;
mod unified;

#[cfg(target_os = "linux")]
mod uring;

#[cfg(not(target_os = "linux"))]
mod poll_backend;

pub use ops::*;
pub use traits::*;
pub use unified::UnifiedPoller;

#[cfg(target_os = "linux")]
pub use uring::IoUringBackend;

#[cfg(not(target_os = "linux"))]
pub use poll_backend::PollBackend;

use std::os::fd::RawFd;

/// Backend type enum for runtime detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// io-uring (Linux 5.1+)
    IoUring,
    /// epoll (Linux, fallback when io-uring unavailable)
    Epoll,
    /// kqueue (macOS, BSD)
    Kqueue,
    /// IOCP (Windows)
    Iocp,
    /// Generic polling (fallback)
    Poll,
}

impl BackendType {
    /// Get the current platform's preferred backend
    #[inline]
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            // On Linux, always use io-uring (with runtime fallback if unavailable)
            if is_io_uring_available() {
                return BackendType::IoUring;
            }
            BackendType::Epoll
        }

        #[cfg(target_os = "macos")]
        {
            BackendType::Kqueue
        }

        #[cfg(target_os = "windows")]
        {
            BackendType::Iocp
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            BackendType::Poll
        }
    }

    /// Check if this backend type is completion-based
    #[inline]
    pub fn is_completion_based(&self) -> bool {
        matches!(self, BackendType::IoUring | BackendType::Iocp)
    }

    /// Check if this backend type is readiness-based
    #[inline]
    pub fn is_readiness_based(&self) -> bool {
        !self.is_completion_based()
    }
}

/// Check if io-uring is available on the current system
#[cfg(target_os = "linux")]
fn is_io_uring_available() -> bool {
    use std::sync::OnceLock;
    
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    
    *AVAILABLE.get_or_init(|| {
        // Try to create a small io-uring instance to check availability
        // io-uring requires kernel 5.1+ and may be disabled in containers
        unsafe {
            // Use syscall directly to check if io_uring_setup is available
            let mut params = std::mem::zeroed::<libc::c_void>();
            let ret = libc::syscall(
                425,  // SYS_io_uring_setup on x86_64
                4u32,  // entries
                &mut params as *mut _,
            );
            if ret >= 0 {
                libc::close(ret as i32);
                true
            } else {
                false
            }
        }
    })
}

/// Create the appropriate backend for the current platform
pub fn create_backend() -> crate::utils::VeloxResult<Box<dyn IoBackend>> {
    #[cfg(target_os = "linux")]
    {
        // On Linux, always use io-uring
        Ok(Box::new(IoUringBackend::new()?))
    }

    #[cfg(not(target_os = "linux"))]
    {
        // On other platforms, use the polling crate
        Ok(Box::new(PollBackend::new()?))
    }
}

/// Event interests for a file descriptor
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Interest {
    pub readable: bool,
    pub writable: bool,
}

impl Interest {
    #[inline(always)]
    pub const fn new(readable: bool, writable: bool) -> Self {
        Self { readable, writable }
    }

    #[inline(always)]
    pub const fn readable() -> Self {
        Self { readable: true, writable: false }
    }

    #[inline(always)]
    pub const fn writable() -> Self {
        Self { readable: false, writable: true }
    }

    #[inline(always)]
    pub const fn both() -> Self {
        Self { readable: true, writable: true }
    }

    #[inline(always)]
    pub const fn none() -> Self {
        Self { readable: false, writable: false }
    }
}

/// A ready event from the backend
#[derive(Clone, Copy, Debug)]
pub struct IoEvent {
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
    pub error: bool,
    pub hangup: bool,
}

impl IoEvent {
    #[inline]
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            readable: false,
            writable: false,
            error: false,
            hangup: false,
        }
    }
}

/// Completion result for async operations
#[derive(Debug)]
pub enum CompletionResult {
    /// Operation completed successfully with byte count
    Success(usize),
    /// Operation completed with an error
    Error(std::io::Error),
    /// Operation would block (for readiness-based backends)
    WouldBlock,
}
