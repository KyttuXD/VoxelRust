//! `voxelrust --server --data DIR --set-pin NAME PIN`: the owner of a server sets an account's
//! PIN from the machine itself (writing the accounts file and leaving), and logs in with it
//! the usual way. This is how an administrator's account (see `admins=` in docs/ACCOUNTS.md)
//! comes to be, since nobody can register a reserved name by joining.

use super::config::Config;
use super::rng::Rng;
use super::store::{account_name, Account, Accounts, KIND_PIN};
use super::strikes::Strikes;
use super::{keys, pin};
use alloc::string::String;

/// The accounts file's new text after setting `name`'s PIN, and what to tell the owner. The PIN is
/// checked like any other (`allow_weak` is the server's `allow_weak_pins`). An account that was
/// there keeps its creation time and loses its lockout.
pub fn apply(file: &str, name: &str, new_pin: &str, cfg: &Config, rng: &mut Rng, now: u64) -> Result<(String, String), String> {
    let Some(lower) = account_name(name) else {
        return Err(alloc::format!("`{}` is not a name a player can have (letters, digits, _ and -, 16 at most)", name));
    };
    pin::check(new_pin, cfg.allow_weak_pins)?;
    let mut accounts = Accounts::default();
    accounts.load_text(file);
    let created = accounts.get(&lower).map_or(now, |a| a.created);
    let salt = rng.bytes16();
    let key = keys::derive_key(new_pin, &salt, cfg.rounds);
    let existed = accounts.get(&lower).is_some();
    accounts.put(Account { name: lower.clone(), kind: String::from(KIND_PIN), salt, key, rounds: cfg.rounds, created, strikes: Strikes::default() });
    let admin = if cfg.is_admin_name(&lower) { " (an administrator: its name is in `admins`)" } else { "" };
    let msg = alloc::format!("{} the account `{}`{}: log in with that PIN", if existed { "changed the PIN of" } else { "made" }, lower, admin);
    Ok((accounts.to_text(), msg))
}

/// Do it for the world `world` in the data folder `base`.
pub fn run(base: &str, world: &str, name: &str, new_pin: &str) -> Result<(), String> {
    let dir = crate::session::world_dir(base);
    crate::sys::make_dirs(&dir);
    let path = super::persist::accounts_path(&alloc::format!("{}{}.server-edits.txt", dir, world));
    let file = crate::sys::read_file(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let settings = crate::sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let cfg = Config::from_text(&settings, None);
    let (text, msg) = apply(&file, name, new_pin, &cfg, &mut Rng::default(), crate::sys::clock::unix_seconds())?;
    if !crate::sys::write_file_atomic(&path, text.as_bytes()) {
        return Err(alloc::format!("could not write {}", path));
    }
    crate::sys::clock::make_private(&path);
    logf!("voxelrust: {} ({})", msg, path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_is_made_and_changed_and_the_file_never_has_the_pin() {
        let cfg = Config::from_text("admins=KYTTU\n", None);
        let mut rng = Rng::from_seed(b"test");
        let (text, msg) = apply("", "Kyttu", "482915", &cfg, &mut rng, 1000).unwrap();
        assert!(msg.contains("made the account `kyttu`") && msg.contains("administrator"), "{msg}");
        assert!(!text.contains("482915"));
        let mut a = Accounts::default();
        assert_eq!(a.load_text(&text), 1);
        let acc = a.get("kyttu").unwrap();
        assert_eq!((acc.created, acc.kind.as_str(), acc.rounds), (1000, "pin", keys::ROUNDS));
        assert_eq!(acc.key, keys::derive_key("482915", &acc.salt, acc.rounds));
        // Again, later: a new salt and key, the old creation time.
        let (text2, msg2) = apply(&text, "KYTTU", "0739", &cfg, &mut rng, 5000).unwrap();
        assert!(msg2.contains("changed the PIN of"), "{msg2}");
        let mut b = Accounts::default();
        assert_eq!(b.load_text(&text2), 1);
        let acc2 = b.get("kyttu").unwrap();
        assert_eq!(acc2.created, 1000);
        assert_ne!(acc2.salt, acc.salt);
        assert_eq!(acc2.key, keys::derive_key("0739", &acc2.salt, acc2.rounds));
    }

    #[test]
    fn bad_names_and_pins_are_refused() {
        let cfg = Config::default();
        let mut rng = Rng::from_seed(b"t");
        assert!(apply("", "bad name", "482915", &cfg, &mut rng, 1).is_err());
        assert!(apply("", "", "482915", &cfg, &mut rng, 1).is_err());
        assert!(apply("", "ann", "1234", &cfg, &mut rng, 1).unwrap_err().contains("too easy"));
        assert!(apply("", "ann", "12", &cfg, &mut rng, 1).is_err());
        let weak_ok = Config::from_text("allow_weak_pins=yes\n", None);
        assert!(apply("", "ann", "1234", &weak_ok, &mut rng, 1).is_ok());
    }
}
