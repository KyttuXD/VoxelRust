//! Link settings that cannot be written in the source: on Windows the game
//! supplies its own entry point and takes nothing from the toolchain's C
//! runtime libraries (see src/sys/mod.rs).

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
    std::fs::write(dest, out).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
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
