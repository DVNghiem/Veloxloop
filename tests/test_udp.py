"""
Test UDP/Datagram transport implementation with VeloxLoop
"""

import asyncio
import veloxloop
import pytest


class EchoDatagramProtocol(asyncio.DatagramProtocol):
    """Simple echo protocol for UDP testing"""

    def __init__(self):
        self.transport = None
        self.received_data = []
        self.connection_made_called = False
        self.connection_lost_called = False
        self.errors = []

    def connection_made(self, transport):
        self.transport = transport
        self.connection_made_called = True

    def datagram_received(self, data, addr):
        self.received_data.append((data, addr))
        # Echo back
        self.transport.sendto(data, addr)

    def error_received(self, exc):
        self.errors.append(exc)

    def connection_lost(self, exc):
        self.connection_lost_called = True


class ClientDatagramProtocol(asyncio.DatagramProtocol):
    """Client protocol for UDP testing"""

    def __init__(self, message):
        self.message = message
        self.transport = None
        self.received = None
        self.done = False

    def connection_made(self, transport):
        self.transport = transport
        # Send message
        self.transport.sendto(self.message)

    def datagram_received(self, data, addr):
        self.received = (data, addr)
        self.done = True

    def error_received(self, exc):
        print(f'Error: {exc}')

    def connection_lost(self, exc):
        pass


class TestUDPTransport:
    """Test UDP/Datagram transport functionality"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_udp_echo_server_client(self):
        """Test basic UDP echo server and client"""

        async def main():
            loop = asyncio.get_event_loop()

            # Verify we're using VeloxLoop
            assert isinstance(loop, veloxloop.VeloxLoop), (
                f'Expected VeloxLoop, got {type(loop)}'
            )

            # Create server
            server_protocol = EchoDatagramProtocol()
            transport, protocol = await loop.create_datagram_endpoint(
                lambda: server_protocol, local_addr=('127.0.0.1', 0)
            )

            # Verify we're using VeloxLoop's UDP transport
            assert type(transport).__name__ == 'UdpTransport', (
                f'Expected UdpTransport, got {type(transport)}'
            )

            assert protocol is server_protocol
            assert server_protocol.connection_made_called

            # Get server address
            server_addr = transport.get_extra_info('sockname')
            assert server_addr is not None
            server_host, server_port = server_addr

            # Create client
            message = b'Hello, UDP!'
            client_protocol = ClientDatagramProtocol(message)
            client_transport, client_proto = await loop.create_datagram_endpoint(
                lambda: client_protocol, remote_addr=(server_host, server_port)
            )

            # Wait for response
            for _ in range(100):  # Try for 1 second
                await asyncio.sleep(0.01)
                if client_protocol.done:
                    break

            # Verify echo
            assert client_protocol.received is not None
            received_data, received_addr = client_protocol.received
            assert received_data == message

            # Cleanup
            client_transport.close()
            transport.close()

            await asyncio.sleep(0.1)

        asyncio.run(main())

    def test_udp_sendto_with_address(self):
        """Test sending to specific address without connecting"""

        async def main():
            loop = asyncio.get_event_loop()

            # Create server
            server_protocol = EchoDatagramProtocol()
            server_transport, _ = await loop.create_datagram_endpoint(
                lambda: server_protocol, local_addr=('127.0.0.1', 0)
            )

            server_addr = server_transport.get_extra_info('sockname')

            # Create unconnected client
            class UnconnectedProtocol(asyncio.DatagramProtocol):
                def __init__(self):
                    self.transport = None
                    self.received = None

                def connection_made(self, transport):
                    self.transport = transport

                def datagram_received(self, data, addr):
                    self.received = (data, addr)

            client_protocol = UnconnectedProtocol()
            client_transport, _ = await loop.create_datagram_endpoint(
                lambda: client_protocol, local_addr=('127.0.0.1', 0)
            )

            # Send using explicit address
            message = b'Test message'
            client_transport.sendto(message, server_addr)

            # Wait for echo
            await asyncio.sleep(0.2)

            assert client_protocol.received is not None
            assert client_protocol.received[0] == message

            # Cleanup
            client_transport.close()
            server_transport.close()

        asyncio.run(main())

    def test_udp_get_extra_info(self):
        """Test get_extra_info on UDP transport"""

        async def main():
            loop = asyncio.get_event_loop()

            protocol = EchoDatagramProtocol()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: protocol, local_addr=('127.0.0.1', 12345)
            )

            # Test sockname
            sockname = transport.get_extra_info('sockname')
            assert sockname is not None
            assert sockname[0] == '127.0.0.1'
            assert sockname[1] == 12345

            # Test socket
            sock = transport.get_extra_info('socket')
            assert sock is not None
            assert hasattr(sock, 'fileno')

            # Test default value
            result = transport.get_extra_info('unknown', 'default')
            assert result == 'default'

            transport.close()

        asyncio.run(main())

    def test_udp_connected_socket(self):
        """Test UDP socket connected to remote address"""

        async def main():
            loop = asyncio.get_event_loop()

            # Create server
            server_protocol = EchoDatagramProtocol()
            server_transport, _ = await loop.create_datagram_endpoint(
                lambda: server_protocol, local_addr=('127.0.0.1', 0)
            )

            server_addr = server_transport.get_extra_info('sockname')

            # Create connected client
            client_protocol = ClientDatagramProtocol(b'Connected test')
            client_transport, _ = await loop.create_datagram_endpoint(
                lambda: client_protocol, remote_addr=server_addr
            )

            # Verify peername
            peername = client_transport.get_extra_info('peername')
            assert peername is not None
            assert peername == server_addr

            # Wait for echo
            for _ in range(100):
                await asyncio.sleep(0.01)
                if client_protocol.done:
                    break

            assert client_protocol.received is not None
            assert client_protocol.received[0] == b'Connected test'

            # Cleanup
            client_transport.close()
            server_transport.close()

        asyncio.run(main())

    def test_udp_close_transport(self):
        """Test closing UDP transport"""

        async def main():
            loop = asyncio.get_event_loop()

            protocol = EchoDatagramProtocol()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: protocol, local_addr=('127.0.0.1', 0)
            )

            assert not transport.is_closing()

            transport.close()

            assert transport.is_closing()

            # Give time for connection_lost to be called
            await asyncio.sleep(0.1)

            assert protocol.connection_lost_called

        asyncio.run(main())

    def test_udp_abort_transport(self):
        """Test aborting UDP transport"""

        async def main():
            loop = asyncio.get_event_loop()

            protocol = EchoDatagramProtocol()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: protocol, local_addr=('127.0.0.1', 0)
            )

            transport.abort()

            assert transport.is_closing()

            await asyncio.sleep(0.1)
            assert protocol.connection_lost_called

        asyncio.run(main())

    def test_udp_multiple_messages(self):
        """Test sending and receiving multiple UDP messages"""

        async def main():
            loop = asyncio.get_event_loop()

            server_protocol = EchoDatagramProtocol()
            server_transport, _ = await loop.create_datagram_endpoint(
                lambda: server_protocol, local_addr=('127.0.0.1', 0)
            )

            server_addr = server_transport.get_extra_info('sockname')

            class MultiMessageProtocol(asyncio.DatagramProtocol):
                def __init__(self):
                    self.transport = None
                    self.received = []

                def connection_made(self, transport):
                    self.transport = transport

                def datagram_received(self, data, addr):
                    self.received.append(data)

            client_protocol = MultiMessageProtocol()
            client_transport, _ = await loop.create_datagram_endpoint(
                lambda: client_protocol, local_addr=('127.0.0.1', 0)
            )

            # Send multiple messages
            messages = [b'Message 1', b'Message 2', b'Message 3']
            for msg in messages:
                client_transport.sendto(msg, server_addr)

            # Wait for echoes
            await asyncio.sleep(0.3)

            # Verify all messages received
            assert len(client_protocol.received) == len(messages)
            assert set(client_protocol.received) == set(messages)

            # Cleanup
            client_transport.close()
            server_transport.close()

        asyncio.run(main())

    def test_udp_broadcast(self):
        """Test UDP broadcast capability"""

        async def main():
            loop = asyncio.get_event_loop()

            # Create socket with broadcast enabled
            protocol = EchoDatagramProtocol()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: protocol, local_addr=('0.0.0.0', 0), allow_broadcast=True
            )

            # Just verify it was created successfully with broadcast
            assert transport is not None

            transport.close()

        asyncio.run(main())

    def test_udp_large_packet(self):
        """Test sending and receiving large UDP packets"""

        async def main():
            loop = asyncio.get_event_loop()

            server_protocol = EchoDatagramProtocol()
            server_transport, _ = await loop.create_datagram_endpoint(
                lambda: server_protocol, local_addr=('127.0.0.1', 0)
            )

            server_addr = server_transport.get_extra_info('sockname')

            # Create large message (but under typical MTU)
            message = b'X' * 8192
            client_protocol = ClientDatagramProtocol(message)
            client_transport, _ = await loop.create_datagram_endpoint(
                lambda: client_protocol, remote_addr=server_addr
            )

            # Wait for echo
            for _ in range(100):
                await asyncio.sleep(0.01)
                if client_protocol.done:
                    break

            assert client_protocol.received is not None
            assert client_protocol.received[0] == message
            assert len(client_protocol.received[0]) == 8192

            client_transport.close()
            server_transport.close()

        asyncio.run(main())

    def test_udp_get_loop(self):
        """Test getting the event loop from UDP transport"""

        async def main():
            loop = asyncio.get_event_loop()

            # Verify we're using VeloxLoop
            assert isinstance(loop, veloxloop.VeloxLoop)

            protocol = EchoDatagramProtocol()
            transport, _ = await loop.create_datagram_endpoint(
                lambda: protocol, local_addr=('127.0.0.1', 0)
            )

            # With VeloxLoop, get_loop() should work
            transport_loop = transport.get_loop()
            assert transport_loop is loop

            transport.close()

        asyncio.run(main())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
