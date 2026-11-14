//! `--script-check FILE`: read a script the way the game would and say
//! what is wrong with it, for people who write them.

use crate::script::error::ScriptError;
use crate::script::{Script, HOOKS};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// What a file turned out to hold.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub functions: Vec<String>,
    /// The hooks it defines.
    pub hooks: Vec<String>,
    pub warnings: Vec<String>,
    /// Calls to functions that do not exist, or with the wrong number of arguments (see `lint.rs`).
    pub mistakes: Vec<String>,
}

/// Read `src` as a script called `file`. `Err` is the first mistake in it.
pub fn check_source(file: &str, src: &str) -> Result<Report, ScriptError> {
    let script = Script::load(file, src)?;
    let mut r = Report::default();
    r.mistakes = crate::script::lint::lint(&script.prog);
    for (name, def) in &script.prog.fns {
        r.functions.push(String::from(&**name));
        match HOOKS.iter().find(|h| h.name == &**name) {
            Some(h) => {
                r.hooks.push(String::from(h.name));
                if def.params.len() > h.params.len() {
                    r.warnings.push(format!("{}:{}: `{}` takes {} parameter(s) but the game passes at most {} ({})", file, def.line, name, def.params.len(), h.params.len(), h.params.join(", ")));
                }
            }
            None => {
                if name.starts_with("on_") {
                    let known: Vec<&str> = HOOKS.iter().map(|h| h.name).collect();
                    r.warnings.push(format!("{}:{}: `{}` looks like a hook but is not one the game calls (hooks: {})", file, def.line, name, known.join(", ")));
                }
            }
        }
    }
    if r.hooks.is_empty() {
        r.warnings.push(format!("{}: this file defines no hook, so the game never runs anything from it", file));
    }
    Ok(r)
}

/// Check a file on disk and print the result. `Err` carries the message to
/// stop the program with.
pub fn check_file(path: &str) -> Result<(), String> {
    let Some(bytes) = crate::sys::read_file(path) else { return Err(format!("could not read {}", path)) };
    let Ok(src) = core::str::from_utf8(&bytes) else { return Err(format!("{} is not valid UTF-8 text", path)) };
    match check_source(path, src) {
        Ok(r) => {
            for w in &r.warnings {
                crate::sys::log(&format!("warning: {}", w));
            }
            for m in &r.mistakes {
                crate::sys::log(&format!("error: {}", m));
            }
            if !r.mistakes.is_empty() {
                return Err(format!("{} has mistakes in it", path));
            }
            crate::sys::log(&format!("{}: ok ({} function(s); hooks: {})", path, r.functions.len(), if r.hooks.is_empty() { String::from("none") } else { r.hooks.join(", ") }));
            Ok(())
        }
        Err(e) => {
            crate::sys::log(&format!("error: {}", e));
            Err(format!("{} has a mistake in it", path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_file_lists_its_hooks() {
        let r = check_source("t.rs", "fn on_tick(dt: f32) {}\nfn helper() {}\nfn on_chat(text: String) -> bool { true }").unwrap();
        assert_eq!(r.hooks.len(), 2);
        assert_eq!(r.functions.len(), 3);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn warnings_for_look_alikes_and_extra_parameters() {
        let r = check_source("t.rs", "fn on_blok_break() {}\nfn on_tick(a: f32, b: f32) {}").unwrap();
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("on_blok_break") && w.contains("looks like a hook")));
        assert!(r.warnings.iter().any(|w| w.contains("on_tick") && w.contains("at most 1")));
    }

    #[test]
    fn a_file_with_no_hooks_is_noted() {
        let r = check_source("t.rs", "fn helper() {}").unwrap();
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn a_mistake_is_an_error_with_a_line() {
        let e = check_source("t.rs", "fn on_tick() {\n let = 3;\n}").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
