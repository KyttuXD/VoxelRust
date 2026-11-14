//! The crafting part of the inventory screen and of the crafting table's:
//! the grid (2 x 2 by hand, 3 x 3 at a table), the result with an arrow
//! to it, and the recipe book, which lists what the inventory can make and
//! fills the grid when one is clicked.
//!
//! The rules (what makes what, what a click takes) are in
//! `game/crafting`; this file only draws and passes clicks on.

use crate::block::reg;
use crate::client::app::{draw_item, draw_slot};
use crate::client::ui::{self, Ui};
use crate::crafting::{self, CraftGrid};
use crate::inventory::Inventory;
use crate::render::{Color, WHITE};
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// What the screen remembers between frames.
#[derive(Default)]
pub struct CraftScreen {
    /// What lies in the grid. Given back to the inventory when the screen
    /// is left.
    pub grid: CraftGrid,
    /// The recipe book is showing instead of the grid.
    pub book: bool,
    /// The book lists every recipe, not only those that can be made now.
    pub show_all: bool,
    /// How far the book is scrolled, in rows.
    pub scroll: i32,
}

impl CraftScreen {
    /// The screen is closing: the grid's contents go back to the inventory.
    pub fn put_away(&mut self, inv: &mut Inventory) {
        self.grid.return_all(inv);
        self.book = false;
        self.scroll = 0;
    }
}

/// Where and how the crafting part is drawn: the upper part of the
/// inventory panel, `rows` cells high and `width` wide.
pub struct Frame {
    pub gx: f32,
    pub uy: f32,
    pub width: f32,
    pub cell: f32,
    pub rows: usize,
    /// At a crafting table (3 x 3), not in the inventory screen (2 x 2).
    pub at_table: bool,
    /// Shift is held.
    pub shift: bool,
}

const DIMMED: Color = [120, 120, 120, 200];
const GHOST: Color = [255, 255, 255, 70];

/// Draw the crafting part and act on clicks. Returns the name to show at
/// the top of the screen for what is pointed at.
pub fn draw(ui: &mut Ui, g: &mut Session, st: &mut CraftScreen, f: &Frame) -> Option<String> {
    let s = ui.s;
    let side = if f.at_table { 3 } else { 2 };
    let bw = 40.0 * s;
    if ui.button(f.gx + f.width - bw - 2.0 * s, f.uy + 2.0 * s, bw, 12.0 * s, if st.book { "GRID" } else { "BOOK" }) {
        st.book = !st.book;
        st.scroll = 0;
        return None;
    }
    if st.book {
        book(ui, g, st, f, side)
    } else {
        grid(ui, g, st, f, side)
    }
}

fn name_of(item: usize) -> String {
    reg().items[item].name.clone()
}

fn grid(ui: &mut Ui, g: &mut Session, st: &mut CraftScreen, f: &Frame, side: usize) -> Option<String> {
    let s = ui.s;
    let recipes = &reg().grid_recipes[..];
    let (click_l, click_r) = (ui.inp.clicked[1], ui.inp.clicked[3]);
    let cell = f.cell;
    let mut tip: Option<String> = None;
    let gw = side as f32 * cell;
    let x0 = f.gx + 10.0 * s;
    let y0 = f.uy + (f.rows as f32 * cell - gw) * 0.5 - 4.0 * s;
    for row in 0..side {
        for col in 0..side {
            let i = row * side + col;
            let (x, y) = (x0 + col as f32 * cell, y0 + row as f32 * cell);
            let hot = ui.hot(x, y, cell, cell);
            draw_slot(ui.r, x, y, cell, s, st.grid.cells[i], hot);
            match (st.grid.cells[i], st.grid.ghost[i]) {
                (Some(stack), _) if hot => tip = Some(name_of(stack.item as usize)),
                (None, Some(item)) => {
                    draw_item(ui.r, x + 3.0 * s, y + 3.0 * s, cell - 6.0 * s, item as usize, GHOST);
                    if hot {
                        tip = Some(name_of(item as usize));
                    }
                }
                _ => {}
            }
            if hot && (click_l || click_r) {
                if f.shift && click_l {
                    st.grid.send_back(i, &mut g.inv);
                } else {
                    st.grid.click_cell(i, &mut g.cursor, click_r);
                }
                ui.clicked_any = true;
            }
        }
    }
    // Everything in the grid back to the inventory.
    if ui.button(x0, y0 + gw + 3.0 * s, gw, 11.0 * s, "CLEAR") {
        st.grid.return_all(&mut g.inv);
    }
    // An arrow, and the result.
    let ym = y0 + gw * 0.5;
    let ax = x0 + gw + 6.0 * s;
    ui.r.rect(ax, ym - s, 12.0 * s, 2.0 * s, ui::DIM);
    ui.r.rect(ax + 10.0 * s, ym - 3.0 * s, s, 6.0 * s, ui::DIM);
    ui.r.rect(ax + 11.0 * s, ym - 2.0 * s, s, 4.0 * s, ui::DIM);
    ui.r.rect(ax + 12.0 * s, ym - s, s, 2.0 * s, ui::DIM);
    let oc = cell + 4.0 * s;
    let (ox, oy) = (ax + 20.0 * s, ym - oc * 0.5);
    let out = st.grid.output(recipes, side);
    let hot = ui.hot(ox, oy, oc, oc);
    draw_slot(ui.r, ox, oy, oc, s, out.map(|o| o.1), hot);
    if let Some((_, made)) = out {
        ui.r.frame(ox, oy, oc, oc, s, ui::ACCENT);
        if hot {
            tip = Some(alloc::format!("{} {}", made.count, name_of(made.item as usize)));
            if click_l || click_r {
                if f.shift {
                    if st.grid.take_all(recipes, side, &mut g.inv) == 0 {
                        g.say("INVENTORY FULL");
                    }
                } else {
                    st.grid.take(recipes, side, &mut g.cursor);
                }
                ui.clicked_any = true;
            }
        }
    }
    ui.text(ox - 6.0 * s, ym + oc * 0.5 + 4.0 * s, "SHIFT-CLICK", ui::DIM);
    ui.text(ox - 6.0 * s, ym + oc * 0.5 + 12.0 * s, "MAKES ALL", ui::DIM);
    tip
}

fn book(ui: &mut Ui, g: &mut Session, st: &mut CraftScreen, f: &Frame, side: usize) -> Option<String> {
    let s = ui.s;
    let r = reg();
    let recipes = &r.grid_recipes[..];
    let click_l = ui.inp.clicked[1];
    let cell = f.cell;
    let mut tip: Option<String> = None;
    if ui.button(f.gx + 2.0 * s, f.uy + 2.0 * s, 64.0 * s, 12.0 * s, if st.show_all { "ALL" } else { "CAN MAKE" }) {
        st.show_all = !st.show_all;
        st.scroll = 0;
        return None;
    }
    // What fits this grid and, unless all are wanted, what the inventory
    // and the grid together have the ingredients for; makeable ones first.
    let have = crafting::available(&g.inv, &st.grid);
    let mut list: Vec<(usize, bool)> = crafting::fitting(recipes, side)
        .into_iter()
        .map(|i| (i, crafting::can_make(&recipes[i], &have)))
        .filter(|&(_, ok)| st.show_all || ok)
        .collect();
    list.sort_by_key(|&(_, ok)| !ok);
    let per_row = 2usize;
    let rows = f.rows.saturating_sub(1).max(1);
    let total_rows = (list.len() + per_row - 1) / per_row;
    let max_scroll = total_rows.saturating_sub(rows) as i32;
    st.scroll = (st.scroll - ui.inp.wheel).clamp(0, max_scroll);
    if list.is_empty() {
        ui.text(f.gx + 4.0 * s, f.uy + cell + 4.0 * s, "NOTHING IN THE INVENTORY", ui::DIM);
        ui.text(f.gx + 4.0 * s, f.uy + cell + 14.0 * s, "MAKES ANYTHING YET", ui::DIM);
    }
    let rw = (f.width - 6.0 * s) / per_row as f32;
    for row in 0..rows {
        for col in 0..per_row {
            let idx = (row + st.scroll as usize) * per_row + col;
            let Some(&(i, ok)) = list.get(idx) else { continue };
            let rc = &recipes[i];
            let (x, y) = (f.gx + col as f32 * rw, f.uy + cell + row as f32 * cell);
            let hot = ui.hot(x, y, rw - s, cell - s);
            ui.r.rect(x, y, rw - s, cell - s, if !ok { ui::BUTTON_OFF } else if hot { ui::BUTTON_HOT } else { ui::BUTTON });
            let tint: Color = if ok { WHITE } else { DIMMED };
            draw_item(ui.r, x + 2.0 * s, y + 2.0 * s, cell - 5.0 * s, rc.out.0 as usize, tint);
            if rc.out.1 > 1 {
                ui.r.label(x + cell - 2.0 * s, y + 6.0 * s, s, &alloc::format!("{}", rc.out.1), tint);
            }
            ui.r.label(x + cell + 7.0 * s, y + 6.0 * s, s, "=", ui::DIM);
            let totals = rc.totals();
            for (k, &(item, n)) in totals.iter().take(3).enumerate() {
                let ix = x + cell + 15.0 * s + k as f32 * 13.0 * s;
                draw_item(ui.r, ix, y + 4.0 * s, 9.0 * s, item as usize, tint);
                if n > 1 {
                    ui.r.label(ix + 5.0 * s, y + 10.0 * s, s, &alloc::format!("{}", n), tint);
                }
            }
            if hot {
                let needs: Vec<String> = totals.iter().map(|&(i, n)| alloc::format!("{} {}", n, name_of(i as usize))).collect();
                tip = Some(alloc::format!("{} {} = {}", rc.out.1, name_of(rc.out.0 as usize), needs.join(" + ")));
                if click_l {
                    st.grid.fill(rc, side, &mut g.inv);
                    st.book = false;
                    st.scroll = 0;
                    ui.clicked_any = true;
                }
            }
        }
    }
    ui.scrollbar(f.gx + f.width + 1.5 * s, f.uy + cell, 3.0 * s, rows as f32 * cell, &mut st.scroll, rows, total_rows);
    tip
}
