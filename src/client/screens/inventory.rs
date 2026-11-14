//! The inventory and chest screens (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn screen_inventory(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 150]);
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let r = reg();
        let cell = 20.0 * s;
        let cols = 9usize;
        let upper_rows = 5usize;
        let pw = cols as f32 * cell + 12.0 * s;
        // The creative list has a search field under its tabs.
        let search_h = if g.mode == GameMode::Creative && self.screen == Screen::Inventory { 14.0 * s } else { 0.0 };
        let ph = 32.0 * s + search_h + upper_rows as f32 * cell + 16.0 * s + 4.0 * cell + 14.0 * s;
        let (px, py) = ((ui.w() - pw) * 0.5, ((ui.h() - ph) * 0.5).max(12.0 * s));
        ui.panel(px, py, pw, ph);
        let gx = px + 6.0 * s;
        let mut tip: Option<String> = None;
        let (click_l, click_r) = (ui.inp.clicked[1], ui.inp.clicked[3]);

        // Upper part: every item (creative) or what can be made (survival).
        let uy = py + 32.0 * s + search_h;
        let at_table = self.screen == Screen::Craft;
        if self.screen == Screen::Chest {
            // The chest's own 27 slots.
            // A chest's 27 slots, or storage a mod's own code keeps.
            let kept = self.chest_ext.and_then(|(name, which)| {
                if name == crate::script::ext::STORAGE_KEY {
                    return crate::script::ext::storage::pointer(g, which);
                }
                let storage = crate::native::active().find(|(_, n)| n.name == name).and_then(|(_, n)| n.storage)?;
                storage(g, which)
            });
            ui.text(gx, py + 6.0 * s, if kept.is_some() { "CARGO" } else { "CHEST" }, ui::DIM);
            let chest = match kept {
                // SAFETY: the pointer is into `g`, which is borrowed for
                // all of this function and not otherwise used for storage
                // while `chest` lives.
                Some(p) => unsafe { &mut *p },
                None => g.chests.entry(self.chest_at).or_default(),
            };
            for i in 0..27 {
                let (x, y) = (gx + (i % cols) as f32 * cell, uy + (i / cols) as f32 * cell);
                let hot = ui.hot(x, y, cell, cell);
                draw_slot(ui.r, x, y, cell, s, chest.slots[i], hot);
                if hot {
                    if let Some(st) = chest.slots[i] {
                        tip = Some(crate::game::gear::wear::tooltip(&r.items[st.item as usize].name, &st));
                    }
                    if click_l || click_r {
                        chest.click(i, &mut g.cursor, click_r);
                        ui.clicked_any = true;
                    }
                }
            }
        } else if g.mode == GameMode::Creative && !at_table {
            // Category tabs in two rows, however many the content defines
            // (long names scroll inside their button).
            // Each tab shows a typical item of its kind; hovering names it.
            let tabs = &r.categories;
            let per_row = ((tabs.len() + 1) / 2).max(4);
            let tw = ((pw - 12.0 * s) / per_row as f32).min(22.0 * s);
            for (i, t) in tabs.iter().take(per_row * 2).enumerate() {
                let (tx, ty) = (gx + (i % per_row) as f32 * tw, py + 3.0 * s + (i / per_row) as f32 * 14.0 * s);
                if ui.button(tx, ty, tw - s, 12.0 * s, "") {
                    self.inv_tab = i;
                    self.inv_scroll = 0;
                }
                if let Some(first) = r.items.iter().position(|it| it.category == *t) {
                    draw_item(ui.r, tx + (tw - s - 11.0 * s) * 0.5, ty + 0.5 * s, 11.0 * s, first, WHITE);
                }
                if ui.hot(tx, ty, tw - s, 12.0 * s) {
                    tip = Some(t.clone());
                }
                if i == self.inv_tab {
                    ui.r.rect(tx, ty + 12.0 * s, tw - s, s, ui::ACCENT);
                }
            }
            let tab = tabs.get(self.inv_tab).cloned().unwrap_or_default();
            // Search the whole list by name, or show the open tab.
            ui.text_field(44, gx, py + 31.0 * s, pw - 12.0 * s, 11.0 * s, &mut self.inv_search, 24, ui::any_char);
            if self.inv_search.is_empty() {
                ui.text(gx + 3.0 * s, py + 33.0 * s, "TYPE HERE TO SEARCH EVERY ITEM", ui::DIM);
            }
            let needle = self.inv_search.to_ascii_lowercase();
            let list: Vec<usize> = (0..r.items.len())
                .filter(|&i| if needle.is_empty() { r.items[i].category == tab } else { r.items[i].name.to_ascii_lowercase().contains(&needle) || r.items[i].id.contains(&needle) })
                .collect();
            let total_rows = (list.len() + cols - 1) / cols;
            let max_scroll = total_rows.saturating_sub(upper_rows) as i32;
            self.inv_scroll = (self.inv_scroll - ui.inp.wheel).clamp(0, max_scroll);
            // The hand before this click, and the stack a click on a palette
            // cell would take (see creative_hand).
            let hand_before = g.cursor;
            let mut pick: Option<Stack> = None;
            for row in 0..upper_rows {
                for col in 0..cols {
                    let idx = (row + self.inv_scroll as usize) * cols + col;
                    let (x, y) = (gx + col as f32 * cell, uy + row as f32 * cell);
                    let hot = ui.hot(x, y, cell, cell);
                    match list.get(idx) {
                        Some(&item) => {
                            draw_slot(ui.r, x, y, cell, s, Some(Stack::new(item as u16, 1)), hot);
                            if hot {
                                tip = Some(r.items[item].name.clone());
                                if click_l || click_r {
                                    pick = Some(Stack::new(item as u16, if click_l { r.items[item].stack } else { 1 }));
                                }
                            }
                        }
                        None => draw_slot(ui.r, x, y, cell, s, None, false),
                    }
                }
            }
            let over_list = ui.hot(gx, uy, cols as f32 * cell, upper_rows as f32 * cell);
            let clicked = click_l || click_r;
            g.cursor = creative_hand(hand_before, pick, over_list, clicked);
            if clicked && (pick.is_some() || (over_list && hand_before.is_some())) {
                ui.clicked_any = true;
            }
            // The wheel scrolls a row at a time; the bar at the side shows
            // where in the list this is, and can be dragged.
            ui.scrollbar(px + pw - 4.5 * s, uy, 3.0 * s, upper_rows as f32 * cell, &mut self.inv_scroll, upper_rows, total_rows);
        } else if !(at_table && (self.at_oven || self.at_furnace)) {
            // The crafting grid, and the recipe book (client/screens/crafting.rs).
            let side = if at_table { self.craft.side() } else { crate::crafting::HAND_SIDE };
            let title = match (at_table, side) {
                (false, _) => "CRAFTING - A TABLE HAS A BIGGER GRID",
                (true, 5) => "CRAFTING BENCH",
                (true, 7) => "WORKSHOP",
                _ => "CRAFTING TABLE",
            };
            ui.text(gx, py + 6.0 * s, title, ui::DIM);
            let frame = crate::client::screens::crafting::Frame {
                gx,
                uy,
                width: cols as f32 * cell,
                cell,
                rows: upper_rows,
                side,
                shift: self.keys[225] || self.keys[229],
            };
            if let Some(t) = crate::client::screens::crafting::draw(ui, g, &mut self.craft, &frame) {
                tip = Some(t);
            }
        } else {
            // An oven's or a furnace's list: click a recipe to make it.
            let at_oven = at_table && self.at_oven;
            let at_furnace = at_table && self.at_furnace;
            ui.text(gx, py + 6.0 * s, if at_furnace { "FURNACE (CLICK SOMETHING TO SMELT IT)" } else { "OVEN (CLICK SOMETHING TO BAKE IT)" }, ui::DIM);
            // Recipes you can make now come first.
            let can = |rc: &block::Recipe| crate::crafting::legacy::affordable(&g.inv, &rc.inputs);
            // By hand only the simplest things; a table offers everything.
            let mut list: Vec<usize> = (0..r.recipes.len())
                .filter(|&i| {
                    let rc = &r.recipes[i];
                    // An oven bakes; a table makes everything else.
                    if at_furnace {
                        rc.furnace
                    } else if at_oven {
                        rc.oven
                    } else {
                        !rc.oven && !rc.furnace && (at_table || !rc.table)
                    }
                })
                .collect();
            list.sort_by_key(|&i| !can(&r.recipes[i]));
            let per_row = 3usize;
            let total_rows = (list.len() + per_row - 1) / per_row;
            let max_scroll = total_rows.saturating_sub(upper_rows) as i32;
            self.inv_scroll = (self.inv_scroll - ui.inp.wheel).clamp(0, max_scroll);
            let rw = (pw - 12.0 * s) / per_row as f32;
            let mut craft = None;
            for row in 0..upper_rows {
                for col in 0..per_row {
                    let idx = (row + self.inv_scroll as usize) * per_row + col;
                    let rc = match list.get(idx) {
                        Some(&i) => &r.recipes[i],
                        None => continue,
                    };
                    let (x, y) = (gx + col as f32 * rw, uy + row as f32 * cell);
                    let ok = can(rc);
                    let hot = ui.hot(x, y, rw - s, cell - s);
                    ui.r.rect(x, y, rw - s, cell - s, if !ok { ui::BUTTON_OFF } else if hot { ui::BUTTON_HOT } else { ui::BUTTON });
                    let tint: Color = if ok { WHITE } else { [120, 120, 120, 200] };
                    draw_item(ui.r, x + 2.0 * s, y + 2.0 * s, cell - 5.0 * s, rc.out.0, tint);
                    ui.r.label(x + cell - 2.0 * s, y + 6.0 * s, s, &alloc::format!("{}", rc.out.1), tint);
                    ui.r.label(x + cell + 7.0 * s, y + 6.0 * s, s, "=", ui::DIM);
                    for (k, &(item, n)) in rc.inputs.iter().take(3).enumerate() {
                        let ix = x + cell + 15.0 * s + k as f32 * 13.0 * s;
                        draw_item(ui.r, ix, y + 4.0 * s, 9.0 * s, item, tint);
                        if n > 1 {
                            ui.r.label(ix + 5.0 * s, y + 10.0 * s, s, &alloc::format!("{}", n), tint);
                        }
                    }
                    if hot {
                        let needs: Vec<String> = rc.inputs.iter().map(|&(i, n)| alloc::format!("{} {}", n, r.items[i].name)).collect();
                        tip = Some(alloc::format!("{} {} = {}", rc.out.1, r.items[rc.out.0].name, needs.join(" + ")));
                        if ok && click_l {
                            craft = Some(rc.clone());
                        }
                    }
                }
            }
            ui.scrollbar(px + pw - 4.5 * s, uy, 3.0 * s, upper_rows as f32 * cell, &mut self.inv_scroll, upper_rows, total_rows);
            if let Some(rc) = craft {
                for &(i, n) in &rc.inputs {
                    g.inv.remove(i as u16, n);
                }
                // What does not fit is set down at the feet, not lost after the ingredients went.
                if g.give_or_drop(rc.out.0 as u16, rc.out.1) > 0 {
                    g.say("INVENTORY FULL");
                }
                crate::game::events::crafted(g, rc.out.0);
                ui.clicked_any = true;
            }
        }

        // A portrait of the player beside the panel, where there is room, with
        // the four armour slots down its left side (client/screens/armor_slots.rs).
        if px > 70.0 * s {
            // The fields, not self.slim(): g holds a mutable borrow of self.session.
            let slim = self.settings.slim_arms || self.skin_slim;
            let body = body_for(g, own_id(g), slim);
            if let Some(model) = reg().model(arm_models(&body, slim).0) {
                let slots = px > 94.0 * s;
                let (pl, pwid) = if slots { (px - 90.0 * s, 84.0 * s) } else { (px - 66.0 * s, 60.0 * s) };
                ui.panel(pl, py, pwid, 124.0 * s);
                let cx = pl + pwid - 30.0 * s;
                let pose = crate::model::Pose { phase: g.clock * 2.0, stride: 0.12, ..Default::default() };
                ui.r.tex_override = self.skin_tex;
                ui.r.draw_model_2d(model, &pose, cx, py + 112.0 * s, PI - 0.45, 96.0 * s);
                if g.inv.worn.iter().any(|w| w.is_some()) {
                    ui.r.draw_model_2d_armor(model, &pose, cx, py + 112.0 * s, PI - 0.45, 96.0 * s, &crate::client::render::armor::looks(&g.inv.worn));
                }
                let name = self.settings.player_name.to_ascii_uppercase();
                ui.centered(cx, py + 114.0 * s, 1.0, &name, WHITE);
                if slots {
                    if let Some(t) = crate::client::screens::armor_slots::draw(ui, g, pl + 4.0 * s, py + 6.0 * s, 20.0 * s, click_l, click_r) {
                        tip = Some(t);
                    }
                }
            }
        }

        // Lower part: the player's own slots, hotbar last.
        let iy = uy + upper_rows as f32 * cell + 16.0 * s;
        ui.text(gx, iy - 10.0 * s, "INVENTORY", ui::DIM);
        for i in 0..SLOTS {
            let (row, col) = if i < HOTBAR { (3, i) } else { ((i - HOTBAR) / cols, (i - HOTBAR) % cols) };
            let (x, y) = (gx + col as f32 * cell, iy + row as f32 * cell + if row == 3 { 4.0 * s } else { 0.0 });
            let hot = ui.hot(x, y, cell, cell);
            draw_slot(ui.r, x, y, cell, s, g.inv.slots[i], hot);
            if i == g.selected {
                ui.r.frame(x, y, cell, cell, s, [250, 250, 250, 255]);
            }
            if hot {
                if let Some(st) = g.inv.slots[i] {
                    tip = Some(crate::game::gear::wear::tooltip(&r.items[st.item as usize].name, &st));
                }
                if click_l || click_r {
                    // Shift-click, or right click with an empty hand, puts armour on.
                    let armour_here = g.inv.slots[i].map_or(false, |st| crate::game::gear::armor::piece(st.item).is_some());
                    let shift = self.keys[225] || self.keys[229];
                    if armour_here && g.cursor.is_none() && (shift || click_r) {
                        g.put_on_armor(i);
                    } else {
                        g.inv.click(i, &mut g.cursor, click_r);
                    }
                    ui.clicked_any = true;
                }
            }
        }
        // Clicking outside the panel with something in hand drops it.
        if (click_l || click_r) && !ui.hot(px, py, pw, ph) && !ui.clicked_any {
            g.cursor = None;
        }
        if let Some(t) = tip {
            ui.centered(ui.w() * 0.5, (py - 10.0 * s).max(1.0), 1.0, &t.to_ascii_uppercase(), WHITE);
        }
        if let Some(c) = g.cursor {
            let (mx, my) = ui.inp.mouse;
            draw_item(ui.r, mx - 7.0 * s, my - 7.0 * s, 14.0 * s, c.item as usize, WHITE);
            if c.count > 1 {
                ui.r.label(mx + 2.0 * s, my + s, s, &alloc::format!("{}", c.count), WHITE);
            }
        }
    }
}
