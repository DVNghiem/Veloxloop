pub const DEFAULT_LIMIT: usize = 128 * 1024; // 128 KB default - increased for better large message perf
pub const DEFAULT_HIGH: usize = 128 * 1024; // 128 KB
pub const DEFAULT_LOW: usize = 32 * 1024; // 32 KB
// Use constants directly since libc may not export them on all platforms
pub const NI_MAXHOST: usize = 1025;
pub const NI_MAXSERV: usize = 32;

pub const WHEEL_BITS: u32 = 8;
pub const WHEEL_SIZE: usize = 1 << WHEEL_BITS; // 256
pub const WHEEL_MASK: u32 = (WHEEL_SIZE as u32) - 1;

pub const WHEELS: usize = 4;
pub const PRECISION_NS: u64 = 1_000_000; // 1ms - keep for timer precision

pub const STACK_BUF_SIZE: usize = 65536;

pub const POLLER_BATCH_THRESHOLD: usize = 32; // Batch size for processing callbacks poller events