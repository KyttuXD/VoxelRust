//! Accounts: a name that has a PIN cannot be used by anyone who does not know it
//! (docs/ACCOUNTS.md has the whole story, and an honest account of what a PIN is worth).
//!
//! - `pin`: what a PIN may be
//! - `keys`: how a PIN becomes a key and a login proof (the recipe both sides follow)
//! - `rng`: random bytes for salts and challenges
//! - `strikes`: wrong tries and lockouts
//! - `store`: the accounts and their file
//! - `config`: the server's settings (`require_accounts`, `admins`, `allow_weak_pins`)
//! - `state`: what a running server keeps, per server and per connection
//! - `persist`: reading and writing the accounts file in the server loop
//! - `set_pin`: `voxelrust --server --set-pin NAME PIN`
//! - `client`: the player's side of the handshake and of `/register`, `/pin`
//! - `remember`: the PINs a player let the game keep, per server
//! - `server_side.rs` (declared inside `net/server.rs`, so that it reaches the server's own
//!   fields): the server's half of the handshake, the admin commands

pub mod client;
pub mod config;
pub mod keys;
pub mod persist;
pub mod pin;
pub mod remember;
pub mod rng;
pub mod set_pin;
pub mod state;
pub mod store;
pub mod strikes;
#[cfg(test)]
mod tests;

/// The first protocol version that knows accounts.
pub const ACCOUNTS_SINCE: u16 = 14;
/// The oldest protocol version a server still lets in: as a guest, with a name that has no
/// account (an older game cannot log in).
pub const OLDEST_ACCEPTED: u16 = 13;

/// Whether a server accepts a client that speaks this protocol version.
pub fn version_accepted(version: u16) -> bool {
    (OLDEST_ACCEPTED..=crate::proto::VERSION).contains(&version)
}

/// The message an older game is shown when its name has an account.
pub const OLD_CLIENT_HAS_ACCOUNT: &str = "this name has an account: update your game to log in";
