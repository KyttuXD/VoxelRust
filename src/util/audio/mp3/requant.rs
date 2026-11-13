//! Requantisation: the Huffman values to real spectral values, `x^(4/3) * 2^(gain / 4)`.

use super::header::{Granule, Header};
use super::scalefac::Scalefacs;
use super::tables::PRETAB;
use alloc::vec::Vec;

/// The biggest magnitude a value can have: 15 plus 13 linbits.
const MAX_VALUE: usize = 15 + 8191;

/// The cube root, by Newton's method (`core` has none).
fn cbrt(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = 1.0f64;
    while y * y * y < x && y < 1e30 {
        y *= 2.0;
    }
    for _ in 0..12 {
        y = (2.0 * y + x / (y * y)) / 3.0;
    }
    y
}

/// `2^(q / 4)` for a whole `q`.
pub(super) fn exp2q(q: i32) -> f32 {
    let k = q.div_euclid(4).clamp(-1000, 1000);
    let base = [1.0f64, 1.189_207_115_002_721, 1.414_213_562_373_095_1, 1.681_792_830_507_429][q.rem_euclid(4) as usize];
    let p = f64::from_bits(((k + 1023) as u64) << 52);
    (base * p) as f32
}

pub(super) struct Requant {
    pow43: Vec<f32>,
}

impl Requant {
    pub fn new() -> Requant {
        let mut pow43 = Vec::with_capacity(MAX_VALUE + 1);
        for i in 0..=MAX_VALUE {
            let x = i as f64;
            pow43.push((x * cbrt(x)) as f32);
        }
        Requant { pow43 }
    }

    pub fn value(&self, v: i32) -> f32 {
        let m = self.pow43[(v.unsigned_abs() as usize).min(MAX_VALUE)];
        if v < 0 {
            -m
        } else {
            m
        }
    }

    /// The spectrum of one granule of one channel, in the order the data came in (a short block
    /// is still sorted by band, then window).
    pub fn requant(&self, is: &[i32; 576], g: &Granule, sf: &Scalefacs, preflag: bool, h: &Header, sfl: &[u16; 23], sfs: &[u16; 14], xr: &mut [f32; 576]) {
        *xr = [0.0; 576];
        let step = if g.scalefac_scale { 4 } else { 2 }; // in quarters of a power of two
        let base = g.global_gain - 210;
        let short = g.is_short();
        let mixed = short && g.mixed;
        let long_bands = if !short {
            22
        } else if mixed {
            if h.mpeg1() {
                8
            } else {
                6
            }
        } else {
            0
        };
        for sfb in 0..long_bands {
            let pre = if preflag && sfb < 22 { PRETAB[sfb] as i32 } else { 0 };
            let scale = exp2q(base - step * (sf.sf[sfb] as i32 + pre));
            for k in sfl[sfb] as usize..sfl[sfb + 1] as usize {
                if is[k] != 0 {
                    xr[k] = self.value(is[k]) * scale;
                }
            }
        }
        if !short {
            return;
        }
        let first = if mixed { 3 } else { 0 };
        let mut pos = 3 * sfs[first] as usize;
        for sfb in first..13 {
            let width = (sfs[sfb + 1] - sfs[sfb]) as usize;
            for w in 0..3 {
                let scale = exp2q(base - 8 * g.subblock_gain[w] - step * sf.sf[sfb * 3 + w] as i32);
                for _ in 0..width {
                    if pos < 576 && is[pos] != 0 {
                        xr[pos] = self.value(is[pos]) * scale;
                    }
                    pos += 1;
                }
            }
        }
    }
}
