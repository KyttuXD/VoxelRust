//! The built-in library, part one: how a method call or a library function
//! finds what to run, and the methods of numbers, characters, `Option`,
//! `Result` and `Ordering`. Strings are in `builtins_text.rs`, vectors and
//! iterators in `builtins_list.rs`.
//!
//! Iterators are eager: `v.iter().map(f)` makes the whole Vec at once. That
//! keeps the interpreter small and every step countable.

use crate::script::ast::{Name, Type};
use crate::script::host::{call_host_fn, int, num, text, want};
use crate::script::interp::{Flow, Interp, Res};
use crate::script::numeric as nm;
use crate::script::value::*;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// What a method looked at: `Ok(None)` is "no such method here".
pub(crate) type Found = Result<Option<Value>, String>;

pub(crate) fn v(x: Value) -> Found {
    Ok(Some(x))
}

/// Methods that change the thing they are called on.
pub(crate) fn is_mutating(val: &Value, name: &str) -> bool {
    match val {
        Value::Vec(_) => matches!(
            name,
            "push" | "pop" | "insert" | "remove" | "clear" | "sort" | "sort_unstable" | "sort_by" | "sort_by_key" | "sort_unstable_by" | "sort_unstable_by_key" | "reverse" | "truncate" | "swap" | "extend" | "append" | "dedup" | "retain" | "drain" | "swap_remove" | "resize" | "fill" | "extend_from_slice" | "rotate_left" | "rotate_right"
        ),
        Value::Str(_) => matches!(name, "push" | "push_str" | "clear" | "insert" | "insert_str" | "pop" | "remove" | "truncate" | "retain" | "make_ascii_uppercase" | "make_ascii_lowercase"),
        Value::Enum(e) if &*e.ty == "Option" => matches!(name, "take" | "replace" | "insert" | "get_or_insert" | "get_or_insert_with"),
        _ => false,
    }
}

const INT_TYPES: [&str; 12] = ["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize"];
const LIB_TYPES: [&str; 10] = ["String", "str", "Vec", "Option", "Result", "char", "bool", "f32", "f64", "ToString"];

fn is_lib_type(ty: &str) -> bool {
    INT_TYPES.contains(&ty) || LIB_TYPES.contains(&ty)
}

/// A value of the kind `val` is, with nothing in it (for `mem::take`).
pub(crate) fn default_of(val: &Value) -> Value {
    default_at(val, 0)
}

fn default_at(val: &Value, depth: usize) -> Value {
    match val {
        Value::Int(_) => Value::Int(0),
        Value::Float(_) => Value::Float(0.0),
        Value::Bool(_) => Value::Bool(false),
        Value::Char(_) => Value::Char('\0'),
        Value::Str(_) => Value::str(""),
        Value::Vec(_) => Value::vec(Vec::new()),
        Value::Tuple(t) if depth < 16 => Value::tuple(t.iter().map(|x| default_at(x, depth + 1)).collect()),
        Value::Enum(e) if &*e.ty == "Option" => Value::none(),
        _ => Value::Unit,
    }
}

impl<'a> Interp<'a> {
    pub(crate) fn deref_all(&self, args: Vec<Value>) -> Result<Vec<Value>, Flow> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.deref(a)?);
        }
        Ok(out)
    }

    /// An error message becomes an error at the current line.
    pub(crate) fn lift(&mut self, r: Result<Value, String>) -> Res {
        match r {
            Ok(v) => {
                let c = v.cells();
                self.charge(c)?;
                Ok(v)
            }
            Err(m) => self.fail(m),
        }
    }

    pub(crate) fn found(&mut self, f: Found) -> Option<Res> {
        match f {
            Ok(Some(v)) => Some(self.lift(Ok(v))),
            Ok(None) => None,
            Err(m) => Some(self.fail(m)),
        }
    }

    pub(crate) fn truthy(&self, val: Value) -> Result<bool, Flow> {
        match self.deref(val)? {
            Value::Bool(b) => Ok(b),
            other => self.fail(format!("a closure here should answer true or false, not {}", other.type_name())),
        }
    }

    pub(crate) fn call1(&mut self, f: &Value, a: Value) -> Res {
        self.step()?;
        self.call_value(f, alloc::vec![a])
    }

    pub(crate) fn call2(&mut self, f: &Value, a: Value, b: Value) -> Res {
        self.step()?;
        self.call_value(f, alloc::vec![a, b])
    }

    // ---- constants

    pub(crate) fn builtin_const(&self, ty: &str, name: &str) -> Option<Value> {
        let int = |x: i64| Some(Value::Int(x));
        match (ty, name) {
            ("i8", "MAX") => int(i8::MAX as i64),
            ("i8", "MIN") => int(i8::MIN as i64),
            ("i16", "MAX") => int(i16::MAX as i64),
            ("i16", "MIN") => int(i16::MIN as i64),
            ("i32", "MAX") => int(i32::MAX as i64),
            ("i32", "MIN") => int(i32::MIN as i64),
            ("u8", "MAX") => int(u8::MAX as i64),
            ("u16", "MAX") => int(u16::MAX as i64),
            ("u32", "MAX") => int(u32::MAX as i64),
            // The 64-bit types stop at the biggest i64.
            ("i64" | "isize" | "i128" | "u64" | "usize" | "u128", "MAX") => int(i64::MAX),
            ("i64" | "isize" | "i128", "MIN") => int(i64::MIN),
            (t, "MIN") if t.starts_with('u') && INT_TYPES.contains(&t) => int(0),
            ("f64" | "f32", "MAX") => Some(Value::Float(if ty == "f32" { f32::MAX as f64 } else { f64::MAX })),
            ("f64" | "f32", "MIN") => Some(Value::Float(if ty == "f32" { f32::MIN as f64 } else { f64::MIN })),
            ("f64" | "f32", "EPSILON") => Some(Value::Float(if ty == "f32" { f32::EPSILON as f64 } else { f64::EPSILON })),
            ("f64" | "f32", "INFINITY") => Some(Value::Float(f64::INFINITY)),
            ("f64" | "f32", "NEG_INFINITY") => Some(Value::Float(f64::NEG_INFINITY)),
            ("f64" | "f32", "NAN") => Some(Value::Float(f64::NAN)),
            ("consts", "PI") => Some(Value::Float(core::f64::consts::PI)),
            ("consts", "TAU") => Some(Value::Float(core::f64::consts::TAU)),
            ("consts", "E") => Some(Value::Float(core::f64::consts::E)),
            ("consts", "SQRT_2") => Some(Value::Float(core::f64::consts::SQRT_2)),
            ("consts", "LN_2") => Some(Value::Float(core::f64::consts::LN_2)),
            ("consts", "LN_10") => Some(Value::Float(core::f64::consts::LN_10)),
            ("consts", "FRAC_PI_2") => Some(Value::Float(core::f64::consts::FRAC_PI_2)),
            ("consts", "FRAC_PI_3") => Some(Value::Float(core::f64::consts::FRAC_PI_3)),
            ("consts", "FRAC_PI_4") => Some(Value::Float(core::f64::consts::FRAC_PI_4)),
            ("consts", "FRAC_PI_6") => Some(Value::Float(core::f64::consts::FRAC_PI_6)),
            ("Ordering", "Less") => Some(Value::ordering(Ordering::Less)),
            ("Ordering", "Equal") => Some(Value::ordering(Ordering::Equal)),
            ("Ordering", "Greater") => Some(Value::ordering(Ordering::Greater)),
            ("Option", "None") => Some(Value::none()),
            // `String::from` and friends used as values: `.map(String::from)`.
            (t, n) if is_lib_type(t) && n.chars().next().map_or(false, |c| c.is_lowercase()) => Some(Value::Func(Name::from(format!("{}::{}", t, n)))),
            _ => None,
        }
    }

    // ---- functions

    /// A function that is not the script's own: the host's, or the few `std` ones.
    pub(crate) fn call_builtin_fn(&mut self, name: &str, args: Vec<Value>) -> Option<Res> {
        match (name, args.len()) {
            ("swap", 2) => return Some(self.mem_swap(&args)),
            ("replace", 2) => return Some(self.mem_replace(&args)),
            ("take", 1) => return Some(self.mem_take(&args)),
            _ => {}
        }
        let args = match self.deref_all(args) {
            Ok(a) => a,
            Err(f) => return Some(Err(f)),
        };
        if let Some(r) = call_host_fn(self, name, &args) {
            return Some(r);
        }
        match name {
            "min" | "max" if args.len() == 2 => Some(self.generic_method(args[0].clone(), name, &args[1..])),
            "drop" => Some(Ok(Value::Unit)),
            _ => None,
        }
    }

    fn mem_swap(&mut self, args: &[Value]) -> Res {
        match (&args[0], &args[1]) {
            (Value::Ref(a), Value::Ref(b)) => {
                let (va, vb) = (self.read_place(a)?, self.read_place(b)?);
                self.write_place(a, vb)?;
                self.write_place(b, va)?;
                Ok(Value::Unit)
            }
            _ => self.fail("`swap` needs two `&mut` arguments"),
        }
    }

    fn mem_replace(&mut self, args: &[Value]) -> Res {
        match &args[0] {
            Value::Ref(a) => {
                let old = self.read_place(a)?;
                self.write_place(a, args[1].clone())?;
                Ok(old)
            }
            _ => self.fail("`replace` needs a `&mut` first argument"),
        }
    }

    fn mem_take(&mut self, args: &[Value]) -> Res {
        match &args[0] {
            Value::Ref(a) => {
                let old = self.read_place(a)?;
                self.write_place(a, default_of(&old))?;
                Ok(old)
            }
            _ => self.fail("`take` needs a `&mut` argument"),
        }
    }

    /// `Type::name(args)` for a library type: `String::new()`, `Vec::new()`,
    /// `char::from_u32(n)`, `f64::from(x)`, `std::cmp::max(a, b)`...
    pub(crate) fn call_assoc_builtin(&mut self, ty: &str, name: &str, args: Vec<Value>) -> Option<Res> {
        if ty == "mem" {
            return self.call_builtin_fn(name, args);
        }
        let args = match self.deref_all(args) {
            Ok(a) => a,
            Err(f) => return Some(Err(f)),
        };
        let r: Found = match (ty, name) {
            ("String", "new") | ("Vec", "new") if args.is_empty() => v(if ty == "String" { Value::str("") } else { Value::vec(Vec::new()) }),
            ("String", "with_capacity") | ("Vec", "with_capacity") => v(if ty == "String" { Value::str("") } else { Value::vec(Vec::new()) }),
            ("String", "from") | ("String", "from_str") if args.len() == 1 => {
                let mut s = String::new();
                if args[0].display(&mut s) {
                    v(Value::string(s))
                } else {
                    Err(format!("a {} cannot be turned into a String", args[0].type_name()))
                }
            }
            ("Vec", "from") if args.len() == 1 => match &args[0] {
                Value::Vec(x) => v(Value::Vec(x.clone())),
                Value::Str(s) => v(Value::vec(s.bytes().map(|b| Value::Int(b as i64)).collect())),
                other => Err(format!("a Vec cannot be made from a {}", other.type_name())),
            },
            ("char", "from_u32") if args.len() == 1 => match int(name, &args, 0) {
                Ok(n) => v(Value::option(if (0..=0x10FFFF).contains(&n) { char::from_u32(n as u32).map(Value::Char) } else { None })),
                Err(e) => Err(e),
            },
            ("char", "from_digit") if args.len() == 2 => match (int(name, &args, 0), int(name, &args, 1)) {
                (Ok(n), Ok(radix)) if (2..=36).contains(&radix) && n >= 0 => v(Value::option(char::from_digit(n as u32, radix as u32).map(Value::Char))),
                (Ok(_), Ok(_)) => v(Value::none()),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            ("char", "from") if args.len() == 1 => crate::script::ops::cast(&args[0], "char").map(Some),
            (t, "from") if INT_TYPES.contains(&t) && args.len() == 1 => crate::script::ops::cast(&args[0], t).map(Some),
            (t, "from") if (t == "f64" || t == "f32") && args.len() == 1 => crate::script::ops::cast(&args[0], t).map(Some),
            (t, "from_str_radix") if INT_TYPES.contains(&t) && args.len() == 2 => match (text(name, &args, 0), int(name, &args, 1)) {
                (Ok(s), Ok(radix)) if (2..=36).contains(&radix) => v(match i64::from_str_radix(s, radix as u32) {
                    Ok(n) => Value::ok(Value::Int(n)),
                    Err(e) => Value::err(Value::string(format!("{}", e))),
                }),
                (Ok(_), Ok(_)) => Err(String::from("the radix must be 2 to 36")),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            ("cmp", "min") | ("cmp", "max") if args.len() == 2 => return Some(self.generic_method(args[0].clone(), name, &args[1..])),
            ("Ordering", _) => Ok(None),
            _ => Ok(None),
        };
        if let Some(r) = self.found(r) {
            return Some(r);
        }
        // `f64::sqrt(x)`, `i32::abs(x)`, `str::len(s)`: the method, with the receiver first.
        if is_lib_type(ty) && !args.is_empty() {
            let mut args = args;
            let first = args.remove(0);
            return Some(self.call_builtin_method(first, None, name, args, None));
        }
        None
    }

    // ---- methods

    pub(crate) fn call_builtin_method(&mut self, recv: Value, place: Option<&Place>, name: &str, args: Vec<Value>, tf: Option<&Type>) -> Res {
        let args = self.deref_all(args)?;
        self.step()?;
        let r: Option<Res> = match &recv {
            Value::Int(i) => {
                let f = int_methods(*i, name, &args);
                self.found(f)
            }
            Value::Float(x) => {
                let f = float_methods(*x, name, &args);
                self.found(f)
            }
            Value::Char(c) => {
                let f = char_methods(*c, name, &args);
                self.found(f)
            }
            Value::Bool(b) => self.bool_methods(*b, name, &args),
            Value::Str(s) => {
                let s = s.clone();
                self.str_methods(&s, name, &args, tf)
            }
            Value::Vec(items) => {
                let items = items.clone();
                self.vec_methods(&items, place, name, &args, tf)
            }
            Value::Range(r) => {
                let r = r.clone();
                self.range_methods(&r, name, &args, tf)
            }
            Value::Enum(e) if &*e.ty == "Option" || &*e.ty == "Result" => self.option_methods(&recv, name, &args),
            Value::Enum(e) if &*e.ty == "Ordering" => self.ordering_methods(&recv, name, &args),
            Value::Tuple(_) | Value::Struct(_) | Value::Enum(_) | Value::Unit | Value::Closure(_) | Value::Func(_) | Value::Ref(_) => None,
        };
        match r {
            Some(res) => res,
            None => self.generic_method(recv, name, &args),
        }
    }

    pub(crate) fn call_builtin_method_mut(&mut self, target: &mut Value, name: &str, args: Vec<Value>, tf: Option<&Type>) -> Res {
        // `append(&mut other)` needs the reference itself; the rest want values.
        let args = if name == "append" { args } else { self.deref_all(args)? };
        self.step()?;
        let tag = match &*target {
            Value::Vec(_) => 0u8,
            Value::Str(_) => 1,
            Value::Enum(_) => 2,
            _ => 3,
        };
        match tag {
            0 => self.vec_mutate(target, name, args, tf),
            1 => self.str_mutate(target, name, args),
            2 => self.option_mutate(target, name, args),
            _ => self.fail(format!("no method `{}` for a {}", name, target.type_name())),
        }
    }

    fn bool_methods(&mut self, b: bool, name: &str, args: &[Value]) -> Option<Res> {
        match name {
            "then" if args.len() == 1 => Some(if b { self.call_value(&args[0], Vec::new()).map(Value::some) } else { Ok(Value::none()) }),
            "then_some" if args.len() == 1 => Some(Ok(if b { Value::some(args[0].clone()) } else { Value::none() })),
            _ => None,
        }
    }

    fn range_methods(&mut self, r: &RangeVal, name: &str, args: &[Value], tf: Option<&Type>) -> Option<Res> {
        match name {
            "contains" if args.len() == 1 => {
                let x = match &args[0] {
                    Value::Int(i) => *i,
                    Value::Char(c) => *c as i64,
                    Value::Float(_) => return Some(self.fail("a range of whole numbers cannot contain a float")),
                    other => return Some(self.fail(format!("a range cannot contain a {}", other.type_name()))),
                };
                let lo_ok = r.lo.map_or(true, |lo| x >= lo);
                let hi_ok = r.hi.map_or(true, |hi| if r.incl { x <= hi } else { x < hi });
                Some(Ok(Value::Bool(lo_ok && hi_ok)))
            }
            "is_empty" => Some(Ok(Value::Bool(r.lo.is_some() && r.hi.is_some() && range_len(r) == 0))),
            "start" | "end" => None,
            _ => {
                // Anything else: walk it out into a Vec first.
                let (Some(lo), Some(hi)) = (r.lo, r.hi) else { return Some(self.fail("an open range cannot be used like a list")) };
                let hi = if r.incl { hi.saturating_add(1) } else { hi };
                let n = if hi > lo { (hi - lo) as u64 } else { 0 };
                if n > self.limits.max_vec as u64 {
                    return Some(self.fail(format!("a Vec may not hold more than {} elements", self.limits.max_vec)));
                }
                if let Err(f) = self.charge(n as usize + 1) {
                    return Some(Err(f));
                }
                let items: Vec<Value> = (lo..hi.max(lo)).map(|i| if r.chars { char::from_u32(i as u32).map_or(Value::Int(i), Value::Char) } else { Value::Int(i) }).collect();
                let items = Rc::new(List(items));
                self.vec_methods(&items, None, name, args, tf)
            }
        }
    }

    fn ordering_methods(&mut self, recv: &Value, name: &str, args: &[Value]) -> Option<Res> {
        let o = recv.as_ordering()?;
        let r = match name {
            "reverse" => Value::ordering(o.reverse()),
            "then" if args.len() == 1 => match args[0].as_ordering() {
                Some(next) => Value::ordering(o.then(next)),
                None => return Some(self.fail("`then` needs an Ordering")),
            },
            "then_with" if args.len() == 1 => {
                if o != Ordering::Equal {
                    return Some(Ok(recv.clone()));
                }
                return Some(self.call_value(&args[0], Vec::new()));
            }
            "is_lt" => Value::Bool(o == Ordering::Less),
            "is_le" => Value::Bool(o != Ordering::Greater),
            "is_gt" => Value::Bool(o == Ordering::Greater),
            "is_ge" => Value::Bool(o != Ordering::Less),
            "is_eq" => Value::Bool(o == Ordering::Equal),
            "is_ne" => Value::Bool(o != Ordering::Equal),
            _ => return None,
        };
        Some(Ok(r))
    }

    /// `Option` and `Result`.
    fn option_methods(&mut self, recv: &Value, name: &str, args: &[Value]) -> Option<Res> {
        let Value::Enum(e) = recv else { return None };
        let is_opt = &*e.ty == "Option";
        let good = if is_opt { &*e.variant == "Some" } else { &*e.variant == "Ok" };
        let payload = e.fields.first().cloned();
        let first = args.first();
        let shown = || payload.as_ref().map(|p| p.debug_string()).unwrap_or_default();
        match name {
            "is_some" | "is_ok" => Some(Ok(Value::Bool(good))),
            "is_none" | "is_err" => Some(Ok(Value::Bool(!good))),
            "unwrap" => Some(if good {
                Ok(payload.unwrap_or(Value::Unit))
            } else if is_opt {
                self.fail("called `Option::unwrap()` on a `None` value")
            } else {
                self.fail(format!("called `Result::unwrap()` on an `Err` value: {}", shown()))
            }),
            "expect" => {
                let msg = match first {
                    Some(Value::Str(s)) => String::from(s.as_str()),
                    _ => String::from("expect failed"),
                };
                Some(if good {
                    Ok(payload.unwrap_or(Value::Unit))
                } else if is_opt {
                    self.fail(msg)
                } else {
                    self.fail(format!("{}: {}", msg, shown()))
                })
            }
            "unwrap_err" | "expect_err" => Some(if !good && !is_opt { Ok(payload.unwrap_or(Value::Unit)) } else { self.fail("called `unwrap_err` on an `Ok` value") }),
            "unwrap_or" if first.is_some() => Some(Ok(if good { payload.unwrap_or(Value::Unit) } else { first.cloned().unwrap_or(Value::Unit) })),
            "unwrap_or_default" => Some(if good { Ok(payload.unwrap_or(Value::Unit)) } else { self.fail("`unwrap_or_default` cannot tell what the default is here; use `unwrap_or(value)`") }),
            "unwrap_or_else" if first.is_some() => {
                if good {
                    return Some(Ok(payload.unwrap_or(Value::Unit)));
                }
                let f = first?.clone();
                Some(if is_opt { self.call_value(&f, Vec::new()) } else { self.call1(&f, payload.unwrap_or(Value::Unit)) })
            }
            "ok" => Some(Ok(if is_opt { recv.clone() } else if good { Value::some(payload.unwrap_or(Value::Unit)) } else { Value::none() })),
            "err" => Some(Ok(if good { Value::none() } else { Value::some(payload.unwrap_or(Value::Unit)) })),
            "ok_or" if is_opt && first.is_some() => Some(Ok(if good { Value::ok(payload.unwrap_or(Value::Unit)) } else { Value::err(first.cloned().unwrap_or(Value::Unit)) })),
            "ok_or_else" if is_opt && first.is_some() => {
                if good {
                    return Some(Ok(Value::ok(payload.unwrap_or(Value::Unit))));
                }
                let f = first?.clone();
                Some(self.call_value(&f, Vec::new()).map(Value::err))
            }
            "map" if first.is_some() => {
                if !good {
                    return Some(Ok(recv.clone()));
                }
                let f = first?.clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)).map(|x| if is_opt { Value::some(x) } else { Value::ok(x) }))
            }
            "map_err" if first.is_some() && !is_opt => {
                if good {
                    return Some(Ok(recv.clone()));
                }
                let f = first?.clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)).map(Value::err))
            }
            "and_then" if first.is_some() => {
                if !good {
                    return Some(Ok(recv.clone()));
                }
                let f = first?.clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)))
            }
            "or_else" if first.is_some() => {
                if good {
                    return Some(Ok(recv.clone()));
                }
                let f = first?.clone();
                Some(if is_opt { self.call_value(&f, Vec::new()) } else { self.call1(&f, payload.unwrap_or(Value::Unit)) })
            }
            "or" if first.is_some() => Some(Ok(if good { recv.clone() } else { first.cloned().unwrap_or(Value::Unit) })),
            "and" if first.is_some() => Some(Ok(if good { first.cloned().unwrap_or(Value::Unit) } else { recv.clone() })),
            "filter" if is_opt && first.is_some() => {
                if !good {
                    return Some(Ok(recv.clone()));
                }
                let f = first?.clone();
                let p = payload.unwrap_or(Value::Unit);
                Some(match self.call1(&f, p.clone()).and_then(|r| self.truthy(r)) {
                    Ok(true) => Ok(Value::some(p)),
                    Ok(false) => Ok(Value::none()),
                    Err(e) => Err(e),
                })
            }
            "is_some_and" | "is_ok_and" if first.is_some() => {
                if !good {
                    return Some(Ok(Value::Bool(false)));
                }
                let f = first?.clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)).and_then(|r| self.truthy(r)).map(Value::Bool))
            }
            "map_or" if args.len() == 2 => {
                if !good {
                    return Some(Ok(args[0].clone()));
                }
                let f = args[1].clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)))
            }
            "map_or_else" if args.len() == 2 => {
                if !good {
                    let f = args[0].clone();
                    return Some(if is_opt { self.call_value(&f, Vec::new()) } else { self.call1(&f, payload.unwrap_or(Value::Unit)) });
                }
                let f = args[1].clone();
                Some(self.call1(&f, payload.unwrap_or(Value::Unit)))
            }
            "flatten" if is_opt => Some(Ok(if good { payload.unwrap_or(Value::Unit) } else { recv.clone() })),
            "copied" | "cloned" | "as_ref" | "as_mut" | "as_deref" => Some(Ok(recv.clone())),
            "iter" | "into_iter" => Some(Ok(Value::vec(if good { alloc::vec![payload.unwrap_or(Value::Unit)] } else { Vec::new() }))),
            _ => None,
        }
    }

    fn option_mutate(&mut self, target: &mut Value, name: &str, args: Vec<Value>) -> Res {
        match name {
            "take" => Ok(core::mem::replace(target, Value::none())),
            "replace" if args.len() == 1 => Ok(core::mem::replace(target, Value::some(args[0].clone()))),
            "insert" if args.len() == 1 => {
                *target = Value::some(args[0].clone());
                Ok(args[0].clone())
            }
            "get_or_insert" if args.len() == 1 => {
                if target.is_variant("Option", "None") {
                    *target = Value::some(args[0].clone());
                }
                Ok(target.payload().cloned().unwrap_or(Value::Unit))
            }
            "get_or_insert_with" if args.len() == 1 => {
                if target.is_variant("Option", "None") {
                    let made = self.call_value(&args[0], Vec::new())?;
                    *target = Value::some(made);
                }
                Ok(target.payload().cloned().unwrap_or(Value::Unit))
            }
            _ => self.fail(format!("no method `{}` for an Option", name)),
        }
    }

    /// Methods every kind of value has.
    pub(crate) fn generic_method(&mut self, recv: Value, name: &str, args: &[Value]) -> Res {
        let other = args.first();
        let r: Result<Value, String> = match name {
            "clone" | "to_owned" | "into" | "as_ref" | "as_mut" | "borrow" | "borrow_mut" | "as_str" | "as_slice" | "to_vec" | "cloned" | "copied" | "as_deref" | "into_boxed_slice" | "unwrap_or_clone" => Ok(recv),
            "to_string" => {
                let mut s = String::new();
                if recv.display(&mut s) {
                    Ok(Value::string(s))
                } else {
                    Err(format!("a {} has no text form (use format!(\"{{:?}}\", x))", recv.type_name()))
                }
            }
            "eq" if other.is_some() => Ok(Value::Bool(eq_values(&recv, &args[0]))),
            "ne" if other.is_some() => Ok(Value::Bool(!eq_values(&recv, &args[0]))),
            "cmp" | "total_cmp" if other.is_some() => match cmp_values(&recv, &args[0]) {
                Some(o) => Ok(Value::ordering(o)),
                None => Err(format!("cannot compare {} with {}", recv.type_name(), args[0].type_name())),
            },
            "partial_cmp" if other.is_some() => Ok(Value::option(cmp_values(&recv, &args[0]).map(Value::ordering))),
            "lt" | "le" | "gt" | "ge" if other.is_some() => {
                let op = match name {
                    "lt" => crate::script::ast::BinOp::Lt,
                    "le" => crate::script::ast::BinOp::Le,
                    "gt" => crate::script::ast::BinOp::Gt,
                    _ => crate::script::ast::BinOp::Ge,
                };
                crate::script::ops::binary(op, recv, args[0].clone(), self.limits.max_string)
            }
            "min" | "max" if other.is_some() => match cmp_values(&recv, &args[0]) {
                Some(o) => Ok(if (name == "min") == (o != Ordering::Greater) { recv } else { args[0].clone() }),
                None => Err(format!("cannot compare {} with {}", recv.type_name(), args[0].type_name())),
            },
            "clamp" if args.len() == 2 => match (cmp_values(&recv, &args[0]), cmp_values(&recv, &args[1])) {
                (Some(Ordering::Less), _) => Ok(args[0].clone()),
                (_, Some(Ordering::Greater)) => Ok(args[1].clone()),
                (Some(_), Some(_)) => Ok(recv),
                _ => Err(format!("cannot compare {} with {}", recv.type_name(), args[0].type_name())),
            },
            _ => Err(format!("no method `{}` found for a {}", name, recv.type_name())),
        };
        self.lift(r)
    }
}

fn range_len(r: &RangeVal) -> i64 {
    match (r.lo, r.hi) {
        (Some(lo), Some(hi)) => {
            let hi = if r.incl { hi.saturating_add(1) } else { hi };
            (hi - lo).max(0)
        }
        _ => 0,
    }
}

// ---- numbers

fn int_methods(i: i64, name: &str, args: &[Value]) -> Found {
    match name {
        "abs" => i.checked_abs().map(|x| Some(Value::Int(x))).ok_or_else(|| String::from("attempt to take abs with overflow")),
        "unsigned_abs" => v(Value::Int(i.checked_abs().unwrap_or(i64::MAX))),
        "signum" => v(Value::Int(i.signum())),
        "is_positive" => v(Value::Bool(i > 0)),
        "is_negative" => v(Value::Bool(i < 0)),
        "pow" => {
            want(name, args, 1)?;
            let e = int(name, args, 0)?;
            if !(0..=u32::MAX as i64).contains(&e) {
                return Err(String::from("`pow` needs a non-negative exponent"));
            }
            match i.checked_pow(e as u32) {
                Some(x) => v(Value::Int(x)),
                None => Err(String::from("attempt to multiply with overflow")),
            }
        }
        "rem_euclid" | "div_euclid" => {
            want(name, args, 1)?;
            let m = int(name, args, 0)?;
            if m == 0 {
                return Err(String::from("attempt to divide by zero"));
            }
            let r = if name == "rem_euclid" { i.checked_rem_euclid(m) } else { i.checked_div_euclid(m) };
            r.map(|x| Some(Value::Int(x))).ok_or_else(|| String::from("attempt to divide with overflow"))
        }
        "abs_diff" => {
            want(name, args, 1)?;
            let o = int(name, args, 0)?;
            v(Value::Int(i.abs_diff(o).min(i64::MAX as u64) as i64))
        }
        "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add" | "saturating_sub" | "saturating_mul" => {
            want(name, args, 1)?;
            let o = int(name, args, 0)?;
            v(Value::Int(match name {
                "wrapping_add" => i.wrapping_add(o),
                "wrapping_sub" => i.wrapping_sub(o),
                "wrapping_mul" => i.wrapping_mul(o),
                "saturating_add" => i.saturating_add(o),
                "saturating_sub" => i.saturating_sub(o),
                _ => i.saturating_mul(o),
            }))
        }
        "checked_add" | "checked_sub" | "checked_mul" | "checked_div" | "checked_rem" => {
            want(name, args, 1)?;
            let o = int(name, args, 0)?;
            let r = match name {
                "checked_add" => i.checked_add(o),
                "checked_sub" => i.checked_sub(o),
                "checked_mul" => i.checked_mul(o),
                "checked_div" => i.checked_div(o),
                _ => i.checked_rem(o),
            };
            v(Value::option(r.map(Value::Int)))
        }
        "checked_pow" => {
            want(name, args, 1)?;
            let e = int(name, args, 0)?;
            v(Value::option(if (0..=u32::MAX as i64).contains(&e) { i.checked_pow(e as u32).map(Value::Int) } else { None }))
        }
        "count_ones" => v(Value::Int(i.count_ones() as i64)),
        "is_power_of_two" => v(Value::Bool(i > 0 && (i & (i - 1)) == 0)),
        "isqrt" if i >= 0 => v(Value::Int(nm::floor(nm::sqrt(i as f64)) as i64)),
        _ => Ok(None),
    }
}

fn float_methods(x: f64, name: &str, args: &[Value]) -> Found {
    let f = |r: f64| v(Value::Float(r));
    match name {
        "abs" => f(nm::fabs(x)),
        "sqrt" => f(nm::sqrt(x)),
        "floor" => f(nm::floor(x)),
        "ceil" => f(nm::ceil(x)),
        "round" => f(nm::round(x)),
        "trunc" => f(nm::trunc(x)),
        "fract" => f(x - nm::trunc(x)),
        "sin" => f(nm::sin(x)),
        "cos" => f(nm::cos(x)),
        "tan" => f(nm::tan(x)),
        "asin" => f(nm::asin(x)),
        "acos" => f(nm::acos(x)),
        "atan" => f(nm::atan(x)),
        "exp" => f(nm::exp(x)),
        "ln" => f(nm::ln(x)),
        "log10" => f(nm::ln(x) / core::f64::consts::LN_10),
        "log2" => f(nm::ln(x) / core::f64::consts::LN_2),
        "to_radians" => f(nm::to_radians(x)),
        "to_degrees" => f(nm::to_degrees(x)),
        "recip" => f(1.0 / x),
        "signum" => f(if x.is_nan() {
            x
        } else if x < 0.0 {
            -1.0
        } else {
            1.0
        }),
        "is_nan" => v(Value::Bool(x.is_nan())),
        "is_finite" => v(Value::Bool(x.is_finite())),
        "is_infinite" => v(Value::Bool(x.is_infinite())),
        "is_sign_negative" => v(Value::Bool(x < 0.0)),
        "is_sign_positive" => v(Value::Bool(x >= 0.0)),
        "powi" => {
            want(name, args, 1)?;
            f(nm::powi(x, int(name, args, 0)?))
        }
        "powf" => {
            want(name, args, 1)?;
            f(nm::powf(x, num(name, args, 0)?))
        }
        "log" => {
            want(name, args, 1)?;
            f(nm::ln(x) / nm::ln(num(name, args, 0)?))
        }
        "atan2" => {
            want(name, args, 1)?;
            f(nm::atan2(x, num(name, args, 0)?))
        }
        "hypot" => {
            want(name, args, 1)?;
            let o = num(name, args, 0)?;
            f(nm::sqrt(x * x + o * o))
        }
        "min" | "max" => {
            want(name, args, 1)?;
            let o = num(name, args, 0)?;
            f(if x.is_nan() {
                o
            } else if o.is_nan() {
                x
            } else if (name == "min") == (x <= o) {
                x
            } else {
                o
            })
        }
        "clamp" => {
            want(name, args, 2)?;
            let (lo, hi) = (num(name, args, 0)?, num(name, args, 1)?);
            f(if x < lo {
                lo
            } else if x > hi {
                hi
            } else {
                x
            })
        }
        "rem_euclid" => {
            want(name, args, 1)?;
            let m = num(name, args, 0)?;
            let r = nm::fmod(x, m);
            f(if r < 0.0 { r + nm::fabs(m) } else { r })
        }
        "mul_add" => {
            want(name, args, 2)?;
            f(x * num(name, args, 0)? + num(name, args, 1)?)
        }
        "copysign" => {
            want(name, args, 1)?;
            let s = num(name, args, 0)?;
            f(if (s < 0.0) == (x < 0.0) { x } else { -x })
        }
        _ => Ok(None),
    }
}

fn char_methods(c: char, name: &str, args: &[Value]) -> Found {
    match name {
        "is_alphabetic" => v(Value::Bool(c.is_alphabetic())),
        "is_numeric" => v(Value::Bool(c.is_numeric())),
        "is_alphanumeric" => v(Value::Bool(c.is_alphanumeric())),
        "is_whitespace" => v(Value::Bool(c.is_whitespace())),
        "is_uppercase" => v(Value::Bool(c.is_uppercase())),
        "is_lowercase" => v(Value::Bool(c.is_lowercase())),
        "is_control" => v(Value::Bool(c.is_control())),
        "is_ascii" => v(Value::Bool(c.is_ascii())),
        "is_ascii_digit" => v(Value::Bool(c.is_ascii_digit())),
        "is_ascii_alphabetic" => v(Value::Bool(c.is_ascii_alphabetic())),
        "is_ascii_alphanumeric" => v(Value::Bool(c.is_ascii_alphanumeric())),
        "is_ascii_uppercase" => v(Value::Bool(c.is_ascii_uppercase())),
        "is_ascii_lowercase" => v(Value::Bool(c.is_ascii_lowercase())),
        "is_ascii_punctuation" => v(Value::Bool(c.is_ascii_punctuation())),
        "is_ascii_whitespace" => v(Value::Bool(c.is_ascii_whitespace())),
        "to_ascii_uppercase" => v(Value::Char(c.to_ascii_uppercase())),
        "to_ascii_lowercase" => v(Value::Char(c.to_ascii_lowercase())),
        // Unlike Rust's iterators these give one character (the first).
        "to_uppercase" => v(Value::Char(c.to_uppercase().next().unwrap_or(c))),
        "to_lowercase" => v(Value::Char(c.to_lowercase().next().unwrap_or(c))),
        "len_utf8" => v(Value::Int(c.len_utf8() as i64)),
        "to_digit" | "is_digit" => {
            want(name, args, 1)?;
            let radix = int(name, args, 0)?;
            if !(2..=36).contains(&radix) {
                return Err(String::from("the radix must be 2 to 36"));
            }
            let d = c.to_digit(radix as u32);
            if name == "is_digit" {
                v(Value::Bool(d.is_some()))
            } else {
                v(Value::option(d.map(|d| Value::Int(d as i64))))
            }
        }
        "eq_ignore_ascii_case" => match args.first() {
            Some(Value::Char(o)) => v(Value::Bool(c.eq_ignore_ascii_case(o))),
            _ => Err(String::from("`eq_ignore_ascii_case` needs a char")),
        },
        _ => Ok(None),
    }
}
