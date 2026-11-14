//! A tiny random number generator for the sound code (xorshift32): fast,
//! allocation-free, and the same on every machine. Not for anything that
//! must be unpredictable.

#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0 up to (not including) 1.
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    /// -1 up to (not including) 1.
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// 0..n (0 when n is 0).
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.next_u32() >> 8) % n
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_in_their_ranges_and_zero_seed_works() {
        let mut r = Rng::new(0);
        for _ in 0..10_000 {
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
            let s = r.signed();
            assert!((-1.0..1.0).contains(&s));
            assert!(r.below(7) < 7);
        }
        assert_eq!(r.below(0), 0);
    }
}
