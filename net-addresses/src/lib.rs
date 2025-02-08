#![allow(unused)]
#![cfg(target_family = "unix")]
pub mod getnameinfo;
pub mod getaddrinfo;

use std::ffi::CStr;
use std::io::{Error, ErrorKind};

use libc::{c_int, c_char, socklen_t, EAI_SYSTEM};

/// Converts a `getaddrinfo` error code to an `io::Error`.
pub(crate) fn process_gai_error(ret: c_int) -> Error {
    if ret == EAI_SYSTEM {
        return Error::last_os_error();
    }
    // SAFETY: `libc::gai_strerror(ret)` returns a pointer to a static string,
    // which is valid for the lifetime of the program. `CStr::from_ptr(cstr)`
    // is safe as long as `cstr` is non-null and points to a valid NUL-terminated string.
    let cstr: *const c_char = unsafe { libc::gai_strerror(ret) };
    let err_msg: String = unsafe { CStr::from_ptr(cstr) }
        .to_string_lossy()
        .into_owned();

    Error::new(ErrorKind::Other, err_msg)
}
