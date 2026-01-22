import asyncio
import veloxloop
import time
import os

async def run_benchmark(size_mb=10, chunk_size=64*1024):
    veloxloop.install()
    loop = asyncio.get_event_loop()
    
    total_bytes = size_mb * 1024 * 1024
    data = os.urandom(chunk_size)
    chunks_count = total_bytes // chunk_size
    
    received_bytes = 0
    done = asyncio.Event()

    class EchoServer(asyncio.Protocol):
        def connection_made(self, transport):
            self.transport = transport

        def data_received(self, data):
            # In zero-copy, 'data' is a VeloxBuffer
            self.transport.write(data)

    class BenchmarkClient(asyncio.Protocol):
        def connection_made(self, transport):
            self.transport = transport
            self.start_time = time.perf_counter()
            self.send_next()

        def send_next(self):
            nonlocal received_bytes
            for _ in range(10): # Pipeline some chunks
                self.transport.write(data)
        
        def data_received(self, data):
            nonlocal received_bytes
            received_bytes += len(data)
            if received_bytes >= total_bytes:
                self.end_time = time.perf_counter()
                done.set()
            else:
                self.send_next()

    server = await loop.create_server(EchoServer, '127.0.0.1', 8888)
    
    transport, protocol = await loop.create_connection(BenchmarkClient, '127.0.0.1', 8888)
    
    await done.wait()
    
    duration = protocol.end_time - protocol.start_time
    throughput = (total_bytes * 2) / (1024 * 1024 * duration) # *2 for both directions
    
    print(f"Benchmark: {size_mb}MB transfer")
    print(f"Duration: {duration:.4f}s")
    print(f"Throughput: {throughput:.2f} MB/s")
    
    transport.close()
    server.close()
    await server.wait_closed()

if __name__ == "__main__":
    asyncio.run(run_benchmark(size_mb=100))
