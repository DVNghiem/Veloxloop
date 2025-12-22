# Veloxloop Benchmarks

Run at: Mon 22 Dec 2025, 21:12  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 62,644 | 50,518 | 19,434 |
| **asyncio** | 41,148 | 33,656 | 16,683 |
| **uvloop** | 43,732 | 37,452 | 14,108 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 62,644.5 | 0.013ms | 0.039ms | 0.010ms | 2.700ms |
| **asyncio** | 41,147.9 | 0.024ms | 0.067ms | 0.010ms | 1.200ms |
| **uvloop** | 43,732.2 | 0.023ms | 0.062ms | 0.010ms | 2.100ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 50,518.0 | 0.019ms | 0.045ms | 0.010ms | 3.590ms |
| **asyncio** | 33,656.1 | 0.028ms | 0.077ms | 0.010ms | 2.060ms |
| **uvloop** | 37,452.4 | 0.024ms | 0.067ms | 0.020ms | 2.500ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,434.3 | 0.048ms | 0.119ms | 0.040ms | 2.160ms |
| **asyncio** | 16,683.0 | 0.058ms | 0.150ms | 0.040ms | 2.720ms |
| **uvloop** | 14,108.5 | 0.068ms | 0.172ms | 0.040ms | 2.150ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 42,692 | 36,122 | 15,077 |
| **asyncio** | 41,096 | 34,751 | 12,120 |
| **uvloop** | 44,418 | 36,676 | 14,299 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 42,691.5 | 0.023ms | 0.063ms | 0.020ms | 2.230ms |
| **asyncio** | 41,095.8 | 0.023ms | 0.067ms | 0.020ms | 3.130ms |
| **uvloop** | 44,418.4 | 0.022ms | 0.063ms | 0.020ms | 1.830ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 36,121.5 | 0.025ms | 0.075ms | 0.020ms | 1.920ms |
| **asyncio** | 34,751.3 | 0.026ms | 0.078ms | 0.020ms | 1.910ms |
| **uvloop** | 36,675.6 | 0.025ms | 0.076ms | 0.020ms | 4.020ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 15,077.4 | 0.065ms | 0.172ms | 0.050ms | 1.220ms |
| **asyncio** | 12,119.9 | 0.081ms | 0.216ms | 0.050ms | 1.940ms |
| **uvloop** | 14,298.8 | 0.068ms | 0.172ms | 0.050ms | 1.860ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 61,520 | 48,481 | 20,894 |
| **asyncio** | 54,044 | 48,087 | 21,742 |
| **uvloop** | 62,735 | 52,666 | 20,982 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 61,520.4 | 0.013ms | 0.037ms | 0.010ms | 1.840ms |
| **asyncio** | 54,043.9 | 0.015ms | 0.047ms | 0.010ms | 1.960ms |
| **uvloop** | 62,734.9 | 0.013ms | 0.043ms | 0.010ms | 1.010ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 48,481.3 | 0.021ms | 0.045ms | 0.010ms | 2.450ms |
| **asyncio** | 48,087.0 | 0.022ms | 0.056ms | 0.020ms | 1.640ms |
| **uvloop** | 52,665.8 | 0.019ms | 0.051ms | 0.010ms | 1.950ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 20,894.1 | 0.046ms | 0.089ms | 0.040ms | 2.010ms |
| **asyncio** | 21,742.3 | 0.046ms | 0.119ms | 0.040ms | 1.280ms |
| **uvloop** | 20,981.9 | 0.046ms | 0.116ms | 0.040ms | 1.370ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop| 6 conn | 11 conn |
| --- | --- | --- |
| **veloxloop** | 87,694 | 88,184 |
| **asyncio** | 46,682 | 47,105 |
| **uvloop** | 64,084 | 64,108 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 87,694.4 | 0.068ms | 0.168ms | 0.010ms | 3.070ms |
| **asyncio** | 46,682.0 | 0.127ms | 0.292ms | 0.010ms | 3.450ms |
| **uvloop** | 64,083.9 | 0.092ms | 0.208ms | 0.040ms | 2.950ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 88,184.3 | 0.123ms | 0.321ms | 0.010ms | 4.000ms |
| **asyncio** | 47,105.2 | 0.232ms | 0.500ms | 0.010ms | 4.060ms |
| **uvloop** | 64,108.5 | 0.170ms | 0.394ms | 0.050ms | 4.350ms |

