//! A mod's folder as one blob: the entries are (path inside the mod, bytes).
//! It is the world-bundle format of `save::bundle`, with every name checked
//! on the way in and on the way out.

use super::path::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Why a bundle was refused (for the log).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Refusal {
    /// Not a bundle at all.
    Damaged,
    BadPath,
    Duplicate,
    TooManyFiles,
    FileTooBig,
    ModTooBig,
}

impl Refusal {
    pub fn text(self) -> &'static str {
        match self {
            Refusal::Damaged => "damaged",
            Refusal::BadPath => "a file name is not allowed",
            Refusal::Duplicate => "a file is there twice",
            Refusal::TooManyFiles => "too many files",
            Refusal::FileTooBig => "a file is too big",
            Refusal::ModTooBig => "the mod is too big",
        }
    }
}

fn is_picture(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [".png", ".jpg", ".gif", ".bmp", ".tga"].iter().any(|e| p.ends_with(e))
}

/// Check a set of files against the rules: every path clean, none twice
/// (also ignoring case, as some file systems do), and the size limits.
pub fn check(files: &[(String, Vec<u8>)]) -> Result<(), Refusal> {
    if files.len() > MAX_FILES {
        return Err(Refusal::TooManyFiles);
    }
    let mut total = 0usize;
    let mut seen: Vec<String> = Vec::with_capacity(files.len());
    for (path, data) in files {
        if clean_rel_path(path).is_none() {
            return Err(Refusal::BadPath);
        }
        if data.len() > MAX_FILE_BYTES {
            return Err(Refusal::FileTooBig);
        }
        // A picture says its size in its header: one that promises more than the game reads
        // (a few kilobytes that would become gigabytes) is refused before anything decodes it.
        if is_picture(path) {
            if let Some((w, h)) = crate::hd::size::peek_size(data) {
                if w > crate::image::MAX_SIDE || h > crate::image::MAX_SIDE {
                    return Err(Refusal::FileTooBig);
                }
            }
        }
        total += data.len();
        if total > MAX_MOD_BYTES {
            return Err(Refusal::ModTooBig);
        }
        let lower = path.to_ascii_lowercase();
        if seen.contains(&lower) {
            return Err(Refusal::Duplicate);
        }
        seen.push(lower);
    }
    Ok(())
}

/// The files as a bundle, sorted by path so the same folder always gives the
/// same bytes (and so the same checksum). Refused if [`check`] fails.
pub fn encode(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Refusal> {
    check(files)?;
    let mut sorted: Vec<(String, Vec<u8>)> = files.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(crate::save::bundle(&sorted))
}

/// The inverse of [`encode`]. Nothing from a damaged or hostile bundle gets
/// through: any bad entry refuses the whole mod.
pub fn decode(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, Refusal> {
    if data.len() > MAX_MOD_BYTES + MAX_FILES * (MAX_PATH_LEN + 8) {
        return Err(Refusal::ModTooBig);
    }
    let files = crate::save::unbundle_labels(data).ok_or(Refusal::Damaged)?;
    check(&files)?;
    Ok(files)
}

/// The checksum a server announces for a bundle: the first 8 bytes of its
/// SHA-256.
pub fn sum_of(bundle: &[u8]) -> u64 {
    let hex = crate::sha256::hex(bundle);
    u64::from_str_radix(&hex[..16], 16).unwrap_or(0)
}

/// What a mod's `pack.txt` says about itself.
#[derive(Debug, PartialEq, Clone)]
pub struct PackMeta {
    /// `version 1.2` (empty if absent).
    pub version: String,
    /// False for `client no`: a mod only the server needs.
    pub client: bool,
}

/// Read the `version` and `client` lines of a pack (top level lines only;
/// `#` starts a comment).
pub fn read_meta(pack: &str) -> PackMeta {
    let mut meta = PackMeta { version: String::new(), client: true };
    for line in pack.lines() {
        if line.starts_with(|c: char| c == ' ' || c == '\t' || c == '#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        let (key, rest) = match line.split_once(|c: char| c == ' ' || c == '\t') {
            Some((k, r)) => (k, r.trim()),
            None => (line, ""),
        };
        match key {
            "version" if meta.version.is_empty() => {
                meta.version = rest.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')).take(16).collect();
            }
            "client" => {
                meta.client = !matches!(rest.to_ascii_lowercase().as_str(), "no" | "off" | "false" | "0");
            }
            _ => {}
        }
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(p: &str, d: &[u8]) -> (String, Vec<u8>) {
        (String::from(p), d.to_vec())
    }

    #[test]
    fn a_bundle_round_trips_and_is_stable() {
        let a = alloc::vec![f("pack.txt", b"block x\n"), f("textures/a.png", &[1, 2, 3]), f("scripts/s.rs", b"fn main() {}")];
        let mut b = a.clone();
        b.reverse();
        let (ea, eb) = (encode(&a).unwrap(), encode(&b).unwrap());
        assert_eq!(ea, eb, "the order of the files does not matter");
        assert_eq!(sum_of(&ea), sum_of(&eb));
        let mut back = decode(&ea).unwrap();
        back.sort();
        let mut want = a.clone();
        want.sort();
        assert_eq!(back, want);
        assert_ne!(sum_of(&ea), sum_of(&encode(&[f("pack.txt", b"block y\n")]).unwrap()));
        assert_eq!(decode(&encode(&[]).unwrap()).unwrap(), Vec::new());
    }

    #[test]
    fn hostile_bundles_are_refused() {
        // A path that climbs out, an absolute one, a program, a repeat.
        for bad in ["../evil.txt", "/etc/x.txt", "a/../../b.txt", "run.exe", "a\\b.txt"] {
            let raw = crate::save::bundle(&[(String::from(bad), alloc::vec![1])]);
            assert!(decode(&raw).is_err(), "{bad}");
            assert_eq!(encode(&[f(bad, b"x")]), Err(Refusal::BadPath), "{bad}");
        }
        let twice = crate::save::bundle(&[f("a.txt", b"1"), f("A.TXT", b"2")]);
        assert_eq!(decode(&twice), Err(Refusal::Duplicate));
        assert_eq!(decode(b"nope"), Err(Refusal::Damaged));
        let good = encode(&[f("a.txt", b"hello")]).unwrap();
        assert_eq!(decode(&good[..good.len() - 2]), Err(Refusal::Damaged));
        // Limits.
        let big = alloc::vec![0u8; MAX_FILE_BYTES + 1];
        assert_eq!(encode(&[f("a.png", &big)]), Err(Refusal::FileTooBig));
        let many: Vec<(String, Vec<u8>)> = (0..MAX_FILES + 1).map(|i| f(&alloc::format!("f{}.txt", i), b"")).collect();
        assert_eq!(encode(&many), Err(Refusal::TooManyFiles));
        // A picture whose header promises 60000 x 60000 pixels is refused unread.
        let mut bomb = alloc::vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        bomb.extend_from_slice(&60000u32.to_be_bytes());
        bomb.extend_from_slice(&60000u32.to_be_bytes());
        bomb.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(encode(&[f("tex/a.png", &bomb)]), Err(Refusal::FileTooBig));
        // ...and a 512 x 512 one (HD tile) is welcome.
        let hd = crate::png::encode(512, 512, &alloc::vec![5u8; 512 * 512 * 4]);
        assert!(hd.len() < MAX_FILE_BYTES);
        assert!(encode(&[f("tex/hd.png", &hd)]).is_ok());
        let chunk = alloc::vec![0u8; MAX_FILE_BYTES];
        let heavy: Vec<(String, Vec<u8>)> = (0..MAX_MOD_BYTES / MAX_FILE_BYTES + 1).map(|i| f(&alloc::format!("f{}.png", i), &chunk)).collect();
        assert_eq!(check(&heavy), Err(Refusal::ModTooBig));
    }

    #[test]
    fn the_pack_says_its_version_and_whether_clients_need_it() {
        let m = read_meta("# a mod\nversion 1.2.3\nblock x\n  version 9\nfeature y\n");
        assert_eq!(m, PackMeta { version: String::from("1.2.3"), client: true });
        assert!(!read_meta("client no\n").client);
        assert!(!read_meta("client off # server only\n").client);
        assert!(read_meta("client yes\n").client);
        assert!(read_meta("  client no\n").client, "indented lines belong to a block");
        assert_eq!(read_meta("version a/b;c\n").version, "abc");
        assert_eq!(read_meta("").version, "");
    }
}
