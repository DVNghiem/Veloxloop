# Veloxloop Benchmarks

Run at: Mon 22 Dec 2025, 00:00  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 34,309 | 29,714 | 12,938 |
| **asyncio** | 39,640 | 33,237 | 15,864 |
| **uvloop** | 45,083 | 36,836 | 14,351 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 34,309.0 | 0.026ms | 0.084ms | 0.010ms | 3.650ms |
| **asyncio** | 39,640.0 | 0.024ms | 0.067ms | 0.010ms | 2.080ms |
| **uvloop** | 45,082.9 | 0.023ms | 0.058ms | 0.010ms | 2.430ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 29,713.5 | 0.033ms | 0.090ms | 0.010ms | 3.390ms |
| **asyncio** | 33,236.8 | 0.028ms | 0.076ms | 0.010ms | 1.840ms |
| **uvloop** | 36,835.7 | 0.025ms | 0.065ms | 0.020ms | 2.090ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 12,937.9 | 0.076ms | 0.165ms | 0.040ms | 3.800ms |
| **asyncio** | 15,863.6 | 0.061ms | 0.162ms | 0.040ms | 2.010ms |
| **uvloop** | 14,350.8 | 0.068ms | 0.150ms | 0.040ms | 2.550ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 41,324 | 34,313 | 14,023 |
| **asyncio** | 40,812 | 34,711 | 12,256 |
| **uvloop** | 44,575 | 36,733 | 14,245 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 41,323.7 | 0.023ms | 0.059ms | 0.020ms | 2.440ms |
| **asyncio** | 40,811.6 | 0.023ms | 0.066ms | 0.020ms | 2.150ms |
| **uvloop** | 44,574.7 | 0.022ms | 0.058ms | 0.020ms | 2.010ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 34,312.9 | 0.028ms | 0.068ms | 0.020ms | 2.630ms |
| **asyncio** | 34,710.9 | 0.026ms | 0.072ms | 0.020ms | 2.390ms |
| **uvloop** | 36,733.3 | 0.024ms | 0.068ms | 0.020ms | 2.080ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 14,023.3 | 0.070ms | 0.166ms | 0.050ms | 2.080ms |
| **asyncio** | 12,256.2 | 0.080ms | 0.201ms | 0.050ms | 2.630ms |
| **uvloop** | 14,244.8 | 0.069ms | 0.172ms | 0.050ms | 2.260ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 54,988 | 47,963 | 19,400 |
| **asyncio** | 56,660 | 47,736 | 22,114 |
| **uvloop** | 61,967 | 52,362 | 20,844 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 54,988.4 | 0.015ms | 0.048ms | 0.010ms | 3.110ms |
| **asyncio** | 56,659.6 | 0.014ms | 0.045ms | 0.010ms | 2.060ms |
| **uvloop** | 61,967.3 | 0.013ms | 0.040ms | 0.010ms | 3.090ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 47,963.2 | 0.022ms | 0.048ms | 0.020ms | 2.380ms |
| **asyncio** | 47,735.8 | 0.022ms | 0.054ms | 0.010ms | 2.290ms |
| **uvloop** | 52,361.7 | 0.019ms | 0.047ms | 0.010ms | 2.150ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,399.5 | 0.049ms | 0.112ms | 0.040ms | 2.120ms |
| **asyncio** | 22,114.2 | 0.045ms | 0.104ms | 0.030ms | 1.990ms |
| **uvloop** | 20,843.7 | 0.046ms | 0.107ms | 0.040ms | 1.930ms |

