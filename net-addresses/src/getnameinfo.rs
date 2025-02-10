use std::io;
use std::ffi::CStr;
use std::net::SocketAddr;
use socket2::SockAddr;

use libc::{c_char, c_int, socklen_t};

const NI_MAXHOST: usize = 1025;
const NI_MAXSERV: usize = 32;

/// Resolves a socket address to a node (host) name and a service name.
///
/// This function is a safe Rust wrapper around the system call [`libc::getnameinfo`].
/// It takes a socket address (either IPv4 or IPv6) and attempts to resolve it
/// to a host name and a service name.
///
/// See: https://pubs.opengroup.org/onlinepubs/009604599/functions/getnameinfo.html
pub fn getnameinfo(sock: impl Into<SocketAddr>, flags: i32) -> io::Result<(String, String)> {
    let sock = SockAddr::from(sock.into());
    let mut host_buf: [c_char; NI_MAXHOST] = [0; NI_MAXHOST];
    let mut serv_buf: [c_char; NI_MAXSERV] = [0; NI_MAXSERV];

    // SAFETY: `libc::getnameinfo` writes to `host_buf` (size: NI_MAXHOST) and `serv_buf` (size: NI_MAXSERV),
    // both of which are properly allocated local arrays with sufficient space (`1025` and `32` bytes, respectively).
    // These sizes are defined in <netdb.h>. The function guarantees that on success, these buffers contain
    // valid NUL-terminated strings. `CStr::from_ptr` is safe as long as the pointers reference valid NUL-terminated data.
    let ret: c_int = unsafe {
        libc::getnameinfo(
            sock.as_ptr(),
            sock.len(),
            host_buf.as_mut_ptr(),
            host_buf.len() as socklen_t,
            serv_buf.as_mut_ptr(),
            serv_buf.len() as socklen_t,
            flags,
        )
    };

    if ret != 0 {
        Err(crate::process_gai_error(ret))?;
    }

    let host: String = unsafe { CStr::from_ptr(host_buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let service: String = unsafe { CStr::from_ptr(serv_buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();

    Ok((host, service))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc::{NI_NUMERICHOST, NI_NUMERICSERV};

    const NUMERIC_FLAGS: c_int = NI_NUMERICHOST | NI_NUMERICSERV;

    // NOTE: These tests do not cover all possible use cases and edge cases and are
    // primarily intended for demonstrating usage.

    fn assert_getnameinfo(
        (socket_addr, flags): (&str, i32),
        (expected_host, expected_service): (&str, &str),
    ) {
        let socket_addr: SocketAddr = socket_addr.parse().unwrap();
        let (host, service): (String, String) = getnameinfo(socket_addr, flags).unwrap();

        assert_eq!(
            host,
            expected_host,
            "Unexpected host for {:?}",
            socket_addr.ip(),
        );
        assert_eq!(
            service,
            expected_service,
            "Unexpected service for {:?}",
            socket_addr.port(),
        );
    }

    #[test]
    fn test_getnameinfo_ipv4_no_resolve() {
        assert_getnameinfo(("127.0.0.1:80", NUMERIC_FLAGS), ("127.0.0.1", "80"));
    }

    #[test]
    fn test_getnameinfo_ipv6_no_resolve() {
        assert_getnameinfo(("[::1]:443", NUMERIC_FLAGS), ("::1", "443"));
    }

    #[test]
    #[ignore = "Performs reverse DNS lookup (network required)"]
    fn test_getnameinfo_ipv4_resolve_host() {
        assert_getnameinfo(("8.8.8.8:443", 0), ("dns.google", "https"));
    }

    #[test]
    #[ignore = "Performs reverse DNS lookup (network required)"]
    fn test_getnameinfo_ipv6_resolve_host() {
        assert_getnameinfo(("[2620:fe::9]:22", 0), ("dns9.quad9.net", "ssh"));
    }
}
