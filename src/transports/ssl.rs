use parking_lot::Mutex;
use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::TcpStream;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use crate::constants::{DEFAULT_HIGH, DEFAULT_LOW};
use crate::event_loop::VeloxLoop;
use crate::transports::{StreamTransport, Transport, TransportState};
use crate::utils::VeloxResult;
use bytes::BytesMut;

/// SSL/TLS Context for configuring secure connections
#[pyclass(module = "veloxloop._veloxloop")]
#[derive(Clone)]
pub struct SSLContext {
    client_config: Option<Arc<ClientConfig>>,
    server_config: Option<Arc<ServerConfig>>,
    purpose: SSLPurpose,
    check_hostname: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum SSLPurpose {
    ClientAuth,
    ServerAuth,
}

#[pymethods]
impl SSLContext {
    /// Create a new SSL context for client connections
    #[staticmethod]
    fn create_client_context(py: Python<'_>) -> PyResult<Py<SSLContext>> {
        let mut root_store = RootCertStore::empty();

        // Load system root certificates
        let native_certs = rustls_native_certs::load_native_certs();
        for cert in native_certs.certs {
            root_store.add(cert).ok();
        }

        // If no native certs loaded, use webpki-roots as fallback
        if root_store.is_empty() {
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let ctx = SSLContext {
            client_config: Some(Arc::new(config)),
            server_config: None,
            purpose: SSLPurpose::ServerAuth,
            check_hostname: true,
        };

        Py::new(py, ctx)
    }

    /// Create a new SSL context for server connections
    #[staticmethod]
    fn create_server_context(py: Python<'_>) -> PyResult<Py<SSLContext>> {
        let ctx = SSLContext {
            client_config: None,
            server_config: None, // Will be configured with load_cert_chain
            purpose: SSLPurpose::ClientAuth,
            check_hostname: false,
        };

        Py::new(py, ctx)
    }

    /// Load certificate chain and private key for server context
    #[pyo3(signature = (certfile, keyfile=None))]
    fn load_cert_chain(&mut self, certfile: String, keyfile: Option<String>) -> PyResult<()> {
        let keyfile = keyfile.unwrap_or_else(|| certfile.clone());

        // Load certificates
        let cert_file = File::open(&certfile).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                "Certificate file not found: {}",
                e
            ))
        })?;
        let mut cert_reader = BufReader::new(cert_file);
        let cert_chain: Vec<CertificateDer> =
            certs(&mut cert_reader).filter_map(Result::ok).collect();

        if cert_chain.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "No certificates found in certfile",
            ));
        }

        // Load private key
        let key_file = File::open(&keyfile).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                "Key file not found: {}",
                e
            ))
        })?;
        let mut key_reader = BufReader::new(key_file);

        // Try PKCS8 first, then RSA
        let private_key_der = {
            let pkcs8_keys = pkcs8_private_keys(&mut key_reader)
                .filter_map(Result::ok)
                .collect::<Vec<_>>();

            if !pkcs8_keys.is_empty() {
                PrivateKeyDer::Pkcs8(pkcs8_keys.into_iter().next().unwrap())
            } else {
                // Reset reader and try RSA format
                let key_file = File::open(&keyfile)?;
                let mut key_reader = BufReader::new(key_file);
                let rsa_keys = rsa_private_keys(&mut key_reader)
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();

                if !rsa_keys.is_empty() {
                    PrivateKeyDer::Pkcs1(rsa_keys.into_iter().next().unwrap())
                } else {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "No private key found in keyfile",
                    ));
                }
            }
        };

        // Build server config
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key_der)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Failed to configure TLS: {}",
                    e
                ))
            })?;

        self.server_config = Some(Arc::new(config));
        Ok(())
    }

    /// Set whether to check hostname (client contexts only)
    fn set_check_hostname(&mut self, check: bool) {
        self.check_hostname = check;
    }

    /// Load CA certificates for verification
    #[pyo3(signature = (cafile=None, capath=None))]
    fn load_verify_locations(
        &mut self,
        cafile: Option<String>,
        capath: Option<String>,
    ) -> PyResult<()> {
        if let Some(cafile_path) = cafile {
            let mut root_store = RootCertStore::empty();

            let ca_file = File::open(&cafile_path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                    "CA file not found: {}",
                    e
                ))
            })?;
            let mut ca_reader = BufReader::new(ca_file);
            let ca_certs: Vec<CertificateDer> =
                certs(&mut ca_reader).filter_map(Result::ok).collect();

            for cert in ca_certs {
                root_store.add(cert).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                        "Failed to add CA certificate: {}",
                        e
                    ))
                })?;
            }

            // Rebuild client config with custom root store
            let config = ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();

            self.client_config = Some(Arc::new(config));
        }

        if capath.is_some() {
            // Directory-based CA loading not implemented yet
            return Err(PyErr::new::<pyo3::exceptions::PyNotImplementedError, _>(
                "Loading CA certificates from directory is not yet supported",
            ));
        }

        Ok(())
    }

    /// Get the purpose of this context
    fn __repr__(&self) -> String {
        match self.purpose {
            SSLPurpose::ServerAuth => "SSLContext(purpose=CLIENT)".to_string(),
            SSLPurpose::ClientAuth => "SSLContext(purpose=SERVER)".to_string(),
        }
    }
}

/// TLS-wrapped transport
#[pyclass(module = "veloxloop._veloxloop")]
pub struct SSLTransport {
    fd: RawFd,
    tls_state: Mutex<TlsState>,
    protocol: Py<PyAny>,
    loop_: Py<VeloxLoop>,
    state: TransportState,
    write_buffer: BytesMut,
    write_buffer_high: usize,
    write_buffer_low: usize,
    #[allow(dead_code)]
    server_hostname: Option<String>,
    ssl_context: Py<SSLContext>,
    handshake_complete: bool,
}

struct TlsState {
    stream: TcpStream,
    connection: TlsConnection,
}

enum TlsConnection {
    Client(rustls::ClientConnection),
    Server(rustls::ServerConnection),
}

impl TlsConnection {
    fn process_tls_records(&mut self, stream: &mut TcpStream) -> std::io::Result<()> {
        match self {
            TlsConnection::Client(conn) => {
                conn.read_tls(stream)?;
                conn.process_new_packets()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                Ok(())
            }
            TlsConnection::Server(conn) => {
                conn.read_tls(stream)?;
                conn.process_new_packets()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                Ok(())
            }
        }
    }

    fn write_tls(&mut self, stream: &mut TcpStream) -> std::io::Result<()> {
        match self {
            TlsConnection::Client(conn) => conn.write_tls(stream).map(|_| ()),
            TlsConnection::Server(conn) => conn.write_tls(stream).map(|_| ()),
        }
    }

    fn wants_write(&self) -> bool {
        match self {
            TlsConnection::Client(conn) => conn.wants_write(),
            TlsConnection::Server(conn) => conn.wants_write(),
        }
    }

    fn is_handshaking(&self) -> bool {
        match self {
            TlsConnection::Client(conn) => conn.is_handshaking(),
            TlsConnection::Server(conn) => conn.is_handshaking(),
        }
    }

    fn reader(&mut self) -> Box<dyn Read + '_> {
        match self {
            TlsConnection::Client(conn) => Box::new(conn.reader()),
            TlsConnection::Server(conn) => Box::new(conn.reader()),
        }
    }

    fn writer(&mut self) -> Box<dyn Write + '_> {
        match self {
            TlsConnection::Client(conn) => Box::new(conn.writer()),
            TlsConnection::Server(conn) => Box::new(conn.writer()),
        }
    }

    fn peer_certificates(&self) -> Option<Vec<CertificateDer<'static>>> {
        match self {
            TlsConnection::Client(conn) => conn.peer_certificates().map(|c| c.to_vec()),
            TlsConnection::Server(conn) => conn.peer_certificates().map(|c| c.to_vec()),
        }
    }
}

// Implement Transport trait for SSLTransport
impl crate::transports::Transport for SSLTransport {
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match name {
            "peername" => {
                let state = self.tls_state.lock();
                if let Ok(addr) = state.stream.peer_addr() {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "sockname" => {
                let state = self.tls_state.lock();
                if let Ok(addr) = state.stream.local_addr() {
                    return Ok(crate::utils::ipv6::socket_addr_to_tuple(py, addr)?);
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "sslcontext" => Ok(self.ssl_context.clone_ref(py).into_any()),
            "ssl_object" => Ok(py.None()),
            "peercert" => {
                let state = self.tls_state.lock();
                let conn = &state.connection;
                if let Some(certs) = conn.peer_certificates() {
                    if let Some(cert) = certs.first() {
                        let cert_bytes = PyBytes::new(py, cert.as_ref());
                        return Ok(cert_bytes.into());
                    }
                }
                Ok(default.unwrap_or_else(|| py.None()))
            }
            "cipher" => Ok(default.unwrap_or_else(|| py.None())),
            "compression" => Ok(default.unwrap_or_else(|| py.None())),
            _ => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn is_closing(&self) -> bool {
        self.state.contains(TransportState::CLOSING) || self.state.contains(TransportState::CLOSED)
    }

    fn get_fd(&self) -> RawFd {
        self.fd
    }
}

// Implement StreamTransport trait for SSLTransport
impl crate::transports::StreamTransport for SSLTransport {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.state.contains(TransportState::CLOSING)
            || self.state.contains(TransportState::CLOSED)
        {
            return Ok(());
        }

        self.state.insert(TransportState::CLOSING);

        if self.write_buffer.is_empty() {
            self.force_close(py)?;
        }
        Ok(())
    }

    fn force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close_internal(py)
    }

    fn write(&mut self, py: Python<'_>, data: Bound<'_, PyAny>) -> PyResult<()> {
        let buf = PyBuffer::<u8>::get(&data)?;
        let slice = buf.as_slice(py).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyBufferError, _>(
                "Could not get buffer as slice",
            )
        })?;

        if self.state.contains(TransportState::CLOSING)
            || self.state.contains(TransportState::CLOSED)
        {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Cannot write to closing transport",
            ));
        }

        let data_slice: Vec<u8> = slice.iter().map(|cell| cell.get()).collect();
        self.write_buffer.extend_from_slice(&data_slice);

        let mut state = self.tls_state.lock();
        let mut writer = state.connection.writer();

        match writer.write_all(&data_slice) {
            Ok(_) => {
                drop(writer);
                // Split the mutable borrows by destructuring
                let TlsState { connection, stream } = &mut *state;
                match connection.write_tls(stream) {
                    Ok(_) => Ok(()),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    fn recv_into(&mut self, py: Python<'_>, buffer: Bound<'_, PyAny>) -> PyResult<usize> {
        let buf = PyBuffer::<u8>::get(&buffer)?;
        let slice = buf.as_mut_slice(py).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyBufferError, _>(
                "Could not get buffer as mutable slice",
            )
        })?;

        let mut state = self.tls_state.lock();
        let mut reader = state.connection.reader();

        // reader.read expects &mut [u8]; PyBuffer gives &mut [Cell<u8>],
        // so read into a temporary u8 buffer then copy into the Cell slice.
        let mut temp_buf = vec![0u8; slice.len()];
        match reader.read(&mut temp_buf) {
            Ok(n) => {
                for (i, b) in temp_buf[..n].iter().enumerate() {
                    slice[i].set(*b);
                }
                Ok(n)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    fn write_eof(&mut self) -> PyResult<()> {
        let state = self.tls_state.lock();
        state.stream.shutdown(std::net::Shutdown::Write)?;
        Ok(())
    }

    fn get_write_buffer_size(&self) -> usize {
        self.write_buffer.len()
    }

    fn set_write_buffer_limits(
        &mut self,
        py: Python<'_>,
        high: Option<usize>,
        low: Option<usize>,
    ) -> PyResult<()> {
        let high_limit = high.unwrap_or(DEFAULT_HIGH);
        let low_limit = low.unwrap_or_else(|| if high_limit == 0 { 0 } else { high_limit / 4 });

        // Special case: high=0 means disable flow control (both should be 0)
        // Otherwise, validate that low < high
        if high_limit > 0 && low_limit >= high_limit {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "low must be less than high",
            ));
        }

        self.write_buffer_high = high_limit;
        self.write_buffer_low = low_limit;

        if high_limit > 0 && self.write_buffer.len() > self.write_buffer_high {
            let _ = self.protocol.call_method0(py, "pause_writing");
        }

        Ok(())
    }

    fn read_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut state = self.tls_state.lock();

        // Read TLS records - split the mutable borrows
        let result = {
            let TlsState { connection, stream } = &mut *state;
            connection.process_tls_records(stream)
        };

        match result {
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) => return Err(e.into()),
        }

        // Handle handshake
        if state.connection.is_handshaking() {
            if state.connection.wants_write() {
                let TlsState { connection, stream } = &mut *state;
                match connection.write_tls(stream) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }

            if !state.connection.is_handshaking() && !self.handshake_complete {
                drop(state);
                self.handshake_complete = true;
                self.protocol
                    .call_method1(py, "connection_made", (py.None(),))?;
            }
            return Ok(());
        }

        // Read application data
        let mut buf = [0u8; 4096];
        let mut reader = state.connection.reader();

        match reader.read(&mut buf) {
            Ok(0) => {
                drop(reader);
                drop(state);
                if let Ok(res) = self.protocol.call_method0(py, "eof_received") {
                    if let Ok(keep_open) = res.extract::<bool>(py) {
                        if !keep_open {
                            self.close(py)?;
                        }
                    } else {
                        self.close(py)?;
                    }
                } else {
                    self.close(py)?;
                }
            }
            Ok(n) => {
                let data = &buf[..n];
                drop(reader);
                drop(state);
                let py_data = PyBytes::new(py, data);
                self.protocol
                    .call_method1(py, "data_received", (py_data,))?;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    fn write_ready(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut state = self.tls_state.lock();

        if state.connection.wants_write() {
            let TlsState { connection, stream } = &mut *state;
            match connection.write_tls(stream) {
                Ok(_) => {
                    if !connection.wants_write() && self.write_buffer.is_empty() {
                        drop(state);
                        self.loop_.bind(py).borrow().remove_writer(py, self.fd)?;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }
}

#[pymethods]
impl SSLTransport {
    #[pyo3(signature = (name, default=None))]
    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Delegate to trait implementation
        Transport::get_extra_info(self, py, name, default)
    }

    fn get_write_buffer_size(&self) -> usize {
        // Delegate to trait implementation
        StreamTransport::get_write_buffer_size(self)
    }

    #[pyo3(signature = (high=None, low=None))]
    fn set_write_buffer_limits(
        &mut self,
        py: Python<'_>,
        high: Option<usize>,
        low: Option<usize>,
    ) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::set_write_buffer_limits(self, py, high, low)
    }

    fn write_eof(&mut self) -> PyResult<()> {
        // Delegate to trait implementation
        StreamTransport::write_eof(self)
    }

    fn is_closing(&self) -> bool {
        // Delegate to trait implementation
        Transport::is_closing(self)
    }

    fn fileno(&self) -> RawFd {
        // Delegate to trait implementation
        Transport::get_fd(self)
    }

    fn pause_reading(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut self_ = slf.borrow_mut();

        if !self_.state.contains(TransportState::READING_PAUSED) {
            self_.state.insert(TransportState::READING_PAUSED);
            let fd = self_.fd;
            let loop_ = self_.loop_.bind(py).borrow();
            loop_.remove_reader(py, fd)?;
        }
        Ok(())
    }

    fn resume_reading(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut self_ = slf.borrow_mut();

        if self_.state.contains(TransportState::READING_PAUSED) {
            self_.state.remove(TransportState::READING_PAUSED);
            let fd = self_.fd;
            drop(self_); // Drop borrow before calling into loop

            let slf_clone = slf.clone().unbind();
            let read_callback =
                Arc::new(move |py: Python<'_>| SSLTransport::_read_ready(&slf_clone.bind(py)));
            let self_ = slf.borrow();
            let loop_ = self_.loop_.bind(py).borrow();
            loop_.add_reader_native(fd, read_callback)?;
        }
        Ok(())
    }

    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let mut protocol = None;
        let mut needs_writer = false;

        {
            let mut self_ = slf.borrow_mut();
            if self_.state.contains(TransportState::CLOSING)
                || self_.state.contains(TransportState::CLOSED)
            {
                return Ok(());
            }

            self_.state.insert(TransportState::CLOSING);

            if self_.write_buffer.is_empty() {
                self_._force_close_internal(py)?;
                protocol = Some(self_.protocol.clone_ref(py));
            } else {
                needs_writer = true;
            }
        }

        // Notify protocol after dropping borrow
        if let Some(proto) = protocol {
            let _ = proto.call_method1(py, "connection_lost", (py.None(),));
        }

        if needs_writer {
            let fd = slf.borrow().fd;
            let slf_clone = slf.clone().unbind();
            let write_callback =
                Arc::new(move |py: Python<'_>| SSLTransport::_write_ready(&slf_clone.bind(py)));
            slf.borrow()
                .loop_
                .bind(py)
                .borrow()
                .add_writer_native(fd, write_callback)?;
        }
        Ok(())
    }

    fn abort(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        let protocol = {
            let mut self_ = slf.borrow_mut();
            self_._force_close_internal(py)?;
            self_.protocol.clone_ref(py)
        };
        let _ = protocol.call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn _force_close(&mut self, py: Python<'_>) -> PyResult<()> {
        self._force_close_internal(py)?;
        let _ = self
            .protocol
            .call_method1(py, "connection_lost", (py.None(),));
        Ok(())
    }

    fn _force_close_internal(&mut self, py: Python<'_>) -> PyResult<()> {
        let fd = self.fd;

        let loop_ = self.loop_.bind(py).borrow();
        loop_.remove_reader(py, fd)?;
        loop_.remove_writer(py, fd)?;
        drop(loop_);

        // Stream will be dropped when tls_state is dropped
        Ok(())
    }

    fn write(slf: &Bound<'_, Self>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let py = slf.py();

        // Delegate to trait implementation
        let mut self_mut = slf.borrow_mut();
        StreamTransport::write(&mut *self_mut, py, data.clone().into_any())?;
        drop(self_mut);

        // Try to flush immediately
        Self::_write_ready(slf)?;

        // Register writer if needed
        let self_ = slf.borrow();
        let state = self_.tls_state.lock();
        let conn = &state.connection;
        if conn.wants_write() || !self_.write_buffer.is_empty() {
            let fd = self_.fd;
            let slf_clone = slf.clone().unbind();
            let write_callback =
                Arc::new(move |py: Python<'_>| SSLTransport::_write_ready(&slf_clone.bind(py)));
            drop(state);
            drop(self_);
            let loop_ = slf.borrow().loop_.clone_ref(py);
            loop_
                .bind(py)
                .borrow()
                .add_writer_native(fd, write_callback)?;
        }

        Ok(())
    }

    pub(crate) fn _write_ready(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();

        // Get necessary data without holding borrows
        let (fd, loop_ref) = {
            let self_ = slf.borrow();
            (self_.fd, self_.loop_.clone_ref(py))
        };

        // Process writes
        loop {
            let (should_write_app_data, should_write_tls, _, _) = {
                let self_ = slf.borrow();
                let state = self_.tls_state.lock();
                let conn = &state.connection;

                let should_write_app = !self_.write_buffer.is_empty();
                let should_write_tls = conn.wants_write();
                let buffer_empty = self_.write_buffer.is_empty();
                let wants_write = conn.wants_write();

                drop(state);
                drop(self_);
                (
                    should_write_app,
                    should_write_tls,
                    buffer_empty,
                    wants_write,
                )
            };

            // Write application data to TLS
            if should_write_app_data {
                let mut self_ = slf.borrow_mut();
                let mut state = self_.tls_state.lock();
                let conn = &mut state.connection;

                if !self_.write_buffer.is_empty() {
                    let mut writer = conn.writer();
                    match writer.write(&self_.write_buffer) {
                        Ok(n) => {
                            drop(writer);
                            drop(state);
                            let _ = self_.write_buffer.split_to(n);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            drop(writer);
                            drop(state);
                            drop(self_);
                            break;
                        }
                        Err(e) => {
                            drop(writer);
                            drop(state);
                            drop(self_);
                            return Err(e.into());
                        }
                    }
                } else {
                    drop(state);
                    drop(self_);
                }
            }

            // Write TLS data to socket
            if should_write_tls || should_write_app_data {
                let self_ = slf.borrow_mut();
                let mut state = self_.tls_state.lock();

                // Use &mut TcpStream from state - split borrow
                let TlsState {
                    connection, stream, ..
                } = &mut *state;
                match connection.write_tls(stream) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        drop(state);
                        drop(self_);
                        break;
                    }
                    Err(e) => {
                        drop(state);
                        drop(self_);
                        return Err(e.into());
                    }
                }
                drop(state);
                drop(self_);
            } else {
                break;
            }

            // Check if we're done
            let done = {
                let self_ = slf.borrow();
                let state = self_.tls_state.lock();
                let conn = &state.connection;
                let result = !conn.wants_write() && self_.write_buffer.is_empty();
                drop(state);
                drop(self_);
                result
            };

            if done {
                break;
            }
        }

        // Remove writer if nothing left to write
        let should_remove_writer = {
            let self_ = slf.borrow();
            let state = self_.tls_state.lock();
            let conn = &state.connection;
            let result = !conn.wants_write() && self_.write_buffer.is_empty();
            drop(state);
            drop(self_);
            result
        };

        if should_remove_writer {
            loop_ref.bind(py).borrow().remove_writer(py, fd).ok();

            // Handle final close if in CLOSING state
            let mut self_ = slf.borrow_mut();
            if self_.state.contains(TransportState::CLOSING) {
                self_._force_close_internal(py)?;
                let protocol = self_.protocol.clone_ref(py);
                drop(self_); // Drop borrow before calling out
                let _ = protocol.call_method1(py, "connection_lost", (py.None(),));
            }
        }

        Ok(())
    }

    pub(crate) fn _read_ready(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();

        // Get protocol reference
        let (protocol, _, _, handshake_complete) = {
            let self_ = slf.borrow();
            (
                self_.protocol.clone_ref(py),
                self_.fd,
                self_.loop_.clone_ref(py),
                self_.handshake_complete,
            )
        };

        // Read TLS records from socket
        {
            let self_ = slf.borrow_mut();
            let mut state = self_.tls_state.lock();

            let TlsState {
                connection, stream, ..
            } = &mut *state;
            match connection.process_tls_records(stream) {
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    drop(state);
                    drop(self_);
                    return Ok(());
                }
                Err(e) => {
                    drop(state);
                    drop(self_);
                    return Err(e.into());
                }
            }
            drop(state);
            drop(self_);
        }

        // Check if handshake just completed
        let handshake_just_completed = {
            let self_ = slf.borrow();
            let state = self_.tls_state.lock();
            let conn = &state.connection;
            let result = !handshake_complete && !conn.is_handshaking();
            drop(state);
            drop(self_);
            result
        };

        if handshake_just_completed {
            slf.borrow_mut().handshake_complete = true;

            // Notify protocol of connection
            let transport_py: Py<PyAny> = slf.clone().unbind().into();
            protocol.call_method1(py, "connection_made", (transport_py,))?;

            // Trigger write if needed for handshake completion
            Self::_write_ready(slf)?;

            return Ok(());
        }

        // Read application data
        let data_read = {
            let self_ = slf.borrow_mut();
            let mut state = self_.tls_state.lock();
            let conn = &mut state.connection;

            let mut buf = vec![0u8; 16384];
            let mut reader = conn.reader();

            match reader.read(&mut buf) {
                Ok(0) => {
                    drop(reader);
                    drop(state);
                    drop(self_);

                    // EOF
                    if let Ok(res) = protocol.call_method0(py, "eof_received") {
                        if let Ok(keep_open) = res.extract::<bool>(py) {
                            if !keep_open {
                                Self::close(&slf)?;
                            }
                        } else {
                            Self::close(&slf)?;
                        }
                    } else {
                        Self::close(&slf)?;
                    }
                    return Ok(());
                }
                Ok(n) => {
                    buf.truncate(n);
                    drop(reader);
                    drop(state);
                    drop(self_);
                    Some(buf)
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    drop(reader);
                    drop(state);
                    drop(self_);
                    None
                }
                Err(e) => {
                    drop(reader);
                    drop(state);
                    drop(self_);
                    return Err(e.into());
                }
            }
        };

        // Deliver data to protocol
        if let Some(data) = data_read {
            let py_data = PyBytes::new(py, &data);
            protocol.call_method1(py, "data_received", (py_data,))?;
        }

        // Handle TLS write needs (e.g., post-handshake messages)
        let needs_write = {
            let self_ = slf.borrow();
            let state = self_.tls_state.lock();
            let conn = &state.connection;
            let result = conn.wants_write();
            drop(state);
            drop(self_);
            result
        };

        if needs_write {
            Self::_write_ready(slf)?;
        }

        Ok(())
    }
}

impl SSLTransport {
    pub fn new_client(
        loop_: Py<VeloxLoop>,
        stream: TcpStream,
        protocol: Py<PyAny>,
        ssl_context: Py<SSLContext>,
        server_hostname: Option<String>,
        py: Python<'_>,
    ) -> VeloxResult<Self> {
        stream.set_nonblocking(true)?;
        let fd = stream.as_raw_fd();

        let client_config = {
            let ctx = ssl_context.borrow(py);
            ctx.client_config
                .as_ref()
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "SSL context not configured for client connections",
                    )
                })?
                .clone()
        };

        let server_name = server_hostname.as_ref().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "server_hostname is required for client SSL connections",
            )
        })?;

        let server_name = rustls::pki_types::ServerName::try_from(server_name.as_str())
            .map_err(|_| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid server hostname")
            })?
            .to_owned();

        let connection =
            rustls::ClientConnection::new(client_config, server_name).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Failed to create TLS connection: {}",
                    e
                ))
            })?;

        Ok(Self {
            fd,
            tls_state: Mutex::new(TlsState {
                stream,
                connection: TlsConnection::Client(connection),
            }),
            protocol,
            loop_,
            state: TransportState::ACTIVE,
            write_buffer: BytesMut::with_capacity(65536),
            write_buffer_high: DEFAULT_HIGH,
            write_buffer_low: DEFAULT_LOW,
            server_hostname,
            ssl_context,
            handshake_complete: false,
        })
    }

    pub fn new_server(
        loop_: Py<VeloxLoop>,
        stream: TcpStream,
        protocol: Py<PyAny>,
        ssl_context: Py<SSLContext>,
        py: Python<'_>,
    ) -> VeloxResult<Self> {
        stream.set_nonblocking(true)?;
        let fd = stream.as_raw_fd();

        let server_config = {
            let ctx = ssl_context.borrow(py);
            ctx.server_config
                .as_ref()
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        "SSL context not configured for server connections",
                    )
                })?
                .clone()
        };

        let connection = rustls::ServerConnection::new(server_config).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to create TLS connection: {}",
                e
            ))
        })?;

        Ok(Self {
            fd,
            tls_state: Mutex::new(TlsState {
                stream,
                connection: TlsConnection::Server(connection),
            }),
            protocol,
            loop_,
            state: TransportState::ACTIVE,
            write_buffer: BytesMut::with_capacity(65536),
            write_buffer_high: DEFAULT_HIGH,
            write_buffer_low: DEFAULT_LOW,
            server_hostname: None,
            ssl_context,
            handshake_complete: false,
        })
    }
}
