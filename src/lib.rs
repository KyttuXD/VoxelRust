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
pub use util::{audio as wav, image, math};
pub use util::data::{json, settings};
pub use util::hash::sha256;
pub use util::image::png;
pub use util::math::noise;
pub use util::archive::zip;
pub use content::{block, textures, painters, palette, font, icon, bbmodel, model, pack_entities};
pub use terrain::{chunk, world, worldgen, mesh, lod, save, jobs, weather};
pub use game::{player, inventory, interact, entity, entity_explode, entity_variant, path, session, explosion, primed, crafting};
pub use net::{proto, server, netplay, voice as net_voice};
pub use client::{app, render, ui, audio, update, picker, skinfile};
