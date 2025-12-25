pub mod future;
pub mod ssl;
pub mod stream_server;
pub mod tcp;
pub mod udp;

use bitflags::bitflags;
use pyo3::prelude::*;
use std::os::fd::RawFd;

use crate::event_loop::VeloxLoop;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TransportState: u32 {
        const ACTIVE         = 1 << 0;
        const CLOSING        = 1 << 1;
        const CLOSED         = 1 << 2;
        const READING_PAUSED = 1 << 3;
        const WRITING_PAUSED = 1 << 4;
        const EOF_RECEIVED   = 1 << 5;
    }
}

/// Base trait for all transports
/// Provides common functionality shared by both stream and datagram transports
pub trait Transport {
    /// Get extra information about the transport
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>>;

    /// Check if the transport is closing or closed
    fn is_closing(&self) -> bool;

    /// Get the file descriptor associated with this transport
    fn get_fd(&self) -> RawFd;
}

/// Trait for stream-based transports (TCP, SSL)
#[allow(dead_code)]
pub trait StreamTransport: Transport {
    /// Close the transport gracefully
    fn close(&mut self, py: Python<'_>) -> PyResult<()>;

    /// Force close the transport immediately
    fn force_close(&mut self, py: Python<'_>) -> PyResult<()>;

    /// Write data to the transport
    fn write(&mut self, py: Python<'_>, data: Bound<'_, PyAny>) -> PyResult<()>;

    /// Zero-copy read into a Python buffer
    fn recv_into(&mut self, py: Python<'_>, buffer: Bound<'_, PyAny>) -> PyResult<usize>;

    /// Write end-of-file marker
    fn write_eof(&mut self) -> PyResult<()>;

    /// Get the size of the write buffer
    fn get_write_buffer_size(&self) -> usize;

    /// Set write buffer limits (high and low water marks)
    fn set_write_buffer_limits(
        &mut self,
        py: Python<'_>,
        high: Option<usize>,
        low: Option<usize>,
    ) -> PyResult<()>;

    /// Internal callback called when the socket is readable
    fn read_ready(&mut self, py: Python<'_>) -> PyResult<()>;

    /// Internal callback called when the socket is writable
    fn write_ready(&mut self, py: Python<'_>) -> PyResult<()>;
}

/// Factory trait for creating different types of transports
pub trait TransportFactory {
    /// Create a TCP transport
    fn create_tcp(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        stream: std::net::TcpStream,
        protocol: Py<PyAny>,
    ) -> PyResult<Py<PyAny>>;

    /// Create an SSL transport
    fn create_ssl(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        stream: std::net::TcpStream,
        protocol: Py<PyAny>,
        ssl_context: Py<PyAny>,
        server_hostname: Option<String>,
        is_client: bool,
    ) -> PyResult<Py<PyAny>>;

    /// Create a UDP transport
    fn create_udp(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        socket: std::net::UdpSocket,
        protocol: Py<PyAny>,
        remote_addr: Option<std::net::SocketAddr>,
        allow_broadcast: bool,
    ) -> PyResult<Py<PyAny>>;
}

/// Default implementation of the transport factory
pub struct DefaultTransportFactory;

impl TransportFactory for DefaultTransportFactory {
    fn create_tcp(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        stream: std::net::TcpStream,
        protocol: Py<PyAny>,
    ) -> PyResult<Py<PyAny>> {
        // Downcast loop_ from PyAny to VeloxLoop
        let velox_loop: Py<VeloxLoop> = loop_.extract(py)?;
        let transport = tcp::TcpTransport::new(velox_loop, stream, protocol)?;
        Ok(Py::new(py, transport)?.into_any())
    }

    fn create_ssl(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        stream: std::net::TcpStream,
        protocol: Py<PyAny>,
        ssl_context: Py<PyAny>,
        server_hostname: Option<String>,
        is_client: bool,
    ) -> PyResult<Py<PyAny>> {
        // Downcast loop_ from PyAny to VeloxLoop
        let velox_loop: Py<VeloxLoop> = loop_.extract(py)?;
        // Downcast ssl_context from PyAny to SSLContext
        let ssl_ctx: Py<ssl::SSLContext> = ssl_context.extract(py)?;

        let transport = if is_client {
            ssl::SSLTransport::new_client(
                velox_loop,
                stream,
                protocol,
                ssl_ctx,
                server_hostname,
                py,
            )?
        } else {
            ssl::SSLTransport::new_server(velox_loop, stream, protocol, ssl_ctx, py)?
        };
        Ok(Py::new(py, transport)?.into_any())
    }

    fn create_udp(
        &self,
        py: Python<'_>,
        loop_: Py<PyAny>,
        socket: std::net::UdpSocket,
        protocol: Py<PyAny>,
        remote_addr: Option<std::net::SocketAddr>,
        _allow_broadcast: bool,
    ) -> PyResult<Py<PyAny>> {
        // Downcast loop_ from PyAny to VeloxLoop
        let velox_loop: Py<VeloxLoop> = loop_.extract(py)?;
        let transport = udp::UdpTransport::new(velox_loop, socket, protocol, remote_addr)?;
        Ok(Py::new(py, transport)?.into_any())
    }
}
