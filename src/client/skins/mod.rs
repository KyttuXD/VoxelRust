//! Other players' skins and the player's own, with big (HD) pictures: how much
//! of the graphics card they may use (`budget`), unpacking and packing off the
//! main thread (`work`), and keeping it all in step (`hub`). See
//! docs/PLAYING.md, "Skins", and docs/IMAGES.md.

pub mod budget;
pub mod hub;
pub mod work;

pub use hub::SkinHub;
