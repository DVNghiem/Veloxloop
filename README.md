# VeloxLoop

[![PyPI version](https://badge.fury.io/py/veloxloop.svg)](https://badge.fury.io/py/veloxloop) <!-- Update when published -->
[![Python versions](https://img.shields.io/pypi/pyversions/veloxloop.svg)](https://pypi.org/project/veloxloop/)
[![License](https://img.shields.io/badge/license-BSD-blue.svg)](https://opensource.org/license/bsd-3-clause)

**VeloxLoop** — A modern, high-performance asyncio event loop implementation written from scratch in **Rust** using **PyO3**, **tokio** and **io-uring** crate.

*Velox* is Latin for "swift" or "rapid" — reflecting the goal of delivering significantly faster I/O and lower overhead than the default selector event loop while remaining fully compatible with the standard `asyncio` API.

> ⚠️ **Note:** VeloxLoop currently only supports **Linux**. macOS and Windows support is planned for future releases.

⚡ **Why VeloxLoop?**  
- Built in Rust for memory safety, zero-cost abstractions, and exceptional performance.  
- Powered by the lightweight and modern `io-uring` Linux kernel feature for maximum performance. Cross-platform support (kqueue for macOS, IOCP for Windows) planned.  
- Completely independent design — no shared code or direct architectural overlap with existing projects (including RLoop, uvloop, or others).  
- Focus on clean, efficient readiness handling, minimal overhead, and excellent performance.  
- Early results show strong potential for outperforming current alternatives.

## Status

**Pre-alpha / Work in Progress**  
VeloxLoop is under active development. Basic socket I/O and timers are functional, but many asyncio features are still being implemented.  
Not yet suitable for production — intended for experimentation, testing, and contributions.

## Installation

**Currently, VeloxLoop is only available for Linux.**

VeloxLoop will be distributed as pre-built wheels via PyPI. Future releases will include support for macOS and Windows.

```bash
pip install veloxloop
```

## Implemented Features

VeloxLoop currently supports the following asyncio features:

### Core Event Loop
- ✅ **Loop lifecycle** - `run_forever()`, `run_once()`, `stop()`, `close()`, `is_running()`, `is_closed()`
- ✅ **Time management** - `time()` for loop's internal clock
- ✅ **Callback scheduling** - `call_soon()`, `call_later()`, `call_at()` with callback support
- ✅ **Thread-safe callbacks** - `call_soon_threadsafe()` for cross-thread task submission
- ✅ **Future creation** - `create_future()` for creating pending futures
- ✅ **Debug mode** - `get_debug()`, `set_debug()` for diagnostic output
- ✅ **I/O operations tracking** - `io_operations()` for performance metrics

### I/O Monitoring
- ✅ **File descriptor watching** - `add_reader()`, `remove_reader()`, `add_writer()`, `remove_writer()`
- ✅ **Low-level socket operations** - `sock_connect()`, `sock_accept()`, `sock_recv()`, `sock_sendall()`
- ✅ **Zero-copy file transfers** - `sendfile()` with offset and count support

### Network & Transports
- ✅ **TCP connections** - `create_connection()` for client connections with `protocol_factory`
- ✅ **TCP servers** - `create_server()` and `start_server()` for server endpoints with `is_serving()` and `wait_closed()`
- ✅ **Stream I/O** - `open_connection()` for high-level stream-based communication
- ✅ **Streams API** - Full `StreamReader` and `StreamWriter` support with async reading operations

### StreamReader Features
- ✅ **Async reads** - `read()`, `readexactly()`, `readline()`, `readuntil()`
- ✅ **Buffer management** - `feed_data()`, `feed_eof()`, `at_eof()`
- ✅ **Exception handling** - `set_exception()`, error propagation
- ✅ **Buffer limits** - `get_limit()`, `buffer_size()` for flow control
- ✅ **Zero-copy socket reading** - Efficient buffering from network sockets

### StreamWriter Features
- ✅ **Async writes** - `write()`, `writelines()`, `drain()`
- ✅ **Write EOF** - `write_eof()`, `can_write_eof()`
- ✅ **Flow control** - High/low water marks with `needs_drain()` detection
- ✅ **Buffer monitoring** - `get_write_buffer_size()`, `is_drained()`, `is_closing()`
- ✅ **Graceful shutdown** - `close()` with proper buffer draining

### Transport Features
- ✅ **StreamTransport** - High-performance stream transport with integrated Reader/Writer
- ✅ **Socket information** - `getsockname()`, `getpeername()`, `fileno()`, `get_extra_info()`
- ✅ **IPv6 support** - Full IPv6 socket address handling with flowinfo and scope_id
- ✅ **Socket options** - `setsockopt()` for low-level socket configuration
- ✅ **TCP NodeDelay** - `TCP_NODELAY` support for latency optimization
- ✅ **SO_REUSEADDR** - Address reuse for server sockets
- ✅ **SO_REUSEPORT** - Port reuse for load balancing
- ✅ **Keep-alive settings** - Full TCP keep-alive configuration (TCP_KEEP_IDLE, TCP_KEEP_INTVL, TCP_KEEP_CNT)
- ✅ **Send/receive buffers** - SO_SNDBUF and SO_RCVBUF tuning

### UDP/Datagram
- ✅ **UDP endpoints** - `create_datagram_endpoint()` for datagram-based communication
- ✅ **Datagram I/O** - `sendto()` with optional address, zero-copy send
- ✅ **Connected UDP** - Support for connected datagram sockets
- ✅ **UDP transports** - `UdpTransport` with full protocol callbacks

### SSL/TLS Support
- ✅ **SSL contexts** - `SSLContext` with both client and server configurations
- ✅ **Certificate management** - Load certificate chains from PEM files
- ✅ **Hostname verification** - Support for hostname checking on client connections
- ✅ **System certificates** - Automatic loading of system root certificates
- ✅ **SSL transports** - Full SSL/TLS encrypted connections
- ✅ **Multiple cipher suites** - Support for rustls cipher configuration

### Domain Name Resolution
- ✅ **`getaddrinfo()`** - Full DNS resolution with hints and address family selection
- ✅ **`getnameinfo()`** - Reverse DNS lookups (address to hostname)
- ✅ **Concurrent DNS** - Async DNS operations without blocking the event loop
- ✅ **IPv4 & IPv6** - Full support for both address families

### Threading & Concurrency
- ✅ **Thread pool executor** - `run_in_executor()` for CPU-bound work
- ✅ **Custom executors** - `set_default_executor()` for task execution
- ✅ **Cross-thread safety** - `call_soon_threadsafe()` for thread-safe operations

### Exception & Task Management
- ✅ **Exception handlers** - `set_exception_handler()`, `get_exception_handler()`, `call_exception_handler()`
- ✅ **Task factories** - `set_task_factory()`, `get_task_factory()` for custom task creation
- ✅ **Async generators** - `track_async_generator()`, `untrack_async_generator()`, `shutdown_asyncgens()`
- ✅ **Executor metrics** - `get_executor_active_tasks()`, `get_executor_num_workers()`

### Performance Optimizations
- ✅ **Buffer pooling** - Efficient memory reuse for stream buffers
- ✅ **Jemalloc allocator** - High-performance memory allocation (Linux/BSD/macOS)
- ✅ **io-uring backend** - Modern Linux kernel I/O interface for maximum performance
- ✅ **Lock-free state** - Atomic flags for hot-path checks without locks

## Missing Features / Roadmap

The following asyncio features are **not yet implemented** and are planned for future development. Currently, development is focused on **Linux** support.

### Network & Transport Layer
- [ ] **Unix domain sockets** - Support for `AF_UNIX` sockets
- [ ] **Unix pipes** - `connect_read_pipe()` and `connect_write_pipe()`

### Subprocess Management
- [ ] **`subprocess_exec()`** - Execute shell commands asynchronously
- [ ] **`subprocess_shell()`** - Shell command execution

### Signal Handling
- [ ] **`add_signal_handler()`** - Register signal callbacks (Unix)
- [ ] **`remove_signal_handler()`** - Unregister signal callbacks

### Advanced I/O
- [ ] **File descriptor passing** - Passing file descriptors between processes
- [ ] **`connect_accepted_socket()`** - Create transport from accepted socket

### Performance & Diagnostics
- [ ] **Slow callback warnings** - Debug mode performance monitoring
- [ ] **Detailed loop instrumentation** - Fine-grained metrics and statistics

### Platform Support (Future)
*The following features will be prioritized after Linux support is stabilized and macOS/Windows support is added:*
- [ ] **Windows IOCP implementation** - Windows async I/O support
- [ ] **macOS/BSD kqueue support** - macOS and BSD async I/O support
- [ ] **Windows named pipes** - Windows IPC support
- [ ] **Windows ProactorEventLoop equivalence** - Windows proactor-style operations

## Benchmarks

VeloxLoop includes a comprehensive benchmark suite to compare performance against asyncio and uvloop. The benchmarks measure:

- **Raw socket performance** - TCP echo using `sock_recv`/`sock_sendall`
- **Stream performance** - TCP echo using `asyncio.StreamReader`/`StreamWriter`
- **Protocol performance** - TCP echo using `asyncio.Protocol`

Each benchmark tests three message sizes (1KB, 10KB, 100KB) and measures throughput, latency, and percentiles.

### Running Benchmarks

```bash
cd benchmarks
pip install -r requirements.txt
python run.py all
```

See [benchmarks/README.md](benchmarks/README.md) for detailed documentation.
