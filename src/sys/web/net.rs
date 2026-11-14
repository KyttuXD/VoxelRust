//! Networking in the browser build: there is none. A page cannot open TCP
//! sockets, so every attempt fails at once with [`NetError::Unsupported`],
//! which the menus show as their usual "could not connect" message.
//!
//! This stands in for `sys/net.rs` and offers the same types and functions.

use alloc::string::String;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    Resolve,
    Connect,
    Timeout,
    Closed,
    Io,
    /// Not possible in a browser.
    Unsupported,
}

// Written out, since this is the text the player is shown.
impl core::fmt::Debug for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            NetError::Resolve => "Resolve",
            NetError::Connect => "Connect",
            NetError::Timeout => "Timeout",
            NetError::Closed => "Closed",
            NetError::Io => "Io",
            NetError::Unsupported => "not possible in a browser",
        })
    }
}

/// Never exists: `connect` always fails.
pub struct TcpStream(());

/// Never exists: `bind` always fails.
pub struct TcpListener(());

pub fn init() -> bool {
    false
}

impl TcpStream {
    pub fn connect(_host: &str, _port: u16, _timeout_ms: u32) -> Result<TcpStream, NetError> {
        Err(NetError::Unsupported)
    }

    pub fn send_all(&mut self, _data: &[u8], _timeout_ms: u32) -> Result<(), NetError> {
        Err(NetError::Unsupported)
    }

    pub fn recv(&mut self, _buf: &mut [u8], _timeout_ms: u32) -> Result<usize, NetError> {
        Err(NetError::Unsupported)
    }

    pub fn recv_exact(&mut self, _buf: &mut [u8], _timeout_ms: u32) -> Result<(), NetError> {
        Err(NetError::Unsupported)
    }

    pub fn set_nodelay(&mut self, _on: bool) {}
    /// The IPv4 address of the other end (none here).
    pub fn peer_ip(&self) -> Option<[u8; 4]> {
        None
    }
}

impl TcpListener {
    pub fn bind(_port: u16) -> Result<TcpListener, NetError> {
        Err(NetError::Unsupported)
    }

    pub fn local_port(&self) -> u16 {
        0
    }

    pub fn accept(&mut self, _timeout_ms: u32) -> Result<Option<TcpStream>, NetError> {
        Err(NetError::Unsupported)
    }
}

pub fn local_address() -> Option<String> {
    None
}
