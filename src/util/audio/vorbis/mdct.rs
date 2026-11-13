//! The inverse MDCT, by way of a DCT-IV done with a half-size complex FFT.

use super::mathx::{cos, sin};
use alloc::vec;
use alloc::vec::Vec;

pub(super) struct Mdct {
    /// Window length.
    n: usize,
    /// Half of n: the number of coefficients.
    m: usize,
    /// n / 4: the size of the FFT.
    h: usize,
    pre: Vec<[f32; 2]>,
    post: Vec<[f32; 2]>,
    tw: Vec<[f32; 2]>,
    rev: Vec<u32>,
}

impl Mdct {
    /// For windows of `n` samples (a power of two, 64 or more).
    pub(super) fn new(n: usize) -> Mdct {
        let (m, h) = (n / 2, n / 4);
        let pi = core::f64::consts::PI;
        let bits = (h as u32).trailing_zeros();
        let pre = (0..h)
            .map(|j| {
                let t = pi * (4 * j + 1) as f64 / (4 * m) as f64;
                [cos(t) as f32, -sin(t) as f32]
            })
            .collect();
        let post = (0..h)
            .map(|k| {
                let t = pi * k as f64 / m as f64;
                [cos(t) as f32, -sin(t) as f32]
            })
            .collect();
        let tw = (0..h / 2)
            .map(|k| {
                let t = 2.0 * pi * k as f64 / h as f64;
                [cos(t) as f32, -sin(t) as f32]
            })
            .collect();
        let rev = (0..h as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        Mdct { n, m, h, pre, post, tw, rev }
    }

    /// `spec` (n/2 coefficients) to `out` (n samples), unscaled, as the Vorbis
    /// specification has it.
    pub(super) fn inverse(&self, spec: &[f32], out: &mut [f32]) {
        let (m, h) = (self.m, self.h);
        if spec.len() != m || out.len() != self.n {
            return;
        }
        // Fold the coefficients into h complex numbers, twiddled, in bit-reversed order.
        let mut re = vec![0f32; h];
        let mut im = vec![0f32; h];
        for j in 0..h {
            let (a, b) = (spec[2 * j], spec[m - 1 - 2 * j]);
            let [c, s] = self.pre[j];
            let r = self.rev[j] as usize;
            re[r] = a * c - b * s;
            im[r] = a * s + b * c;
        }
        // The FFT.
        let mut len = 2;
        while len <= h {
            let half = len / 2;
            let step = h / len;
            let mut start = 0;
            while start < h {
                for k in 0..half {
                    let [wr, wi] = self.tw[k * step];
                    let (a, b) = (start + k, start + k + half);
                    let tr = wr * re[b] - wi * im[b];
                    let ti = wr * im[b] + wi * re[b];
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
                start += len;
            }
            len *= 2;
        }
        // The DCT-IV, then the unfolding into n samples.
        let mut d = vec![0f32; m];
        for k in 0..h {
            let [c, s] = self.post[k];
            d[2 * k] = re[k] * c - im[k] * s;
            d[m - 1 - 2 * k] = -(re[k] * s + im[k] * c);
        }
        let (q, t) = (m / 2, 3 * m / 2);
        for i in 0..self.n {
            out[i] = if i < q {
                d[i + q]
            } else if i < t {
                -d[t - 1 - i]
            } else {
                -d[i - t]
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_formula() {
        let n = 64;
        let spec: Vec<f32> = (0..n / 2).map(|k| ((k * 7 + 3) % 11) as f32 - 5.0).collect();
        let mut out = vec![0f32; n];
        Mdct::new(n).inverse(&spec, &mut out);
        for i in 0..n {
            let mut want = 0f64;
            for (k, &x) in spec.iter().enumerate() {
                want += x as f64 * cos(2.0 * core::f64::consts::PI / n as f64 * (i as f64 + 0.5 + n as f64 / 4.0) * (k as f64 + 0.5));
            }
            assert!((out[i] as f64 - want).abs() < 1e-3, "{i}: {} vs {want}", out[i]);
        }
    }
}
