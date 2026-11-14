//! `format!` strings: reading them (once, when the script is parsed) and
//! showing a value the way a `{:>8.2}` asks.
//!
//! Understood: `{}`, `{:?}`, `{0}`, `{name}` (a variable of that name),
//! `{name}` given as `name = expr`, `{{` and `}}`, and in the spec the fill
//! and alignment (`<`, `^`, `>`), `+`, `#`, `0`, a width, `.precision` and
//! the types `?`, `x`, `X`, `b`, `o` and `e`. Not understood: `width$` and
//! `.*`.

use crate::script::ast::{Name, Piece, Spec};
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Read a format string. `positional` is how many arguments were given
/// without a name, `named` the names of the ones that had one (they come
/// after the positional ones). Returns the pieces, and the names of
/// variables the string mentions that were not given as arguments: the
/// caller appends one `Path` expression for each, in that order, after the
/// named arguments.
pub fn parse_format(fmt: &str, positional: usize, named: &[Name]) -> Result<(Vec<Piece>, Vec<Name>), String> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut lit = String::new();
    let mut extras: Vec<Name> = Vec::new();
    let mut next = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            if chars.get(i + 1) == Some(&'{') {
                lit.push('{');
                i += 2;
                continue;
            }
            let Some(close) = chars[i..].iter().position(|&c| c == '}').map(|p| p + i) else {
                return Err(String::from("a `{` in the format string is never closed (write `{{` for a brace)"));
            };
            let inner: String = chars[i + 1..close].iter().collect();
            let (arg_text, spec_text) = match inner.find(':') {
                Some(p) => (&inner[..p], Some(&inner[p + 1..])),
                None => (&inner[..], None),
            };
            let arg_text = arg_text.trim();
            let arg = if arg_text.is_empty() {
                let a = next;
                next += 1;
                a
            } else if arg_text.chars().all(|c| c.is_ascii_digit()) {
                arg_text.parse::<usize>().map_err(|_| String::from("bad argument number in the format string"))?
            } else if arg_text.chars().all(|c| c.is_alphanumeric() || c == '_') {
                match named.iter().position(|n| &**n == arg_text) {
                    Some(p) => positional + p,
                    None => match extras.iter().position(|n| &**n == arg_text) {
                        Some(p) => positional + named.len() + p,
                        None => {
                            extras.push(Name::from(arg_text));
                            positional + named.len() + extras.len() - 1
                        }
                    },
                }
            } else {
                return Err(format!("`{}` in the format string is not an argument", arg_text));
            };
            let spec = match spec_text {
                Some(t) => parse_spec(t)?,
                None => Spec::DISPLAY,
            };
            if !lit.is_empty() {
                pieces.push(Piece::Lit(core::mem::take(&mut lit)));
            }
            pieces.push(Piece::Arg { arg, spec });
            i = close + 1;
        } else if c == '}' {
            if chars.get(i + 1) == Some(&'}') {
                lit.push('}');
                i += 2;
            } else {
                return Err(String::from("a `}` in the format string has no `{` (write `}}` for a brace)"));
            }
        } else {
            lit.push(c);
            i += 1;
        }
    }
    if !lit.is_empty() {
        pieces.push(Piece::Lit(lit));
    }
    // Positional arguments the string asks for must exist.
    for p in &pieces {
        if let Piece::Arg { arg, .. } = p {
            if *arg >= positional + named.len() + extras.len() {
                return Err(format!("the format string asks for argument {} but only {} were given", arg + 1, positional));
            }
        }
    }
    Ok((pieces, extras))
}

fn parse_spec(t: &str) -> Result<Spec, String> {
    let c: Vec<char> = t.chars().collect();
    let mut s = Spec::DISPLAY;
    let mut i = 0;
    let is_align = |ch: char| ch == '<' || ch == '>' || ch == '^';
    if c.len() >= 2 && is_align(c[1]) {
        s.fill = c[0];
        s.align = Some(c[1]);
        i = 2;
    } else if !c.is_empty() && is_align(c[0]) {
        s.align = Some(c[0]);
        i = 1;
    }
    if c.get(i) == Some(&'+') {
        s.plus = true;
        i += 1;
    }
    if c.get(i) == Some(&'#') {
        s.alt = true;
        i += 1;
    }
    if c.get(i) == Some(&'0') && c.get(i + 1).map_or(false, |d| d.is_ascii_digit() || *d == '$') {
        s.zero = true;
        i += 1;
    } else if c.get(i) == Some(&'0') && i + 1 == c.len() {
        // `{:0}`: a width of zero.
        s.width = Some(0);
        i += 1;
    }
    let mut width = String::new();
    while c.get(i).map_or(false, |d| d.is_ascii_digit()) {
        width.push(c[i]);
        i += 1;
    }
    if c.get(i) == Some(&'$') {
        return Err(String::from("`width$` in a format string is not supported; compute the string first"));
    }
    if !width.is_empty() {
        s.width = Some(width.parse::<usize>().map_err(|_| String::from("bad width in the format string"))?.min(1000));
    }
    if c.get(i) == Some(&'.') {
        i += 1;
        if c.get(i) == Some(&'*') {
            return Err(String::from("`.*` in a format string is not supported"));
        }
        let mut p = String::new();
        while c.get(i).map_or(false, |d| d.is_ascii_digit()) {
            p.push(c[i]);
            i += 1;
        }
        s.prec = Some(p.parse::<usize>().map_err(|_| String::from("bad precision in the format string"))?.min(100));
    }
    if c.get(i) == Some(&'#') && c.get(i + 1) == Some(&'?') {
        s.alt = true;
        i += 1;
    }
    match c.get(i) {
        None => {}
        Some(&t) if (t == '?' || t == 'x' || t == 'X' || t == 'b' || t == 'o' || t == 'e') && i + 1 == c.len() => s.ty = t,
        _ => return Err(format!("`{{:{}}}` is not a format this understands", t)),
    }
    Ok(s)
}

/// Show `v` as `spec` says. `Err` says what could not be shown.
pub fn format_value(v: &Value, spec: &Spec) -> Result<String, String> {
    let numeric = matches!(v, Value::Int(_) | Value::Float(_));
    let mut body = String::new();
    match (spec.ty, v) {
        (' ', Value::Float(f)) => match spec.prec {
            Some(p) => body = format!("{:.*}", p, f),
            None => {
                v.display(&mut body);
            }
        },
        (' ', Value::Str(s)) => match spec.prec {
            Some(p) => body = s.chars().take(p).collect(),
            None => body.push_str(s),
        },
        (' ', _) => {
            if !v.display(&mut body) {
                return Err(format!("a {} cannot be shown with `{{}}`; use `{{:?}}`", v.type_name()));
            }
        }
        ('?', Value::Float(f)) => match spec.prec {
            Some(p) => body = format!("{:.*?}", p, f),
            None => v.debug(&mut body),
        },
        ('?', _) => v.debug(&mut body),
        ('x', Value::Int(i)) => body = if spec.alt { format!("0x{:x}", *i as u64) } else { format!("{:x}", *i as u64) },
        ('X', Value::Int(i)) => body = if spec.alt { format!("0x{:X}", *i as u64) } else { format!("{:X}", *i as u64) },
        ('b', Value::Int(i)) => body = if spec.alt { format!("0b{:b}", *i as u64) } else { format!("{:b}", *i as u64) },
        ('o', Value::Int(i)) => body = if spec.alt { format!("0o{:o}", *i as u64) } else { format!("{:o}", *i as u64) },
        ('e', Value::Float(f)) => body = format!("{:e}", f),
        ('e', Value::Int(i)) => body = format!("{:e}", *i),
        (t, _) => return Err(format!("a {} cannot be shown with `{{:{}}}`", v.type_name(), t)),
    }
    if spec.plus && numeric && !body.starts_with('-') {
        body.insert(0, '+');
    }
    Ok(pad(body, spec, numeric))
}

/// Fill out to the width.
fn pad(body: String, spec: &Spec, numeric: bool) -> String {
    let Some(width) = spec.width else { return body };
    let len = body.chars().count();
    if len >= width {
        return body;
    }
    let gap = width - len;
    if spec.zero && numeric {
        // Zeros go after the sign (and the 0x).
        let split = if body.starts_with('-') || body.starts_with('+') { 1 } else { 0 };
        let split = if body[split..].starts_with("0x") || body[split..].starts_with("0b") || body[split..].starts_with("0o") { split + 2 } else { split };
        let mut out = String::from(&body[..split]);
        for _ in 0..gap {
            out.push('0');
        }
        out.push_str(&body[split..]);
        return out;
    }
    let align = spec.align.unwrap_or(if numeric { '>' } else { '<' });
    let (left, right) = match align {
        '>' => (gap, 0),
        '^' => (gap / 2, gap - gap / 2),
        _ => (0, gap),
    };
    let mut out = String::new();
    for _ in 0..left {
        out.push(spec.fill);
    }
    out.push_str(&body);
    for _ in 0..right {
        out.push(spec.fill);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(t: &str) -> Spec {
        parse_spec(t).unwrap()
    }

    fn show(v: Value, t: &str) -> String {
        format_value(&v, &spec(t)).unwrap()
    }

    #[test]
    fn plain_pieces() {
        let (p, extra) = parse_format("a {} b {:?}", 2, &[]).unwrap();
        assert!(extra.is_empty());
        assert_eq!(p.len(), 4);
        assert_eq!(p[0], Piece::Lit(String::from("a ")));
        assert_eq!(p[1], Piece::Arg { arg: 0, spec: Spec::DISPLAY });
        assert!(matches!(&p[3], Piece::Arg { arg: 1, spec } if spec.ty == '?'));
    }

    #[test]
    fn braces_and_captured_names() {
        let (p, extra) = parse_format("{{x}} {name} {name}", 0, &[]).unwrap();
        assert_eq!(extra, [Name::from("name")]);
        assert_eq!(p[0], Piece::Lit(String::from("{x} ")));
        assert_eq!(p[1], Piece::Arg { arg: 0, spec: Spec::DISPLAY });
        assert_eq!(p[3], Piece::Arg { arg: 0, spec: Spec::DISPLAY });
    }

    #[test]
    fn named_arguments_and_positions() {
        let (p, extra) = parse_format("{b} {0} {a}", 1, &[Name::from("a"), Name::from("b")]).unwrap();
        assert!(extra.is_empty());
        assert_eq!(p[0], Piece::Arg { arg: 2, spec: Spec::DISPLAY });
        assert_eq!(p[2], Piece::Arg { arg: 0, spec: Spec::DISPLAY });
        assert_eq!(p[4], Piece::Arg { arg: 1, spec: Spec::DISPLAY });
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(parse_format("{", 0, &[]).is_err());
        assert!(parse_format("}", 0, &[]).is_err());
        assert!(parse_format("{} {}", 1, &[]).is_err());
        assert!(parse_format("{:q}", 1, &[]).is_err());
        assert!(parse_format("{:w$}", 1, &[]).is_err());
    }

    #[test]
    fn widths_alignment_and_fill() {
        assert_eq!(show(Value::Int(5), "4"), "   5");
        assert_eq!(show(Value::str("ab"), "4"), "ab  ");
        assert_eq!(show(Value::str("ab"), ">4"), "  ab");
        assert_eq!(show(Value::str("ab"), "*^6"), "**ab**");
        assert_eq!(show(Value::Int(42), "05"), "00042");
        assert_eq!(show(Value::Int(-42), "05"), "-0042");
        assert_eq!(show(Value::Int(7), "<3"), "7  ");
    }

    #[test]
    fn precision_and_signs() {
        assert_eq!(show(Value::Float(3.14159), ".2"), "3.14");
        assert_eq!(show(Value::Float(2.0), ".1"), "2.0");
        assert_eq!(show(Value::Float(2.5), "8.3"), "   2.500");
        assert_eq!(show(Value::Int(3), "+"), "+3");
        assert_eq!(show(Value::str("abcdef"), ".3"), "abc");
        assert_eq!(show(Value::Float(1.0), "?"), "1.0");
    }

    #[test]
    fn number_bases() {
        assert_eq!(show(Value::Int(255), "x"), "ff");
        assert_eq!(show(Value::Int(255), "#X"), "0xFF");
        assert_eq!(show(Value::Int(5), "b"), "101");
        assert_eq!(show(Value::Int(5), "08b"), "00000101");
        assert_eq!(show(Value::Int(8), "o"), "10");
    }

    #[test]
    fn what_has_no_display() {
        assert!(format_value(&Value::none(), &Spec::DISPLAY).is_err());
        assert!(format_value(&Value::str("x"), &spec("x")).is_err());
        assert_eq!(show(Value::none(), "?"), "None");
    }
}
