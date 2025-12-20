# Veloxloop Benchmarks

Run at: Sat 20 Dec 2025, 21:08  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 59,442 | 50,676 | 20,588 |
| **asyncio** | 43,753 | 36,278 | 17,739 |
| **uvloop** | 47,732 | 38,872 | 14,716 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 59,442.1 | 0.013ms | 0.036ms | 0.010ms | 2.940ms |
| **asyncio** | 43,753.1 | 0.023ms | 0.049ms | 0.010ms | 0.520ms |
| **uvloop** | 47,732.3 | 0.021ms | 0.042ms | 0.010ms | 0.480ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 50,675.5 | 0.021ms | 0.039ms | 0.010ms | 3.440ms |
| **asyncio** | 36,278.2 | 0.026ms | 0.055ms | 0.010ms | 0.800ms |
| **uvloop** | 38,871.5 | 0.023ms | 0.049ms | 0.020ms | 2.180ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 20,588.3 | 0.046ms | 0.079ms | 0.040ms | 1.560ms |
| **asyncio** | 17,738.9 | 0.055ms | 0.099ms | 0.040ms | 3.470ms |
| **uvloop** | 14,715.8 | 0.066ms | 0.112ms | 0.050ms | 0.860ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 44,408 | 38,831 | 14,555 |
| **asyncio** | 45,444 | 36,249 | 12,959 |
| **uvloop** | 45,839 | 38,138 | 15,656 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 44,408.4 | 0.022ms | 0.045ms | 0.020ms | 1.880ms |
| **asyncio** | 45,444.4 | 0.021ms | 0.043ms | 0.020ms | 0.370ms |
| **uvloop** | 45,838.9 | 0.022ms | 0.044ms | 0.020ms | 1.060ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 38,831.0 | 0.023ms | 0.047ms | 0.020ms | 0.190ms |
| **asyncio** | 36,249.1 | 0.025ms | 0.052ms | 0.020ms | 3.500ms |
| **uvloop** | 38,137.7 | 0.024ms | 0.050ms | 0.020ms | 3.300ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 14,555.4 | 0.067ms | 0.116ms | 0.050ms | 1.970ms |
| **asyncio** | 12,959.1 | 0.076ms | 0.133ms | 0.050ms | 4.190ms |
| **uvloop** | 15,656.3 | 0.063ms | 0.099ms | 0.050ms | 0.610ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 63,969 | 53,852 | 22,690 |
| **asyncio** | 62,096 | 50,498 | 24,281 |
| **uvloop** | 67,981 | 55,338 | 22,603 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 63,968.9 | 0.013ms | 0.033ms | 0.010ms | 1.580ms |
| **asyncio** | 62,096.0 | 0.012ms | 0.033ms | 0.010ms | 1.410ms |
| **uvloop** | 67,980.8 | 0.012ms | 0.030ms | 0.010ms | 0.240ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 53,852.5 | 0.019ms | 0.036ms | 0.010ms | 0.930ms |
| **asyncio** | 50,497.9 | 0.021ms | 0.039ms | 0.010ms | 1.470ms |
| **uvloop** | 55,337.8 | 0.018ms | 0.036ms | 0.010ms | 1.130ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 22,690.2 | 0.042ms | 0.070ms | 0.040ms | 0.210ms |
| **asyncio** | 24,280.8 | 0.042ms | 0.068ms | 0.030ms | 0.400ms |
| **uvloop** | 22,603.3 | 0.043ms | 0.073ms | 0.040ms | 2.180ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 86,318 | 89,083 |
| **asyncio** | 52,583 | 51,994 |
| **uvloop** | 64,047 | 68,575 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 86,317.5 | 0.067ms | 0.143ms | 0.010ms | 3.420ms |
| **asyncio** | 52,582.6 | 0.112ms | 0.176ms | 0.010ms | 9.470ms |
| **uvloop** | 64,046.6 | 0.092ms | 0.159ms | 0.020ms | 5.700ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 89,082.8 | 0.122ms | 0.252ms | 0.010ms | 4.300ms |
| **asyncio** | 51,994.4 | 0.208ms | 0.360ms | 0.010ms | 28.140ms |
| **uvloop** | 68,574.7 | 0.159ms | 0.272ms | 0.020ms | 9.680ms |

