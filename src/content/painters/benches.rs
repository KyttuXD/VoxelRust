//! The two bigger crafting blocks, in the OCD style: the crafting bench's
//! top (a frame round a 5 x 5 grid of cells), the workshop's top (a 7 x 7
//! grid of single-pixel cells) and the sides they share, a plate with
//! rails and rivets. The crafting table's own top (3 x 3) is `workbench`
//! in `blocks.rs`. Flat fills of three tones of the colour given.

use super::*;

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    let (l, m, d) = (pal.light_of(rgb), rgb, pal.dark_of(rgb));
    Some(match style {
        // 14 pixels inside the frame: five cells of two with a line of one
        // between them (lines at 3, 6, 9 and 12). The middle cell is lit.
        "bench5" => {
            if edge(x, y) || matches!(x, 3 | 6 | 9 | 12) || matches!(y, 3 | 6 | 9 | 12) {
                px(d)
            } else if rect(x, y, 7, 7, 2, 2) {
                px(l)
            } else {
                px(m)
            }
        }
        // Seven cells of one pixel with a line between them (lines at the
        // even places 2 to 14). The middle cell is lit.
        "bench7" => {
            if edge(x, y) || (x % 2 == 0) || (y % 2 == 0) {
                px(d)
            } else if x == 7 && y == 7 {
                px(l)
            } else {
                px(m)
            }
        }
        // A side: a plate with a rail along the top and the bottom and a
        // rivet in each corner between them.
        "bench_side" => {
            if edge(x, y) || y == 3 || y == 12 {
                px(d)
            } else if (x == 2 || x == 13) && (y == 1 || y == 14) {
                px(l)
            } else if y < 3 || y > 12 {
                px(m)
            } else if (x == 3 || x == 12) && (y == 5 || y == 10) {
                px(l)
            } else {
                px(m)
            }
        }
        _ => return None,
    })
}
