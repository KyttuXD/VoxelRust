//! The player's side of mods: the mods folder, the Settings > MODS rows,
//! what a hosting game offers for download and the download progress line.
//! (Mods that come from a server are handled in `net::modsync`.)

pub mod folder;
pub mod progress;
pub mod rows;
pub mod serving;
