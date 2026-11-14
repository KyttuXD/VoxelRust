//! Multiplayer: the protocol, the server and the client side.

pub mod accounts;
pub mod chunks;
pub mod proto;
pub mod server;
pub mod netplay;
pub mod voice;
pub mod modsync;
pub mod limits;
pub mod modmsg;
pub mod version;
pub mod skinxfer;
pub mod skinslot;
pub mod skinqueue;
pub mod skinwork;
pub mod sync;
pub mod pvp;
pub mod relay_rules;

#[cfg(test)]
mod skinrelay_tests;
