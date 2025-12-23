//! Poll-based backend using platform-specific primitives.
//! 
//! This backend is used as a fallback when io-uring is not available.
//! On Linux: Uses direct epoll for best performance
//! On other platforms: Stub for future Tokio integration

use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rustc_hash::FxHashMap;

use super::{
    BackendType, Interest, IoBackend, IoEvent, OpCompletion, OpToken,
};

/// Poll-based backend for readiness-based IO.
pub struct PollBackend {
    /// The underlying poller (epoll on Linux, stub on other platforms)
    #[cfg(target_os = "linux")]
    epoll: crate::epoll::Epoll,
    
    /// Track registered FDs and their interests
    interests: FxHashMap<RawFd, Interest>,
    
    /// Token counter for operations
    token_counter: AtomicU64,
}

impl PollBackend {
    pub fn new() -> std::io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                epoll: crate::epoll::Epoll::new()?,
                interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
                token_counter: AtomicU64::new(1),
            })
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                interests: FxHashMap::with_capacity_and_hasher(256, Default::default()),
                token_counter: AtomicU64::new(1),
            })
        }
    }
    
    fn next_token(&self) -> OpToken {
        OpToken::new(self.token_counter.fetch_add(1, Ordering::Relaxed))
    }
}

impl IoBackend for PollBackend {
    fn backend_type(&self) -> BackendType {
        #[cfg(target_os = "linux")]
        { BackendType::Epoll }
        
        #[cfg(target_os = "macos")]
        { BackendType::Kqueue }
        
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        { BackendType::Poll }
    }
    
    fn register(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            let epoll_interest = crate::epoll::Interest::new(interest.readable, interest.writable);
            self.epoll.add(fd, epoll_interest)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement via Tokio for non-Linux platforms
            let _ = fd;
        }
        
        self.interests.insert(fd, interest);
        Ok(())
    }
    
    fn modify(&mut self, fd: RawFd, interest: Interest) -> std::io::Result<()> {
        // Skip syscall if interest unchanged
        if let Some(&current) = self.interests.get(&fd) {
            if current == interest {
                return Ok(());
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            let epoll_interest = crate::epoll::Interest::new(interest.readable, interest.writable);
            self.epoll.modify(fd, epoll_interest)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement via Tokio for non-Linux platforms
            let _ = fd;
        }
        
        self.interests.insert(fd, interest);
        Ok(())
    }
    
    fn unregister(&mut self, fd: RawFd) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.epoll.delete(fd)?;
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement via Tokio for non-Linux platforms
            let _ = fd;
        }
        
        self.interests.remove(&fd);
        Ok(())
    }
    
    fn poll(&mut self, timeout: Option<Duration>) -> std::io::Result<Vec<IoEvent>> {
        #[cfg(target_os = "linux")]
        {
            let events = self.epoll.wait(timeout)?;
            Ok(events.into_iter().map(|e| IoEvent {
                fd: e.fd,
                readable: e.readable,
                writable: e.writable,
                error: false,
                hangup: false,
            }).collect())
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement via Tokio for non-Linux platforms
            let _ = timeout;
            Ok(Vec::new())
        }
    }
    
    fn notify(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.epoll.notify()
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement via Tokio for non-Linux platforms
            Ok(())
        }
    }
    
    // For readiness-based backends, submit_* methods register for interest
    // and return immediately. The actual IO happens in the event loop.
    
    fn submit_read(
        &mut self,
        fd: RawFd,
        _buf: &mut [u8],
        _offset: Option<u64>,
    ) -> std::io::Result<OpToken> {
        // Register for readability
        let current = self.interests.get(&fd).copied().unwrap_or(Interest::none());
        let new_interest = Interest::new(true, current.writable);
        
        if self.interests.contains_key(&fd) {
            self.modify(fd, new_interest)?;
        } else {
            self.register(fd, new_interest)?;
        }
        
        Ok(self.next_token())
    }
    
    fn submit_write(
        &mut self,
        fd: RawFd,
        _buf: &[u8],
        _offset: Option<u64>,
    ) -> std::io::Result<OpToken> {
        let current = self.interests.get(&fd).copied().unwrap_or(Interest::none());
        let new_interest = Interest::new(current.readable, true);
        
        if self.interests.contains_key(&fd) {
            self.modify(fd, new_interest)?;
        } else {
            self.register(fd, new_interest)?;
        }
        
        Ok(self.next_token())
    }
    
    fn submit_recv(
        &mut self,
        fd: RawFd,
        _buf: &mut [u8],
        _flags: i32,
    ) -> std::io::Result<OpToken> {
        self.submit_read(fd, &mut [], None)
    }
    
    fn submit_send(
        &mut self,
        fd: RawFd,
        _buf: &[u8],
        _flags: i32,
    ) -> std::io::Result<OpToken> {
        self.submit_write(fd, &[], None)
    }
    
    fn submit_accept(&mut self, fd: RawFd) -> std::io::Result<OpToken> {
        // For accept, we register for readability on the listening socket
        self.register(fd, Interest::readable())?;
        Ok(self.next_token())
    }
    
    fn submit_connect(
        &mut self,
        fd: RawFd,
        addr: std::net::SocketAddr,
    ) -> std::io::Result<OpToken> {
        use socket2::SockAddr;
        
        // Initiate non-blocking connect
        let sock_addr: SockAddr = addr.into();
        
        unsafe {
            let ret = libc::connect(
                fd,
                sock_addr.as_ptr() as *const libc::sockaddr,
                sock_addr.len(),
            );
            
            if ret == 0 {
                // Connected immediately
                return Ok(self.next_token());
            }
            
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EINPROGRESS) | Some(libc::EWOULDBLOCK) => {
                    // Connection in progress, register for writability
                    self.register(fd, Interest::writable())?;
                    Ok(self.next_token())
                }
                _ => Err(err),
            }
        }
    }
    
    fn submit_close(&mut self, fd: RawFd) -> std::io::Result<OpToken> {
        // For poll backend, we just close directly
        let _ = self.unregister(fd);
        unsafe {
            if libc::close(fd) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(self.next_token())
    }
    
    fn cancel(&mut self, _token: OpToken) -> std::io::Result<()> {
        // For readiness-based backends, cancellation is done by
        // unregistering the FD or ignoring the callback
        Ok(())
    }
    
    fn get_completions(&mut self) -> Vec<OpCompletion> {
        // Readiness-based backends don't have true completions
        Vec::new()
    }
    
    fn is_registered(&self, fd: RawFd) -> bool {
        self.interests.contains_key(&fd)
    }
    
    fn pending_ops(&self) -> usize {
        // For readiness-based backends, pending ops = registered FDs
        self.interests.len()
    }
}

// Safety: PollBackend uses thread-safe primitives
unsafe impl Send for PollBackend {}
unsafe impl Sync for PollBackend {}
