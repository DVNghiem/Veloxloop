# Veloxloop Benchmarks

Run at: Wed 17 Dec 2025, 23:52  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 53,043 | 45,377 | 17,975 |
| **asyncio** | 39,430 | 33,464 | 15,968 |
| **uvloop** | 44,934 | 37,474 | 14,163 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 53,042.7 | 0.016ms | 0.040ms | 0.010ms | 2.280ms |
| **asyncio** | 39,430.4 | 0.025ms | 0.060ms | 0.010ms | 1.860ms |
| **uvloop** | 44,933.8 | 0.022ms | 0.056ms | 0.010ms | 1.700ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 45,377.4 | 0.023ms | 0.047ms | 0.010ms | 2.850ms |
| **asyncio** | 33,464.0 | 0.028ms | 0.070ms | 0.010ms | 2.010ms |
| **uvloop** | 37,474.5 | 0.025ms | 0.067ms | 0.020ms | 2.210ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 17,975.0 | 0.054ms | 0.110ms | 0.040ms | 2.610ms |
| **asyncio** | 15,967.7 | 0.061ms | 0.135ms | 0.040ms | 2.180ms |
| **uvloop** | 14,163.3 | 0.069ms | 0.151ms | 0.040ms | 3.260ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 40,953 | 34,642 | 13,650 |
| **asyncio** | 41,112 | 34,780 | 12,205 |
| **uvloop** | 44,808 | 36,565 | 14,300 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 40,952.9 | 0.023ms | 0.060ms | 0.020ms | 2.440ms |
| **asyncio** | 41,112.4 | 0.023ms | 0.061ms | 0.020ms | 2.540ms |
| **uvloop** | 44,808.0 | 0.022ms | 0.054ms | 0.020ms | 1.610ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 34,642.4 | 0.026ms | 0.068ms | 0.020ms | 1.800ms |
| **asyncio** | 34,779.9 | 0.026ms | 0.072ms | 0.020ms | 2.620ms |
| **uvloop** | 36,564.7 | 0.025ms | 0.065ms | 0.020ms | 1.730ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 13,650.1 | 0.071ms | 0.171ms | 0.060ms | 2.090ms |
| **asyncio** | 12,205.0 | 0.081ms | 0.200ms | 0.050ms | 1.970ms |
| **uvloop** | 14,299.7 | 0.069ms | 0.157ms | 0.050ms | 1.790ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 62,179 | 52,095 | 20,095 |
| **asyncio** | 56,719 | 48,234 | 21,813 |
| **uvloop** | 62,752 | 51,905 | 20,634 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 62,179.0 | 0.014ms | 0.038ms | 0.010ms | 2.480ms |
| **asyncio** | 56,719.3 | 0.015ms | 0.041ms | 0.010ms | 1.770ms |
| **uvloop** | 62,752.1 | 0.014ms | 0.038ms | 0.010ms | 2.240ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 52,095.2 | 0.018ms | 0.044ms | 0.010ms | 1.950ms |
| **asyncio** | 48,234.3 | 0.022ms | 0.048ms | 0.010ms | 2.560ms |
| **uvloop** | 51,904.6 | 0.018ms | 0.045ms | 0.010ms | 2.220ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 20,095.1 | 0.048ms | 0.113ms | 0.040ms | 2.280ms |
| **asyncio** | 21,813.1 | 0.046ms | 0.111ms | 0.030ms | 1.870ms |
| **uvloop** | 20,634.1 | 0.046ms | 0.111ms | 0.040ms | 1.590ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 72,305 | 84,094 |
| **asyncio** | 54,542 | 49,994 |
| **uvloop** | 64,831 | 65,994 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 72,304.9 | 0.081ms | 0.273ms | 0.010ms | 9.480ms |
| **asyncio** | 54,542.5 | 0.108ms | 0.158ms | 0.010ms | 1.130ms |
| **uvloop** | 64,831.0 | 0.091ms | 0.189ms | 0.020ms | 2.420ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 84,093.5 | 0.129ms | 0.286ms | 0.010ms | 3.700ms |
| **asyncio** | 49,994.5 | 0.218ms | 0.411ms | 0.010ms | 11.150ms |
| **uvloop** | 65,994.3 | 0.165ms | 0.366ms | 0.040ms | 4.400ms |

