//! The world itself: chunks, generation, meshing, saving, weather.

pub mod chunk;
pub mod world;
pub mod world_log;
pub mod worldgen;
pub mod mesh;
/// The faces of HD tiles, moved out of a chunk's mesh (see hd_split.rs).
pub mod hd_split;
pub mod liquid_mesh;
pub mod lava_look;
pub mod lod;
pub mod save;
pub mod migrate;
pub mod jobs;
pub mod weather;
pub mod footprint;
pub mod sections;
pub mod visibility;
pub mod uploads;
/// The top and filling blocks of a column, shared by chunk generation and distant terrain.
pub mod surface_rules;
/// Average colours of blocks, for distant terrain.
pub mod block_colour;
/// What distant terrain shows: real surface blocks and everyone's edits.
pub mod far_surface;
/// Which distant tiles must be made again after edits.
pub mod far_dirty;
