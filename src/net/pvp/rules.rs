//! The server's player-versus-player switches.

/// The settings of a server (`settings.txt`: `pvp=on|off`,
/// `spawn_protection_seconds=5`); the `/pvp` command changes the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Whether players may hurt each other at all.
    pub on: bool,
    /// Seconds after joining or respawning in which nobody can hurt you.
    pub protect: f32,
}

/// Longest spawn protection a setting may give.
pub const MAX_PROTECT: f32 = 60.0;

impl Default for Rules {
    fn default() -> Rules {
        Rules { on: true, protect: 5.0 }
    }
}

/// `on`, `yes`, `true`, `1` or `off`, `no`, `false`, `0`, in any case.
pub fn parse_switch(word: &str) -> Option<bool> {
    match word.trim().to_ascii_lowercase().as_str() {
        "on" | "yes" | "true" | "1" => Some(true),
        "off" | "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

impl Rules {
    /// Read the settings text (`key=value` lines); what is missing or unreadable stays as it is by default.
    pub fn from_text(text: &str) -> Rules {
        let mut r = Rules::default();
        if let Some(on) = crate::settings::kv(text, "pvp").and_then(parse_switch) {
            r.on = on;
        }
        if let Some(s) = crate::settings::kv(text, "spawn_protection_seconds").and_then(|v| v.trim().parse::<f32>().ok()) {
            if s >= 0.0 && s.is_finite() {
                r.protect = if s > MAX_PROTECT { MAX_PROTECT } else { s };
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_pvp_on_with_five_seconds_of_protection() {
        assert_eq!(Rules::from_text(""), Rules { on: true, protect: 5.0 });
        assert_eq!(Rules::from_text("music=3\n"), Rules::default());
    }

    #[test]
    fn settings_are_read_and_bad_values_are_ignored() {
        let r = Rules::from_text("pvp=Off\nspawn_protection_seconds=12.5\n");
        assert_eq!(r, Rules { on: false, protect: 12.5 });
        assert_eq!(Rules::from_text("pvp=maybe\nspawn_protection_seconds=soon\n"), Rules::default());
        assert_eq!(Rules::from_text("spawn_protection_seconds=-3\n").protect, 5.0);
        assert_eq!(Rules::from_text("spawn_protection_seconds=9999\n").protect, MAX_PROTECT);
        assert_eq!(parse_switch("YES"), Some(true));
        assert_eq!(parse_switch("0"), Some(false));
        assert_eq!(parse_switch(""), None);
    }
}
