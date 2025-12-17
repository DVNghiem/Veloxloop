# Veloxloop Benchmarks

Run at: Wed 17 Dec 2025, 17:25  
Environment: Linux x86_64 (CPUs: 12)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)

| Loop | 1KB rps | 10KB rps | 100KB rps |

| --- | --- | --- | --- |
| **veloxloop** | 53,711 | 45,948 | 19,088 |
| **asyncio** | 40,544 | 34,186 | 17,296 |
| **uvloop** | 45,993 | 38,923 | 14,538 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 53,711.4 | 0.016ms | 0.039ms | 0.010ms | 3.370ms |
| **asyncio** | 40,543.9 | 0.024ms | 0.055ms | 0.010ms | 1.820ms |
| **uvloop** | 45,992.6 | 0.022ms | 0.050ms | 0.010ms | 2.730ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 45,947.9 | 0.022ms | 0.044ms | 0.010ms | 4.060ms |
| **asyncio** | 34,186.4 | 0.027ms | 0.061ms | 0.010ms | 2.990ms |
| **uvloop** | 38,923.0 | 0.024ms | 0.056ms | 0.020ms | 1.810ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 19,088.0 | 0.050ms | 0.098ms | 0.040ms | 2.910ms |
| **asyncio** | 17,295.8 | 0.056ms | 0.116ms | 0.040ms | 1.550ms |
| **uvloop** | 14,537.9 | 0.067ms | 0.126ms | 0.040ms | 1.720ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)

| Loop | 1KB rps | 10KB rps | 100KB rps |

| --- | --- | --- | --- |
| **veloxloop** | 46,815 | 29,547 | 5,699 |
| **asyncio** | 46,075 | 37,505 | 13,991 |
| **uvloop** | 48,108 | 39,741 | 16,342 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 46,814.6 | 0.022ms | 0.049ms | 0.020ms | 1.960ms |
| **asyncio** | 46,075.4 | 0.021ms | 0.040ms | 0.020ms | 1.120ms |
| **uvloop** | 48,107.5 | 0.022ms | 0.041ms | 0.020ms | 1.430ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 29,547.4 | 0.032ms | 0.061ms | 0.030ms | 0.310ms |
| **asyncio** | 37,504.9 | 0.025ms | 0.050ms | 0.020ms | 1.290ms |
| **uvloop** | 39,741.1 | 0.023ms | 0.048ms | 0.020ms | 1.010ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 5,698.8 | 0.174ms | 0.285ms | 0.150ms | 0.880ms |
| **asyncio** | 13,990.8 | 0.071ms | 0.123ms | 0.050ms | 2.650ms |
| **uvloop** | 16,341.8 | 0.061ms | 0.096ms | 0.050ms | 2.040ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)

| Loop | 1KB rps | 10KB rps | 100KB rps |

| --- | --- | --- | --- |
| **veloxloop** | 66,661 | 41,365 | 5,922 |
| **asyncio** | 59,501 | 52,071 | 22,300 |
| **uvloop** | 63,361 | 53,516 | 21,510 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 66,660.6 | 0.013ms | 0.030ms | 0.010ms | 1.060ms |
| **asyncio** | 59,501.3 | 0.014ms | 0.034ms | 0.010ms | 0.350ms |
| **uvloop** | 63,361.2 | 0.014ms | 0.034ms | 0.010ms | 1.620ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 41,364.6 | 0.022ms | 0.046ms | 0.020ms | 1.490ms |
| **asyncio** | 52,071.1 | 0.021ms | 0.039ms | 0.010ms | 2.050ms |
| **uvloop** | 53,516.5 | 0.018ms | 0.039ms | 0.010ms | 1.310ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 5,921.7 | 0.168ms | 0.270ms | 0.150ms | 2.270ms |
| **asyncio** | 22,300.1 | 0.045ms | 0.094ms | 0.030ms | 2.030ms |
| **uvloop** | 21,510.3 | 0.044ms | 0.093ms | 0.040ms | 0.980ms |


## Concurrency Scaling

TCP echo server performance with different concurrency levels (1KB messages).


### Overview

| Loop | 6 conn | 11 conn | |

| --- | --- | --- |
| **veloxloop** | 77,226 | 80,509 |
| **asyncio** | 46,774 | 48,289 |
| **uvloop** | 66,297 | 67,605 |

### 6 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 77,226.1 | 0.076ms | 0.165ms | 0.010ms | 3.560ms |
| **asyncio** | 46,773.8 | 0.127ms | 0.219ms | 0.010ms | 3.230ms |
| **uvloop** | 66,296.8 | 0.089ms | 0.150ms | 0.030ms | 3.680ms |

### 11 Concurrent Connections

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |

| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 80,509.0 | 0.135ms | 0.297ms | 0.010ms | 4.080ms |
| **asyncio** | 48,289.4 | 0.226ms | 0.399ms | 0.010ms | 4.190ms |
| **uvloop** | 67,605.4 | 0.161ms | 0.273ms | 0.040ms | 3.760ms |

