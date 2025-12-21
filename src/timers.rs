use rustc_hash::FxHashMap;

use crate::constants::{PRECISION_NS, WHEEL_BITS, WHEEL_MASK, WHEEL_SIZE, WHEELS};

/// Hierarchical Timer Wheel for O(1) timer operations.
///
/// Structure:
/// - 4 wheels with 256 slots each (2^8 slots per wheel)
/// - Total bit coverage: 32 bits (4 * 8)
/// - Precision: 1ms (10^6 ns)
/// - Max timeout: ~49.7 days (2^32 ms)
///
/// Wheel 0 covers: 1ms to 256ms
/// Wheel 1 covers: 256ms to 65.5s
/// Wheel 2 covers: 65.5s to 4.6h
/// Wheel 3 covers: 4.6h to 49.7d

pub struct TimerEntry {
    #[allow(dead_code)]
    pub id: u64,
    pub expires_at: u64, // absolute ns
    pub callback: pyo3::Py<pyo3::PyAny>,
    pub args: Vec<pyo3::Py<pyo3::PyAny>>,
    #[allow(dead_code)]
    pub context: Option<pyo3::Py<pyo3::PyAny>>,
}

pub struct Timers {
    /// Wheels storing list of timer IDs
    wheels: [Vec<Vec<u64>>; WHEELS],
    /// Storage for actual timer entries - FxHashMap for faster u64 key lookup
    entries: FxHashMap<u64, TimerEntry>,
    /// Current time in milliseconds (relative to start_time)
    current_ms: u64,
    /// Counter for unique timer IDs
    next_id: u64,
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
            entries: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
            current_ms: 0,
            next_id: 1,
        }
    }

    pub fn insert(
        &mut self,
        expires_at_ns: u64,
        callback: pyo3::Py<pyo3::PyAny>,
        args: Vec<pyo3::Py<pyo3::PyAny>>,
        context: Option<pyo3::Py<pyo3::PyAny>>,
        start_ns: u64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let entry = TimerEntry {
            id,
            expires_at: expires_at_ns,
            callback,
            args,
            context,
        };
        self.entries.insert(id, entry);

        // Calculate relative expiration in MS
        let expiry_ms = (expires_at_ns.saturating_sub(start_ns)) / PRECISION_NS;
        self.cascade_timer(id, expiry_ms);

        id
    }

    fn cascade_timer(&mut self, id: u64, expiry_ms: u64) {
        // If the expiration is in the past or is now, we want it to fire ASAP.
        // We align it to current_ms so it gets picked up by the next tick or current one.
        let effective_ms = if expiry_ms < self.current_ms {
            self.current_ms
        } else {
            expiry_ms
        };

        let diff = effective_ms.saturating_sub(self.current_ms);

        let wheel_idx: usize;
        if diff < (1 << WHEEL_BITS) {
            wheel_idx = 0;
        } else if diff < (1 << (2 * WHEEL_BITS)) {
            wheel_idx = 1;
        } else if diff < (1 << (3 * WHEEL_BITS)) {
            wheel_idx = 2;
        } else {
            wheel_idx = 3;
        }

        let slot = ((effective_ms >> (wheel_idx as u32 * WHEEL_BITS)) & WHEEL_MASK as u64) as usize;
        self.wheels[wheel_idx][slot].push(id);
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        self.entries.remove(&id).is_some()
    }

    pub fn next_expiry(&self) -> Option<u64> {
        // This is a naive implementation because searching the wheel for the next slot
        // can be O(WHEEL_SIZE).
        // For now, let's just use the current_ms as a hint or return the minimum expires_at if available.
        // Actually, we can return the minimum expires_at from self.entries for accuracy.
        self.entries.values().map(|e| e.expires_at).min()
    }

    pub fn pop_expired(&mut self, now_ns: u64, start_ns: u64) -> Vec<TimerEntry> {
        let target_ms = (now_ns.saturating_sub(start_ns)) / PRECISION_NS;
        let mut expired = Vec::new();

        while self.current_ms <= target_ms {
            let slot = (self.current_ms & WHEEL_MASK as u64) as usize;

            // 1. Process wheel 0 slot
            let ids = std::mem::take(&mut self.wheels[0][slot]);
            for id in ids {
                if let Some(entry) = self.entries.remove(&id) {
                    expired.push(entry);
                }
            }

            self.current_ms += 1;

            // 2. Cascade upper wheels if we hit boundaries
            let mut mask = WHEEL_MASK as u64;
            for i in 1..WHEELS {
                if (self.current_ms & mask) == 0 {
                    let next_slot =
                        ((self.current_ms >> (i as u32 * WHEEL_BITS)) & WHEEL_MASK as u64) as usize;
                    let to_cascade = std::mem::take(&mut self.wheels[i][next_slot]);
                    for id in to_cascade {
                        self.re_cascade(id, start_ns);
                    }
                    mask = (mask + 1) << WHEEL_BITS;
                    mask -= 1;
                } else {
                    break;
                }
            }
        }

        expired
    }

    fn re_cascade(&mut self, id: u64, start_ns: u64) {
        if let Some(entry) = self.entries.get(&id) {
            let expiry_ms = (entry.expires_at.saturating_sub(start_ns)) / PRECISION_NS;
            self.cascade_timer(id, expiry_ms);
        }
    }
}
