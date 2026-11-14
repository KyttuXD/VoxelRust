//! The wear bar under the picture of an item that wears (hotbar and inventory
//! slots): a thin bar, green when new through yellow to red, with a dark
//! track. Shown only once the item has been used.

use crate::game::gear::wear;
use crate::inventory::Stack;
use crate::render::Renderer;

/// Draw the bar for `stack` in the slot at (`x`, `y`) of `size` pixels.
pub fn draw(r: &mut Renderer, x: f32, y: f32, size: f32, s: f32, stack: &Stack) {
    if !wear::shows_bar(stack) {
        return;
    }
    let max = wear::durability_of(stack.item);
    let frac = wear::fraction_left(stack, max);
    let (bx, bw) = (x + 2.0 * s, size - 4.0 * s);
    let by = y + size - 3.5 * s;
    r.rect(bx, by, bw, 1.5 * s, [10, 10, 14, 230]);
    let c = wear::bar_colour(frac);
    // At least a pixel of it, so a nearly worn item still shows a bar.
    r.rect(bx, by, (bw * frac).max(s), 1.5 * s, [c[0], c[1], c[2], 255]);
}
