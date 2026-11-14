//! Reading the data lines of grid recipes:
//!
//! ```text
//! craft   <item> <count> <ingredient> <count> [<ingredient> <count> ...]
//! shaped  <item> <count> <row> [<row> ...] with <letter> <ingredient> [<letter> <ingredient> ...]
//! ```
//!
//! An ingredient is an item id or `@group`, and may end in `*N` to need
//! N of it in its cell. A shaped recipe may be up to 7 x 7 cells; which
//! grid it needs (table 3 x 3, bench 5 x 5, workshop 7 x 7) follows from its
//! size once the empty edges are trimmed.

use super::recipe::{GridRecipe, Ingredient, MAX_SIDE, TABLE_SIDE};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Words of a data line, where a double-quoted piece is one word (it may
/// hold spaces) and the quotes are dropped.
pub fn split_words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_quote, mut started) = (false, false);
    for c in line.chars() {
        if in_quote {
            if c == '"' {
                out.push(core::mem::take(&mut cur));
                in_quote = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            if started {
                out.push(core::mem::take(&mut cur));
                started = false;
            }
            in_quote = true;
        } else if c.is_whitespace() {
            if started {
                out.push(core::mem::take(&mut cur));
                started = false;
            }
        } else {
            cur.push(c);
            started = true;
        }
    }
    if in_quote || started {
        out.push(cur);
    }
    out
}

/// How names become items.
pub struct Lookup<'a> {
    /// An item id to its index.
    pub item: &'a dyn Fn(&str) -> Option<u16>,
    /// A group name to the items in it.
    pub group: &'a dyn Fn(&str) -> Option<Vec<u16>>,
}

fn count(word: &str) -> Option<u16> {
    word.parse::<u16>().ok().filter(|n| (1..=999).contains(n))
}

fn ingredient(name: &str, lk: &Lookup) -> Result<Ingredient, String> {
    let (base, need) = match name.rsplit_once('*') {
        Some((b, n)) => (b, count(n).ok_or_else(|| format!("`{}`: the number after * must be 1 to 999", name))?),
        None => (name, 1),
    };
    let items = match base.strip_prefix('@') {
        Some(g) => (lk.group)(g).filter(|v| !v.is_empty()).ok_or_else(|| format!("no group called `{}`", g))?,
        None => alloc::vec![(lk.item)(base).ok_or_else(|| format!("unknown item `{}`", base))?],
    };
    Ok(Ingredient { items, need })
}

/// One `craft`, `recipe`, `shapeless` or `shaped` line, split into words
/// (see [`split_words`]) with the keyword first.
pub fn resolve(words: &[String], lk: &Lookup) -> Result<GridRecipe, String> {
    let key = words.first().map(|w| w.as_str()).unwrap_or("");
    let out_name = words.get(1).ok_or("needs an item to make")?.as_str();
    let out_item = (lk.item)(out_name).ok_or_else(|| format!("unknown item `{}`", out_name))?;
    let out_count = words.get(2).and_then(|n| count(n)).ok_or("the count made must be 1 to 999")?;
    let out = (out_item, out_count);
    let rest = &words[3..];
    if key == "shaped" {
        let with = rest.iter().position(|w| w == "with").ok_or("shaped needs `with <letter> <ingredient> ...` after its rows")?;
        let (rows, key_words) = (&rest[..with], &rest[with + 1..]);
        if rows.is_empty() {
            return Err(String::from("shaped needs at least one row"));
        }
        if key_words.is_empty() || key_words.len() % 2 != 0 {
            return Err(String::from("after `with` come pairs of `<letter> <ingredient>`"));
        }
        let mut letters: Vec<(char, Ingredient)> = Vec::new();
        for pair in key_words.chunks(2) {
            let mut chars = pair[0].chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c != '.' && c != ' ' => letters.push((c, ingredient(&pair[1], lk)?)),
                _ => return Err(format!("`{}` is not a single letter", pair[0])),
            }
        }
        let width = rows[0].chars().count();
        if width == 0 || rows.iter().any(|r| r.chars().count() != width) {
            return Err(String::from("the rows of a shaped recipe must all be as wide as each other"));
        }
        let mut cells = Vec::new();
        for row in rows {
            for c in row.chars() {
                if c == '.' || c == ' ' {
                    cells.push(None);
                } else {
                    match letters.iter().rev().find(|l| l.0 == c) {
                        Some(l) => cells.push(Some(l.1.clone())),
                        None => return Err(format!("the letter `{}` is not given after `with`", c)),
                    }
                }
            }
        }
        return GridRecipe::shaped(out, width, rows.len(), cells).ok_or_else(|| format!("a shaped recipe must be 1 to {0} by 1 to {0} cells, with something in it", MAX_SIDE));
    }
    // Shapeless: pairs of `<ingredient> <count>`.
    if rest.is_empty() || rest.len() % 2 != 0 {
        return Err(String::from("needs `<ingredient> <count>` pairs"));
    }
    let mut parts: Vec<(Ingredient, u16)> = Vec::new();
    for pair in rest.chunks(2) {
        let n = count(&pair[1]).ok_or_else(|| format!("`{}`: the count must be 1 to 999", pair[1]))?;
        parts.push((ingredient(&pair[0], lk)?, n));
    }
    let units: u32 = parts.iter().map(|p| p.1 as u32).sum();
    let mut list = Vec::new();
    for (ing, n) in parts {
        if units <= (TABLE_SIDE * TABLE_SIDE) as u32 {
            // Every unit has a cell of its own.
            for _ in 0..n {
                list.push(ing.clone());
            }
        } else {
            // Too many for the grid: one cell for the whole amount.
            let need = ing.need as u32 * n as u32;
            if need > 999 {
                return Err(String::from("an ingredient needs more than 999 of one item"));
            }
            list.push(Ingredient { items: ing.items, need: need as u16 });
        }
    }
    GridRecipe::shapeless(out, list).ok_or_else(|| String::from("a recipe takes at most 9 different ingredients"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::crafting::recipe::Pattern;

    const IDS: [&str; 6] = ["iron_pick", "iron_ingot", "stick", "planks", "pine_planks", "plane"];

    fn lookup<R>(f: impl FnOnce(&Lookup) -> R) -> R {
        let item = |id: &str| IDS.iter().position(|i| *i == id).map(|i| i as u16);
        let group = |g: &str| if g == "planks" { Some(alloc::vec![3u16, 4]) } else { None };
        f(&Lookup { item: &item, group: &group })
    }

    fn parse(line: &str) -> Result<GridRecipe, String> {
        lookup(|lk| resolve(&split_words(line), lk))
    }

    #[test]
    fn quoted_rows_may_hold_spaces() {
        assert_eq!(split_words("shaped x 1 \"III\" \" S \" with I y"), ["shaped", "x", "1", "III", " S ", "with", "I", "y"]);
        assert_eq!(split_words("  a   b "), ["a", "b"]);
        assert_eq!(split_words("a \"b c"), ["a", "b c"]);
    }

    #[test]
    fn a_shaped_line_reads_with_quotes_or_dots() {
        let a = parse("shaped iron_pick 1 \"III\" \" S \" \" S \" with I iron_ingot S stick").unwrap();
        let b = parse("shaped iron_pick 1 III .S. .S. with I iron_ingot S stick").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.out, (0, 1));
        match &a.pattern {
            Pattern::Shaped { w, h, cells } => {
                assert_eq!((*w, *h), (3, 3));
                assert_eq!(cells[0], Some(Ingredient::one(1)));
                assert_eq!(cells[3], None);
                assert_eq!(cells[4], Some(Ingredient::one(2)));
            }
            _ => panic!("not shaped"),
        }
        assert!(a.needs_table());
    }

    #[test]
    fn empty_edges_are_trimmed_and_groups_and_counts_read() {
        let r = parse("shaped stick 4 .P. .P. ... with P @planks").unwrap();
        match &r.pattern {
            Pattern::Shaped { w, h, cells } => {
                assert_eq!((*w, *h), (1, 2));
                assert_eq!(cells[0].as_ref().unwrap().items, [3, 4]);
            }
            _ => panic!(),
        }
        assert!(!r.needs_table());
        let r = parse("shaped plane 1 S with S stick*5").unwrap();
        assert_eq!(r.ingredients()[0].need, 5);
    }

    #[test]
    fn bad_shaped_lines_are_refused() {
        assert!(parse("shaped iron_pick 1 III .S. with I iron_ingot").is_err()); // S not defined
        assert!(parse("shaped iron_pick 1 III .S with I iron_ingot S stick").is_err()); // ragged
        assert!(parse("shaped iron_pick 1 IIIIIIII with I iron_ingot").is_err()); // 8 wide
        assert!(parse("shaped iron_pick 1 ... with I iron_ingot").is_err()); // nothing in it
        assert!(parse("shaped iron_pick 1 I I I I I I I I with I iron_ingot").is_err()); // 8 tall
        assert!(parse("shaped iron_pick 1 I with I nothing_real").is_err());
        assert!(parse("shaped iron_pick 1 I with I @nothing").is_err());
        assert!(parse("shaped iron_pick 0 I with I stick").is_err());
        assert!(parse("shaped iron_pick 1 I").is_err()); // no `with`
        assert!(parse("shaped iron_pick 1 I with II stick").is_err()); // not a letter
    }

    #[test]
    fn big_shaped_lines_need_a_bench_or_a_workshop() {
        let four = parse("shaped plane 1 IIII with I iron_ingot").unwrap();
        assert_eq!(four.min_side(), 5);
        let five = parse("shaped plane 1 I.... .I... ..I.. ...I. ....I with I iron_ingot").unwrap();
        assert_eq!(five.min_side(), 5);
        let seven = parse("shaped plane 1 IIIIIII ....... with I iron_ingot").unwrap();
        assert_eq!(seven.min_side(), 7);
        let tall = parse("shaped plane 1 I I I I I I with I iron_ingot").unwrap();
        assert_eq!((tall.min_side(), tall.needs_table()), (7, true));
        assert_eq!(parse("shaped plane 1 I with I iron_ingot").unwrap().min_side(), 2);
        assert_eq!(parse("shaped iron_pick 1 III .S. .S. with I iron_ingot S stick").unwrap().min_side(), 3);
        assert_eq!(parse("craft plane 1 stick 5").unwrap().min_side(), 3);
        assert_eq!(parse("craft plane 1 stick 10").unwrap().min_side(), 2);
    }

    #[test]
    fn craft_lines_put_each_unit_in_its_own_cell() {
        let r = parse("craft iron_pick 1 stick 2 iron_ingot 3").unwrap();
        match &r.pattern {
            Pattern::Shapeless(list) => {
                assert_eq!(list.len(), 5);
                assert!(list.iter().all(|i| i.need == 1));
            }
            _ => panic!(),
        }
        assert!(r.needs_table());
        assert!(!parse("craft stick 4 planks 1").unwrap().needs_table());
        assert!(parse("craft stick 4 planks 4").unwrap().fits(2, 2));
        assert!(!parse("craft stick 4 planks 5").unwrap().fits(2, 2));
    }

    #[test]
    fn craft_lines_over_nine_units_become_one_cell_per_ingredient() {
        let r = parse("craft plane 1 iron_ingot 10 planks 8 stick 4").unwrap();
        match &r.pattern {
            Pattern::Shapeless(list) => {
                let needs: Vec<u16> = list.iter().map(|i| i.need).collect();
                assert_eq!(needs, [10, 8, 4]);
            }
            _ => panic!(),
        }
        // Nine units still get a cell each.
        assert_eq!(parse("craft plane 1 stick 9").unwrap().cell_count(), 9);
        assert_eq!(parse("craft plane 1 stick 10").unwrap().cell_count(), 1);
        assert!(parse("craft plane 1 stick 1000").is_err());
        assert!(parse("craft plane 1 stick").is_err());
        assert!(parse("craft plane 1 nothing 1").is_err());
        assert!(parse("craft nothing 1 stick 1").is_err());
    }
}
