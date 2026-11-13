//! From the spectrum back to the 32 subbands: short block reordering, alias reduction, and the
//! inverse MDCT with its windows.

use super::header::Granule;
use crate::math::{cosf, sinf, sqrtf, PI};

pub(super) struct Imdct {
    cos_long: [[f32; 18]; 36],
    cos_short: [[f32; 6]; 12],
    /// The windows of block types 0, 1 (start), 2 (unused: see `win_short`) and 3 (stop).
    win: [[f32; 36]; 4],
    win_short: [f32; 12],
    cs: [f32; 8],
    ca: [f32; 8],
}

impl Imdct {
    pub fn new() -> Imdct {
        let mut t = Imdct { cos_long: [[0.0; 18]; 36], cos_short: [[0.0; 6]; 12], win: [[0.0; 36]; 4], win_short: [0.0; 12], cs: [0.0; 8], ca: [0.0; 8] };
        for n in 0..36 {
            for k in 0..18 {
                // cos(pi / 72 * (2n + 19) * (2k + 1)), with the angle brought into one period.
                let m = ((2 * n + 19) * (2 * k + 1)) % 144;
                t.cos_long[n][k] = cosf(PI * m as f32 / 72.0);
            }
        }
        for n in 0..12 {
            for k in 0..6 {
                let m = ((2 * n + 7) * (2 * k + 1)) % 48;
                t.cos_short[n][k] = cosf(PI * m as f32 / 24.0);
            }
        }
        for n in 0..36 {
            let normal = sinf(PI / 36.0 * (n as f32 + 0.5));
            t.win[0][n] = normal;
            t.win[2][n] = normal;
            t.win[1][n] = if n < 18 {
                normal
            } else if n < 24 {
                1.0
            } else if n < 30 {
                sinf(PI / 12.0 * ((n - 18) as f32 + 0.5))
            } else {
                0.0
            };
            t.win[3][n] = if n < 6 {
                0.0
            } else if n < 12 {
                sinf(PI / 12.0 * ((n - 6) as f32 + 0.5))
            } else if n < 18 {
                1.0
            } else {
                normal
            };
        }
        for n in 0..12 {
            t.win_short[n] = sinf(PI / 12.0 * (n as f32 + 0.5));
        }
        let c = [-0.6f32, -0.535, -0.33, -0.185, -0.095, -0.041, -0.0142, -0.0037];
        for i in 0..8 {
            let q = sqrtf(1.0 + c[i] * c[i]);
            t.cs[i] = 1.0 / q;
            t.ca[i] = c[i] / q;
        }
        t
    }

    /// Put the lines of a short block, which come sorted by band, then window, then frequency,
    /// in the order the transform wants: each frequency's three windows side by side. A mixed
    /// block's first (long) bands stay as they are.
    pub fn reorder(&self, xr: &mut [f32; 576], mixed: bool, sfs: &[u16; 14]) {
        let src = *xr;
        let first = if mixed { 3 } else { 0 };
        let mut pos = 3 * sfs[first] as usize;
        for sfb in first..13 {
            let a = sfs[sfb] as usize;
            let width = (sfs[sfb + 1] - sfs[sfb]) as usize;
            for w in 0..3 {
                for j in 0..width {
                    let dst = 3 * (a + j) + w;
                    if pos < 576 && dst < 576 {
                        xr[dst] = src[pos];
                    }
                    pos += 1;
                }
            }
        }
    }

    /// Alias reduction across the borders of the first `long_sb` subbands.
    pub fn antialias(&self, xr: &mut [f32; 576], long_sb: usize) {
        for sb in 1..long_sb.min(32) {
            for i in 0..8 {
                let (a, b) = (xr[18 * sb - 1 - i], xr[18 * sb + i]);
                xr[18 * sb - 1 - i] = a * self.cs[i] - b * self.ca[i];
                xr[18 * sb + i] = b * self.cs[i] + a * self.ca[i];
            }
        }
    }

    /// The 18 new samples of each of the 32 subbands; `overlap` carries the tail of the previous
    /// granule in and the tail of this one out. The first `long_sb` subbands of a short block are
    /// transformed as long ones (the mixed block).
    pub fn transform(&self, xr: &[f32; 576], g: &Granule, long_sb: usize, overlap: &mut [[f32; 18]; 32], out: &mut [[f32; 18]; 32]) {
        for sb in 0..32 {
            let seg = &xr[sb * 18..sb * 18 + 18];
            let mut bt = if g.window_switching { (g.block_type & 3) as usize } else { 0 };
            if bt == 2 && sb < long_sb {
                bt = 0;
            }
            let mut y = [0.0f32; 36];
            if seg.iter().any(|&v| v != 0.0) {
                if bt != 2 {
                    for n in 0..36 {
                        let mut s = 0.0;
                        for k in 0..18 {
                            s += seg[k] * self.cos_long[n][k];
                        }
                        y[n] = s * self.win[bt][n];
                    }
                } else {
                    for w in 0..3 {
                        for n in 0..12 {
                            let mut s = 0.0;
                            for k in 0..6 {
                                s += seg[3 * k + w] * self.cos_short[n][k];
                            }
                            y[6 + 6 * w + n] += s * self.win_short[n];
                        }
                    }
                }
            }
            for i in 0..18 {
                let mut v = y[i] + overlap[sb][i];
                overlap[sb][i] = y[18 + i];
                if sb & 1 == 1 && i & 1 == 1 {
                    v = -v; // frequency inversion
                }
                out[sb][i] = v;
            }
        }
    }
}
