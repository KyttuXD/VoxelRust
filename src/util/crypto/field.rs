//! Arithmetic modulo p = 2^255 - 19, the field Ed25519 works in.
//!
//! A number is five limbs of 51 bits (radix 2^51), as in the "donna" 64-bit
//! code. Every operation takes limbs below 2^52 and gives limbs a little
//! above 2^51 (carried), so nothing here can overflow: the products are
//! summed in `u128` (five terms of at most 2^52 * 19 * 2^52 each is far
//! below 2^128). Not constant-time: it is only ever used to check a signature.

const MASK: u64 = (1 << 51) - 1;

/// A number modulo 2^255 - 19.
#[derive(Clone, Copy)]
pub struct Fe(pub [u64; 5]);

/// p - 2, little-endian: the exponent that inverts.
const P_MINUS_2: [u8; 32] = {
    let mut e = [0xffu8; 32];
    e[0] = 0xeb;
    e[31] = 0x7f;
    e
};
/// (p - 5) / 8 = 2^252 - 3: the exponent for square roots.
const P_MINUS_5_OVER_8: [u8; 32] = {
    let mut e = [0xffu8; 32];
    e[0] = 0xfd;
    e[31] = 0x0f;
    e
};
/// (p - 1) / 4 = 2^253 - 5: 2 to this power is a square root of -1.
const P_MINUS_1_OVER_4: [u8; 32] = {
    let mut e = [0xffu8; 32];
    e[0] = 0xfb;
    e[31] = 0x1f;
    e
};

fn load8(b: &[u8; 32], at: usize) -> u64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(w)
}

/// Bring limbs back under 2^51 (plus a few bits in the second one).
fn carry(a: [u64; 5]) -> Fe {
    let mut r = a;
    let mut c = r[0] >> 51;
    r[0] &= MASK;
    r[1] += c;
    c = r[1] >> 51;
    r[1] &= MASK;
    r[2] += c;
    c = r[2] >> 51;
    r[2] &= MASK;
    r[3] += c;
    c = r[3] >> 51;
    r[3] &= MASK;
    r[4] += c;
    c = r[4] >> 51;
    r[4] &= MASK;
    r[0] += c * 19;
    c = r[0] >> 51;
    r[0] &= MASK;
    r[1] += c;
    Fe(r)
}

impl Fe {
    pub const ZERO: Fe = Fe([0; 5]);
    pub const ONE: Fe = Fe([1, 0, 0, 0, 0]);

    pub fn from_u64(v: u64) -> Fe {
        Fe([v & MASK, v >> 51, 0, 0, 0])
    }

    /// The number in 32 little-endian bytes. Bit 255 is ignored; a value of
    /// p or more is taken as it stands (see [`Fe::to_bytes`] to compare).
    pub fn from_bytes(b: &[u8; 32]) -> Fe {
        Fe([
            load8(b, 0) & MASK,
            (load8(b, 6) >> 3) & MASK,
            (load8(b, 12) >> 6) & MASK,
            (load8(b, 19) >> 1) & MASK,
            (load8(b, 24) >> 12) & MASK,
        ])
    }

    /// The number, reduced fully (below p), in 32 little-endian bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut h = carry(carry(self.0).0).0;
        // q is 1 when h is p or more, else 0.
        let mut q = (h[0] + 19) >> 51;
        q = (h[1] + q) >> 51;
        q = (h[2] + q) >> 51;
        q = (h[3] + q) >> 51;
        q = (h[4] + q) >> 51;
        h[0] += 19 * q;
        // Subtracting p is adding 19 and dropping bit 255.
        let mut c = h[0] >> 51;
        h[0] &= MASK;
        h[1] += c;
        c = h[1] >> 51;
        h[1] &= MASK;
        h[2] += c;
        c = h[2] >> 51;
        h[2] &= MASK;
        h[3] += c;
        c = h[3] >> 51;
        h[3] &= MASK;
        h[4] += c;
        h[4] &= MASK;
        let words = [h[0] | (h[1] << 51), (h[1] >> 13) | (h[2] << 38), (h[2] >> 26) | (h[3] << 25), (h[3] >> 39) | (h[4] << 12)];
        let mut out = [0u8; 32];
        for i in 0..4 {
            out[i * 8..i * 8 + 8].copy_from_slice(&words[i].to_le_bytes());
        }
        out
    }

    pub fn add(&self, o: &Fe) -> Fe {
        let (a, b) = (&self.0, &o.0);
        carry([a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3], a[4] + b[4]])
    }

    pub fn sub(&self, o: &Fe) -> Fe {
        let (a, b) = (&self.0, &o.0);
        // Add 2p first so that no limb goes below zero (o's limbs are below 2^52 - 38).
        const TWO_P0: u64 = 0xfffffffffffda;
        const TWO_P: u64 = 0xffffffffffffe;
        carry([a[0] + TWO_P0 - b[0], a[1] + TWO_P - b[1], a[2] + TWO_P - b[2], a[3] + TWO_P - b[3], a[4] + TWO_P - b[4]])
    }

    pub fn neg(&self) -> Fe {
        Fe::ZERO.sub(self)
    }

    pub fn mul(&self, o: &Fe) -> Fe {
        let (a, b) = (&self.0, &o.0);
        let m = |x: u64, y: u64| -> u128 { (x as u128) * (y as u128) };
        let (b1, b2, b3, b4) = (b[1] * 19, b[2] * 19, b[3] * 19, b[4] * 19);
        let mut r0 = m(a[0], b[0]) + m(a[4], b1) + m(a[3], b2) + m(a[2], b3) + m(a[1], b4);
        let mut r1 = m(a[1], b[0]) + m(a[0], b[1]) + m(a[4], b2) + m(a[3], b3) + m(a[2], b4);
        let mut r2 = m(a[2], b[0]) + m(a[1], b[1]) + m(a[0], b[2]) + m(a[4], b3) + m(a[3], b4);
        let mut r3 = m(a[3], b[0]) + m(a[2], b[1]) + m(a[1], b[2]) + m(a[0], b[3]) + m(a[4], b4);
        let mut r4 = m(a[4], b[0]) + m(a[3], b[1]) + m(a[2], b[2]) + m(a[1], b[3]) + m(a[0], b[4]);
        let mask = MASK as u128;
        let mut c = r0 >> 51;
        r0 &= mask;
        r1 += c;
        c = r1 >> 51;
        r1 &= mask;
        r2 += c;
        c = r2 >> 51;
        r2 &= mask;
        r3 += c;
        c = r3 >> 51;
        r3 &= mask;
        r4 += c;
        c = r4 >> 51;
        r4 &= mask;
        r0 += c * 19;
        c = r0 >> 51;
        r0 &= mask;
        r1 += c;
        // Every limb is now below 2^51 + 2^20, so the casts lose nothing.
        Fe([r0 as u64, r1 as u64, r2 as u64, r3 as u64, r4 as u64])
    }

    pub fn square(&self) -> Fe {
        self.mul(self)
    }

    /// self^exponent, the exponent in 32 little-endian bytes.
    fn pow(&self, exponent: &[u8; 32]) -> Fe {
        let mut r = Fe::ONE;
        for i in (0..256).rev() {
            r = r.square();
            if (exponent[i / 8] >> (i % 8)) & 1 == 1 {
                r = r.mul(self);
            }
        }
        r
    }

    /// 1 / self (0 for 0).
    pub fn invert(&self) -> Fe {
        self.pow(&P_MINUS_2)
    }

    /// self^((p - 5) / 8), the heart of the square root.
    pub fn pow_p58(&self) -> Fe {
        self.pow(&P_MINUS_5_OVER_8)
    }

    /// A square root of -1.
    pub fn sqrt_m1() -> Fe {
        Fe::from_u64(2).pow(&P_MINUS_1_OVER_4)
    }

    pub fn is_negative(&self) -> bool {
        self.to_bytes()[0] & 1 == 1
    }

    pub fn eq(&self, o: &Fe) -> bool {
        self.to_bytes() == o.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_and_reduce() {
        let mut b = [0u8; 32];
        for i in 0..32 {
            b[i] = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        b[31] &= 0x7f;
        assert_eq!(Fe::from_bytes(&b).to_bytes(), b);
        // p itself is zero; p + 1 is one.
        let mut p = [0xffu8; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        assert_eq!(Fe::from_bytes(&p).to_bytes(), [0u8; 32]);
        p[0] = 0xee;
        assert_eq!(Fe::from_bytes(&p).to_bytes(), Fe::ONE.to_bytes());
    }

    #[test]
    fn arithmetic_agrees_with_itself() {
        let a = Fe::from_u64(123_456_789_012_345);
        let b = Fe::from_u64(987_654_321_098_765);
        // (a + b) - b == a, a * 1 == a, a * a^-1 == 1, a - a == 0
        assert!(a.add(&b).sub(&b).eq(&a));
        assert!(a.mul(&Fe::ONE).eq(&a));
        assert!(a.mul(&a.invert()).eq(&Fe::ONE));
        assert!(a.sub(&a).eq(&Fe::ZERO));
        assert!(a.add(&a.neg()).eq(&Fe::ZERO));
        // (a + b)^2 == a^2 + 2ab + b^2
        let two_ab = a.mul(&b).add(&a.mul(&b));
        assert!(a.add(&b).square().eq(&a.square().add(&two_ab).add(&b.square())));
        // sqrt(-1)^2 == -1
        assert!(Fe::sqrt_m1().square().eq(&Fe::ONE.neg()));
        // 2^255 is 19 (mod p), and (2^51)^5 is 2^255.
        let x = Fe::from_u64(1 << 51);
        let mut r = Fe::ONE;
        for _ in 0..5 {
            r = r.mul(&x);
        }
        assert!(r.eq(&Fe::from_u64(19)));
    }
}
