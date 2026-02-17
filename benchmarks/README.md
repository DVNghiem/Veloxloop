# Veloxloop Benchmarks

Run at: Tue 17 Feb 2026, 17:28  
Environment: Linux x86_64 (CPUs: 8)  
Python version: 3.14  
Veloxloop version: 0.2.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 51,512 | 46,062 | 16,971 |
| **asyncio** | 42,922 | 37,591 | 18,825 |
| **uvloop** | 46,064 | 39,509 | 16,107 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 51,511.9 | 0.021ms | 0.038ms | 0.010ms | 3.830ms |
| **asyncio** | 42,922.2 | 0.022ms | 0.041ms | 0.010ms | 1.220ms |
| **uvloop** | 46,064.4 | 0.021ms | 0.039ms | 0.010ms | 2.920ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 46,062.5 | 0.021ms | 0.037ms | 0.010ms | 0.800ms |
| **asyncio** | 37,590.7 | 0.025ms | 0.047ms | 0.010ms | 2.160ms |
| **uvloop** | 39,509.3 | 0.023ms | 0.040ms | 0.020ms | 1.320ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 16,971.3 | 0.058ms | 0.079ms | 0.050ms | 2.970ms |
| **asyncio** | 18,825.2 | 0.052ms | 0.086ms | 0.030ms | 2.400ms |
| **uvloop** | 16,106.9 | 0.060ms | 0.107ms | 0.040ms | 2.760ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 46,981 | 40,377 | 15,453 |
| **asyncio** | 46,246 | 37,200 | 13,098 |
| **uvloop** | 41,318 | 37,050 | 17,196 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 46,981.2 | 0.022ms | 0.039ms | 0.010ms | 2.150ms |
| **asyncio** | 46,245.9 | 0.021ms | 0.038ms | 0.020ms | 3.140ms |
| **uvloop** | 41,318.2 | 0.023ms | 0.054ms | 0.020ms | 3.170ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 40,377.0 | 0.023ms | 0.046ms | 0.020ms | 4.170ms |
| **asyncio** | 37,200.0 | 0.024ms | 0.053ms | 0.020ms | 2.390ms |
| **uvloop** | 37,049.8 | 0.025ms | 0.057ms | 0.020ms | 3.450ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 15,453.1 | 0.063ms | 0.113ms | 0.040ms | 2.180ms |
| **asyncio** | 13,098.1 | 0.074ms | 0.139ms | 0.040ms | 4.060ms |
| **uvloop** | 17,196.0 | 0.056ms | 0.098ms | 0.040ms | 2.620ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 67,584 | 55,570 | 25,119 |
| **asyncio** | 56,210 | 44,582 | 20,436 |
| **uvloop** | 60,377 | 57,216 | 22,820 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 67,583.8 | 0.012ms | 0.029ms | 0.010ms | 2.080ms |
| **asyncio** | 56,210.1 | 0.016ms | 0.030ms | 0.010ms | 3.030ms |
| **uvloop** | 60,376.9 | 0.014ms | 0.030ms | 0.010ms | 3.700ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 55,570.4 | 0.018ms | 0.030ms | 0.010ms | 2.480ms |
| **asyncio** | 44,582.3 | 0.022ms | 0.048ms | 0.010ms | 3.070ms |
| **uvloop** | 57,216.0 | 0.018ms | 0.030ms | 0.010ms | 2.050ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 25,119.2 | 0.040ms | 0.056ms | 0.030ms | 2.260ms |
| **asyncio** | 20,436.0 | 0.046ms | 0.093ms | 0.040ms | 4.030ms |
| **uvloop** | 22,820.3 | 0.042ms | 0.063ms | 0.040ms | 2.110ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 4 conn | 7 conn |
| --- | --- | --- |
| **veloxloop** | 66,902 | 80,701 |
| **asyncio** | 46,156 | 50,771 |
| **uvloop** | 68,411 | 71,452 |

### 4 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 66,901.6 | 0.057ms | 0.129ms | 0.010ms | 7.300ms |
| **asyncio** | 46,156.3 | 0.085ms | 0.124ms | 0.010ms | 26.290ms |
| **uvloop** | 68,410.9 | 0.056ms | 0.083ms | 0.020ms | 3.350ms |

### 7 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 80,700.7 | 0.085ms | 0.177ms | 0.010ms | 4.150ms |
| **asyncio** | 50,770.9 | 0.135ms | 0.195ms | 0.010ms | 1.290ms |
| **uvloop** | 71,452.4 | 0.095ms | 0.131ms | 0.030ms | 2.760ms |

