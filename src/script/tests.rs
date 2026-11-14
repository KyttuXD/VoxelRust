//! Tests of the language: arithmetic, control flow, structs, vectors,
//! strings, formatting, references, the sandbox's limits, the host API (with
//! a mock host) and error messages.

use crate::script::host::mock::MockHost;
use crate::script::host::{call_host_fn, HOST_FUNCTIONS};
use crate::script::host_ui::UI_FUNCTIONS;
use crate::script::interp::Interp;
use crate::script::limits::Limits;
use crate::script::state::ScriptState;
use crate::script::value::Value;
use crate::script::{Script, ScriptHost, HOOKS};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Run `fn t() { body }` and give what it returns, or the error text.
fn try_run_with(body: &str, host: &mut MockHost, limits: Limits) -> Result<Value, String> {
    let src = format!("{}\n", body);
    let script = Script::load("t.rs", &src).map_err(|e| format!("{}", e))?;
    let Script { prog, mut consts, .. } = script;
    let mut state = ScriptState::default();
    let mut it = Interp::new(&prog, &mut consts, host, &mut state, "demo", limits);
    match it.call_named("t", Vec::new()) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(String::from("no fn t")),
        Err(e) => Err(format!("{}", e)),
    }
}

fn try_run(body: &str) -> Result<Value, String> {
    let mut host = MockHost::default();
    try_run_with(&format!("fn t() -> i64 {{ {} }}", body), &mut host, Limits::DEFAULT)
}

fn run(body: &str) -> Value {
    match try_run(body) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    }
}

/// A whole program with its own `fn t()`.
fn prog(src: &str) -> Value {
    let mut host = MockHost::default();
    match try_run_with(src, &mut host, Limits::DEFAULT) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    }
}

fn prog_err(src: &str) -> String {
    let mut host = MockHost::default();
    try_run_with(src, &mut host, Limits::DEFAULT).unwrap_err()
}

fn int(i: i64) -> Value {
    Value::Int(i)
}

fn text(s: &str) -> Value {
    Value::str(s)
}

// ---- arithmetic

#[test]
fn integer_arithmetic_follows_rust_precedence() {
    assert_eq!(run("1 + 2 * 3"), int(7));
    assert_eq!(run("(1 + 2) * 3"), int(9));
    assert_eq!(run("10 - 4 - 3"), int(3));
    assert_eq!(run("17 / 5 * 5 + 17 % 5"), int(17));
    assert_eq!(run("-7 / 2"), int(-3));
    assert_eq!(run("-7 % 3"), int(-1));
    assert_eq!(run("1 << 4 | 3"), int(19));
    assert_eq!(run("0xff & 0b1010"), int(10));
    assert_eq!(run("6 ^ 3"), int(5));
    assert_eq!(run("!0"), int(-1));
}

#[test]
fn floats_and_mixed_numbers() {
    assert_eq!(prog("fn t() -> f64 { 1.5 * 2.0 + 0.25 }"), Value::Float(3.25));
    assert_eq!(prog("fn t() -> f64 { 7.5 % 2.0 }"), Value::Float(1.5));
    assert_eq!(prog("fn t() -> f64 { 3 as f64 / 2.0 }"), Value::Float(1.5));
    assert_eq!(prog("fn t() -> f32 { let x: f32 = 2; x * 0.5 }"), Value::Float(1.0));
    assert_eq!(prog("fn t() -> i32 { 3.99 as i32 }"), int(3));
    assert_eq!(prog("fn t() -> i32 { (-3.99f64) as i32 }"), int(-3));
}

#[test]
fn comparisons_and_logic_short_circuit() {
    assert_eq!(run("if 1 < 2 && 2 <= 2 && 3 > 2 && 3 >= 3 && 1 != 2 { 1 } else { 0 }"), int(1));
    assert_eq!(run("if false && (1 / 0 == 0) { 1 } else { 2 }"), int(2));
    assert_eq!(run("if true || (1 / 0 == 0) { 1 } else { 2 }"), int(1));
    assert_eq!(prog("fn t() -> bool { !(1 == 1) }"), Value::Bool(false));
}

#[test]
fn casts_wrap_like_rust() {
    assert_eq!(run("300 as u8 as i64"), int(44));
    assert_eq!(run("-1i64 as u32 as i64"), int(4294967295));
    assert_eq!(run("'a' as u8 as i64"), int(97));
    assert_eq!(prog("fn t() -> char { 98u8 as char }"), Value::Char('b'));
}

#[test]
fn integer_methods() {
    assert_eq!(run("(-5i64).abs()"), int(5));
    assert_eq!(run("2i64.pow(10)"), int(1024));
    assert_eq!(run("7.min(3) + 7.max(3)"), int(10));
    assert_eq!(run("15.clamp(0, 10)"), int(10));
    assert_eq!(run("(-7i64).rem_euclid(3)"), int(2));
    assert_eq!(run("5.checked_div(0).unwrap_or(-1)"), int(-1));
    assert_eq!(run("i32::MAX as i64"), int(2147483647));
}

#[test]
fn float_methods() {
    assert_eq!(prog("fn t() -> f64 { 2.5f64.floor() + 2.5f64.ceil() + 2.5f64.round() }"), Value::Float(2.0 + 3.0 + 3.0));
    assert_eq!(prog("fn t() -> bool { (16.0f64.sqrt() - 4.0).abs() < 1e-12 }"), Value::Bool(true));
    assert_eq!(prog("fn t() -> f64 { (-2.5f64).abs().max(1.0).min(10.0) }"), Value::Float(2.5));
    assert_eq!(prog("fn t() -> bool { (0.0f64).sin().abs() < 1e-6 && ((0.0f64).cos() - 1.0).abs() < 1e-4 }"), Value::Bool(true));
    assert_eq!(prog("fn t() -> f64 { 2.0f64.powi(10) }"), Value::Float(1024.0));
    assert_eq!(prog("fn t() -> f64 { std::f64::consts::PI }"), Value::Float(core::f64::consts::PI));
}

// ---- variables and control flow

#[test]
fn let_shadowing_and_blocks() {
    assert_eq!(run("let x = 1; let x = x + 1; { let x = 10; } x"), int(2));
    assert_eq!(run("let mut x = 5; x += 3; x *= 2; x -= 1; x /= 3; x"), int(5));
    assert_eq!(run("let (a, b) = (3, 4); a * b"), int(12));
    assert_eq!(run("let y = { let a = 2; a * a }; y"), int(4));
}

#[test]
fn if_else_chains_are_expressions() {
    let src = "fn f(n: i64) -> &'static str { if n < 0 { \"neg\" } else if n == 0 { \"zero\" } else { \"pos\" } }\nfn t() -> String { format!(\"{}{}{}\", f(-1), f(0), f(5)) }";
    assert_eq!(prog(src), text("negzeropos"));
}

#[test]
fn while_loop_for_and_break_values() {
    assert_eq!(run("let mut i = 0; let mut s = 0; while i < 5 { s += i; i += 1; } s"), int(10));
    assert_eq!(run("let mut s = 0; for i in 0..5 { s += i; } s"), int(10));
    assert_eq!(run("let mut s = 0; for i in 0..=5 { s += i; } s"), int(15));
    assert_eq!(run("let mut n = 0; loop { n += 1; if n == 7 { break; } } n"), int(7));
    assert_eq!(run("let mut n = 0; let r = loop { n += 2; if n > 9 { break n * 10; } }; r"), int(100));
    assert_eq!(run("let mut s = 0; for i in 0..10 { if i % 2 == 0 { continue; } s += i; } s"), int(25));
}

#[test]
fn labelled_loops() {
    let body = "let mut n = 0; 'outer: for i in 0..10 { for j in 0..10 { if j == 3 { continue 'outer; } if i == 2 { break 'outer; } n += 1; } } n";
    assert_eq!(run(body), int(6));
}

#[test]
fn for_over_vec_chars_and_ranges() {
    assert_eq!(run("let v = vec![1, 2, 3]; let mut s = 0; for x in v { s += x; } s"), int(6));
    assert_eq!(run("let v = vec![1, 2, 3]; let mut s = 0; for x in &v { s += *x; } s + v.len() as i64"), int(9));
    assert_eq!(run("let mut n = 0; for c in \"héllo\".chars() { if c == 'l' { n += 1; } } n"), int(2));
    assert_eq!(prog("fn t() -> String { let mut s = String::new(); for c in 'a'..='e' { s.push(c); } s }"), text("abcde"));
    assert_eq!(run("let mut s = 0; for (i, x) in [10, 20, 30].iter().enumerate() { s += i as i64 * x; } s"), int(80));
}

#[test]
fn match_with_literals_ranges_guards_and_bindings() {
    let src = "fn f(n: i64) -> i64 { match n { 0 => 100, 1 | 2 => 200, 3..=9 => 300, x if x < 0 => -x, _ => 400 } }\nfn t() -> i64 { f(0) + f(2) + f(5) + f(-7) + f(50) }";
    assert_eq!(prog(src), int(100 + 200 + 300 + 7 + 400));
    assert_eq!(prog("fn t() -> i64 { match (1, 2) { (1, y) => y * 10, _ => 0 } }"), int(20));
    assert_eq!(prog("fn t() -> i64 { let x = 5; match x { n @ 1..=9 => n * 2, _ => 0 } }"), int(10));
    assert_eq!(prog("fn t() -> &'static str { match 'q' { 'a'..='m' => \"low\", 'n'..='z' => \"high\", _ => \"other\" } }"), text("high"));
    assert_eq!(prog("fn t() -> i64 { match \"hi\" { \"hi\" => 1, _ => 2 } }"), int(1));
}

#[test]
fn if_let_while_let_and_let_else() {
    assert_eq!(prog("fn t() -> i64 { let o = Some(4); if let Some(x) = o { x } else { 0 } }"), int(4));
    assert_eq!(prog("fn t() -> i64 { let mut v = vec![1, 2, 3]; let mut s = 0; while let Some(x) = v.pop() { s = s * 10 + x; } s }"), int(321));
    assert_eq!(prog("fn t() -> i64 { let o: Option<i64> = None; let Some(x) = o else { return -1 }; x }"), int(-1));
    assert_eq!(prog("fn t() -> bool { matches!(Some(3), Some(n) if n > 2) }"), Value::Bool(true));
}

// ---- functions

#[test]
fn functions_recursion_and_returns() {
    let src = "fn fib(n: i64) -> i64 { if n < 2 { return n; } fib(n - 1) + fib(n - 2) }\nfn t() -> i64 { fib(15) }";
    assert_eq!(prog(src), int(610));
    assert_eq!(prog("fn add(a: i64, b: i64) -> i64 { a + b }\nfn t() -> i64 { add(1, add(2, 3)) }"), int(6));
}

#[test]
fn constants_and_statics() {
    let src = "const N: i64 = 4;\nconst DOUBLE: i64 = N * 2;\nstatic NAME: &str = \"x\";\nfn t() -> i64 { DOUBLE + N + NAME.len() as i64 }";
    assert_eq!(prog(src), int(13));
    assert_eq!(prog("const HALF: f64 = 1;\nfn t() -> f64 { HALF / 2.0 }"), Value::Float(0.5));
}

#[test]
fn closures_capture_by_copy_and_can_be_passed() {
    assert_eq!(run("let k = 10; let add = |x: i64| x + k; add(5)"), int(15));
    assert_eq!(run("let f = |a, b| a * b; f(6, 7)"), int(42));
    assert_eq!(prog("fn apply(f: impl Fn(i64) -> i64, x: i64) -> i64 { f(f(x)) }\nfn t() -> i64 { apply(|v| v + 3, 1) }"), int(7));
    assert_eq!(prog("fn double(x: i64) -> i64 { x * 2 }\nfn t() -> i64 { vec![1, 2, 3].into_iter().map(double).sum::<i64>() }"), int(12));
    // A copy: changing the variable afterwards is not seen.
    assert_eq!(run("let mut k = 1; let f = || k; k = 5; f()"), int(1));
}

// ---- structs, enums, impls

const SHAPES: &str = "
#[derive(Debug, Clone)]
struct Point { x: i64, y: i64 }
impl Point {
    const ORIGIN: Point = Point { x: 0, y: 0 };
    fn new(x: i64, y: i64) -> Self { Point { x, y } }
    fn manhattan(&self) -> i64 { self.x.abs() + self.y.abs() }
    fn shift(&mut self, dx: i64) { self.x += dx; }
}
enum Shape { Circle(f64), Rect { w: f64, h: f64 }, Empty }
impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => 3.0 * r * r,
            Shape::Rect { w, h } => w * h,
            Shape::Empty => 0.0,
        }
    }
}
";

#[test]
fn structs_methods_and_associated_items() {
    assert_eq!(prog(&format!("{}fn t() -> i64 {{ let p = Point::new(3, -4); p.manhattan() }}", SHAPES)), int(7));
    assert_eq!(prog(&format!("{}fn t() -> i64 {{ let mut p = Point::new(1, 1); p.shift(5); p.shift(2); p.x }}", SHAPES)), int(8));
    assert_eq!(prog(&format!("{}fn t() -> i64 {{ Point::ORIGIN.manhattan() + Point {{ x: 2, ..Point::ORIGIN }}.manhattan() }}", SHAPES)), int(2));
}

#[test]
fn struct_values_copy_on_assignment() {
    let src = format!("{}fn t() -> i64 {{ let a = Point::new(1, 2); let mut b = a.clone(); b.x = 100; a.x * 1000 + b.x }}", SHAPES);
    assert_eq!(prog(&src), int(1100));
}

#[test]
fn enums_with_data_and_match() {
    let src = format!("{}fn t() -> f64 {{ Shape::Circle(2.0).area() + Shape::Rect {{ w: 2.0, h: 3.0 }}.area() + Shape::Empty.area() }}", SHAPES);
    assert_eq!(prog(&src), Value::Float(18.0));
}

#[test]
fn enum_discriminants_and_unit_variants() {
    let src = "#[derive(PartialEq, Clone, Copy)]\nenum Dir { North = 1, East, South = 10, West }\nfn t() -> i64 { let d = Dir::East; let same = d == Dir::East; Dir::West as i64 + d as i64 * 100 + if same { 1000 } else { 0 } }";
    assert_eq!(prog(src), int(11 + 200 + 1000));
}

#[test]
fn tuple_structs_and_tuples() {
    assert_eq!(prog("struct Meters(i64);\nfn t() -> i64 { let m = Meters(5); m.0 + 1 }"), int(6));
    assert_eq!(prog("fn t() -> i64 { let t = ((1, 2), 3); t.0.1 + t.1 }"), int(5));
    assert_eq!(prog("fn t() -> i64 { let mut t = (1, 2); t.0 = 10; t.0 + t.1 }"), int(12));
}

#[test]
fn debug_of_structs_and_enums() {
    let src = format!("{}fn t() -> String {{ format!(\"{{:?}} {{:?}}\", Point::new(1, 2), Shape::Circle(1.5)) }}", SHAPES);
    assert_eq!(prog(&src), text("Point { x: 1, y: 2 } Circle(1.5)"));
}

// ---- Vec

#[test]
fn vec_basics() {
    assert_eq!(run("let mut v = Vec::new(); v.push(3); v.push(1); v.push(2); v.len() as i64 * 100 + v[0] * 10 + v[2]"), int(332));
    assert_eq!(run("let mut v = vec![1, 2, 3]; let a = v.pop().unwrap(); v.insert(0, 9); v.remove(1); a * 100 + v[0] * 10 + v.len() as i64"), int(392));
    assert_eq!(run("let v = vec![5, 6, 7]; v.contains(&6) as i64 + v.contains(&9) as i64"), int(1));
    assert_eq!(run("let mut v = vec![1, 2]; v.clear(); v.len() as i64 + v.is_empty() as i64"), int(1));
    assert_eq!(run("let v = [1, 2, 3, 4, 5]; let s = &v[1..4]; s.len() as i64 * 10 + s[0]"), int(32));
    assert_eq!(run("let v = vec![0; 4]; v.len() as i64"), int(4));
    assert_eq!(prog("fn t() -> i64 { let v = vec![7, 8]; match v.get(5) { Some(_) => 1, None => v.get(1).copied().unwrap_or(0) } }"), int(8));
}

#[test]
fn vec_sorting_dedup_and_retain() {
    assert_eq!(prog("fn t() -> String { let mut v = vec![3, 1, 2]; v.sort(); format!(\"{:?}\", v) }"), text("[1, 2, 3]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![3, 1, 2]; v.sort_by(|a, b| b.cmp(a)); format!(\"{:?}\", v) }"), text("[3, 2, 1]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![\"bb\", \"a\", \"ccc\"]; v.sort_by_key(|s| s.len()); format!(\"{:?}\", v) }"), text("[\"a\", \"bb\", \"ccc\"]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![1.5, -2.0, 0.5]; v.sort_by(|a, b| a.partial_cmp(b).unwrap()); format!(\"{:?}\", v) }"), text("[-2.0, 0.5, 1.5]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![1, 1, 2, 2, 2, 3, 1]; v.dedup(); format!(\"{:?}\", v) }"), text("[1, 2, 3, 1]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![1, 2, 3, 4, 5, 6]; v.retain(|x| x % 2 == 0); format!(\"{:?}\", v) }"), text("[2, 4, 6]"));
    assert_eq!(prog("fn t() -> String { let mut v = vec![1, 2, 3]; v.reverse(); format!(\"{:?}\", v) }"), text("[3, 2, 1]"));
}

#[test]
fn sort_is_stable() {
    let src = "fn t() -> String { let mut v = vec![(1, 'a'), (0, 'b'), (1, 'c'), (0, 'd')]; v.sort_by_key(|p| p.0); let s: String = v.iter().map(|p| p.1).collect(); s }";
    assert_eq!(prog(src), text("bdac"));
}

#[test]
fn iterator_chains() {
    assert_eq!(run("vec![1, 2, 3, 4].iter().map(|x| x * x).sum::<i64>()"), int(30));
    assert_eq!(run("(1..=5).filter(|x| x % 2 == 1).map(|x| x * 10).sum::<i64>()"), int(90));
    assert_eq!(run("vec![1, 2, 3].iter().fold(0, |acc, x| acc * 10 + x)"), int(123));
    assert_eq!(run("(1..4).rev().map(|x| x as i64).fold(0, |a, x| a * 10 + x)"), int(321));
    assert_eq!(run("vec![3, 9, 2].iter().max().copied().unwrap() * 10 + vec![3, 9, 2].iter().min().copied().unwrap()"), int(92));
    assert_eq!(run("vec![1, 2, 3].iter().any(|x| *x > 2) as i64 * 10 + vec![1, 2, 3].iter().all(|x| *x > 2) as i64"), int(10));
    assert_eq!(run("vec![4, 5, 6].iter().position(|x| *x == 5).unwrap() as i64"), int(1));
    assert_eq!(run("vec![1, 2, 3, 4, 5].iter().skip(1).take(3).count() as i64"), int(3));
    assert_eq!(run("vec![1, 2, 3].iter().zip(vec![10, 20, 30].iter()).map(|(a, b)| a * b).sum::<i64>()"), int(140));
    assert_eq!(prog("fn t() -> String { vec![\"a\", \"b\", \"c\"].join(\"-\") }"), text("a-b-c"));
    assert_eq!(prog("fn t() -> String { let v: Vec<i64> = (1..4).collect(); format!(\"{:?}\", v) }"), text("[1, 2, 3]"));
    assert_eq!(prog("fn t() -> String { \"hello\".chars().rev().collect::<String>() }"), text("olleh"));
    assert_eq!(prog("fn t() -> String { let s: String = \"abc\".chars().map(|c| c.to_ascii_uppercase()).collect(); s }"), text("ABC"));
}

#[test]
fn windows_and_chunks_and_flatten() {
    assert_eq!(run("vec![1, 2, 3, 4].windows(2).map(|w| w[1] - w[0]).sum::<i64>()"), int(3));
    assert_eq!(run("vec![1, 2, 3, 4, 5].chunks(2).count() as i64"), int(3));
    assert_eq!(run("vec![vec![1, 2], vec![3]].into_iter().flatten().sum::<i64>()"), int(6));
    assert_eq!(run("vec![1, 2, 3].iter().flat_map(|x| vec![*x, *x]).count() as i64"), int(6));
    assert_eq!(run("vec![Some(1), None, Some(3)].into_iter().flatten().sum::<i64>()"), int(4));
}

// ---- strings and formatting

#[test]
fn string_methods() {
    assert_eq!(run("\"hello\".len() as i64"), int(5));
    assert_eq!(prog("fn t() -> String { let mut s = String::from(\"ab\"); s.push('c'); s.push_str(\"de\"); s }"), text("abcde"));
    assert_eq!(prog("fn t() -> String { \"Hello\".to_uppercase() + &\"WORLD\".to_lowercase() }"), text("HELLOworld"));
    assert_eq!(run("\"hello world\".contains(\"o w\") as i64 + \"hello\".starts_with(\"he\") as i64 + \"hello\".ends_with(\"x\") as i64"), int(2));
    assert_eq!(prog("fn t() -> String { \"a,b,,c\".split(',').map(|s| s.to_string()).collect::<Vec<String>>().join(\"|\") }"), text("a|b||c"));
    assert_eq!(prog("fn t() -> String { \"  pad \".trim().to_string() }"), text("pad"));
    assert_eq!(prog("fn t() -> String { \"a-b\".replace(\"-\", \"+\") }"), text("a+b"));
    assert_eq!(prog("fn t() -> i64 { \"abcdef\".find(\"cd\").unwrap() as i64 }"), int(2));
    assert_eq!(prog("fn t() -> String { let s = \"hello\"; s[1..3].to_string() }"), text("el"));
    assert_eq!(prog("fn t() -> String { \"ab\".repeat(3) }"), text("ababab"));
    assert_eq!(prog("fn t() -> String { let w: Vec<&str> = \"one two  three\".split_whitespace().collect(); format!(\"{}\", w.len()) }"), text("3"));
}

#[test]
fn string_comparison_and_concatenation() {
    assert_eq!(prog("fn t() -> bool { \"abc\" < \"abd\" && String::from(\"x\") == \"x\" }"), Value::Bool(true));
    assert_eq!(prog("fn t() -> String { let a = String::from(\"x\"); let b = a.clone() + \"y\"; a + &b }"), text("xxy"));
    assert_eq!(prog("fn t() -> String { let mut s = String::new(); s += \"ab\"; s += \"cd\"; s }"), text("abcd"));
}

#[test]
fn parsing_text_into_numbers() {
    assert_eq!(prog("fn t() -> i64 { \"42\".parse::<i64>().unwrap() + 1 }"), int(43));
    assert_eq!(prog("fn t() -> i64 { let n: i64 = \"7\".parse().unwrap(); n * 2 }"), int(14));
    assert_eq!(prog("fn t() -> f64 { \"2.5\".parse::<f64>().unwrap() * 2.0 }"), Value::Float(5.0));
    assert_eq!(prog("fn t() -> i64 { \"abc\".parse::<i64>().unwrap_or(-1) }"), int(-1));
    assert_eq!(prog("fn t() -> bool { \"300\".parse::<u8>().is_err() }"), Value::Bool(true));
    assert_eq!(prog("fn t() -> i64 { match \"x\".parse::<i32>() { Ok(n) => n as i64, Err(_) => -5 } }"), int(-5));
}

#[test]
fn format_macro() {
    assert_eq!(prog("fn t() -> String { format!(\"{} and {}\", 1, \"two\") }"), text("1 and two"));
    assert_eq!(prog("fn t() -> String { let x = 5; let name = \"n\"; format!(\"{x} {name} {0} {0:?}\", name) }"), text("5 n n \"n\""));
    assert_eq!(prog("fn t() -> String { format!(\"{:?} {:?} {:?}\", 1.0, \"s\", 'c') }"), text("1.0 \"s\" 'c'"));
    assert_eq!(prog("fn t() -> String { format!(\"[{:>5}][{:<5}][{:^5}][{:05}]\", 42, 42, 42, 42) }"), text("[   42][42   ][ 42  ][00042]"));
    assert_eq!(prog("fn t() -> String { format!(\"{:.2} {:.0} {}\", 3.14159, 2.5, 1.5) }"), text("3.14 2 1.5"));
    assert_eq!(prog("fn t() -> String { format!(\"{{}} {:x} {:b}\", 255, 5) }"), text("{} ff 101"));
    assert_eq!(prog("fn t() -> String { format!(\"{:?}\", vec![Some(1), None]) }"), text("[Some(1), None]"));
    assert_eq!(prog("fn t() -> String { format!(\"{:?}\", (1, \"a\")) }"), text("(1, \"a\")"));
    assert_eq!(prog("fn t() -> String { format!(\"{n}!\", n = 3) }"), text("3!"));
}

// ---- Option and Result

#[test]
fn option_methods() {
    assert_eq!(prog("fn t() -> i64 { Some(3).map(|x| x * 2).unwrap_or(0) + None.unwrap_or(10) }"), int(16));
    assert_eq!(prog("fn t() -> i64 { Some(5).and_then(|x| if x > 3 { Some(x) } else { None }).unwrap_or(-1) }"), int(5));
    assert_eq!(prog("fn t() -> bool { Some(1).is_some() && None::<i64>.is_none() }"), Value::Bool(true));
    assert_eq!(prog("fn t() -> i64 { let mut o = Some(3); let a = o.take(); a.unwrap() * 10 + o.is_none() as i64 }"), int(31));
    assert_eq!(prog("fn t() -> i64 { Some(2).ok_or(\"no\").unwrap() }"), int(2));
    assert_eq!(prog("fn t() -> i64 { Some(4).filter(|x| *x > 5).unwrap_or(0) }"), int(0));
}

#[test]
fn result_and_the_question_mark() {
    let src = "fn parse(s: &str) -> Result<i64, String> { let n = s.parse::<i64>().map_err(|e| format!(\"bad: {}\", e))?; Ok(n * 2) }\nfn t() -> String { format!(\"{:?} {:?}\", parse(\"21\"), parse(\"x\")) }";
    assert_eq!(prog(src), text("Ok(42) Err(\"bad: invalid digit found in string\")"));
    let src = "fn first(v: &Vec<i64>) -> Option<i64> { let f = v.first()?; Some(*f + 1) }\nfn t() -> i64 { first(&vec![9]).unwrap() + first(&vec![]).unwrap_or(0) }";
    assert_eq!(prog(src), int(10));
}

#[test]
fn unwrap_on_none_is_an_error_not_a_crash() {
    let e = prog_err("fn t() -> i64 {\n    let o: Option<i64> = None;\n    o.unwrap()\n}");
    assert!(e.starts_with("t.rs:3:"), "{}", e);
    assert!(e.contains("unwrap"), "{}", e);
}

// ---- references

#[test]
fn mutable_references_reach_the_callers_variable() {
    assert_eq!(prog("fn inc(x: &mut i64) { *x += 1; }\nfn t() -> i64 { let mut a = 1; inc(&mut a); inc(&mut a); a }"), int(3));
    assert_eq!(prog("fn fill(v: &mut Vec<i64>) { v.push(1); v.push(2); }\nfn t() -> i64 { let mut v = vec![]; fill(&mut v); v.len() as i64 }"), int(2));
    assert_eq!(prog("fn swap_ends(v: &mut Vec<i64>) { let n = v.len(); v.swap(0, n - 1); }\nfn t() -> i64 { let mut v = vec![1, 2, 3]; swap_ends(&mut v); v[0] * 10 + v[2] }"), int(31));
    assert_eq!(prog("fn t() -> i64 { let mut a = 1; let mut b = 2; std::mem::swap(&mut a, &mut b); a * 10 + b }"), int(21));
}

#[test]
fn iter_mut_and_references_into_vectors() {
    assert_eq!(prog("fn t() -> i64 { let mut v = vec![1, 2, 3]; for x in v.iter_mut() { *x *= 10; } v.iter().sum::<i64>() }"), int(60));
    assert_eq!(prog("fn t() -> i64 { let mut v = vec![1, 2, 3]; for x in &mut v { *x += 1; } v[0] + v[1] + v[2] }"), int(9));
    assert_eq!(prog("fn t() -> i64 { let mut v = vec![1, 2, 3]; if let Some(x) = v.get_mut(1) { *x = 50; } v[1] }"), int(50));
    assert_eq!(prog("fn t() -> i64 { let mut v = vec![1, 2, 3]; if let Some(l) = v.last_mut() { *l = 9; } v[2] }"), int(9));
}

#[test]
fn matching_through_a_mutable_reference() {
    let src = "enum S { Count(i64), Done }\nstruct M { s: S }\nimpl M { fn bump(&mut self) { match &mut self.s { S::Count(n) => { *n += 1; } S::Done => {} } } }\nfn t() -> i64 { let mut m = M { s: S::Count(1) }; m.bump(); m.bump(); match m.s { S::Count(n) => n, S::Done => -1 } }";
    assert_eq!(prog(src), int(3));
}

#[test]
fn fields_of_vec_elements_and_nested_changes() {
    let src = "struct P { hp: i64 }\nstruct W { ps: Vec<P> }\nfn t() -> i64 { let mut w = W { ps: vec![P { hp: 1 }, P { hp: 2 }] }; w.ps[1].hp += 40; w.ps[0].hp = 7; w.ps[0].hp + w.ps[1].hp }";
    assert_eq!(prog(src), int(49));
}

#[test]
fn copy_on_write_keeps_values_apart() {
    assert_eq!(run("let a = vec![1, 2]; let mut b = a.clone(); b.push(3); a.len() as i64 * 10 + b.len() as i64"), int(23));
    assert_eq!(run("let a = vec![1, 2]; let mut b = a; b[0] = 9; b[0]"), int(9));
}

// ---- the sandbox

#[test]
fn an_endless_loop_stops_with_an_error() {
    let e = prog_err("fn t() { loop { } }");
    assert!(e.contains("budget"), "{}", e);
    assert!(e.starts_with("t.rs:1:"), "{}", e);
    let e = prog_err("fn t() { let mut i = 0; while true { i += 1; } }");
    assert!(e.contains("budget"), "{}", e);
    let e = prog_err("fn t() { for _ in 0..1000000000 { } }");
    assert!(e.contains("budget"), "{}", e);
}

#[test]
fn endless_recursion_stops_with_an_error() {
    let e = prog_err("fn f(n: i64) -> i64 { f(n + 1) + 1 }\nfn t() -> i64 { f(0) }");
    assert!(e.contains("nested too deeply"), "{}", e);
}

#[test]
fn deep_but_finite_recursion_works() {
    assert_eq!(prog("fn sum(n: i64) -> i64 { if n == 0 { 0 } else { n + sum(n - 1) } }\nfn t() -> i64 { sum(30) }"), int(465));
}

#[test]
fn memory_is_capped() {
    let e = prog_err("fn t() { let mut v = Vec::new(); loop { v.push(1); } }");
    assert!(e.contains("Vec may not hold") || e.contains("budget"), "{}", e);
    let e = prog_err("fn t() { let mut s = String::from(\"x\"); loop { s = s.clone() + &s; } }");
    assert!(e.contains("longer than"), "{}", e);
    let e = prog_err("fn t() { let v = vec![0; 100000000]; }");
    assert!(e.contains("Vec may not hold"), "{}", e);
    let e = prog_err("fn t() { let s = \"ab\".repeat(100000000); }");
    assert!(e.contains("longer than"), "{}", e);
}

#[test]
fn deeply_nested_expressions_do_not_overflow_the_stack() {
    let mut src = String::from("fn t() -> i64 { ");
    for _ in 0..20 {
        src.push_str("(1 + ");
    }
    src.push('0');
    for _ in 0..20 {
        src.push(')');
    }
    src.push_str(" }");
    assert_eq!(prog(&src), int(20));
    // Past what the parser accepts it is an error, not a crash.
    let mut deep = String::from("fn t() -> i64 { ");
    for _ in 0..1000 {
        deep.push_str("(1 + ");
    }
    deep.push('0');
    for _ in 0..1000 {
        deep.push(')');
    }
    deep.push_str(" }");
    assert!(prog_err(&deep).contains("nested too deeply"));
}

#[test]
fn limits_can_be_tightened() {
    let mut host = MockHost::default();
    let tight = Limits { max_steps: 100, ..Limits::DEFAULT };
    let e = try_run_with("fn t() { let mut i = 0; while i < 1000 { i += 1; } }", &mut host, tight).unwrap_err();
    assert!(e.contains("budget of 100 steps"), "{}", e);
}

// ---- errors say where

#[test]
fn runtime_errors_have_file_and_line() {
    let e = prog_err("fn t() -> i64 {\n    let a = 1;\n    let b = 0;\n    a / b\n}");
    assert_eq!(e, "t.rs:4: attempt to divide by zero");
    let e = prog_err("fn t() -> i64 {\n    let v = vec![1];\n    v[3]\n}");
    assert!(e.starts_with("t.rs:3:") && e.contains("index out of bounds"), "{}", e);
    let e = prog_err("fn t() -> i64 {\n    nothing(1)\n}");
    assert!(e.starts_with("t.rs:2:") && e.contains("nothing"), "{}", e);
    let e = prog_err("fn t() -> i64 {\n    let x = y;\n    1\n}");
    assert!(e.contains("cannot find the name `y`"), "{}", e);
    let e = prog_err("fn t() -> i64 {\n    1 + \"a\"\n}");
    assert!(e.starts_with("t.rs:2:") && e.contains("integer and string"), "{}", e);
    let e = prog_err("fn t() {\n    panic!(\"boom {}\", 7);\n}");
    assert_eq!(e, "t.rs:2: the script panicked: boom 7");
    let e = prog_err("fn t() {\n    assert_eq!(1 + 1, 3);\n}");
    assert!(e.contains("left: 2") && e.contains("right: 3"), "{}", e);
    let e = prog_err("fn t() {\n    let v: Vec<i64> = Vec::new();\n    v.frobnicate();\n}");
    assert!(e.contains("no method `frobnicate`"), "{}", e);
}

#[test]
fn parse_errors_have_file_line_and_column() {
    let e = prog_err("fn t() {\n    let x = ;\n}");
    assert!(e.starts_with("t.rs:2:13:"), "{}", e);
}

#[test]
fn integer_overflow_is_an_error() {
    let e = prog_err("fn t() -> i64 { let x = 9223372036854775807; x + 1 }");
    assert!(e.contains("overflow"), "{}", e);
}

// ---- the host API, with a mock host

fn host_run(body: &str, host: &mut MockHost) -> Result<Value, String> {
    try_run_with(&format!("fn t() {{ {} }}", body), host, Limits::DEFAULT)
}

#[test]
fn host_functions_reach_the_host() {
    let mut h = MockHost::default();
    host_run("say(\"hi\"); give(\"stick\", 3); spawn(\"wolf\", 1.5, 64.0, -2.0); set_block(1, 2, 3, \"stone\"); play_sound(\"boom\"); explode(0.0, 70.0, 0.0, 2.0); effect(\"poison\", 5.0, 2); set_time(0.5); log(\"x\");", &mut h).unwrap();
    assert_eq!(
        h.calls,
        ["say hi", "give stick 3", "spawn wolf 1.5 64 -2", "set_block 1 2 3 stone", "sound boom", "explode 0 70 0 2", "effect poison 5 2", "set_time 0.5", "log x"]
    );
}

#[test]
fn effects_can_be_given_removed_and_asked_about_for_the_player_or_the_target() {
    let mut h = MockHost::default();
    let v = try_run_with(
        "fn t() -> String { apply_effect(\"player\", \"poison\", 5.0, 2); apply_effect(\"target\", \"slowness\", 0.0, 1); remove_effect(\"player\", \"\"); format!(\"{} {} {} {}\", has_effect(\"player\", \"poison\"), has_effect(\"player\", \"burning\"), effect_level(\"player\", \"poison\"), effect_level(\"target\", \"poison\")) }",
        &mut h,
        Limits::DEFAULT,
    )
    .unwrap();
    assert_eq!(v, text("true false 2 0"));
    assert_eq!(h.calls, ["apply_effect player poison 5 2", "apply_effect target slowness 0 1", "remove_effect player "]);
}

#[test]
fn the_effect_hooks_may_be_scripted() {
    let mut h = hook_host("fn on_effect_start(id: String, level: i64) { say(id); }\nfn on_effect_tick(id: String, secs_left: f32, level: i64) { say(\"tick\"); }\nfn on_effect_end(id: String) { say(\"end\"); }");
    let mut m = MockHost::default();
    call(&mut h, &mut m, "on_effect_start", &[text("poison"), int(2)]);
    call(&mut h, &mut m, "on_effect_tick", &[text("poison"), Value::Float(4.5), int(2)]);
    call(&mut h, &mut m, "on_effect_end", &[text("poison")]);
    assert_eq!(m.calls, ["say poison", "say tick", "say end"]);
}

#[test]
fn host_functions_return_values() {
    let mut h = MockHost { pos: (1.5, 64.0, -3.0), time: 0.25, health: 12, ..MockHost::default() };
    h.items.push((String::from("coal"), 7));
    let v = try_run_with("fn t() -> String { set_block(5, 6, 7, \"dirt\"); let (x, y, z) = player_pos(); format!(\"{} {} {} {} {} {} {} {}\", x, y, z, time(), get_block(5, 6, 7), get_block(0, 0, 0), item_count(\"coal\"), player_health()) }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, text("1.5 64 -3 0.25 dirt air 7 12"));
    let v = try_run_with("fn t() -> bool { give(\"nothing\", 1) }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(false));
    let v = try_run_with("fn t() -> String { held_item() }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, text("stick"));
}

#[test]
fn run_command_and_rand() {
    let mut h = MockHost { reply: String::from("done"), ..MockHost::default() };
    let v = try_run_with("fn t() -> String { run_command(\"/time day\") }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, text("done"));
    assert_eq!(h.commands, ["/time day"]);
    let v = try_run_with("fn t() -> bool { let mut ok = true; for _ in 0..50 { let r = rand(6); if r < 0 || r >= 6 { ok = false; } } ok && rand(0) == 0 }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn heal_and_hurt() {
    let mut h = MockHost { health: 10, ..MockHost::default() };
    host_run("heal(5); hurt(2);", &mut h).unwrap();
    assert_eq!(h.health, 13);
}

#[test]
fn state_persists_across_calls_in_the_same_state() {
    let src = "fn bump() -> i64 { let n = state_get_int(\"n\", 0) + 1; state_set_int(\"n\", n); n }\nfn name() { state_set(\"who\", \"me\"); }";
    let script = Script::load("t.rs", src).unwrap();
    let Script { prog, mut consts, .. } = script;
    let mut state = ScriptState::default();
    let mut host = MockHost::default();
    for want in 1..=3 {
        let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
        assert_eq!(it.call_named("bump", Vec::new()).unwrap(), Some(int(want)));
    }
    let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
    it.call_named("name", Vec::new()).unwrap();
    assert_eq!(state.get("who"), Some("me"));
    assert_eq!(state.get("n"), Some("3"));
    assert!(state.dirty);
}

#[test]
fn host_calls_with_wrong_arguments_are_errors() {
    let mut h = MockHost::default();
    let e = host_run("say(5);", &mut h).unwrap_err();
    assert!(e.contains("`say`") && e.contains("string"), "{}", e);
    let e = host_run("give(\"a\");", &mut h).unwrap_err();
    assert!(e.contains("takes 2"), "{}", e);
    let e = host_run("state_set(\"\", \"x\");", &mut h).unwrap_err();
    assert!(e.contains("state key"), "{}", e);
}

#[test]
fn every_host_function_in_the_table_is_callable_and_nothing_else_is() {
    let script = Script::load("t.rs", "fn t() {}").unwrap();
    let Script { prog, mut consts, .. } = script;
    let mut state = ScriptState::default();
    let mut host = MockHost::default();
    let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
    for (sig, what) in HOST_FUNCTIONS.iter().chain(UI_FUNCTIONS) {
        let name = sig.split('(').next().unwrap();
        assert!(!what.is_empty());
        assert!(call_host_fn(&mut it, name, &[]).is_some(), "{} is in the table but not implemented", name);
    }
    assert!(call_host_fn(&mut it, "format_disk", &[]).is_none());
    let mut seen = Vec::new();
    for (sig, _) in HOST_FUNCTIONS.iter().chain(UI_FUNCTIONS) {
        let name = sig.split('(').next().unwrap();
        assert!(!seen.contains(&name), "{} is listed twice", name);
        seen.push(name);
    }
}

#[test]
fn the_docs_list_every_host_function_and_hook() {
    let docs = include_str!("../../docs/SCRIPTING.md");
    for (sig, _) in HOST_FUNCTIONS.iter().chain(UI_FUNCTIONS) {
        let name = sig.split('(').next().unwrap();
        assert!(docs.contains(&format!("`{}(", name)), "docs/SCRIPTING.md does not describe the host function {}", name);
    }
    for h in HOOKS {
        assert!(docs.contains(&format!("`fn {}(", h.name)) || docs.contains(&format!("`{}(", h.name)), "docs/SCRIPTING.md does not describe the hook {}", h.name);
    }
}

#[test]
fn the_examples_in_the_docs_load() {
    let docs = include_str!("../../docs/SCRIPTING.md");
    let mut rest = docs;
    let mut n = 0;
    while let Some(start) = rest.find("```rust\n") {
        let after = &rest[start + 8..];
        let end = after.find("```").expect("a code block is never closed");
        let code = &after[..end];
        if let Err(e) = Script::load("doc.rs", code) {
            panic!("{}\n{}", e, code);
        }
        n += 1;
        rest = &after[end + 3..];
    }
    assert!(n >= 5, "found only {} examples", n);
}

#[test]
fn hook_names_are_unique() {
    let mut seen = Vec::new();
    for h in HOOKS {
        assert!(h.name.starts_with("on_"));
        assert!(!seen.contains(&h.name));
        seen.push(h.name);
    }
}

// ---- hooks, through a ScriptHost

fn files(name: &str, src: &str) -> Vec<(String, Vec<u8>)> {
    alloc::vec![(format!("scripts/{}.rs", name), src.as_bytes().to_vec())]
}

fn hook_host(src: &str) -> ScriptHost {
    let mut h = ScriptHost::new();
    let problems = h.add_mod("demo", &files("main", src));
    assert!(problems.is_empty(), "{:?}", problems);
    h
}

fn call(h: &mut ScriptHost, mock: &mut MockHost, hook: &str, args: &[Value]) -> Vec<Value> {
    let mut results = Vec::new();
    h.call_hook(mock, "world", hook, args, &mut |v| {
        results.push(v.clone());
        false
    });
    results
}

#[test]
fn a_hook_can_answer_and_cancel() {
    let mut h = hook_host("fn on_block_break(x: i64, y: i64, z: i64, id: String) -> bool { id != \"bedrock\" }");
    let mut m = MockHost::default();
    let args = |id: &str| alloc::vec![int(1), int(2), int(3), text(id)];
    assert_eq!(call(&mut h, &mut m, "on_block_break", &args("stone")), [Value::Bool(true)]);
    assert_eq!(call(&mut h, &mut m, "on_block_break", &args("bedrock")), [Value::Bool(false)]);
}

#[test]
fn a_hook_may_leave_off_parameters_it_does_not_need() {
    let mut h = hook_host("fn on_block_break(x: i64) -> bool { x != 1 }");
    let mut m = MockHost::default();
    assert_eq!(call(&mut h, &mut m, "on_block_break", &[int(1), int(2), int(3), text("a")]), [Value::Bool(false)]);
}

#[test]
fn on_load_runs_once_per_world_and_state_is_per_mod() {
    let mut h = hook_host("fn on_load() { state_set_int(\"loads\", state_get_int(\"loads\", 0) + 1); say(\"loaded\"); }\nfn on_tick(dt: f32) { say(\"tick\"); }");
    let mut m = MockHost::default();
    call(&mut h, &mut m, "on_tick", &[Value::Float(0.1)]);
    call(&mut h, &mut m, "on_tick", &[Value::Float(0.1)]);
    assert_eq!(m.calls, ["say loaded", "say tick", "say tick"]);
    // Another world: on_load again, with that world's state.
    h.call_hook(&mut m, "other", "on_tick", &[Value::Float(0.1)], &mut |_| false);
    assert_eq!(m.calls.iter().filter(|c| *c == "say loaded").count(), 2);
    assert_eq!(h.mods[0].state.get("loads"), Some("1"));
}

#[test]
fn a_failing_script_is_logged_and_switched_off() {
    let mut h = hook_host("fn on_chat(text: String) -> bool { let v: Vec<i64> = vec![]; v[0] == 1 }\nfn on_tick() { say(\"alive\"); }");
    let mut m = MockHost::default();
    assert!(call(&mut h, &mut m, "on_chat", &[text("hi")]).is_empty());
    assert_eq!(h.problems.len(), 1);
    assert!(h.problems[0].starts_with("demo/scripts/main.rs:1:"), "{}", h.problems[0]);
    assert!(h.mods[0].scripts[0].disabled.is_some());
    // Switched off: nothing of it runs again, not even the other hook.
    call(&mut h, &mut m, "on_tick", &[]);
    assert!(m.calls.is_empty());
    assert!(!h.any_working());
}

#[test]
fn an_endless_hook_is_stopped_and_switched_off() {
    let mut h = hook_host("fn on_tick(dt: f32) { loop {} }");
    let mut m = MockHost::default();
    call(&mut h, &mut m, "on_tick", &[Value::Float(0.1)]);
    assert!(h.problems[0].contains("budget"), "{:?}", h.problems);
    assert!(!h.any_working());
}

#[test]
fn the_first_script_to_object_ends_the_asking() {
    let mut h = ScriptHost::new();
    h.add_mod("a", &files("a", "fn on_chat(t: String) -> bool { false }"));
    h.add_mod("b", &files("b", "fn on_chat(t: String) -> bool { say(\"b heard\"); true }"));
    let mut m = MockHost::default();
    let mut asked = 0;
    h.call_hook(&mut m, "w", "on_chat", &[text("x")], &mut |v| {
        asked += 1;
        matches!(v, Value::Bool(false))
    });
    assert_eq!(asked, 1);
    assert!(m.calls.is_empty());
}

#[test]
fn commands_hook_example() {
    let src = "fn on_command(name: String, args: String) -> bool {\n    if name == \"patch\" {\n        heal(4);\n        say(&format!(\"patched {}\", args));\n        return true;\n    }\n    false\n}";
    let mut h = hook_host(src);
    let mut m = MockHost { health: 3, ..MockHost::default() };
    assert_eq!(call(&mut h, &mut m, "on_command", &[text("patch"), text("arm")]), [Value::Bool(true)]);
    assert_eq!(call(&mut h, &mut m, "on_command", &[text("other"), text("")]), [Value::Bool(false)]);
    assert_eq!(m.health, 7);
    assert_eq!(m.calls, ["say patched arm"]);
}

#[test]
fn boss_reward_example() {
    let src = "const BOSS: &str = \"wither_boss\";\nfn on_entity_death(kind: String, x: f64, y: f64, z: f64) {\n    if kind == BOSS {\n        give(\"diamond\", 5);\n        let n = state_get_int(\"kills\", 0) + 1;\n        state_set_int(\"kills\", n);\n        say(&format!(\"BOSS DOWN ({})\", n));\n    }\n}";
    let mut h = hook_host(src);
    let mut m = MockHost::default();
    let died = |kind: &str| alloc::vec![text(kind), Value::Float(0.0), Value::Float(64.0), Value::Float(0.0)];
    call(&mut h, &mut m, "on_entity_death", &died("wolf"));
    call(&mut h, &mut m, "on_entity_death", &died("wither_boss"));
    call(&mut h, &mut m, "on_entity_death", &died("wither_boss"));
    assert_eq!(m.calls, ["give diamond 5", "say BOSS DOWN (1)", "give diamond 5", "say BOSS DOWN (2)"]);
}

#[test]
fn explosion_example() {
    let src = "fn on_explode(x: f64, y: f64, z: f64, power: f64, source: String) -> bool {\n    if source == \"tnt\" && power > 3.0 { say(\"big one\"); return false; }\n    true\n}";
    let mut h = hook_host(src);
    let mut m = MockHost::default();
    let args = |p: f64| alloc::vec![Value::Float(0.0), Value::Float(0.0), Value::Float(0.0), Value::Float(p), text("tnt")];
    assert_eq!(call(&mut h, &mut m, "on_explode", &args(2.0)), [Value::Bool(true)]);
    assert_eq!(call(&mut h, &mut m, "on_explode", &args(5.0)), [Value::Bool(false)]);
    assert_eq!(m.calls, ["say big one"]);
}

/// The example mod `scripting-demo` (VoxelRust-Mods, scripts/demo.rs), as it is there.
const DEMO: &str = "
use std::fmt;

fn announcement(episode: &str) -> Option<&'static str> {
    match episode {
        \"earthquake\" => Some(\"EARTHQUAKE\"),
        \"tsunami\" => Some(\"TSUNAMI - GET TO HIGH GROUND\"),
        \"tornado\" => Some(\"TORNADO\"),
        \"hailstorm\" => Some(\"HAILSTORM - FIND COVER\"),
        \"blizzard\" => Some(\"BLIZZARD - KEEP WARM\"),
        _ => None,
    }
}

fn on_event(name: String, what: String) {
    if name != \"episode\" {
        return;
    }
    if let Some(text) = announcement(&what) {
        run_command(&format!(\"/title {}\", text));
        let n = state_get_int(\"announced\", 0) + 1;
        state_set_int(\"announced\", n);
        log(&format!(\"announced {} ({} so far in this world)\", what, n));
    }
}
";

#[test]
fn the_demo_mod_announces_episodes() {
    let mut h = hook_host(DEMO);
    let mut m = MockHost::default();
    call(&mut h, &mut m, "on_event", &[text("episode"), text("earthquake")]);
    call(&mut h, &mut m, "on_event", &[text("episode"), text("tsunami")]);
    call(&mut h, &mut m, "on_event", &[text("episode"), text("someone_elses")]);
    call(&mut h, &mut m, "on_event", &[text("night"), text("")]);
    assert_eq!(m.commands, ["/title EARTHQUAKE", "/title TSUNAMI - GET TO HIGH GROUND"]);
    assert_eq!(h.mods[0].state.get("announced"), Some("2"));
    assert!(m.calls[0].starts_with("log announced earthquake (1 so far"), "{:?}", m.calls);
    assert!(h.problems.is_empty(), "{:?}", h.problems);
}

#[test]
fn player_hurt_can_change_the_amount() {
    let mut h = hook_host("fn on_player_hurt(amount: i64, cause: String) -> i64 { if cause == \"fall\" { amount / 2 } else { amount } }");
    let mut m = MockHost::default();
    assert_eq!(call(&mut h, &mut m, "on_player_hurt", &[int(8), text("fall")]), [int(4)]);
    assert_eq!(call(&mut h, &mut m, "on_player_hurt", &[int(8), text("drown")]), [int(8)]);
}

#[test]
fn files_outside_scripts_and_bad_files_are_handled() {
    let mut h = ScriptHost::new();
    let all = alloc::vec![
        (String::from("pack.txt"), b"# nothing".to_vec()),
        (String::from("scripts/good.rs"), b"fn on_tick() {}".to_vec()),
        (String::from("scripts/bad.rs"), b"fn on_tick( {".to_vec()),
        (String::from("scripts/notes.txt"), b"hello".to_vec()),
        (String::from("scripts/../escape.rs"), b"fn on_tick() {}".to_vec()),
        (String::from("scripts/binary.rs"), alloc::vec![0xff, 0xfe, 0x00]),
    ];
    let problems = h.add_mod("m", &all);
    assert_eq!(problems.len(), 2, "{:?}", problems);
    assert!(problems.iter().any(|p| p.starts_with("m/scripts/bad.rs:1:")));
    assert_eq!(h.mods[0].scripts.len(), 1);
    // A mod with no scripts adds nothing; loading again replaces.
    h.add_mod("m", &[(String::from("pack.txt"), Vec::new())]);
    assert!(h.mods.is_empty());
}

#[test]
fn script_paths() {
    assert!(crate::script::is_script_path("scripts/a.rs"));
    assert!(crate::script::is_script_path("scripts\\a.rs"));
    assert!(crate::script::is_script_path("scripts/sub/a.rs"));
    assert!(!crate::script::is_script_path("a.rs"));
    assert!(!crate::script::is_script_path("scripts/a.txt"));
    assert!(!crate::script::is_script_path("scripts/../a.rs"));
}

#[test]
fn the_global_host_loads_and_unloads() {
    // The only test that uses the global host.
    ScriptHost::unload_all();
    assert!(!ScriptHost::is_live());
    let problems = ScriptHost::load_mod("demo", &files("a", "fn on_tick() {}"));
    assert!(problems.is_empty());
    assert!(ScriptHost::is_live());
    let problems = ScriptHost::load_mod("broken", &files("b", "fn ("));
    assert_eq!(problems.len(), 1);
    ScriptHost::unload_all();
    assert!(!ScriptHost::is_live());
}

// ---- controls, HUD and screens

use crate::client::modui::hudcmd::Cmd;
use crate::client::modui::limits::{MAX_HUD_CMDS_PER_MOD, MAX_TEXT};

#[test]
fn hud_drawing_calls_make_commands_up_to_the_cap_and_then_say_false() {
    let mut h = MockHost::default();
    let v = try_run_with("fn t() -> i64 { let mut n = 0; for _ in 0..1000 { if hud_rect(\"top-left\", 0.0, 0.0, 4.0, 4.0, 0xFF0000) { n += 1; } } n }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, int(MAX_HUD_CMDS_PER_MOD as i64));
    assert_eq!(h.ui.hud.buf.len(), MAX_HUD_CMDS_PER_MOD);
    assert_eq!(h.ui.hud.buf.refused as usize, 1000 - MAX_HUD_CMDS_PER_MOD);
}

#[test]
fn every_kind_of_hud_command_can_be_given_from_a_script() {
    let mut h = MockHost::default();
    let src = "hud_text(\"top\", 1.0, 2.0, 1.0, \"hi\", 0xFFFFFF); hud_rect(\"centre\", 0.0, 0.0, 5.0, 5.0, 0x80112233); hud_outline(\"bottom-right\", -2.0, -2.0, 9.0, 9.0, 1.0, 0xFF0000); hud_item(\"left\", 0.0, 0.0, 16.0, \"diamond\"); hud_block(\"right\", 0.0, 0.0, 16.0, \"stone\"); hud_bar(\"bottom\", 0.0, -30.0, 40.0, 4.0, 0.25, 0x00FF00);";
    host_run(src, &mut h).unwrap();
    let c = &h.ui.hud.buf.cmds;
    assert_eq!(c.len(), 6);
    assert!(matches!(&c[0], Cmd::Text { text, .. } if text == "HI"));
    assert!(matches!(&c[1], Cmd::Rect { col, .. } if *col == [0x11, 0x22, 0x33, 0x80]));
    assert!(matches!(&c[2], Cmd::Outline { .. }));
    assert!(matches!(&c[3], Cmd::Item { id, .. } if id == "diamond"));
    assert!(matches!(&c[4], Cmd::Block { id, .. } if id == "stone"));
    assert!(matches!(&c[5], Cmd::Bar { frac, .. } if *frac == 0.25));
}

#[test]
fn hud_text_is_cut_and_a_wrong_anchor_is_an_error() {
    let mut h = MockHost::default();
    host_run("hud_text(\"top-left\", 0.0, 0.0, 1.0, &\"x\".repeat(5000), 0xFFFFFF);", &mut h).unwrap();
    match &h.ui.hud.buf.cmds[0] {
        Cmd::Text { text, .. } => assert_eq!(text.len(), MAX_TEXT),
        c => panic!("{:?}", c),
    }
    let e = host_run("hud_text(\"nowhere\", 0.0, 0.0, 1.0, \"x\", 0);", &mut h).unwrap_err();
    assert!(e.contains("nowhere") && e.contains("anchor"), "{}", e);
    let e = host_run("hud_set(\"nowhere\", \"a\", \"x\", 0);", &mut h).unwrap_err();
    assert!(e.contains("slot"), "{}", e);
}

#[test]
fn slots_toasts_and_the_action_bar_from_a_script() {
    let mut h = MockHost::default();
    let v = try_run_with("fn t() -> bool { hud_set(\"above_hotbar\", \"c\", \"north\", 0xFFFFFF) && hud_set_icon(\"c\", \"compass\") && toast(\"hello\", 2.0) && action_bar(\"saved\", 2.0) && !hud_set_icon(\"nope\", \"x\") }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(true));
    assert_eq!(h.ui.hud.elements.len(), 1);
    assert_eq!((h.ui.hud.elements[0].text.as_str(), h.ui.hud.elements[0].icon.as_str(), h.ui.hud.elements[0].mod_name.as_str()), ("NORTH", "compass", "demo"));
    assert_eq!(h.ui.hud.toasts.len(), 1);
    assert!(h.ui.hud.action.is_some());
    let v = try_run_with("fn t() -> bool { hud_remove(\"c\") && !hud_remove(\"c\") }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(true));
    assert!(h.ui.hud.elements.is_empty());
}

#[test]
fn what_the_hud_may_read() {
    let mut h = MockHost { health: 14, hunger: 17, air: 6.5, biome: String::from("forest"), selected: 3, heading: 90.0, pos: (1.0, 2.0, 3.0), time: 0.25, ..MockHost::default() };
    h.ui.fps = 60.0;
    let v = try_run_with("fn t() -> String { format!(\"{} {} {} {} {} {} {} {}\", player_health(), hunger(), air(), biome(), selected_slot(), heading(), fps(), time()) }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, text("14 17 6.5 forest 3 90 60 0.25"));
}

#[test]
fn key_names_and_held_keys() {
    let mut h = MockHost::default();
    h.ui.set_held("demo", "dash", true);
    let v = try_run_with("fn t() -> String { format!(\"{} {} {} {} {}\", key_scancode(\"g\"), key_scancode(\"F5\"), key_scancode(\"nonsense\"), key_held(\"dash\"), key_held(\"other\")) }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, text("10 62 0 true false"));
}

#[test]
fn switches_are_read_by_the_mod_that_owns_them() {
    let mut h = MockHost::default();
    h.switches.push((String::from("demo.fast"), true));
    let v = try_run_with("fn t() -> bool { setting_on(\"fast\") && !setting_on(\"slow\") }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn declaring_keys_is_only_allowed_while_registering() {
    let mut h = MockHost::default();
    let e = host_run("bind_key(\"a\", \"A\", \"\", 5);", &mut h).unwrap_err();
    assert!(e.contains("on_register"), "{}", e);
    let e = host_run("add_switch(\"a\", \"A\", true);", &mut h).unwrap_err();
    assert!(e.contains("on_register"), "{}", e);
}

#[test]
fn on_register_declares_bindings_and_switches() {
    let mut host = ScriptHost::new();
    let src = "fn on_register() {\n    bind_key(\"compass\", \"toggle compass\", \"hud\", \"g\");\n    bind_key(\"stats\", \"Stats\", \"\", 23);\n    add_switch(\"fast\", \"Fast mode\", true);\n}\n";
    let problems = host.add_mod("demo mod", &files("main", src));
    assert!(problems.is_empty(), "{:?}", problems);
    let d = &host.decls;
    assert_eq!(d.binds.len(), 2);
    assert_eq!((d.binds[0].mod_name.as_str(), d.binds[0].id.as_str(), d.binds[0].default_key, d.binds[0].label.as_str(), d.binds[0].group.as_str()), ("demo_mod", "compass", 10, "TOGGLE COMPASS", "HUD"));
    assert_eq!(d.binds[1].default_key, 23);
    assert_eq!((d.switches.len(), d.switches[0].default), (1, true));
    // Loading the mod again replaces what it declared; a mod without it declares nothing.
    let src2 = "fn on_register() { bind_key(\"only\", \"Only\", \"\", 5); }";
    host.add_mod("demo mod", &files("main", src2));
    assert_eq!((host.decls.binds.len(), host.decls.binds[0].id.as_str(), host.decls.switches.len()), (1, "only", 0));
    host.add_mod("demo mod", &files("main", "fn on_tick(dt: f32) {}"));
    assert!(host.decls.binds.is_empty());
}

#[test]
fn a_script_that_registers_badly_is_reported_and_switched_off() {
    let mut host = ScriptHost::new();
    let problems = host.add_mod("demo", &files("main", "fn on_register() { bind_key(\"bad id\", \"x\", \"\", 5); }\nfn on_tick(dt: f32) { say(\"tick\"); }"));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("key id"), "{:?}", problems);
    let mut mock = MockHost::default();
    call(&mut host, &mut mock, "on_tick", &[Value::Float(0.1)]);
    assert!(mock.calls.is_empty(), "the script that failed is off");
    // A bad key name is an error with the list of names.
    let problems = host.add_mod("demo", &files("main", "fn on_register() { bind_key(\"a\", \"A\", \"\", \"banana\"); }"));
    assert!(problems[0].contains("not a key name"), "{:?}", problems);
    // Other mods are unaffected.
    host.add_mod("other", &files("main", "fn on_tick(dt: f32) { say(\"ok\"); }"));
    call(&mut host, &mut mock, "on_tick", &[Value::Float(0.1)]);
    assert_eq!(mock.calls, ["say ok"]);
}

#[test]
fn a_key_reaches_only_the_mod_it_belongs_to() {
    let mut host = ScriptHost::new();
    host.add_mod("a", &files("main", "fn on_key_down(id: String) { say(&format!(\"a down {}\", id)); }\nfn on_key_up(id: String) { say(&format!(\"a up {}\", id)); }"));
    host.add_mod("b", &files("main", "fn on_key_down(id: String) { say(&format!(\"b down {}\", id)); }"));
    let mut mock = MockHost::default();
    let only_b = |m: &str| m == "b";
    host.call_hook_in(&mut mock, "world", Some(&only_b as &dyn Fn(&str) -> bool), "on_key_down", &[text("jump")], &mut |_| false);
    assert_eq!(mock.calls, ["say b down jump"]);
    mock.calls.clear();
    let only_a = |m: &str| m == "a";
    host.call_hook_in(&mut mock, "world", Some(&only_a as &dyn Fn(&str) -> bool), "on_key_up", &[text("jump")], &mut |_| false);
    assert_eq!(mock.calls, ["say a up jump"]);
    // Asking everyone still works.
    mock.calls.clear();
    call(&mut host, &mut mock, "on_key_down", &[text("x")]);
    assert_eq!(mock.calls, ["say a down x", "say b down x"]);
}

#[test]
fn a_script_builds_a_screen_and_hears_the_player() {
    let src = "fn open() {\n    ui_open(\"stats\", \"Stats\");\n    ui_text(\"hello\");\n    ui_button(\"go\", \"Go\");\n    ui_toggle(\"t\", \"Toggle\", false);\n    ui_stepper(\"n\", \"Count\", 3, 0, 9);\n    ui_input(\"who\", \"Name\", \"abc\");\n    ui_list(\"l\", \"one\\ntwo\\nthree\", 1);\n}\nfn on_ui_click(id: String) { say(&format!(\"click {}\", id)); }\nfn on_ui_change(id: String, value: i64) { say(&format!(\"change {} {}\", id, value)); }\nfn on_ui_close(name: String) { say(&format!(\"close {}\", name)); }\nfn peek() -> String { format!(\"{} {} {} {} {}\", ui_get_value(\"t\"), ui_get_value(\"n\"), ui_get_text(\"who\"), ui_get_value(\"l\"), ui_get_text(\"l\")) }\nfn poke() -> bool { ui_set_value(\"t\", 1) && ui_set_value(\"n\", 99) && ui_set_text(\"who\", \"zed\") && ui_set_line(0, \"changed\") && ui_close() }\n";
    let mut host = ScriptHost::new();
    assert!(host.add_mod("demo", &files("main", src)).is_empty());
    let mut mock = MockHost::default();
    call(&mut host, &mut mock, "open", &[]);
    {
        let o = mock.ui.screens.open.as_ref().expect("the screen is open");
        assert_eq!((o.name.as_str(), o.title.as_str(), o.widgets.len()), ("stats", "STATS", 6));
    }
    assert!(mock.ui.screens.opened_now);
    let seen = call(&mut host, &mut mock, "peek", &[]);
    assert_eq!(seen, [text("0 3 abc 1 TWO")]);
    // What the player does comes back through the hooks.
    call(&mut host, &mut mock, "on_ui_click", &[text("go")]);
    call(&mut host, &mut mock, "on_ui_change", &[text("t"), Value::Bool(true)]);
    call(&mut host, &mut mock, "on_ui_close", &[text("stats")]);
    assert_eq!(mock.calls, ["say click go", "say change t true", "say close stats"]);
    // The script changes it and closes it; closing leaves an event for the game to deliver.
    let r = call(&mut host, &mut mock, "poke", &[]);
    assert_eq!(r, [Value::Bool(true)]);
    assert!(mock.ui.screens.open.is_none());
    let ev = mock.ui.screens.take_events();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].screen, "stats");
}

#[test]
fn screen_calls_are_bounded() {
    let mut h = MockHost::default();
    let v = try_run_with("fn t() -> i64 { ui_open(\"s\", \"S\"); let mut n = 0; for i in 0..200 { if ui_text(&format!(\"line {}\", i)) { n += 1; } } n }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, int(crate::client::modui::limits::MAX_WIDGETS as i64));
    // A used id, a bad id and a widget with no screen are refused (false), not errors.
    let v = try_run_with("fn t() -> bool { ui_close(); !ui_button(\"a\", \"A\") }", &mut h, Limits::DEFAULT).unwrap();
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn a_runaway_hud_script_is_stopped_by_the_step_budget_not_the_frame() {
    let mut host = ScriptHost::new();
    host.add_mod("demo", &files("main", "fn on_hud(dt: f32) { loop { hud_rect(\"top\", 0.0, 0.0, 1.0, 1.0, 0); } }"));
    let mut mock = MockHost::default();
    call(&mut host, &mut mock, "on_hud", &[Value::Float(0.016)]);
    assert!(mock.ui.hud.buf.len() <= MAX_HUD_CMDS_PER_MOD);
    assert!(!host.mods[0].scripts[0].disabled.is_none(), "it ran out of steps and was switched off");
    let before = mock.ui.hud.buf.len();
    call(&mut host, &mut mock, "on_hud", &[Value::Float(0.016)]);
    assert_eq!(mock.ui.hud.buf.len(), before, "a script that was switched off draws nothing more");
}
