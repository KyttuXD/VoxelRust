//! Tests that run the mods' scripts and packs through the runtime, as the game
//! would load them from its mods folder: every script of every mod parses and
//! passes the call check, and every mod's pack loads with no line left over
//! for want of a claim. They read the folder named by `VOXELRUST_MODS_DIR`
//! (the mods repository) and are skipped when it is not set. The two mods the
//! author is working on (orespawn, witherstorm) are left out: they have no
//! scripts, and their packs change under the tests' feet.

use crate::content::block::Registry;
use crate::script::check::check_source;
use crate::script::{native_wins, ScriptHost};
use std::fs;
use std::path::{Path, PathBuf};
use std::string::String;
use std::vec::Vec;

const LEFT_OUT: &[&str] = &["orespawn", "witherstorm"];

/// The mods folder, if the environment names one that exists.
fn mods_dir() -> Option<PathBuf> {
    match std::env::var("VOXELRUST_MODS_DIR") {
        Ok(d) if Path::new(&d).is_dir() => Some(PathBuf::from(d)),
        _ => {
            std::eprintln!("SKIPPED: set VOXELRUST_MODS_DIR to the mods folder to check the mods' scripts and packs");
            None
        }
    }
}

/// The mods in the folder (each has a pack.txt), by name, without the ones left out.
fn mods(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the mods folder can be read")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("pack.txt").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !LEFT_OUT.contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

fn scripts_of(dir: &Path, name: &str) -> Vec<(String, String)> {
    let folder = dir.join(name).join("scripts");
    let mut out: Vec<(String, String)> = Vec::new();
    if let Ok(list) = fs::read_dir(&folder) {
        for e in list.filter_map(|e| e.ok()) {
            let file = e.file_name().into_string().unwrap_or_default();
            if file.ends_with(".rs") {
                let src = fs::read_to_string(e.path()).unwrap_or_else(|err| panic!("{}: {}", e.path().display(), err));
                out.push((std::format!("scripts/{}", file), src));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_script_of_every_mod_parses_and_checks() {
    let Some(dir) = mods_dir() else { return };
    let mut checked = 0;
    for name in mods(&dir) {
        for (file, src) in scripts_of(&dir, &name) {
            let shown = std::format!("{}/{}", name, file);
            let report = check_source(&shown, &src).unwrap_or_else(|e| panic!("{}", e));
            assert!(report.mistakes.is_empty(), "{:?}", report.mistakes);
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            assert!(!report.hooks.is_empty(), "{} defines no hook", shown);
            checked += 1;
        }
    }
    assert!(checked > 0, "no mod in {} has a script", dir.display());
}

#[test]
fn a_mod_folder_loads_into_the_script_host_without_problems() {
    let Some(dir) = mods_dir() else { return };
    for name in mods(&dir) {
        let files: Vec<(String, Vec<u8>)> = scripts_of(&dir, &name).into_iter().map(|(f, s)| (f, s.into_bytes())).collect();
        if files.is_empty() {
            continue;
        }
        let mut host = ScriptHost::new();
        let problems = host.add_mod(&name, &files);
        assert!(problems.is_empty(), "{}: {:?}", name, problems);
        // A mod whose compiled code is in the build runs that instead.
        assert_eq!(host.mods.is_empty(), native_wins(&name), "{}", name);
    }
}

#[test]
fn every_pack_loads_with_the_lines_its_script_reads_kept() {
    let Some(dir) = mods_dir() else { return };
    // The earth mod is compiled code only: its `map` lines are read by that code.
    let names = mods(&dir);
    let mut texts: Vec<(String, String)> = vec![(String::from("base.txt"), String::from(crate::block::BASE_DATA))];
    for n in &names {
        let path = dir.join(n).join("pack.txt");
        texts.push((path.to_string_lossy().into_owned(), fs::read_to_string(&path).unwrap()));
    }
    let refs: Vec<(&str, &str)> = texts.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let r = Registry::from_sources(&refs, &mut |p| fs::read(p).ok());
    let left_over: Vec<&String> = r.warnings.iter().filter(|w| w.contains("unexpected") && !w.contains("/earth/")).collect();
    assert!(left_over.is_empty(), "lines no one claimed: {:?}", left_over);
    if names.iter().any(|n| n == "boats") && names.iter().any(|n| n == "vehicles") {
        assert!(r.sections.iter().any(|s| s.kind == "vehicle"), "the vehicle sections were not kept for the script");
    }
    if names.iter().any(|n| n == "manfromthefog") {
        assert!(r.lines.iter().any(|l| l.0 == "manfromthefog"), "the mod's settings were not kept for the script");
    }
}

#[test]
fn a_mod_with_compiled_code_in_the_build_does_not_also_run_its_scripts() {
    assert!(!native_wins("no-such-mod-at-all"));
    for n in crate::native::all() {
        let mut host = ScriptHost::new();
        let files = vec![(String::from("scripts/a.rs"), b"fn on_tick(dt: f32) {}".to_vec())];
        assert!(host.add_mod(n.name, &files).is_empty());
        assert!(host.mods.is_empty(), "the scripts of {} were loaded beside its compiled code", n.name);
    }
    // And a mod with none is loaded.
    let mut host = ScriptHost::new();
    let files = vec![(String::from("scripts/a.rs"), b"fn on_tick(dt: f32) {}".to_vec())];
    assert!(host.add_mod("no-such-mod-at-all", &files).is_empty());
    assert_eq!(host.mods.len(), 1);
}
