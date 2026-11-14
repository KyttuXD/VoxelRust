//! Reading what the packs say. A pack can keep top-level lines and indented
//! sections the game itself does not know, for a script to read: the pack
//! says which with `claims line <word>` or `claims section <word>` (see
//! docs/MODDING.md, "Lines and sections for scripts"). The scripts read them
//! here, from any mod's pack.

use super::args::{text, want};
use super::Ctx;
use crate::block;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("pack_lines(key: &str) -> Vec<Vec<String>>", "the words after each claimed top-level line that starts with `key`, in the order the packs were read (at most 500)"),
    ("pack_sections(kind: &str) -> Vec<String>", "the ids of the claimed sections of a kind (`vehicle`...), in the order the packs were read (at most 500)"),
    ("pack_prop(kind: &str, id: &str, key: &str, default: &str) -> String", "the words after the line `key` in a claimed section, joined with spaces, or `default` if the section or the line is not there"),
    ("pack_props(kind: &str, id: &str) -> Vec<(String, String)>", "every line of a claimed section as (key, words joined with spaces)"),
];

const MOST: usize = 500;

pub fn call(_g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    let r = block::reg();
    match name {
        "pack_lines" => {
            want(name, args, 1)?;
            let key = text(name, args, 0)?;
            let rows: Vec<Value> = r.lines.iter().filter(|l| l.0 == key).take(MOST).map(|l| Value::vec(l.2.iter().map(|w| Value::str(w)).collect())).collect();
            Ok(Value::vec(rows))
        }
        "pack_sections" => {
            want(name, args, 1)?;
            let kind = text(name, args, 0)?;
            Ok(Value::vec(r.sections.iter().filter(|s| s.kind == kind).take(MOST).map(|s| Value::str(&s.id)).collect()))
        }
        "pack_prop" => {
            want(name, args, 4)?;
            let (kind, id, key, default) = (text(name, args, 0)?, text(name, args, 1)?, text(name, args, 2)?, text(name, args, 3)?);
            let found = r.sections.iter().find(|s| s.kind == kind && s.id == id).and_then(|s| s.get(key).map(|w| w.join(" ")));
            Ok(Value::string(found.unwrap_or_else(|| String::from(default))))
        }
        "pack_props" => {
            want(name, args, 2)?;
            let (kind, id) = (text(name, args, 0)?, text(name, args, 1)?);
            let rows: Vec<Value> = match r.sections.iter().find(|s| s.kind == kind && s.id == id) {
                Some(s) => s.props.iter().take(MOST).map(|p| Value::tuple(alloc::vec![Value::str(&p.0), Value::string(p.1.join(" "))])).collect(),
                None => Vec::new(),
            };
            Ok(Value::vec(rows))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
