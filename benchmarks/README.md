# Veloxloop Benchmarks

Run at: Mon 22 Dec 2025, 19:38  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 64,720 | 50,723 | 19,219 |
| **asyncio** | 40,200 | 33,907 | 16,460 |
| **uvloop** | 42,372 | 38,118 | 14,640 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 64,719.5 | 0.013ms | 0.038ms | 0.010ms | 3.160ms |
| **asyncio** | 40,200.2 | 0.024ms | 0.064ms | 0.010ms | 2.310ms |
| **uvloop** | 42,371.6 | 0.024ms | 0.058ms | 0.010ms | 2.620ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 50,723.3 | 0.019ms | 0.045ms | 0.010ms | 2.920ms |
| **asyncio** | 33,906.6 | 0.027ms | 0.074ms | 0.010ms | 2.830ms |
| **uvloop** | 38,118.1 | 0.024ms | 0.054ms | 0.020ms | 1.050ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,218.6 | 0.049ms | 0.110ms | 0.040ms | 3.060ms |
| **asyncio** | 16,459.7 | 0.059ms | 0.151ms | 0.040ms | 2.090ms |
| **uvloop** | 14,640.4 | 0.066ms | 0.122ms | 0.040ms | 2.910ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 47,311 | 35,484 | 15,173 |
| **asyncio** | 40,285 | 34,468 | 12,240 |
| **uvloop** | 44,470 | 37,156 | 14,556 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 47,310.7 | 0.021ms | 0.040ms | 0.020ms | 0.710ms |
| **asyncio** | 40,284.6 | 0.023ms | 0.065ms | 0.020ms | 2.480ms |
| **uvloop** | 44,470.5 | 0.022ms | 0.057ms | 0.020ms | 2.440ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 35,484.1 | 0.025ms | 0.065ms | 0.020ms | 3.600ms |
| **asyncio** | 34,468.0 | 0.027ms | 0.068ms | 0.020ms | 2.760ms |
| **uvloop** | 37,156.3 | 0.024ms | 0.063ms | 0.020ms | 1.970ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 15,172.8 | 0.064ms | 0.153ms | 0.050ms | 2.520ms |
| **asyncio** | 12,240.4 | 0.080ms | 0.208ms | 0.050ms | 2.050ms |
| **uvloop** | 14,556.5 | 0.067ms | 0.143ms | 0.050ms | 2.010ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 61,866 | 51,458 | 21,336 |
| **asyncio** | 59,510 | 49,783 | 22,651 |
| **uvloop** | 66,235 | 55,250 | 21,545 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 61,866.3 | 0.012ms | 0.035ms | 0.010ms | 2.550ms |
| **asyncio** | 59,510.0 | 0.013ms | 0.038ms | 0.010ms | 0.850ms |
| **uvloop** | 66,235.2 | 0.012ms | 0.034ms | 0.010ms | 1.000ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 51,458.0 | 0.021ms | 0.040ms | 0.010ms | 1.350ms |
| **asyncio** | 49,782.8 | 0.021ms | 0.041ms | 0.020ms | 1.620ms |
| **uvloop** | 55,250.5 | 0.018ms | 0.039ms | 0.010ms | 1.940ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 21,336.3 | 0.045ms | 0.097ms | 0.040ms | 1.680ms |
| **asyncio** | 22,651.0 | 0.044ms | 0.097ms | 0.040ms | 2.200ms |
| **uvloop** | 21,545.1 | 0.044ms | 0.084ms | 0.040ms | 1.260ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 98,198 | 108,515 |
| **asyncio** | 52,685 | 57,058 |
| **uvloop** | 75,768 | 77,825 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 98,198.3 | 0.061ms | 0.133ms | 0.010ms | 3.780ms |
| **asyncio** | 52,684.6 | 0.112ms | 0.166ms | 0.010ms | 0.300ms |
| **uvloop** | 75,767.7 | 0.078ms | 0.110ms | 0.040ms | 4.040ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 108,515.0 | 0.101ms | 0.191ms | 0.010ms | 2.350ms |
| **asyncio** | 57,058.1 | 0.192ms | 0.233ms | 0.010ms | 0.590ms |
| **uvloop** | 77,825.1 | 0.140ms | 0.187ms | 0.060ms | 1.610ms |

