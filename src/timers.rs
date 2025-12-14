use pyo3::prelude::*;
use priority_queue::PriorityQueue;
use std::cmp::Reverse;
use std::collections::HashMap;

pub struct TimerEntry {
    pub callback: PyObject,
    pub args: Vec<PyObject>,
    pub context: Option<PyObject>,
}

pub struct Timers {
    // PriorityQueue stores <ID, Priority>. Priority is Reverse<u64> (nanoseconds) so smallest time is first.
    pq: PriorityQueue<u64, Reverse<u64>>,
    entries: HashMap<u64, TimerEntry>,
    next_id: u64,
}

impl Timers {
    pub fn new() -> Self {
        Self {
            pq: PriorityQueue::new(),
            entries: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn insert(&mut self, when: u64, callback: PyObject, args: Vec<PyObject>, context: Option<PyObject>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let entry = TimerEntry {
            callback,
            args,
            context,
        };

        self.entries.insert(id, entry);
        self.pq.push(id, Reverse(when));
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.pq.remove(&id);
        self.entries.remove(&id);
    }

    pub fn pop_expired(&mut self, now: u64) -> Vec<TimerEntry> {
        let mut expired = Vec::new();
        
        // Check top
        loop {
             if let Some((_, Reverse(when))) = self.pq.peek() {
                 if *when <= now {
                     if let Some((id, _)) = self.pq.pop() {
                         if let Some(entry) = self.entries.remove(&id) {
                             expired.push(entry);
                         }
                     }
                 } else {
                     break;
                 }
             } else {
                 break;
             }
        }
        expired
    }
    
    pub fn next_expiry(&self) -> Option<u64> {
        self.pq.peek().map(|(_, Reverse(when))| *when)
    }

    pub fn len(&self) -> usize {
        self.pq.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.pq.is_empty()
    }
}
