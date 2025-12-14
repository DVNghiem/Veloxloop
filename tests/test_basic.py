import asyncio
import veloxloop
import sys

async def test_lifecycle():
    print("Running test_lifecycle")
    loop = asyncio.get_running_loop()
    try:
        t = loop.time()
        print(f"Loop time: {t}")
    except Exception as e:
        print(f"Loop.time failed: {e}")
        raise e
        
    start = loop.time()
    await asyncio.sleep(0.1)
    end = loop.time()
    print(f"Slept {end - start:.4f}s (expected ~0.1s)")

async def test_tcp_echo():
    print("Running test_tcp_echo")
    
    async def handle_echo(reader, writer):
        print("Server accepted connection")
        data = await reader.read(100)
        print(f"Server received: {data}")
        writer.write(data)
        await writer.drain()
        print("Server sent data back")
        writer.close()
        await writer.wait_closed()

    server = await asyncio.start_server(handle_echo, '127.0.0.1', 8888)
    addr = server.sockets[0].getsockname()
    print(f"Serving on {addr}")

    async with server:
        # Client
        print("Connecting to server...")
        reader, writer = await asyncio.open_connection('127.0.0.1', 8888)
        message = b"Hello Velox!"
        
        print("Client writing...")
        writer.write(message)
        await writer.drain()
        
        print("Client reading...")
        data = await reader.read(100)
        print(f"Client received: {data}")
        assert data == message
        
        print("Closing client...")
        writer.close()
        await writer.wait_closed()
        
    print("Echo test passed")

async def main():
    print(f"Using loop: {asyncio.get_running_loop()}")
    await test_lifecycle()
    await test_tcp_echo()

if __name__ == "__main__":
    veloxloop.install()
    try:
        asyncio.run(main())
    except Exception as e:
        print(f"Test failed: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
