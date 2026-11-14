//! The accounts of one server and the file they are kept in
//! (`<world>.server-accounts.txt`, next to the operators' file).
//!
//! One line per account, fields separated by spaces:
//!
//! ```text
//! name kind salt verifier rounds created fails level locked_until last_fail
//! kyttu pin 000102...0f 9f3a...e1 20000 1767225600 0 0 0 0
//! ```
//!
//! `name` is lower case; `kind` says how the account is proved (only `pin` now: a
//! password will be another kind); `salt` is 16 random bytes and `verifier` the 32 bytes
//! the PIN makes with that salt (see `keys.rs`), both in hex; the times are seconds since
//! 1970; the last four are the record of wrong tries (see `strikes.rs`). The PIN itself is
//! never stored. Lines that cannot be read are skipped.

use super::keys::{hex, unhex, Key, Salt};
use super::strikes::Strikes;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Most accounts a server keeps (a hostile player must not grow the file for ever).
pub const MAX_ACCOUNTS: usize = 10_000;
/// The kind of account a PIN makes.
pub const KIND_PIN: &str = "pin";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Lower case.
    pub name: String,
    pub kind: String,
    pub salt: Salt,
    pub key: Key,
    pub rounds: u32,
    /// Seconds since 1970 when it was made.
    pub created: u64,
    pub strikes: Strikes,
}

#[derive(Default)]
pub struct Accounts {
    map: BTreeMap<String, Account>,
    /// Something changed since the file was written.
    pub dirty: bool,
}

/// A name as accounts know it: lower case, and only what a player name may hold. `None` if
/// nothing is left.
pub fn account_name(name: &str) -> Option<String> {
    let n = crate::settings::clean_name(name, 16).to_ascii_lowercase();
    if n.is_empty() || n.len() != name.len() {
        None
    } else {
        Some(n)
    }
}

impl Accounts {
    pub fn get(&self, name: &str) -> Option<&Account> {
        self.map.get(&name.to_ascii_lowercase())
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Account> {
        self.map.get_mut(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Account> {
        self.map.values()
    }

    /// Add or replace an account (not past [`MAX_ACCOUNTS`] for a new one). False if refused.
    pub fn put(&mut self, a: Account) -> bool {
        let Some(name) = account_name(&a.name) else { return false };
        if !self.map.contains_key(&name) && self.map.len() >= MAX_ACCOUNTS {
            return false;
        }
        self.map.insert(name.clone(), Account { name, ..a });
        self.dirty = true;
        true
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let gone = self.map.remove(&name.to_ascii_lowercase()).is_some();
        self.dirty |= gone;
        gone
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut t = String::from("# VoxelRust accounts: name kind salt verifier rounds created fails level locked_until last_fail\n# The verifier is made from the PIN and salt; the PIN itself is never kept. See docs/ACCOUNTS.md.\n");
        for a in self.map.values() {
            t.push_str(&alloc::format!(
                "{} {} {} {} {} {} {} {} {} {}\n",
                a.name,
                a.kind,
                hex(&a.salt),
                hex(&a.key),
                a.rounds,
                a.created,
                a.strikes.fails,
                a.strikes.level,
                a.strikes.locked_until,
                a.strikes.last
            ));
        }
        t
    }

    /// Read what [`Accounts::to_text`] wrote (adding to what is there). Returns how many lines were taken.
    pub fn load_text(&mut self, text: &str) -> usize {
        let mut taken = 0;
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let w: alloc::vec::Vec<&str> = line.split_whitespace().collect();
            if w.len() < 6 {
                continue;
            }
            let (Some(name), Some(salt), Some(key)) = (account_name(w[0]), unhex::<16>(w[2]), unhex::<32>(w[3])) else { continue };
            let num = |k: usize| w.get(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            let a = Account {
                name,
                kind: String::from(w[1]),
                salt,
                key,
                rounds: w[4].parse().unwrap_or(super::keys::ROUNDS),
                created: num(5),
                strikes: Strikes { fails: num(6).min(1000) as u32, level: num(7).min(1000) as u32, locked_until: num(8), last: num(9) },
            };
            if self.put(a) {
                taken += 1;
            }
        }
        self.dirty = false;
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Account {
        Account { name: String::from(name), kind: String::from(KIND_PIN), salt: [7; 16], key: [9; 32], rounds: 20_000, created: 1_700_000_000, strikes: Strikes { fails: 2, level: 1, locked_until: 1_700_000_100, last: 1_700_000_050 } }
    }

    #[test]
    fn the_file_round_trips_and_never_holds_a_pin() {
        let mut a = Accounts::default();
        assert!(a.put(sample("Kyttu")));
        assert!(a.put(sample("bob_2")));
        let text = a.to_text();
        assert!(a.dirty);
        // Names are lower case; the PIN of a real account would not be in here: the verifier is a hash of it.
        assert!(text.contains("\nkyttu pin 07070707070707070707070707070707 "));
        let mut b = Accounts::default();
        assert_eq!(b.load_text(&text), 2);
        assert!(!b.dirty);
        assert_eq!(b.get("KYTTU"), a.get("kyttu"));
        assert_eq!(b.get("bob_2").map(|x| x.strikes), Some(Strikes { fails: 2, level: 1, locked_until: 1_700_000_100, last: 1_700_000_050 }));
        assert_eq!(b.to_text(), text);
    }

    #[test]
    fn a_real_account_file_does_not_contain_the_pin() {
        let salt = [3u8; 16];
        let key = super::super::keys::derive_key("4821", &salt, 1000);
        let mut a = Accounts::default();
        a.put(Account { name: String::from("ann"), kind: String::from(KIND_PIN), salt, key, rounds: 1000, created: 5, strikes: Strikes::default() });
        let text = a.to_text();
        assert!(!text.contains("4821"), "{text}");
        assert!(text.contains(&hex(&key)));
    }

    #[test]
    fn bad_lines_are_skipped_and_missing_numbers_are_zero() {
        let good = alloc::format!("ann pin {} {} 20000 5", hex(&[1u8; 16]), hex(&[2u8; 32]));
        let text = alloc::format!("# comment\n\nshort line\nbad name! pin {} {} 20000 5\nbob pin zz {} 1 1\n{}\n", hex(&[1u8; 16]), hex(&[2u8; 32]), hex(&[2u8; 32]), good);
        let mut a = Accounts::default();
        assert_eq!(a.load_text(&text), 1);
        let ann = a.get("ann").unwrap();
        assert_eq!((ann.created, ann.strikes), (5, Strikes::default()));
        assert!(a.get("bob").is_none());
    }

    #[test]
    fn names_are_checked_and_the_count_is_capped() {
        assert_eq!(account_name("Kyttu"), Some(String::from("kyttu")));
        assert_eq!(account_name("a b"), None);
        assert_eq!(account_name(""), None);
        assert_eq!(account_name("aaaaaaaaaaaaaaaaa"), None);
        let mut a = Accounts::default();
        assert!(!a.put(sample("bad name")));
        for i in 0..MAX_ACCOUNTS {
            assert!(a.put(sample(&alloc::format!("p{i}"))));
        }
        assert!(!a.put(sample("one_more")));
        // Changing one that is there still works.
        assert!(a.put(sample("p5")));
        assert!(a.remove("P5") && !a.remove("p5"));
    }
}
