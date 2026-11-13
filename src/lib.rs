//! VoxelRust: a voxel sandbox that runs on old and new machines alike.
//!
//! The crate is `no_std` (with `alloc`). Everything the game needs from the
//! operating system comes from two places: the C library (memory, loading a
//! shared library) and SDL2 (window, input, threads, files, timing). Unit
//! tests build against `std` so they run with plain `cargo test`.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

#[macro_use]
pub mod sys;

// The code lives in folders by subject; every module is also reachable by
// its short name (`crate::block`, `crate::world`...).
pub mod util;
pub mod content;
pub mod terrain;
pub mod game;
pub mod net;
pub mod client;
pub mod native;
/// The code of the mods found in `mods/` when the game was built (see
/// [`native`]).
pub mod mods {
    include!(concat!(env!("OUT_DIR"), "/native_mods.rs"));
}
pub use util::{math, noise, json, png, settings, sha256};
pub use content::{block, textures, font, icon, bbmodel, model};
pub use terrain::{chunk, world, worldgen, mesh, lod, save, jobs, weather};
pub use game::{player, inventory, interact, entity, path, session};
pub use net::{proto, server, netplay};
pub use client::{app, render, ui, audio, update};
