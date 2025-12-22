# Veloxloop Benchmarks

Run at: Mon 22 Dec 2025, 22:22  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 60,607 | 49,935 | 19,274 |
| **asyncio** | 39,487 | 32,821 | 16,391 |
| **uvloop** | 44,119 | 36,035 | 13,656 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 60,607.1 | 0.014ms | 0.042ms | 0.010ms | 3.480ms |
| **asyncio** | 39,486.8 | 0.025ms | 0.074ms | 0.010ms | 2.100ms |
| **uvloop** | 44,118.8 | 0.023ms | 0.064ms | 0.010ms | 3.080ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 49,935.4 | 0.020ms | 0.049ms | 0.010ms | 3.360ms |
| **asyncio** | 32,820.9 | 0.028ms | 0.082ms | 0.010ms | 2.110ms |
| **uvloop** | 36,035.2 | 0.025ms | 0.075ms | 0.020ms | 2.650ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,274.1 | 0.049ms | 0.132ms | 0.040ms | 2.940ms |
| **asyncio** | 16,391.3 | 0.059ms | 0.160ms | 0.040ms | 3.060ms |
| **uvloop** | 13,655.6 | 0.070ms | 0.178ms | 0.050ms | 2.450ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 41,229 | 34,575 | 14,306 |
| **asyncio** | 43,127 | 32,619 | 11,713 |
| **uvloop** | 43,825 | 36,108 | 14,254 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 41,228.7 | 0.023ms | 0.066ms | 0.020ms | 2.480ms |
| **asyncio** | 43,127.2 | 0.022ms | 0.050ms | 0.020ms | 2.050ms |
| **uvloop** | 43,825.0 | 0.023ms | 0.064ms | 0.020ms | 2.370ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 34,574.8 | 0.026ms | 0.076ms | 0.020ms | 2.440ms |
| **asyncio** | 32,619.4 | 0.028ms | 0.076ms | 0.020ms | 2.330ms |
| **uvloop** | 36,108.2 | 0.025ms | 0.074ms | 0.020ms | 3.090ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 14,305.5 | 0.070ms | 0.178ms | 0.050ms | 1.600ms |
| **asyncio** | 11,713.4 | 0.084ms | 0.235ms | 0.050ms | 2.410ms |
| **uvloop** | 14,253.5 | 0.069ms | 0.176ms | 0.050ms | 2.090ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 57,786 | 48,589 | 19,595 |
| **asyncio** | 57,427 | 48,438 | 21,766 |
| **uvloop** | 63,115 | 51,974 | 20,611 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 57,785.8 | 0.014ms | 0.048ms | 0.010ms | 1.690ms |
| **asyncio** | 57,426.6 | 0.014ms | 0.048ms | 0.010ms | 2.440ms |
| **uvloop** | 63,114.9 | 0.013ms | 0.044ms | 0.010ms | 2.150ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 48,588.6 | 0.022ms | 0.057ms | 0.010ms | 2.260ms |
| **asyncio** | 48,438.5 | 0.022ms | 0.056ms | 0.010ms | 2.230ms |
| **uvloop** | 51,974.1 | 0.019ms | 0.051ms | 0.010ms | 2.070ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,594.6 | 0.049ms | 0.137ms | 0.040ms | 2.320ms |
| **asyncio** | 21,765.6 | 0.046ms | 0.119ms | 0.030ms | 1.910ms |
| **uvloop** | 20,610.8 | 0.046ms | 0.120ms | 0.040ms | 2.270ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 85,001 | 86,826 |
| **asyncio** | 46,635 | 46,097 |
| **uvloop** | 61,356 | 61,358 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 85,001.3 | 0.070ms | 0.186ms | 0.010ms | 4.410ms |
| **asyncio** | 46,635.0 | 0.127ms | 0.330ms | 0.010ms | 6.300ms |
| **uvloop** | 61,356.1 | 0.096ms | 0.260ms | 0.020ms | 3.350ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 86,825.7 | 0.125ms | 0.342ms | 0.010ms | 4.410ms |
| **asyncio** | 46,097.3 | 0.237ms | 0.552ms | 0.010ms | 4.380ms |
| **uvloop** | 61,358.2 | 0.178ms | 0.499ms | 0.030ms | 4.560ms |

