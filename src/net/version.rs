//! Which program this is and what to tell a player when a server and a
//! client do not match.
//!
//! Only the protocol version (`proto::VERSION`) decides whether a client may
//! join a server. The build name (`linux-server-x86_64`, `windows-x86_64`...)
//! says which kind of computer a program is for and has nothing to do with
//! compatibility; the release number says how new the program is, which is
//! worth showing when somebody wonders why a server looks different.

use alloc::string::String;

/// What a program says about itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub build: String,
    /// The release number; 0 for a program built by hand.
    pub release: u32,
    pub commit: String,
}

/// This program.
pub fn local() -> Info {
    Info { build: String::from(crate::app::BUILD), release: crate::update::SEQ.unwrap_or(0), commit: String::from(crate::update::COMMIT) }
}

/// `release 123 (linux-x86_64, commit abc1234)`.
pub fn describe(i: &Info) -> String {
    let release = if i.release == 0 { String::from("a hand-built program") } else { alloc::format!("release {}", i.release) };
    let commit: String = i.commit.chars().take(8).collect();
    alloc::format!("{} ({}, commit {})", release, i.build, commit)
}

/// The reason a server gives a client whose protocol version differs: both
/// versions, and which of the two must update. At most 200 bytes (what a
/// message can carry).
pub fn mismatch_reason(client: u16, server: u16, server_info: &Info) -> String {
    let who = if client < server { "your game is too old: update it" } else { "this server is too old for your game: its owner must update it" };
    let release = if server_info.release == 0 { String::new() } else { alloc::format!(" release {}", server_info.release) };
    let mut text = alloc::format!("{}. you speak protocol {}, the server{} speaks {}", who, client, release, server);
    text.truncate(200);
    text
}

/// What the player sees when a server turned them away with `reason`. A
/// server from before the protocol named the versions says only that they
/// differ: then this adds what is known here.
pub fn explain_refusal(reason: &str) -> String {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("different version") {
        let own = local();
        return alloc::format!(
            "THE SERVER RUNS A DIFFERENT VERSION: IT IS NOT THE ONE YOUR GAME SPEAKS (PROTOCOL {}, {}). ONE OF YOU MUST UPDATE, MOST LIKELY THE SERVER'S OWNER IF YOUR GAME IS THE NEWEST",
            crate::proto::VERSION,
            describe(&own).to_ascii_uppercase()
        );
    }
    reason.to_ascii_uppercase()
}

/// A line comparing this program with the server's, when they are different
/// releases of the same protocol (they work together, but one is behind).
pub fn compare(own: &Info, server: &Info) -> Option<String> {
    if own.release == 0 || server.release == 0 || own.release == server.release {
        return None;
    }
    Some(if server.release < own.release {
        alloc::format!("THE SERVER IS AN OLDER RELEASE ({}) THAN YOUR GAME ({}): IT WORKS, BUT IT NEEDS UPDATING", server.release, own.release)
    } else {
        alloc::format!("THE SERVER IS A NEWER RELEASE ({}) THAN YOUR GAME ({}): IT WORKS, BUT UPDATE YOUR GAME", server.release, own.release)
    })
}

/// The answer to `/version`: this program and, when connected, the server's.
pub fn report(server: Option<&Info>) -> String {
    let own = local();
    let mut text = alloc::format!("YOUR GAME: {} PROTOCOL {}", describe(&own), crate::proto::VERSION).to_ascii_uppercase();
    match server {
        Some(s) => {
            text.push_str(&alloc::format!("   SERVER: {}", describe(s).to_ascii_uppercase()));
            if let Some(line) = compare(&own, s) {
                text.push_str("   ");
                text.push_str(&line);
            }
        }
        None => {}
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(build: &str, release: u32) -> Info {
        Info { build: String::from(build), release, commit: String::from("0123456789abcdef") }
    }

    #[test]
    fn a_mismatch_names_both_versions_and_who_must_update() {
        let s = info("linux-server-x86_64", 321);
        let old_client = mismatch_reason(11, 12, &s);
        assert!(old_client.contains("your game is too old") && old_client.contains("protocol 11") && old_client.contains("speaks 12") && old_client.contains("release 321"), "{old_client}");
        let old_server = mismatch_reason(13, 12, &s);
        assert!(old_server.contains("server is too old") && old_server.contains("owner must update"), "{old_server}");
        assert!(mismatch_reason(1, 2, &info("x", 0)).len() <= 200);
        assert!(!mismatch_reason(1, 2, &info("x", 0)).contains("release"));
    }

    #[test]
    fn the_build_name_is_shown_but_does_not_matter() {
        assert_eq!(describe(&info("linux-server-x86_64", 7)), "release 7 (linux-server-x86_64, commit 01234567)");
        assert!(describe(&info("windows-x86_64", 0)).starts_with("a hand-built program"));
        // A server and a client of different kinds, same release: nothing to say.
        assert_eq!(compare(&info("linux-server-x86_64", 9), &info("windows-x86_64", 9)), None);
        assert_eq!(compare(&info("a", 0), &info("b", 9)), None);
        assert!(compare(&info("a", 10), &info("b", 9)).unwrap().contains("OLDER RELEASE (9)"));
        assert!(compare(&info("a", 9), &info("b", 10)).unwrap().contains("NEWER RELEASE (10)"));
    }

    #[test]
    fn an_old_servers_vague_refusal_gets_what_is_known() {
        let t = explain_refusal("this server runs a different version of the game");
        assert!(t.contains("PROTOCOL") && t.contains("UPDATE"), "{t}");
        assert_eq!(explain_refusal("the server is full"), "THE SERVER IS FULL");
    }

    #[test]
    fn the_report_has_both_sides() {
        let r = report(Some(&info("linux-server-x86_64", 5)));
        assert!(r.contains("YOUR GAME") && r.contains("SERVER: RELEASE 5"), "{r}");
        assert!(!report(None).contains("SERVER"));
    }
}
