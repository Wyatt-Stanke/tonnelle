pub mod cidr;
pub use socket2::SockAddr;

use socket2::{Domain, Socket, Type};
use std::{
    io,
    net::{Ipv6Addr, SocketAddr},
};

pub unsafe fn create_ipv6_socket(addr: Ipv6Addr) -> Result<Socket, io::Error> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, None)?;

    socket.set_freebind(true)?;
    // socket.set_nonblocking(true)?;
    socket.set_nodelay(true)?;
    socket.bind(&SockAddr::from(SocketAddr::new(addr.into(), 0)))?;

    Ok(socket)
}

pub fn create_tcp_stream(addr: Ipv6Addr, dest: impl Into<SocketAddr>) -> Result<Socket, io::Error> {
    let socket = unsafe { create_ipv6_socket(addr)? };
    socket.connect(&SockAddr::from(dest.into()))?;

    Ok(socket)
}
