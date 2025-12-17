use slab::Slab;
use std::os::fd::RawFd;
use pyo3::prelude::*;

pub struct Handle {
    pub callback: Py<PyAny>,
    pub cancelled: bool,
}

pub struct IoHandles {
    readers: Slab<Handle>,
    writers: Slab<Handle>,
    // Mapping from FD to Slab Key for fast lookup
    // Since FDs can be arbitrary, we might need a HashMap or similar if FDs are sparse.
    // However, usually we can just use the FD directly if we are careful, OR
    // we use a HashMap<RawFd, usize> to map FD -> Slab Key.
    // For O(1) strictly, if max FDs is known, a Vec could work, but HashMap is safer.
    // Let's us a simple strategy:
    // We actually need to map FD -> (ReaderKey, WriterKey).
    fd_map: std::collections::HashMap<RawFd, (Option<usize>, Option<usize>)>,
}

impl IoHandles {
    pub fn new() -> Self {
        Self {
            readers: Slab::new(),
            writers: Slab::new(),
            fd_map: std::collections::HashMap::new(),
        }
    }

    pub fn add_reader(&mut self, fd: RawFd, callback: Py<PyAny>) {
        let entry = self.fd_map.entry(fd).or_insert((None, None));
        
        if let Some(key) = entry.0 {
            // Update existing
            if let Some(handle) = self.readers.get_mut(key) {
                handle.callback = callback;
                handle.cancelled = false;
            }
        } else {
            // Insert new
            let key = self.readers.insert(Handle {
                callback,
                cancelled: false,
            });
            entry.0 = Some(key);
        }
    }

    pub fn remove_reader(&mut self, fd: RawFd) -> bool {
        if let Some(entry) = self.fd_map.get_mut(&fd) {
            if let Some(key) = entry.0.take() {
                self.readers.remove(key);
                // If both reader and writer are gone, we could remove the map entry,
                // but for now keeping it is fine (minor memory) or we can clean up.
                if entry.1.is_none() {
                    self.fd_map.remove(&fd);
                }
                return true;
            }
        }
        false
    }

    pub fn add_writer(&mut self, fd: RawFd, callback: Py<PyAny>) {
         let entry = self.fd_map.entry(fd).or_insert((None, None));
        
        if let Some(key) = entry.1 {
            // Update existing
            if let Some(handle) = self.writers.get_mut(key) {
                handle.callback = callback;
                handle.cancelled = false;
            }
        } else {
            // Insert new
            let key = self.writers.insert(Handle {
                callback,
                cancelled: false,
            });
            entry.1 = Some(key);
        }
    }

    pub fn remove_writer(&mut self, fd: RawFd) -> bool {
         if let Some(entry) = self.fd_map.get_mut(&fd) {
            if let Some(key) = entry.1.take() {
                self.writers.remove(key);
                if entry.0.is_none() {
                    self.fd_map.remove(&fd);
                }
                return true;
            }
        }
        false
    }
    
    pub fn get_reader(&self, fd: RawFd) -> Option<&Handle> {
        self.fd_map.get(&fd)
            .and_then(|(r, _)| *r)
            .and_then(|key| self.readers.get(key))
    }

    pub fn get_writer(&self, fd: RawFd) -> Option<&Handle> {
        self.fd_map.get(&fd)
            .and_then(|(_, w)| *w)
            .and_then(|key| self.writers.get(key))
    }
}
