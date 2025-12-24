# Veloxloop Benchmarks

Run at: Wed 24 Dec 2025, 08:06  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 63,474 | 51,715 | 20,404 |
| **asyncio** | 43,512 | 35,937 | 17,288 |
| **uvloop** | 49,772 | 40,071 | 15,288 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 63,473.9 | 0.012ms | 0.037ms | 0.010ms | 3.180ms |
| **asyncio** | 43,511.8 | 0.023ms | 0.054ms | 0.010ms | 0.510ms |
| **uvloop** | 49,771.7 | 0.021ms | 0.047ms | 0.010ms | 4.440ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 51,715.3 | 0.020ms | 0.042ms | 0.010ms | 2.820ms |
| **asyncio** | 35,936.6 | 0.026ms | 0.060ms | 0.010ms | 0.790ms |
| **uvloop** | 40,070.9 | 0.023ms | 0.054ms | 0.020ms | 2.150ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 20,403.8 | 0.046ms | 0.089ms | 0.040ms | 2.140ms |
| **asyncio** | 17,288.5 | 0.056ms | 0.113ms | 0.040ms | 0.710ms |
| **uvloop** | 15,288.2 | 0.064ms | 0.120ms | 0.040ms | 0.620ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 42,073 | 36,067 | 14,658 |
| **asyncio** | 43,883 | 37,396 | 13,048 |
| **uvloop** | 47,239 | 39,326 | 15,021 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 42,073.4 | 0.022ms | 0.051ms | 0.020ms | 3.490ms |
| **asyncio** | 43,883.3 | 0.022ms | 0.050ms | 0.020ms | 0.700ms |
| **uvloop** | 47,239.3 | 0.022ms | 0.048ms | 0.020ms | 3.670ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 36,067.0 | 0.027ms | 0.057ms | 0.020ms | 0.570ms |
| **asyncio** | 37,396.1 | 0.024ms | 0.058ms | 0.020ms | 0.920ms |
| **uvloop** | 39,326.2 | 0.023ms | 0.056ms | 0.020ms | 1.230ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 14,657.6 | 0.067ms | 0.136ms | 0.050ms | 1.030ms |
| **asyncio** | 13,048.1 | 0.076ms | 0.156ms | 0.050ms | 1.940ms |
| **uvloop** | 15,021.3 | 0.066ms | 0.128ms | 0.050ms | 1.110ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 59,807 | 48,717 | 20,155 |
| **asyncio** | 60,531 | 50,887 | 23,564 |
| **uvloop** | 67,885 | 56,247 | 22,288 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 59,807.0 | 0.014ms | 0.037ms | 0.010ms | 0.590ms |
| **asyncio** | 60,531.1 | 0.013ms | 0.037ms | 0.010ms | 0.410ms |
| **uvloop** | 67,885.4 | 0.012ms | 0.034ms | 0.010ms | 0.650ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 48,716.9 | 0.021ms | 0.043ms | 0.010ms | 3.670ms |
| **asyncio** | 50,887.3 | 0.021ms | 0.042ms | 0.010ms | 1.670ms |
| **uvloop** | 56,246.9 | 0.017ms | 0.039ms | 0.010ms | 0.820ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 20,154.6 | 0.048ms | 0.096ms | 0.040ms | 1.030ms |
| **asyncio** | 23,563.5 | 0.043ms | 0.086ms | 0.030ms | 1.140ms |
| **uvloop** | 22,287.5 | 0.044ms | 0.087ms | 0.040ms | 0.530ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 91,430 | 95,744 |
| **asyncio** | 48,746 | 50,078 |
| **uvloop** | 68,514 | 68,781 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 91,430.2 | 0.065ms | 0.145ms | 0.010ms | 3.390ms |
| **asyncio** | 48,746.2 | 0.121ms | 0.218ms | 0.010ms | 3.770ms |
| **uvloop** | 68,514.4 | 0.086ms | 0.151ms | 0.030ms | 3.870ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 95,744.0 | 0.114ms | 0.258ms | 0.010ms | 3.010ms |
| **asyncio** | 50,077.9 | 0.218ms | 0.411ms | 0.010ms | 3.410ms |
| **uvloop** | 68,780.7 | 0.158ms | 0.280ms | 0.050ms | 3.340ms |

