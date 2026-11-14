//! Tests of the explosion hooks as scripts see them: the names and
//! arguments the docs give, what a script may answer, the host calls that go
//! with them, and that every way to blow something up has a cause name.
//! (The rules about immune blocks, the call cap and the nesting are tested
//! in `game/explosion_hooks/`.)

use crate::explosion::ExplosionSource;
use crate::script::check::check_source;
use crate::script::host::mock::MockHost;
use crate::script::value::Value;
use crate::script::{ScriptHost, HOOKS};
use alloc::string::String;
use alloc::vec::Vec;

fn host(src: &str) -> ScriptHost {
    let mut h = ScriptHost::new();
    let files = alloc::vec![(String::from("scripts/main.rs"), src.as_bytes().to_vec())];
    let problems = h.add_mod("demo", &files);
    assert!(problems.is_empty(), "{:?}", problems);
    h
}

fn answers(h: &mut ScriptHost, name: &str, args: &[Value]) -> Vec<Value> {
    let mut m = MockHost::default();
    let mut out = Vec::new();
    h.call_hook(&mut m, "w", name, args, &mut |v| {
        out.push(v.clone());
        false
    });
    out
}

fn blast(cause: &str) -> Vec<Value> {
    alloc::vec![Value::Float(1.5), Value::Float(64.5), Value::Float(-3.5), Value::Float(8.0), Value::str(cause)]
}

#[test]
fn the_hooks_take_the_arguments_the_docs_name() {
    let p = |name: &str| HOOKS.iter().find(|h| h.name == name).unwrap_or_else(|| panic!("no hook {}", name)).params;
    assert_eq!(p("before_explode"), ["x", "y", "z", "radius", "cause"]);
    assert_eq!(p("explode_block"), ["x", "y", "z", "block"]);
    assert_eq!(p("after_explode"), ["x", "y", "z", "radius", "destroyed", "cause"]);
}

#[test]
fn a_script_with_all_three_hooks_and_the_calls_passes_the_check() {
    let src = "\
fn on_load() { set_immune_blocks([\"obsidian\", \"bedrock\"]); }
fn before_explode(x: f64, y: f64, z: f64, radius: f64, cause: String) -> bool { if radius > 6.0 { set_explosion_radius(radius / 2.0); } cause != \"creature\" }
fn explode_block(x: i64, y: i64, z: i64, block: String) -> bool { block != \"glass\" }
fn after_explode(x: f64, y: f64, z: f64, radius: f64, destroyed: i64, cause: String) { log(&cause); }
fn on_update(dt: f64) { if is_blast_immune(\"stone\") { clear_immune_blocks(); } }
";
    let r = check_source("t.rs", src).expect("it loads");
    assert!(r.mistakes.is_empty(), "{:?}", r.mistakes);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    for name in ["before_explode", "explode_block", "after_explode"] {
        assert!(r.hooks.iter().any(|h| h == name), "{} was not seen as a hook", name);
    }
}

#[test]
fn bad_calls_to_the_explosion_functions_are_found_by_the_check() {
    let r = check_source("t.rs", "fn on_load() { set_explosion_radius(); set_immune_blocks(); clear_immune_blocks(1); is_blast_immune(); }").expect("it loads");
    assert_eq!(r.mistakes.len(), 4, "{:?}", r.mistakes);
}

#[test]
fn a_script_may_cancel_a_blast_by_its_cause() {
    let mut h = host("fn before_explode(x: f64, y: f64, z: f64, radius: f64, cause: String) -> bool { cause != \"tnt\" }");
    assert_eq!(answers(&mut h, "before_explode", &blast("tnt")), [Value::Bool(false)]);
    assert_eq!(answers(&mut h, "before_explode", &blast("creature")), [Value::Bool(true)]);
    // A hook that returns nothing lets it go ahead.
    let mut quiet = host("fn before_explode(x: f64) { log(\"seen\"); }");
    assert!(answers(&mut quiet, "before_explode", &blast("tnt")).is_empty());
}

#[test]
fn the_block_hook_spares_blocks_by_id_and_place() {
    let mut h = host("fn explode_block(x: i64, y: i64, z: i64, block: String) -> bool { block != \"glass\" && y > 10 }");
    let at = |y: i64, id: &str| alloc::vec![Value::Int(4), Value::Int(y), Value::Int(5), Value::str(id)];
    assert_eq!(answers(&mut h, "explode_block", &at(64, "stone")), [Value::Bool(true)]);
    assert_eq!(answers(&mut h, "explode_block", &at(64, "glass")), [Value::Bool(false)]);
    assert_eq!(answers(&mut h, "explode_block", &at(3, "stone")), [Value::Bool(false)]);
}

#[test]
fn the_after_hook_hears_the_count() {
    let mut h = host("fn after_explode(x: f64, y: f64, z: f64, radius: f64, destroyed: i64, cause: String) -> bool { destroyed == 120 && cause == \"script\" && radius == 8.0 }");
    let args = alloc::vec![Value::Float(0.0), Value::Float(0.0), Value::Float(0.0), Value::Float(8.0), Value::Int(120), Value::str("script")];
    assert_eq!(answers(&mut h, "after_explode", &args), [Value::Bool(true)]);
}

#[test]
fn the_host_knows_which_scripts_ask_for_the_block_calls() {
    let h = host("fn before_explode(x: f64) -> bool { true }");
    assert!(!h.any_defines("explode_block"));
    assert!(h.any_defines("before_explode"));
    let h = host("fn explode_block(x: i64) -> bool { true }");
    assert!(h.any_defines("explode_block"));
    assert!(!ScriptHost::new().any_defines("explode_block"));
}

#[test]
fn the_explosion_calls_need_a_running_game() {
    // With the test's mock host there is no session: they fail politely rather than pretend.
    for call in ["set_explosion_radius(4.0)", "set_immune_blocks([\"stone\"])", "clear_immune_blocks()", "is_blast_immune(\"stone\")"] {
        let src = alloc::format!("fn on_load() {{ {}; }}", call);
        let mut h = ScriptHost::new();
        let files = alloc::vec![(String::from("scripts/main.rs"), src.as_bytes().to_vec())];
        assert!(h.add_mod("demo", &files).is_empty());
        let mut m = MockHost::default();
        h.call_hook(&mut m, "w", "on_load", &[], &mut |_| false);
        assert!(!h.problems.is_empty() || h.mods[0].scripts.iter().any(|s| s.disabled.is_some()), "{} did not fail", call);
    }
}

#[test]
fn every_way_to_blow_up_names_its_cause_for_scripts() {
    assert_eq!(ExplosionSource::Tnt.cause(), "tnt");
    assert_eq!(ExplosionSource::Creature(2).cause(), "creature");
    assert_eq!(ExplosionSource::Command.cause(), "player");
    assert_eq!(ExplosionSource::Mod(0x5C21).cause(), "script");
    let docs = include_str!("../../docs/SCRIPTING.md");
    for cause in ["tnt", "creature", "script", "player", "fire", "other"] {
        assert!(docs.contains(&alloc::format!("`{}`", cause)), "the docs do not name the cause {}", cause);
    }
}
