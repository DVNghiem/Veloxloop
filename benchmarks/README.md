# Veloxloop Benchmarks

Run at: Thu 25 Dec 2025, 13:12  
Environment: Linux x86_64 (CPUs: 8)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 71,765 | 61,926 | 23,887 |
| **asyncio** | 48,550 | 41,620 | 21,400 |
| **uvloop** | 54,478 | 44,580 | 17,706 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 71,764.8 | 0.011ms | 0.027ms | 0.010ms | 3.630ms |
| **asyncio** | 48,549.5 | 0.021ms | 0.039ms | 0.010ms | 0.310ms |
| **uvloop** | 54,478.4 | 0.017ms | 0.038ms | 0.010ms | 0.200ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 61,925.7 | 0.015ms | 0.030ms | 0.010ms | 3.850ms |
| **asyncio** | 41,619.6 | 0.023ms | 0.043ms | 0.010ms | 0.960ms |
| **uvloop** | 44,580.2 | 0.022ms | 0.039ms | 0.010ms | 0.650ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 23,886.8 | 0.042ms | 0.065ms | 0.030ms | 2.890ms |
| **asyncio** | 21,400.2 | 0.045ms | 0.072ms | 0.030ms | 0.220ms |
| **uvloop** | 17,706.2 | 0.054ms | 0.083ms | 0.040ms | 2.270ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 47,607 | 40,927 | 17,010 |
| **asyncio** | 47,921 | 40,648 | 14,245 |
| **uvloop** | 52,483 | 45,578 | 17,926 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 47,607.2 | 0.021ms | 0.039ms | 0.010ms | 0.920ms |
| **asyncio** | 47,920.9 | 0.021ms | 0.039ms | 0.020ms | 2.370ms |
| **uvloop** | 52,482.8 | 0.020ms | 0.036ms | 0.010ms | 5.040ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 40,926.9 | 0.023ms | 0.042ms | 0.020ms | 2.990ms |
| **asyncio** | 40,647.9 | 0.023ms | 0.047ms | 0.020ms | 1.210ms |
| **uvloop** | 45,577.6 | 0.021ms | 0.038ms | 0.020ms | 0.710ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 17,010.2 | 0.057ms | 0.090ms | 0.040ms | 1.790ms |
| **asyncio** | 14,245.2 | 0.069ms | 0.120ms | 0.040ms | 2.090ms |
| **uvloop** | 17,925.5 | 0.054ms | 0.086ms | 0.040ms | 3.080ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 60,817 | 55,074 | 23,800 |
| **asyncio** | 66,555 | 58,499 | 27,699 |
| **uvloop** | 73,377 | 61,373 | 26,068 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 60,817.1 | 0.014ms | 0.030ms | 0.010ms | 2.920ms |
| **asyncio** | 66,554.6 | 0.012ms | 0.029ms | 0.010ms | 2.790ms |
| **uvloop** | 73,376.7 | 0.012ms | 0.029ms | 0.010ms | 1.250ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 55,073.8 | 0.019ms | 0.030ms | 0.010ms | 2.910ms |
| **asyncio** | 58,499.2 | 0.017ms | 0.030ms | 0.010ms | 0.340ms |
| **uvloop** | 61,372.8 | 0.015ms | 0.030ms | 0.010ms | 4.530ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 23,800.3 | 0.041ms | 0.060ms | 0.030ms | 1.180ms |
| **asyncio** | 27,698.7 | 0.034ms | 0.054ms | 0.030ms | 0.210ms |
| **uvloop** | 26,067.9 | 0.039ms | 0.054ms | 0.030ms | 0.730ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 4 conn | 7 conn |
| --- | --- | --- |
| **veloxloop** | 86,246 | 90,522 |
| **asyncio** | 47,412 | 48,095 |
| **uvloop** | 64,088 | 65,582 |

### 4 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 86,245.5 | 0.045ms | 0.090ms | 0.010ms | 3.630ms |
| **asyncio** | 47,412.5 | 0.083ms | 0.128ms | 0.010ms | 2.240ms |
| **uvloop** | 64,087.5 | 0.061ms | 0.091ms | 0.020ms | 2.580ms |

### 7 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 90,521.5 | 0.076ms | 0.157ms | 0.010ms | 5.400ms |
| **asyncio** | 48,095.0 | 0.144ms | 0.218ms | 0.010ms | 4.260ms |
| **uvloop** | 65,582.4 | 0.106ms | 0.149ms | 0.020ms | 8.050ms |

