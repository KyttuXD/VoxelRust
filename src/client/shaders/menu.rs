//! The SHADER row of SETTINGS > VIDEO: OFF, or a stepper over the loaded shaders.

use crate::client::ui::Ui;
use alloc::string::String;
use alloc::vec::Vec;

/// The choices: none (empty), then every loaded shader, then the one the
/// settings name if no pack has it now (so it can still be seen and left).
pub fn options(loaded: &[&str], current: &str) -> Vec<String> {
    let mut v: Vec<String> = alloc::vec![String::new()];
    v.extend(loaded.iter().map(|s| String::from(*s)));
    if !current.is_empty() && !loaded.contains(&current) {
        v.push(String::from(current));
    }
    v
}

/// The text on the row: `SHADER  < SOFT BLOOM >`. `problem` (what is wrong
/// with the chosen shader, if anything) is added in brackets.
pub fn label(id: &str, problem: &str, supported: bool) -> String {
    if !supported {
        return String::from("SHADER  < NOT SUPPORTED HERE >");
    }
    if id.is_empty() {
        return String::from("SHADER  < OFF >");
    }
    let name: String = id.chars().map(|c| if c == '_' || c == '-' { ' ' } else { c.to_ascii_uppercase() }).collect();
    if problem.is_empty() {
        alloc::format!("SHADER  < {} >", name)
    } else {
        alloc::format!("SHADER  < {} ({}) >", name, problem)
    }
}

/// Draw the row and apply a click to `current`. Returns whether it changed.
pub fn row(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, current: &mut String) -> bool {
    let supported = ui.r.post.supported();
    let problem = if *current == ui.r.post.wanted { ui.r.post.error.clone() } else { String::new() };
    let text = label(current, &problem, supported);
    if !supported {
        ui.button_lr(x, y, w, h, &text, false);
        return false;
    }
    let loaded: Vec<&str> = crate::block::reg().shaders.list.iter().map(|s| s.id.as_str()).collect();
    let list = options(&loaded, current);
    let mut i = list.iter().position(|s| *s == *current).unwrap_or(0) as i32;
    let changed = ui.stepper_text(x, y, w, h, &text, &mut i, 0, list.len() as i32 - 1, 1);
    if changed {
        *current = list[i as usize].clone();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_start_with_off_and_keep_a_missing_shader() {
        assert_eq!(options(&[], ""), [""]);
        assert_eq!(options(&["a", "b"], "b"), ["", "a", "b"]);
        assert_eq!(options(&["a"], "gone"), ["", "a", "gone"]);
    }

    #[test]
    fn the_label_reads_well() {
        assert_eq!(label("", "", true), "SHADER  < OFF >");
        assert_eq!(label("soft_bloom", "", true), "SHADER  < SOFT BLOOM >");
        assert_eq!(label("retro-dither", "FAILED", true), "SHADER  < RETRO DITHER (FAILED) >");
        assert!(label("x", "", false).contains("NOT SUPPORTED"));
    }
}
