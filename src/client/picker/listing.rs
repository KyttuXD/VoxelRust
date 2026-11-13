//! Folder listings for the file picker: names, extensions, sorting,
//! filtering, path joining and parents, and the shortcut folders. Plain
//! functions, tested below.

use alloc::string::String;
use alloc::vec::Vec;

/// One line of a folder listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
}

/// Backslashes become slashes and a trailing slash goes, except on a root.
pub fn normalize(path: &str) -> String {
    let mut p: String = path.trim().chars().map(|c| if c == '\\' { '/' } else { c }).collect();
    while p.len() > 1 && p.ends_with('/') && !p.ends_with(":/") {
        p.pop();
    }
    p
}

/// Whether `path` is the top of a file system: `/` or a drive such as `C:/`.
pub fn is_root(path: &str) -> bool {
    let b = path.as_bytes();
    path == "/" || (b.len() == 3 && b[1] == b':' && b[2] == b'/') || (b.len() == 2 && b[1] == b':')
}

/// The folder above `path`; `None` at the top. A single relative name has
/// the current folder (`""`) above it.
pub fn parent(path: &str) -> Option<String> {
    let p = normalize(path);
    if p.is_empty() || is_root(&p) {
        return None;
    }
    match p.rfind('/') {
        // "/a" -> "/"
        Some(0) => Some(String::from("/")),
        // "C:/a" -> "C:/"
        Some(i) if p.as_bytes()[..i].ends_with(b":") => Some(String::from(&p[..=i])),
        Some(i) => Some(String::from(&p[..i])),
        None => Some(String::new()),
    }
}

/// `name` inside the folder `dir`.
pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        String::from(name)
    } else if dir.ends_with('/') {
        alloc::format!("{}{}", dir, name)
    } else {
        alloc::format!("{}/{}", dir, name)
    }
}

/// The last part of a path.
pub fn file_name(path: &str) -> &str {
    path.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(path)
}

/// The lower-case extension of a file name (without the dot).
pub fn ext_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Whether `name` has one of the extensions (lower case, no dot). An empty
/// list accepts every file.
pub fn matches_ext(name: &str, exts: &[&str]) -> bool {
    exts.is_empty() || ext_of(name).map_or(false, |e| exts.iter().any(|x| *x == e))
}

/// The listing of a folder from the names in it: hidden names (starting
/// with a dot) go, folders come first, then files with a wanted extension,
/// each group in alphabetical order regardless of case. `is_dir` is asked
/// only about names that are not wanted files.
pub fn build_entries(names: &[String], is_dir: impl Fn(&str) -> bool, exts: &[&str]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for n in names {
        if n.is_empty() || n.starts_with('.') {
            continue;
        }
        if !exts.is_empty() && matches_ext(n, exts) {
            out.push(Entry { name: n.clone(), dir: false });
        } else if is_dir(n) {
            out.push(Entry { name: n.clone(), dir: true });
        } else if exts.is_empty() {
            out.push(Entry { name: n.clone(), dir: false });
        }
    }
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())).then_with(|| a.name.cmp(&b.name)));
    out
}

/// The home folder, as the system names it.
pub fn home_dir() -> Option<String> {
    let h = crate::sys::env("HOME").or_else(|| crate::sys::env("USERPROFILE"))?;
    let h = normalize(&h);
    if h.is_empty() { None } else { Some(h) }
}

/// Shortcut buttons: label and folder, for those folders that exist.
pub fn shortcuts(data: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    if let Some(h) = home_dir() {
        for (label, sub) in [("DOWNLOADS", "Downloads"), ("DESKTOP", "Desktop"), ("PICTURES", "Pictures")] {
            let p = join(&h, sub);
            if crate::sys::is_dir(&p) {
                v.push((String::from(label), p));
            }
        }
        v.insert(0, (String::from("HOME"), h));
    }
    let d = normalize(data);
    if !d.is_empty() && crate::sys::is_dir(&d) {
        v.push((String::from("DATA"), d));
    }
    v.push((String::from(if cfg!(windows) { "C:" } else { "ROOT" }), String::from(if cfg!(windows) { "C:/" } else { "/" })));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn paths_join_and_go_up() {
        assert_eq!(join("/home/a", "x.png"), "/home/a/x.png");
        assert_eq!(join("/", "home"), "/home");
        assert_eq!(join("", "x"), "x");
        assert_eq!(join("C:/", "Users"), "C:/Users");
        assert_eq!(parent("/home/a/b"), Some(String::from("/home/a")));
        assert_eq!(parent("/home"), Some(String::from("/")));
        assert_eq!(parent("/"), None);
        assert_eq!(parent("C:/Users/me"), Some(String::from("C:/Users")));
        assert_eq!(parent("C:/Users"), Some(String::from("C:/")));
        assert_eq!(parent("C:/"), None);
        assert_eq!(parent("data/skins"), Some(String::from("data")));
        assert_eq!(parent("data"), Some(String::new()));
        assert_eq!(parent(""), None);
        // Trailing slashes and backslashes do not matter.
        assert_eq!(parent("/home/a/"), Some(String::from("/home")));
        assert_eq!(normalize("C:\\Users\\me\\"), "C:/Users/me");
        assert_eq!(normalize("C:\\"), "C:/");
        assert_eq!(file_name("/a/b/c.png"), "c.png");
        assert_eq!(file_name("c:\\a\\c.png"), "c.png");
    }

    #[test]
    fn extensions_filter_without_regard_to_case() {
        let exts = ["png", "jpg"];
        assert!(matches_ext("skin.PNG", &exts));
        assert!(matches_ext("a.b.jpg", &exts));
        assert!(!matches_ext("skin.png.txt", &exts));
        assert!(!matches_ext("png", &exts));
        assert!(!matches_ext(".png", &exts));
        assert!(matches_ext("anything", &[]));
        assert_eq!(ext_of("A.TgA"), Some(String::from("tga")));
        assert_eq!(ext_of("noext"), None);
    }

    #[test]
    fn listing_is_folders_first_sorted_filtered_and_without_hidden() {
        let all = names(&["zeta.png", "Beta", "alpha.PNG", "notes.txt", ".hidden.png", ".git", "apple", "Cat.jpg", "file"]);
        let dirs = ["Beta", "apple", ".git"];
        let list = build_entries(&all, |n| dirs.contains(&n), &["png", "jpg"]);
        let got: Vec<(&str, bool)> = list.iter().map(|e| (e.name.as_str(), e.dir)).collect();
        assert_eq!(got, [("apple", true), ("Beta", true), ("alpha.PNG", false), ("Cat.jpg", false), ("zeta.png", false)]);
        // No filter lists every file.
        let list = build_entries(&all, |n| dirs.contains(&n), &[]);
        assert_eq!(list.len(), 7);
        assert!(list[2..].iter().all(|e| !e.dir));
    }
}
