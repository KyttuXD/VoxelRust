//! What the game is made of: the data-driven registry, textures, fonts, models.

pub mod block;
pub mod textures;
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
