import asyncio
import veloxloop
import pytest
import time


def setup_module():
    """Install VeloxLoop policy at module setup"""
    veloxloop.install()


@pytest.mark.asyncio
async def test_lifecycle():
    print('Running test_lifecycle')
    loop = asyncio.get_running_loop()
    try:
        t = loop.time()
        print(f'Loop time: {t}')
    except Exception as e:
        print(f'Loop.time failed: {e}')
        raise e

    start = loop.time()
    await asyncio.sleep(0.1)
    end = loop.time()
    print(f'Slept {end - start:.4f}s (expected ~0.1s)')
    assert 0.09 <= (end - start) <= 0.2


@pytest.mark.asyncio
async def test_tcp_echo():
    print('Running test_tcp_echo')

    async def handle_echo(reader, writer):
        data = await reader.read(100)
        writer.write(data)
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    server = await asyncio.start_server(handle_echo, '127.0.0.1', 0)
    addr = server.sockets[0].getsockname()
    print(f'Serving on {addr}')

    async with server:
        reader, writer = await asyncio.open_connection('127.0.0.1', addr[1])
        writer.write(b'Hello World!')
        await writer.drain()

        data = await reader.read(100)
        print(f'Received: {data.decode()}')
        assert data == b'Hello World!'

        writer.close()
        await writer.wait_closed()


@pytest.mark.asyncio
async def test_concurrency():
    print('Running test_concurrency')

    async def fast_task(i):
        await asyncio.sleep(0.01)
        return i

    tasks = [asyncio.create_task(fast_task(i)) for i in range(100)]
    results = await asyncio.gather(*tasks)
    assert len(results) == 100
    assert sum(results) == 4950


@pytest.mark.asyncio
async def test_cancellation():
    print('Running test_cancellation')

    async def forever():
        try:
            await asyncio.sleep(10)
        except asyncio.CancelledError:
            print('Task cancelled')
            raise

    task = asyncio.create_task(forever())
    await asyncio.sleep(0.1)
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task


@pytest.mark.asyncio
async def test_large_payload():
    print('Running test_large_payload')
    size = 1024 * 1024  # 1MB
    payload = b'a' * size

    async def handle_echo(reader, writer):
        data = await reader.readexactly(size)
        writer.write(data)
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    server = await asyncio.start_server(handle_echo, '127.0.0.1', 0)
    addr = server.sockets[0].getsockname()

    async with server:
        reader, writer = await asyncio.open_connection('127.0.0.1', addr[1])
        writer.write(payload)
        await writer.drain()

        data = await reader.readexactly(size)
        assert data == payload

        writer.close()
        await writer.wait_closed()


@pytest.mark.asyncio
async def test_call_soon_threadsafe():
    print('Running test_call_soon_threadsafe')
    loop = asyncio.get_running_loop()
    fut = asyncio.Future()

    def callback():
        print('Threadsafe callback called')
        loop.call_soon_threadsafe(lambda: fut.set_result('success'))

    import threading

    t = threading.Thread(target=callback)
    t.start()
    t.join()

    res = await fut
    assert res == 'success'
