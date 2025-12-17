# VeloxLoop

[![PyPI version](https://badge.fury.io/py/veloxloop.svg)](https://badge.fury.io/py/veloxloop) <!-- Update when published -->
[![Python versions](https://img.shields.io/pypi/pyversions/veloxloop.svg)](https://pypi.org/project/veloxloop/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)

**VeloxLoop** — A modern, high-performance asyncio event loop implementation written from scratch in **Rust** using **PyO3** and the **polling** crate.

*Velox* is Latin for "swift" or "rapid" — reflecting the goal of delivering significantly faster I/O and lower overhead than the default selector event loop while remaining fully compatible with the standard `asyncio` API.

⚡ **Why VeloxLoop?**  
- Built in Rust for memory safety, zero-cost abstractions, and exceptional performance.  
- Powered by the lightweight and modern `polling` crate for cross-platform epoll/kqueue/IOCP support.  
- Completely independent design — no shared code or direct architectural overlap with existing projects (including RLoop, uvloop, or others).  
- Focus on clean, efficient readiness handling, minimal overhead, and excellent cross-platform behavior.  
- Early results show strong potential for outperforming current alternatives.

## Status

**Pre-alpha / Work in Progress**  
VeloxLoop is under active development. Basic socket I/O and timers are functional, but many asyncio features are still being implemented.  
Not yet suitable for production — intended for experimentation, testing, and contributions.

## Installation

VeloxLoop will be distributed as pre-built wheels for Linux, macOS, and Windows via PyPI.

```bash
pip install veloxloop
```

## Missing Features / Roadmap

The following asyncio features are **not yet implemented** and are planned for future development:

### Core Event Loop Features
- [ ] **Async context managers** - Full support for `async with` on loop methods

### Network & Transport Layer
- [ ] **Unix domain sockets** - Support for `AF_UNIX` sockets
- [ ] **Unix pipes** - `connect_read_pipe()` and `connect_write_pipe()`
- [ ] **Socket options** - Advanced socket configuration (SO_KEEPALIVE, TCP_NODELAY, etc.)
- [ ] **sendfile() support** - Zero-copy file transmission

### Subprocess Management
- [ ] **`subprocess_exec()`** - Execute shell commands asynchronously
- [ ] **`subprocess_shell()`** - Shell command execution
- [ ] **Process protocol** - Full subprocess protocol implementation
- [ ] **Process streams** - stdin/stdout/stderr handling

### Signal Handling
- [ ] **`add_signal_handler()`** - Register signal callbacks (Unix)
- [ ] **`remove_signal_handler()`** - Unregister signal callbacks
- [ ] **Signal integration** - Proper Ctrl+C and signal handling

### Advanced I/O
- [ ] **File descriptor passing** - Passing file descriptors between processes
- [ ] **Raw socket support** - Low-level socket operations
- [ ] **`connect_accepted_socket()`** - Create transport from accepted socket

### Platform-Specific Features
- [ ] **Windows named pipes** - Windows IPC support
- [ ] **Windows IOCP optimizations** - Better Windows performance
- [ ] **macOS/BSD kqueue optimizations** - Platform-specific tuning

### Performance & Diagnostics
- [ ] **Slow callback warnings** - Debug mode performance monitoring
- [ ] **Loop instrumentation** - Detailed metrics and statistics
- [ ] **Memory pooling** - Reduce allocation overhead
- [ ] **Zero-copy operations** - Minimize data copying where possible

### Compatibility & Standards
- [ ] **PEP 567 context variables** - Full context support in all callbacks
- [ ] **ProactorEventLoop equivalence** - Windows proactor-style operations
