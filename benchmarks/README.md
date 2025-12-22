# Veloxloop Benchmarks

Run at: Mon 22 Dec 2025, 08:52  
Environment: Linux x86_64 (CPUs: 8)  
Python version: 3.13  
Veloxloop version: 0.1.0  

## Raw Sockets

TCP echo server with raw sockets comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 79,820 | 67,511 | 24,753 |
| **asyncio** | 49,055 | 41,057 | 21,945 |
| **uvloop** | 53,974 | 40,432 | 12,475 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 79,820.5 | 0.011ms | 0.027ms | 0.010ms | 1.740ms |
| **asyncio** | 49,055.0 | 0.021ms | 0.040ms | 0.010ms | 2.470ms |
| **uvloop** | 53,974.5 | 0.017ms | 0.038ms | 0.010ms | 0.330ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 67,510.7 | 0.012ms | 0.029ms | 0.010ms | 3.830ms |
| **asyncio** | 41,056.9 | 0.023ms | 0.046ms | 0.010ms | 2.630ms |
| **uvloop** | 40,431.8 | 0.024ms | 0.052ms | 0.010ms | 2.890ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 24,752.8 | 0.041ms | 0.064ms | 0.030ms | 3.780ms |
| **asyncio** | 21,945.2 | 0.044ms | 0.070ms | 0.030ms | 0.360ms |
| **uvloop** | 12,475.1 | 0.078ms | 0.167ms | 0.040ms | 3.570ms |


## Streams

TCP echo server with `asyncio` streams comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 53,013 | 47,680 | 18,245 |
| **asyncio** | 49,364 | 38,090 | 13,807 |
| **uvloop** | 54,033 | 42,490 | 18,098 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 53,013.2 | 0.020ms | 0.036ms | 0.010ms | 0.480ms |
| **asyncio** | 49,364.2 | 0.021ms | 0.040ms | 0.020ms | 1.240ms |
| **uvloop** | 54,032.8 | 0.021ms | 0.035ms | 0.010ms | 0.620ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 47,679.8 | 0.021ms | 0.038ms | 0.020ms | 0.690ms |
| **asyncio** | 38,090.3 | 0.024ms | 0.055ms | 0.020ms | 2.050ms |
| **uvloop** | 42,489.7 | 0.023ms | 0.048ms | 0.020ms | 2.030ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 18,245.1 | 0.054ms | 0.095ms | 0.040ms | 1.010ms |
| **asyncio** | 13,807.1 | 0.071ms | 0.133ms | 0.040ms | 2.160ms |
| **uvloop** | 18,097.6 | 0.055ms | 0.120ms | 0.040ms | 0.790ms |


## Protocol

TCP echo server with `asyncio.Protocol` comparison.


### Overview (1 concurrent connection)
| Loop | 1KB rps | 10KB rps | 100KB rps |
| --- | --- | --- | --- |
| **veloxloop** | 65,916 | 57,935 | 24,516 |
| **asyncio** | 60,893 | 53,656 | 26,131 |
| **uvloop** | 73,647 | 64,159 | 26,231 |

### 1KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 65,915.9 | 0.013ms | 0.030ms | 0.010ms | 2.040ms |
| **asyncio** | 60,892.8 | 0.014ms | 0.035ms | 0.010ms | 3.490ms |
| **uvloop** | 73,646.6 | 0.011ms | 0.029ms | 0.010ms | 2.180ms |

### 10KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 57,934.9 | 0.016ms | 0.030ms | 0.010ms | 0.590ms |
| **asyncio** | 53,656.4 | 0.018ms | 0.038ms | 0.010ms | 1.490ms |
| **uvloop** | 64,158.7 | 0.014ms | 0.030ms | 0.010ms | 0.150ms |

### 100KB Details

| Loop | RPS | Mean Latency | 99p Latency | Min | Max |
| --- | --- | --- | --- | --- | --- |
| **veloxloop** | 24,516.5 | 0.040ms | 0.069ms | 0.030ms | 3.100ms |
| **asyncio** | 26,131.2 | 0.036ms | 0.068ms | 0.030ms | 2.130ms |
| **uvloop** | 26,231.3 | 0.038ms | 0.059ms | 0.030ms | 1.610ms |

