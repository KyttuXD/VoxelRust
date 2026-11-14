//! Reading the accounts file when a server starts and writing it as it changes
//! (the server loop calls [`flush`] every round; it writes at most every ten seconds).

use super::config::{Config, Require};
use crate::server::Server;
use alloc::string::String;

/// Where the accounts are kept, from where the world's edits are (next to `.server-ops.txt`).
pub fn accounts_path(edits_path: &str) -> String {
    edits_path.replace(".server-edits.txt", ".server-accounts.txt")
}

/// Set up accounts on a server that is starting: the settings (`settings.txt` in `base`, and the
/// command line's `--require-accounts`), and the accounts from their file.
pub fn load(server: &mut Server, base: &str, edits_path: &str, require_override: Option<Require>) {
    let settings = crate::sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    server.acct.cfg = Config::from_text(&settings, require_override);
    if let Some(text) = crate::sys::read_file(&accounts_path(edits_path)) {
        server.acct.accounts.load_text(&String::from_utf8_lossy(&text));
    }
    let c = &server.acct.cfg;
    logf!(
        "voxelrust: accounts: {} (require_accounts={}), {} account{}, administrators: {}",
        if c.require == Require::No { "off" } else { "on" },
        match c.require {
            Require::No => "no",
            Require::Optional => "optional",
            Require::Yes => "yes",
        },
        server.acct.accounts.len(),
        if server.acct.accounts.len() == 1 { "" } else { "s" },
        if c.admins.is_empty() { String::from("none") } else { c.admins.join(", ") }
    );
}

/// Write the accounts if they changed (and ten seconds have passed since the last write, unless `now`).
pub fn flush(server: &mut Server, edits_path: &str, now: bool) {
    if !server.acct.accounts.dirty || (!now && server.acct.write_wait > 0.0) {
        return;
    }
    let path = accounts_path(edits_path);
    if crate::sys::write_file_private(&path, server.acct.accounts.to_text().as_bytes()) {
        server.acct.accounts.dirty = false;
    } else {
        logf!("voxelrust: could not write the accounts file; trying again in 10 seconds");
    }
    server.acct.write_wait = 10.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accounts_file_sits_next_to_the_operators() {
        assert_eq!(accounts_path("data/worlds/server.server-edits.txt"), "data/worlds/server.server-accounts.txt");
    }
}
