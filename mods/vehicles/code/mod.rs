//! The Vehicles mod's own code: things to ride in.
//!
//! The game knows nothing of vehicles. This is compiled in because it sits
//! in `mods/vehicles/code/`, and runs while a loaded pack says
//! `feature vehicles` (this mod's `pack.txt` does). Other mods then add
//! the vehicles themselves as `vehicle` sections in their packs (Boats,
//! Trains, Planes, Land Vehicles); the game keeps those sections as
//! written and this code reads them.
//!
//! What the game calls is the table at the end, [`NATIVE`].

mod physics;
pub use physics::{Crash, Vehicle, Vehicles};

use crate::audio::{Audio, Sound};
use crate::block::reg;
use crate::math::{floord, forward};
use crate::native::{Native, View};
use crate::render::Renderer;
use crate::session::{Controls, GameMode, Session};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

const NAME: &str = "vehicles";

/// What a vehicle travels on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Medium {
    Water,
    Rail,
    Land,
    Air,
}

/// A kind of vehicle, from a `vehicle` section of a pack:
///
/// ```text
/// vehicle <id>
///   name <display name>      model <model id>      icon <texture>
///   medium water | rail | land | air
///   speed <blocks per second>   height <blocks>   width <blocks>
///   seat <blocks>            how high the rider sits
///   storage                  carries a chest's worth of items
///   powered                  keeps going by itself once set moving
/// ```
#[derive(Clone, Debug)]
pub struct VehicleDef {
    pub id: String,
    /// The item that sets it down.
    pub item: usize,
    pub model: usize,
    pub medium: Medium,
    /// Top speed, blocks per second.
    pub speed: f32,
    /// Size in blocks; the model is scaled to `height`.
    pub height: f32,
    pub width: f32,
    /// Whether it carries a chest's worth of storage.
    pub storage: bool,
    /// Where the rider's eyes are, above the vehicle's base.
    pub seat: f32,
    /// How many can ride: one drives, the others join with `/passenger`.
    pub seats: u8,
    /// Keeps going by itself once set moving (a locomotive).
    pub powered: bool,
}

fn medium(word: &str) -> Option<Medium> {
    match word {
        "water" => Some(Medium::Water),
        "rail" => Some(Medium::Rail),
        "land" | "" => Some(Medium::Land),
        "air" => Some(Medium::Air),
        _ => None,
    }
}

fn read(r: &crate::block::Registry) -> (Vec<VehicleDef>, Vec<String>) {
    let (mut out, mut wrong) = (Vec::new(), Vec::new());
    for (k, sec) in r.sections.iter().enumerate().filter(|s| s.1.kind == "vehicle") {
        let model = match r.models.iter().position(|m| m.id == sec.word("model")) {
            Some(m) => m,
            None => {
                wrong.push(alloc::format!("{}: vehicle {}: unknown model `{}`", sec.at, sec.id, sec.word("model")));
                continue;
            }
        };
        let medium = match medium(sec.word("medium")) {
            Some(m) => m,
            None => {
                wrong.push(alloc::format!("{}: vehicle {}: medium must be water, rail, land or air", sec.at, sec.id));
                continue;
            }
        };
        for (key, _) in &sec.props {
            if !["name", "model", "icon", "category", "medium", "speed", "height", "width", "seat", "seats", "storage", "powered"].contains(&key.as_str()) {
                wrong.push(alloc::format!("{}: vehicle {}: unknown property `{}`", sec.at, sec.id, key));
            }
        }
        out.push(VehicleDef {
            id: sec.id.clone(),
            item: r.items.iter().position(|it| it.section == Some(k)).unwrap_or(0),
            model,
            medium,
            speed: sec.num("speed", 6.0, 0.5, 60.0),
            height: sec.num("height", 1.0, 0.2, 6.0),
            width: sec.num("width", 1.2, 0.2, 3.0),
            storage: sec.flag("storage"),
            seat: sec.num("seat", 0.9, 0.0, 5.0),
            seats: sec.num("seats", 1.0, 1.0, 16.0) as u8,
            powered: sec.flag("powered"),
        });
    }
    (out, wrong)
}

/// The kinds of vehicle the loaded packs define. Worked out once for each
/// loading of the content.
pub fn defs() -> &'static [VehicleDef] {
    static CACHE: AtomicPtr<(usize, Vec<VehicleDef>)> = AtomicPtr::new(core::ptr::null_mut());
    let r = reg();
    let key = r as *const crate::block::Registry as usize;
    if let Some(c) = unsafe { CACHE.load(Ordering::Acquire).as_ref() } {
        if c.0 == key {
            return &c.1;
        }
    }
    // (The old value is leaked on purpose: it may still be in use.)
    CACHE.store(Box::into_raw(Box::new((key, read(r).0))), Ordering::Release);
    defs()
}

fn check(r: &crate::block::Registry) -> Vec<String> {
    read(r).1
}

/// What this code keeps for the world being played.
#[derive(Default)]
pub struct State {
    pub v: Vehicles,
    /// A chain is in hand and this cart was clicked first.
    coupling: Option<usize>,
    /// Across a reload of the content: everything as text, and who rides.
    carried: Option<(String, Option<usize>)>,
    /// Cruise control: the vehicle being driven keeps full throttle (`/cruise`).
    cruise: bool,
}

fn path(g: &Session) -> String {
    alloc::format!("{}vehicles.txt", g.prefix)
}

fn open(g: &mut Session) {
    let mut st = State::default();
    if g.net.is_none() {
        if let Some(text) = crate::sys::read_file(&path(g)) {
            st.v.load_text(&String::from_utf8_lossy(&text));
        }
    }
    g.put_ext(NAME, Box::new(st));
}

fn save(g: &mut Session) {
    if g.net.is_some() {
        return;
    }
    if let Some(st) = g.ext_ref::<State>(NAME) {
        crate::sys::write_file(&path(g), st.v.save_text().as_bytes());
    }
}

/// Vehicles are kept by number of their kind, and a reload renumbers the
/// kinds: carry them across as text.
fn reload(g: &mut Session, before: bool) {
    let mut st = g.take_ext::<State>(NAME);
    if before {
        st.carried = Some((st.v.save_text(), st.v.riding));
    } else if let Some((text, riding)) = st.carried.take() {
        st.v = Vehicles::default();
        st.v.load_text(&text);
        st.v.riding = riding.filter(|&i| i < st.v.list.len());
    }
    g.put_ext(NAME, st);
}

/// Move everything; the one being ridden carries the player.
fn update(g: &mut Session, dt: f32, c: &Controls, audio: &mut Audio) {
    let mut st = g.take_ext::<State>(NAME);
    if st.v.riding.is_some() && (c.leave || g.dead) {
        st.v.riding = None;
        g.player.pos[1] += 0.6;
        g.player.vel = [0.0; 3];
    }
    let mut input = c.input;
    if st.cruise && st.v.riding.is_some() {
        input.forward = 1.0;
    } else {
        st.cruise = false;
    }
    if let Some(eye) = st.v.update(dt, &g.world, &input, (g.player.yaw, g.player.pitch)) {
        g.seat = Some(eye);
    }
    // A passenger rides along: placed at the seat, beside the driver's, without steering.
    if let Some(i) = st.v.passenger {
        if c.leave || g.dead || st.v.riding.is_some() || i >= st.v.list.len() {
            st.v.passenger = None;
            g.player.pos[1] += 0.6;
            g.player.vel = [0.0; 3];
        } else {
            let v = &st.v.list[i];
            let (fx, fz) = (-crate::math::sinf(v.body.yaw), -crate::math::cosf(v.body.yaw));
            let seat = defs()[v.kind].seat as f64;
            g.seat = Some([v.body.pos[0] + (-fz * 0.6) as f64, v.body.pos[1] + seat, v.body.pos[2] + (fx * 0.6) as f64]);
        }
    }
    // Hard crashes wreck the vehicle. Highest index first, so that removing one
    // does not move the others (see Vehicles::remove).
    let mut crashes = core::mem::take(&mut st.v.crashes);
    crashes.sort_by(|a, b| b.index.cmp(&a.index));
    for crash in crashes {
        explode(g, &mut st.v, crash, audio);
    }
    g.put_ext(NAME, st);
}

/// A wreck: the vehicle is gone, whoever was inside is thrown clear, and anyone
/// close by is hurt, more the faster it was going.
fn explode(g: &mut Session, v: &mut Vehicles, crash: Crash, audio: &mut Audio) {
    let riding = v.riding == Some(crash.index);
    v.remove(crash.index);
    let (dx, dy, dz) = (g.player.pos[0] - crash.pos[0], g.player.pos[1] - crash.pos[1], g.player.pos[2] - crash.pos[2]);
    let near = dx * dx + dy * dy + dz * dz < 16.0;
    if riding {
        g.player.pos = [crash.pos[0], crash.pos[1] + 1.0, crash.pos[2]];
        g.player.vel = [0.0; 3];
    }
    let hurt = 2 + (crash.speed / 3.0) as i32;
    if riding || near {
        g.damage(hurt, audio);
    }
    logf!("voxelrust: vehicle crash at {:.1} {:.1} {:.1}, speed {:.1}: wrecked{}", crash.pos[0], crash.pos[1], crash.pos[2], crash.speed, if riding || near { alloc::format!(", the player is hurt for {}", hurt) } else { String::new() });
    audio.play_named("thunder", 1.0);
}

/// Left click picks a vehicle up, right click gets in, and a vehicle item
/// in hand sets a new one down.
fn interact(g: &mut Session, c: &Controls, reach: f32, audio: &mut Audio) -> bool {
    let mut st = g.take_ext::<State>(NAME);
    let used = hands(g, &mut st, c, reach, audio);
    g.put_ext(NAME, st);
    used
}

/// Whether a vehicle locked to someone other than the player here may not be used.
fn locked_to_other(g: &Session, owner: &str) -> bool {
    !owner.is_empty() && owner != g.who
}

fn ask(g: &mut Session, text: &str) {
    g.send_ext(NAME, false, "ask", text.as_bytes());
}

fn hands(g: &mut Session, st: &mut State, c: &Controls, reach: f32, audio: &mut Audio) -> bool {
    let all = defs();
    let creative = g.mode == GameMode::Creative;
    let held = g.held();
    if let Some(i) = st.v.riding {
        // Riding: hands are busy steering. Sneak + right click opens the hold.
        if c.place_held && g.cooldown[1] <= 0.0 && c.input.descend && st.v.list.get(i).map_or(false, |v| all[v.kind].storage) {
            g.cooldown[1] = 0.4;
            g.opened_ext = Some((NAME, i));
        }
        return true;
    }
    let eye = g.player.eye();
    let dir = forward(g.player.yaw, g.player.pitch);
    if let Some((i, _)) = st.v.hit_test(eye, dir, 5.0).filter(|v| v.1 < reach) {
        if c.place_held && g.cooldown[1] <= 0.0 && held.map_or(false, |h| h.1.id == "chain") {
            // A chain joins rail carts into a train: click the one in
            // front, then the one to pull behind it.
            g.cooldown[1] = 0.4;
            if all[st.v.list[i].kind].medium != Medium::Rail {
                g.say("CHAINS ARE FOR RAIL CARTS");
            } else if st.v.list[i].leader.is_some() {
                st.v.list[i].leader = None;
                g.say("UNCHAINED");
            } else {
                match st.coupling.take().filter(|&f| f != i && f < st.v.list.len()) {
                    Some(front) => {
                        st.v.list[i].leader = Some(front);
                        if !creative {
                            g.inv.take_one(g.selected);
                        }
                        audio.play_named("rail", 1.0);
                        g.say("CHAINED");
                    }
                    None => {
                        st.coupling = Some(i);
                        g.say("NOW CLICK THE CART TO PULL BEHIND IT");
                    }
                }
            }
            return true;
        }
        if c.place_held && g.cooldown[1] <= 0.0 {
            g.cooldown[1] = 0.4;
            if c.input.descend && all[st.v.list[i].kind].storage {
                g.opened_ext = Some((NAME, i));
            } else if !locked_to_other(g, &st.v.list[i].owner) {
                st.v.riding = Some(i);
            } else {
                g.say("LOCKED TO ANOTHER PLAYER");
            }
            return true;
        }
        if c.break_held && g.cooldown[0] <= 0.0 {
            g.cooldown[0] = 0.4;
            if !g.runs_the_world() {
                ask(g, &alloc::format!("remove {}", i));
            }
            if let Some(v) = st.v.remove(i) {
                let item = all[v.kind].item;
                if !creative || g.inv.count(item as u16) == 0 {
                    g.inv.add(item as u16, 1);
                }
                for s in v.storage.slots.iter().flatten() {
                    g.inv.add(s.item, s.count);
                }
                audio.play(Sound::Pop, 0.8);
            }
            return true;
        }
        return false;
    }
    // A vehicle item in hand: set one down on the block aimed at, or on
    // water if that comes first.
    let kind = match held.and_then(|h| all.iter().position(|d| d.item == h.0)) {
        Some(k) if c.place_held && g.cooldown[1] <= 0.0 => k,
        _ => return false,
    };
    let water = reg().wk.water;
    let mut spot = g.target.as_ref().map(|h| ([h.prev[0] as f64 + 0.5, h.prev[1] as f64 + 0.05, h.prev[2] as f64 + 0.5], h.dist));
    let mut t = 0.5f32;
    while t < 6.0 && spot.map_or(true, |s| t < s.1) {
        let p = [eye[0] + (dir.x * t) as f64, eye[1] + (dir.y * t) as f64, eye[2] + (dir.z * t) as f64];
        if g.world.get(floord(p[0]), floord(p[1]), floord(p[2])) == water {
            spot = Some(([p[0], floord(p[1]) as f64 + 0.9, p[2]], t));
            break;
        }
        t += 0.2;
    }
    let pos = match spot {
        Some((pos, _)) => pos,
        None => return false,
    };
    g.cooldown[1] = 0.5;
    if g.runs_the_world() {
        st.v.place(kind, pos, g.player.yaw);
    } else {
        // The player who runs the vehicles sets it down for all.
        ask(g, &alloc::format!("place {} {} {} {} {}", all[kind].id, pos[0], pos[1], pos[2], g.player.yaw));
    }
    audio.play(Sound::Place, 1.0);
    if !creative {
        g.inv.take_one(g.selected);
    }
    true
}

/// In a shared game one player's program runs the empty vehicles: it tells
/// the others where everything is ("all"), and they ask it for changes
/// ("ask": `place ...`, `remove N`, `ride N x y z yaw`).
fn message(g: &mut Session, key: &str, text: &str) {
    let host = g.runs_the_world();
    let mut st = g.take_ext::<State>(NAME);
    match key {
        "all" if !host => st.v.apply_snapshot(text),
        "ask" if host => {
            let w: Vec<&str> = text.split(' ').collect();
            let num = |k: usize| w.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
            match (w.first().copied(), num(2), num(3), num(4), num(5)) {
                (Some("place"), Some(x), Some(y), Some(z), Some(yaw)) => {
                    if let Some(kind) = defs().iter().position(|v| Some(&v.id.as_str()) == w.get(1)) {
                        if st.v.list.len() < 200 {
                            st.v.place(kind, [x, y, z], yaw as f32);
                        }
                    }
                }
                (Some("ride"), Some(x), Some(y), Some(z), Some(yaw)) => {
                    if let Some(v) = num(1).and_then(|i| st.v.list.get_mut(i as usize)) {
                        v.body.pos = [x, y, z];
                        v.body.yaw = yaw as f32;
                        v.body.vel = [0.0; 3];
                    }
                }
                (Some("remove"), ..) => {
                    if let Some(i) = num(1) {
                        st.v.remove(i as usize);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    g.put_ext(NAME, st);
}

fn share(g: &mut Session) {
    let host = g.runs_the_world();
    let (all, riding) = match g.ext_ref::<State>(NAME) {
        Some(st) => (
            if host { Some(st.v.save_text()) } else { None },
            st.v.riding.and_then(|i| st.v.list.get(i).map(|v| (i, v.body.pos, v.body.yaw))),
        ),
        None => return,
    };
    if let Some(text) = all {
        g.send_ext(NAME, false, "all", text.as_bytes());
    }
    if let (false, Some((i, p, yaw))) = (host, riding) {
        ask(g, &alloc::format!("ride {} {} {} {} {}", i, p[0], p[1], p[2], yaw));
    }
}

fn storage(g: &mut Session, which: usize) -> Option<*mut crate::inventory::Inventory> {
    g.ext_mut::<State>(NAME).and_then(|st| st.v.list.get_mut(which)).map(|v| &mut v.storage as *mut _)
}

fn draw(g: &Session, r: &mut Renderer, view: &View) {
    let st = match g.ext_ref::<State>(NAME) {
        Some(st) => st,
        None => return,
    };
    let all = defs();
    for (i, v) in st.v.list.iter().enumerate() {
        let p = [(v.body.pos[0] - view.eye[0]) as f32, (v.body.pos[1] - view.eye[1]) as f32, (v.body.pos[2] - view.eye[2]) as f32];
        if p[0] * p[0] + p[2] * p[2] > 96.0 * 96.0 {
            continue;
        }
        // From inside, your own vehicle is only drawn in third person.
        if st.v.riding == Some(i) && !view.third_person {
            continue;
        }
        let d = match all.get(v.kind) {
            Some(d) => d,
            None => continue,
        };
        let l = ((view.light)(v.body.pos) * 255.0) as u8;
        let pose = crate::model::Pose { phase: v.phase, stride: 0.0, ..Default::default() };
        r.draw_model(d.model, &pose, p, v.body.yaw, d.height, [l, l, l]);
    }
}

/// `riding` is true in a vehicle too.
fn condition(g: &mut Session, words: &[&str]) -> Option<bool> {
    match words.first() {
        Some(&"driving") => Some(g.ext_ref::<State>(NAME).map_or(false, |st| st.v.riding.is_some())),
        _ => None,
    }
}

/// `/vehicle ID` sets one down two blocks ahead; `/board` gets into the
/// nearest one within reach; `/vehicles` says how many there are. (For scripts and for making packs.)
fn command(g: &mut Session, words: &[&str]) -> Option<String> {
    match words.first().copied() {
        Some("/vehicle") => {
            let kind = match defs().iter().position(|d| Some(&d.id.as_str()) == words.get(1)) {
                Some(k) => k,
                None => return Some(String::from("NO SUCH VEHICLE")),
            };
            let f = forward(g.player.yaw, 0.0);
            let pos = [g.player.pos[0] + (f.x * 2.5) as f64, g.player.pos[1] + 0.3, g.player.pos[2] + (f.z * 2.5) as f64];
            if g.runs_the_world() {
                let mut st = g.take_ext::<State>(NAME);
                st.v.place(kind, pos, g.player.yaw);
                g.put_ext(NAME, st);
            } else {
                // The player who runs the vehicles sets it down for all.
                ask(g, &alloc::format!("place {} {} {} {} {}", defs()[kind].id, pos[0], pos[1], pos[2], g.player.yaw));
            }
            Some(String::from("set down"))
        }
        // /lock and /unlock: the vehicle you are in, or the one you are looking at.
        Some(w @ ("/lock" | "/unlock")) => {
            let mut st = g.take_ext::<State>(NAME);
            let aimed = match st.v.riding {
                Some(i) => Some((i, 0.0)),
                None => st.v.hit_test(g.player.eye(), forward(g.player.yaw, g.player.pitch), 6.0),
            };
            let reply = match aimed {
                None => String::from("NO VEHICLE HERE"),
                Some((i, _)) if locked_to_other(g, &st.v.list[i].owner) => String::from("THAT ONE IS LOCKED TO ANOTHER PLAYER"),
                Some((i, _)) => {
                    st.v.list[i].owner = if w == "/lock" { g.who.clone() } else { String::new() };
                    String::from(if w == "/lock" { "LOCKED TO YOU" } else { "UNLOCKED" })
                }
            };
            g.put_ext(NAME, st);
            Some(reply)
        }
        // /passenger: ride along in the nearest vehicle that has a seat to spare. /leave gets out.
        Some("/passenger") => {
            let p = g.player.pos;
            let mut st = g.take_ext::<State>(NAME);
            let near = (0..st.v.list.len()).filter(|&i| {
                let b = st.v.list[i].body.pos;
                defs()[st.v.list[i].kind].seats > 1 && (b[0] - p[0]) * (b[0] - p[0]) + (b[1] - p[1]) * (b[1] - p[1]) + (b[2] - p[2]) * (b[2] - p[2]) < 36.0
            }).next();
            let reply = match near {
                None => String::from("NO VEHICLE WITH A FREE SEAT NEARBY"),
                Some(i) if locked_to_other(g, &st.v.list[i].owner) => String::from("LOCKED TO ANOTHER PLAYER"),
                Some(i) => {
                    st.v.riding = None;
                    st.v.passenger = Some(i);
                    String::from("RIDING ALONG")
                }
            };
            g.put_ext(NAME, st);
            Some(reply)
        }
        // /cruise: keep the vehicle you are driving at full throttle (it stops when you get out).
        Some("/cruise") => {
            let mut st = g.take_ext::<State>(NAME);
            let reply = if st.v.riding.is_some() {
                st.cruise = !st.cruise;
                String::from(if st.cruise { "CRUISE ON" } else { "CRUISE OFF" })
            } else {
                String::from("GET IN A VEHICLE FIRST")
            };
            g.put_ext(NAME, st);
            Some(reply)
        }
        Some("/vehicles") => Some(alloc::format!("{} here", g.ext_ref::<State>(NAME).map_or(0, |st| st.v.list.len()))),
        Some("/board") => {
            let p = g.player.pos;
            let mut st = g.take_ext::<State>(NAME);
            let near = (0..st.v.list.len()).filter(|&i| {
                let b = st.v.list[i].body.pos;
                (b[0] - p[0]) * (b[0] - p[0]) + (b[1] - p[1]) * (b[1] - p[1]) + (b[2] - p[2]) * (b[2] - p[2]) < 36.0
            }).next();
            st.v.riding = near.or(st.v.riding);
            g.put_ext(NAME, st);
            Some(String::from(if near.is_some() { "aboard" } else { "NOTHING TO BOARD" }))
        }
        _ => None,
    }
}

fn status(g: &Session) -> String {
    alloc::format!("{} vehicles", g.ext_ref::<State>(NAME).map_or(0, |st| st.v.list.len()))
}

pub static NATIVE: Native = Native {
    name: NAME,
    sections: &["vehicle"],
    files: &["vehicles.txt"],
    check: Some(check),
    open: Some(open),
    save: Some(save),
    reload: Some(reload),
    update: Some(update),
    interact: Some(interact),
    command: Some(command),
    condition: Some(condition),
    message: Some(message),
    share: Some(share),
    storage: Some(storage),
    status: Some(status),
    draw: Some(draw),
    ..Native::NONE
};
