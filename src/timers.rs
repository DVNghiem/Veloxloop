use std::{cmp::Reverse, collections::BinaryHeap};

use slab::Slab;

use crate::constants::{PRECISION_NS, WHEEL_BITS, WHEEL_MASK, WHEEL_SIZE, WHEELS};

/// Timer entry key for slab storage
pub type TimerKey = usize;

pub struct TimerEntry {
    pub expires_at: u64, // absolute ns
    pub callback: pyo3::Py<pyo3::PyAny>,
    pub args: Vec<pyo3::Py<pyo3::PyAny>>,
}

/// Slot entry storing timer ID and its slab key for efficient lookup
#[derive(Clone, Copy)]
struct SlotEntry {
    id: u64,
    slab_key: TimerKey,
}

pub struct Timers {
    /// Wheels storing list of timer slot entries
    wheels: [Vec<Vec<SlotEntry>>; WHEELS],
    /// Pre-allocated storage for timer entries using slab
    entries: Slab<TimerEntry>,
    /// Fast ID to slab key lookup (for cancel operations)
    id_to_key: rustc_hash::FxHashMap<u64, TimerKey>,
    /// Current time in milliseconds (relative to start_time)
    current_ms: u64,
    /// Counter for unique timer IDs
    next_id: u64,
    /// Cached minimum expiry for fast next_expiry() calls
    min_expiry_cache: Option<u64>,

    heap: BinaryHeap<Reverse<(u64, TimerKey)>>
}

impl Timers {
    pub fn new() -> Self {
        let mut wheels = [(); WHEELS].map(|_| Vec::with_capacity(WHEEL_SIZE));
        for w in &mut wheels {
            for _ in 0..WHEEL_SIZE {
                w.push(Vec::with_capacity(8));
            }
        }

        Self {
            wheels,
            entries: Slab::with_capacity(1024),
            id_to_key: rustc_hash::FxHashMap::with_capacity_and_hasher(1024, Default::default()),
            current_ms: 0,
            next_id: 1,
            min_expiry_cache: None,
            heap: BinaryHeap::with_capacity(1024),
        }
    }

    pub fn insert(
        &mut self,
        expires_at_ns: u64,
        callback: pyo3::Py<pyo3::PyAny>,
        args: Vec<pyo3::Py<pyo3::PyAny>>,
        _context: Option<pyo3::Py<pyo3::PyAny>>,
        start_ns: u64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        // Pre-allocate slab entry
        let slab_key = self.entries.vacant_key();
        
        let entry = TimerEntry {
            expires_at: expires_at_ns,
            callback,
            args,
        };

        self.entries.insert(entry);
        self.id_to_key.insert(id, slab_key);

        // Calculate relative expiry in ms
        let expiry_ms = (expires_at_ns.saturating_sub(start_ns)) / PRECISION_NS;
        self.cascade_timer(id, slab_key, expiry_ms);
        
        // Update cache if this is earlier
        match self.min_expiry_cache {
            Some(min) if expires_at_ns < min => self.min_expiry_cache = Some(expires_at_ns),
            None => self.min_expiry_cache = Some(expires_at_ns),
            _ => {}
        }
        self.heap.push(Reverse((expires_at_ns, slab_key)));
        id
    }

    fn cascade_timer(&mut self, id: u64, slab_key: TimerKey, expiry_ms: u64) {
        // Calculate which wheel and slot
        let delta = expiry_ms.saturating_sub(self.current_ms);
        
        let (wheel, slot) = if delta < WHEEL_SIZE as u64 {
            (0, (self.current_ms + delta) & WHEEL_MASK as u64)
        } else {
            let mut level: u32 = 1;
            let mut reduced_delta = delta >> WHEEL_BITS;
            while (level as usize) < WHEELS - 1 && reduced_delta >= WHEEL_SIZE as u64 {
                level += 1;
                reduced_delta >>= WHEEL_BITS;
            }
            let slot = ((self.current_ms >> (level * WHEEL_BITS)) + reduced_delta) & WHEEL_MASK as u64;
            (level as usize, slot)
        };

        self.wheels[wheel][slot as usize].push(SlotEntry { id, slab_key });
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        if let Some(slab_key) = self.id_to_key.remove(&id) {
            if self.entries.contains(slab_key) {
                self.entries.remove(slab_key);
                self.min_expiry_cache = None;
                return true;
            }
        }
        self.min_expiry_cache = None; // Invalidate, recompute lazy
        false
    }

    pub fn next_expiry(&mut self) -> Option<u64> {
        if self.min_expiry_cache.is_none() {
            self.recompute_min_expiry();
        }
        self.min_expiry_cache
    }

    fn recompute_min_expiry(&mut self) {
        while let Some(Reverse((exp, key))) = self.heap.peek() {
            if self.entries.contains(*key) && *exp == self.entries[*key].expires_at {
                self.min_expiry_cache = Some(*exp);
                return;
            }
            self.heap.pop(); // Clean stale
        }
        self.min_expiry_cache = None;
    }

    /// Pop all expired timers up to current_ns
    pub fn pop_expired(
        &mut self,
        current_ns: u64,
        start_ns: u64,
    ) -> Vec<TimerEntry> {
        let target_ms = (current_ns.saturating_sub(start_ns)) / PRECISION_NS;
        let mut expired = Vec::new();

        while self.current_ms <= target_ms {
            let slot = (self.current_ms & WHEEL_MASK as u64) as usize;

            // Collect expired timers from wheel 0
            for slot_entry in std::mem::take(&mut self.wheels[0][slot]) {
                if let Some(entry) = self.entries.try_remove(slot_entry.slab_key) {
                    self.id_to_key.remove(&slot_entry.id);
                    expired.push(entry);
                }
            }

            self.current_ms += 1;

            // Cascade higher wheels when appropriate
            if self.current_ms % WHEEL_SIZE as u64 == 0 {
                self.cascade_down(1, start_ns);
            }
            if self.current_ms % (WHEEL_SIZE as u64 * WHEEL_SIZE as u64) == 0 {
                self.cascade_down(2, start_ns);
            }
            if self.current_ms % (WHEEL_SIZE as u64 * WHEEL_SIZE as u64 * WHEEL_SIZE as u64) == 0 {
                self.cascade_down(3, start_ns);
            }
        }

        // Invalidate cache if any timers expired
        if !expired.is_empty() {
            self.min_expiry_cache = None;
        }

        expired
    }

    fn cascade_down(&mut self, wheel: u32, start_ns: u64) {
        let slot = ((self.current_ms >> (wheel * WHEEL_BITS)) & WHEEL_MASK as u64) as usize;
        
        for slot_entry in std::mem::take(&mut self.wheels[wheel as usize][slot]) {
            self.re_cascade(slot_entry.id, slot_entry.slab_key, start_ns);
        }
    }

    fn re_cascade(&mut self, id: u64, slab_key: TimerKey, start_ns: u64) {
        if let Some(entry) = self.entries.get(slab_key) {
            let expiry_ms = (entry.expires_at.saturating_sub(start_ns)) / PRECISION_NS;
            self.cascade_timer(id, slab_key, expiry_ms);
        }
    }
}
