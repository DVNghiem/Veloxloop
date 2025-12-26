import asyncio
import os
import tempfile
import pytest
import veloxloop


def setup_module():
    veloxloop.install()


@pytest.mark.asyncio
async def test_sendfile_basic():
    loop = asyncio.get_event_loop()

    # Create a temporary file
    content = b'Hello, this is a test file for sendfile!' * 1000  # ~40KB
    with tempfile.NamedTemporaryFile(delete=False) as f:
        f.write(content)
        file_path = f.name

    received_content = b''
    server_finished = asyncio.Event()

    async def handle_client(reader, writer):
        nonlocal received_content
        while True:
            data = await reader.read(4096)
            if not data:
                break
            received_content += data
        writer.close()
        await writer.wait_closed()
        server_finished.set()

    server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
    addr = server.sockets[0].getsockname()

    reader, writer = await asyncio.open_connection(addr[0], addr[1])

    with open(file_path, 'rb') as f:
        # transport is the writer's transport
        transport = writer.transport
        await loop.sendfile(transport, f)

    writer.close()
    await writer.wait_closed()

    await server_finished.wait()
    server.close()
    await server.wait_closed()

    assert received_content == content
    os.unlink(file_path)


@pytest.mark.asyncio
async def test_sendfile_offset_count():
    loop = asyncio.get_event_loop()

    # Create a temporary file
    content = b'0123456789abcdefghijklmnopqrstuvwxyz'  # 36 bytes
    with tempfile.NamedTemporaryFile(delete=False) as f:
        f.write(content)
        file_path = f.name

    received_content = b''
    server_finished = asyncio.Event()

    async def handle_client(reader, writer):
        nonlocal received_content
        while True:
            data = await reader.read(4096)
            if not data:
                break
            received_content += data
        writer.close()
        await writer.wait_closed()
        server_finished.set()

    server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
    addr = server.sockets[0].getsockname()

    reader, writer = await asyncio.open_connection(addr[0], addr[1])

    offset = 10
    count = 10
    expected_content = content[offset : offset + count]

    with open(file_path, 'rb') as f:
        transport = writer.transport
        await loop.sendfile(transport, f, offset=offset, count=count)

    writer.close()
    await writer.wait_closed()

    await server_finished.wait()
    server.close()
    await server.wait_closed()

    assert received_content == expected_content
    os.unlink(file_path)


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
