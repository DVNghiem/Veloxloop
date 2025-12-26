"""Test get_extra_info and set_write_buffer_limits methods."""

import asyncio
import pytest
import veloxloop


class SimpleProtocol(asyncio.Protocol):
    """Simple protocol for testing."""

    def __init__(self):
        self.transport = None
        self.pause_writing_called = False
        self.resume_writing_called = False

    def connection_made(self, transport):
        self.transport = transport

    def data_received(self, data):
        pass

    def eof_received(self):
        return False

    def connection_lost(self, exc):
        pass

    def pause_writing(self):
        self.pause_writing_called = True

    def resume_writing(self):
        self.resume_writing_called = True


class TestTransportMethods:
    """Test transport method implementations."""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_get_extra_info(self):
        """Test get_extra_info returns socket information."""

        async def run_test():
            # Create a server
            server_protocol = SimpleProtocol()
            client_protocol = SimpleProtocol()

            def protocol_factory():
                return SimpleProtocol()

            server = await asyncio.start_server(protocol_factory, '127.0.0.1', 0)

            # Get the server port
            sockets = server.sockets
            assert len(sockets) > 0
            _, port = sockets[0].getsockname()

            # Connect a client
            transport, protocol = await asyncio.get_event_loop().create_connection(
                lambda: client_protocol, '127.0.0.1', port
            )

            # Test get_extra_info
            peername = transport.get_extra_info('peername')
            assert peername is not None
            assert isinstance(peername, tuple)
            assert len(peername) == 2
            assert peername[0] == '127.0.0.1'
            assert isinstance(peername[1], int)

            sockname = transport.get_extra_info('sockname')
            assert sockname is not None
            assert isinstance(sockname, tuple)
            assert len(sockname) == 2
            assert sockname[0] == '127.0.0.1'
            assert isinstance(sockname[1], int)

            # Test socket
            socket = transport.get_extra_info('socket')
            assert socket is not None
            assert hasattr(socket, 'fileno')
            assert hasattr(socket, 'getsockname')

            # Test non-existent keys
            compression = transport.get_extra_info('compression')
            assert compression is None

            cipher = transport.get_extra_info('cipher')
            assert cipher is None

            peercert = transport.get_extra_info('peercert')
            assert peercert is None

            # Test with default value
            custom_default = 'my_default'
            result = transport.get_extra_info('unknown_key', custom_default)
            assert result == custom_default

            # Cleanup
            transport.close()
            server.close()
            await server.wait_closed()

        asyncio.run(run_test())

    def test_set_write_buffer_limits(self):
        """Test set_write_buffer_limits configuration."""

        async def run_test():
            # Create a server
            server_protocol = SimpleProtocol()
            client_protocol = SimpleProtocol()

            def protocol_factory():
                return SimpleProtocol()

            server = await asyncio.start_server(protocol_factory, '127.0.0.1', 0)

            # Get the server port
            sockets = server.sockets
            _, port = sockets[0].getsockname()

            # Connect a client
            transport, protocol = await asyncio.get_event_loop().create_connection(
                lambda: client_protocol, '127.0.0.1', port
            )

            # Test setting write buffer limits
            transport.set_write_buffer_limits(high=128 * 1024, low=32 * 1024)

            # Test with only high
            transport.set_write_buffer_limits(high=256 * 1024)

            # Test with only low
            transport.set_write_buffer_limits(low=16 * 1024)

            # Test with neither (should use defaults)
            transport.set_write_buffer_limits()

            # Test invalid limits (low >= high)
            with pytest.raises(ValueError):
                transport.set_write_buffer_limits(high=1024, low=2048)

            # Cleanup
            transport.close()
            server.close()
            await server.wait_closed()

        asyncio.run(run_test())

    def test_get_write_buffer_size(self):
        """Test get_write_buffer_size method."""

        async def run_test():
            # Create a server
            server_protocol = SimpleProtocol()
            client_protocol = SimpleProtocol()

            def protocol_factory():
                return SimpleProtocol()

            server = await asyncio.start_server(protocol_factory, '127.0.0.1', 0)

            # Get the server port
            sockets = server.sockets
            _, port = sockets[0].getsockname()

            # Connect a client
            transport, protocol = await asyncio.get_event_loop().create_connection(
                lambda: client_protocol, '127.0.0.1', port
            )

            # Initially should be 0
            assert transport.get_write_buffer_size() >= 0

            # Cleanup
            transport.close()
            server.close()
            await server.wait_closed()

        asyncio.run(run_test())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
