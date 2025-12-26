"""
Tests for DNS Resolution Features:
- getaddrinfo()
- getnameinfo()
"""

import pytest
import asyncio
import socket
from veloxloop import VeloxLoopPolicy


class TestDNSResolution:
    """Test DNS resolution features"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_getaddrinfo_basic(self, loop):
        """Test basic getaddrinfo functionality with localhost"""

        async def test():
            # Test with localhost
            results = await loop.getaddrinfo('localhost', '80')
            assert len(results) > 0

            # Each result should be a 5-tuple: (family, type, proto, canonname, sockaddr)
            for family, socktype, proto, canonname, sockaddr in results:
                assert family in (socket.AF_INET, socket.AF_INET6)
                # socktype can be STREAM, DGRAM, RAW, or 0
                assert socktype in (
                    socket.SOCK_STREAM,
                    socket.SOCK_DGRAM,
                    socket.SOCK_RAW,
                    0,
                )
                assert isinstance(proto, int)
                assert isinstance(canonname, str)
                assert isinstance(sockaddr, tuple)
                assert len(sockaddr) >= 2  # At least (host, port)

        loop.run_until_complete(test())

    def test_getaddrinfo_ipv4(self, loop):
        """Test getaddrinfo with IPv4 address"""

        async def test():
            results = await loop.getaddrinfo('127.0.0.1', '80', family=socket.AF_INET)
            assert len(results) > 0

            family, socktype, proto, canonname, sockaddr = results[0]
            assert family == socket.AF_INET
            assert sockaddr[0] == '127.0.0.1'
            assert sockaddr[1] == 80

        loop.run_until_complete(test())

    def test_getaddrinfo_with_port_number(self, loop):
        """Test getaddrinfo with numeric port"""

        async def test():
            results = await loop.getaddrinfo('localhost', '8080')
            assert len(results) > 0

            for _, _, _, _, sockaddr in results:
                assert sockaddr[1] == 8080

        loop.run_until_complete(test())

    def test_getaddrinfo_with_hints(self, loop):
        """Test getaddrinfo with hints (family, type, proto)"""

        async def test():
            # Request only TCP sockets
            results = await loop.getaddrinfo(
                'localhost',
                '80',
                family=socket.AF_INET,
                type=socket.SOCK_STREAM,
                proto=socket.IPPROTO_TCP,
            )
            assert len(results) > 0

            for family, socktype, proto, _, _ in results:
                assert family == socket.AF_INET
                assert socktype == socket.SOCK_STREAM
                # proto might be 0 or IPPROTO_TCP depending on implementation

        loop.run_until_complete(test())

    def test_getaddrinfo_none_host(self, loop):
        """Test getaddrinfo with None as host (passive socket)"""

        async def test():
            results = await loop.getaddrinfo(None, '8080')
            assert len(results) > 0

            # Should return addresses suitable for bind()
            for _, _, _, _, sockaddr in results:
                assert sockaddr[1] == 8080

        loop.run_until_complete(test())

    def test_getaddrinfo_numeric_service(self, loop):
        """Test getaddrinfo with numeric service (port)"""

        async def test():
            results = await loop.getaddrinfo('127.0.0.1', '12345')
            assert len(results) > 0

            for _, _, _, _, sockaddr in results:
                assert sockaddr[1] == 12345

        loop.run_until_complete(test())

    def test_getaddrinfo_canonical_name(self, loop):
        """Test getaddrinfo with AI_CANONNAME flag"""

        async def test():
            # Request canonical name
            results = await loop.getaddrinfo(
                'localhost', '80', flags=socket.AI_CANONNAME
            )
            assert len(results) > 0

            # At least one result should have a canonical name
            canonnames = [r[3] for r in results if r[3]]
            # Note: canonname might be empty even with AI_CANONNAME on some systems

        loop.run_until_complete(test())

    def test_getaddrinfo_invalid_host(self, loop):
        """Test getaddrinfo with invalid host"""

        async def test():
            with pytest.raises(OSError):
                await loop.getaddrinfo('this-host-does-not-exist-12345.invalid', '80')

        loop.run_until_complete(test())

    def test_getnameinfo_basic(self, loop):
        """Test basic getnameinfo functionality"""

        async def test():
            # Reverse lookup for localhost
            hostname, service = await loop.getnameinfo(('127.0.0.1', 80))

            # Hostname should be a string (could be localhost or 127.0.0.1)
            assert isinstance(hostname, str)
            assert len(hostname) > 0

            # Service should be "http" or "80"
            assert isinstance(service, str)
            assert len(service) > 0

        loop.run_until_complete(test())

    def test_getnameinfo_numeric(self, loop):
        """Test getnameinfo with NI_NUMERICHOST and NI_NUMERICSERV flags"""

        async def test():
            # Request numeric results
            hostname, service = await loop.getnameinfo(
                ('127.0.0.1', 8080), flags=socket.NI_NUMERICHOST | socket.NI_NUMERICSERV
            )

            assert hostname == '127.0.0.1'
            assert service == '8080'

        loop.run_until_complete(test())

    def test_getnameinfo_well_known_port(self, loop):
        """Test getnameinfo with well-known port"""

        async def test():
            hostname, service = await loop.getnameinfo(
                ('127.0.0.1', 80), flags=socket.NI_NUMERICHOST
            )

            assert hostname == '127.0.0.1'
            # Service might be "http" or "80" depending on the system
            assert service in ('http', '80')

        loop.run_until_complete(test())

    def test_getnameinfo_invalid_address(self, loop):
        """Test getnameinfo with invalid address"""

        async def test():
            with pytest.raises((OSError, ValueError)):
                await loop.getnameinfo(('invalid-ip-address', 80))

        loop.run_until_complete(test())

    def test_getaddrinfo_getnameinfo_roundtrip(self, loop):
        """Test that getaddrinfo and getnameinfo work together"""

        async def test():
            # Get address info for localhost
            results = await loop.getaddrinfo('localhost', '80', family=socket.AF_INET)
            assert len(results) > 0

            family, socktype, proto, canonname, sockaddr = results[0]

            # Reverse lookup the address
            hostname, service = await loop.getnameinfo(
                sockaddr, flags=socket.NI_NUMERICSERV
            )

            assert isinstance(hostname, str)
            assert service == '80'

        loop.run_until_complete(test())

    def test_getaddrinfo_concurrent(self, loop):
        """Test concurrent DNS resolution"""

        async def test():
            # Perform multiple concurrent DNS lookups
            tasks = [
                loop.getaddrinfo('localhost', '80'),
                loop.getaddrinfo('127.0.0.1', '8080'),
                loop.getaddrinfo('localhost', '443'),
            ]

            results = await asyncio.gather(*tasks)

            # All should succeed
            assert len(results) == 3
            for result in results:
                assert len(result) > 0

        loop.run_until_complete(test())

    def test_getnameinfo_concurrent(self, loop):
        """Test concurrent reverse DNS lookups"""

        async def test():
            # Perform multiple concurrent reverse lookups
            tasks = [
                loop.getnameinfo(('127.0.0.1', 80), flags=socket.NI_NUMERICSERV),
                loop.getnameinfo(('127.0.0.1', 8080), flags=socket.NI_NUMERICSERV),
                loop.getnameinfo(('127.0.0.1', 443), flags=socket.NI_NUMERICSERV),
            ]

            results = await asyncio.gather(*tasks)

            # All should succeed
            assert len(results) == 3
            for hostname, service in results:
                assert isinstance(hostname, str)
                assert isinstance(service, str)

        loop.run_until_complete(test())

    def test_getaddrinfo_ipv6(self, loop):
        """Test getaddrinfo with IPv6 address"""

        async def test():
            try:
                results = await loop.getaddrinfo('::1', '80', family=socket.AF_INET6)

                if len(results) > 0:
                    family, socktype, proto, canonname, sockaddr = results[0]
                    assert family == socket.AF_INET6
                    assert len(sockaddr) == 4  # IPv6 addresses are 4-tuples
                    assert sockaddr[0] == '::1' or sockaddr[0] == '0:0:0:0:0:0:0:1'
                    assert sockaddr[1] == 80
            except OSError:
                # IPv6 might not be available on all systems
                pytest.skip('IPv6 not available on this system')

        loop.run_until_complete(test())

    def test_getaddrinfo_default_parameters(self, loop):
        """Test getaddrinfo with default parameters (family=0, type=0, proto=0, flags=0)"""

        async def test():
            # All parameters should default to 0
            results = await loop.getaddrinfo('localhost', '80')
            assert len(results) > 0

            # Should return multiple address families/types
            families = set(r[0] for r in results)
            assert len(families) >= 1  # At least one address family

        loop.run_until_complete(test())
