"""Test socket options functionality."""

import asyncio
import pytest
import socket
import veloxloop


class SimpleProtocol(asyncio.Protocol):
    """Simple protocol for testing."""

    def __init__(self):
        self.transport = None
        self.data = b''

    def connection_made(self, transport):
        self.transport = transport

    def data_received(self, data):
        self.data += data

    def eof_received(self):
        return False

    def connection_lost(self, exc):
        pass


class TestSocketOptions:
    """Test socket options on TCP transports."""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_tcp_nodelay_option(self):
        """Test setting TCP_NODELAY on a transport."""

        async def run_test():
            loop = asyncio.get_event_loop()

            # Create a server
            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            # Get server address
            addr = server.sockets[0].getsockname()

            # Connect to server
            reader, writer = await asyncio.open_connection(addr[0], addr[1])

            # Get the transport from writer
            transport = writer.transport

            # Set TCP_NODELAY
            transport.set_tcp_nodelay(True)

            # Send some data
            writer.write(b'test data')
            await writer.drain()

            # Close
            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    def test_keepalive_option(self):
        """Test setting SO_KEEPALIVE on a transport."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set SO_KEEPALIVE
            transport.set_keepalive(True)

            writer.write(b'keepalive test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    def test_reuse_address_option(self):
        """Test setting SO_REUSEADDR on a transport."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set SO_REUSEADDR
            transport.set_reuse_address(True)

            writer.write(b'reuseaddr test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    @pytest.mark.skipif(
        not hasattr(socket, 'SO_REUSEPORT'),
        reason='SO_REUSEPORT not supported on this platform',
    )
    def test_reuse_port_option_on_server(self):
        """Test setting SO_REUSEPORT on server socket."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            # Try to set reuse port (may fail on some systems)
            try:
                server.set_reuse_port(True)
            except OSError:
                # This is acceptable - SO_REUSEPORT not available
                pass

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    @pytest.mark.skipif(
        not hasattr(socket, 'TCP_KEEPIDLE'),
        reason='TCP_KEEPIDLE not supported on this platform',
    )
    def test_keepalive_time_option(self):
        """Test setting TCP keep-alive time (TCP_KEEPIDLE)."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set keep-alive time (60 seconds)
            transport.set_keepalive_time(60)

            writer.write(b'keepalive time test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    @pytest.mark.skipif(
        not hasattr(socket, 'TCP_KEEPINTVL'),
        reason='TCP_KEEPINTVL not supported on this platform',
    )
    def test_keepalive_interval_option(self):
        """Test setting TCP keep-alive interval (TCP_KEEPINTVL)."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set keep-alive interval (10 seconds)
            transport.set_keepalive_interval(10)

            writer.write(b'keepalive interval test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    @pytest.mark.skipif(
        not hasattr(socket, 'TCP_KEEPCNT'),
        reason='TCP_KEEPCNT not supported on this platform',
    )
    def test_keepalive_count_option(self):
        """Test setting TCP keep-alive count (TCP_KEEPCNT)."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set keep-alive count (5 probes)
            transport.set_keepalive_count(5)

            writer.write(b'keepalive count test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    def test_server_reuse_address_option(self):
        """Test setting SO_REUSEADDR on server socket."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            # Set reuse address
            server.set_reuse_address(True)

            # Test that the server still works
            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            writer.write(b'test')
            await writer.drain()
            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    def test_multiple_options(self):
        """Test setting multiple socket options together."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            server = await loop.create_server(protocol_factory, '127.0.0.1', 0)

            addr = server.sockets[0].getsockname()

            reader, writer = await asyncio.open_connection(addr[0], addr[1])
            transport = writer.transport

            # Set multiple options
            transport.set_tcp_nodelay(True)
            transport.set_keepalive(True)
            transport.set_reuse_address(True)

            writer.write(b'multiple options test')
            await writer.drain()

            writer.close()
            await writer.wait_closed()

            server.close()
            await server.wait_closed()

            return True

        result = asyncio.run(run_test())
        assert result is True

    def test_socket_options_class(self):
        """Test the SocketOptions class directly."""
        from veloxloop._veloxloop import SocketOptions

        # Create instance
        opts = SocketOptions()

        # Set options
        opts.set_tcp_nodelay(True)
        opts.set_keepalive(True)
        opts.set_keepalive_time(60)
        opts.set_keepalive_interval(10)
        opts.set_keepalive_count(5)
        opts.set_reuse_address(True)
        opts.set_reuse_port(True)
        opts.set_recv_buffer_size(8192)
        opts.set_send_buffer_size(8192)

        # Get options
        assert opts.get_tcp_nodelay() == True
        assert opts.get_keepalive() == True
        assert opts.get_keepalive_time() == 60
        assert opts.get_keepalive_interval() == 10
        assert opts.get_keepalive_count() == 5
        assert opts.get_reuse_address() == True
        assert opts.get_reuse_port() == True
        assert opts.get_recv_buffer_size() == 8192
        assert opts.get_send_buffer_size() == 8192

        # Test repr
        repr_str = repr(opts)
        assert 'SocketOptions' in repr_str
        assert (
            'tcp_nodelay=Some(true)' in repr_str
            or 'tcp_nodelay=Some(True)' in repr_str.lower()
        )

    def test_socket_options_reset(self):
        """Test resetting SocketOptions."""
        from veloxloop._veloxloop import SocketOptions

        opts = SocketOptions()

        # Set options
        opts.set_tcp_nodelay(True)
        opts.set_keepalive(True)

        # Verify they're set
        assert opts.get_tcp_nodelay() == True
        assert opts.get_keepalive() == True

        # Reset
        opts.reset()

        # Verify they're unset
        assert opts.get_tcp_nodelay() is None
        assert opts.get_keepalive() is None

    def test_socket_options_partial_set(self):
        """Test SocketOptions with only some options set."""
        from veloxloop._veloxloop import SocketOptions

        opts = SocketOptions()

        # Set only one option
        opts.set_tcp_nodelay(True)

        # Verify
        assert opts.get_tcp_nodelay() == True
        assert opts.get_keepalive() is None
        assert opts.get_reuse_address() is None


class TestSocketOptionsWithContext:
    """Test socket options with async context manager."""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_options_with_server_context(self):
        """Test socket options work with server context manager."""

        async def run_test():
            loop = asyncio.get_event_loop()

            def protocol_factory():
                return SimpleProtocol()

            async with await loop.create_server(
                protocol_factory, '127.0.0.1', 0
            ) as server:
                # Set options on the server
                server.set_reuse_address(True)

                addr = server.sockets[0].getsockname()

                reader, writer = await asyncio.open_connection(addr[0], addr[1])
                transport = writer.transport

                # Set options on client transport
                transport.set_tcp_nodelay(True)
                transport.set_keepalive(True)

                writer.write(b'context test')
                await writer.drain()

                writer.close()
                await writer.wait_closed()

                return True

        result = asyncio.run(run_test())
        assert result is True
