# Veloxloop Benchmarks

Run at: Tue 17 Feb 2026, 08:37  
Environment: Linux x86_64 (CPUs: 8)  
Python version: 3.14  
Veloxloop version: 0.2.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 48,407 | 39,292 | 9,868 |
| **asyncio** | 31,609 | 25,391 | 13,916 |
| **uvloop** | 36,298 | 29,473 | 13,469 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 48,406.6 | 0.021ms | 0.037ms | 0.020ms | 3.620ms |
| **asyncio** | 31,608.6 | 0.032ms | 0.061ms | 0.010ms | 0.960ms |
| **uvloop** | 36,297.6 | 0.024ms | 0.048ms | 0.020ms | 1.060ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 39,292.2 | 0.023ms | 0.046ms | 0.020ms | 3.160ms |
| **asyncio** | 25,391.0 | 0.038ms | 0.077ms | 0.020ms | 3.250ms |
| **uvloop** | 29,472.8 | 0.034ms | 0.068ms | 0.020ms | 3.070ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 9,867.8 | 0.098ms | 0.151ms | 0.080ms | 3.370ms |
| **asyncio** | 13,915.5 | 0.068ms | 0.137ms | 0.040ms | 2.050ms |
| **uvloop** | 13,468.6 | 0.073ms | 0.101ms | 0.060ms | 3.120ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 34,674 | 32,248 | 8,310 |
| **asyncio** | 34,351 | 30,649 | 10,173 |
| **uvloop** | 36,601 | 29,129 | 12,272 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 34,673.8 | 0.025ms | 0.059ms | 0.020ms | 2.790ms |
| **asyncio** | 34,350.8 | 0.027ms | 0.052ms | 0.020ms | 3.210ms |
| **uvloop** | 36,601.4 | 0.024ms | 0.048ms | 0.020ms | 3.160ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 32,247.5 | 0.030ms | 0.058ms | 0.020ms | 0.920ms |
| **asyncio** | 30,649.3 | 0.032ms | 0.056ms | 0.030ms | 1.510ms |
| **uvloop** | 29,129.1 | 0.034ms | 0.068ms | 0.020ms | 4.680ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 8,310.2 | 0.117ms | 0.247ms | 0.090ms | 2.400ms |
| **asyncio** | 10,173.4 | 0.095ms | 0.182ms | 0.060ms | 0.820ms |
| **uvloop** | 12,271.7 | 0.079ms | 0.164ms | 0.060ms | 1.110ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 43,175 | 41,358 | 10,408 |
| **asyncio** | 41,472 | 38,219 | 15,942 |
| **uvloop** | 46,447 | 37,637 | 16,608 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 43,175.1 | 0.022ms | 0.055ms | 0.010ms | 3.160ms |
| **asyncio** | 41,471.6 | 0.023ms | 0.060ms | 0.020ms | 2.230ms |
| **uvloop** | 46,447.3 | 0.021ms | 0.040ms | 0.010ms | 2.340ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 41,358.2 | 0.022ms | 0.040ms | 0.020ms | 2.350ms |
| **asyncio** | 38,218.6 | 0.024ms | 0.054ms | 0.020ms | 1.910ms |
| **uvloop** | 37,636.6 | 0.025ms | 0.079ms | 0.020ms | 3.580ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 10,408.4 | 0.093ms | 0.224ms | 0.070ms | 2.060ms |
| **asyncio** | 15,942.2 | 0.059ms | 0.144ms | 0.050ms | 2.030ms |
| **uvloop** | 16,607.6 | 0.057ms | 0.137ms | 0.050ms | 0.840ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 4 conn | 7 conn |
| --- | --- | --- |
| **veloxloop** | 65,207 | 68,874 |
| **asyncio** | 37,880 | 37,122 |
| **uvloop** | 46,273 | 46,964 |

### 4 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 65,206.6 | 0.060ms | 0.127ms | 0.020ms | 4.670ms |
| **asyncio** | 37,879.5 | 0.102ms | 0.143ms | 0.010ms | 1.240ms |
| **uvloop** | 46,273.0 | 0.084ms | 0.142ms | 0.020ms | 4.080ms |

### 7 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 68,874.0 | 0.097ms | 0.210ms | 0.020ms | 6.960ms |
| **asyncio** | 37,121.9 | 0.183ms | 0.292ms | 0.010ms | 2.370ms |
| **uvloop** | 46,964.2 | 0.144ms | 0.227ms | 0.040ms | 3.090ms |

