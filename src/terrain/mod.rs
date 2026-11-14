//! The world itself: chunks, generation, meshing, saving, weather.

pub mod chunk;
pub mod world;
pub mod world_log;
pub mod worldgen;
pub mod mesh;
/// The faces of HD tiles, moved out of a chunk's mesh (see hd_split.rs).
pub mod hd_split;
pub mod liquid_mesh;
pub mod lod;
pub mod save;
pub mod migrate;
pub mod jobs;
pub mod weather;
pub mod footprint;
