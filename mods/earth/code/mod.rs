//! The Earth mod's own code: worlds made from the real planet.
//!
//! The game knows nothing of this. It is compiled in because it sits in
//! `mods/earth/code/`, and runs while a loaded pack says `feature earth`
//! (this mod's `pack.txt` does). What the game calls is the table at the
//! end, [`NATIVE`]: the terrain itself, the choice on the new-world screen,
//! the `--earth` option, the map on M, place-name notices, two rows of
//! settings, and answering other players' requests for map tiles.

mod map;
pub use map::*;

use crate::audio::Audio;
use crate::native::Native;
use crate::session::{Controls, Session};
use crate::settings::Settings;
use crate::ui::{self, Ui};
use crate::worldgen::{Column, Source};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The map projection of the open world, if it is an Earth world.
static PROJ: AtomicPtr<Proj> = AtomicPtr::new(core::ptr::null_mut());

pub fn proj() -> Option<&'static Proj> {
    if crate::worldgen::source().is_none() {
        return None;
    }
    unsafe { PROJ.load(Ordering::Acquire).as_ref() }
}

/// What the screens remember. Only the main thread touches it.
struct State {
    // The new-world form.
    place: usize,
    scale: i32,
    vscale: i32,
    features: bool,
    // The command line.
    opt_at: Option<(f64, f64)>,
    opt_scale: Option<(u32, u32)>,
    // The map screen.
    center: (f64, f64),
    zoom: i32,
    tiles: BTreeMap<(u32, u32, u32), MapTile>,
    // Place notices: the detail wanted (0 = off), and the asking.
    level: i32,
    timer: f32,
    asked: bool,
    last: String,
}

/// A map tile on its way to the screen.
enum MapTile {
    /// Asked for; a worker is fetching it.
    Loading,
    Ready(u32),
    Missing,
}

fn state() -> &'static mut State {
    static mut STATE: Option<State> = None;
    // SAFETY: the hooks that use this all run on the main thread.
    unsafe {
        (*core::ptr::addr_of_mut!(STATE)).get_or_insert_with(|| State {
            place: 0,
            scale: 30,
            vscale: 75,
            features: true,
            opt_at: None,
            opt_scale: None,
            center: (0.5, 0.5),
            zoom: 10,
            tiles: BTreeMap::new(),
            level: 1,
            timer: 4.0,
            asked: false,
            last: String::new(),
        })
    }
}

// ------------------------------------------------------------------ terrain

fn kind_text(lat: f64, lon: f64, scale: u32, vscale: u32, features: bool) -> String {
    alloc::format!("earth {} {} {} {} {}", lat, lon, scale, vscale, features as i32)
}

/// `earth <lat> <lon> <metres per block> <metres per block up> [0|1]`.
fn parse_kind(text: &str) -> Option<(f64, f64, u32, u32, bool)> {
    let mut w = text.split(' ');
    if w.next() != Some("earth") {
        return None;
    }
    let lat = w.next()?.parse::<f64>().ok()?;
    let lon = w.next()?.parse::<f64>().ok()?;
    let scale = w.next()?.parse::<u32>().ok()?;
    let vscale = w.next()?.parse::<u32>().ok()?;
    if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) || scale == 0 || vscale == 0 {
        return None;
    }
    // Older saves have no fifth word: roads and buildings on.
    Some((lat, lon, scale.min(100_000), vscale.min(10_000), w.next() != Some("0")))
}

fn terrain_parse(text: &str) -> Option<Option<String>> {
    if text.split(' ').next() != Some("earth") {
        return None;
    }
    Some(parse_kind(text).map(|(lat, lon, s, v, f)| kind_text(lat, lon, s, v, f)))
}

fn the_proj() -> &'static Proj {
    // Only called through a `Source` this code handed out, after `PROJ`
    // was set.
    unsafe { &*PROJ.load(Ordering::Acquire) }
}

fn source_column(x: i32, z: i32) -> Column {
    column(the_proj(), x, z)
}

fn source_built(x: i32, z: i32) -> bool {
    cover_at(the_proj(), x, z) == class::BUILDING
}

fn source_road(x: i32, z: i32) -> bool {
    cover_at(the_proj(), x, z) == class::ROAD
}

fn terrain_open(text: &str) -> Option<Source> {
    let (lat, lon, scale, vscale, features) = parse_kind(text)?;
    reset_base();
    let mut p = Proj::new(lat, lon, scale, vscale);
    p.features = features;
    // (The old value is leaked on purpose: workers may still be reading it.)
    PROJ.store(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(p)), Ordering::Release);
    let st = state();
    st.last.clear();
    st.asked = false;
    st.timer = 4.0;
    Some(Source { column: source_column, built: source_built, road: source_road, lanes: p.scale <= 6.0, charted: true })
}

// ----------------------------------------------------- the new-world screen

fn terrain_label() -> Option<&'static str> {
    if available() {
        Some("EARTH")
    } else {
        None
    }
}

fn step_list(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, shown: &str, v: &mut i32, list: &[i32]) {
    let mut i = list.iter().position(|x| x == v).unwrap_or(0) as i32;
    ui.stepper_text(x, y, w, h, shown, &mut i, 0, list.len() as i32 - 1, 1);
    *v = list[i as usize];
}

fn terrain_form(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, step: f32) -> f32 {
    let st = state();
    let places = places();
    if places.is_empty() {
        return 0.0;
    }
    st.place = st.place.min(places.len() - 1);
    let place = alloc::format!("START AT  < {} >", places[st.place].0);
    let mut i = st.place as i32;
    ui.stepper_text(x, y, w, h, &place, &mut i, 0, places.len() as i32 - 1, 1);
    st.place = i as usize;
    let half = (w - 4.0 * ui.s) * 0.5;
    let sc = alloc::format!("SCALE  < 1:{} >", st.scale);
    step_list(ui, x, y + step, half, h, &sc, &mut st.scale, &[1, 2, 5, 10, 30, 100, 300, 1000]);
    let vs = alloc::format!("HEIGHT  < 1:{} >", st.vscale);
    step_list(ui, x + half + 4.0 * ui.s, y + step, half, h, &vs, &mut st.vscale, &[1, 2, 5, 10, 25, 50, 75, 150, 300]);
    ui.toggle(x, y + step * 2.0, w, h, "ROADS AND BUILDINGS", &mut st.features);
    ui.centered(ui.w() * 0.5, y + step * 3.1, 1.0, "NEEDS THE INTERNET. PRESS M IN THE WORLD FOR THE MAP.", ui::DIM);
    3.9
}

fn terrain_chosen() -> String {
    let st = state();
    let (lat, lon) = places().get(st.place).map_or((0.0, 0.0), |p| (p.1, p.2));
    kind_text(lat, lon, st.scale as u32, st.vscale as u32, st.features)
}

// --------------------------------------------------------- the command line

const HELP: &str = "  --earth LAT LON     make the new world real-world terrain starting there\n  --scale M V         with --earth: metres per block across and up (30 75)\n";

fn option(name: &str, rest: &mut dyn Iterator<Item = String>) -> Option<Result<(), String>> {
    let mut num = |what: &str| rest.next().and_then(|v| v.parse::<f64>().ok()).ok_or_else(|| alloc::format!("{} needs two numbers", what));
    match name {
        "--earth" => Some((|| {
            let (lat, lon) = (num("--earth")?, num("--earth")?);
            if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return Err(String::from("--earth: latitude must be -85..85 and longitude -180..180"));
            }
            state().opt_at = Some((lat, lon));
            Ok(())
        })()),
        "--scale" => Some((|| {
            let (m, v) = (num("--scale")?, num("--scale")?);
            state().opt_scale = Some(((m as u32).clamp(1, 100_000), (v as u32).clamp(1, 10_000)));
            Ok(())
        })()),
        _ => None,
    }
}

fn option_kind() -> Option<String> {
    let st = state();
    let (lat, lon) = st.opt_at?;
    let (scale, vscale) = st.opt_scale.unwrap_or((30, 75));
    Some(kind_text(lat, lon, scale, vscale, true))
}

fn start(base: &str, settings: &Settings) {
    configure(base, settings.extra("relay"));
    state().level = settings.extra("place_notices").parse::<i32>().unwrap_or(1).clamp(0, 3);
}

// ------------------------------------------------------------ place notices

const TAG_PLACE: u32 = 0xEA47_0001;
const TAG_TILE: u32 = 0xEA47_0002;

/// On a worker: what a place is called. `arg` is "lat lon level".
fn task_place(arg: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(arg);
    let mut w = text.split(' ');
    let mut n = || w.next().and_then(|v| v.parse::<f64>().ok());
    match (n(), n(), n()) {
        (Some(lat), Some(lon), Some(level)) => place_name(lat, lon, level as i32).unwrap_or_default().into_bytes(),
        _ => Vec::new(),
    }
}

/// On a worker: the picture of a map tile. `arg` is "z x y"; the answer
/// is 256 x 256 RGBA, or nothing if it could not be fetched.
fn task_tile(arg: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(arg);
    let mut w = text.split(' ').filter_map(|v| v.parse::<u32>().ok());
    match (w.next(), w.next(), w.next()) {
        (Some(z), Some(x), Some(y)) => tile((Layer::Map, z, x, y)).map(|t| t.rgba.clone()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Take this code's finished tasks out of the world's list.
fn take_results(g: &mut Session, tag: u32) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut mine = Vec::new();
    let mut k = 0;
    while k < g.world.task_results.len() {
        if g.world.task_results[k].0 == tag {
            let (_, arg, out) = g.world.task_results.remove(k);
            mine.push((arg, out));
        } else {
            k += 1;
        }
    }
    mine
}

/// Say where the player is whenever that changes: crossing a border brings
/// up the new country, region or town.
fn update(g: &mut Session, dt: f32, _c: &Controls, _audio: &mut Audio) {
    let st = state();
    for (_, out) in take_results(g, TAG_PLACE) {
        st.asked = false;
        let name = String::from_utf8_lossy(&out).into_owned();
        if !name.is_empty() && name != st.last {
            let verb = if st.last.is_empty() { "YOU ARE IN" } else { "ENTERING" };
            g.note = (alloc::format!("{} {}", verb, name.to_ascii_uppercase()), 5.0);
            g.push_chat(alloc::format!("{} {}", verb, name.to_ascii_uppercase()));
            st.last = name;
        }
    }
    let p = match proj() {
        Some(p) if st.level > 0 && crate::worldgen::shape() == 0 => p,
        _ => return,
    };
    st.timer -= dt;
    if st.timer <= 0.0 && !st.asked {
        // One question every twelve seconds at most: the service is a
        // shared, free one.
        st.timer = 12.0;
        st.asked = true;
        let (lat, lon) = p.block_to_latlon(g.player.pos[0], g.player.pos[2]);
        g.pool.submit(crate::jobs::Job::Task { tag: TAG_PLACE, arg: alloc::format!("{} {} {}", lat, lon, st.level).into_bytes(), run: task_place }, false);
    }
}

// ------------------------------------------------------------------ the map

fn map_open(g: &mut Session) -> bool {
    let p = match proj() {
        Some(p) => p,
        None => return false,
    };
    let size = (256u64 << p.zoom) as f64;
    let (px, py) = p.block_to_px(g.player.pos[0], g.player.pos[2]);
    let st = state();
    st.center = (px / size, py / size);
    st.zoom = (p.zoom as i32 - 2).clamp(2, 16);
    retry_failed();
    true
}

/// The world map: drag to move, wheel to zoom, right click to travel there.
fn map_screen(ui: &mut Ui, g: &mut Session) -> bool {
    let s = ui.s;
    let (w, h) = (ui.w(), ui.h());
    ui.r.rect(0.0, 0.0, w, h, [14, 18, 26, 255]);
    let p = match proj() {
        Some(p) => *p,
        None => return false,
    };
    let st = state();
    // Zoom toward the middle of the screen.
    if ui.inp.wheel != 0 {
        st.zoom = (st.zoom + ui.inp.wheel).clamp(2, 16);
    }
    let zoom = st.zoom as u32;
    let size = (256u64 << zoom) as f64;
    // Tiles are drawn at whole-number scale so they stay crisp.
    let scale = if h > 900.0 { 2.0 } else { 1.0 };
    let top = 27.0 * s;
    if ui.inp.down[1] && ui.inp.mouse.1 > top {
        st.center.0 -= ui.inp.moved.0 as f64 / scale as f64 / size;
        st.center.1 -= ui.inp.moved.1 as f64 / scale as f64 / size;
    }
    st.center.1 = st.center.1.clamp(0.0, 1.0);
    st.center.0 -= crate::math::floord(st.center.0) as f64;
    let (cx, cy) = (st.center.0 * size, st.center.1 * size);
    // Screen position of a map pixel.
    let to_screen = |px: f64, py: f64| (w * 0.5 + ((px - cx) * scale as f64) as f32, h * 0.5 + ((py - cy) * scale as f64) as f32);
    let n = 1i64 << zoom;
    let (tx0, tx1) = (crate::math::floord((cx - (w / scale) as f64 * 0.5) / 256.0) as i64, crate::math::floord((cx + (w / scale) as f64 * 0.5) / 256.0) as i64);
    let (ty0, ty1) = (crate::math::floord((cy - (h / scale) as f64 * 0.5) / 256.0) as i64, crate::math::floord((cy + (h / scale) as f64 * 0.5) / 256.0) as i64);
    let mut loading = 0;
    for ty in ty0.max(0)..=ty1.min(n - 1) {
        for tx in tx0..=tx1 {
            let key = (zoom, tx.rem_euclid(n) as u32, ty as u32);
            let (sx, sy) = to_screen(tx as f64 * 256.0, ty as f64 * 256.0);
            match st.tiles.get(&key) {
                Some(MapTile::Ready(tex)) => ui.r.image(*tex, sx, sy, 256.0 * scale, 256.0 * scale),
                Some(MapTile::Loading) => loading += 1,
                Some(MapTile::Missing) => ui.r.rect(sx + 1.0, sy + 1.0, 256.0 * scale - 2.0, 256.0 * scale - 2.0, [40, 30, 34, 255]),
                None => {
                    // Ask a worker to fetch it; at most a few at a time.
                    if loading < 4 {
                        g.pool.submit(crate::jobs::Job::Task { tag: TAG_TILE, arg: alloc::format!("{} {} {}", key.0, key.1, key.2).into_bytes(), run: task_tile }, true);
                        st.tiles.insert(key, MapTile::Loading);
                        loading += 1;
                    }
                }
            }
        }
    }
    // Finished tiles become textures.
    for (arg, rgba) in take_results(g, TAG_TILE) {
        let text = String::from_utf8_lossy(&arg).into_owned();
        let mut k = text.split(' ').filter_map(|v| v.parse::<u32>().ok());
        if let (Some(z), Some(x), Some(y)) = (k.next(), k.next(), k.next()) {
            let entry = if rgba.len() == 256 * 256 * 4 { MapTile::Ready(ui.r.add_texture(256, 256, &rgba)) } else { MapTile::Missing };
            st.tiles.insert((z, x, y), entry);
        }
    }
    // The player, as a marker.
    let k = size / (256u64 << p.zoom) as f64;
    let (px, py) = p.block_to_px(g.player.pos[0], g.player.pos[2]);
    let (sx, sy) = to_screen(px * k, py * k);
    ui.r.rect(sx - 4.0 * s, sy - 4.0 * s, 8.0 * s, 8.0 * s, [0, 0, 0, 255]);
    ui.r.rect(sx - 3.0 * s, sy - 3.0 * s, 6.0 * s, 6.0 * s, [255, 70, 60, 255]);
    // Where the mouse points, in the world.
    let (mx, my) = ui.inp.mouse;
    let (wx, wy) = (cx + ((mx - w * 0.5) / scale) as f64, cy + ((my - h * 0.5) / scale) as f64);
    let (bx, bz) = p.px_to_block(wx / k, wy / k);
    let (lat, lon) = p.block_to_latlon(bx, bz);
    ui.r.rect(0.0, 0.0, w, top, [10, 12, 18, 235]);
    let info = alloc::format!(
        "{:.3}{} {:.3}{}   BLOCK {:.0} {:.0}   ZOOM {}",
        if lat < 0.0 { -lat } else { lat },
        if lat < 0.0 { "S" } else { "N" },
        if lon < 0.0 { -lon } else { lon },
        if lon < 0.0 { "W" } else { "E" },
        bx,
        bz,
        zoom
    );
    ui.text(4.0 * s, 4.0 * s, &info, WHITE);
    ui.text(4.0 * s, 15.0 * s, "DRAG TO MOVE - WHEEL TO ZOOM - RIGHT CLICK TO TRAVEL THERE - M TO CLOSE", ui::DIM);
    ui.r.rect(0.0, h - 11.0 * s, w, 11.0 * s, [10, 12, 18, 200]);
    ui.text(4.0 * s, h - 9.0 * s, "MAP DATA (C) OPENSTREETMAP CONTRIBUTORS - ELEVATION: MAPZEN / AWS TERRAIN TILES", ui::DIM);
    if ui.inp.clicked[3] && my > top {
        // Arrive in the air above the spot; survival players are set down
        // gently once the ground has loaded.
        g.travel_to(bx, bz);
        return false;
    }
    true
}

fn show(what: &str, g: &mut Session) -> bool {
    what == "map" && map_open(g)
}

// ----------------------------------------------------------------- settings

fn settings(ui: &mut Ui, st: &mut Settings, x: f32, y: f32, w: f32, h: f32, step: f32) -> (f32, bool) {
    let mut changed = false;
    let mut level = st.extra("place_notices").parse::<i32>().unwrap_or(1).clamp(0, 3);
    let label = alloc::format!("PLACE NOTICES  < {} >", ["OFF", "COUNTRIES", "REGIONS", "TOWNS"][level as usize]);
    if ui.stepper_text(x, y, w, h, &label, &mut level, 0, 3, 1) {
        st.set_extra("place_notices", &alloc::format!("{}", level));
        state().level = level;
        changed = true;
    }
    ui.text(x, y + step * 1.5 - 9.0 * ui.s, "MAP DATA RELAY (HOST:PORT). BLANK: AUTOMATIC", ui::DIM);
    let mut relay = String::from(st.extra("relay"));
    let before = relay.clone();
    ui.text_field(11, x, y + step * 1.5, w, h, &mut relay, 80, ui::any_char);
    if relay != before {
        st.set_extra("relay", &relay);
        map::set_relay(&relay);
        changed = true;
    }
    (2.5, changed)
}

// ------------------------------------------------------------------ serving

/// Other players' games may ask a server for map data it can reach and
/// they cannot: "TILE <elev|map> <z> <x> <y>" and "GEO <lat> <lon> <level>".
fn serve(w: &[&str]) -> Option<Vec<u8>> {
    match w.first() {
        Some(&"GEO") => {
            let f = |i: usize| w.get(i).and_then(|v| v.parse::<f64>().ok());
            match (f(1), f(2), w.get(3).and_then(|v| v.parse::<i32>().ok())) {
                (Some(lat), Some(lon), Some(level)) if (-85.0..=85.0).contains(&lat) && (-180.0..=180.0).contains(&lon) => map::serve_place_json(lat, lon, level.clamp(1, 3)),
                _ => None,
            }
        }
        Some(&"TILE") => {
            let layer = match w.get(1) {
                Some(&"elev") => Layer::Elevation,
                Some(&"map") => Layer::Map,
                _ => return None,
            };
            let n = |i: usize| w.get(i).and_then(|v| v.parse::<u32>().ok());
            match (n(2), n(3), n(4)) {
                (Some(z), Some(x), Some(y)) if z <= 16 && x < (1 << z) && y < (1 << z) => map::serve_tile((layer, z, x, y)),
                _ => None,
            }
        }
        _ => None,
    }
}

pub static NATIVE: Native = Native {
    name: "earth",
    lines: &["map"],
    help: HELP,
    start: Some(start),
    option: Some(option),
    option_kind: Some(option_kind),
    show: Some(show),
    check: Some(check),
    terrain_parse: Some(terrain_parse),
    terrain_open: Some(terrain_open),
    terrain_label: Some(terrain_label),
    terrain_form: Some(terrain_form),
    terrain_chosen: Some(terrain_chosen),
    update: Some(update),
    map_open: Some(map_open),
    map_screen: Some(map_screen),
    settings: Some(settings),
    serve: Some(serve),
    ..Native::NONE
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_kind_round_trips_and_nonsense_is_refused() {
        let text = kind_text(46.5, -121.75, 30, 40, false);
        assert_eq!(terrain_parse(&text), Some(Some(text.clone())));
        assert_eq!(crate::worldgen::WorldKind::parse(&text), Some(crate::worldgen::WorldKind::Custom(text)));
        assert_eq!(terrain_parse("earth 99 0 30 40"), Some(None));
        assert_eq!(terrain_parse("earth 1 2 0 40"), Some(None));
        assert_eq!(terrain_parse("moon 1 2"), None);
        assert_eq!(crate::worldgen::WorldKind::parse("earth 99 0 30 40"), None);
        // Saves from before the fifth word have roads and buildings on.
        assert_eq!(parse_kind("earth 1 2 30 40").map(|k| k.4), Some(true));
    }
}
