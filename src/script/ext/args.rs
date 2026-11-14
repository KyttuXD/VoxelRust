//! Reading the arguments of the host functions in this folder.

pub(crate) use crate::script::host::{int, num, text, want};
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;

/// A `bool` argument.
pub fn flag(name: &str, args: &[Value], i: usize) -> Result<bool, String> {
    match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("`{}`: argument {} should be true or false, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// A number that must be finite (a position, an angle).
pub fn finite(name: &str, args: &[Value], i: usize) -> Result<f64, String> {
    let v = num(name, args, i)?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("`{}`: argument {} is not a finite number", name, i + 1))
    }
}

/// As far from the origin as a block may be (the edge `set_block` keeps).
pub const EDGE: i64 = 30_000_000;

/// A coordinate in blocks, kept inside the world's edge.
pub fn coord(name: &str, args: &[Value], i: usize) -> Result<i32, String> {
    let v = int(name, args, i)?;
    Ok(v.clamp(-EDGE, EDGE) as i32)
}

/// A tuple of three floats.
pub fn xyz(x: f64, y: f64, z: f64) -> Value {
    Value::tuple(alloc::vec![Value::Float(x), Value::Float(y), Value::Float(z)])
}

/// A whole number argument that may be an id of something (`-1` for none is the caller's business).
pub fn id(name: &str, args: &[Value], i: usize) -> Result<i64, String> {
    int(name, args, i)
}
