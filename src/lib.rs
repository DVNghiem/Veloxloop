use pyo3::prelude::*;

mod utils;
mod handles;
mod poller;
mod callbacks;
mod timers;
mod event_loop;
mod policy;
mod transports;
mod executor;
mod streams;
mod socket;

use event_loop::VeloxLoop;
use transports::tcp::{TcpTransport, TcpServer, SocketWrapper};
use transports::udp::{UdpTransport, UdpSocketWrapper};
use transports::ssl::{SSLContext, SSLTransport};
use transports::future::CompletedFuture;
use callbacks::AsyncConnectCallback;
use policy::VeloxLoopPolicy;
use streams::{StreamReader, StreamWriter};
use socket::SocketOptions;

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
    m.add_class::<SocketOptions>()?;
    Ok(())
}
