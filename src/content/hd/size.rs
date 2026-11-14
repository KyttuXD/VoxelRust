//! Which pictures may be a tile, and how big they are before they are read.

/// The least and the most pixels on a side of a tile's picture.
pub const MIN_SIDE: usize = 16;
pub const MAX_SIDE: usize = 512;
/// The biggest file of an HD picture (PNG and the like), in bytes. A server's
/// mod may carry many; each is bounded.
pub const MAX_PICTURE_BYTES: usize = 2 << 20;
/// The sizes the TEXTURE SIZE setting offers.
pub const CAPS: [usize; 6] = [16, 32, 64, 128, 256, 512];

/// What a picture of a given size is good for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// A tile of this many pixels on a side.
    Tile(usize),
    /// Not a tile shape (not square, or not a multiple of 16): fitted to 16 x 16.
    Misfit,
    /// Bigger than 512 on a side: refused.
    TooBig,
    /// Empty or smaller than 16 in some way that cannot be a tile (0 wide).
    Empty,
}

/// Whether `w` x `h` is a square of a multiple of 16, from 16 to 512.
pub fn valid_side(w: usize, h: usize) -> bool {
    w == h && w >= MIN_SIDE && w <= MAX_SIDE && w % 16 == 0
}

pub fn classify(w: usize, h: usize) -> Verdict {
    if w == 0 || h == 0 {
        Verdict::Empty
    } else if w > MAX_SIDE || h > MAX_SIDE {
        Verdict::TooBig
    } else if valid_side(w, h) {
        Verdict::Tile(w)
    } else {
        Verdict::Misfit
    }
}

/// What to tell the player about a picture that is not a tile.
pub fn complaint(w: usize, h: usize) -> alloc::string::String {
    match classify(w, h) {
        Verdict::TooBig => alloc::format!("is {} x {}: tiles are {} x {} at most", w, h, MAX_SIDE, MAX_SIDE),
        Verdict::Misfit => alloc::format!("is {} x {}: a tile is a square, a multiple of 16 pixels on a side (16 to {}); it is fitted to 16 x 16", w, h, MAX_SIDE),
        _ => alloc::string::String::from("is empty"),
    }
}

/// The side an HD picture of `natural` pixels is kept at under the cap `cap`.
pub fn effective_side(natural: usize, cap: usize) -> usize {
    if natural <= cap {
        natural
    } else {
        cap
    }
}

/// The cap for a setting value: the step at or under it (16 to 512).
pub fn cap_for(setting: i32) -> usize {
    let v = setting.clamp(16, 512) as usize;
    CAPS.iter().rev().copied().find(|c| *c <= v).unwrap_or(16)
}

fn be16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as usize)
}
fn le16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]) as usize)
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}
fn le32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

/// The size a picture file declares in its header (PNG, GIF, BMP, JPEG, TGA),
/// without reading the picture. The receiver of a server's mod looks at this
/// before it decodes anything. `None` if the header is not one of those or is cut short.
pub fn peek_size(b: &[u8]) -> Option<(usize, usize)> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        if b.get(12..16)? != b"IHDR" {
            return None;
        }
        return Some((be32(b, 16)? as usize, be32(b, 20)? as usize));
    }
    if b.starts_with(b"GIF8") {
        return Some((le16(b, 6)?, le16(b, 8)?));
    }
    if b.starts_with(b"BM") {
        let (w, h) = (le32(b, 18)?, le32(b, 22)?);
        return Some((w.unsigned_abs() as usize, h.unsigned_abs() as usize));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        // Walk the segments to the first frame header.
        for _ in 0..64 {
            if *b.get(i)? != 0xFF {
                return None;
            }
            let m = *b.get(i + 1)?;
            if m == 0xFF {
                i += 1;
                continue;
            }
            if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                return Some((be16(b, i + 7)?, be16(b, i + 5)?));
            }
            if m == 0xD8 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
                i += 2;
                continue;
            }
            i += 2 + be16(b, i + 2)?;
        }
        return None;
    }
    // TGA has no signature at the start; its header is 18 bytes with the size at 12.
    if b.len() >= 18 && matches!(b[2], 1 | 2 | 3 | 9 | 10 | 11) {
        return Some((le16(b, 12)?, le16(b, 14)?));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_squares_of_multiples_of_16_up_to_512() {
        for s in [16usize, 32, 48, 64, 80, 96, 128, 256, 400, 512] {
            assert!(valid_side(s, s), "{s}");
            assert_eq!(classify(s, s), Verdict::Tile(s));
        }
        for (w, h) in [(24, 24), (17, 17), (15, 15), (8, 8), (100, 100), (511, 511), (32, 16), (16, 32), (64, 128)] {
            assert!(!valid_side(w, h), "{w}x{h}");
            assert_eq!(classify(w, h), Verdict::Misfit, "{w}x{h}");
        }
        for (w, h) in [(528, 528), (1024, 1024), (512, 1024), (4096, 16)] {
            assert_eq!(classify(w, h), Verdict::TooBig, "{w}x{h}");
        }
        assert_eq!(classify(0, 16), Verdict::Empty);
        assert_eq!(classify(16, 0), Verdict::Empty);
    }

    #[test]
    fn complaints_say_the_size_and_the_rule() {
        let c = complaint(24, 24);
        assert!(c.contains("24 x 24") && c.contains("multiple of 16"), "{c}");
        assert!(complaint(1024, 1024).contains("512"));
    }

    #[test]
    fn the_cap_only_shrinks() {
        assert_eq!(effective_side(512, 512), 512);
        assert_eq!(effective_side(512, 128), 128);
        assert_eq!(effective_side(48, 128), 48);
        assert_eq!(effective_side(64, 32), 32);
        assert_eq!(effective_side(16, 512), 16);
    }

    #[test]
    fn settings_become_steps() {
        assert_eq!(cap_for(512), 512);
        assert_eq!(cap_for(9999), 512);
        assert_eq!(cap_for(300), 256);
        assert_eq!(cap_for(100), 64);
        assert_eq!(cap_for(33), 32);
        assert_eq!(cap_for(16), 16);
        assert_eq!(cap_for(-5), 16);
    }

    #[test]
    fn the_declared_size_is_read_from_each_header() {
        // PNG: signature, length, IHDR, width, height.
        let mut png = alloc::vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&512u32.to_be_bytes());
        png.extend_from_slice(&256u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(peek_size(&png), Some((512, 256)));
        // A real picture from the tests.
        let real = crate::png::encode(16, 16, &alloc::vec![7u8; 16 * 16 * 4]);
        assert_eq!(peek_size(&real), Some((16, 16)));
        // GIF.
        let mut gif = alloc::vec::Vec::from(&b"GIF89a"[..]);
        gif.extend_from_slice(&[64, 0, 32, 0, 0, 0, 0]);
        assert_eq!(peek_size(&gif), Some((64, 32)));
        // BMP (the height may be negative: top down).
        let mut bmp = alloc::vec![0u8; 30];
        bmp[0] = b'B';
        bmp[1] = b'M';
        bmp[18..22].copy_from_slice(&128i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-128i32).to_le_bytes());
        assert_eq!(peek_size(&bmp), Some((128, 128)));
        // JPEG: SOI, a short APP0, then SOF0 (height, then width).
        let jpg = alloc::vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 1, 0, 0, 255, 3, 1, 0x22, 0];
        assert_eq!(peek_size(&jpg), Some((255, 256)));
        // TGA.
        let mut tga = alloc::vec![0u8; 18];
        tga[2] = 2;
        tga[12] = 48;
        tga[14] = 48;
        assert_eq!(peek_size(&tga), Some((48, 48)));
        // Not pictures, and cut short.
        assert_eq!(peek_size(b"hello, this is text"), None);
        assert_eq!(peek_size(&png[..18]), None);
        assert_eq!(peek_size(&[0xFF, 0xD8, 0xFF]), None);
        assert_eq!(peek_size(&[]), None);
    }
}
