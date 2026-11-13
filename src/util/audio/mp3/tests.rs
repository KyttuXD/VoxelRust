//! Tests of the MP3 decoder. The files in `testdata/mp3_*` were made with LAME from a tone with
//! noise bursts (so there are short blocks and window switching), and each `.wav` next to one is
//! what ffmpeg decodes it to. Decoders differ a little and encoders add delay, so the
//! comparisons look for the best alignment and allow a generous error.

use super::super::wav::wav;
use super::super::{decode, RATE};
use super::bits::BitReader;
use super::header::{self, Granule, Header};
use super::huffman::{walk, Huffman};
use super::requant::{exp2q, Requant};
use super::scalefac::{self, Scalefacs};
use super::tables::{HUFF, QUAD_A_CODES, QUAD_A_LENS};
use super::{mp3, stereo};
use alloc::vec;
use alloc::vec::Vec;

struct Fixture {
    name: &'static str,
    mp3: &'static [u8],
    wav: &'static [u8],
    rate: u32,
    channels: usize,
}

const FIXTURES: [Fixture; 8] = [
    Fixture { name: "mono 44.1k, ID3v2 + Info", mp3: include_bytes!("../testdata/mp3_m44.mp3"), wav: include_bytes!("../testdata/mp3_m44.wav"), rate: 44100, channels: 1 },
    Fixture { name: "mono 22.05k, bare", mp3: include_bytes!("../testdata/mp3_m22.mp3"), wav: include_bytes!("../testdata/mp3_m22.wav"), rate: 22050, channels: 1 },
    Fixture { name: "joint 44.1k, Info", mp3: include_bytes!("../testdata/mp3_s44.mp3"), wav: include_bytes!("../testdata/mp3_s44.wav"), rate: 44100, channels: 2 },
    Fixture { name: "joint 22.05k, ID3v2", mp3: include_bytes!("../testdata/mp3_s22.mp3"), wav: include_bytes!("../testdata/mp3_s22.wav"), rate: 22050, channels: 2 },
    Fixture { name: "mono 11.025k (MPEG 2.5)", mp3: include_bytes!("../testdata/mp3_m11.mp3"), wav: include_bytes!("../testdata/mp3_m11.wav"), rate: 11025, channels: 1 },
    Fixture { name: "joint 32k", mp3: include_bytes!("../testdata/mp3_s32.mp3"), wav: include_bytes!("../testdata/mp3_s32.wav"), rate: 32000, channels: 2 },
    Fixture { name: "joint 44.1k, VBR", mp3: include_bytes!("../testdata/mp3_v44.mp3"), wav: include_bytes!("../testdata/mp3_v44.wav"), rate: 44100, channels: 2 },
    Fixture { name: "mono 48k, with CRCs", mp3: include_bytes!("../testdata/mp3_m48p.mp3"), wav: include_bytes!("../testdata/mp3_m48p.wav"), rate: 48000, channels: 1 },
];

const FREE_FORMAT: &[u8] = include_bytes!("../testdata/mp3_free.mp3");

/// The RMS difference of `got` and `want` at the best alignment (got shifted by `shift`
/// samples against want, within 2400 frames either way), and that shift. The shift is found on
/// the start of the sound, the error is measured over all of it.
fn best_alignment(got: &[f32], want: &[f32], ch: usize) -> (f64, isize) {
    let error = |shift: isize, len: usize| -> Option<f64> {
        let (mut sum, mut n) = (0.0f64, 0usize);
        for (i, &w) in want.iter().take(len).enumerate() {
            let j = i as isize + shift;
            if j >= 0 && (j as usize) < got.len() {
                let d = (got[j as usize] - w) as f64;
                sum += d * d;
                n += 1;
            }
        }
        // Mostly overlapping, or it proves nothing.
        if n * 10 >= want.len().min(len) * 8 {
            Some((sum / n as f64).sqrt())
        } else {
            None
        }
    };
    let mut best = (f64::MAX, 0isize);
    let mut shift = -2400 * ch as isize;
    while shift <= 2400 * ch as isize {
        if let Some(e) = error(shift, 6000) {
            if e < best.0 {
                best = (e, shift);
            }
        }
        shift += ch as isize;
    }
    (error(best.1, usize::MAX).unwrap_or(f64::MAX), best.1)
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

#[test]
fn every_fixture_decodes_like_ffmpeg() {
    for f in &FIXTURES {
        let got = mp3(f.mp3).unwrap_or_else(|| panic!("{}: not decoded", f.name));
        let want = wav(f.wav).expect("reference wav");
        assert_eq!(got.rate, f.rate, "{}", f.name);
        assert_eq!(got.channels, f.channels, "{}", f.name);
        assert_eq!(want.rate, f.rate, "{}", f.name);
        assert!(got.data.len() % f.channels == 0, "{}", f.name);
        // About as long (the decoders may differ by a frame or two at the ends).
        let (a, b) = (got.data.len() as isize, want.data.len() as isize);
        assert!((a - b).abs() <= 2 * 1152 * f.channels as isize, "{}: {} against {} samples", f.name, a, b);
        let (err, shift) = best_alignment(&got.data, &want.data, f.channels);
        let level = rms(&want.data);
        assert!(level > 0.05, "{}: reference is nearly silent", f.name);
        assert!(err < 0.01, "{}: error {} (signal {}) at shift {}", f.name, err, level, shift);
    }
}

#[test]
fn decode_gives_the_mixers_sound() {
    for f in &FIXTURES {
        let got = decode(f.mp3).unwrap_or_else(|| panic!("{}: decode refused it", f.name));
        let want = decode(f.wav).expect("reference");
        let (a, b) = (got.len() as isize, want.len() as isize);
        assert!((a - b).abs() <= 2 * 1152 * RATE as isize / f.rate as isize + 4, "{}: {} against {}", f.name, a, b);
        let g: Vec<f32> = got.iter().map(|&s| s as f32 / 32768.0).collect();
        let w: Vec<f32> = want.iter().map(|&s| s as f32 / 32768.0).collect();
        let (err, shift) = best_alignment(&g, &w, 1);
        assert!(err < 0.02, "{}: error {} at shift {}", f.name, err, shift);
    }
}

#[test]
fn id3_tags_and_leading_junk_are_skipped() {
    let plain = mp3(FIXTURES[1].mp3).unwrap();
    // A made-up ID3v2 tag (version 4, with a footer) of 40 bytes of "frames".
    let mut tagged = b"ID3\x04\x00\x10\x00\x00\x00\x28".to_vec();
    tagged.extend(vec![b'x'; 40]);
    tagged.extend(vec![0u8; 10]); // the footer
    tagged.extend_from_slice(FIXTURES[1].mp3);
    // And an ID3v1 tag at the end.
    tagged.extend_from_slice(b"TAG");
    tagged.extend(vec![0u8; 125]);
    let got = mp3(&tagged).expect("tagged");
    assert_eq!(got.data, plain.data);

    // Junk before the first frame is skipped by looking for the sync.
    let mut junk = vec![0u8; 300];
    junk.extend_from_slice(FIXTURES[1].mp3);
    assert_eq!(mp3(&junk).expect("junk").data, plain.data);
}

#[test]
fn the_info_frame_is_not_played_and_the_encoder_delay_is_cut() {
    // The first fixture's Info frame says how much silence LAME put in front.
    let f = &FIXTURES[0];
    let got = mp3(f.mp3).unwrap();
    let want = wav(f.wav).unwrap();
    // 0.35 s of tone, give or take the frames: not 1105 samples more.
    assert!((got.data.len() as isize - want.data.len() as isize).abs() < 1152, "{} against {}", got.data.len(), want.data.len());
    let (_, shift) = best_alignment(&got.data, &want.data, 1);
    assert!(shift.abs() <= 8, "shift {}", shift);
}

#[test]
fn free_format_and_other_things_are_refused() {
    assert!(mp3(FREE_FORMAT).is_none());
    assert!(decode(FREE_FORMAT).is_none());
    assert!(mp3(&[]).is_none());
    assert!(mp3(&[0xFF, 0xFB]).is_none());
    assert!(mp3(b"ID3\x03\x00\x00\x00\x00\x00\x05abcde").is_none());
    assert!(mp3(&vec![0xFFu8; 4000]).is_none());
    assert!(mp3(&vec![0u8; 4000]).is_none());
    // Layer II (a header with the layer bits 10) is not layer III.
    let mut l2 = FIXTURES[2].mp3.to_vec();
    let mut at = 0;
    while at + 4 < l2.len() {
        if l2[at] == 0xFF && l2[at + 1] & 0xE6 == 0xE2 {
            l2[at + 1] = (l2[at + 1] & !6) | 4;
        }
        at += 1;
    }
    assert!(mp3(&l2).is_none());
}

#[test]
fn headers_are_read() {
    // MPEG-1, 128 kbit/s, 44.1 kHz, joint stereo, no CRC.
    let h = header::parse(&[0xFF, 0xFB, 0x90, 0x44], 0).expect("header");
    assert_eq!((h.version, h.rate, h.channels, h.mode), (3, 44100, 2, 1));
    assert!(h.frame_len == 417 || h.frame_len == 418);
    assert_eq!((h.side_info_len(), h.granules(), h.crc), (32, 2, false));
    // MPEG-2, 128 kbit/s, 22.05 kHz, mono.
    let h = header::parse(&[0xFF, 0xF3, 0xC0, 0xC4], 0).expect("header");
    assert_eq!((h.version, h.rate, h.channels, h.sr_index), (2, 22050, 1, 3));
    assert_eq!((h.side_info_len(), h.granules()), (9, 1));
    // Not: free format, bad bitrate, reserved sample rate, layer I, MPEG reserved, no sync.
    for bad in [[0xFF, 0xFB, 0x00, 0x44], [0xFF, 0xFB, 0xF0, 0x44], [0xFF, 0xFB, 0x9C, 0x44], [0xFF, 0xFF, 0x90, 0x44], [0xFF, 0xEB, 0x90, 0x44], [0xFE, 0xFB, 0x90, 0x44]] {
        assert!(header::parse(&bad, 0).is_none(), "{:?}", bad);
    }
    assert!(header::parse(&[0xFF, 0xFB, 0x90], 0).is_none());
    assert!(header::parse(&[0xFF, 0xFB, 0x90, 0x44], usize::MAX).is_none());
}

#[test]
fn silent_frames_make_silence() {
    // Five MPEG-1 mono frames of nothing: a header, then zeros (no data at all).
    let mut file = Vec::new();
    for _ in 0..5 {
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0xC0]);
        file.extend(frame);
    }
    let got = mp3(&file).expect("silence");
    assert_eq!((got.rate, got.channels, got.data.len()), (44100, 1, 5 * 1152));
    assert!(got.data.iter().all(|&s| s == 0.0));
}

#[test]
fn damaged_files_never_panic() {
    // A cheap random sequence.
    let mut state = 0x2545_F491u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    for f in &FIXTURES {
        // Cut anywhere.
        let step = 1 + f.mp3.len() / 40;
        for len in (0..f.mp3.len()).step_by(step) {
            let _ = mp3(&f.mp3[..len]);
        }
        // One bit flipped anywhere, in the first frames (headers and side information)...
        for _ in 0..25 {
            let mut d = f.mp3.to_vec();
            let at = next() as usize % 1500.min(d.len());
            d[at] ^= 1 << (next() % 8);
            let _ = mp3(&d);
        }
        // ...and in the rest.
        for _ in 0..25 {
            let mut d = f.mp3.to_vec();
            let at = next() as usize % d.len();
            d[at] ^= 1 << (next() % 8);
            let _ = mp3(&d);
        }
        // Many bytes scrambled.
        for _ in 0..5 {
            let mut d = f.mp3.to_vec();
            for _ in 0..30 {
                let at = next() as usize % d.len();
                d[at] = next() as u8;
            }
            let _ = mp3(&d);
        }
    }
    // Pure noise behind a header.
    for _ in 0..20 {
        let mut d = vec![0xFF, 0xFB, 0x90, 0x44];
        for _ in 0..3000 {
            d.push(next() as u8);
        }
        let _ = mp3(&d);
        let _ = decode(&d);
    }
}

#[test]
fn damage_in_one_frame_leaves_the_rest() {
    let f = &FIXTURES[2];
    let good = mp3(f.mp3).unwrap();
    // Wreck the middle of the file: the frames before and after still come out.
    let mut d = f.mp3.to_vec();
    let mid = d.len() / 2;
    for b in &mut d[mid..mid + 100] {
        *b = 0x55;
    }
    let got = mp3(&d).expect("still decodes");
    assert!(got.data.len() + 4 * 1152 * 2 >= good.data.len());
    // The start is untouched.
    let n = 3000;
    assert_eq!(&got.data[..n], &good.data[..n]);
}

fn pack(parts: &[(u32, u32)]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::new();
    for &(v, n) in parts {
        for i in (0..n).rev() {
            bits.push(((v >> i) & 1) as u8);
        }
    }
    let mut out = vec![0u8; (bits.len() + 7) / 8 + 1];
    for (i, b) in bits.iter().enumerate() {
        out[i / 8] |= b << (7 - i % 8);
    }
    out
}

#[test]
fn every_huffman_code_comes_back_as_its_symbol() {
    let h = Huffman::new();
    for (t, table) in HUFF.iter().enumerate() {
        for s in 0..table.lens.len() {
            let data = pack(&[(table.codes[s] as u32, table.lens[s] as u32)]);
            let mut br = BitReader::new(&data);
            assert_eq!(walk(&h.trees[t], &mut br), Some(s), "table index {} symbol {}", t, s);
            assert_eq!(br.pos(), table.lens[s] as usize);
        }
    }
    for s in 0..16 {
        let data = pack(&[(QUAD_A_CODES[s] as u32, QUAD_A_LENS[s] as u32)]);
        let mut br = BitReader::new(&data);
        assert_eq!(walk(&h.quad_a, &mut br), Some(s));
    }
    // Running out of data is None, not a panic.
    let mut br = BitReader::new(&[]);
    assert_eq!(walk(&h.trees[0], &mut br), None);
}

#[test]
fn a_granule_of_values_is_read() {
    let h = Huffman::new();
    let sfl = &super::tables::SFB_LONG[0];
    // Two pairs with table 1 (2x2): (1, 0) is the code 01 and a sign bit 1 for the x; (0, 0) is
    // the code 1. Then, in the count1 part, a group of four from table B: the 4 bits 1101 mean
    // the value 15 - 13 = 2, so the third value is 1 (its sign bit is 0).
    let g = Granule { big_values: 2, table_select: [1, 0, 0], region0_count: 15, region1_count: 7, count1_table_b: true, ..Granule::default() };
    let data = pack(&[(0b01, 2), (1, 1), (1, 1), (0b1101, 4), (0, 1)]);
    let mut br = BitReader::new(&data);
    let mut is = [7i32; 576];
    h.decode(&mut br, &g, sfl, 9, &mut is).expect("decodes");
    assert_eq!(&is[..8], &[-1, 0, 0, 0, 0, 0, 1, 0]);
    assert!(is[8..].iter().all(|&v| v == 0));
    assert_eq!(br.pos(), 9);
    // The unused table numbers are refused.
    let bad = Granule { big_values: 2, table_select: [4, 0, 0], region0_count: 15, region1_count: 7, ..Granule::default() };
    let mut br = BitReader::new(&data);
    assert!(h.decode(&mut br, &bad, sfl, 9, &mut is).is_none());
}

#[test]
fn powers_and_roots() {
    assert_eq!(exp2q(0), 1.0);
    assert_eq!(exp2q(4), 2.0);
    assert_eq!(exp2q(-8), 0.25);
    assert!((exp2q(2) - 1.414_213_5).abs() < 1e-6);
    assert!((exp2q(-1) - 0.840_896_4).abs() < 1e-6);
    assert_eq!(exp2q(-100_000), 0.0);
    let r = Requant::new();
    assert_eq!(r.value(0), 0.0);
    assert!((r.value(8) - 16.0).abs() < 1e-4);
    assert!((r.value(-1000) + 10000.0).abs() < 0.1);
    assert!(r.value(1_000_000) > 0.0); // clamped, not out of range
}

fn mpeg1_joint(ext: u8) -> Header {
    Header { version: 3, sr_index: 0, rate: 44100, mode: 1, mode_ext: ext, channels: 2, crc: false, frame_len: 417 }
}

#[test]
fn mid_side_and_intensity_stereo() {
    let sfl = &super::tables::SFB_LONG[0];
    let sfs = &super::tables::SFB_SHORT[0];
    let g = Granule::default();
    let sf = Scalefacs::zero();
    // Mid/side: mid 1, side 0 is both channels at 1/sqrt 2.
    let mut xr = [[0.0f32; 576]; 2];
    xr[0].iter_mut().for_each(|v| *v = 1.0);
    stereo::process(&mut xr, &g, &sf, &mpeg1_joint(2), sfl, sfs);
    assert!((xr[0][5] - 0.707_106_8).abs() < 1e-5 && (xr[1][5] - 0.707_106_8).abs() < 1e-5);
    // Not joint stereo: left alone.
    let mut plain = mpeg1_joint(2);
    plain.mode = 0;
    let mut xr = [[0.5f32; 576]; 2];
    stereo::process(&mut xr, &g, &sf, &plain, sfl, sfs);
    assert_eq!(xr[0][9], 0.5);
    // Intensity: the right channel is empty, so every band is split by its position: 0 puts
    // everything right, 3 (45 degrees) halves it, 7 is "not intensity".
    for (pos, want) in [(0u8, (0.0f32, 1.0f32)), (3, (0.5, 0.5)), (7, (1.0, 0.0))] {
        let mut sf = Scalefacs::zero();
        sf.sf = [pos; 39];
        let mut xr = [[0.0f32; 576]; 2];
        xr[0].iter_mut().for_each(|v| *v = 1.0);
        stereo::process(&mut xr, &g, &sf, &mpeg1_joint(1), sfl, sfs);
        for k in [0usize, 100, 400, 575] {
            assert!((xr[0][k] - want.0).abs() < 1e-3 && (xr[1][k] - want.1).abs() < 1e-3, "pos {} line {}: {} {}", pos, k, xr[0][k], xr[1][k]);
        }
    }
    // A band the right channel has something in is not touched by intensity.
    let mut xr = [[0.0f32; 576]; 2];
    xr[0].iter_mut().for_each(|v| *v = 1.0);
    xr[1][3] = 0.25;
    let mut sf = Scalefacs::zero();
    sf.sf = [0; 39];
    stereo::process(&mut xr, &g, &sf, &mpeg1_joint(1), sfl, sfs);
    assert_eq!((xr[0][0], xr[1][3]), (1.0, 0.25));
    assert_eq!(xr[0][500], 0.0);
    // MPEG-2 intensity: position 1 is the left channel quieter by a quarter step.
    let mut h2 = mpeg1_joint(1);
    h2.version = 2;
    h2.sr_index = 3;
    let sfl2 = &super::tables::SFB_LONG[3];
    let sfs2 = &super::tables::SFB_SHORT[3];
    let mut sf = Scalefacs::zero();
    sf.sf = [1; 39];
    sf.max = [3; 39];
    let mut xr = [[0.0f32; 576]; 2];
    xr[0].iter_mut().for_each(|v| *v = 1.0);
    stereo::process(&mut xr, &g, &sf, &h2, sfl2, sfs2);
    assert!((xr[0][300] - 0.840_896_4).abs() < 1e-4 && xr[1][300] == 1.0);
}

#[test]
fn scale_factors_of_mpeg2_follow_their_partitions() {
    let h = Header { version: 2, sr_index: 3, rate: 22050, mode: 3, mode_ext: 0, channels: 1, crc: false, frame_len: 200 };
    // scalefac_compress 16: lengths 0, 1, 0, 0; parts of 6, 5, 5, 5 bands: so bands 6..11 have one bit each.
    let g = Granule { scalefac_compress: 16, ..Granule::default() };
    let data = pack(&[(0b10110, 5), (0b11111, 5)]);
    let mut br = BitReader::new(&data);
    let (sf, pre) = scalefac::read(&mut br, &g, &h, 0, &[false; 4], &Scalefacs::zero(), false).expect("reads");
    assert_eq!(br.pos(), 5);
    assert!(!pre);
    assert_eq!(&sf.sf[..12], &[0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0]);
    assert_eq!(sf.max[6], 1);
    // Out of data: None.
    let mut br = BitReader::new(&[]);
    assert!(scalefac::read(&mut br, &g, &h, 0, &[false; 4], &Scalefacs::zero(), false).is_none());
    // MPEG-1 reuses the first granule's factors where scfsi says so.
    let h1 = mpeg1_joint(0);
    let g1 = Granule { scalefac_compress: 5, ..Granule::default() }; // lengths 1 and 1
    let mut first = Scalefacs::zero();
    first.sf[..21].copy_from_slice(&[3; 21]);
    let data = pack(&[(0b1111_1111, 8), (0b1111_1111, 8)]);
    let mut br = BitReader::new(&data);
    let (sf, _) = scalefac::read(&mut br, &g1, &h1, 1, &[true, false, false, true], &first, false).expect("reads");
    assert_eq!((sf.sf[0], sf.sf[5], sf.sf[6], sf.sf[11], sf.sf[16], sf.sf[20]), (3, 3, 1, 1, 3, 3));
    assert_eq!(br.pos(), 10); // two groups of 5 bands read, the other two reused
}
