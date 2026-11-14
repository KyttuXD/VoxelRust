//! Points of the Edwards curve -x^2 + y^2 = 1 + d x^2 y^2 (edwards25519), in
//! extended coordinates (X, Y, Z, T) with x = X/Z, y = Y/Z, xy = T/Z.
//! RFC 8032, section 5.1. Not constant-time: it only checks signatures.

use super::field::Fe;

/// The curve's constants, worked out once per check.
pub struct Curve {
    d: Fe,
    d2: Fe,
    sqrt_m1: Fe,
}

#[derive(Clone, Copy)]
pub struct Point {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

/// The base point B, as RFC 8032 encodes it: y = 4/5 and an even x.
const BASE_POINT: [u8; 32] = {
    let mut b = [0x66u8; 32];
    b[0] = 0x58;
    b
};

impl Curve {
    pub fn new() -> Curve {
        // d = -121665 / 121666
        let d = Fe::from_u64(121665).neg().mul(&Fe::from_u64(121666).invert());
        Curve { d, d2: d.add(&d), sqrt_m1: Fe::sqrt_m1() }
    }

    pub fn base(&self) -> Point {
        // The constant is a valid encoding, so this cannot fail.
        self.decompress(&BASE_POINT).unwrap_or_else(Point::identity)
    }

    /// The point a 32-byte encoding names, or None if it names none (the y
    /// is not below p, or no x belongs to it).
    pub fn decompress(&self, bytes: &[u8; 32]) -> Option<Point> {
        let sign = bytes[31] >> 7 == 1;
        let mut yb = *bytes;
        yb[31] &= 0x7f;
        let y = Fe::from_bytes(&yb);
        // Only the shortest form of y counts.
        if y.to_bytes() != yb {
            return None;
        }
        let yy = y.square();
        let u = yy.sub(&Fe::ONE);
        let v = self.d.mul(&yy).add(&Fe::ONE);
        // x = u v^3 (u v^7)^((p-5)/8), then fixed up by sqrt(-1) if need be.
        let v3 = v.square().mul(&v);
        let v7 = v3.square().mul(&v);
        let mut x = u.mul(&v3).mul(&u.mul(&v7).pow_p58());
        let vxx = v.mul(&x.square());
        if !vxx.eq(&u) {
            if vxx.eq(&u.neg()) {
                x = x.mul(&self.sqrt_m1);
            } else {
                return None;
            }
        }
        let zero = x.eq(&Fe::ZERO);
        if zero && sign {
            return None;
        }
        if x.is_negative() != sign {
            x = x.neg();
        }
        Some(Point { x, y, z: Fe::ONE, t: x.mul(&y) })
    }

    /// P + Q (the formula also doubles).
    pub fn add(&self, p: &Point, q: &Point) -> Point {
        let a = p.y.sub(&p.x).mul(&q.y.sub(&q.x));
        let b = p.y.add(&p.x).mul(&q.y.add(&q.x));
        let c = p.t.mul(&self.d2).mul(&q.t);
        let d = p.z.mul(&q.z);
        let d = d.add(&d);
        let e = b.sub(&a);
        let f = d.sub(&c);
        let g = d.add(&c);
        let h = b.add(&a);
        Point { x: e.mul(&f), y: g.mul(&h), z: f.mul(&g), t: e.mul(&h) }
    }

    /// k * P, the scalar k as any number of little-endian bytes.
    pub fn scalar_mul(&self, p: &Point, k: &[u8]) -> Point {
        let mut r = Point::identity();
        for i in (0..k.len() * 8).rev() {
            r = self.add(&r, &r);
            if (k[i / 8] >> (i % 8)) & 1 == 1 {
                r = self.add(&r, p);
            }
        }
        r
    }
}

impl Point {
    pub fn identity() -> Point {
        Point { x: Fe::ZERO, y: Fe::ONE, z: Fe::ONE, t: Fe::ZERO }
    }

    /// Whether two points are the same one (compared without dividing).
    pub fn same(&self, o: &Point) -> bool {
        self.x.mul(&o.z).eq(&o.x.mul(&self.z)) && self.y.mul(&o.z).eq(&o.y.mul(&self.z))
    }

    /// The 32-byte encoding: y, and the low bit of x in the top bit.
    pub fn compress(&self) -> [u8; 32] {
        let zi = self.z.invert();
        let (x, y) = (self.x.mul(&zi), self.y.mul(&zi));
        let mut out = y.to_bytes();
        if x.is_negative() {
            out[31] |= 0x80;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_point_encodes_back_to_itself() {
        let curve = Curve::new();
        let b = curve.base();
        assert_eq!(b.compress(), BASE_POINT);
        // B + identity is B; doubling by add and by scalar agree.
        assert!(curve.add(&b, &Point::identity()).same(&b));
        assert!(curve.scalar_mul(&b, &[2]).same(&curve.add(&b, &b)));
        assert!(curve.scalar_mul(&b, &[1]).same(&b));
        // The identity encodes as y = 1.
        let mut one = [0u8; 32];
        one[0] = 1;
        assert_eq!(Point::identity().compress(), one);
    }

    #[test]
    fn the_group_order_takes_the_base_point_to_the_identity() {
        let curve = Curve::new();
        // L = 2^252 + 27742317777372353535851937790883648493
        let l: [u8; 32] = [
            0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
        ];
        assert!(curve.scalar_mul(&curve.base(), &l).same(&Point::identity()));
    }

    #[test]
    fn bad_encodings_are_refused() {
        let curve = Curve::new();
        // y = p (not the shortest form of 0).
        let mut p = [0xffu8; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        assert!(curve.decompress(&p).is_none());
        // y = 2 has no x on the curve.
        let mut two = [0u8; 32];
        two[0] = 2;
        assert!(curve.decompress(&two).is_none());
    }
}
