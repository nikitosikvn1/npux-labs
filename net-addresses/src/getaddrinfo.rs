use std::{ptr, fmt};
use std::mem::MaybeUninit;
use std::ffi::{CStr, CString};
use std::net::SocketAddr;
use std::iter::FusedIterator;
use std::io::{self, Error, ErrorKind};
use socket2::SockAddr;
use clap::ValueEnum;

use libc::{
    c_int, addrinfo, AF_UNSPEC, AF_INET, AF_INET6, SOCK_STREAM, SOCK_DGRAM, SOCK_RAW,
    SOCK_SEQPACKET, IPPROTO_TCP, IPPROTO_UDP, IPPROTO_SCTP, IPPROTO_IP,
};

macro_rules! impl_debug {
    ($enum:ty, $($variant:ident => $debug_name:expr),+ $(,)?) => {
        impl fmt::Debug for $enum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant => write!(f, "{} ({})", $debug_name, *self as c_int),)+
                }
            }
        }
    };
}

macro_rules! impl_display {
    ($enum:ty, $($variant:ident => $display_name:expr),+ $(,)?) => {
        impl fmt::Display for $enum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant => write!(f, "{}", $display_name),)+
                }
            }
        }
    };
}

/// Address family
#[repr(i32)]
#[derive(Copy, Clone, PartialEq, Eq, Default, ValueEnum)]
pub enum AddrFamily {
    #[default]
    Unspecified = AF_UNSPEC,
    Inet = AF_INET,
    Inet6 = AF_INET6,
}

impl_debug!(
    AddrFamily,
    Unspecified => "AF_UNSPEC",
    Inet => "AF_INET",
    Inet6 => "AF_INET6"
);

impl_display!(
    AddrFamily,
    Unspecified => "Unspecified",
    Inet => "IPv4",
    Inet6 => "IPv6"
);

impl From<c_int> for AddrFamily {
    fn from(family: c_int) -> Self {
        match family {
            AF_UNSPEC => Self::Unspecified,
            AF_INET => Self::Inet,
            AF_INET6 => Self::Inet6,
            _ => panic!("Unsupported address family: {}", family),
        }
    }
}

/// Socket type
#[repr(i32)]
#[derive(Copy, Clone, PartialEq, Eq, Default, ValueEnum)]
pub enum SockType {
    #[default]
    Unspecified = 0,
    Stream = SOCK_STREAM,
    Datagram = SOCK_DGRAM,
    Raw = SOCK_RAW,
    SeqPacket = SOCK_SEQPACKET,
}

impl_debug!(
    SockType,
    Unspecified => "SOCK_UNSPEC",
    Stream => "SOCK_STREAM",
    Datagram => "SOCK_DGRAM",
    Raw => "SOCK_RAW",
    SeqPacket => "SOCK_SEQPACKET",
);

impl_display!(
    SockType,
    Unspecified => "Unspecified",
    Stream => "Stream",
    Datagram => "Datagram",
    Raw => "Raw",
    SeqPacket => "SeqPacket",
);

impl From<c_int> for SockType {
    fn from(socktype: c_int) -> Self {
        match socktype {
            0 => Self::Unspecified,
            SOCK_STREAM => Self::Stream,
            SOCK_DGRAM => Self::Datagram,
            SOCK_RAW => Self::Raw,
            SOCK_SEQPACKET => Self::SeqPacket,
            _ => panic!("Unsupported socket type: {}", socktype),
        }
    }
}

/// Protocol
#[repr(i32)]
#[derive(Copy, Clone, PartialEq, Eq, Default, ValueEnum)]
pub enum Protocol {
    #[default]
    Unspecified = IPPROTO_IP,
    Tcp = IPPROTO_TCP,
    Udp = IPPROTO_UDP,
    Sctp = IPPROTO_SCTP,
}

impl_debug!(
    Protocol,
    Unspecified => "IPPROTO_IP",
    Tcp => "IPPROTO_TCP",
    Udp => "IPPROTO_UDP",
    Sctp => "IPPROTO_SCTP",
);

impl_display!(
    Protocol,
    Unspecified => "Unspecified",
    Tcp => "TCP",
    Udp => "UDP",
    Sctp => "SCTP",
);

impl From<c_int> for Protocol {
    fn from(protocol: c_int) -> Self {
        match protocol {
            IPPROTO_IP => Self::Unspecified,
            IPPROTO_TCP => Self::Tcp,
            IPPROTO_UDP => Self::Udp,
            IPPROTO_SCTP => Self::Sctp,
            _ => panic!("Unsupported protocol: {}", protocol),
        }
    }
}

/// Holds optional hints or preferences for address resolution
#[derive(Debug, Copy, Clone, Default)]
pub struct AddrInfoHints {
    pub flags: i32,
    pub family: AddrFamily,
    pub socktype: SockType,
    pub protocol: Protocol,
}

impl AddrInfoHints {
    pub fn new(
        flags: i32,
        family: impl Into<AddrFamily>,
        socktype: impl Into<SockType>,
        protocol: impl Into<Protocol>,
    ) -> Self {
        Self {
            flags,
            family: family.into(),
            socktype: socktype.into(),
            protocol: protocol.into(),
        }
    }

    pub fn as_addrinfo(&self) -> addrinfo {
        let mut addrinfo: MaybeUninit<addrinfo> = MaybeUninit::zeroed();
        unsafe {
            let addrinfo_ptr: *mut addrinfo = addrinfo.as_mut_ptr();
            (*addrinfo_ptr).ai_flags = self.flags;
            (*addrinfo_ptr).ai_family = self.family as c_int;
            (*addrinfo_ptr).ai_socktype = self.socktype as c_int;
            (*addrinfo_ptr).ai_protocol = self.protocol as c_int;

            addrinfo.assume_init()
        }
    }
}

/// Consolidates the address info returned by [`getaddrinfo`]
#[derive(Clone)]
pub struct AddrInfo {
    pub flags: i32,
    pub family: AddrFamily,
    pub socktype: SockType,
    pub protocol: Protocol,
    pub socket_addr: SocketAddr,
    pub canonname: Option<String>,
}

// TODO: Display a set of flags as a list of flag names
impl fmt::Debug for AddrInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddrInfo")
            .field("flags", &format_args!("{:#x}", self.flags))
            .field("family", &self.family)
            .field("socktype", &self.socktype)
            .field("protocol", &self.protocol)
            .field("socket_addr", &self.socket_addr)
            .field("canonname", &self.canonname.as_deref().unwrap_or("None"))
            .finish()
    }
}

// TODO: Display a set of flags as a list of flag names
impl fmt::Display for AddrInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (Family: {}, Type: {}, Proto: {}",
            self.socket_addr, self.family, self.socktype, self.protocol,
        )?;

        if self.flags != 0 {
            write!(f, ", Flags: {:#x}", self.flags)?;
        }

        if let Some(ref canonname) = self.canonname {
            write!(f, ", Canonical name: {:?}", canonname)?;
        }

        write!(f, ")")
    }
}

impl AddrInfo {
    /// Construct an AddrInfo from a pointer to an addrinfo structure.
    ///
    /// # Safety
    /// The function dereferences `addrinfo_ptr`, so the caller must ensure it is a valid,
    /// non-null pointer to an `addrinfo` structure. The function safely copies `ai_addr`
    /// into a properly allocated storage buffer using `ptr::copy_nonoverlapping`, ensuring
    /// `addrinfo.ai_addrlen` bytes are copied. `CStr::from_ptr(addrinfo.ai_canonname)` is
    /// safe as long as `ai_canonname` is a valid NUL-terminated string when it is non-null.
    /// The caller is responsible for ensuring that `addrinfo_ptr` remains valid for the
    /// duration of this function.
    pub unsafe fn from_ptr(addrinfo_ptr: *mut addrinfo) -> io::Result<Self> {
        let addrinfo: addrinfo = *addrinfo_ptr;
        let (_, sockaddr) = SockAddr::try_init(|storage, len| {
            *len = addrinfo.ai_addrlen;
            ptr::copy_nonoverlapping(
                addrinfo.ai_addr as *const u8,
                storage as *mut u8,
                addrinfo.ai_addrlen as usize,
            );
            Ok(())
        })?;

        let socket_addr: SocketAddr = sockaddr.as_socket().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                format!("Unsupported socket address family: {:?}", sockaddr.family()),
            )
        })?;

        let canonname: Option<String> = addrinfo.ai_canonname.as_ref().map(|_| {
            CStr::from_ptr(addrinfo.ai_canonname)
                .to_string_lossy()
                .into_owned()
        });

        Ok(Self {
            flags: addrinfo.ai_flags,
            family: addrinfo.ai_family.into(),
            socktype: addrinfo.ai_socktype.into(),
            protocol: addrinfo.ai_protocol.into(),
            socket_addr,
            canonname,
        })
    }
}

/// An iterator over the linked list created by a `getaddrinfo` call
#[derive(Debug)]
pub struct AddrInfoIter {
    orig: *mut addrinfo,
    cur: *mut addrinfo,
}

impl Iterator for AddrInfoIter {
    type Item = io::Result<AddrInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let cur: &addrinfo = self.cur.as_ref()?;
            let res: io::Result<AddrInfo> = AddrInfo::from_ptr(self.cur);
            self.cur = cur.ai_next;

            Some(res)
        }
    }
}

impl FusedIterator for AddrInfoIter {}

impl Drop for AddrInfoIter {
    fn drop(&mut self) {
        unsafe { libc::freeaddrinfo(self.orig) }
    }
}

/// Translates the name of a service location and/or a service name and returns
/// an iterator over the resulting address records.
///
/// This function is a safe Rust wrapper around the system call [`libc::getaddrinfo`].
/// It takes an optional hostname and/or service name, as well as hints to narrow
/// down the type of returned addresses (e.g., IPv4 vs. IPv6, stream vs. datagram).
///
/// See: https://pubs.opengroup.org/onlinepubs/009604599/functions/getaddrinfo.html
pub fn getaddrinfo(
    host: Option<&str>,
    service: Option<&str>,
    hints: Option<AddrInfoHints>,
) -> io::Result<AddrInfoIter> {
    // Either host or service must be specified
    if host.is_none() && service.is_none() {
        Err(Error::new(
            ErrorKind::InvalidInput,
            "Either host or service must be specified",
        ))?;
    }
    let host_cstring: Option<CString> = match host {
        Some(h) => Some(
            CString::new(h)
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid host string"))?,
        ),
        None => None,
    };
    let host_ptr: *const i8 = host_cstring.as_ref().map_or_else(ptr::null, |s| s.as_ptr());

    let service_cstring: Option<CString> = match service {
        Some(s) => Some(
            CString::new(s)
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid service string"))?,
        ),
        None => None,
    };
    let service_ptr: *const i8 = service_cstring
        .as_ref()
        .map_or_else(ptr::null, |s| s.as_ptr());

    let addrinfo: addrinfo = hints.unwrap_or_default().as_addrinfo();
    let mut res_ptr: *mut addrinfo = ptr::null_mut();

    let ret: c_int = unsafe { libc::getaddrinfo(host_ptr, service_ptr, &addrinfo, &mut res_ptr) };
    if ret != 0 {
        Err(crate::process_gai_error(ret))?;
    }

    Ok(AddrInfoIter {
        orig: res_ptr,
        cur: res_ptr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc::{AI_PASSIVE, AI_CANONNAME};

    // NOTE: These tests do not cover all possible use cases and edge cases and are
    // primarily intended for demonstrating usage.

    // Returns a sample AddrInfo structure for testing purposes
    fn get_addrinfo() -> AddrInfo {
        AddrInfo {
            flags: AI_PASSIVE | AI_CANONNAME,
            family: AddrFamily::Inet,
            socktype: SockType::Stream,
            protocol: Protocol::Unspecified,
            socket_addr: ([127, 0, 0, 1], 80).into(),
            canonname: Some("localhost".into()),
        }
    }

    #[test]
    fn test_addrinfo_debug_output() {
        // GIVEN
        let addrinfo: AddrInfo = get_addrinfo();
        let expected_debug_output: &str = "AddrInfo { flags: 0x3, \
            family: AF_INET (2), \
            socktype: SOCK_STREAM (1), \
            protocol: IPPROTO_IP (0), \
            socket_addr: 127.0.0.1:80, \
            canonname: \"localhost\" }";
        // WHEN + THEN
        assert_eq!(format!("{:?}", addrinfo), expected_debug_output);
    }

    #[test]
    fn test_addrinfo_display_output() {
        // GIVEN
        let addrinfo: AddrInfo = get_addrinfo();
        let expected_display: &str = "127.0.0.1:80 (\
            Family: IPv4, \
            Type: Stream, \
            Proto: Unspecified, \
            Flags: 0x3, \
            Canonical name: \"localhost\")";
        // WHEN + THEN
        assert_eq!(addrinfo.to_string(), expected_display);
    }

    // Collects the resolved addresses (iterator) into a vector
    fn get_sockaddrs(h: Option<&str>, s: Option<&str>, hi: Option<AddrInfoHints>) -> Vec<AddrInfo> {
        getaddrinfo(h, s, hi)
            .expect("Failed to resolve addresses")
            .map(|s| s.expect("Failed to unwrap AddrInfo"))
            .collect()
    }

    #[test]
    fn test_getaddrinfo_resolves_localhost_http_without_hints() {
        // GIVEN
        let host: Option<&str> = Some("localhost");
        let service: Option<&str> = Some("http");
        let ai_hints: Option<AddrInfoHints> = None;

        let expected_inet_sa: SocketAddr = "127.0.0.1:80".parse().unwrap();
        let expected_inet6_sa: SocketAddr = "[::1]:80".parse().unwrap();
        // WHEN
        let sockaddrs: Vec<AddrInfo> = get_sockaddrs(host, service, ai_hints);
        // THEN
        assert!(sockaddrs.len() >= 4); // TCP and UDP for IPv4 and IPv6 (SCTP support depends on the platform)
        assert!(sockaddrs
            .iter()
            .any(|ai| ai.family == AddrFamily::Inet && ai.socket_addr == expected_inet_sa));
        assert!(sockaddrs
            .iter()
            .any(|ai| ai.family == AddrFamily::Inet6 && ai.socket_addr == expected_inet6_sa));
    }

    #[test]
    fn test_getaddrinfo_resolves_nfs_without_host_inet_family() {
        // GIVEN
        let host: Option<&str> = None;
        let service: Option<&str> = Some("nfs");
        let ai_hints: Option<AddrInfoHints> = Some(AddrInfoHints {
            flags: 0,
            family: AddrFamily::Inet,
            socktype: SockType::Unspecified,
            protocol: Protocol::Unspecified,
        });

        let expected_sa: SocketAddr = "127.0.0.1:2049".parse().unwrap();
        // WHEN
        let sockaddrs: Vec<AddrInfo> = get_sockaddrs(host, service, ai_hints);
        // THEN
        assert!(sockaddrs.len() >= 2); // TCP and UDP for IPv4 (SCTP support depends on the platform)
        assert!(sockaddrs
            .iter()
            .all(|ai| ai.family == AddrFamily::Inet && ai.socket_addr == expected_sa));
    }

    #[test]
    #[ignore = "Performs DNS lookup (network required)"]
    fn test_getaddrinfo_resolves_dns_google_inet6_dgram() {
        // GIVEN
        let host: Option<&str> = Some("dns.google");
        let service: Option<&str> = None;
        let ai_hints: Option<AddrInfoHints> = Some(AddrInfoHints {
            flags: 0,
            family: AddrFamily::Inet6,
            socktype: SockType::Datagram,
            protocol: Protocol::Unspecified,
        });

        let expected_sa_1: SocketAddr = "[2001:4860:4860::8844]:0".parse().unwrap();
        let expected_sa_2: SocketAddr = "[2001:4860:4860::8888]:0".parse().unwrap();
        // WHEN
        let sockaddrs: Vec<AddrInfo> = get_sockaddrs(host, service, ai_hints);
        // THEN
        assert!(sockaddrs.len() >= 2); // UDP for both IPv6 addresses
        assert!(sockaddrs.iter().all(|ai| ai.family == AddrFamily::Inet6));
        assert!(sockaddrs.iter().any(|ai| ai.socket_addr == expected_sa_1));
        assert!(sockaddrs.iter().any(|ai| ai.socket_addr == expected_sa_2));
    }

    #[test]
    fn test_getaddrinfo_missing_host_and_service() {
        // WHEN
        let result: io::Result<AddrInfoIter> = getaddrinfo(None, None, None);
        // THEN
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidInput);
    }
}
