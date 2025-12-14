use pyo3::prelude::*;
use std::collections::VecDeque;
use parking_lot::Mutex;
use std::sync::Arc;
use crate::poller::LoopPoller;

pub struct Callback {
    pub callback: PyObject,
    pub args: Vec<PyObject>, // Minimal args, usually Context + Args
    pub context: Option<PyObject>,
}

pub struct CallbackQueue {
    queue: Mutex<VecDeque<Callback>>,
    poller: Arc<LoopPoller>, // Needed to wake up the loop
}

impl CallbackQueue {
    pub fn new(poller: Arc<LoopPoller>) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            poller,
        }
    }

    pub fn push(&self, callback: Callback) {
        let mut q = self.queue.lock();
        q.push_back(callback);
        // We always notify for now to ensure loop wakes up
        // Optimization: only notify if loop is sleeping? 
        // Poller::notify is cheap on Linux (eventfd write), verifying calling it is safe.
        // But LoopPoller struct I made wraps `Arc<Poller>`.
        // Let's ensure LoopPoller exposes notify.
        let _ = self.poller.notify();
    }

    pub fn pop_all(&self) -> VecDeque<Callback> {
        let mut q = self.queue.lock();
        std::mem::take(&mut *q)
    }
    
    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}
