//! Joint stereo: mid/side and intensity stereo.

use super::header::{Granule, Header};
use super::requant::exp2q;
use super::scalefac::Scalefacs;
use crate::math::{cosf, sinf, PI};

const SQRT_HALF: f32 = 0.707_106_77;

/// Undo the joint stereo coding of one granule, on the spectra before they are reordered. `g1`
/// and `sf1` are the second channel's granule and scale factors (its scale factors are the
/// intensity positions).
pub(super) fn process(xr: &mut [[f32; 576]; 2], g1: &Granule, sf1: &Scalefacs, h: &Header, sfl: &[u16; 23], sfs: &[u16; 14]) {
    let ms = h.mode == 1 && h.mode_ext & 2 != 0;
    let intensity = h.mode == 1 && h.mode_ext & 1 != 0;
    let mut coded = [false; 576]; // lines done by intensity stereo
    if intensity {
        let (a, b) = xr.split_at_mut(1);
        intensity_stereo(&mut a[0], &mut b[0], g1, sf1, h, sfl, sfs, &mut coded);
    }
    if ms {
        for k in 0..576 {
            if !coded[k] {
                let (m, s) = (xr[0][k], xr[1][k]);
                xr[0][k] = (m + s) * SQRT_HALF;
                xr[1][k] = (m - s) * SQRT_HALF;
            }
        }
    }
}

/// How the sum signal in `l` splits between the channels at an intensity position, or `None`
/// where the position says the band is not intensity coded.
fn split(pos: u8, max: u8, h: &Header, g1: &Granule) -> Option<(f32, f32)> {
    if h.mpeg1() {
        if pos >= 7 {
            return None;
        }
        let a = pos as f32 * PI / 12.0;
        let (s, c) = (sinf(a), cosf(a));
        Some((s / (s + c), c / (s + c)))
    } else {
        if pos >= max {
            return None;
        }
        let quarters = if g1.scalefac_compress & 1 == 1 { 2 } else { 1 };
        let m = ((pos as i32) + 1) >> 1;
        if pos & 1 == 1 {
            Some((exp2q(-m * quarters), 1.0))
        } else {
            Some((1.0, exp2q(-(pos as i32 >> 1) * quarters)))
        }
    }
}

fn intensity_stereo(l: &mut [f32; 576], r: &mut [f32; 576], g1: &Granule, sf1: &Scalefacs, h: &Header, sfl: &[u16; 23], sfs: &[u16; 14], coded: &mut [bool; 576]) {
    if !g1.is_short() {
        // Every band above the last one with anything in the right channel.
        let last: i32 = match r.iter().rposition(|&v| v != 0.0) {
            Some(k) => k as i32,
            None => -1,
        };
        for sfb in 0..22 {
            let (a, b) = (sfl[sfb] as usize, sfl[sfb + 1] as usize);
            if (a as i32) <= last {
                continue;
            }
            // The last band has no scale factor of its own: it takes the one before.
            let si = sfb.min(20);
            if let Some((kl, kr)) = split(sf1.sf[si], sf1.max[si], h, g1) {
                for k in a..b {
                    let x = l[k];
                    l[k] = x * kl;
                    r[k] = x * kr;
                    coded[k] = true;
                }
            }
        }
        return;
    }
    // Short blocks: the same, window by window (the long part of a mixed block is left alone).
    let first = if g1.mixed { 3 } else { 0 };
    for w in 0..3 {
        let mut last_band = first as i32 - 1;
        for sfb in first..13 {
            let width = (sfs[sfb + 1] - sfs[sfb]) as usize;
            let start = 3 * sfs[sfb] as usize + w * width;
            if r[start..start + width].iter().any(|&v| v != 0.0) {
                last_band = sfb as i32;
            }
        }
        for sfb in (last_band + 1) as usize..13 {
            let width = (sfs[sfb + 1] - sfs[sfb]) as usize;
            let start = 3 * sfs[sfb] as usize + w * width;
            let si = sfb.min(11) * 3 + w;
            if let Some((kl, kr)) = split(sf1.sf[si], sf1.max[si], h, g1) {
                for k in start..start + width {
                    let x = l[k];
                    l[k] = x * kl;
                    r[k] = x * kr;
                    coded[k] = true;
                }
            }
        }
    }
}
