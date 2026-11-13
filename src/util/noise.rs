//! Hash-based value noise on an integer lattice.
//!
//! Coordinates are whole blocks and the lattice spacing is a whole number of
//! blocks, so the result is exact no matter how far from the origin you are.

#[inline]
pub fn hash3(seed: u32, x: i32, y: i32, z: i32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

#[inline]
pub fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    hash3(seed, x, 0x5bd1, z)
}

#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth 2D noise in `[0, 1)` with features about `scale` blocks across.
pub fn noise2(seed: u32, x: i32, z: i32, scale: i32) -> f32 {
    let (cx, cz) = (x.div_euclid(scale), z.div_euclid(scale));
    let fx = fade(x.rem_euclid(scale) as f32 / scale as f32);
    let fz = fade(z.rem_euclid(scale) as f32 / scale as f32);
    let a = unit(hash2(seed, cx, cz));
    let b = unit(hash2(seed, cx + 1, cz));
    let c = unit(hash2(seed, cx, cz + 1));
    let d = unit(hash2(seed, cx + 1, cz + 1));
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fz
}

/// Several octaves of [`noise2`], still in `[0, 1)`.
pub fn fbm2(seed: u32, x: i32, z: i32, scale: i32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut total = 0.0;
    let mut s = scale;
    for o in 0..octaves {
        if s < 1 {
            break;
        }
        sum += noise2(seed.wrapping_add(o.wrapping_mul(1013)), x, z, s) * amp;
        total += amp;
        amp *= 0.5;
        s /= 2;
    }
    sum / total
}

/// Smooth 3D noise in `[0, 1)`; `sxz` and `sy` are the feature sizes.
pub fn noise3(seed: u32, x: i32, y: i32, z: i32, sxz: i32, sy: i32) -> f32 {
    let (cx, cy, cz) = (x.div_euclid(sxz), y.div_euclid(sy), z.div_euclid(sxz));
    let fx = fade(x.rem_euclid(sxz) as f32 / sxz as f32);
    let fy = fade(y.rem_euclid(sy) as f32 / sy as f32);
    let fz = fade(z.rem_euclid(sxz) as f32 / sxz as f32);
    let v = |dx: i32, dy: i32, dz: i32| unit(hash3(seed, cx + dx, cy + dy, cz + dz));
    let x00 = v(0, 0, 0) + (v(1, 0, 0) - v(0, 0, 0)) * fx;
    let x01 = v(0, 0, 1) + (v(1, 0, 1) - v(0, 0, 1)) * fx;
    let x10 = v(0, 1, 0) + (v(1, 1, 0) - v(0, 1, 0)) * fx;
    let x11 = v(0, 1, 1) + (v(1, 1, 1) - v(0, 1, 1)) * fx;
    let y0 = x00 + (x01 - x00) * fz;
    let y1 = x10 + (x11 - x10) * fz;
    y0 + (y1 - y0) * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_in_range_and_continuous() {
        let mut prev = noise2(7, -200, 33, 32);
        for x in -199..200 {
            let v = noise2(7, x, 33, 32);
            assert!((0.0..1.0).contains(&v));
            assert!((v - prev).abs() < 0.1, "jump at {x}");
            prev = v;
        }
    }

    #[test]
    fn noise_is_deterministic_and_seeded() {
        assert_eq!(noise3(1, 5, 6, 7, 16, 8), noise3(1, 5, 6, 7, 16, 8));
        assert_ne!(noise2(1, 100, 100, 16), noise2(2, 100, 100, 16));
    }
}
