//! The values a script works with.
//!
//! Strings, vectors, tuples, structs and enums share their storage with
//! `Rc` and copy it when one of the sharers changes it (copy on write), so
//! passing a Vec around costs nothing until it is changed, and everything
//! behaves like the by-value Rust it imitates.

use crate::script::ast::{self, Name};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt::Write;

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    /// Every integer type is a 64-bit integer here.
    Int(i64),
    /// `f32` and `f64` are both 64-bit floats here.
    Float(f64),
    Char(char),
    /// `&str` and `String`.
    Str(Rc<String>),
    Vec(Rc<List>),
    Tuple(Rc<List>),
    Struct(Rc<StructVal>),
    Enum(Rc<EnumVal>),
    Range(Rc<RangeVal>),
    Closure(Rc<ClosureVal>),
    /// A function used as a value: `name` or `Type::name`.
    Func(Name),
    /// `&mut place`: a way to reach a variable (or a part of one) that
    /// lives further down the stack.
    Ref(Rc<Place>),
}

#[derive(Clone, Debug)]
pub struct StructVal {
    pub name: Name,
    pub fields: Vec<(Name, Value)>,
}

#[derive(Clone, Debug)]
pub struct EnumVal {
    pub ty: Name,
    pub variant: Name,
    pub fields: Vec<Value>,
    /// The field names of a variant written with braces.
    pub names: Option<Rc<Vec<Name>>>,
}

#[derive(Clone, Debug)]
pub struct RangeVal {
    pub lo: Option<i64>,
    pub hi: Option<i64>,
    pub incl: bool,
    /// The ends were characters (`'a'..='z'`).
    pub chars: bool,
}

#[derive(Debug)]
pub struct ClosureVal {
    pub def: Rc<ast::Closure>,
    /// What it could see where it was made, copied.
    pub env: Vec<(Name, Value)>,
    pub self_ty: Option<Name>,
}

/// The elements of a Vec or tuple. It is a `Vec<Value>` (it derefs to one)
/// whose destructor does not recurse: a script that builds a list inside a
/// list inside a list... tens of thousands deep must not overflow the
/// stack when it all goes away.
#[derive(Clone, Debug, Default)]
pub struct List(pub Vec<Value>);

impl core::ops::Deref for List {
    type Target = Vec<Value>;
    fn deref(&self) -> &Vec<Value> {
        &self.0
    }
}

impl core::ops::DerefMut for List {
    fn deref_mut(&mut self) -> &mut Vec<Value> {
        &mut self.0
    }
}

impl Drop for List {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            unwind(core::mem::take(&mut self.0));
        }
    }
}

impl Drop for EnumVal {
    fn drop(&mut self) {
        if !self.fields.is_empty() {
            unwind(core::mem::take(&mut self.fields));
        }
    }
}

impl Drop for StructVal {
    fn drop(&mut self) {
        if !self.fields.is_empty() {
            unwind(core::mem::take(&mut self.fields).into_iter().map(|f| f.1).collect());
        }
    }
}

impl Drop for ClosureVal {
    fn drop(&mut self) {
        if !self.env.is_empty() {
            unwind(core::mem::take(&mut self.env).into_iter().map(|f| f.1).collect());
        }
    }
}

/// Let go of values one at a time, from a list of our own, taking the
/// insides of every container that nobody else holds on to.
fn unwind(mut work: Vec<Value>) {
    while let Some(v) = work.pop() {
        match v {
            Value::Vec(rc) | Value::Tuple(rc) => {
                if let Ok(mut list) = Rc::try_unwrap(rc) {
                    work.append(&mut list.0);
                }
            }
            Value::Enum(rc) => {
                if let Ok(mut e) = Rc::try_unwrap(rc) {
                    work.append(&mut e.fields);
                }
            }
            Value::Struct(rc) => {
                if let Ok(mut s) = Rc::try_unwrap(rc) {
                    work.extend(core::mem::take(&mut s.fields).into_iter().map(|f| f.1));
                }
            }
            Value::Closure(rc) => {
                if let Ok(mut c) = Rc::try_unwrap(rc) {
                    work.extend(core::mem::take(&mut c.env).into_iter().map(|f| f.1));
                }
            }
            _ => {}
        }
    }
}

/// One step into a value.
#[derive(Clone, Debug, PartialEq)]
pub enum Elem {
    Field(Name),
    /// An element of a Vec, a tuple, a tuple struct or a variant's fields.
    Index(usize),
}

/// A variable on the interpreter's stack (by position) and the way into it.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub slot: usize,
    pub path: Vec<Elem>,
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::new(String::from(s)))
    }

    pub fn string(s: String) -> Value {
        Value::Str(Rc::new(s))
    }

    pub fn vec(v: Vec<Value>) -> Value {
        Value::Vec(Rc::new(List(v)))
    }

    pub fn tuple(v: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(List(v)))
    }

    fn variant(ty: &str, variant: &str, fields: Vec<Value>) -> Value {
        Value::Enum(Rc::new(EnumVal { ty: Name::from(ty), variant: Name::from(variant), fields, names: None }))
    }

    pub fn some(v: Value) -> Value {
        Value::variant("Option", "Some", alloc::vec![v])
    }

    pub fn none() -> Value {
        Value::variant("Option", "None", Vec::new())
    }

    pub fn ok(v: Value) -> Value {
        Value::variant("Result", "Ok", alloc::vec![v])
    }

    pub fn err(v: Value) -> Value {
        Value::variant("Result", "Err", alloc::vec![v])
    }

    pub fn option(v: Option<Value>) -> Value {
        match v {
            Some(v) => Value::some(v),
            None => Value::none(),
        }
    }

    pub fn ordering(o: Ordering) -> Value {
        Value::variant(
            "Ordering",
            match o {
                Ordering::Less => "Less",
                Ordering::Equal => "Equal",
                Ordering::Greater => "Greater",
            },
            Vec::new(),
        )
    }

    /// The `Ordering` a value stands for, if it is one.
    pub fn as_ordering(&self) -> Option<Ordering> {
        match self {
            Value::Enum(e) if &*e.ty == "Ordering" => match &*e.variant {
                "Less" => Some(Ordering::Less),
                "Equal" => Some(Ordering::Equal),
                "Greater" => Some(Ordering::Greater),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn is_variant(&self, ty: &str, variant: &str) -> bool {
        matches!(self, Value::Enum(e) if &*e.ty == ty && &*e.variant == variant)
    }

    /// The payload of `Some(..)`, `Ok(..)` or `Err(..)`.
    pub fn payload(&self) -> Option<&Value> {
        match self {
            Value::Enum(e) => e.fields.first(),
            _ => None,
        }
    }

    /// What this kind of value is called in an error message.
    pub fn type_name(&self) -> String {
        match self {
            Value::Unit => String::from("()"),
            Value::Bool(_) => String::from("bool"),
            Value::Int(_) => String::from("integer"),
            Value::Float(_) => String::from("float"),
            Value::Char(_) => String::from("char"),
            Value::Str(_) => String::from("string"),
            Value::Vec(_) => String::from("Vec"),
            Value::Tuple(_) => String::from("tuple"),
            Value::Struct(s) => String::from(&*s.name),
            Value::Enum(e) => String::from(&*e.ty),
            Value::Range(_) => String::from("range"),
            Value::Closure(_) => String::from("closure"),
            Value::Func(_) => String::from("function"),
            Value::Ref(_) => String::from("reference"),
        }
    }

    /// How many cells this value costs to make (see `Limits::max_cells`).
    pub fn cells(&self) -> usize {
        match self {
            Value::Str(s) => 1 + s.len() / 16,
            Value::Vec(v) | Value::Tuple(v) => 1 + v.len(),
            Value::Struct(s) => 1 + s.fields.len(),
            Value::Enum(e) => 1 + e.fields.len(),
            Value::Closure(c) => 1 + c.env.len(),
            _ => 0,
        }
    }

    /// Write the `{}` form. `false` when the value has none.
    pub fn display(&self, out: &mut String) -> bool {
        match self {
            Value::Unit => out.push_str("()"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(i) => {
                let _ = write!(out, "{}", i);
            }
            Value::Float(f) => {
                let _ = write!(out, "{}", f);
            }
            Value::Char(c) => out.push(*c),
            Value::Str(s) => out.push_str(s),
            _ => return false,
        }
        true
    }

    /// Write the `{:?}` form.
    pub fn debug(&self, out: &mut String) {
        self.debug_at(out, 0);
    }

    /// `debug`, `depth` levels down (a list inside a list inside a list... stops at 64).
    fn debug_at(&self, out: &mut String, depth: usize) {
        if depth > 64 {
            out.push_str("...");
            return;
        }
        match self {
            Value::Unit => out.push_str("()"),
            Value::Bool(_) | Value::Int(_) => {
                self.display(out);
            }
            Value::Float(f) => {
                let _ = write!(out, "{:?}", f);
            }
            Value::Char(c) => {
                let _ = write!(out, "{:?}", c);
            }
            Value::Str(s) => {
                let _ = write!(out, "{:?}", s.as_str());
            }
            Value::Vec(v) => debug_list(out, "[", "]", v, depth),
            Value::Tuple(v) => {
                debug_list(out, "(", "", v, depth);
                out.push_str(if v.len() == 1 { ",)" } else { ")" });
            }
            Value::Struct(s) => {
                out.push_str(&s.name);
                let named = s.fields.first().map_or(true, |f| !f.0.starts_with(|c: char| c.is_ascii_digit()));
                if s.fields.is_empty() {
                    // A unit struct.
                } else if named {
                    out.push_str(" { ");
                    for (k, (n, v)) in s.fields.iter().enumerate() {
                        if k > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(n);
                        out.push_str(": ");
                        v.debug_at(out, depth + 1);
                    }
                    out.push_str(" }");
                } else {
                    out.push('(');
                    for (k, (_, v)) in s.fields.iter().enumerate() {
                        if k > 0 {
                            out.push_str(", ");
                        }
                        v.debug_at(out, depth + 1);
                    }
                    out.push(')');
                }
            }
            Value::Enum(e) => {
                out.push_str(&e.variant);
                if e.fields.is_empty() {
                } else if let Some(names) = &e.names {
                    out.push_str(" { ");
                    for (k, v) in e.fields.iter().enumerate() {
                        if k > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(names.get(k).map_or("?", |n| n));
                        out.push_str(": ");
                        v.debug_at(out, depth + 1);
                    }
                    out.push_str(" }");
                } else {
                    debug_list(out, "(", ")", &e.fields, depth);
                }
            }
            Value::Range(r) => {
                if let Some(lo) = r.lo {
                    let _ = write!(out, "{}", lo);
                }
                out.push_str(if r.incl { "..=" } else { ".." });
                if let Some(hi) = r.hi {
                    let _ = write!(out, "{}", hi);
                }
            }
            Value::Closure(_) => out.push_str("<closure>"),
            Value::Func(n) => {
                let _ = write!(out, "<fn {}>", n);
            }
            Value::Ref(_) => out.push_str("<reference>"),
        }
    }

    pub fn debug_string(&self) -> String {
        let mut s = String::new();
        self.debug(&mut s);
        s
    }

    pub fn display_string(&self) -> String {
        let mut s = String::new();
        if !self.display(&mut s) {
            self.debug(&mut s);
        }
        s
    }
}

fn debug_list(out: &mut String, open: &str, close: &str, v: &[Value], depth: usize) {
    out.push_str(open);
    for (k, x) in v.iter().enumerate() {
        if k > 0 {
            out.push_str(", ");
        }
        x.debug_at(out, depth + 1);
    }
    out.push_str(close);
}

/// `a == b`, as Rust writes it (a whole number equals the same float).
pub fn eq_values(a: &Value, b: &Value) -> bool {
    eq_at(a, b, 0)
}

/// How deep `==` and `<` look into values (deeper ones are not equal).
const COMPARE_DEPTH: usize = 200;

fn eq_at(a: &Value, b: &Value, depth: usize) -> bool {
    if depth > COMPARE_DEPTH {
        return false;
    }
    match (a, b) {
        (Value::Unit, Value::Unit) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => (*x as f64) == *y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Vec(x), Value::Vec(y)) | (Value::Tuple(x), Value::Tuple(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| eq_at(p, q, depth + 1)),
        (Value::Struct(x), Value::Struct(y)) => x.name == y.name && x.fields.len() == y.fields.len() && x.fields.iter().zip(y.fields.iter()).all(|(p, q)| p.0 == q.0 && eq_at(&p.1, &q.1, depth + 1)),
        (Value::Enum(x), Value::Enum(y)) => x.ty == y.ty && x.variant == y.variant && x.fields.len() == y.fields.len() && x.fields.iter().zip(y.fields.iter()).all(|(p, q)| eq_at(p, q, depth + 1)),
        (Value::Range(x), Value::Range(y)) => x.lo == y.lo && x.hi == y.hi && x.incl == y.incl,
        (Value::Func(x), Value::Func(y)) => x == y,
        _ => false,
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        eq_values(self, other)
    }
}

/// Which place a unit variant of a built-in enum has in the order Rust
/// gives it (`None` before `Some`, `Ok` before `Err`, `Less` before ...).
fn builtin_rank(e: &EnumVal) -> Option<u8> {
    match (&*e.ty, &*e.variant) {
        ("Option", "None") | ("Result", "Ok") | ("Ordering", "Less") => Some(0),
        ("Option", "Some") | ("Result", "Err") | ("Ordering", "Equal") => Some(1),
        ("Ordering", "Greater") => Some(2),
        _ => None,
    }
}

/// `a.cmp(b)`, where Rust would allow it; `None` when the two cannot be
/// ordered (a float that is NaN, different kinds of value).
pub fn cmp_values(a: &Value, b: &Value) -> Option<Ordering> {
    cmp_at(a, b, 0)
}

fn cmp_at(a: &Value, b: &Value, depth: usize) -> Option<Ordering> {
    if depth > COMPARE_DEPTH {
        return None;
    }
    match (a, b) {
        (Value::Unit, Value::Unit) => Some(Ordering::Equal),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::Char(x), Value::Char(y)) => Some(x.cmp(y)),
        (Value::Str(x), Value::Str(y)) => Some(x.as_str().cmp(y.as_str())),
        (Value::Vec(x), Value::Vec(y)) | (Value::Tuple(x), Value::Tuple(y)) => cmp_lists(x, y, depth),
        (Value::Enum(x), Value::Enum(y)) if x.ty == y.ty => {
            if x.variant == y.variant {
                cmp_lists(&x.fields, &y.fields, depth)
            } else {
                match (builtin_rank(x), builtin_rank(y)) {
                    (Some(p), Some(q)) => Some(p.cmp(&q)),
                    _ => None,
                }
            }
        }
        _ => None,
    }
}

fn cmp_lists(x: &[Value], y: &[Value], depth: usize) -> Option<Ordering> {
    for (p, q) in x.iter().zip(y.iter()) {
        match cmp_at(p, q, depth + 1)? {
            Ordering::Equal => {}
            other => return Some(other),
        }
    }
    Some(x.len().cmp(&y.len()))
}

/// A float written the way the shortest `f32` would be, as a float: the
/// numbers the game hands over as `f32` print as the digits a Rust `f32`
/// would show (0.016666668, not 0.016666667740702629).
pub fn clean_f32(x: f32) -> f64 {
    let s = format!("{}", x);
    s.parse::<f64>().unwrap_or(x as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_debug() {
        assert_eq!(Value::Int(5).display_string(), "5");
        assert_eq!(Value::Float(1.0).display_string(), "1");
        assert_eq!(Value::Float(1.0).debug_string(), "1.0");
        assert_eq!(Value::Float(0.5).display_string(), "0.5");
        assert_eq!(Value::str("hi").debug_string(), "\"hi\"");
        assert_eq!(Value::Char('x').debug_string(), "'x'");
        assert_eq!(Value::vec(alloc::vec![Value::Int(1), Value::Int(2)]).debug_string(), "[1, 2]");
        assert_eq!(Value::tuple(alloc::vec![Value::Int(1), Value::str("a")]).debug_string(), "(1, \"a\")");
        assert_eq!(Value::tuple(alloc::vec![Value::Int(1)]).debug_string(), "(1,)");
        assert_eq!(Value::some(Value::Int(3)).debug_string(), "Some(3)");
        assert_eq!(Value::none().debug_string(), "None");
        assert_eq!(Value::Unit.debug_string(), "()");
    }

    #[test]
    fn a_struct_prints_like_derived_debug() {
        let s = Value::Struct(Rc::new(StructVal { name: Name::from("P"), fields: alloc::vec![(Name::from("x"), Value::Int(1)), (Name::from("y"), Value::Int(2))] }));
        assert_eq!(s.debug_string(), "P { x: 1, y: 2 }");
        let t = Value::Struct(Rc::new(StructVal { name: Name::from("W"), fields: alloc::vec![(Name::from("0"), Value::Int(7))] }));
        assert_eq!(t.debug_string(), "W(7)");
    }

    #[test]
    fn equality_is_structural() {
        assert_eq!(Value::vec(alloc::vec![Value::Int(1)]), Value::vec(alloc::vec![Value::Int(1)]));
        assert_ne!(Value::Int(1), Value::Int(2));
        assert_eq!(Value::Int(2), Value::Float(2.0));
        assert_ne!(Value::str("a"), Value::Char('a'));
        assert_eq!(Value::some(Value::Int(1)), Value::some(Value::Int(1)));
        assert_ne!(Value::some(Value::Int(1)), Value::none());
    }

    #[test]
    fn ordering_of_values() {
        assert_eq!(cmp_values(&Value::Int(1), &Value::Int(2)), Some(Ordering::Less));
        assert_eq!(cmp_values(&Value::str("b"), &Value::str("a")), Some(Ordering::Greater));
        assert_eq!(cmp_values(&Value::Float(f64::NAN), &Value::Float(1.0)), None);
        assert_eq!(cmp_values(&Value::none(), &Value::some(Value::Int(0))), Some(Ordering::Less));
        assert_eq!(cmp_values(&Value::tuple(alloc::vec![Value::Int(1), Value::Int(5)]), &Value::tuple(alloc::vec![Value::Int(1), Value::Int(6)])), Some(Ordering::Less));
        assert_eq!(cmp_values(&Value::Int(1), &Value::str("a")), None);
    }

    #[test]
    fn f32_numbers_print_short() {
        assert_eq!(Value::Float(clean_f32(1.0 / 60.0)).display_string(), "0.016666668");
        assert_eq!(Value::Float(clean_f32(0.5)).display_string(), "0.5");
    }
}
