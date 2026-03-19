pub mod cidr;
pub use socket2::{SockAddr, Socket};

use socket2::{Domain, Type};
use std::{
    io,
    net::{Ipv6Addr, SocketAddr},
};

pub fn create_bound_ipv6_socket(addr: Ipv6Addr) -> Result<std::net::TcpStream, io::Error> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, None)?;

    socket.set_freebind_v4(true)?;
    socket.set_nonblocking(true)?;
    socket.set_tcp_nodelay(true)?;
    socket.bind(&SockAddr::from(SocketAddr::new(addr.into(), 0)))?;

    Ok(socket.into())
}

pub fn create_tcp_stream(
    addr: Ipv6Addr,
    dest: impl Into<SocketAddr>,
) -> Result<std::net::TcpStream, io::Error> {
    let socket: Socket = create_bound_ipv6_socket(addr)?.into();
    // Ensure a blocking connect, since create_bound_ipv6_socket sets the socket to non-blocking.
    socket.set_nonblocking(false)?;
    socket.connect(&SockAddr::from(dest.into()))?;

    Ok(socket.into())
}
