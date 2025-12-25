# Veloxloop Benchmarks

Run at: Thu 25 Dec 2025, 23:41  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 70,497 | 60,809 | 22,613 |
| **asyncio** | 48,599 | 40,687 | 19,606 |
| **uvloop** | 54,034 | 43,173 | 16,506 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 70,496.6 | 0.011ms | 0.027ms | 0.010ms | 0.120ms |
| **asyncio** | 48,598.6 | 0.021ms | 0.040ms | 0.010ms | 0.550ms |
| **uvloop** | 54,033.6 | 0.020ms | 0.035ms | 0.010ms | 0.160ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 60,808.7 | 0.016ms | 0.030ms | 0.010ms | 3.150ms |
| **asyncio** | 40,687.3 | 0.024ms | 0.042ms | 0.010ms | 0.180ms |
| **uvloop** | 43,172.7 | 0.021ms | 0.040ms | 0.020ms | 0.180ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 22,613.1 | 0.041ms | 0.060ms | 0.040ms | 0.500ms |
| **asyncio** | 19,606.2 | 0.051ms | 0.070ms | 0.040ms | 0.210ms |
| **uvloop** | 16,505.8 | 0.061ms | 0.086ms | 0.040ms | 0.490ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 51,497 | 42,075 | 10,498 |
| **asyncio** | 48,450 | 36,281 | 14,407 |
| **uvloop** | 48,726 | 39,307 | 15,463 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 51,497.0 | 0.020ms | 0.036ms | 0.010ms | 0.600ms |
| **asyncio** | 48,449.8 | 0.021ms | 0.037ms | 0.020ms | 0.150ms |
| **uvloop** | 48,726.4 | 0.021ms | 0.040ms | 0.020ms | 1.490ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 42,075.0 | 0.021ms | 0.040ms | 0.020ms | 2.050ms |
| **asyncio** | 36,281.3 | 0.025ms | 0.054ms | 0.020ms | 0.840ms |
| **uvloop** | 39,306.6 | 0.023ms | 0.049ms | 0.020ms | 1.020ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 10,498.4 | 0.094ms | 0.154ms | 0.080ms | 1.350ms |
| **asyncio** | 14,407.0 | 0.069ms | 0.102ms | 0.050ms | 0.640ms |
| **uvloop** | 15,462.9 | 0.065ms | 0.107ms | 0.050ms | 2.160ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 73,901 | 61,783 | 14,652 |
| **asyncio** | 66,702 | 56,513 | 26,273 |
| **uvloop** | 73,409 | 60,983 | 24,346 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 73,901.3 | 0.011ms | 0.027ms | 0.010ms | 1.080ms |
| **asyncio** | 66,701.9 | 0.011ms | 0.029ms | 0.010ms | 0.140ms |
| **uvloop** | 73,408.6 | 0.011ms | 0.027ms | 0.010ms | 0.140ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 61,783.4 | 0.015ms | 0.030ms | 0.010ms | 0.740ms |
| **asyncio** | 56,513.4 | 0.020ms | 0.030ms | 0.010ms | 0.160ms |
| **uvloop** | 60,983.2 | 0.015ms | 0.030ms | 0.010ms | 0.470ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 14,652.3 | 0.066ms | 0.104ms | 0.060ms | 0.510ms |
| **asyncio** | 26,273.4 | 0.038ms | 0.055ms | 0.030ms | 0.160ms |
| **uvloop** | 24,346.3 | 0.041ms | 0.059ms | 0.030ms | 0.230ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 101,082 | 98,644 |
| **asyncio** | 56,103 | 57,098 |
| **uvloop** | 69,835 | 79,608 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 101,081.8 | 0.060ms | 0.124ms | 0.010ms | 17.440ms |
| **asyncio** | 56,103.1 | 0.105ms | 0.147ms | 0.010ms | 1.130ms |
| **uvloop** | 69,835.2 | 0.085ms | 0.130ms | 0.030ms | 3.560ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 98,644.1 | 0.110ms | 0.240ms | 0.010ms | 5.050ms |
| **asyncio** | 57,097.6 | 0.191ms | 0.274ms | 0.010ms | 3.560ms |
| **uvloop** | 79,608.3 | 0.136ms | 0.193ms | 0.040ms | 3.780ms |

