//! PINs the player let the game remember, one per server and name, in `server-pins.txt` in the
//! data folder. **They are kept in plain text**: anyone who can read that file can log in as the
//! player on those servers (hiding them would only be pretending: the game has to be able to
//! read them back, and so would anyone with the file). Deleting a line, or the file, forgets them.

use alloc::string::String;

const FILE: &str = "server-pins.txt";
const HEADER: &str = "# VoxelRust: the PINs this game keeps for servers (server, name, PIN).\n# THEY ARE IN PLAIN TEXT: anyone who can read this file can log in as you on those servers.\n# Delete a line, or this file, to forget a PIN. See docs/ACCOUNTS.md.\n";

/// A server address as the file spells it: `host:port`, lower case, the port filled in.
pub fn normalize(address: &str) -> String {
    match crate::netplay::parse_address(address) {
        Some((host, port)) => alloc::format!("{}:{}", host.to_ascii_lowercase(), port),
        None => address.trim().to_ascii_lowercase(),
    }
}

/// The PIN in `text` for this server and name.
pub fn find(text: &str, address: &str, name: &str) -> Option<String> {
    let address = normalize(address);
    text.lines().filter(|l| !l.starts_with('#')).find_map(|l| {
        let w: alloc::vec::Vec<&str> = l.split_whitespace().collect();
        match w[..] {
            [a, n, pin] if a == address && n.eq_ignore_ascii_case(name) => Some(String::from(pin)),
            _ => None,
        }
    })
}

/// `text` with this server's and name's line (if any) gone.
pub fn without(text: &str, address: &str, name: &str) -> String {
    let address = normalize(address);
    let mut out = String::new();
    for l in text.lines() {
        let w: alloc::vec::Vec<&str> = l.split_whitespace().collect();
        let mine = !l.starts_with('#') && matches!(w[..], [a, n, _] if a == address && n.eq_ignore_ascii_case(name));
        if !mine && !l.trim().is_empty() {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// `text` with the PIN for this server and name set (the header is there, and no more than 64 lines).
pub fn with_entry(text: &str, address: &str, name: &str, pin: &str) -> String {
    let body = without(text, address, name);
    let mut kept: alloc::vec::Vec<&str> = body.lines().filter(|l| !l.starts_with('#')).collect();
    if kept.len() >= 64 {
        kept.remove(0);
    }
    let mut out = String::from(HEADER);
    for l in kept {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&alloc::format!("{} {} {}\n", normalize(address), name, pin));
    out
}

fn path(base: &str) -> String {
    alloc::format!("{}{}", base, FILE)
}

fn read(base: &str) -> String {
    crate::sys::read_file(&path(base)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// The PIN remembered for this server and name, if any.
pub fn lookup(base: &str, address: &str, name: &str) -> Option<String> {
    find(&read(base), address, name)
}

/// Remember a PIN.
pub fn save(base: &str, address: &str, name: &str, pin: &str) {
    let text = with_entry(&read(base), address, name, pin);
    if crate::sys::write_file_atomic(&path(base), text.as_bytes()) {
        crate::sys::clock::make_private(&path(base));
    }
}

/// Forget a PIN.
pub fn forget(base: &str, address: &str, name: &str) {
    let text = read(base);
    if find(&text, address, name).is_some() {
        let kept = without(&text, address, name);
        let out = if kept.is_empty() { String::new() } else { alloc::format!("{}{}", HEADER, kept.lines().filter(|l| !l.starts_with('#')).map(|l| alloc::format!("{}\n", l)).collect::<String>()) };
        crate::sys::write_file_atomic(&path(base), out.as_bytes());
    }
}

/// How many PINs are remembered (for the multiplayer screen).
pub fn count(base: &str) -> usize {
    read(base).lines().filter(|l| !l.starts_with('#') && l.split_whitespace().count() == 3).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_added_replaced_found_and_forgotten() {
        let t = with_entry("", "Play.Example.org", "Kyttu", "4821");
        assert!(t.starts_with("# VoxelRust") && t.contains("PLAIN TEXT"));
        assert_eq!(find(&t, "play.example.org:25590", "kyttu"), Some(String::from("4821")));
        assert_eq!(find(&t, "play.example.org:25591", "kyttu"), None);
        assert_eq!(find(&t, "play.example.org", "bob"), None);
        let t = with_entry(&t, "play.example.org", "Kyttu", "937156");
        assert_eq!(t.matches("kyttu").count() + t.matches("Kyttu").count(), 1, "{t}");
        assert_eq!(find(&t, "play.example.org", "Kyttu"), Some(String::from("937156")));
        let t = with_entry(&t, "10.0.0.5:7000", "Kyttu", "0739");
        assert_eq!(find(&t, "10.0.0.5:7000", "kyttu"), Some(String::from("0739")));
        assert_eq!(find(&t, "play.example.org", "kyttu"), Some(String::from("937156")));
        let t = without(&t, "play.example.org", "kyttu");
        assert_eq!(find(&t, "play.example.org", "kyttu"), None);
        assert_eq!(find(&t, "10.0.0.5:7000", "kyttu"), Some(String::from("0739")));
    }

    #[test]
    fn the_file_never_grows_without_bound() {
        let mut t = String::new();
        for i in 0..100 {
            t = with_entry(&t, &alloc::format!("host{i}.example"), "me", "4821");
        }
        assert!(t.lines().filter(|l| !l.starts_with('#')).count() <= 64);
        assert_eq!(find(&t, "host99.example", "me"), Some(String::from("4821")));
        assert_eq!(find(&t, "host0.example", "me"), None);
    }
}
