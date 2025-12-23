use pyo3::prelude::*;

mod buffer_pool;
mod callbacks;
mod constants;
// epoll module kept for potential fallback, but io-uring is primary on Linux
#[cfg(target_os = "linux")]
mod epoll;
mod event_loop;
mod executor;
mod handles;
mod io_backend;
mod policy;
mod poller;
mod socket;
mod streams;
mod timers;
mod transports;
mod utils;

use callbacks::AsyncConnectCallback;
use event_loop::VeloxLoop;
use policy::VeloxLoopPolicy;
use socket::SocketOptions;
use streams::{StreamReader, StreamWriter, VeloxBuffer};
use transports::future::CompletedFuture;
use transports::ssl::{SSLContext, SSLTransport};
use transports::stream_server::{StreamServer, StreamTransport};
use transports::tcp::{SocketWrapper, TcpServer, TcpTransport};
use transports::udp::{UdpSocketWrapper, UdpTransport};

#[pymodule(gil_used = false)]
fn _veloxloop(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<VeloxLoop>()?;
    m.add_class::<TcpTransport>()?;
    m.add_class::<TcpServer>()?;
    m.add_class::<SocketWrapper>()?;
    m.add_class::<UdpTransport>()?;
    m.add_class::<UdpSocketWrapper>()?;
    m.add_class::<SSLContext>()?;
    m.add_class::<SSLTransport>()?;
    m.add_class::<CompletedFuture>()?;
    m.add_class::<AsyncConnectCallback>()?;
    m.add_class::<VeloxLoopPolicy>()?;
    m.add_class::<StreamReader>()?;
    m.add_class::<StreamWriter>()?;
    m.add_class::<VeloxBuffer>()?;
    m.add_class::<StreamServer>()?;
    m.add_class::<StreamTransport>()?;
    m.add_class::<SocketOptions>()?;
    Ok(())
}
