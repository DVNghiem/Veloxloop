pub const DEFAULT_LIMIT: usize = 64 * 1024; // 64 KB default
pub const DEFAULT_HIGH: usize = 64 * 1024; // 64 KB
pub const DEFAULT_LOW: usize = 16 * 1024; // 16 KB
// Use constants directly since libc may not export them on all platforms
pub const NI_MAXHOST: usize = 1025;
pub const NI_MAXSERV: usize = 32;
