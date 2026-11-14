//! The packs and the files they use as one blob a server can send to the
//! players who join. It is made only when it is needed (hosting, or a server
//! sending its own): reading and copying every picture and model of every mod
//! at start-up held the start for seconds and doubled the memory of big mods.

use alloc::string::String;
use alloc::vec::Vec;

/// What it takes to make the blob, and the blob once made.
pub struct PackText {
    /// The pack files (path, text), base data left out.
    packs: Vec<(String, String)>,
    /// The paths of the other files the packs pulled in (models, pictures, includes).
    used: Vec<String>,
    /// The blob, once made (or received from a server).
    bytes: Option<Vec<u8>>,
}

impl PackText {
    /// Nothing but the built-in content.
    pub fn none() -> PackText {
        PackText { packs: Vec::new(), used: Vec::new(), bytes: Some(Vec::new()) }
    }

    /// A blob that is already made (the one a server sent).
    pub fn ready(bytes: Vec<u8>) -> PackText {
        PackText { packs: Vec::new(), used: Vec::new(), bytes: Some(bytes) }
    }

    /// The packs that were loaded and the files they used; the blob is made when asked for.
    pub fn lazy(packs: Vec<(String, String)>, used: Vec<String>) -> PackText {
        if packs.is_empty() {
            return PackText::none();
        }
        PackText { packs, used, bytes: None }
    }

    /// The blob (empty if no packs are in use). `read` fetches a file by the path the pack named it by.
    pub fn bytes(&mut self, read: fn(&str) -> Option<Vec<u8>>) -> &[u8] {
        if self.bytes.is_none() {
            let used: Vec<(String, Vec<u8>)> = self.used.iter().filter_map(|p| read(p).map(|d| (p.clone(), d))).collect();
            self.bytes = Some(bundle(&self.packs, &used));
        }
        self.bytes.as_deref().unwrap_or(&[])
    }
}

/// The packs and the files they use as one blob a server can send. Folder
/// names are replaced by `p0/`, `p1/`...: players need the files, not to
/// know where the host keeps them.
fn bundle(packs: &[(String, String)], used: &[(String, Vec<u8>)]) -> Vec<u8> {
    if packs.is_empty() {
        return Vec::new();
    }
    let dir = |p: &str| String::from(&p[..p.rfind(|c| c == '/' || c == '\\').map_or(0, |i| i + 1)]);
    let mut dirs: Vec<String> = Vec::new();
    for (p, _) in packs {
        let d = dir(p);
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    // Longest first, so a folder inside another keeps its own name.
    dirs.sort_by(|a, b| b.len().cmp(&a.len()));
    let rename = |p: &str| match dirs.iter().position(|d| p.starts_with(d.as_str())) {
        Some(i) => alloc::format!("p{}/{}", i, &p[dirs[i].len()..]),
        None => alloc::format!("x/{}", p.replace(|c| c == '/' || c == '\\' || c == ':', "_")),
    };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (p, text) in packs {
        files.push((alloc::format!("pack:{}", rename(p)), text.as_bytes().to_vec()));
    }
    for (p, data) in used {
        files.push((alloc::format!("file:{}", rename(p)), data.clone()));
    }
    crate::save::bundle(&files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(p: &str) -> Option<Vec<u8>> {
        if p == "mods/x/models/a.bbmodel" { Some(b"{}".to_vec()) } else { None }
    }

    #[test]
    fn the_blob_is_made_on_demand_and_names_folders_by_number() {
        let mut t = PackText::lazy(
            alloc::vec![(String::from("mods/x/pack.txt"), String::from("model a file models/a.bbmodel\n"))],
            alloc::vec![String::from("mods/x/models/a.bbmodel"), String::from("gone.txt")],
        );
        let blob = t.bytes(read).to_vec();
        let files = crate::save::unbundle_labels(&blob).expect("a bundle");
        assert!(files.iter().any(|f| f.0 == "pack:p0/pack.txt"));
        assert!(files.iter().any(|f| f.0 == "file:p0/models/a.bbmodel"));
        // A file that cannot be read any more is left out, not an error.
        assert_eq!(files.len(), 2);
        // Asking again gives the same.
        assert_eq!(t.bytes(read), &blob[..]);
        assert!(PackText::none().bytes(read).is_empty());
    }
}
