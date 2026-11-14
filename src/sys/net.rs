//! A small TCP socket layer over the C library's BSD sockets (Winsock on
//! Windows).
//!
//! Every socket is non-blocking; anything that has to wait does so in `poll`
//! (`select` on Windows) with a timeout, so nothing here can hang for ever.
//! The one exception is name resolution: `getaddrinfo` has no timeout of its
//! own. There is no clock in this module either: a deadline that spans several
//! waits is counted down by the time each wait was asked for.
//!
//! Only calls that exist on Mac OS X 10.6, any glibc and Windows Vista are
//! used.

use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// The host name could not be resolved.
    Resolve,
    /// No address of the host accepted the connection.
    Connect,
    /// The deadline passed first.
    Timeout,
    /// The peer closed or reset the connection.
    Closed,
    /// Anything else the operating system reported.
    Io,
}

pub struct TcpStream {
    fd: os::Sock,
}

pub struct TcpListener {
    fd: os::Sock,
    port: u16,
}

const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const IPPROTO_TCP: c_int = 6;
const TCP_NODELAY: c_int = 1;

// ------------------------------------------------------------------- unix

#[cfg(unix)]
mod os {
    use core::ffi::{c_char, c_int, c_void};

    pub type Sock = c_int;
    pub const INVALID: Sock = -1;

    pub const E_INTR: c_int = 4;
    pub const E_PIPE: c_int = 32;

    #[cfg(target_os = "linux")]
    mod k {
        use core::ffi::c_int;
        pub const E_AGAIN: c_int = 11;
        pub const E_INPROGRESS: c_int = 115;
        pub const E_CONNABORTED: c_int = 103;
        pub const E_CONNRESET: c_int = 104;
        pub const SOL_SOCKET: c_int = 1;
        pub const SO_REUSEADDR: c_int = 2;
        pub const SO_ERROR: c_int = 4;
        pub const O_NONBLOCK: c_int = 0x800;
        /// MSG_NOSIGNAL: a send to a closed peer fails instead of raising SIGPIPE.
        pub const SEND_FLAGS: c_int = 0x4000;
        pub type NfdsT = core::ffi::c_ulong;
    }

    // Mac OS X and the other BSDs.
    #[cfg(not(target_os = "linux"))]
    mod k {
        use core::ffi::c_int;
        pub const E_AGAIN: c_int = 35;
        pub const E_INPROGRESS: c_int = 36;
        pub const E_CONNABORTED: c_int = 53;
        pub const E_CONNRESET: c_int = 54;
        pub const SOL_SOCKET: c_int = 0xffff;
        pub const SO_REUSEADDR: c_int = 0x0004;
        pub const SO_ERROR: c_int = 0x1007;
        /// There is no MSG_NOSIGNAL here; the socket option does the same.
        pub const SO_NOSIGPIPE: c_int = 0x1022;
        pub const O_NONBLOCK: c_int = 0x0004;
        pub const SEND_FLAGS: c_int = 0;
        pub type NfdsT = core::ffi::c_uint;
    }

    pub use k::{E_AGAIN, E_CONNABORTED, E_CONNRESET, E_INPROGRESS, SOL_SOCKET, SO_REUSEADDR};
    use k::*;

    const F_GETFL: c_int = 3;
    const F_SETFL: c_int = 4;
    const POLLIN: i16 = 0x1;
    const POLLOUT: i16 = 0x4;

    #[repr(C)]
    pub struct AddrInfo {
        pub ai_flags: c_int,
        pub ai_family: c_int,
        pub ai_socktype: c_int,
        pub ai_protocol: c_int,
        pub ai_addrlen: u32,
        #[cfg(target_os = "linux")]
        pub ai_addr: *mut c_void,
        pub ai_canonname: *mut c_char,
        #[cfg(not(target_os = "linux"))]
        pub ai_addr: *mut c_void,
        pub ai_next: *mut AddrInfo,
    }

    #[repr(C)]
    struct PollFd {
        fd: c_int,
        events: i16,
        revents: i16,
    }

    // The C library itself is linked by `sys/mod.rs`.
    extern "C" {
        fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
        fn connect(fd: c_int, addr: *const c_void, len: u32) -> c_int;
        fn bind(fd: c_int, addr: *const c_void, len: u32) -> c_int;
        fn listen(fd: c_int, backlog: c_int) -> c_int;
        fn accept(fd: c_int, addr: *mut c_void, len: *mut u32) -> c_int;
        fn getsockname(fd: c_int, addr: *mut c_void, len: *mut u32) -> c_int;
        fn getpeername(fd: c_int, addr: *mut c_void, len: *mut u32) -> c_int;
        fn send(fd: c_int, buf: *const c_void, len: usize, flags: c_int) -> isize;
        fn recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> isize;
        fn close(fd: c_int) -> c_int;
        fn setsockopt(fd: c_int, level: c_int, name: c_int, val: *const c_void, len: u32) -> c_int;
        fn getsockopt(fd: c_int, level: c_int, name: c_int, val: *mut c_void, len: *mut u32) -> c_int;
        fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
        fn poll(fds: *mut PollFd, n: NfdsT, timeout_ms: c_int) -> c_int;
        pub fn getaddrinfo(node: *const c_char, service: *const c_char, hints: *const AddrInfo, res: *mut *mut AddrInfo) -> c_int;
        pub fn freeaddrinfo(res: *mut AddrInfo);
        #[cfg(target_os = "linux")]
        #[link_name = "__errno_location"]
        fn errno_ptr() -> *mut c_int;
        #[cfg(not(target_os = "linux"))]
        #[link_name = "__error"]
        fn errno_ptr() -> *mut c_int;
    }

    pub fn startup() -> bool {
        true
    }

    pub fn last_err() -> c_int {
        unsafe { *errno_ptr() }
    }

    /// The bytes of a `sockaddr_in` for 0.0.0.0 and `port`.
    pub fn sockaddr_in(port: u16) -> [u8; 16] {
        let mut a = [0u8; 16];
        #[cfg(target_os = "linux")]
        {
            let f = (super::AF_INET as u16).to_ne_bytes();
            a[0] = f[0];
            a[1] = f[1];
        }
        #[cfg(not(target_os = "linux"))]
        {
            a[0] = 16; // sin_len
            a[1] = super::AF_INET as u8;
        }
        let p = port.to_be_bytes();
        a[2] = p[0];
        a[3] = p[1];
        a
    }

    pub fn sock_open(family: c_int, kind: c_int, protocol: c_int) -> Sock {
        let s = unsafe { socket(family, kind, protocol) };
        if s < 0 {
            INVALID
        } else {
            s
        }
    }

    /// Make the socket non-blocking and keep it from raising SIGPIPE.
    pub fn sock_prepare(s: Sock) -> bool {
        unsafe {
            let fl = fcntl(s, F_GETFL);
            if fl < 0 || fcntl(s, F_SETFL, fl | O_NONBLOCK) < 0 {
                return false;
            }
        }
        #[cfg(not(target_os = "linux"))]
        sock_setopt(s, SOL_SOCKET, SO_NOSIGPIPE, 1);
        true
    }

    pub fn sock_close(s: Sock) {
        unsafe {
            close(s);
        }
    }

    /// # Safety
    /// `addr` must point to `len` readable bytes.
    pub unsafe fn sock_connect(s: Sock, addr: *const c_void, len: usize) -> c_int {
        connect(s, addr, len as u32)
    }

    pub fn sock_bind(s: Sock, addr: &[u8; 16]) -> c_int {
        unsafe { bind(s, addr.as_ptr() as *const c_void, 16) }
    }

    pub fn sock_listen(s: Sock, backlog: c_int) -> c_int {
        unsafe { listen(s, backlog) }
    }

    pub fn sock_accept(s: Sock) -> Sock {
        let c = unsafe { accept(s, core::ptr::null_mut(), core::ptr::null_mut()) };
        if c < 0 {
            INVALID
        } else {
            c
        }
    }

    pub fn sock_name(s: Sock, addr: &mut [u8; 16]) -> bool {
        let mut len: u32 = 16;
        unsafe { getsockname(s, addr.as_mut_ptr() as *mut c_void, &mut len) == 0 }
    }

    /// The address of the other end (an IPv4 `sockaddr_in`).
    pub fn sock_peer(s: Sock, addr: &mut [u8; 16]) -> bool {
        let mut len: u32 = 16;
        unsafe { getpeername(s, addr.as_mut_ptr() as *mut c_void, &mut len) == 0 }
    }

    pub fn sock_send(s: Sock, data: &[u8]) -> isize {
        unsafe { send(s, data.as_ptr() as *const c_void, data.len(), SEND_FLAGS) }
    }

    pub fn sock_recv(s: Sock, buf: &mut [u8]) -> isize {
        unsafe { recv(s, buf.as_mut_ptr() as *mut c_void, buf.len(), 0) }
    }

    pub fn sock_setopt(s: Sock, level: c_int, name: c_int, val: c_int) -> c_int {
        unsafe { setsockopt(s, level, name, &val as *const c_int as *const c_void, 4) }
    }

    /// The pending error of the socket (0 if none), as left by a
    /// non-blocking connect.
    pub fn sock_error(s: Sock) -> c_int {
        let mut val: c_int = 0;
        let mut len: u32 = 4;
        let rc = unsafe { getsockopt(s, SOL_SOCKET, SO_ERROR, &mut val as *mut c_int as *mut c_void, &mut len) };
        if rc != 0 {
            return -1;
        }
        val
    }

    /// Wait until the socket is readable (or writable). Returns 1 when it is
    /// (or has an error to report), 0 on timeout, -1 on failure.
    pub fn sock_wait(s: Sock, write: bool, timeout_ms: u32) -> c_int {
        let ms = if timeout_ms > i32::MAX as u32 { i32::MAX } else { timeout_ms as c_int };
        // A signal interrupts the wait; start it again, but not for ever.
        for _ in 0..32 {
            let mut p = PollFd { fd: s, events: if write { POLLOUT } else { POLLIN }, revents: 0 };
            let rc = unsafe { poll(&mut p, 1, ms) };
            if rc > 0 {
                return 1;
            }
            if rc == 0 {
                return 0;
            }
            if last_err() != E_INTR {
                return -1;
            }
        }
        0
    }
}

// ---------------------------------------------------------------- windows

#[cfg(windows)]
mod os {
    use core::ffi::{c_char, c_int, c_void};
    use core::sync::atomic::{AtomicBool, Ordering};

    pub type Sock = usize;
    pub const INVALID: Sock = usize::MAX;

    pub const E_INTR: c_int = 10004;
    pub const E_AGAIN: c_int = 10035; // WSAEWOULDBLOCK
    pub const E_INPROGRESS: c_int = 10036;
    pub const E_CONNABORTED: c_int = 10053;
    pub const E_CONNRESET: c_int = 10054;
    pub const E_PIPE: c_int = 10058; // WSAESHUTDOWN
    pub const SOL_SOCKET: c_int = 0xffff;
    const SO_ERROR: c_int = 0x1007;
    const FIONBIO: c_int = 0x8004667Eu32 as c_int;

    #[repr(C)]
    pub struct AddrInfo {
        pub ai_flags: c_int,
        pub ai_family: c_int,
        pub ai_socktype: c_int,
        pub ai_protocol: c_int,
        pub ai_addrlen: usize,
        pub ai_canonname: *mut c_char,
        pub ai_addr: *mut c_void,
        pub ai_next: *mut AddrInfo,
    }

    #[repr(C)]
    struct FdSet {
        count: u32,
        socks: [Sock; 64],
    }

    #[repr(C)]
    struct TimeVal {
        sec: i32,
        usec: i32,
    }

    #[link(name = "ws2_32", kind = "raw-dylib")]
    extern "system" {
        fn WSAStartup(version: u16, data: *mut c_void) -> c_int;
        fn WSAGetLastError() -> c_int;
        fn socket(af: c_int, kind: c_int, protocol: c_int) -> Sock;
        fn connect(s: Sock, addr: *const c_void, len: c_int) -> c_int;
        fn bind(s: Sock, addr: *const c_void, len: c_int) -> c_int;
        fn listen(s: Sock, backlog: c_int) -> c_int;
        fn accept(s: Sock, addr: *mut c_void, len: *mut c_int) -> Sock;
        fn getsockname(s: Sock, addr: *mut c_void, len: *mut c_int) -> c_int;
        fn getpeername(s: Sock, addr: *mut c_void, len: *mut c_int) -> c_int;
        fn send(s: Sock, buf: *const u8, len: c_int, flags: c_int) -> c_int;
        fn recv(s: Sock, buf: *mut u8, len: c_int, flags: c_int) -> c_int;
        fn closesocket(s: Sock) -> c_int;
        fn setsockopt(s: Sock, level: c_int, name: c_int, val: *const c_void, len: c_int) -> c_int;
        fn getsockopt(s: Sock, level: c_int, name: c_int, val: *mut c_void, len: *mut c_int) -> c_int;
        fn ioctlsocket(s: Sock, cmd: c_int, arg: *mut u32) -> c_int;
        fn select(nfds: c_int, read: *mut FdSet, write: *mut FdSet, except: *mut FdSet, timeout: *const TimeVal) -> c_int;
        pub fn getaddrinfo(node: *const c_char, service: *const c_char, hints: *const AddrInfo, res: *mut *mut AddrInfo) -> c_int;
        pub fn freeaddrinfo(res: *mut AddrInfo);
    }

    static STARTED: AtomicBool = AtomicBool::new(false);

    pub fn startup() -> bool {
        if STARTED.load(Ordering::Acquire) {
            return true;
        }
        // WSADATA is about 400 bytes; it is only written to. Winsock counts
        // start-ups itself, so two threads racing here do no harm.
        let mut data = [0u64; 64];
        let ok = unsafe { WSAStartup(0x0202, data.as_mut_ptr() as *mut c_void) } == 0;
        if ok {
            STARTED.store(true, Ordering::Release);
        }
        ok
    }

    pub fn last_err() -> c_int {
        unsafe { WSAGetLastError() }
    }

    /// The bytes of a `sockaddr_in` for 0.0.0.0 and `port`.
    pub fn sockaddr_in(port: u16) -> [u8; 16] {
        let mut a = [0u8; 16];
        let f = (super::AF_INET as u16).to_ne_bytes();
        a[0] = f[0];
        a[1] = f[1];
        let p = port.to_be_bytes();
        a[2] = p[0];
        a[3] = p[1];
        a
    }

    pub fn sock_open(family: c_int, kind: c_int, protocol: c_int) -> Sock {
        unsafe { socket(family, kind, protocol) }
    }

    /// Make the socket non-blocking. (Windows has no SIGPIPE.)
    pub fn sock_prepare(s: Sock) -> bool {
        let mut on: u32 = 1;
        unsafe { ioctlsocket(s, FIONBIO, &mut on) == 0 }
    }

    pub fn sock_close(s: Sock) {
        unsafe {
            closesocket(s);
        }
    }

    /// # Safety
    /// `addr` must point to `len` readable bytes.
    pub unsafe fn sock_connect(s: Sock, addr: *const c_void, len: usize) -> c_int {
        connect(s, addr, len as c_int)
    }

    pub fn sock_bind(s: Sock, addr: &[u8; 16]) -> c_int {
        unsafe { bind(s, addr.as_ptr() as *const c_void, 16) }
    }

    pub fn sock_listen(s: Sock, backlog: c_int) -> c_int {
        unsafe { listen(s, backlog) }
    }

    pub fn sock_accept(s: Sock) -> Sock {
        unsafe { accept(s, core::ptr::null_mut(), core::ptr::null_mut()) }
    }

    pub fn sock_name(s: Sock, addr: &mut [u8; 16]) -> bool {
        let mut len: c_int = 16;
        unsafe { getsockname(s, addr.as_mut_ptr() as *mut c_void, &mut len) == 0 }
    }

    /// The address of the other end (an IPv4 `sockaddr_in`).
    pub fn sock_peer(s: Sock, addr: &mut [u8; 16]) -> bool {
        let mut len: c_int = 16;
        unsafe { getpeername(s, addr.as_mut_ptr() as *mut c_void, &mut len) == 0 }
    }

    const MAX_IO: usize = 1 << 30;

    pub fn sock_send(s: Sock, data: &[u8]) -> isize {
        let n = if data.len() > MAX_IO { MAX_IO } else { data.len() };
        unsafe { send(s, data.as_ptr(), n as c_int, 0) as isize }
    }

    pub fn sock_recv(s: Sock, buf: &mut [u8]) -> isize {
        let n = if buf.len() > MAX_IO { MAX_IO } else { buf.len() };
        unsafe { recv(s, buf.as_mut_ptr(), n as c_int, 0) as isize }
    }

    pub fn sock_setopt(s: Sock, level: c_int, name: c_int, val: c_int) -> c_int {
        unsafe { setsockopt(s, level, name, &val as *const c_int as *const c_void, 4) }
    }

    /// The pending error of the socket (0 if none), as left by a
    /// non-blocking connect.
    pub fn sock_error(s: Sock) -> c_int {
        let mut val: c_int = 0;
        let mut len: c_int = 4;
        let rc = unsafe { getsockopt(s, SOL_SOCKET, SO_ERROR, &mut val as *mut c_int as *mut c_void, &mut len) };
        if rc != 0 {
            return -1;
        }
        val
    }

    /// Wait until the socket is readable (or writable). Returns 1 when it is
    /// (or has an error to report), 0 on timeout, -1 on failure.
    pub fn sock_wait(s: Sock, write: bool, timeout_ms: u32) -> c_int {
        let mut set = FdSet { count: 1, socks: [0; 64] };
        set.socks[0] = s;
        // A failed connect shows up in the exception set, not the write set.
        let mut exc = FdSet { count: 1, socks: [0; 64] };
        exc.socks[0] = s;
        let tv = TimeVal { sec: (timeout_ms / 1000) as i32, usec: (timeout_ms % 1000 * 1000) as i32 };
        let null = core::ptr::null_mut();
        let rc = unsafe {
            if write {
                select(0, null, &mut set, &mut exc, &tv)
            } else {
                select(0, &mut set, null, &mut exc, &tv)
            }
        };
        if rc > 0 {
            1
        } else {
            rc
        }
    }
}

// ----------------------------------------------------------------- shared

/// Call once before anything else (WSAStartup on Windows; nothing elsewhere).
/// Safe to call repeatedly; `connect` and `bind` also call it themselves.
pub fn init() -> bool {
    os::startup()
}

fn would_block(e: c_int) -> bool {
    e == os::E_AGAIN || e == os::E_INPROGRESS
}

fn io_error(e: c_int) -> NetError {
    if e == os::E_CONNRESET || e == os::E_CONNABORTED || e == os::E_PIPE {
        NetError::Closed
    } else {
        NetError::Io
    }
}

/// One connection attempt. `Err(true)` means it ran out of time.
///
/// # Safety
/// `a` must be an entry of a live `getaddrinfo` result.
unsafe fn connect_one(a: &os::AddrInfo, timeout_ms: u32) -> Result<os::Sock, bool> {
    if a.ai_addr.is_null() {
        return Err(false);
    }
    let s = os::sock_open(a.ai_family, a.ai_socktype, a.ai_protocol);
    if s == os::INVALID {
        return Err(false);
    }
    if !os::sock_prepare(s) {
        os::sock_close(s);
        return Err(false);
    }
    if os::sock_connect(s, a.ai_addr as *const c_void, a.ai_addrlen as usize) != 0 {
        let e = os::last_err();
        if !(would_block(e) || e == os::E_INTR) {
            os::sock_close(s);
            return Err(false);
        }
        // In progress: the socket turns writable when it is settled.
        let ready = os::sock_wait(s, true, timeout_ms);
        if ready <= 0 || os::sock_error(s) != 0 {
            os::sock_close(s);
            return Err(ready == 0);
        }
    }
    Ok(s)
}

impl TcpStream {
    /// Resolve `host` (a name or an IPv4/IPv6 literal) and connect, trying
    /// each of its addresses in turn, within `timeout_ms` in total (not
    /// counting the name lookup itself).
    pub fn connect(host: &str, port: u16, timeout_ms: u32) -> Result<TcpStream, NetError> {
        if !init() {
            return Err(NetError::Io);
        }
        if host.is_empty() || host.as_bytes().contains(&0) {
            return Err(NetError::Resolve);
        }
        let mut name: Vec<u8> = Vec::with_capacity(host.len() + 1);
        name.extend_from_slice(host.as_bytes());
        name.push(0);

        // The port as a decimal C string.
        let mut service = [0u8; 6];
        let mut digits = [0u8; 5];
        let mut n = 0;
        let mut p = port;
        loop {
            digits[n] = b'0' + (p % 10) as u8;
            n += 1;
            p /= 10;
            if p == 0 {
                break;
            }
        }
        for i in 0..n {
            service[i] = digits[n - 1 - i];
        }

        unsafe {
            let mut hints: os::AddrInfo = core::mem::zeroed();
            hints.ai_socktype = SOCK_STREAM;
            let mut list: *mut os::AddrInfo = core::ptr::null_mut();
            let rc = os::getaddrinfo(name.as_ptr() as *const c_char, service.as_ptr() as *const c_char, &hints, &mut list);
            if rc != 0 || list.is_null() {
                return Err(NetError::Resolve);
            }

            let mut left: u32 = 0;
            let mut ai = list;
            while !ai.is_null() {
                left += 1;
                ai = (*ai).ai_next;
            }

            // Share the time out evenly, so that one dead address (say, an
            // IPv6 route to nowhere) cannot use up all of it.
            let mut remaining = timeout_ms;
            let mut timed_out = false;
            let mut found = None;
            ai = list;
            while !ai.is_null() {
                let share = remaining / left;
                left -= 1;
                match connect_one(&*ai, share) {
                    Ok(s) => {
                        found = Some(s);
                        break;
                    }
                    Err(true) => {
                        timed_out = true;
                        remaining -= share;
                    }
                    Err(false) => {}
                }
                ai = (*ai).ai_next;
            }
            os::freeaddrinfo(list);

            match found {
                Some(fd) => Ok(TcpStream { fd }),
                None if timed_out => Err(NetError::Timeout),
                None => Err(NetError::Connect),
            }
        }
    }

    /// Send everything or fail. `timeout_ms` is how long the peer may go
    /// without taking any data before this gives up.
    pub fn send_all(&mut self, data: &[u8], timeout_ms: u32) -> Result<(), NetError> {
        let mut sent = 0;
        while sent < data.len() {
            let n = os::sock_send(self.fd, &data[sent..]);
            if n > 0 {
                sent += n as usize;
                continue;
            }
            let e = os::last_err();
            if n < 0 && e == os::E_INTR {
                continue;
            }
            if n < 0 && !would_block(e) {
                return Err(io_error(e));
            }
            match os::sock_wait(self.fd, true, timeout_ms) {
                0 => return Err(NetError::Timeout),
                r if r < 0 => return Err(NetError::Io),
                _ => {}
            }
        }
        Ok(())
    }

    /// Read up to `buf.len()` bytes. `Ok(0)` means nothing arrived within
    /// `timeout_ms` (0 = just poll); `Err(Closed)` when the peer closed.
    pub fn recv(&mut self, buf: &mut [u8], timeout_ms: u32) -> Result<usize, NetError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut waited = false;
        loop {
            let n = os::sock_recv(self.fd, buf);
            if n > 0 {
                return Ok(n as usize);
            }
            if n == 0 {
                return Err(NetError::Closed);
            }
            let e = os::last_err();
            if e == os::E_INTR {
                continue;
            }
            if !would_block(e) {
                return Err(io_error(e));
            }
            // Wait once; a wake-up with nothing to read counts as idle.
            if waited || timeout_ms == 0 {
                return Ok(0);
            }
            waited = true;
            match os::sock_wait(self.fd, false, timeout_ms) {
                0 => return Ok(0),
                r if r < 0 => return Err(NetError::Io),
                _ => {}
            }
        }
    }

    /// Read exactly `buf.len()` bytes or fail. `Timeout` if the stream stays
    /// silent for `timeout_ms`.
    pub fn recv_exact(&mut self, buf: &mut [u8], timeout_ms: u32) -> Result<(), NetError> {
        let mut got = 0;
        while got < buf.len() {
            match self.recv(&mut buf[got..], timeout_ms)? {
                0 => return Err(NetError::Timeout),
                n => got += n,
            }
        }
        Ok(())
    }

    /// Turn Nagle's algorithm off (`on` = send small packets at once).
    pub fn set_nodelay(&mut self, on: bool) {
        os::sock_setopt(self.fd, IPPROTO_TCP, TCP_NODELAY, on as c_int);
    }

    /// The IPv4 address of the other end (the server listens on IPv4 only), for
    /// the limits on wrong PINs per address.
    pub fn peer_ip(&self) -> Option<[u8; 4]> {
        let mut addr = [0u8; 16];
        if os::sock_peer(self.fd, &mut addr) && (addr[0] == 2 || addr[1] == 2) {
            Some([addr[4], addr[5], addr[6], addr[7]])
        } else {
            None
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        os::sock_close(self.fd);
    }
}

impl TcpListener {
    /// Listen on all IPv4 interfaces on `port`; port 0 picks a free one.
    pub fn bind(port: u16) -> Result<TcpListener, NetError> {
        if !init() {
            return Err(NetError::Io);
        }
        let fd = os::sock_open(AF_INET, SOCK_STREAM, 0);
        if fd == os::INVALID {
            return Err(NetError::Io);
        }
        // From here on the socket is closed by Drop.
        let mut l = TcpListener { fd, port };
        // Lets a restarted server take its port back at once. Not on
        // Windows, where the option means "steal the port" instead.
        #[cfg(unix)]
        os::sock_setopt(fd, os::SOL_SOCKET, os::SO_REUSEADDR, 1);
        let mut addr = os::sockaddr_in(port);
        if !os::sock_prepare(fd) || os::sock_bind(fd, &addr) != 0 || os::sock_listen(fd, 16) != 0 {
            return Err(NetError::Io);
        }
        if !os::sock_name(fd, &mut addr) {
            return Err(NetError::Io);
        }
        l.port = u16::from_be_bytes([addr[2], addr[3]]);
        Ok(l)
    }

    pub fn local_port(&self) -> u16 {
        self.port
    }

    /// `Ok(None)` if nobody connected within `timeout_ms`.
    pub fn accept(&mut self, timeout_ms: u32) -> Result<Option<TcpStream>, NetError> {
        let mut waited = false;
        loop {
            let s = os::sock_accept(self.fd);
            if s != os::INVALID {
                let stream = TcpStream { fd: s };
                if !os::sock_prepare(s) {
                    return Err(NetError::Io);
                }
                return Ok(Some(stream));
            }
            let e = os::last_err();
            if e == os::E_INTR {
                continue;
            }
            // The caller gave up between the wait and the accept.
            if e == os::E_CONNABORTED {
                return Ok(None);
            }
            if !would_block(e) {
                return Err(NetError::Io);
            }
            if waited || timeout_ms == 0 {
                return Ok(None);
            }
            waited = true;
            match os::sock_wait(self.fd, false, timeout_ms) {
                0 => return Ok(None),
                r if r < 0 => return Err(NetError::Io),
                _ => {}
            }
        }
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        os::sock_close(self.fd);
    }
}

/// This machine's address on its network (what friends on the same
/// network type to join), if it has one. Nothing is sent anywhere: an
/// unconnected-style socket is merely asked which address it would use.
pub fn local_address() -> Option<alloc::string::String> {
    if !init() {
        return None;
    }
    const SOCK_DGRAM: c_int = 2;
    let fd = os::sock_open(AF_INET, SOCK_DGRAM, 0);
    if fd == os::INVALID {
        return None;
    }
    // An address reserved for documentation: routable, never reached.
    let mut to = os::sockaddr_in(9);
    to[4..8].copy_from_slice(&[192, 0, 2, 1]);
    let ok = unsafe { os::sock_connect(fd, to.as_ptr() as *const c_void, 16) } == 0;
    let mut me = os::sockaddr_in(0);
    let named = ok && os::sock_name(fd, &mut me);
    os::sock_close(fd);
    if named && me[4..8] != [0, 0, 0, 0] {
        Some(alloc::format!("{}.{}.{}.{}", me[4], me[5], me[6], me[7]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::vec;

    fn pair() -> (TcpStream, TcpStream) {
        assert!(init());
        let mut l = TcpListener::bind(0).unwrap();
        let port = l.local_port();
        assert_ne!(port, 0);
        let client = TcpStream::connect("127.0.0.1", port, 2000).unwrap();
        let server = l.accept(2000).unwrap().expect("no connection");
        (client, server)
    }

    #[test]
    fn is_send() {
        fn check<T: Send>() {}
        check::<TcpStream>();
        check::<TcpListener>();
    }

    #[test]
    fn small_exchange_both_ways() {
        let (mut c, mut s) = pair();
        c.set_nodelay(true);
        s.set_nodelay(true);
        c.send_all(b"hello", 1000).unwrap();
        let mut buf = [0u8; 5];
        s.recv_exact(&mut buf, 1000).unwrap();
        assert_eq!(&buf, b"hello");
        s.send_all(b"world!", 1000).unwrap();
        let mut buf = [0u8; 64];
        let mut got = 0;
        while got < 6 {
            let n = c.recv(&mut buf[got..], 1000).unwrap();
            assert!(n > 0);
            got += n;
        }
        assert_eq!(&buf[..6], b"world!");
    }

    #[test]
    fn one_megabyte() {
        let (mut c, mut s) = pair();
        let data: Vec<u8> = (0..1 << 20).map(|i: u32| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        let expect = data.clone();
        let reader = thread::spawn(move || {
            let mut buf = vec![0u8; 1 << 20];
            s.recv_exact(&mut buf, 5000).unwrap();
            // And one the other way, from the far thread.
            s.send_all(&buf[..1000], 5000).unwrap();
            buf
        });
        c.send_all(&data, 5000).unwrap();
        let mut back = [0u8; 1000];
        c.recv_exact(&mut back, 5000).unwrap();
        let got = reader.join().unwrap();
        assert!(got == expect);
        assert_eq!(&back[..], &expect[..1000]);
    }

    #[test]
    fn idle_recv_and_accept_time_out() {
        let (mut c, _s) = pair();
        let mut buf = [0u8; 8];
        assert_eq!(c.recv(&mut buf, 0), Ok(0));
        assert_eq!(c.recv(&mut buf, 50), Ok(0));
        assert_eq!(c.recv_exact(&mut buf, 50), Err(NetError::Timeout));
        let mut l = TcpListener::bind(0).unwrap();
        assert!(l.accept(0).unwrap().is_none());
        assert!(l.accept(50).unwrap().is_none());
    }

    #[test]
    fn peer_drop_is_closed() {
        let (mut c, s) = pair();
        drop(s);
        let mut buf = [0u8; 8];
        assert_eq!(c.recv(&mut buf, 2000), Err(NetError::Closed));
        assert_eq!(c.recv_exact(&mut buf, 2000), Err(NetError::Closed));
        // Writing to it must fail too, without a SIGPIPE.
        let block = [0u8; 4096];
        let mut failed = false;
        for _ in 0..100 {
            if c.send_all(&block, 1000).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
    }

    #[test]
    fn closed_port_refuses() {
        // Bind to learn a free port, then close it again.
        let port = TcpListener::bind(0).unwrap().local_port();
        let r = TcpStream::connect("127.0.0.1", port, 5000);
        assert_eq!(r.err(), Some(NetError::Connect));
    }

    #[test]
    fn localhost_resolves() {
        let mut l = TcpListener::bind(0).unwrap();
        let mut c = TcpStream::connect("localhost", l.local_port(), 2000).unwrap();
        let mut s = l.accept(2000).unwrap().expect("no connection");
        c.send_all(b"x", 1000).unwrap();
        let mut b = [0u8; 1];
        s.recv_exact(&mut b, 1000).unwrap();
        assert_eq!(b[0], b'x');
    }

    #[test]
    fn bad_names_do_not_resolve() {
        assert_eq!(TcpStream::connect("no-such-host.invalid", 80, 1000).err(), Some(NetError::Resolve));
        assert_eq!(TcpStream::connect("", 80, 1000).err(), Some(NetError::Resolve));
        assert_eq!(TcpStream::connect("a\0b", 80, 1000).err(), Some(NetError::Resolve));
    }
}
