# Veloxloop Benchmarks

Run at: Fri 19 Dec 2025, 09:21  
Environment: Linux x86_64 (CPUs: 8)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 62,315 | 56,538 | 23,250 |
| **asyncio** | 49,454 | 38,955 | 21,839 |
| **uvloop** | 56,694 | 34,749 | 17,002 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 62,315.3 | 0.012ms | 0.030ms | 0.010ms | 2.020ms |
| **asyncio** | 49,454.1 | 0.021ms | 0.039ms | 0.010ms | 0.310ms |
| **uvloop** | 56,694.0 | 0.015ms | 0.039ms | 0.010ms | 2.360ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 56,538.0 | 0.019ms | 0.030ms | 0.010ms | 1.840ms |
| **asyncio** | 38,955.4 | 0.024ms | 0.050ms | 0.010ms | 4.210ms |
| **uvloop** | 34,749.2 | 0.027ms | 0.060ms | 0.010ms | 3.660ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 23,249.8 | 0.042ms | 0.066ms | 0.030ms | 3.250ms |
| **asyncio** | 21,838.8 | 0.044ms | 0.077ms | 0.030ms | 0.490ms |
| **uvloop** | 17,002.3 | 0.057ms | 0.096ms | 0.040ms | 3.110ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 43,933 | 40,117 | 16,896 |
| **asyncio** | 44,537 | 42,626 | 15,188 |
| **uvloop** | 39,522 | 46,405 | 18,654 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 43,932.6 | 0.023ms | 0.048ms | 0.010ms | 4.090ms |
| **asyncio** | 44,536.6 | 0.023ms | 0.047ms | 0.020ms | 3.850ms |
| **uvloop** | 39,521.6 | 0.025ms | 0.059ms | 0.020ms | 6.020ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 40,116.9 | 0.023ms | 0.049ms | 0.020ms | 3.170ms |
| **asyncio** | 42,625.7 | 0.022ms | 0.044ms | 0.020ms | 2.090ms |
| **uvloop** | 46,405.3 | 0.021ms | 0.039ms | 0.020ms | 1.500ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 16,895.9 | 0.057ms | 0.104ms | 0.050ms | 3.330ms |
| **asyncio** | 15,188.3 | 0.065ms | 0.105ms | 0.040ms | 3.000ms |
| **uvloop** | 18,653.7 | 0.053ms | 0.088ms | 0.040ms | 2.260ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 74,778 | 65,163 | 26,499 |
| **asyncio** | 64,472 | 56,135 | 28,324 |
| **uvloop** | 72,056 | 63,968 | 27,311 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 74,778.3 | 0.011ms | 0.028ms | 0.010ms | 0.230ms |
| **asyncio** | 64,472.3 | 0.013ms | 0.030ms | 0.010ms | 1.980ms |
| **uvloop** | 72,055.8 | 0.012ms | 0.029ms | 0.010ms | 0.560ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 65,162.8 | 0.014ms | 0.029ms | 0.010ms | 0.180ms |
| **asyncio** | 56,135.2 | 0.017ms | 0.036ms | 0.010ms | 0.970ms |
| **uvloop** | 63,967.8 | 0.014ms | 0.030ms | 0.010ms | 0.170ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 26,499.2 | 0.037ms | 0.054ms | 0.030ms | 0.440ms |
| **asyncio** | 28,324.4 | 0.033ms | 0.055ms | 0.030ms | 0.540ms |
| **uvloop** | 27,311.0 | 0.035ms | 0.050ms | 0.030ms | 0.200ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 4 conn | 7 conn |
| --- | --- | --- |
| **veloxloop** | 77,489 | 81,395 |
| **asyncio** | 46,975 | 45,698 |
| **uvloop** | 57,951 | 67,464 |

### 4 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 77,488.9 | 0.050ms | 0.100ms | 0.010ms | 2.590ms |
| **asyncio** | 46,975.3 | 0.084ms | 0.129ms | 0.010ms | 4.520ms |
| **uvloop** | 57,951.4 | 0.067ms | 0.134ms | 0.020ms | 4.060ms |

### 7 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 81,395.3 | 0.084ms | 0.176ms | 0.010ms | 3.590ms |
| **asyncio** | 45,698.3 | 0.152ms | 0.210ms | 0.010ms | 3.300ms |
| **uvloop** | 67,464.3 | 0.103ms | 0.157ms | 0.020ms | 3.110ms |

