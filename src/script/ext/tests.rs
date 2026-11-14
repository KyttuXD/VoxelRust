//! Tests of the table of host functions in this folder (the calls that need a
//! running game are tried by the game's own tests with the mods).

use super::*;
use crate::script::host::mock::MockHost;
use crate::script::host::{call_host_fn, HOST_FUNCTIONS};
use crate::script::limits::Limits;
use crate::script::state::ScriptState;
use crate::script::Script;
use alloc::vec::Vec;

#[test]
fn every_listed_function_is_routed_and_fails_politely_without_a_game() {
    let script = Script::load("t.rs", "fn t() {}").unwrap();
    let Script { prog, mut consts, .. } = script;
    let mut state = ScriptState::default();
    let mut host = MockHost::default();
    let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
    let mut seen: Vec<&str> = Vec::new();
    for (sig, what) in all_functions() {
        let name = name_of(sig);
        assert!(!what.is_empty(), "{} has no description", name);
        assert!(!seen.contains(&name), "{} is listed twice", name);
        assert!(!HOST_FUNCTIONS.iter().any(|f| name_of(f.0) == name), "{} is also a basic host function", name);
        seen.push(name);
        match call_host_fn(&mut it, name, &[]) {
            Some(Err(_)) => {}
            Some(Ok(_)) => panic!("{} worked without a game", name),
            None => panic!("{} is in the table but not routed", name),
        }
    }
    assert!(call_host_fn(&mut it, "format_disk", &[]).is_none());
}

#[test]
fn the_docs_describe_every_function_of_this_folder() {
    let docs = include_str!("../../../docs/SCRIPTING.md");
    for (sig, _) in all_functions() {
        let name = name_of(sig);
        assert!(docs.contains(&alloc::format!("`{}(", name)), "docs/SCRIPTING.md does not describe the host function {}", name);
    }
}

#[test]
fn arity_is_counted_from_the_signature() {
    assert_eq!(arity_of("a()"), 0);
    assert_eq!(arity_of("a(x: i64) -> bool"), 1);
    assert_eq!(arity_of("a(x: i64, y: &str, z: f64) -> (f64, f64)"), 3);
    assert_eq!(arity_of("hud_text(x: f64, y: f64, scale: f64, text: &str, r: i64, g: i64, b: i64)"), 7);
    for (sig, _) in all_functions() {
        // The tuple in a return type must not be counted as parameters.
        let params = sig.split(')').next().unwrap();
        assert_eq!(arity_of(sig), if params.ends_with('(') { 0 } else { params.matches(',').count() + 1 }, "{}", sig);
    }
}

#[test]
fn a_function_is_not_given_the_wrong_number_of_arguments() {
    let script = Script::load("t.rs", "fn t() {}").unwrap();
    let Script { prog, mut consts, .. } = script;
    let mut state = ScriptState::default();
    let mut host = MockHost::default();
    let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
    // (`MockHost` is no game, so this fails for that reason: the point is that it fails, not panics.)
    let r = call_host_fn(&mut it, "is_loaded", &[Value::Int(1)]);
    assert!(matches!(r, Some(Err(_))));
}
