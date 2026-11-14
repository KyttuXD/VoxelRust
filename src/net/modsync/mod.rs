//! Mods that come from the server instead of the player's mods folder.
//!
//! * [`path`]: which names and paths are allowed, and the size limits.
//! * [`bundle`]: a mod folder as one checked blob, and what a pack.txt says
//!   about its mod (`version`, `client no`).
//! * [`announce`]: what to do with the server's list of mods (take, keep,
//!   delete).
//! * [`tmp`]: `<data>/tmp/mods/<host_port>/<mod>/` and clearing it.
//! * [`serve`]: the server finding and bundling its mods.
//! * [`client`]: the client's download state, held by the session.
//!
//! See docs/MULTIPLAYER.md ("Mods from the server") for the whole flow.

pub mod announce;
pub mod bundle;
pub mod client;
pub mod path;
pub mod serve;
pub mod tmp;
