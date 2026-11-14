//! Tests of the quality-of-life mods (waypoints, death marker, light
//! warning, sort inventory, quick stack, coordinates and clock on the
//! screen, tree felling, vein miner). They live in the mods repository
//! (VOXELRUST_MODS_DIR, default `../VoxelRust-Mods` beside this one); the
//! tests read their scripts from there and skip themselves when the folder
//! or a mod is not there. Each script is read the way `--script-check` reads
//! it, then run against the mock host.

use crate::script::check::check_source;
use crate::script::host::mock::MockHost;
use crate::script::value::Value;
use crate::script::ScriptHost;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The mods and the hooks each must define.
const QOL: &[(&str, &[&str])] = &[
    ("waypoints", &["on_command"]),
    ("death_marker", &["on_event", "on_tick", "on_command"]),
    ("light_warning", &["on_tick", "on_command"]),
    ("sort_inventory", &["on_command"]),
    ("quick_stack", &["on_command"]),
    ("coordinates_hud", &["on_tick", "on_command"]),
    ("clock_hud", &["on_tick", "on_command"]),
    ("tree_felling", &["on_block_break", "on_command"]),
    ("veinminer", &["on_block_break", "on_command"]),
];

fn mods_dir() -> Option<std::path::PathBuf> {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = match std::env::var_os("VOXELRUST_MODS_DIR").filter(|v| !v.is_empty()) {
        Some(v) => manifest.join(v),
        None => manifest.join("..").join("VoxelRust-Mods"),
    };
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// The script of a QoL mod, or `None` (with a note) when it is not there.
fn source(name: &str) -> Option<String> {
    let dir = mods_dir().or_else(|| {
        eprintln!("SKIPPED: no mods folder (set VOXELRUST_MODS_DIR to the VoxelRust-Mods folder)");
        None
    })?;
    match std::fs::read_to_string(dir.join(name).join("scripts").join(format!("{}.rs", name))) {
        Ok(s) => Some(s),
        Err(_) => {
            eprintln!("SKIPPED: the mod {} is not in {}", name, dir.display());
            None
        }
    }
}

/// A script host with the mod loaded, or `None` when it is not there.
fn load(name: &str) -> Option<ScriptHost> {
    let src = source(name)?;
    let mut h = ScriptHost::new();
    let problems = h.add_mod(name, &[(format!("scripts/{}.rs", name), src.into_bytes())]);
    assert!(problems.is_empty(), "{:?}", problems);
    Some(h)
}

fn text(s: &str) -> Value {
    Value::string(String::from(s))
}

/// Call a hook; the values the scripts answered.
fn call(h: &mut ScriptHost, m: &mut MockHost, hook: &str, args: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    h.call_hook(m, "world", hook, args, &mut |v| {
        out.push(v.clone());
        false
    });
    assert!(h.problems.is_empty(), "{:?}", h.problems);
    out
}

/// Type a chat command (name without the slash, then the rest of the line).
fn command(h: &mut ScriptHost, m: &mut MockHost, name: &str, args: &str) -> Vec<Value> {
    call(h, m, "on_command", &[text(name), text(args)])
}

fn said(m: &MockHost) -> Vec<&str> {
    m.calls.iter().filter_map(|c| c.strip_prefix("say ")).collect()
}

#[test]
fn every_qol_script_reads_and_checks() {
    for (name, hooks) in QOL {
        let Some(src) = source(name) else { continue };
        let file = format!("{}/scripts/{}.rs", name, name);
        let report = check_source(&file, &src).unwrap_or_else(|e| panic!("{}", e));
        assert!(report.warnings.is_empty(), "{}: {:?}", name, report.warnings);
        for hook in *hooks {
            assert!(report.hooks.iter().any(|h| h.as_str() == *hook), "{} does not define {}", name, hook);
        }
        // Each mod has its pack.txt and a README.
        let dir = mods_dir().unwrap().join(name);
        assert!(dir.join("pack.txt").is_file(), "{} has no pack.txt", name);
        assert!(dir.join("README.txt").is_file(), "{} has no README.txt", name);
    }
}

#[test]
fn the_qol_scripts_only_call_host_functions_that_exist() {
    use crate::script::host::HOST_FUNCTIONS;
    // Every `name(` in a script that is not a function of its own, a method,
    // a macro or part of the language must be a host function.
    let known_words = [
        "if", "while", "for", "match", "return", "fn", "let", "Some", "None", "Ok", "Err", "Wp", "String", "Vec", "format", "vec", "else", "in", "as", "loop", "const", "struct", "impl",
    ];
    for (name, _) in QOL {
        let Some(src) = source(name) else { continue };
        let own: Vec<&str> = src.lines().filter_map(|l| l.trim_start().strip_prefix("fn ")).filter_map(|l| l.split('(').next()).collect();
        for line in src.lines().filter(|l| !l.trim_start().starts_with("//")) {
            let bytes = line.as_bytes();
            for (i, b) in bytes.iter().enumerate() {
                if *b != b'(' || i == 0 {
                    continue;
                }
                let start = line[..i].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map_or(0, |p| p + 1);
                let word = &line[start..i];
                let before = if start > 0 { bytes[start - 1] } else { b' ' };
                if word.is_empty() || before == b'.' || before == b':' || word.chars().next().unwrap().is_ascii_digit() || word.chars().next().unwrap().is_ascii_uppercase() || line[..i].ends_with('!') {
                    continue;
                }
                if known_words.contains(&word) || own.contains(&word) {
                    continue;
                }
                assert!(HOST_FUNCTIONS.iter().any(|(sig, _)| sig.split('(').next() == Some(word)), "{}: `{}(` is neither the script's own function nor a host function", name, word);
            }
        }
    }
}

#[test]
fn waypoints_are_kept_and_described() {
    let Some(mut h) = load("waypoints") else { return };
    let mut m = MockHost::default();
    m.pos = (10.5, 64.0, 20.5);
    assert_eq!(command(&mut h, &mut m, "wp", "add Home Base"), [Value::Bool(true)]);
    assert_eq!(said(&m), ["WAYPOINT HOME_BASE SET AT 10 64 20"]);
    // Some way off, to the south, looking north.
    m.pos = (10.5, 64.0, -80.5);
    m.calls.clear();
    command(&mut h, &mut m, "wp", "home_base");
    assert_eq!(said(&m), ["HOME_BASE: 101 BLOCKS S (BEHIND YOU)"]);
    // The same, looking south.
    m.heading = 180.0;
    m.calls.clear();
    command(&mut h, &mut m, "waypoint", "Home Base");
    assert_eq!(said(&m), ["HOME_BASE: 101 BLOCKS S (AHEAD)"]);
    m.calls.clear();
    command(&mut h, &mut m, "wp", "list");
    assert_eq!(said(&m).len(), 1);
    command(&mut h, &mut m, "wp", "del home_base");
    m.calls.clear();
    command(&mut h, &mut m, "wp", "list");
    assert!(said(&m)[0].starts_with("NO WAYPOINTS"), "{:?}", said(&m));
    // A command of someone else's is left alone.
    assert_eq!(command(&mut h, &mut m, "heal", ""), [Value::Bool(false)]);
}

#[test]
fn the_death_marker_remembers_and_guides() {
    let Some(mut h) = load("death_marker") else { return };
    let mut m = MockHost::default();
    m.pos = (5.5, 70.0, 5.5);
    call(&mut h, &mut m, "on_event", &[text("death"), text("")]);
    m.pos = (5.5, 70.0, 105.5);
    command(&mut h, &mut m, "death", "");
    assert_eq!(said(&m), ["YOU DIED AT 5 70 5: 100 BLOCKS N (AHEAD)"]);
    call(&mut h, &mut m, "on_event", &[text("respawn"), text("")]);
    assert_eq!(said(&m).len(), 2);
    assert!(m.commands.iter().any(|c| c.starts_with("/title")), "{:?}", m.commands);
    // The line on the screen.
    command(&mut h, &mut m, "death", "hud");
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.0)]);
    assert!(m.calls.iter().any(|c| c == "hud DEATH 100 N"), "{:?}", m.calls);
    command(&mut h, &mut m, "death", "clear");
    m.calls.clear();
    command(&mut h, &mut m, "death", "");
    assert_eq!(said(&m), ["YOU HAVE NOT DIED YET IN THIS WORLD"]);
}

#[test]
fn the_light_warning_speaks_only_in_the_dark() {
    let Some(mut h) = load("light_warning") else { return };
    let mut m = MockHost::default();
    m.pos = (0.5, 64.0, 0.5);
    m.time = 0.75; // midnight, open sky: dark
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.1)]);
    assert!(m.commands.iter().any(|c| c.starts_with("/title DARK")), "{:?}", m.commands);
    // Still dark a second later: no second notice yet.
    m.commands.clear();
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.1)]);
    assert!(m.commands.is_empty());
    // A lamp beside you: light, and the count starts over.
    m.blocks.push(((1, 64, 0), String::from("lamp")));
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.1)]);
    assert!(m.commands.is_empty());
    // Daylight: fine.
    m.blocks.clear();
    m.time = 0.25;
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.1)]);
    assert!(m.commands.is_empty());
    command(&mut h, &mut m, "lightwarn", "off");
    m.time = 0.75;
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.1)]);
    assert!(m.commands.is_empty());
}

#[test]
fn sort_merges_and_orders_the_pack() {
    let Some(mut h) = load("sort_inventory") else { return };
    let mut m = MockHost::default();
    m.slots = alloc::vec![(String::new(), 0); 36];
    m.slots[0] = (String::from("zeta"), 1);
    m.slots[10] = (String::from("beta"), 3);
    m.slots[12] = (String::from("alpha"), 2);
    m.slots[20] = (String::from("beta"), 5);
    m.slots[30] = (String::from("alpha"), 63);
    command(&mut h, &mut m, "sort", "");
    let ids: Vec<&str> = m.slots[9..14].iter().map(|s| s.0.as_str()).collect();
    assert_eq!(ids, ["alpha", "alpha", "beta", "", ""]);
    let total = |m: &MockHost, id: &str| m.slots.iter().filter(|s| s.0 == id).map(|s| s.1).sum::<i64>();
    assert_eq!(total(&m, "alpha"), 65);
    assert_eq!(total(&m, "beta"), 8);
    // The hotbar was left alone.
    assert_eq!(m.slots[0], (String::from("zeta"), 1));
    command(&mut h, &mut m, "sort", "all");
    assert_eq!(m.slots[0].0, "alpha");
    assert_eq!(total(&m, "zeta"), 1);
}

#[test]
fn quick_stack_finds_the_chest() {
    let Some(mut h) = load("quick_stack") else { return };
    let mut m = MockHost::default();
    m.pos = (0.5, 64.0, 0.5);
    command(&mut h, &mut m, "stack", "");
    assert_eq!(said(&m), ["NO CHEST WITHIN 6 BLOCKS"]);
    m.blocks.push(((3, 64, 0), String::from("oak_chest")));
    m.blocks.push(((-1, 64, 1), String::from("oak_chest")));
    m.calls.clear();
    command(&mut h, &mut m, "stack", "");
    // The nearer chest first, and only it.
    assert_eq!(m.calls[0], "stack_to_chest -1 64 1");
    assert_eq!(m.calls.iter().filter(|c| c.starts_with("stack_to_chest")).count(), 1);
    m.calls.clear();
    command(&mut h, &mut m, "stack", "all");
    assert_eq!(m.calls.iter().filter(|c| c.starts_with("stack_to_chest")).count(), 2);
}

#[test]
fn the_coordinates_line_shows_place_facing_and_biome() {
    let Some(mut h) = load("coordinates_hud") else { return };
    let mut m = MockHost::default();
    m.pos = (10.7, 64.2, -20.3);
    m.heading = 92.0;
    m.biome = String::from("plains");
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.0)]);
    assert!(m.calls.iter().any(|c| c == "hud X 10 Y 64 Z -21  E  plains"), "{:?}", m.calls);
    command(&mut h, &mut m, "coords", "off");
    assert!(m.calls.iter().any(|c| c == "hud "));
    m.calls.clear();
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.0)]);
    assert!(m.calls.is_empty());
}

#[test]
fn the_clock_tells_the_time_of_the_game_day() {
    let Some(mut h) = load("clock_hud") else { return };
    let mut m = MockHost::default();
    m.time = 0.25;
    command(&mut h, &mut m, "clock", "");
    assert_eq!(said(&m), ["12:00  NIGHTFALL IN 6 MIN"]);
    m.time = 0.0;
    m.calls.clear();
    command(&mut h, &mut m, "clock", "");
    assert!(said(&m)[0].starts_with("06:00"), "{:?}", said(&m));
    m.time = 0.75;
    m.calls.clear();
    command(&mut h, &mut m, "clock", "");
    assert!(said(&m)[0].starts_with("00:00  SUNRISE IN"), "{:?}", said(&m));
    command(&mut h, &mut m, "clock", "12");
    m.time = 0.5;
    m.calls.clear();
    call(&mut h, &mut m, "on_tick", &[Value::Float(1.5)]);
    assert!(m.calls.iter().any(|c| c.starts_with("hud 6:00 PM")), "{:?}", m.calls);
}

#[test]
fn a_tree_comes_down_but_a_log_wall_does_not() {
    let Some(mut h) = load("tree_felling") else { return };
    let mut m = MockHost::default();
    m.held = String::from("wood_axe");
    for y in 64..68 {
        m.blocks.push(((0, y, 0), String::from("log")));
    }
    m.blocks.push(((0, 68, 0), String::from("leaves")));
    let args = [Value::Int(0), Value::Int(64), Value::Int(0), text("log")];
    assert_eq!(call(&mut h, &mut m, "on_block_break", &args), [Value::Bool(true)]);
    let felled = m.calls.iter().filter(|c| c.starts_with("set_block 0 6") && c.ends_with(" air")).count();
    assert_eq!(felled, 3, "{:?}", m.calls);
    assert_eq!(m.calls.iter().filter(|c| *c == "give log 1").count(), 3);
    // Sneaking, or no axe, or no leaves: nothing else falls.
    for y in 70..74 {
        m.blocks.push(((5, y, 5), String::from("log")));
    }
    m.calls.clear();
    let wall = [Value::Int(5), Value::Int(70), Value::Int(5), text("log")];
    call(&mut h, &mut m, "on_block_break", &wall);
    assert!(m.calls.is_empty(), "{:?}", m.calls);
    m.blocks.push(((5, 74, 5), String::from("leaves")));
    m.sneak = true;
    call(&mut h, &mut m, "on_block_break", &wall);
    assert!(m.calls.is_empty());
    m.sneak = false;
    m.held = String::from("wood_pick");
    call(&mut h, &mut m, "on_block_break", &wall);
    assert!(m.calls.is_empty());
}

#[test]
fn the_vein_miner_takes_the_vein_when_sneaking_with_a_pickaxe() {
    let Some(mut h) = load("veinminer") else { return };
    let mut m = MockHost::default();
    m.held = String::from("iron_pick");
    for c in [(1, 10, 1), (2, 10, 1), (2, 11, 2), (9, 10, 1)] {
        m.blocks.push((c, String::from("iron_ore")));
    }
    let args = [Value::Int(1), Value::Int(10), Value::Int(1), text("iron_ore")];
    call(&mut h, &mut m, "on_block_break", &args);
    assert!(m.calls.is_empty(), "{:?}", m.calls);
    m.sneak = true;
    call(&mut h, &mut m, "on_block_break", &args);
    // The two that touch, not the one far off.
    assert_eq!(m.calls.iter().filter(|c| c.ends_with(" air")).count(), 2, "{:?}", m.calls);
    assert_eq!(m.calls.iter().filter(|c| *c == "give iron_ore 1").count(), 2);
    // Other blocks are not a vein.
    m.calls.clear();
    call(&mut h, &mut m, "on_block_break", &[Value::Int(1), Value::Int(10), Value::Int(1), text("stone")]);
    assert!(m.calls.is_empty());
}
