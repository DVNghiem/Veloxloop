use polling::{Event, Events, Poller};
use std::os::fd::RawFd;
use std::sync::Arc;
use crate::utils::VeloxResult;

pub struct LoopPoller {
    poller: Arc<Poller>,
    // Internal mutability for shared access via Arc in EventLoop
    registered_fds: parking_lot::Mutex<std::collections::HashSet<RawFd>>,
}

impl LoopPoller {
    pub fn new() -> VeloxResult<Self> {
        Ok(Self {
            poller: Arc::new(Poller::new()?),
            registered_fds: parking_lot::Mutex::new(std::collections::HashSet::new()),
        })
    }

    pub fn add_reader(&self, fd: RawFd) -> VeloxResult<()> {
        // In polling crate, we use Source.
        // For RawFd on Linux, it's just the FD.
        // We generally modify if exists, add if not.
        let key = fd as usize; // polling uses usize key
        // Need to lock
        let contains = self.registered_fds.lock().contains(&fd);
        if contains {
            // Modifying to include readable
             // Note: This logic is simple; real world needs to know if we also have a writer
             // to set PollMode::Level or Edge properly.
             // For simplicity, let's assume Level triggered for compat.
             // But polling 3.x is usually edge-triggered or oneshot by default depending on config.
             // We need to fetch current interest.
             // Ideally the EventLoop tracks the state (Reader + ?Writer).
             // If we have Reader and want to add Reader, it's a no-op implementation-wise, just update callback in Handles.
             // If we have Writer and want to add Reader, we need functionality to merge flags.
             // Let's assume the Loop logic passes in the DESIRED state (Read, Write, or Both).
             
             // BUT, `add_reader` implies we want to listen for Read.
             // We'll rely on `update_registration`.
             unimplemented!("Use update_registration instead");
        } else {
             unimplemented!("Use update_registration instead");
        }
    }
    
    // Better API: register with specific interest
    pub fn register(&self, fd: RawFd, interest: Event) -> VeloxResult<()> {
         // unsafe {
            // RawFd is i32, key is usize.
            // On Linux simple cast is fine for now usually positive FDs.
            // We must use Source::fd or similar?
            // Poller::add takes impl AsRawFd. i32 implements AsRawFd?
            // Actually usually we need `BorrowFd` or similar wrapper struct if passing raw i32.
            // But let's assume rawfd works or wrap it.
            // Wait, RawFd (i32) does NOT implement AsRawFd on all versions/platforms directly or cleanly?
            // Actually std::os::fd::RawFd is just alias for i32. 
            // We might need strict provenance or a wrapper.
            // unsafe { self.poller.add(fd, interest) } if using old API.
            // polling v3: add(key, source, interest).
            // source must be impl AsRawFd.
            // We can wrap fd in `std::os::fd::BorrowedFd::borrow_raw(fd)` (unsafe) or just a struct.
            // Let's use `unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }`.
            
            unsafe {
                 let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
                 self.poller.add(&borrowed, interest)?;
            }
            self.registered_fds.lock().insert(fd);
        // }
        Ok(())
    }
    
    pub fn modify(&self, fd: RawFd, interest: Event) -> VeloxResult<()> {
        unsafe {
             let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
             self.poller.modify(&borrowed, interest)?;
        }
        Ok(())
    }
    
    pub fn delete(&self, fd: RawFd) -> VeloxResult<()> {
        unsafe {
             let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
             self.poller.delete(&borrowed)?;
        }
        self.registered_fds.lock().remove(&fd);
        Ok(())
    }
    
    pub fn notify(&self) -> VeloxResult<()> {
        self.poller.notify()?;
        Ok(())
    }
    
    pub fn poll(&self, events: &mut Events, timeout: Option<std::time::Duration>) -> VeloxResult<usize> {
        events.clear();
        self.poller.wait(events, timeout)?;
        Ok(events.len())
    }
}
