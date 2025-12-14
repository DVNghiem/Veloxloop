use pyo3::prelude::*;

mod utils;
mod handles;
mod poller;
mod callbacks;
mod timers;
mod event_loop;
mod policy;
mod transports;

use event_loop::VeloxLoop;
use transports::tcp::TcpTransport;
use policy::VeloxLoopPolicy;

#[pymodule(gil_used = false)]
fn _veloxloop(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<VeloxLoop>()?;
    m.add_class::<TcpTransport>()?;
    m.add_class::<transports::tcp::TcpServer>()?;
    m.add_class::<VeloxLoopPolicy>()?;
    Ok(())
}
