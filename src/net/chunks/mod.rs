//! Land from the server, for worlds the player's game cannot make itself.
//!
//! A world whose terrain comes from a mod's own compiled-in code (the Earth
//! mod) cannot be made by a game that does not have that code: native code
//! mods are not downloaded. Such a game asks the server for the land instead,
//! one chunk at a time (docs/MULTIPLAYER.md, "Land from the server").
//!
//! - `codec`: a chunk as bytes on the wire (the save format's run-length
//!   encoding, deflated, cut into pieces) and back
//! - `limiter`: what one connection may ask for (in flight, per second, where)
//! - `cache`: the server's last few answers, as compressed bytes
//! - `gen`: the server's chunk maker: a worker thread, the cache, the edits on top
//! - `client_state`: the client's side as a plain state machine (what to ask
//!   for and when, retries, putting the pieces together, a small cache)
//! - `server_side.rs` (declared inside `net/server.rs`, so that it reaches the
//!   server's own fields): the server's half of the messages

pub mod cache;
pub mod client_state;
pub mod codec;
pub mod gen;
pub mod limiter;

#[cfg(test)]
mod net_tests;
