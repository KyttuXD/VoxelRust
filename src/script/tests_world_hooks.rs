//! Tests of the world and creature hooks (`on_chunk_generated`,
//! `on_block_placed`, `on_creature_*`): their names and arguments as the docs
//! give them, what a script may return, and the host calls that go with them.

use crate::script::check::check_source;
use crate::script::host::mock::MockHost;
use crate::script::value::Value;
use crate::script::{ScriptHost, HOOKS};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

fn hook(name: &str) -> &'static crate::script::HookDoc {
    HOOKS.iter().find(|h| h.name == name).unwrap_or_else(|| panic!("no hook {}", name))
}

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

#[test]
fn the_new_hooks_take_the_arguments_the_docs_name() {
    assert_eq!(hook("on_chunk_generated").params, ["cx", "cz", "seed"]);
    assert_eq!(hook("on_block_placed").params, ["x", "y", "z", "id", "cause"]);
    assert_eq!(hook("on_creature_death").params, ["id", "kind", "x", "y", "z", "killer"]);
    assert_eq!(hook("on_creature_removed").params, ["id", "kind", "reason"]);
    assert_eq!(hook("on_creature_interact").params, ["id", "kind", "item"]);
    assert_eq!(hook("on_creature_sunburn").params, ["id", "kind"]);
}

#[test]
fn a_script_with_all_the_new_hooks_passes_the_check() {
    let src = "\
fn on_chunk_generated(cx: i64, cz: i64, seed: i64) { if rand_chunk(cx, cz, 1) % 10 == 0 { set_block(cx * 16, 70, cz * 16, \"stone\"); } }
fn on_block_placed(x: i64, y: i64, z: i64, id: String, cause: String) { log(&cause); }
fn on_creature_death(id: i64, kind: String, x: f64, y: f64, z: f64, killer: String) { log(&killer); }
fn on_creature_removed(id: i64, kind: String, reason: String) { log(&reason); }
fn on_creature_interact(id: i64, kind: String, item: String) -> bool { item != \"stick\" }
fn on_creature_sunburn(id: i64, kind: String) -> bool { kind != \"zombie\" }
fn on_key_down(id: String) { open_crafting(5); open_inventory(); }
";
    let r = check_source("t.rs", src).expect("it loads");
    assert!(r.mistakes.is_empty(), "{:?}", r.mistakes);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    for name in ["on_chunk_generated", "on_block_placed", "on_creature_death", "on_creature_removed", "on_creature_interact", "on_creature_sunburn"] {
        assert!(r.hooks.iter().any(|h| h == name), "{} was not seen as a hook", name);
    }
}

#[test]
fn a_hook_with_too_many_parameters_is_warned_about() {
    let r = check_source("t.rs", "fn on_creature_sunburn(id: i64, kind: String, extra: i64) -> bool { true }").expect("it loads");
    assert!(r.warnings.iter().any(|w| w.contains("on_creature_sunburn")), "{:?}", r.warnings);
    let r = check_source("t.rs", "fn on_chunk_generated(a: i64, b: i64, c: i64, d: i64) {}").expect("it loads");
    assert!(r.warnings.iter().any(|w| w.contains("on_chunk_generated")), "{:?}", r.warnings);
}

#[test]
fn bad_calls_to_the_new_host_functions_are_found_by_the_check() {
    let r = check_source("t.rs", "fn on_load() { open_crafting(); let n = rand_chunk(1, 2); }").expect("it loads");
    assert_eq!(r.mistakes.len(), 2, "{:?}", r.mistakes);
    let r = check_source("t.rs", "fn on_load() { open_crafting(3); open_inventory(); let n = rand_chunk(1, 2, 3); let s = world_seed(); }").expect("it loads");
    assert!(r.mistakes.is_empty(), "{:?}", r.mistakes);
}

#[test]
fn a_script_may_veto_a_click_and_a_burning() {
    let mut h = host("fn on_creature_interact(id: i64, kind: String, item: String) -> bool { item != \"stick\" }\nfn on_creature_sunburn(id: i64) -> bool { id != 7 }");
    let click = |item: &str| alloc::vec![Value::Int(3), Value::str("cow"), Value::str(item)];
    assert_eq!(answers(&mut h, "on_creature_interact", &click("stick")), [Value::Bool(false)]);
    assert_eq!(answers(&mut h, "on_creature_interact", &click("")), [Value::Bool(true)]);
    // A hook may leave off the parameters it does not need.
    assert_eq!(answers(&mut h, "on_creature_sunburn", &[Value::Int(7), Value::str("zombie")]), [Value::Bool(false)]);
    assert_eq!(answers(&mut h, "on_creature_sunburn", &[Value::Int(8), Value::str("zombie")]), [Value::Bool(true)]);
}

#[test]
fn a_script_that_returns_nothing_lets_things_go_ahead() {
    let mut h = host("fn on_creature_interact(id: i64) { log(\"seen\"); }");
    let got = answers(&mut h, "on_creature_interact", &[Value::Int(1), Value::str("cow"), Value::str("")]);
    assert!(got.is_empty(), "{:?}", got);
}

#[test]
fn the_placed_hook_hears_the_cause_as_text() {
    let mut h = host("fn on_block_placed(x: i64, y: i64, z: i64, id: String, cause: String) -> bool { cause == \"fluid\" && id == \"water\" && y == 64 }");
    let args = |cause: &str| alloc::vec![Value::Int(1), Value::Int(64), Value::Int(-2), Value::str("water"), Value::str(cause)];
    assert_eq!(answers(&mut h, "on_block_placed", &args("fluid")), [Value::Bool(true)]);
    assert_eq!(answers(&mut h, "on_block_placed", &args("player")), [Value::Bool(false)]);
}

#[test]
fn every_cause_the_world_names_is_in_the_docs() {
    let docs = include_str!("../../docs/SCRIPTING.md");
    for cause in crate::terrain::world_log::cause::ALL {
        assert!(docs.contains(&format!("`{}`", cause)), "the docs do not name the cause {}", cause);
    }
    for reason in crate::game::mob_hooks::reason::ALL {
        assert!(docs.contains(&format!("`{}`", reason)), "the docs do not name the reason {}", reason);
    }
}

#[test]
fn the_write_allowance_is_the_one_the_docs_give() {
    assert_eq!(crate::script::ext::structures::MAX_WRITES, 4096);
    let docs = include_str!("../../docs/SCRIPTING.md");
    assert!(docs.contains("**4096**"));
}
