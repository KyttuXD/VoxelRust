//! The fullscreen world map (M and N) and the picture behind it, which the
//! minimap shares.
//!
//! A [`MapPicture`] is a square of texels, each a sample of the world at
//! `step` blocks apart. It is painted a few texels a frame and goes on show
//! only when a whole pass is finished, so a long pass never shows half a
//! picture. The map shows a [`View`] of the picture; the view can move and
//! zoom, and the picture is repainted when it has moved too far from where
//! it was painted, when the zoom changes, when the world changes, or when
//! more ground has been explored.

use crate::client::ui::{self, Ui};
use crate::math::floord;
use crate::game::waypoints::{self, Waypoint, COLOURS};
use crate::render::{Renderer, WHITE};
use crate::worldgen;
use alloc::string::String;
use alloc::vec::Vec;

/// Texels on a side of the full map.
pub const TEXELS: usize = 256;
/// Texels on a side of the minimap.
pub const MINIMAP_TEXELS: usize = 64;
/// The zoom levels, in blocks per texel.
pub const MAP_SCALES: [i32; 7] = [1, 2, 4, 8, 16, 32, 64];
/// Round to the nearest block.
fn rnd(v: f32) -> i32 {
    floord(v as f64 + 0.5)
}

/// Round up.
fn cel(v: f32) -> i32 {
    -floord(-(v as f64))
}

/// A height nobody has painted yet.
const UNPAINTED: i32 = i32::MIN;

// ------------------------------------------------------------ colours

/// The terrain colour at a block, and its height (what the shading uses).
pub fn ground(seed: u32, x: i32, z: i32) -> ([u8; 3], i32) {
    let col = worldgen::column(seed, x, z);
    let base = if col.river {
        [44, 104, 198]
    } else if col.building > 0 {
        [196, 190, 184]
    } else if col.road {
        [86, 88, 94]
    } else if col.farm {
        [198, 180, 100]
    } else if col.paved {
        [120, 118, 116]
    } else {
        crate::lod::colour(col.height, col.biome)
    };
    (base, col.height)
}

/// Terrain shaded by height: higher ground is lighter, and slopes facing
/// north-west catch the light, which makes hills readable.
pub fn shade(base: [u8; 3], height: i32, north: i32) -> [u8; 3] {
    let lift = ((height - worldgen::SEA).clamp(0, 60) / 3 + (height - north).clamp(-3, 3) * 9) as i32;
    [0, 1, 2].map(|k| (base[k] as i32 + if height >= worldgen::SEA { lift } else { 0 }).clamp(0, 255) as u8)
}

/// The colour of one texel: terrain if it is seen, dimmer terrain if only
/// charted, and dark where nothing is known.
pub fn texel_colour(base: [u8; 3], height: i32, north: i32, seen: bool, charted: bool) -> [u8; 3] {
    if !(seen || charted) {
        return [24, 26, 32];
    }
    let lit = shade(base, height, north);
    if seen {
        lit
    } else {
        lit.map(|v| (v as u16 * 45 / 100 + 14) as u8)
    }
}

/// A key for the world a picture belongs to (seed and name).
pub fn world_key(seed: u32, name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ seed as u64;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ------------------------------------------------------------ the picture

pub struct MapPicture {
    pub size: usize,
    /// RGBA, `size` by `size`, row 0 at the north (smallest z).
    pub px: Vec<u8>,
    /// The height of every texel of this pass (for the north-west shading).
    heights: Vec<i32>,
    /// Texels painted so far in this pass.
    pub cursor: usize,
    /// The centre of this pass, in blocks, and the blocks per texel.
    pub origin: (i32, i32),
    pub step: i32,
    /// The texture on the graphics card (0 = none yet) and what it shows.
    pub tex: u32,
    pub shown_origin: (i32, i32),
    pub shown_step: i32,
    world: u64,
    explored: usize,
    begun: bool,
    ready: bool,
}

impl MapPicture {
    pub fn new(size: usize) -> MapPicture {
        MapPicture {
            size,
            px: alloc::vec![0; size * size * 4],
            heights: alloc::vec![UNPAINTED; size * size],
            cursor: 0,
            origin: (0, 0),
            step: 1,
            tex: 0,
            shown_origin: (0, 0),
            shown_step: 1,
            world: 0,
            explored: 0,
            begun: false,
            ready: false,
        }
    }

    pub fn len(&self) -> usize {
        self.size * self.size
    }

    /// Whether the picture is of this world.
    pub fn same_world(&self, world: u64) -> bool {
        self.world == world
    }

    /// Whether the current pass has painted every texel.
    pub fn done(&self) -> bool {
        self.begun && self.cursor >= self.len()
    }

    /// Start a new pass centred on `origin`.
    pub fn restart(&mut self, origin: (i32, i32), step: i32, world: u64, explored: usize) {
        self.origin = origin;
        self.step = step;
        self.world = world;
        self.explored = explored;
        self.begun = true;
        self.ready = false;
        self.cursor = 0;
        for h in self.heights.iter_mut() {
            *h = UNPAINTED;
        }
    }

    /// Whether the picture must be painted again for a view centred on
    /// `centre` at `step`. The picture is repainted once the view has moved
    /// a quarter of its width away from where the pass began.
    pub fn stale(&self, centre: (i32, i32), step: i32, world: u64, explored: usize) -> bool {
        if !self.begun {
            return true;
        }
        let cover = self.size as i32 * self.step;
        let moved = (centre.0 - self.origin.0).abs().max((centre.1 - self.origin.1).abs()) > cover / 4;
        world != self.world || step != self.step || moved || (self.done() && explored != self.explored)
    }

    /// Paint up to `count` more texels of the current pass. `seen` says
    /// whether the ground at a block has been explored; `charted` whether
    /// unexplored ground shows at all.
    pub fn paint(&mut self, seed: u32, seen: &dyn Fn(i32, i32) -> bool, charted: bool, count: usize) {
        let n = self.size;
        let half = (n / 2) as i32;
        let step = self.step;
        let end = (self.cursor + count).min(self.len());
        while self.cursor < end {
            let i = self.cursor;
            let (px, pz) = ((i % n) as i32, (i / n) as i32);
            let (x, z) = (self.origin.0 + (px - half) * step, self.origin.1 + (pz - half) * step);
            let (base, height) = ground(seed, x, z);
            // The texel to the north-west was painted earlier in this pass, so
            // its height is already known: one terrain lookup per texel.
            let north = if px > 0 && pz > 0 { self.heights[i - n - 1] } else { worldgen::column(seed, x - step, z - step).height };
            self.heights[i] = height;
            let c = texel_colour(base, height, north, seen(x, z), charted);
            self.px[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            self.cursor += 1;
        }
        if self.done() {
            self.ready = true;
        }
    }

    /// Show a finished pass: upload it, once per pass.
    pub fn upload_if_ready(&mut self, r: &mut Renderer) {
        if !self.ready {
            return;
        }
        self.ready = false;
        if self.tex == 0 {
            self.tex = r.add_texture(self.size, self.size, &self.px);
        } else {
            r.update_texture(self.tex, self.size, self.size, &self.px);
        }
        self.shown_origin = self.origin;
        self.shown_step = self.step;
    }
}

// ------------------------------------------------------------ the view

/// What part of the world the full map shows: the centre, in blocks, and
/// the blocks per texel (the picture covers `TEXELS * step` blocks across).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub centre: [f32; 2],
    pub step: i32,
}

impl View {
    /// Blocks across the whole picture.
    pub fn cover(&self, size: usize) -> f32 {
        size as f32 * self.step as f32
    }

    /// A block position as a point on the square of `side` pixels.
    pub fn to_screen(&self, size: usize, side: f32, bx: f32, bz: f32) -> [f32; 2] {
        let cover = self.cover(size);
        let (left, top) = (self.centre[0] - cover * 0.5, self.centre[1] - cover * 0.5);
        [(bx - left) / cover * side, (bz - top) / cover * side]
    }

    /// The block under a point on the square.
    pub fn to_block(&self, size: usize, side: f32, sx: f32, sy: f32) -> [f32; 2] {
        let cover = self.cover(size);
        let (left, top) = (self.centre[0] - cover * 0.5, self.centre[1] - cover * 0.5);
        [left + sx / side * cover, top + sy / side * cover]
    }

    /// Zoom to `new_step`, keeping the block under the point fixed.
    pub fn zoom_about(&mut self, size: usize, side: f32, sx: f32, sy: f32, new_step: i32) {
        let anchor = self.to_block(size, side, sx, sy);
        let k = new_step as f32 / self.step as f32;
        self.centre = [anchor[0] + (self.centre[0] - anchor[0]) * k, anchor[1] + (self.centre[1] - anchor[1]) * k];
        self.step = new_step;
    }

    /// Move the view by a drag of `dx` by `dy` pixels (the picture follows the pointer).
    pub fn pan(&mut self, size: usize, side: f32, dx: f32, dy: f32) {
        let k = self.cover(size) / side;
        self.centre = [self.centre[0] - dx * k, self.centre[1] - dy * k];
    }
}

/// The index into [`MAP_SCALES`] nearest to a step.
pub fn scale_index(step: i32) -> usize {
    MAP_SCALES.iter().position(|&s| s >= step).unwrap_or(MAP_SCALES.len() - 1)
}

/// Where to draw a picture on screen: the part of `pic` that falls inside
/// `frame` (both as x0, y0, x1, y1), as a screen rectangle and the matching
/// part of the texture (u0, v0, u1, v1). None if nothing shows.
pub fn clip(pic: [f32; 4], frame: [f32; 4]) -> Option<([f32; 4], [f32; 4])> {
    let (x0, y0) = (pic[0].max(frame[0]), pic[1].max(frame[1]));
    let (x1, y1) = (pic[2].min(frame[2]), pic[3].min(frame[3]));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = (pic[2] - pic[0], pic[3] - pic[1]);
    let uv = [(x0 - pic[0]) / w, (y0 - pic[1]) / h, (x1 - pic[0]) / w, (y1 - pic[1]) / h];
    Some(([x0, y0, x1, y1], uv))
}

/// Where the map square and the side panel go on a screen of `w` by `h`
/// pixels, at `s` pixels per UI pixel: (map as x, y, side; panel as x, y, w, h).
pub fn layout(w: f32, h: f32, s: f32) -> ([f32; 3], [f32; 4]) {
    let margin = 24.0 * s;
    let side = (h - 2.0 * margin).min((w - 2.0 * margin) * 0.66).min(w - 2.0 * margin - 138.0 * s).max(64.0 * s);
    let map = [margin, (h - side) * 0.5, side];
    let px = margin + side + 18.0 * s;
    (map, [px, margin, (w - margin - px).max(120.0 * s), h - 2.0 * margin])
}

// ------------------------------------------------------------ the screen

/// The map's own state (what the picture and the view are, and the toggles).
pub struct FullMap {
    pub pic: MapPicture,
    pub view: View,
    pub follow: bool,
    pub grid: bool,
    pub show_waypoints: bool,
    pub selected: Option<usize>,
    pub confirm_delete: bool,
    pub dragging: bool,
    /// The name being typed for the selected waypoint.
    pub name_buf: String,
    pub name_for: Option<usize>,
    /// The first waypoint shown in the panel's list.
    pub list_scroll: i32,
}

/// What the screen needs from the game each frame.
pub struct Frame<'a> {
    /// The other players the game knows the whereabouts of: name and place (x, z).
    pub others: &'a [(String, [f32; 2])],
    pub seed: u32,
    pub world: u64,
    pub explored: &'a alloc::collections::BTreeSet<(i32, i32)>,
    pub charted: bool,
    pub player: [f32; 2],
    pub now: f64,
}

impl FullMap {
    pub fn new(scale: usize, follow: bool, grid: bool, show_waypoints: bool) -> FullMap {
        FullMap {
            pic: MapPicture::new(TEXELS),
            view: View { centre: [0.0, 0.0], step: MAP_SCALES[scale.min(MAP_SCALES.len() - 1)] },
            follow,
            grid,
            show_waypoints,
            selected: None,
            confirm_delete: false,
            dragging: false,
            name_buf: String::new(),
            name_for: None,
            list_scroll: 0,
        }
    }

    /// Paint for a little while (at least 64 texels, and at most 2 ms of work).
    pub fn work(&mut self, r: &mut Renderer, f: &Frame) {
        if self.follow {
            self.view.centre = f.player;
        }
        let centre = (rnd(self.view.centre[0]), rnd(self.view.centre[1]));
        if self.pic.stale(centre, self.view.step, f.world, f.explored.len()) {
            self.pic.restart(centre, self.view.step, f.world, f.explored.len());
        }
        let explored = f.explored;
        let seen = |x: i32, z: i32| explored.contains(&(x.div_euclid(crate::session::EXPLORE), z.div_euclid(crate::session::EXPLORE)));
        let start = crate::sys::now();
        let mut first = true;
        while !self.pic.done() && (first || crate::sys::now() - start < 0.002) {
            self.pic.paint(f.seed, &seen, f.charted, 64);
            first = false;
        }
        self.pic.upload_if_ready(r);
    }
}

/// The minimap's picture: 2 blocks per texel, restarted on the player.
pub fn minimap_work(pic: &mut MapPicture, r: &mut Renderer, f: &Frame, texels_per_frame: usize) {
    // Each pass is centred on where the player is when it starts.
    let centre = (floord(f.player[0] as f64), floord(f.player[1] as f64));
    if !pic.begun || pic.done() || !pic.same_world(f.world) {
        pic.restart(centre, 2, f.world, f.explored.len());
    }
    let explored = f.explored;
    let seen = |x: i32, z: i32| explored.contains(&(x.div_euclid(crate::session::EXPLORE), z.div_euclid(crate::session::EXPLORE)));
    pic.paint(f.seed, &seen, f.charted, texels_per_frame);
    pic.upload_if_ready(r);
}

/// What the player asked the map to do this frame: a zoom preset (0, 1 or 2).
#[derive(Default)]
pub struct Asked {
    pub preset: Option<usize>,
}

/// Draw the full map and its panel, and handle what the player does with
/// it. Returns what the player asked for.
pub fn screen(fm: &mut FullMap, ui: &mut Ui, f: &Frame, waypoints: &mut Vec<Waypoint>) -> Asked {
    let mut asked = Asked::default();
    let s = ui.s;
    let (w, h) = (ui.w(), ui.h());
    let ([mx, my, side], [px, py, pw, ph]) = layout(w, h, s);
    ui.r.rect(0.0, 0.0, w, h, [10, 12, 18, 236]);
    let frame = [mx, my, mx + side, my + side];

    // Keys: 1-3 presets, C centres on the player, F follow, G grid, H
    // waypoints. Closing (M, N, Escape) is the app's job, so that the key that
    // opened the map does not close it again.
    let typing = *ui.focus == 60;
    for &k in &ui.inp.keys {
        if typing {
            if k == ui::KEY_ESCAPE {
                *ui.focus = 0;
            }
            continue;
        }
        match k {
            30..=32 => asked.preset = Some((k - 30) as usize),
            6 => {
                fm.view.centre = f.player;
                fm.follow = true;
            }
            9 => fm.follow = !fm.follow,
            10 => fm.grid = !fm.grid,
            11 => fm.show_waypoints = !fm.show_waypoints,
            _ => {}
        }
    }

    // The picture, clipped to its frame.
    ui.r.frame(mx - 2.0 * s, my - 2.0 * s, side + 4.0 * s, side + 4.0 * s, s, [96, 104, 130, 255]);
    ui.r.rect(mx, my, side, side, [22, 24, 32, 255]);
    let to_screen = |bx: f32, bz: f32| {
        let p = fm.view.to_screen(TEXELS, side, bx, bz);
        [mx + p[0], my + p[1]]
    };
    if fm.pic.tex != 0 && fm.pic.same_world(f.world) {
        // Texel i samples the block at origin + (i - half) * step, and covers
        // half a step either side of that: the picture spans from there.
        let (n, st) = (fm.pic.size as f32, fm.pic.shown_step as f32);
        let half = n * 0.5;
        let (ox, oz) = (fm.pic.shown_origin.0 as f32, fm.pic.shown_origin.1 as f32);
        let lo = [ox - (half + 0.5) * st, oz - (half + 0.5) * st];
        let a = to_screen(lo[0], lo[1]);
        let b = to_screen(lo[0] + n * st, lo[1] + n * st);
        if let Some((rect, uv)) = clip([a[0], a[1], b[0], b[1]], frame) {
            ui.r.image_part(fm.pic.tex, rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1], uv);
        }
    }

    // Grid every 64 blocks.
    if fm.grid {
        let cover = fm.view.cover(TEXELS);
        let (left, top) = (fm.view.centre[0] - cover * 0.5, fm.view.centre[1] - cover * 0.5);
        let col = [255, 255, 255, 40];
        let mut gx = cel(left / 64.0) * 64;
        while (gx as f32) < left + cover {
            let p = to_screen(gx as f32, top);
            ui.r.rect(p[0], my, s, side, col);
            gx += 64;
        }
        let mut gz = cel(top / 64.0) * 64;
        while (gz as f32) < top + cover {
            let p = to_screen(left, gz as f32);
            ui.r.rect(mx, p[1], side, s, col);
            gz += 64;
        }
    }

    // Waypoints.
    if fm.show_waypoints {
        for (i, wp) in waypoints.iter().enumerate() {
            let p = to_screen(wp.x as f32 + 0.5, wp.z as f32 + 0.5);
            if p[0] < mx || p[0] > mx + side || p[1] < my || p[1] > my + side {
                continue;
            }
            let c = COLOURS[wp.colour.min(7) as usize];
            let d = 7.0 * s;
            if fm.selected == Some(i) {
                ui.r.rect(p[0] - d * 0.5 - 2.0 * s, p[1] - d * 0.5 - 2.0 * s, d + 4.0 * s, d + 4.0 * s, WHITE);
            }
            ui.r.rect(p[0] - d * 0.5, p[1] - d * 0.5, d, d, [c[0], c[1], c[2], 255]);
            ui.text(p[0] + 7.0 * s, p[1] - 4.0 * s, &wp.name, WHITE);
        }
    }

    // The other players, with their names.
    for (name, at) in f.others {
        let p = to_screen(at[0], at[1]);
        if p[0] > mx && p[0] < mx + side && p[1] > my && p[1] < my + side {
            ui.r.rect(p[0] - 3.0 * s, p[1] - 3.0 * s, 6.0 * s, 6.0 * s, [110, 220, 255, 255]);
            ui.text(p[0] + 6.0 * s, p[1] - 4.0 * s, &name.to_ascii_uppercase(), [180, 235, 255, 255]);
        }
    }
    // The player.
    let pp = to_screen(f.player[0], f.player[1]);
    ui.r.rect(pp[0] - 3.0 * s, pp[1] - 3.0 * s, 6.0 * s, 6.0 * s, [255, 70, 60, 255]);

    // Hover, and what a click does there.
    let inside = ui.inp.mouse.0 >= mx && ui.inp.mouse.0 <= mx + side && ui.inp.mouse.1 >= my && ui.inp.mouse.1 <= my + side;
    let mut hover_block: Option<[f32; 2]> = None;
    if inside {
        let sx = ui.inp.mouse.0 - mx;
        let sy = ui.inp.mouse.1 - my;
        let b = fm.view.to_block(TEXELS, side, sx, sy);
        hover_block = Some(b);
        let wheel = ui.inp.wheel;
        if wheel != 0 {
            let idx = scale_index(fm.view.step) as i32 - wheel;
            let idx = idx.clamp(0, MAP_SCALES.len() as i32 - 1) as usize;
            if MAP_SCALES[idx] != fm.view.step {
                fm.view.zoom_about(TEXELS, side, sx, sy, MAP_SCALES[idx]);
                fm.follow = false;
            }
        }
        if ui.inp.clicked[1] {
            fm.dragging = true;
            ui.clicked_any = true;
            let hit = waypoints::nearest(waypoints, b[0], b[1], 6.0 * fm.view.cover(TEXELS) / side);
            fm.selected = hit;
            fm.confirm_delete = false;
        }
        if ui.inp.clicked[3] && waypoints.len() < waypoints::MAX {
            waypoints.push(Waypoint {
                x: rnd(b[0]),
                z: rnd(b[1]),
                colour: (waypoints.len() % COLOURS.len()) as u8,
                name: alloc::format!("WAYPOINT {}", waypoints.len() + 1),
            });
            fm.selected = Some(waypoints.len() - 1);
            fm.confirm_delete = false;
            ui.clicked_any = true;
        }
    }
    if !ui.inp.down[1] {
        fm.dragging = false;
    }
    if fm.dragging && ui.inp.down[1] && (ui.inp.moved.0 != 0.0 || ui.inp.moved.1 != 0.0) {
        fm.view.pan(TEXELS, side, ui.inp.moved.0, ui.inp.moved.1);
        fm.follow = false;
    }
    if let Some(b) = hover_block {
        ui.text(mx + 4.0 * s, my + side - 14.0 * s, &alloc::format!("X {}   Z {}", rnd(b[0]), rnd(b[1])), WHITE);
    }

    // The panel.
    ui.panel(px - 6.0 * s, py - 6.0 * s, pw + 12.0 * s, ph + 12.0 * s);
    let bh = 16.0 * s;
    let step = 19.0 * s;
    let mut y = py;
    ui.text(px, y, "WORLD MAP", ACCENT_TEXT);
    y += step * 1.3;
    ui.toggle(px, y, pw, bh, "FOLLOW ME", &mut fm.follow);
    y += step;
    ui.toggle(px, y, pw, bh, "GRID", &mut fm.grid);
    y += step;
    ui.toggle(px, y, pw, bh, "WAYPOINTS", &mut fm.show_waypoints);
    y += step * 1.3;
    let scale_text = alloc::format!("ZOOM: {} BLOCK(S) PER PIXEL", fm.view.step);
    ui.text(px, y, &scale_text, ui::DIM);
    y += step;
    let third = (pw - 8.0 * s) / 3.0;
    for (i, label) in ["1", "2", "3"].iter().enumerate() {
        if ui.button(px + i as f32 * (third + 4.0 * s), y, third, bh, label) {
            asked.preset = Some(i);
        }
    }
    y += step * 1.3;
    ui.text(px, y, "WAYPOINTS", ui::DIM);
    y += step * 0.9;
    // Room is kept below the list for the name, the actions and ADD HERE.
    let rows = ((ph - (y - py) - step * 8.0) / step).max(2.0) as usize;
    let total = waypoints.len();
    let max_scroll = total.saturating_sub(rows) as i32;
    if ui.hot(px, y, pw, rows as f32 * step) {
        fm.list_scroll -= ui.inp.wheel;
    }
    fm.list_scroll = fm.list_scroll.clamp(0, max_scroll);
    let first = fm.list_scroll as usize;
    let shown = rows.min(total - first.min(total));
    for row in 0..shown {
        let k = first + row;
        let wp = &waypoints[k];
        let c = COLOURS[wp.colour.min(7) as usize];
        let ry = y + row as f32 * step;
        ui.r.rect(px, ry + 3.0 * s, 8.0 * s, 8.0 * s, [c[0], c[1], c[2], 255]);
        let label = if fm.selected == Some(k) { alloc::format!("> {}", wp.name) } else { wp.name.clone() };
        if ui.button(px + 12.0 * s, ry, pw - 18.0 * s, bh, &label) {
            fm.selected = Some(k);
            fm.confirm_delete = false;
        }
    }
    if total > rows {
        ui.scrollbar(px + pw - 4.0 * s, y, 3.0 * s, rows as f32 * step, &mut fm.list_scroll, rows, total);
    }
    y += rows.min(total).max(1) as f32 * step;
    if total == 0 {
        ui.text(px, y - step * 0.6, "NONE YET: RIGHT CLICK THE MAP", ui::DIM);
    }
    y += step * 0.4;

    // Actions on the selected waypoint.
    if let Some(k) = fm.selected.filter(|&k| k < waypoints.len()) {
        if fm.name_for != Some(k) {
            fm.name_buf = waypoints[k].name.clone();
            fm.name_for = Some(k);
        }
        ui.text(px, y, "NAME", ui::DIM);
        y += step * 0.9;
        ui.text_field(60, px, y, pw, bh, &mut fm.name_buf, waypoints::NAME_MAX, ui::any_char);
        // Typing changes the name at once; an empty box leaves the name as it was.
        if !fm.name_buf.trim().is_empty() {
            let typed = waypoints::clean_name(&fm.name_buf);
            if typed != waypoints[k].name {
                waypoints[k].name = typed;
            }
        }
        y += step * 1.2;
        let half = (pw - 4.0 * s) * 0.5;
        if ui.button(px, y, half, bh, "GO TO") {
            fm.view.centre = [waypoints[k].x as f32 + 0.5, waypoints[k].z as f32 + 0.5];
            fm.follow = false;
        }
        if ui.button(px + half + 4.0 * s, y, half, bh, "COLOUR") {
            waypoints[k].colour = (waypoints[k].colour + 1) % COLOURS.len() as u8;
        }
        y += step;
        let del = if fm.confirm_delete { "SURE? DELETE" } else { "DELETE" };
        if ui.button(px, y, half, bh, del) {
            if fm.confirm_delete {
                waypoints.remove(k);
                fm.selected = None;
                fm.name_for = None;
                fm.confirm_delete = false;
            } else {
                fm.confirm_delete = true;
            }
        }
    }
    let add_y = py + ph - bh;
    if waypoints.len() < waypoints::MAX && ui.button(px, add_y - step, pw, bh, "ADD HERE") {
        let c = fm.view.centre;
        waypoints.push(Waypoint { x: rnd(c[0]), z: rnd(c[1]), colour: (waypoints.len() % COLOURS.len()) as u8, name: alloc::format!("WAYPOINT {}", waypoints.len() + 1) });
        fm.selected = Some(waypoints.len() - 1);
        fm.confirm_delete = false;
    }
    // Nothing is selected, so no name box is drawn: give up its keyboard too.
    if fm.selected.is_none() && *ui.focus == 60 {
        *ui.focus = 0;
    }

    // The footer.
    let hint = "M OR ESC CLOSES - DRAG TO MOVE - WHEEL TO ZOOM - RIGHT CLICK ADDS A WAYPOINT";
    ui.centered(w * 0.5, h - 14.0 * s, 0.8, hint, ui::DIM);

    asked
}

const ACCENT_TEXT: crate::render::Color = ui::ACCENT;

#[cfg(test)]
mod tests {
    use super::*;

    /// The minimap's per-texel rule as it was written before it was shared:
    /// a literal copy, kept to show that the shared painter gives the same pixels.
    fn old_rule(seed: u32, origin: (i32, i32), scale: i32, i: usize, n: usize, seen: &dyn Fn(i32, i32) -> bool, charted: bool) -> [u8; 4] {
        let half = (n / 2) as i32;
        let (px, pz) = ((i % n) as i32, (i / n) as i32);
        let (x, z) = (origin.0 + (px - half) * scale, origin.1 + (pz - half) * scale);
        let seen_here = seen(x, z);
        let c = if seen_here || charted {
            let col = worldgen::column(seed, x, z);
            let base = if col.river {
                [44, 104, 198]
            } else if col.building > 0 {
                [196, 190, 184]
            } else if col.road {
                [86, 88, 94]
            } else if col.farm {
                [198, 180, 100]
            } else if col.paved {
                [120, 118, 116]
            } else {
                crate::lod::colour(col.height, col.biome)
            };
            let north = worldgen::column(seed, x - scale, z - scale).height;
            let lift = ((col.height - worldgen::SEA).clamp(0, 60) / 3 + (col.height - north).clamp(-3, 3) * 9) as i32;
            let lit = [0, 1, 2].map(|k| (base[k] as i32 + if col.height >= worldgen::SEA { lift } else { 0 }).clamp(0, 255) as u8);
            if seen_here {
                lit
            } else {
                lit.map(|v| (v as u16 * 45 / 100 + 14) as u8)
            }
        } else {
            [24, 26, 32]
        };
        [c[0], c[1], c[2], 255]
    }

    #[test]
    fn the_shared_painter_gives_the_minimap_its_old_pixels() {
        let seed = 13;
        let origin = (-130, 75);
        let seen = |x: i32, z: i32| (x / 3 + z / 5).rem_euclid(3) != 0;
        for &charted in &[false, true] {
            let mut pic = MapPicture::new(MINIMAP_TEXELS);
            pic.restart(origin, 2, 1, 0);
            pic.paint(seed, &seen, charted, MINIMAP_TEXELS * MINIMAP_TEXELS);
            assert!(pic.done());
            for i in (0..MINIMAP_TEXELS * MINIMAP_TEXELS).step_by(37) {
                let want = old_rule(seed, origin, 2, i, MINIMAP_TEXELS, &seen, charted);
                assert_eq!(&pic.px[i * 4..i * 4 + 4], &want, "texel {i}, charted {charted}");
            }
        }
    }

    #[test]
    fn a_pass_paints_in_small_steps_and_is_ready_once() {
        let seen = |_: i32, _: i32| true;
        let mut pic = MapPicture::new(16);
        pic.restart((0, 0), 4, 9, 0);
        pic.paint(3, &seen, false, 10);
        assert_eq!(pic.cursor, 10);
        assert!(!pic.done());
        pic.paint(3, &seen, false, 1000);
        assert!(pic.done());
        assert!(pic.ready);
    }

    #[test]
    fn the_picture_is_repainted_only_when_the_view_really_changed() {
        let mut pic = MapPicture::new(256);
        assert!(pic.stale((0, 0), 2, 1, 0), "nothing painted yet");
        pic.restart((0, 0), 2, 1, 0);
        assert!(!pic.stale((100, 100), 2, 1, 0), "a short move is fine");
        assert!(pic.stale((0, 200), 2, 1, 0), "a quarter of the width (256 * 2 / 4 = 128 blocks) is too far");
        assert!(pic.stale((0, 0), 4, 1, 0), "a zoom change repaints");
        assert!(pic.stale((0, 0), 2, 2, 0), "another world repaints");
        pic.paint(1, &|_, _| false, false, 256 * 256);
        assert!(!pic.stale((0, 0), 2, 1, 0));
        assert!(pic.stale((0, 0), 2, 1, 5), "explored ground refreshes once the pass is done");
    }

    #[test]
    fn block_and_screen_positions_agree_at_every_zoom_and_for_negative_coordinates() {
        for &step in &MAP_SCALES {
            let v = View { centre: [-77.5, 310.25], step };
            for &(x, z) in &[(-500.0f32, 40.0f32), (0.0, 0.0), (123.0, -456.0)] {
                let p = v.to_screen(TEXELS, 600.0, x, z);
                let b = v.to_block(TEXELS, 600.0, p[0], p[1]);
                assert!((b[0] - x).abs() < 1e-2 && (b[1] - z).abs() < 1e-2, "step {step}: {b:?} vs {x} {z}");
            }
        }
    }

    #[test]
    fn zooming_keeps_the_block_under_the_pointer_fixed() {
        let mut v = View { centre: [40.0, -90.0], step: 4 };
        let before = v.to_block(TEXELS, 500.0, 120.0, 300.0);
        v.zoom_about(TEXELS, 500.0, 120.0, 300.0, 16);
        let after = v.to_block(TEXELS, 500.0, 120.0, 300.0);
        assert!((before[0] - after[0]).abs() < 1e-3 && (before[1] - after[1]).abs() < 1e-3);
        assert_eq!(v.step, 16);
    }

    #[test]
    fn a_pan_moves_the_view_against_the_drag() {
        let mut v = View { centre: [0.0, 0.0], step: 2 };
        v.pan(TEXELS, 512.0, 64.0, 0.0);
        // A drag of 64 pixels right moves the view 64 * (256 * 2 / 512) = 64 blocks left.
        assert!((v.centre[0] + 64.0).abs() < 1e-4 && v.centre[1].abs() < 1e-4);
    }

    #[test]
    fn the_visible_part_of_a_picture_is_clipped_to_its_frame() {
        let frame = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(clip([-50.0, 0.0, 50.0, 100.0], frame), Some(([0.0, 0.0, 50.0, 100.0], [0.5, 0.0, 1.0, 1.0])));
        assert_eq!(clip([200.0, 0.0, 300.0, 100.0], frame), None);
        let whole = clip([10.0, 10.0, 20.0, 20.0], frame).unwrap();
        assert_eq!(whole.1, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn the_layout_leaves_room_for_the_panel() {
        let (map, panel) = layout(1280.0, 720.0, 1.0);
        assert!(map[0] + map[2] < panel[0], "the panel starts right of the map");
        assert!(map[1] >= 0.0 && map[1] + map[2] <= 720.0);
        assert!(panel[0] + panel[2] <= 1280.0 + 1e-3);
    }

    #[test]
    fn scale_indices_step_through_the_zoom_levels() {
        assert_eq!(scale_index(1), 0);
        assert_eq!(scale_index(4), 2);
        assert_eq!(scale_index(64), 6);
        assert_eq!(scale_index(100), 6);
    }
}
