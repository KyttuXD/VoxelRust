//! What a PIN may be: exactly 4 or 6 digits, and not one that is the first thing anybody tries.

use alloc::string::String;

/// Whether `pin` is a PIN of the right shape: 4 or 6 digits and nothing else.
pub fn well_formed(pin: &str) -> bool {
    (pin.len() == 4 || pin.len() == 6) && pin.bytes().all(|b| b.is_ascii_digit())
}

/// Whether a well formed PIN is one of the trivial ones: one digit repeated
/// (0000, 777777), counting up or down by one (1234, 123456, 4321, 7890, also
/// round the corner as in 8901), a short pattern repeated (1212, 123123), or a
/// few that are famous (2580 is the middle column of a keypad).
pub fn is_weak(pin: &str) -> bool {
    let d: alloc::vec::Vec<i32> = pin.bytes().map(|b| (b as i32) - (b'0' as i32)).collect();
    if d.len() < 4 {
        return true;
    }
    if d.iter().all(|&x| x == d[0]) {
        return true;
    }
    let step_all = |step: i32| d.windows(2).all(|w| (w[1] - w[0]).rem_euclid(10) == step.rem_euclid(10));
    if step_all(1) || step_all(-1) {
        return true;
    }
    let half = d.len() / 2;
    if d.len() % 2 == 0 && d[..half] == d[half..] {
        return true;
    }
    if d.len() % 3 == 0 && d[..d.len() / 3] == d[d.len() / 3..d.len() / 3 * 2] && d[..d.len() / 3] == d[d.len() / 3 * 2..] {
        return true;
    }
    ["2580", "0852", "1357", "2468", "1004", "2000", "1122", "1212"].contains(&pin)
}

/// Check a PIN someone chose. `Err` is a polite message (lower case; the game shows it in capitals).
pub fn check(pin: &str, allow_weak: bool) -> Result<(), String> {
    if !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err(String::from("a PIN is made of digits only"));
    }
    if !well_formed(pin) {
        return Err(String::from("a PIN is exactly 4 or 6 digits"));
    }
    if !allow_weak && is_weak(pin) {
        return Err(String::from("that PIN is too easy to guess (not 1234, 0000, 123456 and the like): please pick another"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_four_or_six_digits_pass() {
        for ok in ["4821", "937156", "0739"] {
            assert!(check(ok, false).is_ok(), "{ok}");
        }
        for bad in ["", "1", "482", "48213", "4821567", "48a1", "48 21", "-4821", "４８２１", "48.1"] {
            assert!(check(bad, true).is_err(), "{bad}");
        }
    }

    #[test]
    fn trivial_pins_are_refused_politely_unless_the_server_allows_them() {
        for weak in ["1234", "0000", "1111", "123456", "000000", "654321", "4321", "9876", "7890", "8901", "1212", "123123", "121212", "2580", "0123", "999999", "2468"] {
            assert!(is_weak(weak), "{weak}");
            let e = check(weak, false).unwrap_err();
            assert!(e.contains("too easy"), "{e}");
            assert!(check(weak, true).is_ok(), "{weak} is allowed when the server says so");
        }
        for fine in ["4821", "937156", "1358", "5902", "314159"] {
            assert!(!is_weak(fine), "{fine}");
        }
        // The shape is checked even where weak PINs are allowed.
        assert!(check("12", true).is_err());
    }
}
