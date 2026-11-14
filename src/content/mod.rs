//! What the game is made of: the data-driven registry, textures, fonts, models.

pub mod block;
pub mod textures;
/// Textures bigger than 16 x 16, up to 512 x 512 (see hd/mod.rs).
pub mod hd;
pub mod effects;
pub mod art;
pub mod painters;
pub mod palette;
pub mod font;
pub mod icon;
pub mod bbmodel;
pub mod model;
pub mod bone_pose;
pub mod pack_entities;
pub mod emotes;
pub mod shader_defs;
pub mod pose;
pub mod mod_runtime;
pub mod script_claims;
pub mod dim_sky;

#[cfg(test)]
mod core_tests;
