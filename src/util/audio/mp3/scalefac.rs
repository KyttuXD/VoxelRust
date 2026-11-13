//! Scale factors: how much louder each band of a granule is than its global gain says.

use super::bits::BitReader;
use super::header::{Granule, Header};
use super::tables::{LSF_PARTS, SLEN1, SLEN2};

/// The scale factors of one channel in one granule.
#[derive(Clone, Copy)]
pub(super) struct Scalefacs {
    /// Long blocks: `sf[band]` for bands 0..=20 (band 21 has none: 0). Short blocks:
    /// `sf[band * 3 + window]`. A mixed block has both (long bands below 8, short from band 3).
    pub sf: [u8; 39],
    /// For intensity stereo: a value this big in `sf` means the band is not intensity coded
    /// (7 in MPEG-1, the largest the field can hold in MPEG-2).
    pub max: [u8; 39],
}

impl Scalefacs {
    pub fn zero() -> Scalefacs {
        Scalefacs { sf: [0; 39], max: [7; 39] }
    }
}

/// Read the scale factors of granule `gran` (0 or 1). `first` holds those of granule 0 of the
/// same channel, which `scfsi` may ask to reuse (MPEG-1 only). `intensity` is true for the
/// second channel of a stream that uses intensity stereo (MPEG-2 packs its scale factors
/// differently then). Returns the scale factors and whether `preflag` is in effect.
pub(super) fn read(
    br: &mut BitReader,
    g: &Granule,
    h: &Header,
    gran: usize,
    scfsi: &[bool; 4],
    first: &Scalefacs,
    intensity: bool,
) -> Option<(Scalefacs, bool)> {
    let mut out = Scalefacs::zero();
    let short = g.is_short();
    let mixed = short && g.mixed;
    if h.mpeg1() {
        let sc = (g.scalefac_compress & 15) as usize;
        let (s1, s2) = (SLEN1[sc] as u32, SLEN2[sc] as u32);
        if short {
            let long_bands = if mixed { 8 } else { 0 };
            for i in 0..long_bands {
                out.sf[i] = br.bits(s1)? as u8;
            }
            for sfb in (if mixed { 3 } else { 0 })..12 {
                let len = if sfb < 6 { s1 } else { s2 };
                for w in 0..3 {
                    out.sf[sfb * 3 + w] = br.bits(len)? as u8;
                }
            }
        } else {
            let groups = [(0usize, 6usize, s1), (6, 11, s1), (11, 16, s2), (16, 21, s2)];
            for (k, &(a, b, len)) in groups.iter().enumerate() {
                for i in a..b {
                    out.sf[i] = if gran == 1 && scfsi[k] { first.sf[i] } else { br.bits(len)? as u8 };
                }
            }
        }
        return Some((out, g.preflag));
    }

    // MPEG-2 and 2.5: one granule, the lengths of the four parts from `scalefac_compress`.
    let mut sc = g.scalefac_compress as usize;
    let slen: [u32; 4];
    let kind: usize;
    let preflag: bool;
    if intensity {
        sc >>= 1;
        if sc < 180 {
            slen = [(sc / 36) as u32, ((sc % 36) / 6) as u32, ((sc % 36) % 6) as u32, 0];
            kind = 3;
        } else if sc < 244 {
            sc -= 180;
            slen = [((sc & 63) >> 4) as u32, ((sc & 15) >> 2) as u32, (sc & 3) as u32, 0];
            kind = 4;
        } else {
            sc -= 244;
            slen = [(sc / 3) as u32, (sc % 3) as u32, 0, 0];
            kind = 5;
        }
        preflag = false;
    } else if sc < 400 {
        slen = [((sc >> 4) / 5) as u32, ((sc >> 4) % 5) as u32, ((sc & 15) >> 2) as u32, (sc & 3) as u32];
        kind = 0;
        preflag = false;
    } else if sc < 500 {
        sc -= 400;
        slen = [((sc >> 2) / 5) as u32, ((sc >> 2) % 5) as u32, (sc & 3) as u32, 0];
        kind = 1;
        preflag = false;
    } else {
        sc -= 500;
        slen = [(sc / 3) as u32, (sc % 3) as u32, 0, 0];
        kind = 2;
        preflag = true;
    }
    let counts = &LSF_PARTS[kind][if mixed { 2 } else if short { 1 } else { 0 }];
    let mut vals = [0u8; 64];
    let mut vmax = [7u8; 64];
    let mut n = 0usize;
    for part in 0..4 {
        for _ in 0..counts[part] {
            if n >= 64 {
                return None;
            }
            vals[n] = br.bits(slen[part])? as u8;
            vmax[n] = ((1u32 << slen[part]) - 1) as u8;
            n += 1;
        }
    }
    if !short {
        for i in 0..21 {
            out.sf[i] = vals[i];
            out.max[i] = vmax[i];
        }
    } else {
        let mut k = 0;
        if mixed {
            for i in 0..6 {
                out.sf[i] = vals[i];
                out.max[i] = vmax[i];
            }
            k = 6;
        }
        for sfb in (if mixed { 3 } else { 0 })..12 {
            for w in 0..3 {
                out.sf[sfb * 3 + w] = vals[k];
                out.max[sfb * 3 + w] = vmax[k];
                k += 1;
            }
        }
    }
    Some((out, preflag))
}
