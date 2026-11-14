//! The built-in library, part two: strings.
//!
//! `&str` and `String` are one kind of value. Positions are byte positions,
//! as in Rust. Methods that take a pattern take a string or a char (not a
//! closure).

use crate::script::ast::Type;
use crate::script::builtins::{v, Found};
use crate::script::host::{int, text, want};
use crate::script::interp::{Interp, Res};
use crate::script::value::*;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

/// A pattern argument: a string or a char.
fn pat(name: &str, args: &[Value], i: usize) -> Result<String, String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(String::from(s.as_str())),
        Some(Value::Char(c)) => Ok(String::from(*c)),
        Some(Value::Closure(_)) | Some(Value::Func(_)) => Err(format!("`{}` takes a string or a char here, not a closure", name)),
        Some(other) => Err(format!("`{}`: argument {} should be a string or a char, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

fn strs(parts: impl Iterator<Item = String>) -> Value {
    Value::vec(parts.map(Value::string).collect())
}

fn parse_error(msg: &str) -> Value {
    Value::err(Value::str(msg))
}

fn range_check(n: i64, ty: &str) -> bool {
    match ty {
        "i8" => (i8::MIN as i64..=i8::MAX as i64).contains(&n),
        "i16" => (i16::MIN as i64..=i16::MAX as i64).contains(&n),
        "i32" => (i32::MIN as i64..=i32::MAX as i64).contains(&n),
        "u8" => (0..=u8::MAX as i64).contains(&n),
        "u16" => (0..=u16::MAX as i64).contains(&n),
        "u32" => (0..=u32::MAX as i64).contains(&n),
        "u64" | "usize" | "u128" => n >= 0,
        _ => true,
    }
}

/// `s.parse::<T>()`: a Result.
pub(crate) fn parse_str(s: &str, tf: Option<&Type>) -> Value {
    let mut target = tf;
    if let Some(t) = target {
        if (&*t.name == "Result" || &*t.name == "Option") && !t.args.is_empty() {
            target = Some(&t.args[0]);
        }
    }
    let ty: &str = target.map_or("", |t| &*t.name);
    let parse_int = |s: &str| -> Value {
        if s.is_empty() {
            return parse_error("cannot parse integer from empty string");
        }
        match s.parse::<i64>() {
            Ok(n) if range_check(n, ty) => Value::ok(Value::Int(n)),
            Ok(_) => parse_error("number too large or too small to fit in target type"),
            Err(e) => {
                let m = format!("{}", e);
                parse_error(&m)
            }
        }
    };
    match ty {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => parse_int(s),
        "f32" | "f64" => match s.parse::<f64>() {
            Ok(f) => Value::ok(Value::Float(f)),
            Err(_) => parse_error("invalid float literal"),
        },
        "bool" => match s {
            "true" => Value::ok(Value::Bool(true)),
            "false" => Value::ok(Value::Bool(false)),
            _ => parse_error("provided string was not `true` or `false`"),
        },
        "char" => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Value::ok(Value::Char(c)),
                (None, _) => parse_error("cannot parse char from empty string"),
                _ => parse_error("too many characters in string"),
            }
        }
        "String" => Value::ok(Value::str(s)),
        // No type known: a whole number if it looks like one, else a float.
        _ => match s.parse::<i64>() {
            Ok(n) => Value::ok(Value::Int(n)),
            Err(_) => match s.parse::<f64>() {
                Ok(f) if !s.is_empty() => Value::ok(Value::Float(f)),
                _ => parse_error("invalid number"),
            },
        },
    }
}

fn str_pure(s: &str, name: &str, args: &[Value], tf: Option<&Type>, max_string: usize, max_vec: usize) -> Found {
    match name {
        "len" => v(Value::Int(s.len() as i64)),
        "is_empty" => v(Value::Bool(s.is_empty())),
        "is_ascii" => v(Value::Bool(s.is_ascii())),
        "to_uppercase" => v(Value::string(s.to_uppercase())),
        "to_lowercase" => v(Value::string(s.to_lowercase())),
        "to_ascii_uppercase" => v(Value::string(s.to_ascii_uppercase())),
        "to_ascii_lowercase" => v(Value::string(s.to_ascii_lowercase())),
        "trim" => v(Value::str(s.trim())),
        "trim_start" => v(Value::str(s.trim_start())),
        "trim_end" => v(Value::str(s.trim_end())),
        "trim_matches" | "trim_start_matches" | "trim_end_matches" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            let mut t = s;
            if p.is_empty() {
                return v(Value::str(t));
            }
            if name != "trim_end_matches" {
                while let Some(rest) = t.strip_prefix(p.as_str()) {
                    t = rest;
                }
            }
            if name != "trim_start_matches" {
                while let Some(rest) = t.strip_suffix(p.as_str()) {
                    t = rest;
                }
            }
            v(Value::str(t))
        }
        "contains" => {
            want(name, args, 1)?;
            v(Value::Bool(s.contains(pat(name, args, 0)?.as_str())))
        }
        "starts_with" => {
            want(name, args, 1)?;
            v(Value::Bool(s.starts_with(pat(name, args, 0)?.as_str())))
        }
        "ends_with" => {
            want(name, args, 1)?;
            v(Value::Bool(s.ends_with(pat(name, args, 0)?.as_str())))
        }
        "find" | "rfind" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            let at = if name == "find" { s.find(p.as_str()) } else { s.rfind(p.as_str()) };
            v(Value::option(at.map(|i| Value::Int(i as i64))))
        }
        "replace" => {
            want(name, args, 2)?;
            let (a, b) = (pat(name, args, 0)?, pat(name, args, 1)?);
            if a.is_empty() {
                return Err(String::from("`replace` cannot look for an empty string"));
            }
            let n = s.matches(a.as_str()).count();
            if s.len() + n * b.len() > max_string.saturating_add(n * a.len()) {
                return Err(format!("this string would be longer than {} bytes", max_string));
            }
            v(Value::string(s.replace(a.as_str(), &b)))
        }
        "replacen" => {
            want(name, args, 3)?;
            let (a, b, n) = (pat(name, args, 0)?, pat(name, args, 1)?, int(name, args, 2)?);
            if a.is_empty() || n < 0 {
                return Err(String::from("`replacen` needs a non-empty pattern and a count of 0 or more"));
            }
            v(Value::string(s.replacen(a.as_str(), &b, n as usize)))
        }
        "split" | "rsplit" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            if p.is_empty() {
                return Err(String::from("`split` cannot split on an empty string"));
            }
            let mut parts: Vec<String> = s.split(p.as_str()).map(String::from).collect();
            if name == "rsplit" {
                parts.reverse();
            }
            v(strs(parts.into_iter()))
        }
        "splitn" => {
            want(name, args, 2)?;
            let n = int(name, args, 0)?;
            let p = pat(name, args, 1)?;
            if p.is_empty() || n < 0 {
                return Err(String::from("`splitn` needs a count of 0 or more and a non-empty pattern"));
            }
            v(strs(s.splitn(n as usize, p.as_str()).map(String::from)))
        }
        "split_once" | "rsplit_once" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            if p.is_empty() {
                return Err(String::from("`split_once` cannot split on an empty string"));
            }
            let r = if name == "split_once" { s.split_once(p.as_str()) } else { s.rsplit_once(p.as_str()) };
            v(Value::option(r.map(|(a, b)| Value::tuple(alloc::vec![Value::str(a), Value::str(b)]))))
        }
        "split_whitespace" => v(strs(s.split_whitespace().map(String::from))),
        "lines" => v(strs(s.lines().map(String::from))),
        "chars" => v(Value::vec(s.chars().map(Value::Char).collect())),
        "char_indices" => v(Value::vec(s.char_indices().map(|(i, c)| Value::tuple(alloc::vec![Value::Int(i as i64), Value::Char(c)])).collect())),
        "bytes" | "as_bytes" | "into_bytes" => v(Value::vec(s.bytes().map(|b| Value::Int(b as i64)).collect())),
        "repeat" => {
            want(name, args, 1)?;
            let n = int(name, args, 0)?;
            if n < 0 || (n as u128) * (s.len() as u128) > max_string as u128 {
                return Err(format!("this string would be longer than {} bytes", max_string));
            }
            v(Value::string(s.repeat(n as usize)))
        }
        "matches" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            if p.is_empty() {
                return Err(String::from("`matches` cannot look for an empty string"));
            }
            let found: Vec<String> = s.matches(p.as_str()).map(String::from).take(max_vec).collect();
            v(strs(found.into_iter()))
        }
        "parse" => v(parse_str(s, tf)),
        "strip_prefix" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            v(Value::option(s.strip_prefix(p.as_str()).map(Value::str)))
        }
        "strip_suffix" => {
            want(name, args, 1)?;
            let p = pat(name, args, 0)?;
            v(Value::option(s.strip_suffix(p.as_str()).map(Value::str)))
        }
        "get" => {
            want(name, args, 1)?;
            match &args[0] {
                Value::Range(r) => {
                    let lo = r.lo.unwrap_or(0);
                    let hi = match r.hi {
                        Some(h) => h.saturating_add(r.incl as i64),
                        None => s.len() as i64,
                    };
                    v(Value::option(if lo >= 0 && hi >= lo { s.get(lo as usize..hi as usize).map(Value::str) } else { None }))
                }
                other => Err(format!("`get` on a string needs a range, found {}", other.type_name())),
            }
        }
        "eq_ignore_ascii_case" => {
            want(name, args, 1)?;
            v(Value::Bool(s.eq_ignore_ascii_case(text(name, args, 0)?)))
        }
        "is_char_boundary" => {
            want(name, args, 1)?;
            let i = int(name, args, 0)?;
            v(Value::Bool(i >= 0 && s.is_char_boundary(i as usize)))
        }
        _ => Ok(None),
    }
}

impl<'a> Interp<'a> {
    pub(crate) fn str_methods(&mut self, s: &Rc<String>, name: &str, args: &[Value], tf: Option<&Type>) -> Option<Res> {
        if name != "len" && name != "is_empty" {
            if let Err(f) = self.work(s.len()) {
                return Some(Err(f));
            }
        }
        let r = str_pure(s, name, args, tf, self.limits.max_string, self.limits.max_vec);
        if let Ok(Some(Value::Vec(items))) = &r {
            if let Err(f) = self.check_vec(items.len()) {
                return Some(Err(f));
            }
        }
        self.found(r)
    }

    pub(crate) fn str_mutate(&mut self, target: &mut Value, name: &str, args: Vec<Value>) -> Res {
        let max = self.limits.max_string;
        let Value::Str(rc) = target else { return self.fail("internal error: not a string") };
        let r: Result<Value, String> = match name {
            "push" => match args.first() {
                Some(Value::Char(c)) if rc.len() + c.len_utf8() <= max => {
                    Rc::make_mut(rc).push(*c);
                    Ok(Value::Unit)
                }
                Some(Value::Char(_)) => Err(format!("a string may not be longer than {} bytes", max)),
                _ => Err(String::from("`push` needs a char (use `push_str` for a string)")),
            },
            "push_str" => match args.first() {
                Some(Value::Str(t)) if rc.len() + t.len() <= max => {
                    Rc::make_mut(rc).push_str(t);
                    Ok(Value::Unit)
                }
                Some(Value::Str(_)) => Err(format!("a string may not be longer than {} bytes", max)),
                Some(Value::Char(c)) if rc.len() + c.len_utf8() <= max => {
                    Rc::make_mut(rc).push(*c);
                    Ok(Value::Unit)
                }
                _ => Err(String::from("`push_str` needs a string")),
            },
            "clear" => {
                Rc::make_mut(rc).clear();
                Ok(Value::Unit)
            }
            "pop" => Ok(Value::option(Rc::make_mut(rc).pop().map(Value::Char))),
            "insert" | "insert_str" => match (int(name, &args, 0), args.get(1)) {
                (Ok(i), Some(x)) => {
                    let piece = match x {
                        Value::Char(c) => String::from(*c),
                        Value::Str(t) => String::from(t.as_str()),
                        _ => return self.fail(format!("`{}` needs a char or a string to put in", name)),
                    };
                    if i < 0 || i as usize > rc.len() || !rc.is_char_boundary(i as usize) {
                        Err(format!("{} is not a place in this string where a character starts", i))
                    } else if rc.len() + piece.len() > max {
                        Err(format!("a string may not be longer than {} bytes", max))
                    } else {
                        Rc::make_mut(rc).insert_str(i as usize, &piece);
                        Ok(Value::Unit)
                    }
                }
                (Err(e), _) => Err(e),
                _ => Err(format!("`{}` needs a position and something to put in", name)),
            },
            "remove" => match int(name, &args, 0) {
                Ok(i) if i >= 0 && (i as usize) < rc.len() && rc.is_char_boundary(i as usize) => Ok(Value::Char(Rc::make_mut(rc).remove(i as usize))),
                Ok(i) => Err(format!("{} is not a place in this string where a character starts", i)),
                Err(e) => Err(e),
            },
            "truncate" => match int(name, &args, 0) {
                Ok(n) if n >= 0 && (n as usize >= rc.len() || rc.is_char_boundary(n as usize)) => {
                    Rc::make_mut(rc).truncate(n as usize);
                    Ok(Value::Unit)
                }
                Ok(n) => Err(format!("{} is not a place in this string where a character starts", n)),
                Err(e) => Err(e),
            },
            "make_ascii_uppercase" => {
                Rc::make_mut(rc).make_ascii_uppercase();
                Ok(Value::Unit)
            }
            "make_ascii_lowercase" => {
                Rc::make_mut(rc).make_ascii_lowercase();
                Ok(Value::Unit)
            }
            "retain" => {
                let Some(f) = args.first().cloned() else { return self.fail("`retain` needs a closure") };
                let chars: Vec<char> = rc.chars().collect();
                let mut kept = String::new();
                for c in chars {
                    let keep = self.call1(&f, Value::Char(c))?;
                    if self.truthy(keep)? {
                        kept.push(c);
                    }
                }
                *Rc::make_mut(rc) = kept;
                Ok(Value::Unit)
            }
            _ => Err(format!("no method `{}` for a string", name)),
        };
        self.lift(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str, name: &str, args: &[Value]) -> Value {
        str_pure(s, name, args, None, 1000, 1000).unwrap().unwrap()
    }

    #[test]
    fn searching_and_cutting() {
        assert_eq!(run("hello world", "contains", &[Value::str("lo w")]), Value::Bool(true));
        assert_eq!(run("hello", "starts_with", &[Value::Char('h')]), Value::Bool(true));
        assert_eq!(run("hello", "find", &[Value::str("l")]), Value::some(Value::Int(2)));
        assert_eq!(run("hello", "find", &[Value::str("z")]), Value::none());
        assert_eq!(run("  hi \n", "trim", &[]), Value::str("hi"));
        assert_eq!(run("a-b-c", "replace", &[Value::str("-"), Value::str("+")]), Value::str("a+b+c"));
        assert_eq!(run("k=v=w", "split_once", &[Value::Char('=')]), Value::some(Value::tuple(alloc::vec![Value::str("k"), Value::str("v=w")])));
        assert_eq!(run("pre-x", "strip_prefix", &[Value::str("pre-")]), Value::some(Value::str("x")));
    }

    #[test]
    fn splitting() {
        let parts = |s: &str, sep: Value| match run(s, "split", &[sep]) {
            Value::Vec(v) => v.iter().map(|x| x.display_string()).collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(parts("a b  c", Value::Char(' ')), ["a", "b", "", "c"]);
        assert_eq!(parts("a,b", Value::str(",")), ["a", "b"]);
        match run("  a  b ", "split_whitespace", &[]) {
            Value::Vec(v) => assert_eq!(v.len(), 2),
            _ => panic!(),
        }
        assert!(str_pure("x", "split", &[Value::str("")], None, 10, 10).is_err());
    }

    #[test]
    fn characters_and_bytes() {
        match run("héy", "chars", &[]) {
            Value::Vec(v) => assert_eq!(v.len(), 3),
            _ => panic!(),
        }
        assert_eq!(run("héy", "len", &[]), Value::Int(4));
        assert_eq!(run("ab", "repeat", &[Value::Int(3)]), Value::str("ababab"));
        assert!(str_pure("ab", "repeat", &[Value::Int(1000)], None, 100, 10).is_err());
    }

    fn ty(name: &str) -> Type {
        Type { name: crate::script::ast::Name::from(name), args: Vec::new() }
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_str("42", Some(&ty("i32"))), Value::ok(Value::Int(42)));
        assert_eq!(parse_str("-7", Some(&ty("i64"))), Value::ok(Value::Int(-7)));
        assert!(parse_str("x1", Some(&ty("i32"))).is_variant("Result", "Err"));
        assert!(parse_str("300", Some(&ty("u8"))).is_variant("Result", "Err"));
        assert!(parse_str("-1", Some(&ty("u32"))).is_variant("Result", "Err"));
        assert!(parse_str("", Some(&ty("i32"))).is_variant("Result", "Err"));
        assert_eq!(parse_str("2.5", Some(&ty("f64"))), Value::ok(Value::Float(2.5)));
        assert_eq!(parse_str("true", Some(&ty("bool"))), Value::ok(Value::Bool(true)));
        assert_eq!(parse_str("12", None), Value::ok(Value::Int(12)));
        assert_eq!(parse_str("1.5", None), Value::ok(Value::Float(1.5)));
        assert!(parse_str("abc", None).is_variant("Result", "Err"));
        let wrapped = Type { name: crate::script::ast::Name::from("Result"), args: alloc::vec![ty("u8")] };
        assert_eq!(parse_str("7", Some(&wrapped)), Value::ok(Value::Int(7)));
    }
}
