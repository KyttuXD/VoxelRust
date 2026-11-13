//! Reading pictures: PNG, JPEG (baseline), GIF (the first picture), BMP and
//! TGA. Everything comes out as 8-bit RGBA, top row first. Used for textures
//! the player or a pack supplies, which are fitted to 16 x 16.

use alloc::vec::Vec;

/// Largest picture accepted, in pixels on a side, and the biggest file.
pub const MAX_SIDE: usize = 4096;
pub const MAX_BYTES: usize = 24 << 20;

/// Read a picture of any of the kinds above, told apart by how it starts.
pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 18 || d.len() > MAX_BYTES {
        return None;
    }
    let img = if d.starts_with(&[0x89, b'P', b'N', b'G']) {
        png::decode(d)
    } else if d.starts_with(&[0xFF, 0xD8]) {
        jpeg(d)
    } else if d.starts_with(b"GIF8") {
        gif(d)
    } else if d.starts_with(b"BM") {
        bmp(d)
    } else {
        tga(d)
    }?;
    if img.w == 0 || img.h == 0 || img.w > MAX_SIDE || img.h > MAX_SIDE || img.rgba.len() != img.w * img.h * 4 {
        return None;
    }
    Some(img)
}

/// Fit a picture into `side` x `side` pixels: smaller ones are enlarged
/// (each pixel stays a hard square), bigger ones averaged down.
pub fn fit(img: &Image, side: usize) -> Vec<u8> {
    let mut out = alloc::vec![0u8; side * side * 4];
    for y in 0..side {
        // The source rows and columns this pixel covers.
        let (y0, y1) = (y * img.h / side, ((y + 1) * img.h / side).max(y * img.h / side + 1).min(img.h));
        for x in 0..side {
            let (x0, x1) = (x * img.w / side, ((x + 1) * img.w / side).max(x * img.w / side + 1).min(img.w));
            let (mut sum, mut a_sum, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &img.rgba[(sy * img.w + sx) * 4..][..4];
                    // Colours weigh by their alpha, so a see-through pixel does not darken its neighbours.
                    sum = [sum[0] + p[0] as u32 * p[3] as u32, sum[1] + p[1] as u32 * p[3] as u32, sum[2] + p[2] as u32 * p[3] as u32];
                    a_sum += p[3] as u32;
                    n += 1;
                }
            }
            let o = (y * side + x) * 4;
            if a_sum > 0 {
                out[o] = (sum[0] / a_sum) as u8;
                out[o + 1] = (sum[1] / a_sum) as u8;
                out[o + 2] = (sum[2] / a_sum) as u8;
            }
            out[o + 3] = if n > 0 { (a_sum / n) as u8 } else { 0 };
        }
    }
    out
}


mod bmp;
mod gif;
mod jpeg;
pub mod png;
mod tga;

use bmp::bmp;
use gif::gif;
use jpeg::jpeg;
use tga::tga;
use png::Image;

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &Image, b: &Image) -> bool {
        a.w == b.w && a.h == b.h && a.rgba == b.rgba
    }

    #[test]
    fn lossless_formats_read_back_the_same_picture() {
        let want = decode(include_bytes!("testdata/pic.png")).expect("png");
        assert_eq!((want.w, want.h), (24, 16));
        for (name, file) in [
            ("bmp24", &include_bytes!("testdata/pic24.bmp")[..]),
            ("bmp8", &include_bytes!("testdata/pic8.bmp")[..]),
            ("tga", &include_bytes!("testdata/pic.tga")[..]),
            ("tga rle", &include_bytes!("testdata/pic_rle.tga")[..]),
        ] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert!(same(&got, &want), "{name} differs");
        }
        // GIF has a 256-colour palette: the picture was made with few enough colours.
        let g = decode(include_bytes!("testdata/pic.gif")).expect("gif");
        assert_eq!((g.w, g.h), (24, 16));
    }

    #[test]
    fn jpeg_is_close_to_the_original() {
        let want = decode(include_bytes!("testdata/pic.png")).unwrap();
        for (name, file) in [("jpeg 4:4:4", &include_bytes!("testdata/pic444.jpg")[..]), ("jpeg 4:2:0", &include_bytes!("testdata/pic420.jpg")[..]), ("gray", &include_bytes!("testdata/gray.jpg")[..])] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert_eq!((got.w, got.h), (want.w, want.h), "{name}");
            let mut err = 0.0f64;
            for i in 0..want.w * want.h {
                for c in 0..3 {
                    let a = got.rgba[i * 4 + c] as f64;
                    let b = if name == "gray" { (want.rgba[i * 4] as f64 * 0.299 + want.rgba[i * 4 + 1] as f64 * 0.587 + want.rgba[i * 4 + 2] as f64 * 0.114) } else { want.rgba[i * 4 + c] as f64 };
                    err += (a - b) * (a - b);
                }
            }
            let rms = (err / (want.w * want.h * 3) as f64).sqrt();
            assert!(rms < 24.0, "{name}: rms {rms}");
        }
    }

    #[test]
    fn pictures_are_fitted_to_a_tile() {
        let big = decode(include_bytes!("testdata/pic.png")).unwrap();
        let t = fit(&big, 16);
        assert_eq!(t.len(), 16 * 16 * 4);
        let small = Image { w: 2, h: 2, rgba: alloc::vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0] };
        let t = fit(&small, 16);
        assert_eq!(&t[0..4], &[255, 0, 0, 255]);
        assert_eq!(&t[(15 * 16) * 4..(15 * 16) * 4 + 4], &[0, 0, 255, 255]);
        assert_eq!(t[(15 * 16 + 15) * 4 + 3], 0, "a see-through corner stays so");
    }

    #[test]
    fn things_that_are_not_pictures_are_refused() {
        assert!(decode(b"this is plainly not a picture at all").is_none());
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xC2, 0, 8, 8, 0, 8, 0, 8, 1, 0, 0, 0, 0, 0, 0]).is_none(), "progressive jpeg");
    }
}
