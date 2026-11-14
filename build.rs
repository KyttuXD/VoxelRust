//! Link settings that cannot be written in the source: on Windows the game
//! supplies its own entry point and takes nothing from the toolchain's C
//! runtime libraries (see src/sys/mod.rs).

/// Write a generated file only when its text differs from what is there, so
/// that an unchanged table keeps its modification time and nothing that
/// includes it is compiled again for no reason.
fn write_if_changed(path: &std::path::Path, text: &str) {
    if std::fs::read(path).map_or(true, |old| old != text.as_bytes()) {
        std::fs::write(path, text).unwrap();
    }
}

/// Ask git for a path (`rev-parse --git-path` knows about worktrees, where
/// `.git` is a file and not a folder), as an absolute path.
fn git_path(what: &str) -> Option<std::path::PathBuf> {
    let o = std::process::Command::new("git").args(["rev-parse", "--git-path", what]).output().ok().filter(|o| o.status.success())?;
    let p = std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    Some(if p.is_absolute() { p } else { std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join(p) })
}

/// Write a table of every file under data/ (except base.txt, which is
/// included directly) so the game carries its models and extra data files
/// inside the program and needs no data folder beside it.
fn embed_data() {
    use std::fmt::Write as _;
    let root = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("data");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut entries: Vec<_> = std::fs::read_dir(&dir).map(|r| r.flatten().collect()).unwrap_or_default();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().map_or(false, |n| n != "base.txt") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut out = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        writeln!(out, "    ({:?}, include_bytes!({:?})),", rel, f.display().to_string()).unwrap();
        println!("cargo:rerun-if-changed={}", f.display());
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("embedded.rs");
    write_if_changed(&dest, &out);
}

/// Where the mods are built from (only with the `native-mods` feature): the folder named by VOXELRUST_MODS_DIR
/// (relative paths are taken from the repository), else `../VoxelRust-Mods`
/// beside the repository. The folder may be absent: the game then builds
/// with no mod code and the tests of the shipped mods are skipped.
fn mods_dir() -> std::path::PathBuf {
    println!("cargo:rerun-if-env-changed=VOXELRUST_MODS_DIR");
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    match std::env::var_os("VOXELRUST_MODS_DIR").filter(|v| !v.is_empty()) {
        Some(v) => manifest.join(v),
        None => manifest.join("..").join("VoxelRust-Mods"),
    }
}

/// The module a mod's code is compiled as: its folder name in lower case, with "-" as "_".
fn module_name(folder: &str) -> String {
    folder.to_lowercase().replace('-', "_")
}

/// Find the code mods bring with them (`<mods dir>/<name>/code/mod.rs`) and
/// write the list the game compiles in. Also, for the tests, the files of
/// the mods that come with the game (those named in `<mods dir>/SHIPPED`).
fn embed_mods() {
    use std::fmt::Write as _;
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("native_mods.rs");
    // Without the `native-mods` feature the game carries no mods: no code,
    // no files, and the mods folder is not even looked at.
    if std::env::var_os("CARGO_FEATURE_NATIVE_MODS").is_none() {
        let none = "pub static NATIVES: &[&crate::native::Native] = &[];\n#[cfg(test)]\npub static SHIPPED: &[(&str, &[u8])] = &[];\n";
        write_if_changed(&dest, none);
        return;
    }
    let root = mods_dir();
    println!("cargo:rerun-if-changed={}", root.display());
    // For the tests of the mods, which also read mods that do not ship.
    println!("cargo:rustc-env=VOXELRUST_MODS_DIR_RESOLVED={}", root.display());
    if !root.is_dir() {
        println!("cargo:warning=no mods folder at {} (set VOXELRUST_MODS_DIR): building with no native mods and no shipped mods", root.display());
    }
    let mut names: Vec<String> = std::fs::read_dir(&root).map(|r| r.flatten().filter(|e| e.path().join("code").join("mod.rs").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    // A folder name may hold "-" (as "mlp-mod" does): the module is named with "_" in its place.
    names.retain(|n| n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && n.chars().next().map_or(false, |c| c.is_ascii_alphabetic()));
    names.sort();
    let mut out = String::new();
    for n in &names {
        let file = root.join(n).join("code").join("mod.rs");
        println!("cargo:rerun-if-changed={}", root.join(n).join("code").display());
        writeln!(out, "#[path = {:?}]\npub mod {};", file.display().to_string(), module_name(n)).unwrap();
    }
    writeln!(out, "pub static NATIVES: &[&crate::native::Native] = &[{}];", names.iter().map(|n| format!("&{}::NATIVE", module_name(n))).collect::<Vec<_>>().join(", ")).unwrap();
    // The shipped mods' own files, so the tests see the game as it ships.
    let shipped = std::fs::read_to_string(root.join("SHIPPED")).unwrap_or_default();
    println!("cargo:rerun-if-changed={}", root.join("SHIPPED").display());
    writeln!(out, "#[cfg(test)]\npub static SHIPPED: &[(&str, &[u8])] = &[").unwrap();
    for m in shipped.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut stack = vec![root.join(m)];
        let mut files = Vec::new();
        while let Some(dir) = stack.pop() {
            println!("cargo:rerun-if-changed={}", dir.display());
            for e in std::fs::read_dir(&dir).map(|r| r.flatten().collect::<Vec<_>>()).unwrap_or_default() {
                let p = e.path();
                if p.is_dir() {
                    if p.file_name().map_or(true, |n| n != "code") {
                        stack.push(p);
                    }
                } else {
                    files.push(p);
                }
            }
        }
        files.sort();
        for f in files {
            // The game's tests know these files as "mods/<name>/...".
            let rel = format!("mods/{}", f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"));
            writeln!(out, "    ({:?}, include_bytes!({:?})),", rel, f.display().to_string()).unwrap();
            println!("cargo:rerun-if-changed={}", f.display());
        }
    }
    out.push_str("];\n");
    // Tests that belong to the mods rather than to the game.
    let tests = root.join("tests.rs");
    println!("cargo:rerun-if-changed={}", tests.display());
    if tests.is_file() {
        writeln!(out, "#[cfg(test)]\n#[path = {:?}]\nmod shipped_tests;", tests.display().to_string()).unwrap();
    }
    write_if_changed(&dest, &out);
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    embed_mods();
    // Which commit this is, for the updater to compare with the release.
    // (VOXELRUST_COMMIT wins when set, as the release build does.)
    println!("cargo:rerun-if-env-changed=VOXELRUST_COMMIT");
    // The release number, when the release script sets one (see client::update).
    println!("cargo:rerun-if-env-changed=VOXELRUST_SEQ");
    println!("cargo:rerun-if-env-changed=VOXELRUST_RELEASE");
    // The commit moves when HEAD or the file of the branch it names does. Only
    // paths that exist are named: cargo runs a build script again on every
    // build when a path it was told to watch is missing (as `.git/HEAD` is
    // in a git worktree, where `.git` is a file), so the paths come from git
    // itself.
    if std::env::var("VOXELRUST_COMMIT").is_err() {
        let watch = |p: Option<std::path::PathBuf>| {
            if let Some(p) = p.filter(|p| p.exists()) {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        };
        watch(git_path("HEAD"));
        watch(git_path("packed-refs"));
        let branch = std::process::Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if let Some(b) = branch.filter(|b| !b.is_empty()) {
            watch(git_path(&b));
        }
        let commit = std::process::Command::new("git").args(["rev-parse", "--short=7", "HEAD"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if let Some(c) = commit.filter(|c| !c.is_empty()) {
            println!("cargo:rustc-env=VOXELRUST_COMMIT={c}");
        }
    }
    // A build with no release number from the script still gets one: how many commits
    // the checkout has, so that it can tell when a newer release is out and update
    // itself (a build with no number never updates). The same rule as the release script.
    if std::env::var("VOXELRUST_SEQ").is_err() {
        let git = |args: &[&str]| std::process::Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if git(&["rev-parse", "--is-shallow-repository"]).as_deref() == Some("false") {
            if let Some(n) = git(&["rev-list", "--count", "HEAD"]).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
                println!("cargo:rustc-env=VOXELRUST_SEQ={n}");
            }
        }
    }
    embed_data();
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    if var("CARGO_CFG_TARGET_OS") != "windows" {
        return;
    }
    let args: &[&str] = if var("CARGO_CFG_TARGET_ENV") == "msvc" {
        &["/ENTRY:mainCRTStartup", "/NODEFAULTLIB", "/SUBSYSTEM:CONSOLE"]
    } else {
        &["-nostartfiles", "-Wl,--entry=mainCRTStartup"]
    };
    for a in args {
        println!("cargo:rustc-link-arg-bins={a}");
    }
    // The program's icon, so Explorer and the taskbar show it. The linker
    // takes a compiled resource file; it is simple enough to write here,
    // which saves needing a resource compiler.
    if var("CARGO_CFG_TARGET_ENV") == "msvc" {
        let ico = std::path::Path::new(&var("CARGO_MANIFEST_DIR")).join("assets").join("icon.ico");
        println!("cargo:rerun-if-changed={}", ico.display());
        if let Some(res) = std::fs::read(&ico).ok().and_then(|d| icon_resource(&d)) {
            let dest = std::path::Path::new(&var("OUT_DIR")).join("icon.res");
            std::fs::write(&dest, res).unwrap();
            println!("cargo:rustc-link-arg-bins={}", dest.display());
        }
    }
}

/// Turn an .ico file into a compiled Windows resource (.res): one RT_ICON
/// entry per picture and an RT_GROUP_ICON entry listing them.
fn icon_resource(ico: &[u8]) -> Option<Vec<u8>> {
    let u16_at = |o: usize| ico.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| ico.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let count = u16_at(4)? as usize;
    // One resource: a header (sizes, type, name, attributes), then the
    // data, padded to four bytes. Types and names here are numbers.
    fn entry(out: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        for id in [kind, name] {
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&id.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // data version
        out.extend_from_slice(&0x1030u16.to_le_bytes()); // moveable, pure, discardable
        out.extend_from_slice(&0x0409u16.to_le_bytes()); // language
        out.extend_from_slice(&[0u8; 8]); // version, characteristics
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let mut out = Vec::new();
    // Every .res file starts with an empty entry.
    entry(&mut out, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let dir = 6 + i * 16;
        let (size, offset) = (u32_at(dir + 8)? as usize, u32_at(dir + 12)? as usize);
        let id = i as u16 + 1;
        entry(&mut out, 3, id, ico.get(offset..offset + size)?);
        // The group's entry is the file's, with an id in place of the offset.
        group.extend_from_slice(ico.get(dir..dir + 12)?);
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut out, 14, 1, &group);
    Some(out)
}
