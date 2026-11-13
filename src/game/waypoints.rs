//! Waypoints: named places on a world's map, kept with the world.
//!
//! A world's waypoints are in `worlds/<name>.waypoints.txt`, one per line:
//! `x z colour name`. The name comes last, so it may contain spaces. Lines
//! that do not parse are skipped, so a damaged file loses only its bad lines.

use alloc::string::String;
use alloc::vec::Vec;

/// Most waypoints one world can have made on the map. A file that holds more
/// is read in full, so nothing is lost when it is saved again.
pub const MAX: usize = 64;
/// Longest name, in characters.
pub const NAME_MAX: usize = 24;
/// The colours a waypoint can have, by index.
pub const COLOURS: [[u8; 3]; 8] = [
    [236, 72, 64],
    [250, 178, 52],
    [240, 226, 72],
    [96, 210, 96],
    [72, 196, 230],
    [96, 128, 236],
    [186, 110, 230],
    [240, 240, 240],
];

#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    /// Block coordinates.
    pub x: i32,
    pub z: i32,
    /// An index into [`COLOURS`].
    pub colour: u8,
    pub name: String,
}

/// A name the file can hold: printable ASCII and spaces, trimmed, at most
/// [`NAME_MAX`] characters. An empty name becomes "WAYPOINT".
pub fn clean_name(s: &str) -> String {
    let kept: String = s.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(NAME_MAX).collect();
    let trimmed = kept.trim();
    if trimmed.is_empty() {
        String::from("WAYPOINT")
    } else {
        String::from(trimmed)
    }
}

/// The list as text, one waypoint per line.
pub fn format(list: &[Waypoint]) -> String {
    let mut out = String::new();
    for w in list {
        out.push_str(&alloc::format!("{} {} {} {}\n", w.x, w.z, w.colour.min(COLOURS.len() as u8 - 1), clean_name(&w.name)));
    }
    out
}

/// The first word of `s` and what follows it (leading spaces skipped).
fn word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

/// The list read from text. Bad lines are skipped; the colour is clamped.
pub fn parse(text: &str) -> Vec<Waypoint> {
    let mut out = Vec::new();
    for line in text.lines() {
        let (x, rest) = word(line);
        let (z, rest) = word(rest);
        let (colour, name) = word(rest);
        let (Ok(x), Ok(z), Ok(colour)) = (x.parse::<i32>(), z.parse::<i32>(), colour.parse::<i32>()) else { continue };
        out.push(Waypoint { x, z, colour: colour.clamp(0, COLOURS.len() as i32 - 1) as u8, name: clean_name(name) });
    }
    out
}

/// The index of the waypoint nearest to (x, z), if one is within `radius`
/// blocks (measured straight across).
pub fn nearest(list: &[Waypoint], x: f32, z: f32, radius: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, w) in list.iter().enumerate() {
        let (dx, dz) = (w.x as f32 + 0.5 - x, w.z as f32 + 0.5 - z);
        let d = if dx < 0.0 { -dx } else { dx }.max(if dz < 0.0 { -dz } else { dz });
        if d <= radius && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wp(x: i32, z: i32, colour: u8, name: &str) -> Waypoint {
        Waypoint { x, z, colour, name: String::from(name) }
    }

    #[test]
    fn a_list_survives_a_round_trip_with_spaces_in_names() {
        let list = alloc::vec![wp(-120, 45, 0, "Home base"), wp(7, -9, 7, "River crossing")];
        assert_eq!(parse(&format(&list)), list);
    }

    #[test]
    fn names_are_cleaned_and_limited() {
        assert_eq!(clean_name("  Hello\tthere\n  "), "Hellothere");
        assert_eq!(clean_name(""), "WAYPOINT");
        assert_eq!(clean_name("   "), "WAYPOINT");
        assert_eq!(clean_name("abcdefghijklmnopqrstuvwxyz").chars().count(), NAME_MAX);
        assert_eq!(clean_name("ünïcode"), "ncode", "non-ASCII is dropped, not mangled");
    }

    #[test]
    fn bad_lines_are_skipped_and_colours_are_clamped() {
        let text = "1 2 3 Good\nnot a line\n4 5 Missing\n6 7 99 Clamped\n8 x 1 Also bad\n-3 -4 -1 Negative colour\n";
        let list = parse(text);
        assert_eq!(list, alloc::vec![wp(1, 2, 3, "Good"), wp(6, 7, 7, "Clamped"), wp(-3, -4, 0, "Negative colour")]);
    }

    #[test]
    fn a_file_with_more_than_the_maximum_keeps_every_line() {
        let mut text = String::new();
        for i in 0..(MAX + 10) {
            text.push_str(&alloc::format!("{} 0 0 P{}\n", i, i));
        }
        assert_eq!(parse(&text).len(), MAX + 10, "saving must not drop waypoints");
        let many: Vec<Waypoint> = (0..(MAX as i32 + 5)).map(|i| wp(i, 0, 0, "w")).collect();
        assert_eq!(parse(&format(&many)).len(), MAX + 5);
    }

    #[test]
    fn extra_spaces_do_not_lose_the_name() {
        assert_eq!(parse("5  6 2 Home base")[0].name, "Home base");
        assert_eq!(parse("  5 6 2 Home base")[0].name, "Home base");
        assert_eq!(parse("5\t6   2\tMine entrance")[0].name, "Mine entrance");
    }

    #[test]
    fn the_nearest_waypoint_within_reach_is_picked() {
        let list = alloc::vec![wp(0, 0, 0, "a"), wp(10, 10, 0, "b"), wp(-30, 0, 0, "c")];
        assert_eq!(nearest(&list, 1.0, 1.0, 6.0), Some(0));
        assert_eq!(nearest(&list, 10.0, 12.0, 6.0), Some(1));
        assert_eq!(nearest(&list, 5.0, 5.0, 2.0), None, "too far from every waypoint");
        assert_eq!(nearest(&list, -29.0, 0.0, 6.0), Some(2), "negative coordinates work too");
    }
}
