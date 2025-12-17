# Veloxloop Benchmarks

Run at: Wed 17 Dec 2025, 23:23  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 61,561 | 55,243 | 21,798 |
| **asyncio** | 43,154 | 38,011 | 17,970 |
| **uvloop** | 50,398 | 42,836 | 15,431 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 61,560.7 | 0.013ms | 0.036ms | 0.010ms | 3.510ms |
| **asyncio** | 43,154.5 | 0.023ms | 0.049ms | 0.010ms | 1.820ms |
| **uvloop** | 50,398.1 | 0.020ms | 0.040ms | 0.010ms | 0.340ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 55,242.8 | 0.020ms | 0.035ms | 0.010ms | 0.730ms |
| **asyncio** | 38,011.4 | 0.025ms | 0.050ms | 0.010ms | 0.340ms |
| **uvloop** | 42,836.5 | 0.021ms | 0.046ms | 0.020ms | 0.380ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 21,797.6 | 0.042ms | 0.069ms | 0.040ms | 0.340ms |
| **asyncio** | 17,969.8 | 0.053ms | 0.086ms | 0.040ms | 0.760ms |
| **uvloop** | 15,431.4 | 0.064ms | 0.100ms | 0.040ms | 0.320ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 45,907 | 37,948 | 15,296 |
| **asyncio** | 48,032 | 39,359 | 13,992 |
| **uvloop** | 51,392 | 42,268 | 16,298 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 45,906.7 | 0.022ms | 0.043ms | 0.020ms | 3.160ms |
| **asyncio** | 48,031.6 | 0.021ms | 0.039ms | 0.020ms | 0.390ms |
| **uvloop** | 51,392.3 | 0.021ms | 0.038ms | 0.020ms | 0.510ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 37,947.6 | 0.024ms | 0.049ms | 0.020ms | 0.380ms |
| **asyncio** | 39,359.3 | 0.023ms | 0.048ms | 0.020ms | 0.460ms |
| **uvloop** | 42,268.5 | 0.021ms | 0.042ms | 0.020ms | 0.280ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 15,296.0 | 0.064ms | 0.099ms | 0.050ms | 0.580ms |
| **asyncio** | 13,992.3 | 0.071ms | 0.114ms | 0.050ms | 0.230ms |
| **uvloop** | 16,297.5 | 0.062ms | 0.093ms | 0.050ms | 0.420ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 63,566 | 53,752 | 21,032 |
| **asyncio** | 58,395 | 45,398 | 21,453 |
| **uvloop** | 64,300 | 53,124 | 20,668 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 63,566.1 | 0.013ms | 0.036ms | 0.010ms | 3.270ms |
| **asyncio** | 58,395.4 | 0.014ms | 0.037ms | 0.010ms | 1.260ms |
| **uvloop** | 64,300.2 | 0.013ms | 0.036ms | 0.010ms | 1.710ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 53,752.5 | 0.018ms | 0.038ms | 0.010ms | 1.540ms |
| **asyncio** | 45,398.2 | 0.022ms | 0.048ms | 0.010ms | 2.410ms |
| **uvloop** | 53,123.7 | 0.018ms | 0.040ms | 0.010ms | 1.970ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 21,032.3 | 0.045ms | 0.089ms | 0.040ms | 1.250ms |
| **asyncio** | 21,453.3 | 0.046ms | 0.099ms | 0.030ms | 2.230ms |
| **uvloop** | 20,668.0 | 0.047ms | 0.097ms | 0.040ms | 2.020ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 88,524 | 91,138 |
| **asyncio** | 53,559 | 57,939 |
| **uvloop** | 77,115 | 77,111 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 88,524.3 | 0.066ms | 0.136ms | 0.010ms | 0.710ms |
| **asyncio** | 53,558.8 | 0.110ms | 0.170ms | 0.010ms | 1.420ms |
| **uvloop** | 77,115.1 | 0.076ms | 0.102ms | 0.040ms | 0.380ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 91,137.6 | 0.119ms | 0.242ms | 0.010ms | 1.710ms |
| **asyncio** | 57,939.4 | 0.188ms | 0.257ms | 0.010ms | 1.160ms |
| **uvloop** | 77,110.9 | 0.142ms | 0.185ms | 0.060ms | 0.810ms |

