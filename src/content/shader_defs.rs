//! The shaders of the content packs: `shader <id> <file> [<file2> [<file3>]]`,
//! kept in the registry as a [`ShaderTable`] (see docs/SHADERS.md).
//!
//! ```text
//! shader soft_bloom shaders/soft_bloom.glsl
//! shader night_vision shaders/blur.glsl shaders/green.glsl
//! ```
//!
//! Each file is the body of one full-screen fragment shader (a pass); the
//! files of one line run one after another. The text is read here, when the
//! pack is read; compiling it is the renderer's business, and only happens
//! if the player picks the shader.

use crate::client::shaders::{names, plan, source};
use alloc::string::String;
use alloc::vec::Vec;

/// Most shaders all packs together may register.
pub const MAX_SHADERS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderDef {
    pub id: String,
    /// The fragment shader text of each pass, in order (1 to 3).
    pub passes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ShaderTable {
    pub list: Vec<ShaderDef>,
}

impl ShaderTable {
    pub fn find(&self, id: &str) -> Option<&ShaderDef> {
        self.list.iter().find(|s| s.id == id)
    }
}

/// Read one `shader` line (`args` are the words after the keyword). `load`
/// reads a file named on the line (relative to the pack). A shader with the
/// id of an earlier one replaces it. The error is for the log; nothing is
/// registered then.
pub fn read_line(table: &mut ShaderTable, args: &[&str], load: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<(), String> {
    const USAGE: &str = "shader needs: <id> <file> [<file2> [<file3>]]";
    let Some((id, files)) = args.split_first() else { return Err(String::from(USAGE)) };
    if !names::valid_id(id) {
        return Err(alloc::format!("`{}` is not a shader id (letters, digits, _ and -, up to {} long)", id, names::MAX_ID));
    }
    if files.is_empty() {
        return Err(String::from(USAGE));
    }
    if files.len() > plan::MAX_PASSES {
        return Err(alloc::format!("a shader has at most {} files (passes)", plan::MAX_PASSES));
    }
    let mut passes = Vec::new();
    for name in files {
        let bytes = load(name).ok_or_else(|| alloc::format!("cannot read the shader file {}", name))?;
        let text = String::from_utf8(bytes).map_err(|_| alloc::format!("{} is not text", name))?;
        source::check(&text).map_err(|m| alloc::format!("{}: {}", name, m))?;
        passes.push(text);
    }
    let def = ShaderDef { id: String::from(*id), passes };
    match table.list.iter().position(|s| s.id == def.id) {
        Some(i) => table.list[i] = def,
        None if table.list.len() >= MAX_SHADERS => return Err(alloc::format!("more than {} shaders", MAX_SHADERS)),
        None => table.list.push(def),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files<'a>(list: &'a [(&'a str, &'a str)]) -> impl FnMut(&str) -> Option<Vec<u8>> + 'a {
        move |name| list.iter().find(|f| f.0 == name).map(|f| f.1.as_bytes().to_vec())
    }

    const BODY: &str = "void main() { gl_FragColor = scene(v_uv); }";

    #[test]
    fn a_line_with_one_to_three_files_registers_a_shader() {
        let mut t = ShaderTable::default();
        let mut load = files(&[("a.glsl", BODY), ("b.glsl", BODY), ("c.glsl", BODY), ("d.glsl", BODY)]);
        assert!(read_line(&mut t, &["one", "a.glsl"], &mut load).is_ok());
        assert!(read_line(&mut t, &["two", "a.glsl", "b.glsl"], &mut load).is_ok());
        assert!(read_line(&mut t, &["three", "a.glsl", "b.glsl", "c.glsl"], &mut load).is_ok());
        assert_eq!(t.list.iter().map(|s| (s.id.as_str(), s.passes.len())).collect::<Vec<_>>(), [("one", 1), ("two", 2), ("three", 3)]);
        assert_eq!(t.find("two").unwrap().passes[1], BODY);
        assert!(t.find("four").is_none());
        // Four passes are too many, and nothing is added.
        assert!(read_line(&mut t, &["four", "a.glsl", "b.glsl", "c.glsl", "d.glsl"], &mut load).unwrap_err().contains("at most 3"));
        assert_eq!(t.list.len(), 3);
    }

    #[test]
    fn a_later_line_with_the_same_id_replaces_the_shader() {
        let mut t = ShaderTable::default();
        let mut load = files(&[("a.glsl", BODY), ("b.glsl", "void main() { gl_FragColor = vec4(1.0); }")]);
        read_line(&mut t, &["x", "a.glsl"], &mut load).unwrap();
        read_line(&mut t, &["x", "b.glsl"], &mut load).unwrap();
        assert_eq!(t.list.len(), 1);
        assert!(t.list[0].passes[0].contains("vec4(1.0)"));
    }

    #[test]
    fn bad_lines_are_refused_with_a_reason() {
        let mut t = ShaderTable::default();
        let mut load = files(&[("a.glsl", BODY), ("v.glsl", "#version 100\nvoid main(){}"), ("e.glsl", "  ")]);
        assert!(read_line(&mut t, &[], &mut load).unwrap_err().contains("shader needs"));
        assert!(read_line(&mut t, &["only_an_id"], &mut load).unwrap_err().contains("shader needs"));
        assert!(read_line(&mut t, &["bad id!", "a.glsl"], &mut load).unwrap_err().contains("not a shader id"));
        assert!(read_line(&mut t, &["x", "missing.glsl"], &mut load).unwrap_err().contains("cannot read"));
        assert!(read_line(&mut t, &["x", "v.glsl"], &mut load).unwrap_err().contains("#version"));
        assert!(read_line(&mut t, &["x", "e.glsl"], &mut load).unwrap_err().contains("empty"));
        let mut binary = |_: &str| Some(alloc::vec![0xFFu8, 0xFE, 0x00]);
        assert!(read_line(&mut t, &["x", "a.glsl"], &mut binary).unwrap_err().contains("not text"));
        assert!(t.list.is_empty());
    }

    #[test]
    fn the_table_has_a_limit() {
        let mut t = ShaderTable::default();
        let mut load = files(&[("a.glsl", BODY)]);
        for i in 0..MAX_SHADERS {
            read_line(&mut t, &[&alloc::format!("s{}", i), "a.glsl"], &mut load).unwrap();
        }
        assert!(read_line(&mut t, &["one_too_many", "a.glsl"], &mut load).unwrap_err().contains("more than"));
        // A replacement is still fine when the table is full.
        assert!(read_line(&mut t, &["s0", "a.glsl"], &mut load).is_ok());
        assert_eq!(t.list.len(), MAX_SHADERS);
    }
}
