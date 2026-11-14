//! Small self-contained helpers, in folders by subject:
//!
//! - `math` (and `math::noise`): numbers, vectors, noise
//! - `data`: JSON and the settings file
//! - `hash`: SHA-256
//! - `archive`: zip files
//! - `crypto`: Ed25519 signature checks (and SHA-512), for the updater
//! - `image`: pictures (PNG, JPEG, GIF, BMP, TGA)
//! - `audio`: sounds (WAV, AIFF, FLAC, ...)
//! - `budget`: what the game may hold in memory, and what to give up when it holds too much

pub mod archive;
pub mod audio;
pub mod budget;
pub mod crypto;
pub mod data;
pub mod hash;
pub mod image;
pub mod math;

// The older short paths (`util::noise`, `util::zip`, ...) still work: mods use them.
pub use archive::zip;
pub use audio as wav;
pub use data::{json, settings};
pub use hash::sha256;
pub use image::png;
pub use math::noise;
