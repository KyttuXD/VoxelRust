//! The mixer's soft limiter: loud mixes are bent smoothly towards full
//! scale instead of being cut flat (a flat cut is the harsh crackle of many
//! loud sounds stacking up).

/// Below this level nothing changes.
pub const KNEE: i64 = 18_000;
/// Room above the knee. The output never gets past `KNEE + ROOM`.
pub const ROOM: i64 = 14_000;

/// `x` (a sum of voices, any size) as a 16-bit sample. Identical below
/// [`KNEE`]; above it the slope falls smoothly to zero at `KNEE + ROOM`
/// (32000, a little under full scale). Never exceeds full scale.
pub fn soft_limit(x: i32) -> i16 {
    let v = x as i64;
    let a = if v < 0 { -v } else { v };
    if a <= KNEE {
        return x as i16;
    }
    let over = a - KNEE;
    let y = KNEE + over * ROOM / (over + ROOM);
    (if v < 0 { -y } else { y }) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_samples_pass_unchanged() {
        for x in -(KNEE as i32)..=(KNEE as i32) {
            assert_eq!(soft_limit(x) as i32, x);
        }
    }

    #[test]
    fn nothing_ever_exceeds_full_scale() {
        for &x in &[i32::MIN, i32::MAX, -1_000_000, 1_000_000, 32_768, -32_768, 40_000, -90_000] {
            let y = soft_limit(x) as i32;
            assert!(y.abs() <= 32_000, "{x} -> {y}");
        }
        let mut x = -200_000;
        while x < 200_000 {
            assert!((soft_limit(x) as i32).abs() <= 32_000);
            x += 7;
        }
    }

    #[test]
    fn it_is_monotonic_and_symmetric() {
        let mut last = i32::MIN;
        let mut x = -100_000;
        while x <= 100_000 {
            let y = soft_limit(x) as i32;
            assert!(y >= last, "not monotonic at {x}");
            last = y;
            assert_eq!(soft_limit(-x) as i32, -(soft_limit(x) as i32));
            x += 13;
        }
    }
}
