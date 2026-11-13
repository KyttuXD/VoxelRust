//! The polyphase synthesis filter bank: 32 subbands back to samples.

use super::tables::ENWINDOW;
use crate::math::{cosf, PI};
use alloc::boxed::Box;

pub(super) struct Synth {
    window: [f32; 512],
    matrix: Box<[[f32; 32]; 64]>,
}

impl Synth {
    pub fn new() -> Synth {
        let mut window = [0.0f32; 512];
        for i in 0..257 {
            let v = ENWINDOW[i] as f32 / 65536.0;
            window[i] = v;
            if i != 0 {
                window[512 - i] = if i & 63 != 0 { -v } else { v };
            }
        }
        let mut matrix = Box::new([[0.0f32; 32]; 64]);
        for i in 0..64 {
            for k in 0..32 {
                // cos((16 + i) * (2k + 1) * pi / 64), the angle brought into one period.
                let m = ((16 + i) * (2 * k + 1)) % 128;
                matrix[i][k] = cosf(PI * m as f32 / 64.0);
            }
        }
        Synth { window, matrix }
    }

    /// One granule: 18 times 32 samples, in time order, from the subband samples `sbs[band][t]`.
    /// `v` is the filter's memory, kept between granules.
    pub fn granule(&self, v: &mut [f32; 1024], sbs: &[[f32; 18]; 32], out: &mut [f32; 576]) {
        for t in 0..18 {
            v.copy_within(0..960, 64);
            for i in 0..64 {
                let row = &self.matrix[i];
                let mut s = 0.0;
                for k in 0..32 {
                    s += row[k] * sbs[k][t];
                }
                v[i] = s;
            }
            for j in 0..32 {
                let mut s = 0.0;
                for i in 0..16 {
                    let u = (i / 2) * 128 + j + if i & 1 == 1 { 96 } else { 0 };
                    s += v[u] * self.window[j + 32 * i];
                }
                out[t * 32 + j] = s;
            }
        }
    }
}
