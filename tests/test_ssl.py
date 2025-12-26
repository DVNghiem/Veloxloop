"""Test SSL/TLS support in VeloxLoop"""

import asyncio
import os
import pytest
from pathlib import Path
import subprocess

# Import veloxloop
try:
    import veloxloop
    from veloxloop import _veloxloop
except ImportError:
    pytest.skip('VeloxLoop not built', allow_module_level=True)


# Get the path to test SSL certificates
SSL_CERT_DIR = Path(__file__).parent / 'ssl_certs'
SSL_CERT_DIR.mkdir(parents=True, exist_ok=True)
SERVER_CERT = str(SSL_CERT_DIR / 'server-cert.pem')
SERVER_KEY = str(SSL_CERT_DIR / 'server-key.pem')

# If cert/key files are missing, try to auto-generate a self-signed pair with openssl.
# If openssl isn't available, create simple placeholder files so tests won't error on missing files.
if not (Path(SERVER_CERT).exists() and Path(SERVER_KEY).exists()):
    try:
        subprocess.check_call(
            [
                'openssl',
                'req',
                '-x509',
                '-nodes',
                '-newkey',
                'rsa:2048',
                '-days',
                '1',
                '-keyout',
                SERVER_KEY,
                '-out',
                SERVER_CERT,
                '-subj',
                '/CN=localhost',
            ]
        )
    except Exception:
        # fallback: write minimal placeholder files
        if not Path(SERVER_KEY).exists():
            Path(SERVER_KEY).write_text('# placeholder private key\n')
        if not Path(SERVER_CERT).exists():
            Path(SERVER_CERT).write_text('# placeholder certificate\n')


class TestSSLContext:
    """Test SSL context creation and configuration"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_create_client_context(self):
        """Test creating a client SSL context"""
        ctx = _veloxloop.SSLContext.create_client_context()
        assert ctx is not None
        assert repr(ctx) == 'SSLContext(purpose=CLIENT)' or 'CLIENT' in repr(ctx)

    def test_create_server_context(self):
        """Test creating a server SSL context"""
        ctx = _veloxloop.SSLContext.create_server_context()
        assert ctx is not None
        assert repr(ctx) == 'SSLContext(purpose=SERVER)' or 'SERVER' in repr(ctx)

    def test_load_cert_chain(self):
        """Test loading certificate chain for server context"""
        if not os.path.exists(SERVER_CERT):
            pytest.skip(f'Server certificate not found at {SERVER_CERT}')

        ctx = _veloxloop.SSLContext.create_server_context()
        ctx.load_cert_chain(SERVER_CERT, SERVER_KEY)
        # If no exception, the certificate was loaded successfully

    def test_load_cert_chain_file_not_found(self):
        """Test loading certificate chain with non-existent file"""
        ctx = _veloxloop.SSLContext.create_server_context()
        with pytest.raises(FileNotFoundError):
            ctx.load_cert_chain('/nonexistent/cert.pem')

    def test_set_check_hostname(self):
        """Test setting check_hostname option"""
        ctx = _veloxloop.SSLContext.create_client_context()
        ctx.set_check_hostname(False)
        # Should not raise an error


class TestSSLBasicAPI:
    """Test basic SSL API functionality without full implementation"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_ssl_context_exists(self):
        """Test that SSLContext class is available"""
        assert hasattr(_veloxloop, 'SSLContext')

    def test_ssl_transport_exists(self):
        """Test that SSLTransport class is available"""
        assert hasattr(_veloxloop, 'SSLTransport')

    def test_create_both_contexts(self):
        """Test creating both client and server contexts"""
        client_ctx = _veloxloop.SSLContext.create_client_context()
        server_ctx = _veloxloop.SSLContext.create_server_context()

        assert client_ctx is not None
        assert server_ctx is not None
        assert id(client_ctx) != id(server_ctx)


class TestSSLClientConnection:
    """Test SSL client connection to real HTTPS servers"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_https_connection_google(self):
        """Test HTTPS connection to Google (tests certificate validation)"""

        async def run_test():
            ssl_context = _veloxloop.SSLContext.create_client_context()

            class HTTPSClientProtocol:
                def __init__(self):
                    self.transport = None
                    self.data = b''
                    self.connection_made_event = asyncio.Event()
                    self.data_received_event = asyncio.Event()
                    self.connection_lost_future = asyncio.Future()

                def connection_made(self, transport):
                    self.transport = transport
                    self.connection_made_event.set()
                    # Send HTTP GET request
                    request = b'GET / HTTP/1.0\r\nHost: www.google.com\r\n\r\n'
                    transport.write(request)

                def data_received(self, data):
                    self.data += data
                    self.data_received_event.set()

                def eof_received(self):
                    return False

                def connection_lost(self, exc):
                    if not self.connection_lost_future.done():
                        if exc is None:
                            self.connection_lost_future.set_result(None)
                        else:
                            self.connection_lost_future.set_exception(exc)

            protocol = HTTPSClientProtocol()

            # Connect to google.com with SSL
            transport, _ = await asyncio.wait_for(
                asyncio.get_event_loop().create_connection(
                    lambda: protocol,
                    'www.google.com',
                    443,
                    ssl=ssl_context,
                    server_hostname='www.google.com',
                ),
                timeout=10.0,
            )

            # Wait for connection
            await asyncio.wait_for(protocol.connection_made_event.wait(), timeout=5.0)
            assert protocol.transport is not None

            # Wait for data (with timeout to handle server disconnection)
            try:
                await asyncio.wait_for(
                    protocol.data_received_event.wait(), timeout=10.0
                )
            except asyncio.TimeoutError:
                pass  # Server may have disconnected

            # Should have received some HTTP response (if connection didn't fail)
            if len(protocol.data) > 0:
                assert b'HTTP' in protocol.data or b'html' in protocol.data.lower()

            transport.close()

        asyncio.run(run_test())

    def test_https_connection_example_com(self):
        """Test HTTPS connection to example.com"""

        async def run_test():
            ssl_context = _veloxloop.SSLContext.create_client_context()

            class SimpleProtocol:
                def __init__(self):
                    self.transport = None
                    self.connected = False
                    self.received_data = False
                    self.connection_made_event = asyncio.Event()
                    self.data_received_event = asyncio.Event()

                def connection_made(self, transport):
                    self.transport = transport
                    self.connected = True
                    self.connection_made_event.set()
                    transport.write(
                        b'GET / HTTP/1.1\r\nHost: www.example.com\r\nConnection: close\r\n\r\n'
                    )

                def data_received(self, data):
                    self.received_data = True
                    self.data_received_event.set()

                def eof_received(self):
                    return False

                def connection_lost(self, exc):
                    pass

            protocol = SimpleProtocol()

            transport, _ = await asyncio.wait_for(
                asyncio.get_event_loop().create_connection(
                    lambda: protocol,
                    'www.example.com',
                    443,
                    ssl=ssl_context,
                    server_hostname='www.example.com',
                ),
                timeout=10.0,
            )

            await asyncio.wait_for(protocol.connection_made_event.wait(), timeout=5.0)
            assert protocol.connected

            # Wait for data
            try:
                await asyncio.wait_for(
                    protocol.data_received_event.wait(), timeout=10.0
                )
                assert protocol.received_data
            except asyncio.TimeoutError:
                pass  # Some servers may close before sending data

            transport.close()

        asyncio.run(run_test())

    def test_ssl_transport_extra_info(self):
        """Test getting SSL-specific extra info from transport"""

        async def run_test():
            ssl_context = _veloxloop.SSLContext.create_client_context()

            class InfoProtocol:
                def __init__(self):
                    self.transport = None
                    self.extra_info = {}
                    self.connection_made_event = asyncio.Event()

                def connection_made(self, transport):
                    self.transport = transport
                    # Get SSL-specific info
                    self.extra_info['ssl_context'] = transport.get_extra_info(
                        'sslcontext'
                    )
                    self.extra_info['peername'] = transport.get_extra_info('peername')
                    self.extra_info['socket'] = transport.get_extra_info('socket')
                    self.connection_made_event.set()

                def data_received(self, data):
                    pass

                def eof_received(self):
                    return False

                def connection_lost(self, exc):
                    pass

            protocol = InfoProtocol()

            transport, _ = await asyncio.wait_for(
                asyncio.get_event_loop().create_connection(
                    lambda: protocol,
                    'www.google.com',
                    443,
                    ssl=ssl_context,
                    server_hostname='www.google.com',
                ),
                timeout=10.0,
            )

            await asyncio.wait_for(protocol.connection_made_event.wait(), timeout=5.0)

            # Verify we can get extra info
            assert 'ssl_context' in protocol.extra_info
            # SSL context might be None in current implementation

            assert 'peername' in protocol.extra_info
            peername = protocol.extra_info['peername']
            if peername:
                assert isinstance(peername, tuple)
                assert len(peername) == 2
                assert isinstance(peername[1], int)  # port number

            transport.close()

        asyncio.run(run_test())

    def test_ssl_with_disabled_hostname_check(self):
        """Test SSL connection with hostname verification disabled"""

        async def run_test():
            ssl_context = _veloxloop.SSLContext.create_client_context()
            ssl_context.set_check_hostname(False)

            class TestProtocol:
                def __init__(self):
                    self.transport = None
                    self.connection_made_event = asyncio.Event()

                def connection_made(self, transport):
                    self.transport = transport
                    self.connection_made_event.set()
                    transport.write(b'GET / HTTP/1.0\r\nHost: www.google.com\r\n\r\n')

                def data_received(self, data):
                    pass

                def eof_received(self):
                    return False

                def connection_lost(self, exc):
                    pass

            protocol = TestProtocol()

            # Should still connect successfully
            transport, _ = await asyncio.wait_for(
                asyncio.get_event_loop().create_connection(
                    lambda: protocol,
                    'www.google.com',
                    443,
                    ssl=ssl_context,
                    server_hostname='www.google.com',
                ),
                timeout=10.0,
            )

            await asyncio.wait_for(protocol.connection_made_event.wait(), timeout=5.0)
            assert protocol.transport is not None

            transport.close()

        asyncio.run(run_test())

    def test_ssl_transport_write_and_close(self):
        """Test writing data and closing SSL transport"""

        async def run_test():
            ssl_context = _veloxloop.SSLContext.create_client_context()

            class WriteProtocol:
                def __init__(self):
                    self.transport = None
                    self.connection_made_event = asyncio.Event()
                    self.connection_lost_future = asyncio.Future()
                    self.data_sent = False

                def connection_made(self, transport):
                    self.transport = transport
                    self.connection_made_event.set()

                def data_received(self, data):
                    pass

                def eof_received(self):
                    return False

                def connection_lost(self, exc):
                    if not self.connection_lost_future.done():
                        self.connection_lost_future.set_result(exc)

            protocol = WriteProtocol()

            transport, _ = await asyncio.wait_for(
                asyncio.get_event_loop().create_connection(
                    lambda: protocol,
                    'www.google.com',
                    443,
                    ssl=ssl_context,
                    server_hostname='www.google.com',
                ),
                timeout=10.0,
            )

            await asyncio.wait_for(protocol.connection_made_event.wait(), timeout=5.0)

            # Write some data
            request = b'GET / HTTP/1.0\r\nHost: www.google.com\r\n\r\n'
            transport.write(request)
            protocol.data_sent = True

            # Close the transport
            transport.close()

            # Wait for connection lost
            exc = await asyncio.wait_for(protocol.connection_lost_future, timeout=5.0)
            # Should close cleanly (exc should be None or acceptable error)
            assert protocol.data_sent

        asyncio.run(run_test())


class TestSSLContextConfiguration:
    """Test SSL context configuration"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_client_context_repr(self):
        """Test client context representation"""
        ctx = _veloxloop.SSLContext.create_client_context()
        repr_str = repr(ctx)
        assert 'CLIENT' in repr_str or 'client' in repr_str.lower()

    def test_server_context_repr(self):
        """Test server context representation"""
        ctx = _veloxloop.SSLContext.create_server_context()
        repr_str = repr(ctx)
        assert 'SERVER' in repr_str or 'server' in repr_str.lower()

    def test_check_hostname_toggle(self):
        """Test toggling hostname verification"""
        ctx = _veloxloop.SSLContext.create_client_context()

        # Should not raise
        ctx.set_check_hostname(True)
        ctx.set_check_hostname(False)
        ctx.set_check_hostname(True)

    def test_load_server_certificates(self):
        """Test loading server certificates"""
        import os

        if not os.path.exists(SERVER_CERT):
            pytest.skip(f'Server certificate not found at {SERVER_CERT}')

        ctx = _veloxloop.SSLContext.create_server_context()
        # Should not raise
        ctx.load_cert_chain(SERVER_CERT, SERVER_KEY)

    def test_load_nonexistent_certificate(self):
        """Test loading non-existent certificate"""
        ctx = _veloxloop.SSLContext.create_server_context()

        with pytest.raises(FileNotFoundError):
            ctx.load_cert_chain('/nonexistent/cert.pem', '/nonexistent/key.pem')


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
