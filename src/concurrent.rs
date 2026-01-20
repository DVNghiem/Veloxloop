use dashmap::DashMap;
use crossbeam_channel::{unbounded, Sender, Receiver, TrySendError};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Uses crossbeam channels for high-performance concurrent callback scheduling.
pub struct ConcurrentCallbackQueue<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    len: AtomicUsize,
}

impl<T> ConcurrentCallbackQueue<T> {
    /// Create a new unbounded concurrent queue
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            sender,
            receiver,
            len: AtomicUsize::new(0),
        }
    }

    /// Push an item to the queue
    #[inline]
    pub fn push(&self, item: T) -> bool {
        match self.sender.try_send(item) {
            Ok(()) => {
                self.len.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(item)) => {
                // For unbounded queues, this shouldn't happen
                // For bounded queues, we block
                let _ = self.sender.send(item);
                self.len.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Try to pop an item from the queue
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.receiver.try_recv().ok().map(|item| {
            self.len.fetch_sub(1, Ordering::Relaxed);
            item
        })
    }

    /// Pop all items into a vector (drains the queue)
    pub fn drain_into(&self, target: &mut Vec<T>) {
        while let Some(item) = self.try_pop() {
            target.push(item);
        }
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len.load(Ordering::Relaxed) == 0
    }
}

impl<T> Default for ConcurrentCallbackQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A concurrent hash map optimized for integer keys (like file descriptors)
/// 
/// Wraps DashMap with convenience methods for the event loop use case.
pub struct ConcurrentIntMap<V> {
    inner: DashMap<i32, V, rustc_hash::FxBuildHasher>,
}

impl<V> ConcurrentIntMap<V> {
    /// Create a new concurrent map
    pub fn new() -> Self {
        Self {
            inner: DashMap::with_hasher(Default::default()),
        }
    }

    /// Create with initial capacity
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: DashMap::with_capacity_and_hasher(cap, Default::default()),
        }
    }

    /// Get a value
    #[inline]
    pub fn get(&self, key: &i32) -> Option<dashmap::mapref::one::Ref<'_, i32, V>> {
        self.inner.get(key)
    }

    /// Get a mutable reference
    #[inline]
    pub fn get_mut(&self, key: &i32) -> Option<dashmap::mapref::one::RefMut<'_, i32, V>> {
        self.inner.get_mut(key)
    }

    /// Remove a value
    #[inline]
    pub fn remove(&self, key: &i32) -> Option<(i32, V)> {
        self.inner.remove(key)
    }

    /// Entry API for conditional insertion/update
    #[inline]
    pub fn entry(&self, key: i32) -> dashmap::Entry<'_, i32, V> {
        self.inner.entry(key)
    }
}

impl<V> Default for ConcurrentIntMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic counter with various ordering options
pub struct AtomicCounter {
    value: AtomicU64,
}

impl AtomicCounter {
    pub const fn new(initial: u64) -> Self {
        Self {
            value: AtomicU64::new(initial),
        }
    }

    #[inline]
    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }

    #[inline]
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A lock-free flag for coordination
pub struct AtomicFlag {
    value: AtomicBool,
}

impl AtomicFlag {
    pub const fn new(initial: bool) -> Self {
        Self {
            value: AtomicBool::new(initial),
        }
    }

    #[inline]
    pub fn set(&self) {
        self.value.store(true, Ordering::Release);
    }

    #[inline]
    pub fn clear(&self) {
        self.value.store(false, Ordering::Release);
    }

    #[inline]
    pub fn is_set(&self) -> bool {
        self.value.load(Ordering::Acquire)
    }
}

impl Default for AtomicFlag {
    fn default() -> Self {
        Self::new(false)
    }
}
