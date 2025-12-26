"""Tests for TCP Transport"""

import asyncio
import veloxloop
import pytest
import time


class TestTCPTransport:
    """Test TCP transport functionality"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_tcp_echo_server_basic(self):
        """Test basic TCP echo server"""

        async def handle_echo(reader, writer):
            data = await reader.read(100)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_echo, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                # Client
                reader, writer = await asyncio.open_connection('127.0.0.1', port)
                message = b'Hello Velox!'

                writer.write(message)
                await writer.drain()

                data = await reader.read(100)
                assert data == message

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_multiple_messages(self):
        """Test sending multiple messages through TCP"""

        async def handle_client(reader, writer):
            while True:
                data = await reader.read(100)
                if not data:
                    break
                writer.write(data)
                await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                messages = [b'msg1', b'msg2', b'msg3']
                for msg in messages:
                    writer.write(msg)
                    await writer.drain()
                    response = await reader.read(100)
                    assert response == msg

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_large_data(self):
        """Test sending large data through TCP"""

        async def handle_client(reader, writer):
            # Use readexactly to ensure we get all data (since transport is chunked)
            data = await reader.readexactly(1024 * 50)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                # Send 50KB of data
                message = b'x' * (1024 * 50)
                writer.write(message)
                await writer.drain()

                data = await reader.readexactly(1024 * 50)
                assert data == message

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_multiple_clients(self):
        """Test multiple concurrent clients"""
        clients_served = []

        async def handle_client(reader, writer):
            data = await reader.read(100)
            clients_served.append(data)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def client_task(port, message):
            reader, writer = await asyncio.open_connection('127.0.0.1', port)
            writer.write(message)
            await writer.drain()
            data = await reader.read(100)
            writer.close()
            await writer.wait_closed()
            return data

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                # Launch 5 concurrent clients
                tasks = []
                messages = [f'client{i}'.encode() for i in range(5)]
                for msg in messages:
                    task = asyncio.create_task(client_task(port, msg))
                    tasks.append(task)

                results = await asyncio.gather(*tasks)

                # Verify all clients got their responses
                assert len(results) == 5
                assert set(results) == set(messages)
                assert len(clients_served) == 5

        asyncio.run(main())

    def test_tcp_server_close(self):
        """Test proper server closing"""

        async def handle_client(reader, writer):
            data = await reader.read(100)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            # Test connection works
            reader, writer = await asyncio.open_connection('127.0.0.1', port)
            writer.write(b'test')
            await writer.drain()
            data = await reader.read(100)
            assert data == b'test'
            writer.close()
            await writer.wait_closed()

            # Close server
            server.close()
            await server.wait_closed()

            # Verify new connections fail
            try:
                await asyncio.wait_for(
                    asyncio.open_connection('127.0.0.1', port), timeout=0.5
                )
                assert False, 'Connection should have failed'
            except (ConnectionRefusedError, asyncio.TimeoutError, OSError):
                pass  # Expected

        asyncio.run(main())

    def test_tcp_reader_readline(self):
        """Test reading line-by-line from TCP"""

        async def handle_client(reader, writer):
            line = await reader.readline()
            writer.write(line)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                message = b'Hello World\n'
                writer.write(message)
                await writer.drain()

                line = await reader.readline()
                assert line == message

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_reader_readexactly(self):
        """Test reading exact number of bytes"""

        async def handle_client(reader, writer):
            data = await reader.readexactly(10)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                message = b'1234567890extra'
                writer.write(message)
                await writer.drain()

                data = await reader.readexactly(10)
                assert data == b'1234567890'

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_connection_timeout(self):
        """Test connection timeout handling"""

        async def slow_handler(reader, writer):
            # Sleep longer than timeout to trigger it
            await asyncio.sleep(1.0)
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(slow_handler, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                # Try to read with timeout (shorter than handler sleep)
                try:
                    await asyncio.wait_for(reader.read(100), timeout=0.2)
                    assert False, 'Should have timed out'
                except asyncio.TimeoutError:
                    pass  # Expected

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_partial_writes(self):
        """Test writing data in chunks"""

        async def handle_client(reader, writer):
            chunks = []
            while True:
                data = await reader.read(10)
                if not data:
                    break
                chunks.append(data)

            full_data = b''.join(chunks)
            writer.write(full_data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                # Write in small chunks
                message = b'Hello World from Velox!'
                for i in range(0, len(message), 5):
                    chunk = message[i : i + 5]
                    writer.write(chunk)
                    await writer.drain()
                    await asyncio.sleep(0.01)

                # Signal end
                writer.write_eof()

                data = await reader.read(1024)
                assert data == message

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_bidirectional_communication(self):
        """Test bidirectional communication"""

        async def handle_client(reader, writer):
            # Receive first message
            data1 = await reader.read(100)
            # Send response
            writer.write(b'ACK1:' + data1)
            await writer.drain()

            # Receive second message
            data2 = await reader.read(100)
            # Send response
            writer.write(b'ACK2:' + data2)
            await writer.drain()

            writer.close()
            await writer.wait_closed()

        async def main():
            server = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server.sockets[0].getsockname()
            port = addr[1]

            async with server:
                reader, writer = await asyncio.open_connection('127.0.0.1', port)

                # Send first message
                writer.write(b'MSG1')
                await writer.drain()
                response1 = await reader.read(100)
                assert response1 == b'ACK1:MSG1'

                # Send second message
                writer.write(b'MSG2')
                await writer.drain()
                response2 = await reader.read(100)
                assert response2 == b'ACK2:MSG2'

                writer.close()
                await writer.wait_closed()

        asyncio.run(main())

    def test_tcp_server_reuse_address(self):
        """Test server can reuse address after close"""

        async def handle_client(reader, writer):
            writer.close()
            await writer.wait_closed()

        async def main():
            # First server
            server1 = await asyncio.start_server(handle_client, '127.0.0.1', 0)
            addr = server1.sockets[0].getsockname()
            port = addr[1]
            server1.close()
            await server1.wait_closed()

            # Wait a bit
            await asyncio.sleep(0.1)

            # Second server on same port should work
            server2 = await asyncio.start_server(handle_client, '127.0.0.1', port)
            server2.close()
            await server2.wait_closed()

        asyncio.run(main())

    def test_tcp_connection_refused(self):
        """Test handling connection refused"""

        async def main():
            # Try to connect to non-existent server
            try:
                await asyncio.wait_for(
                    asyncio.open_connection('127.0.0.1', 59999), timeout=1.0
                )
                assert False, 'Should have failed'
            except (ConnectionRefusedError, asyncio.TimeoutError, OSError):
                pass  # Expected

        asyncio.run(main())

    def test_tcp_concurrent_servers(self):
        """Test multiple servers on different ports"""

        async def echo_handler(reader, writer):
            data = await reader.read(100)
            writer.write(data)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def reverse_handler(reader, writer):
            data = await reader.read(100)
            writer.write(data[::-1])
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        async def main():
            server1 = await asyncio.start_server(echo_handler, '127.0.0.1', 0)
            server2 = await asyncio.start_server(reverse_handler, '127.0.0.1', 0)

            port1 = server1.sockets[0].getsockname()[1]
            port2 = server2.sockets[0].getsockname()[1]

            async with server1, server2:
                # Test echo server
                reader, writer = await asyncio.open_connection('127.0.0.1', port1)
                writer.write(b'echo')
                await writer.drain()
                data = await reader.read(100)
                assert data == b'echo'
                writer.close()
                await writer.wait_closed()

                # Test reverse server
                reader, writer = await asyncio.open_connection('127.0.0.1', port2)
                writer.write(b'reverse')
                await writer.drain()
                data = await reader.read(100)
                assert data == b'esrever'
                writer.close()
                await writer.wait_closed()

        asyncio.run(main())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
