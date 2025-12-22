use bytes::BytesMut;
use std::cell::RefCell;

/// Default buffer size for the pool (128 KB)
const BUFFER_SIZE: usize = 128 * 1024;
/// Maximum number of buffers to keep in the pool per thread
const MAX_POOL_SIZE: usize = 64;

thread_local! {
    static POOL: RefCell<Vec<BytesMut>> = RefCell::new(Vec::with_capacity(MAX_POOL_SIZE));
}

/// A simple thread-local buffer pool for managing BytesMut buffers.
pub struct BufferPool;

impl BufferPool {
    /// Acquire a buffer from the pool or create a new one.
    pub fn acquire() -> BytesMut {
        POOL.with(|p| {
            let mut pool = p.borrow_mut();
            if let Some(mut buf) = pool.pop() {
                buf.clear();
                buf
            } else {
                BytesMut::with_capacity(BUFFER_SIZE)
            }
        })
    }

    /// Release a buffer back to the pool.
    pub fn release(buf: BytesMut) {
        // Only pool buffers that have enough capacity but aren't excessively large
        if buf.capacity() >= BUFFER_SIZE && buf.capacity() <= BUFFER_SIZE * 2 {
            POOL.with(|p| {
                let mut pool = p.borrow_mut();
                if pool.len() < MAX_POOL_SIZE {
                    pool.push(buf);
                }
            });
        }
    }
}
