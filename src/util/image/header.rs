//! A PNG's header, read without decoding anything: what a skin says it is,
//! checked before the (much bigger) picture is unpacked. A header can claim
//! 4096 x 4096 in a few bytes, and unpacking that costs tens of megabytes.

use super::png::{crc32, SIGNATURE};

/// What the first chunk of a PNG says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub w: u32,
    pub h: u32,
    pub depth: u8,
    pub colour_type: u8,
    pub interlaced: bool,
}

/// The header of `png`: the signature must be right and `IHDR` the first
/// chunk, with a good checksum. Nothing else is looked at.
pub fn header(png: &[u8]) -> Option<Header> {
    if png.len() < 33 || png[..8] != SIGNATURE || &png[12..16] != b"IHDR" || png[8..12] != 13u32.to_be_bytes() {
        return None;
    }
    let crc = u32::from_be_bytes([png[29], png[30], png[31], png[32]]);
    if crc != crc32(&png[12..29]) {
        return None;
    }
    Some(Header {
        w: u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        h: u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
        depth: png[24],
        colour_type: png[25],
        interlaced: png[28] != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_of_an_encoded_picture_is_read() {
        let f = crate::png::encode(7, 5, &[200u8; 7 * 5 * 4]);
        let h = header(&f).unwrap();
        assert_eq!((h.w, h.h, h.depth, h.interlaced), (7, 5, 8, false));
    }

    #[test]
    fn bad_headers_are_refused() {
        let f = crate::png::encode(7, 5, &[200u8; 7 * 5 * 4]);
        assert!(header(&f[..32]).is_none());
        assert!(header(b"this is not a picture, and it is long enough").is_none());
        let mut bad = f.clone();
        bad[17] ^= 1; // the width changed: the checksum no longer fits
        assert!(header(&bad).is_none());
        let mut bad = f.clone();
        bad[11] = 12; // a header of the wrong length
        assert!(header(&bad).is_none());
    }
}
