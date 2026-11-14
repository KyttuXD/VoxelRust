//! Short forms of commands, turned into the full ones before they run:
//! `/gm c`, `/gm 1`, `/gmc`, `/gms`, `/gamemode s` and the like.

use alloc::string::String;

/// The mode a word names (`c`, `creative`, `1`, `s`, `survival`, `0`), as `creative` or `survival`.
fn mode_word(w: &str) -> Option<&'static str> {
    match w {
        "c" | "creative" | "1" => Some("creative"),
        "s" | "survival" | "0" => Some("survival"),
        _ => None,
    }
}

/// `line` (already lower case, starting with `/`) with an alias spelled out. `online` is whether
/// this is a multiplayer game (the server's command is `/gamemode`, a single player's is `/mode`).
/// A line that is not an alias comes back as it was.
pub fn expand(line: &str, online: bool) -> String {
    let words: alloc::vec::Vec<&str> = line.split_whitespace().collect();
    let first = match words.first() {
        Some(f) => *f,
        None => return String::from(line),
    };
    // /gm, /gamemode: a mode word first (or after a player's name for operators).
    let (mode, who): (Option<&str>, Option<&str>) = match first {
        "/gmc" => (Some("creative"), words.get(1).copied()),
        "/gms" => (Some("survival"), words.get(1).copied()),
        "/gm" | "/gamemode" | "/mode" => match (words.get(1).and_then(|w| mode_word(w)), words.get(2).and_then(|w| mode_word(w))) {
            (Some(m), _) => (Some(m), words.get(2).copied()),
            (None, Some(m)) => (Some(m), words.get(1).copied()),
            _ => return String::from(line),
        },
        _ => return String::from(line),
    };
    let mode = match mode {
        Some(m) => m,
        None => return String::from(line),
    };
    let command = if online { "/gamemode" } else { "/mode" };
    match who {
        // On a server an operator may change someone else's mode: `/gamemode NAME creative`.
        Some(n) if online => alloc::format!("{} {} {}", command, n, mode),
        _ => alloc::format!("{} {}", command, mode),
    }
}

#[cfg(test)]
mod tests {
    use super::expand;

    #[test]
    fn short_game_mode_commands_become_the_full_ones() {
        assert_eq!(expand("/gm c", true), "/gamemode creative");
        assert_eq!(expand("/gm 1", true), "/gamemode creative");
        assert_eq!(expand("/gm s", true), "/gamemode survival");
        assert_eq!(expand("/gm 0", false), "/mode survival");
        assert_eq!(expand("/gmc", false), "/mode creative");
        assert_eq!(expand("/gms", true), "/gamemode survival");
        assert_eq!(expand("/gamemode c", true), "/gamemode creative");
        assert_eq!(expand("/gamemode creative", true), "/gamemode creative");
        // An operator names the player, either way round.
        assert_eq!(expand("/gm bob c", true), "/gamemode bob creative");
        assert_eq!(expand("/gm c bob", true), "/gamemode bob creative");
        assert_eq!(expand("/gmc bob", true), "/gamemode bob creative");
        // Not aliases: left alone.
        assert_eq!(expand("/gm", true), "/gm");
        assert_eq!(expand("/gm what", true), "/gm what");
        assert_eq!(expand("/time day", false), "/time day");
        assert_eq!(expand("", false), "");
    }
}
