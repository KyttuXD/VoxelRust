//! Regression tests from the audit of the engine: odd input that once
//! panicked or wrapped.

use crate::script::host::mock::MockHost;
use crate::script::interp::Interp;
use crate::script::limits::Limits;
use crate::script::state::ScriptState;
use crate::script::value::Value;
use crate::script::Script;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

fn run(body: &str) -> Result<Value, String> {
    let src = format!("fn t() -> bool {{ {} }}\n", body);
    let script = Script::load("t.rs", &src).map_err(|e| format!("{}", e))?;
    let Script { prog, mut consts, .. } = script;
    let mut host = MockHost::default();
    let mut state = ScriptState::default();
    let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
    match it.call_named("t", Vec::new()) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(String::from("no fn t")),
        Err(e) => Err(format!("{}", e)),
    }
}

#[test]
fn the_widest_range_is_not_empty_and_does_not_overflow() {
    let r = run("((-9223372036854775807 - 1)..9223372036854775807).is_empty()");
    assert_eq!(r, Ok(Value::Bool(false)));
}

#[test]
fn a_huge_range_cannot_be_walked_into_a_list() {
    // The size is worked out without overflowing, and then refused as too big.
    let r = run("let v: Vec<i64> = ((-9223372036854775807 - 1)..9223372036854775807).collect(); v.len() == 0");
    assert!(r.is_err(), "{:?}", r);
}

#[test]
fn a_slice_end_past_the_list_is_an_error_not_a_panic() {
    let r = run("let v = vec![1, 2, 3]; let s = &v[1..4294967297]; s.len() == 0");
    assert!(r.is_err(), "{:?}", r);
}
