//! Fuzz-style tests of the PNG encoder, the deflate compressor and the
//! decoder (see `png.rs`, `png_encode.rs`, `deflate.rs`): pseudo-random
//! pictures of odd sizes and palettes, every row filter, and hostile files.
//! No picture or stream may make any of them panic or ask for a huge
//! allocation.

use super::deflate;
use super::png::{self, decode, inflate, inflate_raw, Image, SIGNATURE};
use alloc::vec;
use alloc::vec::Vec;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

/// A picture of `w` x `h` pixels using exactly `colours` different colours
/// (every one of them appears), with see-through ones if `alpha`.
fn picture(rng: &mut Rng, w: usize, h: usize, colours: usize, alpha: bool) -> Vec<u8> {
    let palette: Vec<[u8; 4]> = (0..colours)
        .map(|i| {
            // Different by construction: the index is in the colour.
            let a = if alpha && i % 3 == 1 { (rng.below(255)) as u8 } else { 255 };
            [(i & 255) as u8, (i >> 8) as u8 ^ rng.below(1) as u8, (i * 7) as u8, a]
        })
        .collect();
    let mut px = Vec::with_capacity(w * h * 4);
    for i in 0..w * h {
        // The first pixels walk the palette so that each colour is used.
        let k = if i < colours { i } else { rng.below(colours as u32) as usize };
        px.extend_from_slice(&palette[k]);
    }
    px
}

fn round_trip(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let file = png::encode(w, h, px);
    let img = decode(&file).unwrap_or_else(|| panic!("{}x{} was not read back", w, h));
    assert_eq!((img.w, img.h), (w, h));
    assert!(img.rgba == px, "{}x{}: pixels differ", w, h);
    file
}

#[test]
fn odd_sizes_and_palettes_round_trip() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let sizes = [(1usize, 1usize), (1, 7), (7, 1), (3, 5), (17, 13), (31, 29), (64, 3), (5, 70), (33, 33)];
    for &(w, h) in &sizes {
        for colours in [1usize, 2, 255, 256, 257] {
            // (A picture cannot show more colours than it has pixels.)
            if colours > w * h {
                continue;
            }
            for alpha in [false, true] {
                let px = picture(&mut rng, w, h, colours, alpha);
                let file = round_trip(w, h, &px);
                let kind = file[25];
                if colours <= 256 {
                    assert_eq!(kind, 3, "{}x{} with {} colours is indexed", w, h, colours);
                } else if alpha && px.chunks_exact(4).any(|p| p[3] != 255) {
                    assert_eq!(kind, 6);
                } else {
                    assert_eq!(kind, 2);
                }
            }
        }
    }
}

#[test]
fn random_noise_pictures_round_trip() {
    let mut rng = Rng(77);
    for round in 0..24 {
        let (w, h) = (1 + rng.below(40) as usize, 1 + rng.below(40) as usize);
        let px: Vec<u8> = (0..w * h * 4).map(|i| if round % 2 == 0 && i % 4 == 3 { 255 } else { rng.next() as u8 }).collect();
        round_trip(w, h, &px);
    }
}

#[test]
fn empty_and_short_input_is_padded_not_a_panic() {
    // Zero sizes and too little data: the encoder writes something, never panics.
    for (w, h) in [(0usize, 0usize), (0, 5), (5, 0)] {
        let f = png::encode(w, h, &[]);
        assert_eq!(&f[..8], &SIGNATURE);
        // (The decoder refuses an empty picture.)
        assert!(decode(&f).is_none());
    }
    let f = png::encode(4, 4, &[9, 9, 9, 255]);
    let img = decode(&f).unwrap();
    assert_eq!(&img.rgba[..4], &[9, 9, 9, 255]);
    assert!(img.rgba[4..].iter().all(|&b| b == 0));
}

/// The row filters, written again here (not shared with the encoder).
fn filter_with(kind: u8, cur: &[u8], prev: &[u8], bpp: usize) -> Vec<u8> {
    let paeth = |a: u8, b: u8, c: u8| {
        let (ia, ib, ic) = (a as i32, b as i32, c as i32);
        let p = ia + ib - ic;
        let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    };
    (0..cur.len())
        .map(|i| {
            let left = if i >= bpp { cur[i - bpp] } else { 0 };
            let up = prev[i];
            let diag = if i >= bpp { prev[i - bpp] } else { 0 };
            let guess = match kind {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((left as u16 + up as u16) / 2) as u8,
                _ => paeth(left, up, diag),
            };
            cur[i].wrapping_sub(guess)
        })
        .collect()
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    png::push_chunk(out, kind, body);
}

/// A PNG made by hand: RGBA 8-bit rows, row `y` filtered with `kinds[y % kinds.len()]`.
fn hand_made(w: usize, h: usize, px: &[u8], kinds: &[u8]) -> Vec<u8> {
    let stride = w * 4;
    let mut raw = Vec::new();
    for y in 0..h {
        let cur = &px[y * stride..(y + 1) * stride];
        let zero = vec![0u8; stride];
        let prev = if y == 0 { &zero[..] } else { &px[(y - 1) * stride..y * stride] };
        let k = kinds[y % kinds.len()];
        raw.push(k);
        raw.extend_from_slice(&filter_with(k, cur, prev, 4));
    }
    let mut out = SIGNATURE.to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &deflate::zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[test]
fn the_decoder_undoes_every_filter_type() {
    let mut rng = Rng(5);
    for &(w, h) in &[(1usize, 1usize), (2, 9), (13, 11), (40, 6)] {
        let px: Vec<u8> = (0..w * h * 4).map(|_| rng.next() as u8).collect();
        for kind in 0..5u8 {
            let file = hand_made(w, h, &px, &[kind]);
            let img = decode(&file).unwrap_or_else(|| panic!("filter {} on {}x{}", kind, w, h));
            assert!(img.rgba == px, "filter {} on {}x{}", kind, w, h);
        }
        // All five in turn, row by row.
        let file = hand_made(w, h, &px, &[0, 1, 2, 3, 4]);
        assert!(decode(&file).unwrap().rgba == px);
    }
    // A filter byte that does not exist is refused.
    let mut raw = vec![0u8];
    raw.extend_from_slice(&[3u8; 16]);
    let mut bad = SIGNATURE.to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&4u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut bad, b"IHDR", &ihdr);
    raw[0] = 9;
    chunk(&mut bad, b"IDAT", &deflate::zlib(&raw));
    chunk(&mut bad, b"IEND", &[]);
    assert!(decode(&bad).is_none());
}

#[test]
fn every_prefix_and_every_flipped_byte_of_a_file_is_survived() {
    let mut rng = Rng(11);
    let px: Vec<u8> = (0..20 * 20 * 4).map(|i| if i % 4 == 3 { 255 } else { (rng.next() % 40) as u8 }).collect();
    let file = png::encode(20, 20, &px);
    for n in 0..file.len() {
        assert!(decode(&file[..n]).is_none(), "a file cut at {} was taken", n);
    }
    // Flipping any one byte either breaks a checksum or still decodes to something: no panic.
    for i in 0..file.len() {
        let mut f = file.clone();
        f[i] ^= 0x5A;
        let _ = decode(&f);
    }
}

#[test]
fn random_bytes_and_mutated_streams_never_panic_the_inflater() {
    let mut rng = Rng(99);
    for _ in 0..300 {
        let n = rng.below(200) as usize;
        let junk: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        let _ = inflate(&junk);
        let _ = inflate_raw(&junk, 1 << 16);
        // With a header that passes the first check.
        let mut z = vec![0x78, 0x9C];
        z.extend_from_slice(&junk);
        let _ = inflate(&z);
    }
    let data: Vec<u8> = (0..5000u32).map(|i| (i % 13 + i / 97) as u8).collect();
    let z = deflate::zlib(&data);
    assert_eq!(inflate(&z).as_deref(), Some(&data[..]));
    for i in 0..z.len() {
        for bit in [0x01u8, 0x10, 0x80] {
            let mut m = z.clone();
            m[i] ^= bit;
            if let Some(out) = inflate(&m) {
                // The checksum can only agree with the right bytes.
                assert_eq!(out, data);
            }
        }
    }
    for n in 0..z.len() {
        assert!(inflate(&z[..n]).is_none(), "cut at {}", n);
    }
}

/// A dynamic block header with the code length codes given: HLIT 257, HDIST 1, then `hclen` entries.
fn dynamic_header(cl_lengths: &[u8]) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut put = |v: u32, n: u32| {
        for k in 0..n {
            bits.push(v >> k & 1 == 1);
        }
    };
    put(1, 1); // last block
    put(2, 2); // dynamic
    put(0, 5); // 257 literal codes
    put(0, 5); // 1 distance code
    put(cl_lengths.len() as u32 - 4, 4);
    for &l in cl_lengths {
        put(l as u32, 3);
    }
    let mut out = vec![0x78, 0x9C];
    for chunk in bits.chunks(8) {
        let mut b = 0u8;
        for (k, &x) in chunk.iter().enumerate() {
            b |= (x as u8) << k;
        }
        out.push(b);
    }
    // Some more bytes for it to read.
    out.extend_from_slice(&[0xFF; 64]);
    out
}

#[test]
fn bad_huffman_tables_are_refused() {
    // Seven codes of length 1: more than a one-bit code can hold.
    assert!(inflate(&dynamic_header(&[1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])).is_none());
    // All code length codes absent.
    assert!(inflate(&dynamic_header(&[0; 19])).is_none());
    // One code only (incomplete): reading symbols from it fails or ends without an end-of-block code.
    assert!(inflate(&dynamic_header(&[1, 0, 0, 0])).is_none());
    // Reserved block type 3.
    assert!(inflate(&[0x78, 0x9C, 0x07, 0, 0, 0, 0, 0, 0]).is_none());
    // A stored block whose length check bits do not match.
    assert!(inflate(&[0x78, 0x9C, 0x01, 0x05, 0x00, 0x00, 0x00, 1, 2, 3, 4, 5, 0, 0, 0, 0]).is_none());
}

#[test]
fn output_past_the_limit_is_refused_early() {
    // 300 kB of zeros is a few hundred bytes of stream: the limit stops it.
    let z = deflate::zlib(&vec![0u8; 300_000]);
    assert!(z.len() < 1000, "{}", z.len());
    let raw = &z[2..z.len() - 4];
    assert!(inflate_raw(raw, 1000).is_none());
    assert_eq!(inflate_raw(raw, 300_000).map(|v| v.len()), Some(300_000));
    assert!(inflate_raw(raw, 299_999).is_none());
}

#[test]
fn a_header_that_claims_a_huge_picture_costs_nothing() {
    for (w, h, body_ok) in [(4096u32, 4096u32, true), (4097, 1, false), (1, 4097, false), (0, 5, false), (u32::MAX, u32::MAX, false)] {
        let mut out = SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        // A tiny stream that holds a few bytes only.
        chunk(&mut out, b"IDAT", &deflate::zlib(&[0u8; 40]));
        chunk(&mut out, b"IEND", &[]);
        assert!(decode(&out).is_none(), "{}x{} with 40 bytes of data (ok={})", w, h, body_ok);
    }
    // The image type is plain data: a picture of the wrong length is not trusted by the encoder.
    let img = Image { w: 2, h: 2, rgba: vec![1, 2, 3, 4] };
    let _ = png::encode(img.w, img.h, &img.rgba);
}

#[test]
fn encoding_flat_and_stripy_pictures_stays_correct_around_the_window_size() {
    // Rows that repeat at exactly the window distance and a flat area bigger than a block.
    let (w, h) = (256usize, 256usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let v = if y < 100 { 40 } else { ((x * 5 + (y % 2) * 128) % 256) as u8 };
            let o = (y * w + x) * 4;
            px[o..o + 4].copy_from_slice(&[v, v / 2, 200 - v / 2, 255]);
        }
    }
    round_trip(w, h, &px);
}
