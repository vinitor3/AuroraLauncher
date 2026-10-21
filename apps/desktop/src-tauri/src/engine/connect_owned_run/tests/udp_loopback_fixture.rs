//! Only the 85A fixture creates a mapped socket. The production observer never
//! creates sockets or normalizes endpoints.
use std::{
    mem::size_of,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    os::windows::io::{AsRawSocket, FromRawSocket, OwnedSocket},
};
use windows_sys::Win32::Networking::WinSock::{
    bind, setsockopt, socket, AF_INET6, IN6_ADDR, IN6_ADDR_0, INVALID_SOCKET, IPPROTO_IPV6,
    IPPROTO_UDP, IPV6_V6ONLY, SOCKADDR, SOCKADDR_IN6, SOCKADDR_IN6_0, SOCKET_ERROR, SOCK_DGRAM,
};

pub(super) fn is_mapped(endpoint: SocketAddr) -> bool {
    matches!(endpoint, SocketAddr::V6(value)
        if *value.ip() == Ipv4Addr::LOCALHOST.to_ipv6_mapped()
            && value.scope_id() == 0 && value.flowinfo() == 0)
}

pub(super) fn target(endpoint: SocketAddr) -> SocketAddrV4 {
    assert!(is_mapped(endpoint), "fixture_exact_mapped_endpoint");
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port())
}

pub(super) fn bind_mapped(endpoint: SocketAddr) -> Result<UdpSocket, &'static str> {
    if !is_mapped(endpoint) {
        return Err("fixture_mapped_literal");
    }
    // The standard library initializes Winsock once before the raw API is
    // used. OwnedSocket is installed immediately after socket succeeds, so
    // setsockopt/bind errors also close exactly this socket.
    let _winsock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| "fixture_winsock_init")?;
    let raw = unsafe { socket(i32::from(AF_INET6), SOCK_DGRAM, IPPROTO_UDP) };
    if raw == INVALID_SOCKET {
        return Err("fixture_mapped_socket");
    }
    let owned = unsafe { OwnedSocket::from_raw_socket(raw as _) };
    let disabled: u32 = 0;
    // Windows requires V6ONLY=0 before bind for IPv4-mapped IPv6 sockets.
    if unsafe {
        setsockopt(
            owned.as_raw_socket() as _,
            IPPROTO_IPV6,
            IPV6_V6ONLY,
            (&disabled as *const u32).cast(),
            size_of::<u32>() as i32,
        )
    } == SOCKET_ERROR
    {
        return Err("fixture_mapped_dual_stack");
    }
    let address = SOCKADDR_IN6 {
        sin6_family: AF_INET6,
        sin6_port: endpoint.port().to_be(),
        sin6_flowinfo: 0,
        sin6_addr: IN6_ADDR {
            u: IN6_ADDR_0 {
                Byte: Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
            },
        },
        Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
    };
    if unsafe {
        bind(
            owned.as_raw_socket() as _,
            (&address as *const SOCKADDR_IN6).cast::<SOCKADDR>(),
            size_of::<SOCKADDR_IN6>() as i32,
        )
    } == SOCKET_ERROR
    {
        return Err("fixture_mapped_bind");
    }
    let socket = UdpSocket::from(owned);
    if !is_mapped(socket.local_addr().map_err(|_| "fixture_mapped_address")?) {
        return Err("fixture_mapped_representation");
    }
    Ok(socket)
}
