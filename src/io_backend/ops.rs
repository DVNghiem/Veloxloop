//! IO operation definitions for the backend abstraction layer.
//! 
//! These types represent asynchronous IO operations that can be submitted
//! to completion-based backends (io-uring, IOCP) or emulated on readiness-based
//! backends (epoll, kqueue).

use std::os::fd::RawFd;
use std::net::SocketAddr;

/// Token to track submitted operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpToken(pub u64);

impl OpToken {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Types of IO operations
#[derive(Debug, Clone)]
pub enum IoOp {
    /// Accept a new connection
    Accept(AcceptOp),
    /// Connect to a remote address
    Connect(ConnectOp),
    /// Read data from a file descriptor
    Read(ReadOp),
    /// Write data to a file descriptor
    Write(WriteOp),
    /// Receive data from a socket
    Recv(RecvOp),
    /// Send data to a socket
    Send(SendOp),
    /// Receive from a UDP socket
    RecvFrom(RecvFromOp),
    /// Send to a UDP socket
    SendTo(SendToOp),
    /// Close a file descriptor
    Close(CloseOp),
    /// Shutdown a socket
    Shutdown(ShutdownOp),
    /// Poll for readiness (readiness-based backends)
    Poll(PollOp),
    /// Timeout operation
    Timeout(TimeoutOp),
    /// Cancel an operation
    Cancel(CancelOp),
    /// sendfile operation
    Sendfile(SendfileOp),
}

/// Accept operation
#[derive(Debug, Clone)]
pub struct AcceptOp {
    pub fd: RawFd,
    pub token: OpToken,
}

/// Connect operation
#[derive(Debug, Clone)]
pub struct ConnectOp {
    pub fd: RawFd,
    pub addr: SocketAddr,
    pub token: OpToken,
}

/// Read operation
#[derive(Debug, Clone)]
pub struct ReadOp {
    pub fd: RawFd,
    pub offset: Option<u64>,
    pub len: usize,
    pub token: OpToken,
}

/// Write operation
#[derive(Debug, Clone)]
pub struct WriteOp {
    pub fd: RawFd,
    pub offset: Option<u64>,
    pub token: OpToken,
}

/// Recv operation
#[derive(Debug, Clone)]
pub struct RecvOp {
    pub fd: RawFd,
    pub len: usize,
    pub flags: i32,
    pub token: OpToken,
}

/// Send operation
#[derive(Debug, Clone)]
pub struct SendOp {
    pub fd: RawFd,
    pub flags: i32,
    pub token: OpToken,
}

/// RecvFrom operation (UDP)
#[derive(Debug, Clone)]
pub struct RecvFromOp {
    pub fd: RawFd,
    pub len: usize,
    pub flags: i32,
    pub token: OpToken,
}

/// SendTo operation (UDP)
#[derive(Debug, Clone)]
pub struct SendToOp {
    pub fd: RawFd,
    pub addr: SocketAddr,
    pub flags: i32,
    pub token: OpToken,
}

/// Close operation
#[derive(Debug, Clone)]
pub struct CloseOp {
    pub fd: RawFd,
    pub token: OpToken,
}

/// Shutdown operation
#[derive(Debug, Clone)]
pub struct ShutdownOp {
    pub fd: RawFd,
    pub how: ShutdownHow,
    pub token: OpToken,
}

/// How to shutdown a socket
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownHow {
    Read,
    Write,
    Both,
}

impl ShutdownHow {
    #[cfg(unix)]
    pub fn to_libc(self) -> i32 {
        match self {
            ShutdownHow::Read => libc::SHUT_RD,
            ShutdownHow::Write => libc::SHUT_WR,
            ShutdownHow::Both => libc::SHUT_RDWR,
        }
    }
}

/// Poll operation (for readiness-based emulation)
#[derive(Debug, Clone)]
pub struct PollOp {
    pub fd: RawFd,
    pub interest: super::Interest,
    pub token: OpToken,
}

/// Timeout operation
#[derive(Debug, Clone)]
pub struct TimeoutOp {
    pub duration_ns: u64,
    pub token: OpToken,
}

/// Cancel operation
#[derive(Debug, Clone)]
pub struct CancelOp {
    pub target_token: OpToken,
    pub token: OpToken,
}

/// Sendfile operation
#[derive(Debug, Clone)]
pub struct SendfileOp {
    pub out_fd: RawFd,
    pub in_fd: RawFd,
    pub offset: u64,
    pub count: usize,
    pub token: OpToken,
}

/// Result of a completed operation
#[derive(Debug)]
pub struct OpCompletion {
    pub token: OpToken,
    pub result: OpResult,
}

/// Result types for different operations
#[derive(Debug)]
pub enum OpResult {
    /// Generic success with byte count
    Bytes(usize),
    /// Accept succeeded with new fd and address
    Accept {
        client_fd: RawFd,
        addr: Option<SocketAddr>,
    },
    /// RecvFrom succeeded with byte count and source address
    RecvFrom {
        len: usize,
        addr: SocketAddr,
    },
    /// Connect completed
    Connect,
    /// Close completed
    Close,
    /// Shutdown completed
    Shutdown,
    /// Poll completed (readiness ready)
    Poll {
        readable: bool,
        writable: bool,
    },
    /// Timeout expired
    Timeout,
    /// Operation cancelled
    Cancelled,
    /// Error occurred
    Error(std::io::Error),
}

impl OpResult {
    /// Check if this result is an error
    #[inline]
    pub fn is_error(&self) -> bool {
        matches!(self, OpResult::Error(_))
    }

    /// Get the byte count if this is a bytes result
    #[inline]
    pub fn bytes(&self) -> Option<usize> {
        match self {
            OpResult::Bytes(n) => Some(*n),
            OpResult::RecvFrom { len, .. } => Some(*len),
            _ => None,
        }
    }
}
