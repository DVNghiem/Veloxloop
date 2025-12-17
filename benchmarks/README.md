# Veloxloop Benchmarks

Run at: Wed 17 Dec 2025, 22:35  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 50,991 | 42,582 | 17,849 |
| **asyncio** | 37,823 | 31,853 | 15,969 |
| **uvloop** | 41,844 | 35,281 | 13,769 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 50,990.6 | 0.016ms | 0.042ms | 0.010ms | 3.860ms |
| **asyncio** | 37,822.9 | 0.026ms | 0.070ms | 0.010ms | 2.540ms |
| **uvloop** | 41,843.7 | 0.024ms | 0.067ms | 0.010ms | 2.640ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 42,581.8 | 0.024ms | 0.050ms | 0.010ms | 3.480ms |
| **asyncio** | 31,853.0 | 0.029ms | 0.078ms | 0.010ms | 2.370ms |
| **uvloop** | 35,281.3 | 0.026ms | 0.072ms | 0.020ms | 2.290ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 17,848.8 | 0.054ms | 0.130ms | 0.040ms | 3.960ms |
| **asyncio** | 15,968.7 | 0.060ms | 0.159ms | 0.040ms | 2.600ms |
| **uvloop** | 13,769.0 | 0.071ms | 0.163ms | 0.040ms | 3.540ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 39,801 | 33,510 | 12,547 |
| **asyncio** | 39,409 | 33,026 | 11,646 |
| **uvloop** | 41,936 | 34,392 | 13,701 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 39,801.0 | 0.024ms | 0.065ms | 0.020ms | 3.380ms |
| **asyncio** | 39,408.8 | 0.024ms | 0.069ms | 0.020ms | 2.400ms |
| **uvloop** | 41,936.0 | 0.023ms | 0.061ms | 0.020ms | 3.200ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 33,509.9 | 0.028ms | 0.075ms | 0.020ms | 2.390ms |
| **asyncio** | 33,026.1 | 0.028ms | 0.077ms | 0.020ms | 2.500ms |
| **uvloop** | 34,392.2 | 0.027ms | 0.074ms | 0.020ms | 2.450ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 12,547.4 | 0.078ms | 0.179ms | 0.060ms | 2.760ms |
| **asyncio** | 11,646.2 | 0.084ms | 0.221ms | 0.050ms | 2.720ms |
| **uvloop** | 13,701.3 | 0.071ms | 0.177ms | 0.050ms | 2.290ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 54,724 | 49,347 | 19,405 |
| **asyncio** | 53,956 | 46,155 | 20,742 |
| **uvloop** | 58,030 | 50,800 | 20,208 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 54,724.1 | 0.016ms | 0.054ms | 0.010ms | 3.160ms |
| **asyncio** | 53,955.5 | 0.016ms | 0.048ms | 0.010ms | 2.320ms |
| **uvloop** | 58,030.5 | 0.015ms | 0.044ms | 0.010ms | 4.400ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 49,346.7 | 0.020ms | 0.050ms | 0.010ms | 3.460ms |
| **asyncio** | 46,155.4 | 0.022ms | 0.055ms | 0.010ms | 2.480ms |
| **uvloop** | 50,799.9 | 0.019ms | 0.049ms | 0.010ms | 2.450ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,405.2 | 0.049ms | 0.141ms | 0.040ms | 2.300ms |
| **asyncio** | 20,741.8 | 0.048ms | 0.122ms | 0.030ms | 3.510ms |
| **uvloop** | 20,207.8 | 0.048ms | 0.120ms | 0.040ms | 2.740ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop | 6 conn | 11 conn | |
| --- | --- | --- |
| **veloxloop** | 71,430 | 74,132 |
| **asyncio** | 44,516 | 46,369 |
| **uvloop** | 62,430 | 63,134 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 71,430.0 | 0.082ms | 0.219ms | 0.010ms | 10.030ms |
| **asyncio** | 44,516.0 | 0.133ms | 0.411ms | 0.010ms | 12.860ms |
| **uvloop** | 62,430.3 | 0.094ms | 0.220ms | 0.030ms | 3.130ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 74,132.3 | 0.147ms | 0.432ms | 0.010ms | 4.220ms |
| **asyncio** | 46,369.2 | 0.235ms | 0.656ms | 0.010ms | 6.050ms |
| **uvloop** | 63,134.4 | 0.172ms | 0.445ms | 0.030ms | 6.520ms |

