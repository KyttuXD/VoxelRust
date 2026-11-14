//! The server's account settings, from its `settings.txt` (and `--require-accounts`):
//!
//! ```text
//! require_accounts=optional     yes | no | optional (the default)
//! admins=KYTTU                  names reserved for administrators, comma separated
//! allow_weak_pins=no            yes lets players choose 1234 and the like
//! ```

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Require {
    /// Accounts are not offered: nobody can register; names that have an account
    /// (the administrators') still log in.
    No,
    /// Anyone may play as a guest, or register a name and protect it with a PIN.
    Optional,
    /// Every player needs an account; a new name must register when it joins.
    Yes,
}

impl Require {
    pub fn parse(text: &str) -> Option<Require> {
        match text.trim().to_ascii_lowercase().as_str() {
            "yes" | "true" | "on" | "required" => Some(Require::Yes),
            "no" | "false" | "off" | "none" => Some(Require::No),
            "optional" | "" => Some(Require::Optional),
            _ => None,
        }
    }

    /// The number the protocol uses (0 no, 1 optional, 2 yes).
    pub fn code(self) -> u8 {
        match self {
            Require::No => 0,
            Require::Optional => 1,
            Require::Yes => 2,
        }
    }
}

/// `--require-accounts` on the command line (0 = not given, else the mode's code plus one).
static OVERRIDE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Remember the mode the command line asked for.
pub fn set_override(mode: Require) {
    OVERRIDE.store(mode.code() + 1, core::sync::atomic::Ordering::Relaxed);
}

/// The mode the command line asked for, if it did.
pub fn override_value() -> Option<Require> {
    match OVERRIDE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => Some(Require::No),
        2 => Some(Require::Optional),
        3 => Some(Require::Yes),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub require: Require,
    /// Lower case.
    pub admins: Vec<String>,
    pub allow_weak_pins: bool,
    /// How many times a new PIN is hashed.
    pub rounds: u32,
}

impl Default for Config {
    fn default() -> Config {
        Config { require: Require::Optional, admins: Vec::new(), allow_weak_pins: false, rounds: super::keys::ROUNDS }
    }
}

impl Config {
    /// Read the settings text (`key=value` lines); `require_override` (the command line) wins.
    pub fn from_text(text: &str, require_override: Option<Require>) -> Config {
        let get = |k: &str| crate::settings::kv(text, k).unwrap_or("").trim();
        let admins = get("admins").split(',').filter_map(|n| super::store::account_name(n.trim())).collect();
        Config {
            require: require_override.or_else(|| Require::parse(get("require_accounts"))).unwrap_or(Require::Optional),
            admins,
            allow_weak_pins: matches!(get("allow_weak_pins").to_ascii_lowercase().as_str(), "yes" | "true" | "on" | "1"),
            rounds: super::keys::ROUNDS,
        }
    }

    /// Whether `name` (any case) is reserved for an administrator.
    pub fn is_admin_name(&self, name: &str) -> bool {
        self.admins.iter().any(|a| a.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read_with_safe_defaults() {
        let d = Config::from_text("", None);
        assert_eq!(d, Config::default());
        assert_eq!(d.require, Require::Optional);
        assert!(!d.allow_weak_pins && d.admins.is_empty());
        let c = Config::from_text("music=3\nrequire_accounts=Yes\nadmins=KYTTU, Second_Admin ,,bad name\nallow_weak_pins=yes\n", None);
        assert_eq!(c.require, Require::Yes);
        assert_eq!(c.admins, ["kyttu", "second_admin"]);
        assert!(c.allow_weak_pins);
        assert!(c.is_admin_name("Kyttu") && c.is_admin_name("SECOND_ADMIN") && !c.is_admin_name("bob"));
        // An unreadable value falls back to the default, and the command line wins.
        assert_eq!(Config::from_text("require_accounts=maybe\n", None).require, Require::Optional);
        assert_eq!(Config::from_text("require_accounts=yes\n", Some(Require::No)).require, Require::No);
        assert_eq!(Config::from_text("require_accounts=no\n", None).require, Require::No);
    }

    #[test]
    fn the_modes_have_protocol_numbers() {
        assert_eq!([Require::No.code(), Require::Optional.code(), Require::Yes.code()], [0, 1, 2]);
        assert_eq!(Require::parse("OPTIONAL"), Some(Require::Optional));
        assert_eq!(Require::parse("sometimes"), None);
    }
}
