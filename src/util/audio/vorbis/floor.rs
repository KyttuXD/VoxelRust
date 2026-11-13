//! Floors: the coarse spectral envelope each channel carries. Type 1 is a
//! piecewise line in the log domain; type 0 is an LSP filter curve.

use super::bits::{ilog, Bits};
use super::codebook::Codebook;
use super::mathx;
use alloc::vec;
use alloc::vec::Vec;

pub(super) enum Floor {
    Zero(Floor0),
    One(Floor1),
}

pub(super) struct Floor0 {
    order: usize,
    rate: u32,
    bark_map_size: u32,
    amp_bits: u32,
    amp_offset: u32,
    books: Vec<usize>,
}

struct Class1 {
    dims: usize,
    subs: u32,
    master: usize,
    books: Vec<i32>,
}

pub(super) struct Floor1 {
    partition_class: Vec<usize>,
    classes: Vec<Class1>,
    multiplier: i32,
    x: Vec<i32>,
    /// Indexes of `x` in order of increasing x.
    sorted: Vec<usize>,
    low: Vec<usize>,
    high: Vec<usize>,
}

/// The table that turns a floor value 0..255 into an amplitude, from about
/// 1.06e-7 (-140 dB) to 1.
pub(super) fn inverse_db_table() -> Vec<f32> {
    let ratio = 1.064_985_632_743_960_9f64;
    let mut t = vec![1.0f32; 256];
    let mut v = 1.0f64;
    for i in (0..255).rev() {
        v /= ratio;
        t[i] = v as f32;
    }
    t
}

impl Floor {
    pub(super) fn parse(b: &mut Bits, books: usize) -> Option<Floor> {
        match b.read(16) {
            0 => {
                let order = b.read(8) as usize;
                let rate = b.read(16);
                let bark_map_size = b.read(16);
                let amp_bits = b.read(6);
                let amp_offset = b.read(8);
                let n = b.read(4) as usize + 1;
                let mut list = Vec::new();
                for _ in 0..n {
                    let v = b.read(8) as usize;
                    if v >= books {
                        return None;
                    }
                    list.push(v);
                }
                if order == 0 || rate == 0 || bark_map_size == 0 || b.eop {
                    return None;
                }
                Some(Floor::Zero(Floor0 { order, rate, bark_map_size, amp_bits, amp_offset, books: list }))
            }
            1 => {
                let parts = b.read(5) as usize;
                let mut partition_class = Vec::new();
                let mut max_class = -1i32;
                for _ in 0..parts {
                    let c = b.read(4) as usize;
                    max_class = max_class.max(c as i32);
                    partition_class.push(c);
                }
                let mut classes = Vec::new();
                for _ in 0..(max_class + 1) {
                    let dims = b.read(3) as usize + 1;
                    let subs = b.read(2);
                    let mut master = 0;
                    if subs != 0 {
                        master = b.read(8) as usize;
                        if master >= books {
                            return None;
                        }
                    }
                    let mut cb = Vec::new();
                    for _ in 0..(1u32 << subs) {
                        let v = b.read(8) as i32 - 1;
                        if v >= books as i32 {
                            return None;
                        }
                        cb.push(v);
                    }
                    classes.push(Class1 { dims, subs, master, books: cb });
                }
                let multiplier = b.read(2) as i32 + 1;
                let range_bits = b.read(4);
                let mut x = vec![0i32, 1i32 << range_bits];
                for &c in &partition_class {
                    for _ in 0..classes[c].dims {
                        x.push(b.read(range_bits) as i32);
                    }
                }
                if x.len() > 65 || b.eop {
                    return None;
                }
                let mut sorted: Vec<usize> = (0..x.len()).collect();
                sorted.sort_by_key(|&i| x[i]);
                for w in sorted.windows(2) {
                    if x[w[0]] == x[w[1]] {
                        return None;
                    }
                }
                let mut low = vec![0usize; x.len()];
                let mut high = vec![0usize; x.len()];
                for i in 2..x.len() {
                    let (mut lo, mut hi) = (0usize, 1usize);
                    for j in 0..i {
                        if x[j] < x[i] && x[j] > x[lo] {
                            lo = j;
                        }
                        if x[j] > x[i] && x[j] < x[hi] {
                            hi = j;
                        }
                    }
                    low[i] = lo;
                    high[i] = hi;
                }
                Some(Floor::One(Floor1 { partition_class, classes, multiplier, x, sorted, low, high }))
            }
            _ => None,
        }
    }

    /// Read this floor for one channel and write its curve (`out`, `half`
    /// values) . `Some(false)` is a channel without floor (silent); `None` is
    /// a damaged stream.
    pub(super) fn decode(&self, b: &mut Bits, books: &[Codebook], db: &[f32], half: usize, out: &mut Vec<f32>) -> Option<bool> {
        out.clear();
        out.resize(half, 0.0);
        match self {
            Floor::One(f) => f.decode(b, books, db, out),
            Floor::Zero(f) => f.decode(b, books, out),
        }
    }
}

/// A codebook read that may hit the end of the packet (Some(None)) or be damaged (None).
fn read_entry(book: &Codebook, b: &mut Bits) -> Option<Option<usize>> {
    match book.scalar(b) {
        Some(v) => Some(Some(v)),
        None if b.eop => Some(None),
        None => None,
    }
}

impl Floor1 {
    fn decode(&self, b: &mut Bits, books: &[Codebook], db: &[f32], out: &mut [f32]) -> Option<bool> {
        if b.bit() == 0 {
            return Some(false);
        }
        let range: i32 = [256, 128, 86, 64][(self.multiplier - 1) as usize & 3];
        let rb = ilog(range as u32 - 1);
        let count = self.x.len();
        let mut y = vec![0i32; count];
        y[0] = b.read(rb) as i32;
        y[1] = b.read(rb) as i32;
        let mut at = 2;
        for &c in &self.partition_class {
            let cl = &self.classes[c];
            let mut cval = 0usize;
            if cl.subs > 0 {
                match read_entry(books.get(cl.master)?, b)? {
                    Some(v) => cval = v,
                    None => return Some(false),
                }
            }
            for j in 0..cl.dims {
                let book = cl.books[cval & ((1usize << cl.subs) - 1)];
                cval >>= cl.subs;
                y[at + j] = if book >= 0 {
                    match read_entry(books.get(book as usize)?, b)? {
                        Some(v) => v as i32,
                        None => return Some(false),
                    }
                } else {
                    0
                };
            }
            at += cl.dims;
        }
        if b.eop {
            return Some(false);
        }

        // Rebuild the amplitudes: each point is predicted from its two neighbours.
        let mut fy = vec![0i32; count];
        let mut used = vec![false; count];
        fy[0] = y[0];
        fy[1] = y[1];
        used[0] = true;
        used[1] = true;
        for i in 2..count {
            let (lo, hi) = (self.low[i], self.high[i]);
            let predicted = render_point(self.x[lo], fy[lo], self.x[hi], fy[hi], self.x[i]);
            let val = y[i];
            let highroom = range - predicted;
            let lowroom = predicted;
            let room = if highroom < lowroom { highroom } else { lowroom } * 2;
            if val != 0 {
                used[lo] = true;
                used[hi] = true;
                used[i] = true;
                fy[i] = if val >= room {
                    if highroom > lowroom {
                        val - lowroom + predicted
                    } else {
                        predicted - val + highroom - 1
                    }
                } else if val & 1 != 0 {
                    predicted - ((val + 1) >> 1)
                } else {
                    predicted + (val >> 1)
                };
            } else {
                used[i] = false;
                fy[i] = predicted;
            }
        }

        // Draw the lines.
        let (mut hx, mut hy) = (0i32, 0i32);
        let (mut lx, mut ly) = (0i32, fy[self.sorted[0]] * self.multiplier);
        for &i in self.sorted.iter().skip(1) {
            if used[i] {
                hy = fy[i] * self.multiplier;
                hx = self.x[i];
                render_line(out, db, lx, ly, hx, hy);
                lx = hx;
                ly = hy;
            }
        }
        if (hx as usize) < out.len() {
            render_line(out, db, hx, hy, out.len() as i32, hy);
        }
        Some(true)
    }
}

fn render_point(x0: i32, y0: i32, x1: i32, y1: i32, x: i32) -> i32 {
    let dy = y1 - y0;
    let adx = x1 - x0;
    if adx <= 0 {
        return y0;
    }
    let ady = dy.abs();
    let off = ady * (x - x0) / adx;
    if dy < 0 {
        y0 - off
    } else {
        y0 + off
    }
}

/// Draw the line from (x0, y0) up to but not including x1, as amplitudes.
fn render_line(out: &mut [f32], db: &[f32], x0: i32, y0: i32, x1: i32, y1: i32) {
    let dy = y1 - y0;
    let adx = x1 - x0;
    if adx <= 0 || x0 < 0 {
        return;
    }
    let base = dy / adx;
    let sy = if dy < 0 { base - 1 } else { base + 1 };
    let ady = dy.abs() - base.abs() * adx;
    let x1 = x1.min(out.len() as i32);
    let mut x = x0;
    let mut y = y0;
    let mut err = 0;
    let put = |out: &mut [f32], x: i32, y: i32| {
        let yi = if y < 0 {
            0
        } else if y > 255 {
            255
        } else {
            y as usize
        };
        out[x as usize] = db[yi];
    };
    if x < x1 {
        put(out, x, y);
    }
    x += 1;
    while x < x1 {
        err += ady;
        if err >= adx {
            err -= adx;
            y += sy;
        } else {
            y += base;
        }
        put(out, x, y);
        x += 1;
    }
}

impl Floor0 {
    fn decode(&self, b: &mut Bits, books: &[Codebook], out: &mut [f32]) -> Option<bool> {
        let amp = b.read(self.amp_bits) as f64;
        if amp == 0.0 {
            return Some(false);
        }
        let which = b.read(ilog(self.books.len() as u32)) as usize;
        let book = books.get(*self.books.get(which)?)?;
        let mut coef: Vec<f64> = Vec::new();
        let mut tmp = Vec::new();
        let mut last = 0f64;
        while coef.len() < self.order {
            let Some(e) = read_entry(book, b)? else { return Some(false) };
            book.vector(e, &mut tmp)?;
            for &v in tmp.iter() {
                let v = v as f64 + last;
                coef.push(v);
            }
            if let Some(&l) = coef.last() {
                last = l;
            }
        }
        if b.eop {
            return Some(false);
        }
        coef.truncate(self.order);
        let cosc: Vec<f64> = coef.iter().map(|&c| mathx::cos(c)).collect();

        // The map from spectrum bins to the bark scale.
        let n = out.len();
        let bms = self.bark_map_size as f64;
        let bark = |x: f64| -> f64 {
            let a = crate::math::atan2f((0.00074 * x) as f32, 1.0) as f64;
            let c = crate::math::atan2f((1.85e-8 * x * x) as f32, 1.0) as f64;
            13.1 * a + 2.24 * c + 1.0e-4 * x
        };
        let scale = bms / bark(0.5 * self.rate as f64);
        let map = |i: usize| -> i64 {
            let v = mathx::floor(bark(self.rate as f64 * i as f64 / (2.0 * n as f64)) * scale);
            let v = if v < 0.0 { 0 } else { v as i64 };
            v.min(self.bark_map_size as i64 - 1)
        };
        let ampmax = ((1u64 << self.amp_bits) - 1) as f64;
        let offset = self.amp_offset as f64;
        let mut i = 0;
        while i < n {
            let m = map(i);
            let omega = core::f64::consts::PI * m as f64 / bms;
            let cw = mathx::cos(omega);
            let mut p;
            let mut q;
            if self.order & 1 == 1 {
                p = 1.0 - cw * cw;
                q = 0.25;
                for j in 0..(self.order - 1) / 2 {
                    let d = cosc[2 * j + 1] - cw;
                    p *= 4.0 * d * d;
                }
                for j in 0..(self.order + 1) / 2 {
                    let d = cosc[2 * j] - cw;
                    q *= 4.0 * d * d;
                }
            } else {
                p = 0.5 * (1.0 - cw);
                q = 0.5 * (1.0 + cw);
                for j in 0..self.order / 2 {
                    let d1 = cosc[2 * j + 1] - cw;
                    let d0 = cosc[2 * j] - cw;
                    p *= 4.0 * d1 * d1;
                    q *= 4.0 * d0 * d0;
                }
            }
            let s = crate::math::sqrtf((p + q) as f32) as f64;
            let s = if s < 1.0e-15 { 1.0e-15 } else { s };
            let v = mathx::exp(0.115_129_25 * (amp * offset / (ampmax * s) - offset)) as f32;
            let v = if v.is_finite() { v } else { 0.0 };
            // Every bin with the same map value gets the same amplitude.
            let mut j = i;
            while j < n && map(j) == m {
                out[j] = v;
                j += 1;
            }
            i = if j > i { j } else { i + 1 };
        }
        Some(true)
    }
}
