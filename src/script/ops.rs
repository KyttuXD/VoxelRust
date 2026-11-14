//! Operators and conversions on values: `+`, `<`, `as`, and the small
//! kindnesses (a whole number where a float is wanted).

use crate::script::ast::{BinOp, Type};
use crate::script::numeric;
use crate::script::value::{clean_f32, cmp_values, eq_values, Value};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use core::cmp::Ordering;

fn symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
    }
}

fn mismatch(op: BinOp, a: &Value, b: &Value) -> String {
    format!("cannot use `{}` on {} and {}", symbol(op), a.type_name(), b.type_name())
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// `a op b`. `max_string` limits what `+` may make.
pub fn binary(op: BinOp, a: Value, b: Value, max_string: usize) -> Result<Value, String> {
    match op {
        BinOp::Eq => return Ok(Value::Bool(eq_values(&a, &b))),
        BinOp::Ne => return Ok(Value::Bool(!eq_values(&a, &b))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => return compare(op, &a, &b),
        _ => {}
    }
    let (a, b) = match (a, b) {
        (Value::Str(mut x), Value::Str(y)) if op == BinOp::Add => {
            if x.len() + y.len() > max_string {
                return Err(format!("this string would be longer than {} bytes", max_string));
            }
            Rc::make_mut(&mut x).push_str(&y);
            return Ok(Value::Str(x));
        }
        other => other,
    };
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => int_op(op, *x, *y),
        (Value::Float(_), Value::Float(_)) | (Value::Int(_), Value::Float(_)) | (Value::Float(_), Value::Int(_)) => {
            let (x, y) = (as_f64(&a).unwrap_or(0.0), as_f64(&b).unwrap_or(0.0));
            match op {
                BinOp::Add => Ok(Value::Float(x + y)),
                BinOp::Sub => Ok(Value::Float(x - y)),
                BinOp::Mul => Ok(Value::Float(x * y)),
                BinOp::Div => Ok(Value::Float(x / y)),
                BinOp::Rem => Ok(Value::Float(numeric::fmod(x, y))),
                _ => Err(mismatch(op, &a, &b)),
            }
        }
        (Value::Bool(x), Value::Bool(y)) => match op {
            BinOp::BitAnd => Ok(Value::Bool(*x & *y)),
            BinOp::BitOr => Ok(Value::Bool(*x | *y)),
            BinOp::BitXor => Ok(Value::Bool(*x ^ *y)),
            _ => Err(mismatch(op, &a, &b)),
        },
        _ => Err(mismatch(op, &a, &b)),
    }
}

fn int_op(op: BinOp, x: i64, y: i64) -> Result<Value, String> {
    let overflow = |what: &str| Err(format!("attempt to {} with overflow", what));
    match op {
        BinOp::Add => x.checked_add(y).map(Value::Int).map_or_else(|| overflow("add"), Ok),
        BinOp::Sub => x.checked_sub(y).map(Value::Int).map_or_else(|| overflow("subtract"), Ok),
        BinOp::Mul => x.checked_mul(y).map(Value::Int).map_or_else(|| overflow("multiply"), Ok),
        BinOp::Div => {
            if y == 0 {
                return Err(String::from("attempt to divide by zero"));
            }
            x.checked_div(y).map(Value::Int).map_or_else(|| overflow("divide"), Ok)
        }
        BinOp::Rem => {
            if y == 0 {
                return Err(String::from("attempt to calculate the remainder with a divisor of zero"));
            }
            x.checked_rem(y).map(Value::Int).map_or_else(|| overflow("calculate the remainder"), Ok)
        }
        BinOp::BitAnd => Ok(Value::Int(x & y)),
        BinOp::BitOr => Ok(Value::Int(x | y)),
        BinOp::BitXor => Ok(Value::Int(x ^ y)),
        BinOp::Shl | BinOp::Shr => {
            if !(0..64).contains(&y) {
                return Err(format!("attempt to shift by {}", y));
            }
            Ok(Value::Int(if op == BinOp::Shl { x.wrapping_shl(y as u32) } else { x >> y }))
        }
        _ => Err(String::from("internal error: not an arithmetic operator")),
    }
}

fn compare(op: BinOp, a: &Value, b: &Value) -> Result<Value, String> {
    // Floats compare as floats (NaN is neither smaller nor bigger).
    if matches!(a, Value::Float(_)) || matches!(b, Value::Float(_)) {
        if let (Some(x), Some(y)) = (as_f64(a), as_f64(b)) {
            return Ok(Value::Bool(match op {
                BinOp::Lt => x < y,
                BinOp::Le => x <= y,
                BinOp::Gt => x > y,
                _ => x >= y,
            }));
        }
    }
    match cmp_values(a, b) {
        Some(o) => Ok(Value::Bool(match op {
            BinOp::Lt => o == Ordering::Less,
            BinOp::Le => o != Ordering::Greater,
            BinOp::Gt => o == Ordering::Greater,
            _ => o != Ordering::Less,
        })),
        None => Err(format!("cannot compare {} with {}", a.type_name(), b.type_name())),
    }
}

/// `v as ty`.
pub fn cast(v: &Value, ty: &str) -> Result<Value, String> {
    let int = |i: i64| -> Value {
        Value::Int(match ty {
            "i8" => i as i8 as i64,
            "i16" => i as i16 as i64,
            "i32" => i as i32 as i64,
            "u8" => i as u8 as i64,
            "u16" => i as u16 as i64,
            "u32" => i as u32 as i64,
            _ => i,
        })
    };
    let from_float = |f: f64| -> Value {
        Value::Int(match ty {
            "i8" => f as i8 as i64,
            "i16" => f as i16 as i64,
            "i32" => f as i32 as i64,
            "u8" => f as u8 as i64,
            "u16" => f as u16 as i64,
            "u32" => f as u32 as i64,
            "u64" | "usize" | "u128" => f as u64 as i64,
            _ => f as i64,
        })
    };
    match ty {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => match v {
            Value::Int(i) => Ok(int(*i)),
            Value::Float(f) => Ok(from_float(*f)),
            Value::Bool(b) => Ok(Value::Int(*b as i64)),
            Value::Char(c) => Ok(int(*c as u32 as i64)),
            _ => Err(format!("cannot cast {} as {}", v.type_name(), ty)),
        },
        "f64" => match v {
            Value::Int(i) => Ok(Value::Float(*i as f64)),
            Value::Float(f) => Ok(Value::Float(*f)),
            _ => Err(format!("cannot cast {} as {}", v.type_name(), ty)),
        },
        "f32" => match v {
            Value::Int(i) => Ok(Value::Float(clean_f32(*i as f32))),
            Value::Float(f) => Ok(Value::Float(clean_f32(*f as f32))),
            _ => Err(format!("cannot cast {} as {}", v.type_name(), ty)),
        },
        "char" => match v {
            Value::Char(c) => Ok(Value::Char(*c)),
            Value::Int(i) if (0..=0x10FFFF).contains(i) => match char::from_u32(*i as u32) {
                Some(c) => Ok(Value::Char(c)),
                None => Err(format!("{} is not a character code", i)),
            },
            _ => Err(format!("cannot cast {} as char", v.type_name())),
        },
        "bool" => Err(String::from("cannot cast to bool (compare with `!= 0` instead)")),
        _ => Err(format!("cannot cast as `{}` (the script language casts to numbers and char)", ty)),
    }
}

/// A whole number given where the type says float becomes a float.
pub fn coerce(ty: &Type, v: Value) -> Value {
    if ty.is_float() {
        if let Value::Int(i) = v {
            return Value::Float(i as f64);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(op: BinOp, a: Value, b: Value) -> Result<Value, String> {
        binary(op, a, b, 1000)
    }

    #[test]
    fn integer_arithmetic_and_its_errors() {
        assert_eq!(run(BinOp::Add, Value::Int(2), Value::Int(3)), Ok(Value::Int(5)));
        assert_eq!(run(BinOp::Div, Value::Int(7), Value::Int(2)), Ok(Value::Int(3)));
        assert_eq!(run(BinOp::Rem, Value::Int(-7), Value::Int(3)), Ok(Value::Int(-1)));
        assert!(run(BinOp::Div, Value::Int(1), Value::Int(0)).unwrap_err().contains("divide by zero"));
        assert!(run(BinOp::Add, Value::Int(i64::MAX), Value::Int(1)).unwrap_err().contains("overflow"));
        assert_eq!(run(BinOp::Shl, Value::Int(1), Value::Int(4)), Ok(Value::Int(16)));
        assert!(run(BinOp::Shl, Value::Int(1), Value::Int(70)).is_err());
    }

    #[test]
    fn floats_and_mixed() {
        assert_eq!(run(BinOp::Mul, Value::Float(1.5), Value::Float(2.0)), Ok(Value::Float(3.0)));
        assert_eq!(run(BinOp::Add, Value::Int(1), Value::Float(0.5)), Ok(Value::Float(1.5)));
        assert_eq!(run(BinOp::Rem, Value::Float(5.5), Value::Float(2.0)), Ok(Value::Float(1.5)));
        assert_eq!(run(BinOp::Div, Value::Float(1.0), Value::Float(0.0)), Ok(Value::Float(f64::INFINITY)));
    }

    #[test]
    fn strings_and_comparison() {
        assert_eq!(run(BinOp::Add, Value::str("ab"), Value::str("cd")), Ok(Value::str("abcd")));
        assert!(binary(BinOp::Add, Value::str("abc"), Value::str("def"), 5).is_err());
        assert_eq!(run(BinOp::Lt, Value::Int(1), Value::Int(2)), Ok(Value::Bool(true)));
        assert_eq!(run(BinOp::Ge, Value::str("b"), Value::str("a")), Ok(Value::Bool(true)));
        assert_eq!(run(BinOp::Lt, Value::Float(f64::NAN), Value::Float(1.0)), Ok(Value::Bool(false)));
        assert_eq!(run(BinOp::Eq, Value::Int(2), Value::Float(2.0)), Ok(Value::Bool(true)));
        assert!(run(BinOp::Lt, Value::Int(1), Value::str("a")).is_err());
        assert!(run(BinOp::Add, Value::Int(1), Value::str("a")).unwrap_err().contains("integer and string"));
    }

    #[test]
    fn casts() {
        assert_eq!(cast(&Value::Float(3.9), "i32"), Ok(Value::Int(3)));
        assert_eq!(cast(&Value::Float(-1.5), "u8"), Ok(Value::Int(0)));
        assert_eq!(cast(&Value::Int(300), "u8"), Ok(Value::Int(44)));
        assert_eq!(cast(&Value::Int(-1), "u32"), Ok(Value::Int(4294967295)));
        assert_eq!(cast(&Value::Int(3), "f64"), Ok(Value::Float(3.0)));
        assert_eq!(cast(&Value::Char('a'), "u8"), Ok(Value::Int(97)));
        assert_eq!(cast(&Value::Int(98), "char"), Ok(Value::Char('b')));
        assert_eq!(cast(&Value::Bool(true), "i32"), Ok(Value::Int(1)));
        assert!(cast(&Value::str("x"), "i32").is_err());
    }

    #[test]
    fn whole_numbers_become_floats_where_floats_are_asked() {
        let f = Type { name: crate::script::ast::Name::from("f32"), args: alloc::vec::Vec::new() };
        let i = Type { name: crate::script::ast::Name::from("i32"), args: alloc::vec::Vec::new() };
        assert_eq!(coerce(&f, Value::Int(2)), Value::Float(2.0));
        assert_eq!(coerce(&i, Value::Int(2)), Value::Int(2));
    }
}
