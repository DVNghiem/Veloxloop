# Veloxloop Benchmarks

Run at: Wed 17 Dec 2025, 18:55  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 56,894 | 48,966 | 19,318 |
| **asyncio** | 40,525 | 33,766 | 16,850 |
| **uvloop** | 47,126 | 40,579 | 13,951 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 56,893.6 | 0.014ms | 0.038ms | 0.010ms | 3.120ms |
| **asyncio** | 40,525.0 | 0.024ms | 0.055ms | 0.010ms | 2.040ms |
| **uvloop** | 47,126.0 | 0.022ms | 0.055ms | 0.010ms | 2.300ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 48,966.1 | 0.021ms | 0.042ms | 0.010ms | 2.060ms |
| **asyncio** | 33,766.4 | 0.028ms | 0.064ms | 0.010ms | 1.810ms |
| **uvloop** | 40,579.1 | 0.023ms | 0.048ms | 0.020ms | 0.470ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,317.7 | 0.050ms | 0.096ms | 0.040ms | 2.150ms |
| **asyncio** | 16,849.6 | 0.058ms | 0.117ms | 0.030ms | 1.830ms |
| **uvloop** | 13,950.8 | 0.069ms | 0.117ms | 0.050ms | 2.100ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 44,875 | 25,474 | 4,929 |
| **asyncio** | 40,052 | 33,029 | 11,788 |
| **uvloop** | 43,527 | 35,858 | 13,775 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 44,875.0 | 0.023ms | 0.059ms | 0.010ms | 1.960ms |
| **asyncio** | 40,051.8 | 0.024ms | 0.069ms | 0.020ms | 2.110ms |
| **uvloop** | 43,527.0 | 0.023ms | 0.063ms | 0.020ms | 2.510ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 25,473.6 | 0.037ms | 0.100ms | 0.030ms | 1.820ms |
| **asyncio** | 33,029.1 | 0.028ms | 0.080ms | 0.020ms | 6.990ms |
| **uvloop** | 35,858.3 | 0.025ms | 0.074ms | 0.020ms | 2.480ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 4,928.7 | 0.201ms | 0.498ms | 0.160ms | 2.270ms |
| **asyncio** | 11,788.5 | 0.084ms | 0.233ms | 0.050ms | 2.040ms |
| **uvloop** | 13,775.0 | 0.071ms | 0.184ms | 0.050ms | 3.640ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 65,261 | 38,029 | 6,631 |
| **asyncio** | 61,345 | 48,433 | 22,457 |
| **uvloop** | 62,730 | 52,694 | 21,102 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 65,260.8 | 0.013ms | 0.039ms | 0.010ms | 2.230ms |
| **asyncio** | 61,344.9 | 0.013ms | 0.031ms | 0.010ms | 0.790ms |
| **uvloop** | 62,729.7 | 0.014ms | 0.035ms | 0.010ms | 1.710ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 38,029.2 | 0.024ms | 0.065ms | 0.020ms | 1.760ms |
| **asyncio** | 48,432.6 | 0.021ms | 0.046ms | 0.010ms | 2.400ms |
| **uvloop** | 52,694.1 | 0.019ms | 0.039ms | 0.010ms | 1.120ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 6,630.6 | 0.149ms | 0.254ms | 0.130ms | 1.530ms |
| **asyncio** | 22,456.7 | 0.045ms | 0.093ms | 0.030ms | 1.860ms |
| **uvloop** | 21,101.5 | 0.045ms | 0.094ms | 0.040ms | 1.900ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop | 6 conn | 11 conn | |
| --- | --- | --- |
| **veloxloop** | 82,206 | 89,223 |
| **asyncio** | 50,398 | 49,710 |
| **uvloop** | 62,124 | 63,973 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 82,206.5 | 0.071ms | 0.158ms | 0.010ms | 4.170ms |
| **asyncio** | 50,398.0 | 0.118ms | 0.199ms | 0.010ms | 26.390ms |
| **uvloop** | 62,124.2 | 0.095ms | 0.255ms | 0.020ms | 4.160ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 89,223.4 | 0.121ms | 0.258ms | 0.010ms | 4.190ms |
| **asyncio** | 49,709.8 | 0.220ms | 0.390ms | 0.010ms | 34.840ms |
| **uvloop** | 63,973.3 | 0.170ms | 0.404ms | 0.040ms | 4.590ms |

