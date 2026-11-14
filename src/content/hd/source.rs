//! Reading a tile's picture: from a mod's `texture <id> file <picture>`, a
//! texture pack or the data folder's textures/ folder.
//!
//! A mod's HD picture is kept as the file it came in (a PNG is a fraction of
//! the decoded size) in the tile's definition, and decoded each time the atlas
//! is built; the decoded copy is let go once it is on the card. The tile's
//! normal-atlas picture (16 x 16) is made at once.

use super::size::{self, Verdict};
use super::Pic;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// An HD picture of a mod's tile, as the file.
#[derive(Clone, Debug)]
pub struct Source {
    pub side: usize,
    pub bytes: Arc<Vec<u8>>,
}

impl Source {
    /// The picture as `side` x `side` RGBA, if the file still reads that way.
    pub fn decode(&self) -> Option<Vec<u8>> {
        let img = crate::image::decode(&self.bytes)?;
        if img.w == self.side && img.h == self.side {
            Some(img.rgba)
        } else {
            None
        }
    }
}

/// What reading a mod's picture gave.
pub struct Loaded {
    /// The tile's 16 x 16 picture for the normal atlas.
    pub px16: Vec<u8>,
    /// The HD picture, if the tile is one.
    pub hd: Option<Source>,
    /// Something to tell the mod's author (a picture that had to be fitted).
    pub note: Option<String>,
}

/// Read the picture of a `texture <id> file ...` line. Strict: it is what a
/// server may send. The declared size is looked at before anything is
/// decoded, a picture over 512 on a side is refused, and so is an HD
/// picture of over 2 MiB.
pub fn load(bytes: &[u8]) -> Result<Loaded, String> {
    if let Some((w, h)) = size::peek_size(bytes) {
        match size::classify(w, h) {
            Verdict::TooBig | Verdict::Empty => return Err(alloc::format!("{}", size::complaint(w, h))),
            Verdict::Tile(s) if s > 16 && bytes.len() > size::MAX_PICTURE_BYTES => {
                return Err(alloc::format!("is {} bytes: a picture of a tile may be {} at most", bytes.len(), size::MAX_PICTURE_BYTES));
            }
            _ => {}
        }
    }
    let Some(img) = crate::image::decode(bytes) else {
        return Err(String::from("cannot be read as a picture (PNG, JPEG, GIF, BMP or TGA)"));
    };
    match size::classify(img.w, img.h) {
        Verdict::Tile(16) => Ok(Loaded { px16: img.rgba, hd: None, note: None }),
        Verdict::Tile(s) => {
            if bytes.len() > size::MAX_PICTURE_BYTES {
                return Err(alloc::format!("is {} bytes: a picture of a tile may be {} at most", bytes.len(), size::MAX_PICTURE_BYTES));
            }
            super::note_seen();
            let px16 = super::scale::box_down(&img.rgba, s, 16);
            Ok(Loaded { px16, hd: Some(Source { side: s, bytes: Arc::new(bytes.to_vec()) }), note: None })
        }
        Verdict::Misfit => Ok(Loaded { px16: crate::image::fit(&img, 16), hd: None, note: Some(size::complaint(img.w, img.h)) }),
        Verdict::TooBig | Verdict::Empty => Err(size::complaint(img.w, img.h)),
    }
}

/// Read a picture of a pack or of the textures folder, for the tile `id`. Local
/// files are trusted more than a server's: a picture that is not a tile is
/// fitted to 16 x 16 (with a note) rather than refused. With `probe` only the
/// size is found out (from the header where it can be) and `px` is left empty.
/// With `strip` a tall strip of square frames (an animation) is its first frame.
pub fn read_picture(bytes: &[u8], probe: bool, strip: bool) -> Option<(Pic, Option<String>)> {
    let frame = |w: usize, h: usize| if strip && h > w && w > 0 && h % w == 0 { (w, w) } else { (w, h) };
    if probe {
        if let Some((w, h)) = size::peek_size(bytes) {
            let (w, h) = frame(w, h);
            return match size::classify(w, h) {
                Verdict::Tile(s) if s == 16 || bytes.len() <= size::MAX_PICTURE_BYTES => Some((Pic { side: s, px: Vec::new() }, None)),
                Verdict::Empty => None,
                _ => Some((Pic { side: 16, px: Vec::new() }, None)),
            };
        }
    }
    let mut img = crate::image::decode(bytes)?;
    let (w, h) = frame(img.w, img.h);
    if (w, h) != (img.w, img.h) {
        img.rgba.truncate(w * h * 4);
        img.h = h;
    }
    let pic = match size::classify(img.w, img.h) {
        Verdict::Tile(16) => Pic { side: 16, px: if probe { Vec::new() } else { img.rgba } },
        Verdict::Tile(s) if bytes.len() <= size::MAX_PICTURE_BYTES => {
            super::note_seen();
            Pic { side: s, px: if probe { Vec::new() } else { img.rgba } }
        }
        _ => {
            let note = if matches!(size::classify(img.w, img.h), Verdict::Tile(_)) {
                alloc::format!("is {} bytes: a picture of a tile may be {} at most; it is fitted to 16 x 16", bytes.len(), size::MAX_PICTURE_BYTES)
            } else {
                alloc::format!("{}", size::complaint(img.w, img.h))
            };
            return Some((Pic { side: 16, px: if probe { Vec::new() } else { crate::image::fit(&img, 16) } }, Some(note)));
        }
    };
    Some((pic, None))
}

/// What the HD sources of a registry's tiles cost in memory (the files, kept to build the atlas again).
pub fn registry_bytes(tiles: &[crate::block::TileDef]) -> usize {
    tiles.iter().filter_map(|t| t.hd.as_ref()).map(|s| s.bytes.len()).sum()
}

/// A line of text for the content checksum of a server and its clients: each
/// HD tile and its side, so that two ends with the same names but pictures of
/// other sizes do not count as the same. Empty when no tile is HD (the
/// checksum is then what it always was, and older clients still agree).
pub fn checksum_text(tiles: &[crate::block::TileDef]) -> String {
    let mut s = String::new();
    for t in tiles {
        if let Some(h) = &t.hd {
            if s.is_empty() {
                s.push_str("\nhd");
            }
            s.push_str(&alloc::format!(" {}:{}", t.id, h.side));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(side: usize) -> Vec<u8> {
        let px: Vec<u8> = (0..side * side).flat_map(|i| [(i % 251) as u8, (i / 7 % 251) as u8, 90, 255]).collect();
        crate::png::encode(side, side, &px)
    }

    #[test]
    fn a_16_picture_is_a_plain_tile() {
        let l = load(&png(16)).ok().unwrap();
        assert!(l.hd.is_none() && l.note.is_none());
        assert_eq!(l.px16.len(), 16 * 16 * 4);
    }

    #[test]
    fn hd_pictures_keep_the_file_and_give_a_16_version() {
        for side in [32usize, 64, 48, 128, 512] {
            let f = png(side);
            let l = load(&f).ok().unwrap_or_else(|| panic!("{side}"));
            let hd = l.hd.expect("hd");
            assert_eq!(hd.side, side);
            assert_eq!(l.px16.len(), 16 * 16 * 4);
            assert_eq!(hd.decode().unwrap().len(), side * side * 4);
            assert_eq!(hd.bytes.len(), f.len());
        }
    }

    #[test]
    fn odd_shapes_are_fitted_with_a_note_and_big_ones_refused() {
        let l = load(&png(24)).ok().unwrap();
        assert!(l.hd.is_none());
        assert!(l.note.unwrap().contains("24 x 24"));
        assert_eq!(l.px16.len(), 16 * 16 * 4);
        let rect = crate::png::encode(32, 16, &alloc::vec![200u8; 32 * 16 * 4]);
        assert!(load(&rect).ok().unwrap().note.is_some());
        // 528 x 528 is over 512: refused before it is decoded (the header says so).
        let big = crate::png::encode(528, 528, &alloc::vec![9u8; 528 * 528 * 4]);
        let e = load(&big).err().unwrap();
        assert!(e.contains("512"), "{e}");
        // Not a picture at all.
        assert!(load(b"just some text that is not a picture").is_err());
    }

    #[test]
    fn a_hostile_header_is_refused_without_decoding() {
        // A PNG header that promises 60000 x 60000 and nothing more.
        let mut png = alloc::vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&60000u32.to_be_bytes());
        png.extend_from_slice(&60000u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        let e = load(&png).err().unwrap();
        assert!(e.contains("512"), "{e}");
    }

    #[test]
    fn local_pictures_are_read_as_tiles_or_fitted() {
        let f = png(64);
        let (p, note) = read_picture(&f, false, false).unwrap();
        assert_eq!((p.side, p.px.len(), note.is_none()), (64, 64 * 64 * 4, true));
        // Probing finds the size and reads no pixels.
        let (p, _) = read_picture(&f, true, false).unwrap();
        assert_eq!((p.side, p.px.len()), (64, 0));
        // A tall strip is its first frame.
        let strip = crate::png::encode(32, 96, &alloc::vec![77u8; 32 * 96 * 4]);
        let (p, _) = read_picture(&strip, false, true).unwrap();
        assert_eq!((p.side, p.px.len()), (32, 32 * 32 * 4));
        let (p, _) = read_picture(&strip, true, true).unwrap();
        assert_eq!(p.side, 32);
        // Not a tile: 16 x 16 and a note (but only when the pixels are read for real).
        let odd = png(20);
        let (p, note) = read_picture(&odd, false, false).unwrap();
        assert_eq!((p.side, p.px.len()), (16, 16 * 16 * 4));
        assert!(note.is_some());
        let (p, _) = read_picture(&odd, true, false).unwrap();
        assert_eq!((p.side, p.px.len()), (16, 0));
        assert!(read_picture(b"nonsense nonsense nonsense", false, false).is_none());
    }

    #[test]
    fn the_checksum_text_is_empty_without_hd_and_names_sizes_with_it() {
        use crate::block::{TileDef, TileSrc};
        let plain = TileDef { id: alloc::string::String::from("a"), src: TileSrc::Flat { rgb: [1, 2, 3], noise: 0 }, hd: None };
        assert_eq!(checksum_text(&[plain.clone()]), "");
        assert_eq!(registry_bytes(&[plain.clone()]), 0);
        let hd = TileDef { id: alloc::string::String::from("b"), src: TileSrc::Pixels(alloc::vec![0; 1024]), hd: Some(Source { side: 128, bytes: Arc::new(alloc::vec![1, 2, 3]) }) };
        let hd2 = TileDef { hd: Some(Source { side: 256, bytes: Arc::new(alloc::vec![1]) }), ..hd.clone() };
        let a = checksum_text(&[plain.clone(), hd.clone()]);
        assert!(a.contains("b:128"), "{a}");
        assert_ne!(a, checksum_text(&[plain, hd2.clone()]), "another size is another checksum");
        assert_eq!(registry_bytes(&[hd, hd2]), 4);
    }
}
