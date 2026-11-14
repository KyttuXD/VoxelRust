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
/// Mods' plain-text scripts, run by the game (see docs/SCRIPTING.md).
pub mod script;
/// The code of the mods found in the mods folder (VOXELRUST_MODS_DIR, default
/// `../VoxelRust-Mods`) when the game was built with the `native-mods`
/// feature (see [`native`]); empty otherwise.
pub mod mods {
    include!(concat!(env!("OUT_DIR"), "/native_mods.rs"));
}
pub use util::{audio as wav, budget, image, math};
pub use util::data::{json, settings};
pub use util::hash::sha256;
pub use util::image::png;
pub use util::math::noise;
pub use util::archive::zip;
pub use content::{block, textures, painters, palette, font, icon, bbmodel, model, pack_entities};
pub use terrain::{chunk, world, worldgen, mesh, lod, save, jobs, weather};
pub use game::{player, inventory, interact, entity, entity_explode, entity_variant, path, session, explosion, primed, chest_anim, crafting};
pub use net::{proto, server, netplay, voice as net_voice};
pub use client::{app, render, ui, audio, update, picker, skinfile};
