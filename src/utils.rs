use pyo3::exceptions::{PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::io;
use std::net::IpAddr;

pub type VeloxResult<T> = Result<T, VeloxError>;

#[derive(thiserror::Error, Debug)]
pub enum VeloxError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Python Error: {0}")]
    Python(#[from] PyErr),
    #[error("Value Error: {0}")]
    ValueError(String),
    #[error("Runtime Error: {0}")]
    RuntimeError(String),
}

impl From<VeloxError> for PyErr {
    fn from(err: VeloxError) -> PyErr {
        match err {
            VeloxError::Io(e) => PyOSError::new_err(e.to_string()),
            VeloxError::Python(e) => e,
            VeloxError::ValueError(s) => PyValueError::new_err(s),
            VeloxError::RuntimeError(s) => PyRuntimeError::new_err(s),
        }
    }
}

/// IPv6 helper utilities for improved address handling
/// These utilities are planned for future IPv6 enhancements
/// socket_addr_to_tuple() is actively used in transports
#[allow(dead_code)]
pub mod ipv6 {
    use super::*;
    use pyo3::types::{PyInt, PyString, PyTuple};
    use std::net::{Ipv6Addr, SocketAddr};

    /// Normalize an IPv6 address string to standard representation
    /// Removes leading zeros and expands :: notation properly
    pub fn normalize_ipv6_address(addr: &str) -> VeloxResult<String> {
        let ipv6: Ipv6Addr = addr
            .parse()
            .map_err(|_| VeloxError::ValueError(format!("Invalid IPv6 address: {}", addr)))?;
        Ok(ipv6.to_string())
    }

    /// Check if an IPv6 address is IPv4-mapped (::ffff:x.x.x.x)
    pub fn is_ipv4_mapped(addr: &Ipv6Addr) -> bool {
        addr.to_ipv4_mapped().is_some()
    }

    /// Convert IPv4-mapped IPv6 address to IPv4 if applicable
    pub fn to_ipv4_if_mapped(addr: &Ipv6Addr) -> IpAddr {
        if let Some(ipv4) = addr.to_ipv4_mapped() {
            IpAddr::V4(ipv4)
        } else {
            IpAddr::V6(*addr)
        }
    }

    /// Check if an IPv6 address is loopback (::1)
    pub fn is_loopback(addr: &Ipv6Addr) -> bool {
        addr.is_loopback()
    }

    /// Check if an IPv6 address is link-local (fe80::/10)
    pub fn is_link_local(addr: &Ipv6Addr) -> bool {
        // Check if first 10 bits are 1111111010
        addr.segments()[0] & 0xffc0 == 0xfe80
    }

    /// Check if an IPv6 address has a scope ID (link-local addresses typically have scope)
    pub fn needs_scope_id(addr: &Ipv6Addr) -> bool {
        // Link-local addresses (fe80::/10) need scope IDs
        addr.segments()[0] & 0xffc0 == 0xfe80
    }

    /// Validate an IPv6 address string and return parsed address
    pub fn validate_ipv6(addr_str: &str) -> VeloxResult<Ipv6Addr> {
        addr_str
            .parse::<Ipv6Addr>()
            .map_err(|_| VeloxError::ValueError(format!("Invalid IPv6 address: {}", addr_str)))
    }

    /// Check if a string is an IPv6 address (quick detection)
    pub fn is_ipv6_string(addr: &str) -> bool {
        addr.contains(':') && !addr.starts_with('[')
    }

    /// Check if a string is an IPv4 address (quick detection)
    pub fn is_ipv4_string(addr: &str) -> bool {
        addr.contains('.') && !addr.contains(':')
    }

    /// Detect address family from string representation
    /// Returns true if IPv6, false if IPv4
    pub fn detect_is_ipv6(addr: &str) -> VeloxResult<bool> {
        let parsed: IpAddr = addr
            .parse()
            .map_err(|_| VeloxError::ValueError(format!("Invalid IP address: {}", addr)))?;

        Ok(matches!(parsed, IpAddr::V6(_)))
    }

    /// Convert a SocketAddr to a Python tuple
    /// For IPv4: (ip, port)
    /// For IPv6: (ip, port, flowinfo, scope_id)
    /// This properly handles both address families with their complete information
    pub fn socket_addr_to_tuple(py: Python<'_>, addr: SocketAddr) -> PyResult<Py<PyAny>> {
        match addr {
            SocketAddr::V4(v4_addr) => {
                let ip_str = PyString::new(py, &v4_addr.ip().to_string());
                let port_num = PyInt::new(py, v4_addr.port());
                let tuple = PyTuple::new(py, vec![ip_str.as_any(), port_num.as_any()])?;
                Ok(tuple.into())
            }
            SocketAddr::V6(v6_addr) => {
                let ip_str = PyString::new(py, &v6_addr.ip().to_string());
                let port_num = PyInt::new(py, v6_addr.port());
                let flowinfo_num = PyInt::new(py, v6_addr.flowinfo());
                let scope_id_num = PyInt::new(py, v6_addr.scope_id());
                let tuple = PyTuple::new(
                    py,
                    vec![
                        ip_str.as_any(),
                        port_num.as_any(),
                        flowinfo_num.as_any(),
                        scope_id_num.as_any(),
                    ],
                )?;
                Ok(tuple.into())
            }
        }
    }
}
