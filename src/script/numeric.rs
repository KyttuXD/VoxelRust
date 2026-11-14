//! The arithmetic the script language needs: floor, rounding, `%` on
//! floats, powers, square roots, `exp`, `ln` and the trigonometric
//! functions. `core` has none of them without `std`, and the float `%`
//! operator is avoided for the old toolchain (see docs/PORTING.md), so
//! they are written out here. `sin`, `cos` and the like go through
//! `crate::math` (single precision, the same on every machine) and are
//! cleaned back to the digits an `f32` would show.

use crate::script::value::clean_f32;
use core::f64::consts::{LN_2, PI};

pub fn fabs(x: f64) -> f64 {
    if x < 0.0 {
        -x
    } else if x == 0.0 {
        0.0
    } else {
        x
    }
}

/// Too big to have a fractional part (or not a number).
fn whole(x: f64) -> bool {
    !x.is_finite() || fabs(x) >= 4.5e15
}

pub fn trunc(x: f64) -> f64 {
    if whole(x) {
        x
    } else {
        (x as i64) as f64
    }
}

pub fn floor(x: f64) -> f64 {
    if whole(x) {
        return x;
    }
    let t = trunc(x);
    if t > x {
        t - 1.0
    } else {
        t
    }
}

pub fn ceil(x: f64) -> f64 {
    if whole(x) {
        return x;
    }
    let t = trunc(x);
    if t < x {
        t + 1.0
    } else {
        t
    }
}

/// To the nearest whole number, halves away from zero (as Rust's `round`).
pub fn round(x: f64) -> f64 {
    if whole(x) {
        return x;
    }
    if x < 0.0 {
        -floor(-x + 0.5)
    } else {
        floor(x + 0.5)
    }
}

/// `a % b` for floats: the remainder with the sign of `a`.
pub fn fmod(a: f64, b: f64) -> f64 {
    if b == 0.0 || !a.is_finite() || b.is_nan() {
        return f64::NAN;
    }
    if !b.is_finite() {
        return a;
    }
    a - trunc(a / b) * b
}

pub fn powi(x: f64, n: i64) -> f64 {
    let mut result = 1.0;
    let mut base = x;
    let mut k = if n < 0 { -(n as i128) } else { n as i128 };
    while k > 0 {
        if k & 1 == 1 {
            result *= base;
        }
        base *= base;
        k >>= 1;
    }
    if n < 0 {
        1.0 / result
    } else {
        result
    }
}

pub fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let mut y = f64::from_bits((x.to_bits() >> 1) + 0x1FF8_0000_0000_0000);
    for _ in 0..6 {
        y = 0.5 * (y + x / y);
    }
    y
}

pub fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let mut m = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    if e == -1023 {
        // A subnormal number: scale it up first.
        return ln(x * 18014398509481984.0) - 54.0 * LN_2;
    }
    if m > 1.414_213_562_373_095_1 {
        m *= 0.5;
        e += 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let mut term = z;
    let mut sum = 0.0;
    let mut k = 1.0;
    for _ in 0..14 {
        sum += term / k;
        term *= z2;
        k += 2.0;
    }
    2.0 * sum + e as f64 * LN_2
}

pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.78 {
        return f64::INFINITY;
    }
    if x < -745.0 {
        return 0.0;
    }
    let k = round(x / LN_2);
    let r = x - k * LN_2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for n in 1..16 {
        term *= r / n as f64;
        sum += term;
    }
    // sum * 2^k, in two steps so that the exponent stays in range.
    let k = k as i64;
    let half = k / 2;
    sum * pow2(half) * pow2(k - half)
}

fn pow2(k: i64) -> f64 {
    let e = (k + 1023).clamp(1, 2046) as u64;
    f64::from_bits(e << 52)
}

pub fn powf(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if y == trunc(y) && fabs(y) < 1e9 {
        return powi(x, y as i64);
    }
    if x > 0.0 {
        return exp(y * ln(x));
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    f64::NAN
}

pub fn sin(x: f64) -> f64 {
    clean_f32(crate::math::sinf(x as f32))
}

pub fn cos(x: f64) -> f64 {
    clean_f32(crate::math::cosf(x as f32))
}

pub fn tan(x: f64) -> f64 {
    let c = crate::math::cosf(x as f32);
    clean_f32(crate::math::sinf(x as f32) / c)
}

pub fn atan2(y: f64, x: f64) -> f64 {
    clean_f32(crate::math::atan2f(y as f32, x as f32))
}

pub fn atan(x: f64) -> f64 {
    atan2(x, 1.0)
}

pub fn asin(x: f64) -> f64 {
    if fabs(x) > 1.0 {
        return f64::NAN;
    }
    atan2(x, sqrt(1.0 - x * x))
}

pub fn acos(x: f64) -> f64 {
    if fabs(x) > 1.0 {
        return f64::NAN;
    }
    atan2(sqrt(1.0 - x * x), x)
}

pub fn to_radians(x: f64) -> f64 {
    x * (PI / 180.0)
}

pub fn to_degrees(x: f64) -> f64 {
    x * (180.0 / PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        fabs(a - b) <= 1e-9 * (1.0 + fabs(b))
    }

    #[test]
    fn rounding() {
        assert_eq!(floor(2.7), 2.0);
        assert_eq!(floor(-2.1), -3.0);
        assert_eq!(ceil(2.1), 3.0);
        assert_eq!(ceil(-2.9), -2.0);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -3.0);
        assert_eq!(round(2.4), 2.0);
        assert_eq!(trunc(-2.9), -2.0);
        assert_eq!(floor(1e20), 1e20);
        assert!(floor(f64::NAN).is_nan());
    }

    #[test]
    fn remainder_and_powers() {
        assert!(near(fmod(7.5, 2.0), 1.5));
        assert!(near(fmod(-7.5, 2.0), -1.5));
        assert!(fmod(1.0, 0.0).is_nan());
        assert_eq!(powi(2.0, 10), 1024.0);
        assert_eq!(powi(2.0, -2), 0.25);
        assert!(near(powf(2.0, 0.5), 1.414_213_562_373_095_1));
        assert!(near(powf(10.0, 3.0), 1000.0));
        assert!(near(powf(2.0, 10.5), 1448.154_687_911_136_1));
    }

    #[test]
    fn roots_logs_and_exponentials() {
        assert!(near(sqrt(2.0), 1.414_213_562_373_095_1));
        assert_eq!(sqrt(0.0), 0.0);
        assert!(near(sqrt(16.0), 4.0));
        assert!(sqrt(-1.0).is_nan());
        assert!(near(ln(1.0), 0.0));
        assert!(near(ln(core::f64::consts::E), 1.0));
        assert!(near(ln(1000.0), 6.907_755_278_982_137));
        assert!(near(ln(0.001), -6.907_755_278_982_137));
        assert!(near(exp(1.0), core::f64::consts::E));
        assert!(near(exp(-3.0), 0.049_787_068_367_863_944));
        assert!(near(exp(20.0), 485_165_195.409_790_3));
        assert_eq!(exp(1000.0), f64::INFINITY);
        assert_eq!(ln(0.0), f64::NEG_INFINITY);
    }

    #[test]
    fn trigonometry_is_close() {
        assert!(fabs(sin(0.5) - 0.479_425_5) < 1e-6);
        assert!(fabs(cos(0.0) - 1.0) < 1e-4);
        assert!(fabs(atan2(1.0, 1.0) - PI / 4.0) < 1e-3);
        assert!(fabs(acos(0.0) - PI / 2.0) < 1e-3);
    }
}
