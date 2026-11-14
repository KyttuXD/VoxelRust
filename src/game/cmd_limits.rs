//! How much a command may repeat itself. `/repeat 64 /repeat 64 ...` nested
//! as deep as `/if` and `/repeat` allow would run 64^6 commands and hang the
//! game, so the count shrinks with the depth.

/// The most times `/repeat` may run its command at nesting `depth` (0 is a
/// typed command, 1 an action of a pack's `when` line).
pub fn repeat_cap(depth: u32) -> u32 {
    match depth {
        0 | 1 => 64,
        2 => 8,
        _ => 2,
    }
}

/// The most commands one typed command can start through nested repeats
/// (depth 0 to 5, as the runner allows).
pub fn worst_case() -> u64 {
    (0..6u32).map(|d| repeat_cap(d) as u64).product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_repeats_cannot_run_away() {
        assert!(worst_case() <= 1_000_000, "{}", worst_case());
        assert_eq!(repeat_cap(0), 64);
        assert!(repeat_cap(5) >= 1);
    }
}
