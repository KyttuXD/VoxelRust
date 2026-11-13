//! The few functions Vorbis needs in double precision (`core` has none of them).

const PI: f64 = core::f64::consts::PI;
const LN2: f64 = core::f64::consts::LN_2;

pub(super) fn floor(x: f64) -> f64 {
    let i = x as i64;
    if (i as f64) > x {
        (i - 1) as f64
    } else {
        i as f64
    }
}

pub(super) fn sin(x: f64) -> f64 {
    if !(x > -1.0e9 && x < 1.0e9) {
        return 0.0;
    }
    let mut r = x - floor(x / (2.0 * PI) + 0.5) * 2.0 * PI;
    if r > PI * 0.5 {
        r = PI - r;
    } else if r < -PI * 0.5 {
        r = -PI - r;
    }
    let r2 = r * r;
    let mut term = r;
    let mut sum = r;
    for i in 1..10 {
        let k = (2 * i) as f64;
        term *= -r2 / (k * (k + 1.0));
        sum += term;
    }
    sum
}

pub(super) fn cos(x: f64) -> f64 {
    sin(x + PI * 0.5)
}

pub(super) fn exp(x: f64) -> f64 {
    let x = if x > 700.0 {
        700.0
    } else if x < -700.0 {
        -700.0
    } else {
        x
    };
    let k = floor(x / LN2 + 0.5);
    let r = x - k * LN2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..16 {
        term *= r / i as f64;
        sum += term;
    }
    let mut n = k as i32;
    while n > 0 {
        sum *= 2.0;
        n -= 1;
    }
    while n < 0 {
        sum *= 0.5;
        n += 1;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_functions_are_close() {
        assert!((sin(1.0) - 0.841_470_984_807_896_5).abs() < 1e-12);
        assert!((cos(2.0) + 0.416_146_836_547_142_4).abs() < 1e-12);
        assert!((sin(-20.0) + 0.912_945_250_727_628_3).abs() < 1e-10);
        assert!((exp(1.0) - core::f64::consts::E).abs() < 1e-12);
        assert!((exp(-5.0) - 0.006_737_946_999_085_467).abs() < 1e-14);
    }
}
