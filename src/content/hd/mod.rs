//! HD textures: pictures of tiles (blocks and items) bigger than 16 x 16, up
//! to 512 x 512, from texture packs, mods and the data folder.
//!
//! How it fits together:
//! * The normal atlas (`textures.rs`, 512 x 512, 16 x 16 tiles) always has
//!   every tile. An HD tile is in it as its picture shrunk to 16 x 16 (box
//!   filter), so everything that does not know about HD (particles, distant
//!   terrain, held items, the map) keeps working and costs nothing extra.
//! * The HD picture itself goes to a second atlas of one to four pages
//!   (`plan.rs` chooses the places, `pack.rs` is the shelf packer, `atlas.rs`
//!   paints the pages). The mesher moves the faces of HD tiles into a
//!   separate vertex list (`terrain/hd_split.rs`) and the renderer draws it
//!   once for each page: the draw calls grow by the pages in use, no more.
//! * The TEXTURE SIZE setting (`size.rs`, [`cap`]) shrinks HD pictures above
//!   it, and so does a memory budget that the atlas would not fit.
//!
//! Sizes: a picture is a tile if it is square, a multiple of 16 and 16 to 512
//! on a side (`size.rs`); others are fitted to 16 x 16 as they always were.

pub mod atlas;
pub mod pack;
pub mod plan;
pub mod scale;
pub mod size;
pub mod source;
pub mod table;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A tile's picture as read: `side` x `side` RGBA, top row first. `px` is
/// empty when only the size was asked for (the planning pass).
pub struct Pic {
    pub side: usize,
    pub px: alloc::vec::Vec<u8>,
}

static CAP: AtomicUsize = AtomicUsize::new(size::MAX_SIDE);
static MAX_TEXTURE: AtomicUsize = AtomicUsize::new(4096);
static BYTE_LIMIT: AtomicUsize = AtomicUsize::new(plan::DEFAULT_BYTE_LIMIT);
static SEEN: AtomicBool = AtomicBool::new(false);

/// Set the TEXTURE SIZE (the `texture_size` setting: 16, 32, ... 512).
pub fn set_cap_setting(v: i32) {
    CAP.store(size::cap_for(v), Ordering::Relaxed);
}

/// The most pixels on a side an HD picture is kept at (16 means none is).
pub fn cap() -> usize {
    CAP.load(Ordering::Relaxed)
}

/// The largest texture the graphics card takes (the renderer asks the card
/// and says so here); 4096 until then.
pub fn set_max_texture(n: usize) {
    MAX_TEXTURE.store(n.clamp(256, 16384), Ordering::Relaxed);
}

/// The side of the biggest page: the card's limit, but never over 4096.
pub fn max_texture() -> usize {
    MAX_TEXTURE.load(Ordering::Relaxed).min(4096)
}

/// The most bytes (card and main memory) the HD atlas may cost.
pub fn set_byte_limit(bytes: usize) {
    BYTE_LIMIT.store(bytes.max(1 << 20), Ordering::Relaxed);
}

pub fn byte_limit() -> usize {
    BYTE_LIMIT.load(Ordering::Relaxed)
}

/// Some HD picture has been met (even one that was shrunk away): changing the
/// TEXTURE SIZE is worth a rebuild of the atlas.
pub fn note_seen() {
    SEEN.store(true, Ordering::Relaxed);
}

pub fn in_use() -> bool {
    SEEN.load(Ordering::Relaxed) || table::current().is_some_and(|t| !t.pages.is_empty())
}

/// Shrink a picture of a creature or a model to what is allowed: at most 512
/// on a side, and at most the TEXTURE SIZE (but not below 64, the size of the
/// game's own). It is halved while it is too big and has even sides.
pub fn limit_picture(img: crate::image::png::Image) -> crate::image::png::Image {
    let mut img = img;
    let target = cap().max(64).min(size::MAX_SIDE);
    while (img.w > target || img.h > target) && img.w % 2 == 0 && img.h % 2 == 0 && img.w >= 2 && img.h >= 2 {
        let rgba = scale::half_wh(&img.rgba, img.w, img.h);
        img = crate::image::png::Image { w: img.w / 2, h: img.h / 2, rgba };
    }
    // A picture that cannot be halved (odd sides) and is still too big is averaged down.
    if img.w > size::MAX_SIDE || img.h > size::MAX_SIDE {
        let k = (img.w.max(img.h) + size::MAX_SIDE - 1) / size::MAX_SIDE;
        let (w, h) = ((img.w / k).max(1), (img.h / k).max(1));
        let rgba = scale::box_down_wh(&img.rgba, img.w, img.h, w, h);
        img = crate::image::png::Image { w, h, rgba };
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::png::Image;

    #[test]
    fn creature_pictures_are_limited_by_the_size_and_the_cap_but_not_below_64() {
        let big = |s: usize| Image { w: s, h: s, rgba: alloc::vec![200; s * s * 4] };
        let img = limit_picture(big(1024));
        assert_eq!((img.w, img.h), (512, 512));
        let img = limit_picture(big(64));
        assert_eq!((img.w, img.h), (64, 64));
        let img = limit_picture(big(32));
        assert_eq!((img.w, img.h), (32, 32), "small pictures are left");
        // Odd sizes over the limit are averaged down.
        let odd = Image { w: 1025, h: 513, rgba: alloc::vec![9; 1025 * 513 * 4] };
        let img = limit_picture(odd);
        assert!(img.w <= 512 && img.h <= 512 && img.rgba.len() == img.w * img.h * 4);
    }
}
