// Allow dead code for utility functions that may be used in the future

use slab::Slab;
use quanta::Clock;
use std::sync::OnceLock;

use crate::constants::{PRECISION_NS, WHEEL_BITS, WHEEL_MASK, WHEEL_SIZE, WHEELS};

/// Global high-precision clock instance using quanta
/// Provides TSC-based timing on supported platforms for nanosecond precision
static CLOCK: OnceLock<Clock> = OnceLock::new();

/// Get the global quanta clock instance
#[inline]
pub fn get_clock() -> &'static Clock {
    CLOCK.get_or_init(Clock::new)
}

/// Get current time in nanoseconds using quanta's high-precision clock
#[inline]
pub fn now_ns() -> u64 {
    get_clock().raw()
}

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
///
/// Now uses:
/// - `quanta` for high-precision timing (TSC-based on x86)
/// - `slab` for pre-allocated timer entry storage (reduces allocations)

/// Timer entry key for slab storage
pub type TimerKey = usize;

pub struct TimerEntry {
    pub id: u64,
    pub expires_at: u64, // absolute ns
    pub callback: pyo3::Py<pyo3::PyAny>,
    pub args: Vec<pyo3::Py<pyo3::PyAny>>,
    pub context: Option<pyo3::Py<pyo3::PyAny>>,
    /// Slab key for O(1) removal
    slab_key: TimerKey,
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
    /// Provides O(1) insert/remove and better cache locality
    entries: Slab<TimerEntry>,
    /// Fast ID to slab key lookup (for cancel operations)
    id_to_key: rustc_hash::FxHashMap<u64, TimerKey>,
    /// Current time in milliseconds (relative to start_time)
    current_ms: u64,
    /// Counter for unique timer IDs
    next_id: u64,
    /// High-precision clock for timing
    clock: &'static Clock,
    /// Cached minimum expiry for fast next_expiry() calls
    min_expiry_cache: Option<u64>,
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
            clock: get_clock(),
            min_expiry_cache: None,
        }
    }

    /// Get current time from high-precision quanta clock
    #[inline]
    pub fn now(&self) -> u64 {
        self.clock.raw()
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

        // Pre-allocate slab entry
        let slab_key = self.entries.vacant_key();
        
        let entry = TimerEntry {
            id,
            expires_at: expires_at_ns,
            callback,
            args,
            context,
            slab_key,
        };
        
        // Insert into slab (O(1) operation)
        let key = self.entries.insert(entry);
        self.id_to_key.insert(id, key);

        // Update min expiry cache
        match self.min_expiry_cache {
            Some(min) if expires_at_ns < min => self.min_expiry_cache = Some(expires_at_ns),
            None => self.min_expiry_cache = Some(expires_at_ns),
            _ => {}
        }

        // Calculate relative expiration in MS
        let expiry_ms = (expires_at_ns.saturating_sub(start_ns)) / PRECISION_NS;
        self.cascade_timer(id, key, expiry_ms);

        id
    }

    fn cascade_timer(&mut self, id: u64, slab_key: TimerKey, expiry_ms: u64) {
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
        self.wheels[wheel_idx][slot].push(SlotEntry { id, slab_key });
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        if let Some(key) = self.id_to_key.remove(&id) {
            // O(1) removal from slab
            if self.entries.try_remove(key).is_some() {
                // Invalidate min expiry cache (will be recalculated on next next_expiry call)
                self.min_expiry_cache = None;
                return true;
            }
        }
        false
    }

    pub fn next_expiry(&self) -> Option<u64> {
        // Use cached value if available
        if let Some(min) = self.min_expiry_cache {
            return Some(min);
        }
        
        // Recalculate from slab entries
        self.entries.iter().map(|(_, e)| e.expires_at).min()
    }

    pub fn pop_expired(&mut self, now_ns: u64, start_ns: u64) -> Vec<TimerEntry> {
        let target_ms = (now_ns.saturating_sub(start_ns)) / PRECISION_NS;
        let mut expired = Vec::new();

        while self.current_ms <= target_ms {
            let slot = (self.current_ms & WHEEL_MASK as u64) as usize;

            // 1. Process wheel 0 slot
            let slot_entries = std::mem::take(&mut self.wheels[0][slot]);
            for entry in slot_entries {
                // O(1) removal from slab
                if let Some(timer_entry) = self.entries.try_remove(entry.slab_key) {
                    self.id_to_key.remove(&entry.id);
                    expired.push(timer_entry);
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
                    for entry in to_cascade {
                        self.re_cascade(entry.id, entry.slab_key, start_ns);
                    }
                    mask = (mask + 1) << WHEEL_BITS;
                    mask -= 1;
                } else {
                    break;
                }
            }
        }

        // Invalidate cache if any timers expired
        if !expired.is_empty() {
            self.min_expiry_cache = None;
        }

        expired
    }

    fn re_cascade(&mut self, id: u64, slab_key: TimerKey, start_ns: u64) {
        if let Some(entry) = self.entries.get(slab_key) {
            let expiry_ms = (entry.expires_at.saturating_sub(start_ns)) / PRECISION_NS;
            self.cascade_timer(id, slab_key, expiry_ms);
        }
    }

    /// Get statistics about timer storage for monitoring
    pub fn stats(&self) -> TimerStats {
        TimerStats {
            active_timers: self.entries.len(),
            capacity: self.entries.capacity(),
            current_ms: self.current_ms,
        }
    }
}

/// Statistics about the timer wheel
#[derive(Debug, Clone, Copy)]
pub struct TimerStats {
    pub active_timers: usize,
    pub capacity: usize,
    pub current_ms: u64,
}
