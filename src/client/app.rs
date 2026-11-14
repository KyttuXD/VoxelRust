//! The program: start-up, the main loop, input, and every screen.

use crate::audio::{Audio, Sound};
use crate::block::{self, reg, Kind, Registry};
use crate::chunk::CH;
use crate::inventory::{Stack, HOTBAR, SLOTS};
use crate::math::{floord, sinf, PI, TAU};
use crate::player::Input;
use crate::render::{Camera, Color, Env, Renderer, WHITE};
use crate::session::{self, Controls, GameMode, NewWorld, Session, MAX_HEALTH};
use crate::settings::{Action, Settings, ACTIONS};
use crate::sys::{self, gl, sdl, thread};
use crate::ui::{self, Ui, UiInput};
use crate::worldgen::WorldKind;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

const USAGE: &str = "\
VoxelRust - a voxel sandbox for old and new machines

Run it with no options for the title screen. Options:

  --world NAME        open (or create) this world straight away
  --seed N            seed for a new world (a number or any word)
  --mode MODE         survival or creative, for a new world
  --join HOST[:PORT]  join a multiplayer server straight away
  --host NAME         host the shared world NAME from inside the game, and play
                      in it; others join with this machine's address
  --server            run a multiplayer server instead of the game (no window);
                      uses --world, --seed, --port and --players
  --port N            server port (default 25590)
  --players N         most players at once on the server (default 16)
  --distance N        view distance in chunks, 2..48
  --far N             distant terrain out to N chunks, 0 = off (up to 256)
  --threads N         worker threads, 0 = none (default: one per spare core)
  --size WxH          window size (default 1280x720)
  --fullscreen        start in full screen
  --display N         open on display N (0 is the main one)
  --render-scale P    draw the 3D view at P percent of full resolution
                      (25..100); much faster on old graphics cards
  --novsync           do not wait for the display refresh
  --data DIR          where worlds and settings are kept
  --log FILE          also write what the game prints to FILE
  --pack FILE         extra content file (blocks, items, textures); repeatable
  --mods-dir DIR      read mods from DIR instead of the data folder's mods
  --serve-mods LIST   with --server (or --host): the mods players download,
                      comma separated (default: every mod without `client no`)
  --screenshot FILE   load the world, save one BMP frame, and exit
  --bench N           load the world, time N frames, and exit
  --flypass N         like --bench, but flying straight ahead over the land
                      for N frames; saves four pictures (flypass-1..4.bmp in
                      the data folder) and reports frames spent waiting for
                      terrain. A render test.
  --run 'CMDS'        with --screenshot / --bench: chat commands to carry out
                      first, separated by semicolons, for example
                      --run '/time night; /spawn wolf 3'
  --write-icon FILE   save the game's emblem as a PNG and exit
  --icon-size N       with --write-icon: its size in pixels (default 256)
  --version           print the version and which build this is, and exit
  --no-update         do not look for a newer version this time
  --add-mod PATH      copy a mod (a folder, a .zip or a pack file) into the
                      data folder's mods, and exit
  --script-check FILE read a mod's script (a .rs file, see docs/SCRIPTING.md) the way
                      the game would, say what is wrong with it, and exit
  --export FILE       with --world NAME: write that world as one file, and exit
  --texture-pack DIR  use a Minecraft-style texture pack folder (repeat for several, later ones win);
                      or list them in settings.txt as texture_pack=DIR|DIR
  --export-textures DIR   write every texture as a PNG file (and an atlas) in DIR, and exit.
                      Edit them and put them in the data folder's textures/ folder
  --import FILE       with --world NAME: add a world from such a file, and exit
  --pos X Y Z         start position      --look YAW PITCH   in degrees
  --time T            time of day, 0..1 (0.25 = noon)
  --hidden            do not show the window (with --screenshot / --bench)
  --showcase          with --screenshot: lay out every block in front of the
                      camera first (handy for checking a pack)
  --show LIST         with --screenshot: also draw hud, debug, pause, items,
                      title, worlds, newworld, multiplayer, dead, map,
                      settings:TAB, items:TAB (the creative list at that
                      tab), third (camera behind the player), creatures
                      (one of each) or walking (the same, mid-stride);
                      craft:ITEM (the crafting table with that item's recipe
                      laid out), looks:NAME (with creatures: every look of
                      the creatures with NAME in their id)

Controls (all can be changed in Settings): WASD move, mouse look, Space jump,
Ctrl sprint, Space twice to fly (creative), left click break, right click place, middle click pick,
1-9 or wheel select, E inventory, M map, T chat, F1 hide the HUD,
F2 screenshot, F3 details, F5 third-person view, F11 full screen, Esc menu.";

mod key {
    pub const N1: i32 = 30;
    pub const N9: i32 = 38;
    pub const ESCAPE: i32 = 41;
    pub const F1: i32 = 58;
    pub const F2: i32 = 59;
    pub const F3: i32 = 60;
    pub const F5: i32 = 62;
    pub const F11: i32 = 68;
}

#[derive(Default)]
struct Opts {
    world: Option<String>,
    seed: Option<u32>,
    mode: Option<GameMode>,
    join: Option<String>,
    host: Option<String>,
    server: bool,
    port: Option<u16>,
    players: Option<usize>,
    distance: Option<i32>,
    far: Option<i32>,
    threads: Option<usize>,
    size: Option<(i32, i32)>,
    display: i32,
    render_scale: Option<i32>,
    fullscreen: bool,
    novsync: bool,
    data: Option<String>,
    log: Option<String>,
    packs: Vec<String>,
    /// `--mods-dir DIR`: where the player's mods are (default `<data>/mods`).
    mods_dir: Option<String>,
    /// `--serve-mods a,b`: the mods a server sends to players.
    serve_mods: Option<String>,
    /// `--texture-pack DIR` (repeatable): texture pack folders to use this time.
    texture_packs: Vec<String>,
    screenshot: Option<String>,
    bench: Option<u32>,
    /// Fly straight ahead for this many frames, timing them (a render test).
    flypass: Option<u32>,
    /// Chat commands to carry out once the world has loaded.
    run: Option<String>,
    write_icon: Option<String>,
    no_update: bool,
    /// `--update-mirror URL`: fetch updates from this web folder (see the settings' `update_mirror`).
    update_mirror: Option<String>,
    import_mod: Option<String>,
    /// `--script-check FILE`: read a mod script and report its mistakes, and exit.
    script_check: Option<String>,
    icon_size: usize,
    version: bool,
    /// `--stamp FILE`: write which build this is to FILE, and exit (the
    /// updater runs a new program this way before installing it).
    stamp: Option<String>,
    /// `--update-now` (only in the update-test build): check and install at once.
    #[cfg_attr(not(feature = "update-test"), allow(dead_code))]
    update_now: bool,
    export: Option<String>,
    /// `--export-textures DIR`: write every texture as a PNG file, and exit.
    export_textures: Option<String>,
    /// `--world-op delete|copy|rename NAME [TO]`: change a world's files, and exit.
    world_op: Option<(String, String, String)>,
    import: Option<String>,
    pos: Option<[f64; 3]>,
    look: Option<(f32, f32)>,
    time: Option<f32>,
    /// `--wait-frames N`: with `--screenshot`, wait N more frames after the world settles (for remeshing).
    wait_frames: u32,
    /// `--step SECONDS`: in a scripted run, how much game time each frame lasts (default none: the game stands still).
    step: Option<f32>,
    hidden: bool,
    showcase: bool,
    show: String,
    help: bool,
}

/// Turn seed text into a number: plain numbers are used as they are, and
/// any other text is hashed.
fn seed_from(text: &str) -> u32 {
    text.parse::<u32>().unwrap_or_else(|_| text.bytes().fold(2_166_136_261u32, |h, b| (h ^ b as u32).wrapping_mul(16_777_619)))
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut i = 1;
    let next = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| alloc::format!("{} needs a value", flag))
    };
    fn num<T: core::str::FromStr>(s: &str, flag: &str) -> Result<T, String> {
        s.parse::<T>().map_err(|_| alloc::format!("{}: `{}` is not a valid number", flag, s))
    }
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--help" | "-h" => o.help = true,
            "--fullscreen" => o.fullscreen = true,
            "--novsync" => o.novsync = true,
            "--hidden" => o.hidden = true,
            "--showcase" => o.showcase = true,
            "--show" => o.show = next(&mut i, a)?,
            "--world" => o.world = Some(next(&mut i, a)?),
            "--seed" => o.seed = Some(seed_from(&next(&mut i, a)?)),
            "--mode" => {
                o.mode = Some(match next(&mut i, a)?.as_str() {
                    "survival" => GameMode::Survival,
                    "creative" => GameMode::Creative,
                    other => return Err(alloc::format!("--mode: `{}` should be survival or creative", other)),
                })
            }
            "--join" => o.join = Some(next(&mut i, a)?),
            "--host" => o.host = Some(next(&mut i, a)?),
            "--server" => o.server = true,
            "--port" => o.port = Some(num::<u16>(&next(&mut i, a)?, a)?),
            "--players" => o.players = Some(num::<usize>(&next(&mut i, a)?, a)?.clamp(1, 200)),
            "--distance" => o.distance = Some(num::<i32>(&next(&mut i, a)?, a)?.clamp(2, 48)),
            "--far" => o.far = Some(num::<i32>(&next(&mut i, a)?, a)?.clamp(0, 256)),
            "--threads" => o.threads = Some(num::<usize>(&next(&mut i, a)?, a)?.min(32)),
            "--size" => {
                let v = next(&mut i, a)?;
                let mut p = v.split(|c| c == 'x' || c == 'X');
                match (p.next().and_then(|s| s.parse::<i32>().ok()), p.next().and_then(|s| s.parse::<i32>().ok())) {
                    (Some(w), Some(h)) if w >= 160 && h >= 120 => o.size = Some((w, h)),
                    _ => return Err(alloc::format!("--size: `{}` should look like 1280x720", v)),
                }
            }
            "--render-scale" => o.render_scale = Some(num::<i32>(&next(&mut i, a)?, a)?.clamp(25, 100)),
            "--display" => o.display = num::<i32>(&next(&mut i, a)?, a)?.clamp(0, 15),
            "--data" => o.data = Some(next(&mut i, a)?),
            "--log" => o.log = Some(next(&mut i, a)?),
            "--pack" => o.packs.push(next(&mut i, a)?),
            "--mods-dir" => o.mods_dir = Some(next(&mut i, a)?),
            "--serve-mods" => o.serve_mods = Some(next(&mut i, a)?),
            "--texture-pack" => o.texture_packs.push(next(&mut i, a)?),
            "--screenshot" => o.screenshot = Some(next(&mut i, a)?),
            "--write-icon" => o.write_icon = Some(next(&mut i, a)?),
            "--icon-size" => o.icon_size = num::<usize>(&next(&mut i, a)?, a)?.clamp(8, 1024),
            "--version" => o.version = true,
            "--stamp" => o.stamp = Some(next(&mut i, a)?),
            #[cfg(feature = "update-test")]
            "--update-now" => o.update_now = true,
            "--no-update" => o.no_update = true,
            "--update-mirror" => o.update_mirror = Some(next(&mut i, a)?),
            "--add-mod" => o.import_mod = Some(next(&mut i, a)?),
            "--script-check" => o.script_check = Some(next(&mut i, a)?),
            "--export" => o.export = Some(next(&mut i, a)?),
            "--export-textures" => o.export_textures = Some(next(&mut i, a)?),
            "--world-op" => {
                let op = next(&mut i, a)?;
                let name = next(&mut i, a)?;
                let to = if op == "delete" || op == "new" { String::new() } else { next(&mut i, a)? };
                o.world_op = Some((op, name, to));
            }
            "--import" => o.import = Some(next(&mut i, a)?),
            "--bench" => o.bench = Some(num::<u32>(&next(&mut i, a)?, a)?.max(1)),
            "--flypass" => {
                o.flypass = Some(num::<u32>(&next(&mut i, a)?, a)?.max(8));
                o.bench = o.flypass;
            }
            "--run" => o.run = Some(next(&mut i, a)?),
            "--exp" => crate::render::EXPERIMENT.store(num::<u32>(&next(&mut i, a)?, a)?, core::sync::atomic::Ordering::Relaxed),
            "--pos" => {
                let mut p = [0.0f64; 3];
                for v in p.iter_mut() {
                    *v = num::<f64>(&next(&mut i, a)?, a)?;
                }
                o.pos = Some(p);
            }
            "--look" => {
                let yaw = num::<f32>(&next(&mut i, a)?, a)?;
                let pitch = num::<f32>(&next(&mut i, a)?, a)?;
                o.look = Some((yaw * PI / 180.0, pitch.clamp(-89.0, 89.0) * PI / 180.0));
            }
            "--time" => o.time = Some(num::<f32>(&next(&mut i, a)?, a)?.clamp(0.0, 1.0)),
            "--wait-frames" => o.wait_frames = num::<u32>(&next(&mut i, a)?, a)?,
            "--step" => o.step = Some(num::<f32>(&next(&mut i, a)?, a)?.clamp(0.0, 0.1)),
            // Finder on old Mac OS X adds a process serial number when an
            // app is double-clicked; it is not for us.
            _ if a.starts_with("-psn_") => {}
            _ => {
                // An option of some mod's own code, perhaps.
                let mut rest = args[i + 1..].iter().cloned();
                let before = rest.len();
                match crate::native::all().iter().filter_map(|n| n.option).find_map(|option| option(a, &mut rest)) {
                    Some(Ok(())) => i += before - rest.len(),
                    Some(Err(e)) => return Err(e),
                    None => return Err(alloc::format!("unknown option `{}` (try --help)", a)),
                }
            }
        }
        i += 1;
    }
    Ok(o)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Title,
    Worlds,
    NewWorld,
    Settings,
    Game,
    Pause,
    Inventory,
    Dead,
    Map,
    /// The block map (M, or N for waypoints), on every world.
    FullMap,
    Join,
    /// Adding or editing one entry of the server list.
    AddServer,
    Chat,
    /// The crafting table's recipe list.
    Craft,
    /// A chest's contents, at this position.
    Chest,
    /// One world's options: export, rename, copy, delete.
    EditWorld,
    /// The file browser (see `client::picker`), opened from the settings.
    Picker,
    /// The emote wheel (see `client::emote_wheel`), over the game.
    Emote,
    /// Typing a web address to fetch a skin picture from (see `client::skinweb`).
    SkinUrl,
}

/// Which build this is: the name its download carries. An update must
/// only ever replace a program with one of the same build name, so a
/// machine running the build for old Macs is never handed the one for new.
pub const BUILD: &str = match option_env!("VOXELRUST_BUILD") {
    Some(name) => name,
    None => {
        if cfg!(target_os = "windows") {
            "windows-x86_64"
        } else if cfg!(target_os = "macos") {
            "macos-x86_64"
        } else if cfg!(target_arch = "powerpc") {
            "linux-powerpc"
        } else {
            "linux-x86_64"
        }
    }
};

/// The project's public server (port 25590, the default).
const PUBLIC_SERVER: &str = "195.62.47.178";

const TABS: [&str; 7] = ["VIDEO", "AUDIO", "CONTROLS", "GAME", "SKIN", "MODS", "VOICE"];

/// A small arrow on the minimap at (cx, cy), its tip pointing the way `yaw`
/// faces: a shaft of pixels and a head that widens back from the tip.
fn minimap_arrow(r: &mut Renderer, cx: f32, cy: f32, yaw: f32, s: f32, col: Color) {
    let (fx, fz) = (-crate::math::sinf(yaw), -crate::math::cosf(yaw));
    let (px, pz) = (-fz, fx);
    let mut dot = |along: f32, across: f32| {
        let (x, y) = (cx + fx * along * s + px * across * s, cy + fz * along * s + pz * across * s);
        r.rect(x - s * 0.5, y - s * 0.5, s, s, col);
    };
    for k in -2..=1 {
        dot(k as f32, 0.0);
    }
    dot(3.0, 0.0);
    for j in 1..=2 {
        let along = (3 - j) as f32;
        for a in -j..=j {
            dot(along, a as f32);
        }
    }
}

/// The player model and the first-person hand for a body model. `player` and
/// `player_slim` have their own hands; a mod's body (any other id) has none of
/// its own, so it gets the hand that goes with the arms (`slim`).
fn arm_models(body: &str, slim: bool) -> (&str, &'static str) {
    let built_in = body == "player" || body == "player_slim";
    let hand = if body == "player_slim" || (!built_in && slim) { "hand_slim" } else { "hand" };
    (body, hand)
}

/// The body model id for `player`: a mod's choice if one makes it, else the
/// game's own body for the arms (`slim`).
fn body_for(g: &Session, player: u16, slim: bool) -> String {
    crate::native::body_of(g, player).unwrap_or_else(|| String::from(if slim { "player_slim" } else { "player" }))
}

/// Our own player number in a shared game (0 when playing alone).
fn own_id(g: &Session) -> u16 {
    g.net.as_ref().map_or(0, |n| n.id)
}

/// The player's skin as 64 x 64 RGBA: a tinted copy of the default body, or
/// `skin.png` from the data folder. `None` if a custom skin cannot be used.
/// A temporary panel while the chunks around the player are still arriving.
fn draw_loading(r: &mut Renderer, w: f32, h: f32, s: f32, g: &Session, distance: i32) {
    let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
    let (done, total) = g.world.progress(cx, cz, 2, distance);
    r.rect(0.0, 0.0, w, h, [12, 14, 20, 170]);
    let title = "LOADING THE WORLD";
    r.label((w - Renderer::text_width(title, 2.0 * s)) * 0.5, h * 0.5 - 52.0 * s, 2.0 * s, title, WHITE);
    let (bw, bh) = (260.0 * s, 12.0 * s);
    let (x, y) = ((w - bw) * 0.5, h * 0.5 - 14.0 * s);
    r.rect(x - 2.0 * s, y - 2.0 * s, bw + 4.0 * s, bh + 4.0 * s, [10, 11, 16, 255]);
    r.rect(x, y, bw, bh, [40, 44, 60, 255]);
    let frac = if total == 0 { 0.0 } else { (done as f32 / total as f32).min(1.0) };
    r.rect(x, y, bw * frac, bh, [120, 200, 110, 255]);
    let line = alloc::format!("{} OF {} AREAS READY", done, total);
    r.label((w - Renderer::text_width(&line, s)) * 0.5, y + 22.0 * s, s, &line, WHITE);
}

/// The hand after a click in the creative list. `pick` is the stack under
/// the pointer when it is a palette cell that was clicked. An empty hand
/// takes that stack; a full hand is emptied (the item is thrown away); a
/// click over the list with something carried lets it go. A click that
/// picks an item up is not also a letting go, because `hand` is the hand
/// from before the click.
fn creative_hand(hand: Option<Stack>, pick: Option<Stack>, over_list: bool, clicked: bool) -> Option<Stack> {
    if !clicked {
        return hand;
    }
    if pick.is_some() {
        return if hand.is_some() { None } else { pick };
    }
    if over_list && hand.is_some() {
        return None;
    }
    hand
}

/// Picture kinds a skin can be chosen from (those `image::decode` reads).
const SKIN_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "tga"];

/// Read the picture at `path` and keep it as the custom skin (`skin.png` in
/// the data folder). The error is a message for the player.
fn import_skin(base: &str, path: &str) -> Result<(), String> {
    let bytes = sys::read_file(path).ok_or_else(|| String::from("COULD NOT READ THAT FILE"))?;
    let img = crate::image::decode(&bytes).ok_or_else(|| String::from("THAT IS NOT A PICTURE THE GAME CAN READ"))?;
    let px = crate::client::skinfile::from_image(&img)?;
    if sys::write_file(&alloc::format!("{}skin.png", base), &crate::png::encode(64, 64, &px)) {
        Ok(())
    } else {
        Err(String::from("COULD NOT SAVE THE SKIN IN THE DATA FOLDER"))
    }
}

fn skin_pixels(base: &str, name: &str, player: &str) -> Option<Vec<u8>> {
    if name == "custom" {
        let img = crate::png::decode(&sys::read_file(&alloc::format!("{}skin.png", base))?)?;
        return if img.w == 64 && img.h == 64 { Some(img.rgba) } else { None };
    }
    // `default` is one of the built-in skins, picked by the player's name; the others are chosen by name.
    // A built-in skin, or any skin a pack or mod brings a picture for (`skin_<name>`).
    // Decode the named skin once: it is both the test and the picture.
    let named = if name != "default" { crate::pack_entities::skin_image(name) } else { None };
    let known = named.is_some() || crate::settings::SKINS.iter().any(|s| *s == name);
    if let (true, Some(img)) = (known, named) {
        return Some(img.rgba);
    }
    Some(crate::pack_entities::skin_image(crate::pack_entities::default_skin_for(player))?.rgba)
}

/// Everything except the renderer and the frame's input, which are passed
/// alongside so that screens can borrow them freely.
struct App {
    window: *mut c_void,
    audio: Audio,
    /// Proximity voice chat (client::voice): off unless on a server and switched on.
    voice: crate::client::voice::Voice,
    settings: Settings,
    settings_path: String,
    base: String,
    threads: usize,
    screen: Screen,
    session: Option<Session>,
    /// Which text field has the keyboard (0 = none).
    focus: u32,
    keys: [bool; 512],
    buttons: [bool; 4],
    pick: bool,
    toggle_fly: bool,
    leave: bool,
    drop_item: bool,
    /// When the jump key was last pressed, to notice a double tap.
    last_jump_tap: f64,
    quit: bool,
    fps: f32,
    /// The view distance in use; at most the setting, lower when holding a
    /// target frame rate.
    distance: i32,
    tune_timer: f32,
    show_debug: bool,
    help_timer: f32,
    /// When the world started waiting for its first chunks (0 once playable).
    loading_since: f64,
    /// The emote wheel's pointer and state.
    wheel: crate::client::emote_wheel::Wheel,
    /// Camera behind the player instead of in their head.
    third_person: bool,
    /// In the third-person view, from the front (the camera faces the player).
    third_front: bool,
    /// Drop the text typed by the key press that opened a text box.
    swallow_text: bool,
    /// F1: no HUD at all (for looking and for pictures).
    hud_hidden: bool,
    ui_clock: f32,
    // Forms.
    worlds: Vec<String>,
    form_name: String,
    form_seed: String,
    form_mode: GameMode,
    /// The terrain chosen is one a mod's code supplies: which.
    form_native: Option<usize>,
    form_flat: bool,
    form_address: String,
    /// World name typed for hosting, and the server thread while hosting.
    form_host: String,
    /// Servers the player has saved (name, address), and the name box.
    servers: Vec<(String, String)>,
    servers_loaded: bool,
    form_server: String,
    server_edit: Option<usize>,
    server_scroll: i32,
    host: Option<sys::thread::Thread>,
    /// This machine's network address, looked up once.
    my_address: Option<Option<String>>,
    form_chat: String,
    text_selected: bool,
    /// The player's own skin texture, other players' by id, and the PNG to
    /// send to a server.
    skin_tex: u32,
    /// The player name the built-in skin was picked for.
    skin_for: String,
    remote_skins: BTreeMap<u16, u32>,
    /// Which other players have slim arms.
    remote_slim: BTreeMap<u16, bool>,
    skin_png: Vec<u8>,
    skin_sent: bool,
    /// Pack files read at start-up, for reading again on request.
    pack_paths: Vec<String>,
    /// Where the player's mods are (see `--mods-dir`), without a trailing slash.
    mods_dir: String,
    /// `--serve-mods`: the mods a game hosted from the menu sends to players (None: all that allow it).
    serve_mods: Option<Vec<String>>,
    /// The packs in use, bundled; empty when there are none.
    pack_text: crate::client::pack_text::PackText,
    /// Playing with content a server sent rather than our own packs.
    server_content: bool,
    /// Set by the settings button or `/reload`; acted on between frames.
    want_reload: bool,
    /// The crafting screen was opened at an oven, not a table.
    at_oven: bool,
    show_walking: bool,
    /// `--show creatures,looks:NAME`: line up every look of the creatures whose id contains NAME instead.
    show_looks: Option<String>,
    mods_scroll: i32,
    /// Which sub-page of the settings tab is open (0 is the tab itself).
    video_detail: u8,
    form_mod_path: String,
    at_furnace: bool,
    /// The crafting grid and recipe book of the inventory and table screens.
    craft: crate::client::screens::crafting::CraftScreen,
    /// Seconds and frames counted while timing full screen.
    tune: (f32, u32),
    worlds_scroll: i32,
    /// The world the edit screen is for, its name as typed for a rename,
    /// and whether the next click on DELETE will delete it.
    edit_world: String,
    edit_name: String,
    edit_confirm: bool,
    /// The minimap's picture (client::fullmap).
    minimap: super::fullmap::MapPicture,
    /// The full map, once opened in this world (its view, toggles, picture).
    full: Option<super::fullmap::FullMap>,
    /// World file name typed on the worlds screen.
    form_import: String,
    /// The web address typed for a skin download.
    skin_url: String,
    picker: crate::client::picker::Picker,
    /// Name tags to draw this frame: screen position and text.
    tags: Vec<(f32, f32, String)>,
    /// The map screen: centre in "unit" map coordinates (0..1) and zoom.
    /// Whose screen the map key has opened (a mod's code).
    map_native: Option<usize>,
    status: String,
    tab: usize,
    /// Index into ACTIONS of the key being rebound.
    rebinding: Option<usize>,
    inv_tab: usize,
    inv_scroll: i32,
    /// What the creative list is searched for (empty: show the open tab).
    inv_search: String,
    chest_at: [i32; 3],
    /// When the chest screen shows storage a mod's code keeps instead
    /// of a chest: whose, and which.
    chest_ext: Option<(&'static str, usize)>,
    // Scripted runs (--screenshot / --bench).
    scripted: bool,
    show_hud: bool,
}

impl App {
    fn ui_scale(&self, r: &Renderer) -> f32 {
        if self.settings.gui_scale > 0 {
            self.settings.gui_scale as f32
        } else {
            (r.h / 360).max(1) as f32
        }
    }

    fn set_screen(&mut self, screen: Screen) {
        // Leaving the inventory: whatever the mouse carries goes back in.
        if matches!(self.screen, Screen::Inventory | Screen::Craft | Screen::Chest) && !matches!(screen, Screen::Inventory | Screen::Craft | Screen::Chest) {
            if let Some(s) = self.session.as_mut() {
                if let Some(c) = s.cursor.take() {
                    if s.mode == GameMode::Survival {
                        s.inv.add(c.item, c.count);
                    }
                }
            }
        }
        // Whatever lies in the crafting grid goes back in too.
        if self.screen != screen {
            match self.session.as_mut() {
                Some(s) => self.craft.put_away(&mut s.inv),
                None => self.craft = Default::default(),
            }
        }
        self.screen = screen;
        self.focus = 0;
        self.rebinding = None;
        self.buttons = [false; 4];
        if !self.scripted {
            unsafe { (sdl::sdl().SDL_SetRelativeMouseMode)((screen == Screen::Game || screen == Screen::Emote) as c_int) };
        }
    }

    fn apply_settings(&mut self) {
        let s = &self.settings;
        self.audio.set_volumes(s.master_volume, s.effects_volume, s.steps_volume, s.ui_volume);
        self.distance = self.distance.min(s.distance).max(2);
        if s.target_fps == 0 {
            self.distance = s.distance;
        }
        unsafe { (sdl::sdl().SDL_GL_SetSwapInterval)((s.vsync && !self.scripted) as c_int) };
    }

    /// (Re)build the player's skin texture from the settings.
    fn load_skin(&mut self, r: &mut Renderer) {
        self.skin_for = self.settings.player_name.clone();
        let who = crate::settings::clean_name(&self.settings.player_name, 16);
        let px = match skin_pixels(&self.base, &self.settings.skin, &who) {
            Some(px) => px,
            None => {
                self.status = String::from("NO USABLE SKIN.PNG (64 X 64) IN THE DATA FOLDER");
                self.settings.skin = String::from("default");
                match skin_pixels(&self.base, "default", &who) {
                    Some(px) => px,
                    None => return,
                }
            }
        };
        // The texture made the last time is written over, not left on the card.
        if self.skin_tex == 0 {
            self.skin_tex = r.add_texture(64, 64, &px);
        } else {
            r.update_texture(self.skin_tex, 64, 64, &px);
        }
        self.skin_png = crate::png::encode(64, 64, &px);
        self.skin_sent = false;
    }

    fn toggle_fullscreen(&mut self) {
        self.settings.fullscreen = !self.settings.fullscreen;
        self.apply_fullscreen();
    }

    /// The first time the game is played full screen, time it for a few
    /// seconds. A graphics card that cannot keep up at the desktop's own
    /// resolution (an old one driving a big display) is given a smaller
    /// full-screen resolution, which it draws far faster. Done once; the
    /// choice is the player's to change in the video settings.
    fn auto_tune(&mut self, real_dt: f32, width: i32) {
        let st = &self.settings;
        let ready = st.fullscreen && st.fullscreen_mode == 0 && !st.auto_tuned && !st.vsync && !self.scripted;
        let playing = self.screen == Screen::Game && self.session.as_ref().map_or(false, |g| g.world.settled);
        if !ready || !playing {
            self.tune = (0.0, 0);
            return;
        }
        self.tune.0 += real_dt;
        self.tune.1 += 1;
        if self.tune.0 < 4.0 {
            return;
        }
        let fps = self.tune.1 as f32 / self.tune.0;
        self.settings.auto_tuned = true;
        if fps < 170.0 && width > 1440 {
            // 1280 x 800.
            self.settings.fullscreen_mode = 2;
            self.apply_fullscreen();
            if let Some(g) = self.session.as_mut() {
                g.push_chat(String::from("FULL SCREEN NOW USES 1280 X 800, WHICH THIS GRAPHICS CARD DRAWS MUCH FASTER."));
                g.push_chat(String::from("CHANGE IT BACK IN SETTINGS > VIDEO IF YOU PREFER."));
            }
        }
        logf!("voxelrust: full screen timed at {:.0} FPS; resolution {}", fps, if self.settings.fullscreen_mode == 2 { "set to 1280x800" } else { "kept" });
        sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
    }

    /// Put the window in the full-screen state the settings ask for.
    fn apply_fullscreen(&mut self) {
        let api = sdl::sdl();
        let (w, h) = crate::settings::FULLSCREEN_MODES[self.settings.fullscreen_mode as usize];
        unsafe {
            (api.SDL_SetWindowFullscreen)(self.window, 0);
            if !self.settings.fullscreen {
                return;
            }
            if w == 0 {
                (api.SDL_SetWindowFullscreen)(self.window, sdl::WINDOW_FULLSCREEN_DESKTOP);
            } else {
                // The display switches to the mode nearest the window's size.
                (api.SDL_SetWindowSize)(self.window, w, h);
                (api.SDL_SetWindowFullscreen)(self.window, sdl::WINDOW_FULLSCREEN);
            }
        }
    }

    fn start_world(&mut self, r: &mut Renderer, name: &str, new: &NewWorld) {
        self.close_world(r);
        if self.server_content {
            // Back from a server: our own packs again.
            self.reload_packs(r);
        }
        let s = Session::open(&self.base, name, new, self.threads);
        logf!("voxelrust: world `{}` (seed {}), {} worker threads", s.name, s.world.seed, s.pool.thread_count());
        self.help_timer = if s.is_new { 12.0 } else { 0.0 };
        let mut s = s;
        s.who = crate::settings::clean_name(&self.settings.player_name, 16);
        self.session = Some(s);
        self.distance = self.settings.distance;
        self.set_screen(Screen::Game);
    }

    fn close_world(&mut self, r: &mut Renderer) {
        self.loading_since = 0.0;
        // The map's selection, name box and view belong to the world it was in.
        self.full = None;
        if let Some(mut s) = self.session.take() {
            // Leaving a server: its mods (downloaded for this visit only) go.
            if let Some(sync) = s.mod_sync.as_mut() {
                sync.end();
            }
            // Items lying in the crafting grid are saved with the inventory.
            self.craft.put_away(&mut s.inv);
            if !self.scripted && !s.save() {
                logf!("voxelrust: some files of world `{}` could not be written", s.name);
            }
            let Session { mut world, pool, .. } = s;
            drop(pool);
            world.free_meshes(r);
        }
        // Nothing of the world left may go on sounding.
        self.audio.silence();
        // A game hosted from the menu ends when its host leaves.
        if let Some(t) = self.host.take() {
            HOST_STOP.store(true, core::sync::atomic::Ordering::Relaxed);
            t.join();
            HOST_STOP.store(false, core::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Start a server inside this program and join it.
    fn host_game(&mut self, r: &mut Renderer) {
        self.close_world(r);
        let name = crate::settings::clean_world_name(&self.form_host, 24);
        let port = crate::proto::DEFAULT_PORT;
        let (server, edits) = match open_server(if name.is_empty() { "shared" } else { &name }, None, None, port, 16, &self.base, self.pack_text.bytes(read_mod_file), crate::client::mods::serving::served(&self.pack_paths, &self.settings.mods_off, self.serve_mods.as_deref()), &self.settings) {
            Ok(v) => v,
            Err(e) => {
                self.status = e.to_ascii_uppercase();
                return;
            }
        };
        let port = server.port();
        // The server holds raw sockets; it is handed to its thread whole
        // and never touched from here again.
        struct Parcel(crate::server::Server, String);
        unsafe impl Send for Parcel {}
        let parcel = Parcel(server, edits);
        self.host = sys::thread::spawn(alloc::boxed::Box::new(move || {
            let p = parcel;
            let _ = server_loop(p.0, p.1, true);
        }));
        if self.host.is_none() {
            self.status = String::from("COULD NOT START THE SERVER");
            return;
        }
        let player = crate::settings::clean_name(&self.settings.player_name, 16);
        match Session::join(&self.base, &alloc::format!("127.0.0.1:{}", port), if player.is_empty() { "Player" } else { &player }, self.threads) {
            Ok(mut g) => {
                let address = sys::net::local_address().unwrap_or_else(|| String::from("YOUR ADDRESS"));
                g.push_chat(alloc::format!("HOSTING. OTHERS JOIN WITH  {}:{}", address, port));
                g.push_chat(String::from("FROM OUTSIDE YOUR NETWORK, FORWARD THAT PORT ON YOUR ROUTER."));
                let mut g = g;
                g.who = crate::settings::clean_name(&self.settings.player_name, 16);
                self.session = Some(g);
                self.distance = self.settings.distance;
                self.set_screen(Screen::Game);
            }
            Err(e) => {
                self.status = e.to_ascii_uppercase();
                self.close_world(r);
            }
        }
    }

    fn save_screenshot(&mut self, r: &Renderer) {
        sys::make_dir(&alloc::format!("{}screenshots/", self.base));
        let path = alloc::format!("{}screenshots/screenshot-{}.bmp", self.base, (sys::now() * 1000.0) as u64);
        let ok = sys::write_file(&path, &r.screenshot());
        if let Some(s) = self.session.as_mut() {
            s.say(if ok { "SCREENSHOT SAVED" } else { "COULD NOT SAVE SCREENSHOT" });
        }
        if ok {
            logf!("screenshot: {}", path);
        }
    }

    // --------------------------------------------------------------- input

    fn key_down(&mut self, r: &Renderer, inp: &mut UiInput, sc: i32, repeat: bool) {
        if let Some(i) = self.rebinding.take() {
            if sc != key::ESCAPE {
                self.settings.keys[i] = sc;
            }
            return;
        }
        // Ctrl (Cmd on a Mac) with A, C, X or V works on the text box that
        // has the keyboard.
        let command = [224usize, 228, 227, 231].iter().any(|&k| self.keys[k]);
        if command && self.focus != 0 {
            match sc {
                4 => inp.keys.push(ui::KEY_SELECT_ALL),
                6 => inp.keys.push(ui::KEY_COPY),
                27 => inp.keys.push(ui::KEY_CUT),
                25 => inp.paste.push_str(&sys::clipboard()),
                _ => {}
            }
            return;
        }
        inp.keys.push(sc);
        if repeat {
            return;
        }
        match sc {
            key::F2 => return self.save_screenshot(r),
            key::F3 => {
                self.show_debug = !self.show_debug;
                return;
            }
            key::F11 => return self.toggle_fullscreen(),
            _ => {}
        }
        let typing = self.focus != 0;
        let (k_inv, k_fly, k_map) = (self.settings.key(Action::Inventory), self.settings.key(Action::Fly), self.settings.key(Action::Map));
        let k_waypoints = self.settings.key(Action::Waypoints);
        let k_chat = self.settings.key(Action::Chat);
        match self.screen {
            Screen::Game => {
                // Jump twice quickly to take off or land (creative flying).
                if sc == self.settings.key(Action::Jump) && self.session.as_ref().map_or(false, |g| g.mode == GameMode::Creative) {
                    let now = sys::now();
                    if now - self.last_jump_tap < 0.3 {
                        self.toggle_fly = true;
                        self.last_jump_tap = 0.0;
                    } else {
                        self.last_jump_tap = now;
                    }
                }
                if sc == key::ESCAPE {
                    self.set_screen(Screen::Pause);
                } else if sc == k_inv {
                    self.set_screen(Screen::Inventory);
                } else if sc == k_map {
                    self.open_map();
                } else if sc == k_waypoints {
                    self.open_full_map();
                } else if sc == self.settings.key(Action::Drop) {
                    self.drop_item = true;
                } else if sc == k_chat {
                    self.form_chat.clear();
                    self.set_screen(Screen::Chat);
                    self.focus = 20;
                    // The key that opened the box must not be typed into it.
                    inp.text.clear();
                    self.swallow_text = true;
                } else if sc == k_fly {
                    self.leave = true;
                } else if sc == self.settings.key(Action::Emote) && !typing && self.session.as_ref().map_or(false, |g| !g.dead) {
                    self.wheel.open();
                    self.set_screen(Screen::Emote);
                } else if sc == key::F5 {
                    // F5 cycles: first person, behind, in front.
                    if !self.third_person {
                        self.third_person = true;
                        self.third_front = false;
                    } else if !self.third_front {
                        self.third_front = true;
                    } else {
                        self.third_person = false;
                        self.third_front = false;
                    }
                } else if sc == key::F1 {
                    self.hud_hidden = !self.hud_hidden;
                } else if (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Inventory | Screen::Craft | Screen::Chest => {
                if sc == key::ESCAPE || (sc == k_inv && !typing) {
                    self.set_screen(Screen::Game);
                } else if !typing && (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Pause if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Chat if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Join if sc == key::ESCAPE && !typing => self.set_screen(Screen::Title),
            Screen::EditWorld if sc == key::ESCAPE && !typing => self.set_screen(Screen::Worlds),
            Screen::SkinUrl if sc == key::ESCAPE => self.set_screen(Screen::Settings),
            Screen::Picker if sc == key::ESCAPE => {
                self.close_picker(None);
            }
            Screen::AddServer if sc == key::ESCAPE => self.set_screen(Screen::Join),
            Screen::Map if sc == key::ESCAPE || sc == k_map => self.set_screen(Screen::Game),
            Screen::FullMap if !typing && (sc == key::ESCAPE || sc == k_map || sc == k_waypoints) => self.close_full_map(),
            Screen::Settings if sc == key::ESCAPE && !typing => self.leave_settings(),
            Screen::NewWorld if sc == key::ESCAPE => self.set_screen(Screen::Worlds),
            Screen::Worlds if sc == key::ESCAPE => self.set_screen(Screen::Title),
            _ => {}
        }
    }

    /// Open the file browser to choose a skin picture.
    fn open_skin_picker(&mut self) {
        let mut fallbacks: Vec<String> = Vec::new();
        if let Some(h) = crate::client::picker::home_dir() {
            fallbacks.push(crate::client::picker::join(&h, "Downloads"));
            fallbacks.push(h);
        }
        fallbacks.push(self.base.clone());
        let start = self.settings.picker_dir.clone();
        self.picker.open("CHOOSE A SKIN: A 64 X 64 OR 64 X 32 PICTURE", &start, &fallbacks, SKIN_EXTS, &self.base);
        self.picker.size_ok = Some(crate::client::skinfile::size_ok);
        self.picker.hint = String::from("SKINS: 64 X 64 OR 64 X 32");
        self.set_screen(Screen::Picker);
    }

    /// The file browser is done: remember where it was, and use the file if one was chosen.
    fn close_picker(&mut self, chosen: Option<String>) -> bool {
        self.settings.picker_dir = self.picker.dir.clone();
        if !self.scripted {
            sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
        }
        self.set_screen(Screen::Settings);
        if let Some(path) = chosen {
            match import_skin(&self.base, &path) {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("SKIN LOADED");
                    return true;
                }
                Err(e) => self.status = e,
            }
        }
        false
    }

    fn screen_picker(&mut self, ui: &mut Ui) {
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 9, 14, 235]);
        match self.picker.show(ui) {
            crate::client::picker::Outcome::Open => {}
            crate::client::picker::Outcome::Cancelled => {
                self.close_picker(None);
            }
            crate::client::picker::Outcome::Chosen(p) => {
                if self.close_picker(Some(p)) {
                    self.load_skin(ui.r);
                }
            }
        }
    }

    /// A file was dropped on the window: on the skin settings (or the web
    /// address panel) a picture becomes the custom skin.
    fn skin_dropped(&mut self, r: &mut Renderer, path: &str) {
        if !(self.screen == Screen::SkinUrl || (self.screen == Screen::Settings && self.tab == 4)) {
            return;
        }
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !SKIN_EXTS.contains(&ext.as_str()) {
            self.status = String::from("NOT A SKIN PICTURE");
        } else {
            match import_skin(&self.base, path) {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("SKIN LOADED");
                    self.load_skin(r);
                }
                Err(e) => self.status = e,
            }
        }
        if self.screen == Screen::SkinUrl {
            self.set_screen(Screen::Settings);
        }
    }

    /// Open the panel to type a web address for a skin picture.
    fn open_skin_url(&mut self) {
        crate::client::skinweb::reset();
        self.status.clear();
        self.set_screen(Screen::SkinUrl);
        self.focus = 40;
    }

    /// The panel itself: a text box, GO and BACK, and what the download is doing.
    fn screen_skin_url(&mut self, ui: &mut Ui) {
        use crate::client::skinweb as web;
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 190]);
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, "SKIN FROM A WEB ADDRESS", WHITE);
        ui.text(x, y - 9.0 * s, "ADDRESS OF A 64 X 64 OR 64 X 32 PICTURE", ui::DIM);
        let busy = web::state() == web::DOWNLOADING;
        let enter = ui.text_field(40, x, y, bw, bh, &mut self.skin_url, web::MAX_URL, ui::any_char);
        let half = bw * 0.5 - 2.0 * s;
        let go = ui.button(x, y + step * 1.5, half, bh, "DOWNLOAD") || enter;
        if ui.button(x + half + 4.0 * s, y + step * 1.5, half, bh, "BACK") {
            self.set_screen(Screen::Settings);
            return;
        }
        if go && !busy {
            web::start(&self.skin_url, &self.base, BUILD);
        }
        if web::take_ok() {
            let file = web::file(&self.base);
            let result = import_skin(&self.base, &file);
            sys::remove_file(&file);
            match result {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("IMPORTED");
                    self.set_screen(Screen::Settings);
                    self.load_skin(ui.r);
                    return;
                }
                Err(e) => self.status = e,
            }
        }
        let st = web::state();
        let text = if st == web::IDLE { self.status.clone() } else { String::from(web::message(st)) };
        let colour = if web::is_failure(st) { [255, 120, 100, 255] } else { ui::ACCENT };
        ui.centered(ui.w() * 0.5, y + step * 3.0, 1.0, &text, colour);
        ui.centered(ui.w() * 0.5, y + step * 4.2, 1.0, "PASTE WITH CTRL+V (CMD+V ON A MAC)", ui::DIM);
        ui.centered(ui.w() * 0.5, y + step * 5.0, 1.0, "A DIRECT LINK TO THE PICTURE, HTTP OR HTTPS", ui::DIM);
    }

    fn leave_settings(&mut self) {
        self.settings.player_name = crate::settings::clean_name(&self.settings.player_name, 16);
        if self.settings.player_name.is_empty() {
            self.settings.player_name = String::from("Player");
        }
        if !self.scripted {
            sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
        }
        self.set_screen(if self.session.is_some() { Screen::Pause } else { Screen::Title });
    }

    fn handle_events(&mut self, r: &mut Renderer, inp: &mut UiInput) {
        let api = sdl::sdl();
        let mut ev = sdl::Event::new();
        inp.clear_frame();
        while unsafe { (api.SDL_PollEvent)(&mut ev) } != 0 {
            match ev.u32_at(0) {
                sdl::EV_QUIT => self.quit = true,
                sdl::EV_WINDOW => match ev.u8_at(12) {
                    sdl::WINEV_SIZE_CHANGED => {
                        r.w = ev.i32_at(16).max(1);
                        r.h = ev.i32_at(20).max(1);
                    }
                    sdl::WINEV_FOCUS_LOST => {
                        if self.screen == Screen::Game && !self.scripted {
                            self.set_screen(Screen::Pause);
                        }
                        self.keys = [false; 512];
                    }
                    _ => {}
                },
                sdl::EV_KEYDOWN => {
                    let sc = ev.i32_at(16);
                    if (0..512).contains(&sc) {
                        self.keys[sc as usize] = true;
                        self.key_down(r, inp, sc, ev.u8_at(13) != 0);
                    }
                }
                sdl::EV_KEYUP => {
                    let sc = ev.i32_at(16);
                    if (0..512).contains(&sc) {
                        self.keys[sc as usize] = false;
                    }
                }
                sdl::EV_DROPFILE => {
                    // A picture dropped on the window: the pointer at offset 8, freed with SDL_free.
                    let p = ev.ptr_at(8) as *mut core::ffi::c_char;
                    if !p.is_null() {
                        let path = unsafe { sys::from_cstr(p as *const core::ffi::c_char) };
                        unsafe { (api.SDL_free)(p as *mut core::ffi::c_void) };
                        self.skin_dropped(r, &path);
                    }
                }
                sdl::EV_TEXTINPUT if self.swallow_text => self.swallow_text = false,
                sdl::EV_TEXTINPUT if [224usize, 228, 227, 231].iter().any(|&k| self.keys[k]) => {}
                sdl::EV_TEXTINPUT => {
                    // A NUL-terminated UTF-8 string at offset 12.
                    for &b in ev.0[12..44].iter().take_while(|&&b| b != 0) {
                        if b.is_ascii() {
                            inp.text.push(b as char);
                        }
                    }
                }
                sdl::EV_MOUSEMOTION => {
                    inp.mouse = (ev.i32_at(20) as f32, ev.i32_at(24) as f32);
                    inp.moved.0 += ev.i32_at(28) as f32;
                    inp.moved.1 += ev.i32_at(32) as f32;
                    if self.screen == Screen::Game && !self.scripted {
                        if let Some(g) = self.session.as_mut() {
                            let k = self.settings.sens as f32 * 0.0005;
                            let dy = if self.settings.invert_y { -ev.i32_at(32) } else { ev.i32_at(32) };
                            g.player.yaw -= ev.i32_at(28) as f32 * k;
                            g.player.pitch = (g.player.pitch - dy as f32 * k).clamp(-1.55, 1.55);
                            // Keep the angle small so precision never degrades.
                            if g.player.yaw > PI {
                                g.player.yaw -= TAU;
                            } else if g.player.yaw < -PI {
                                g.player.yaw += TAU;
                            }
                        }
                    }
                }
                sdl::EV_MOUSEDOWN => {
                    inp.mouse = (ev.i32_at(20) as f32, ev.i32_at(24) as f32);
                    let b = ev.u8_at(16) as usize;
                    if b < 4 {
                        if self.screen == Screen::Game {
                            self.buttons[b] = true;
                            if b == 2 {
                                self.pick = true;
                            }
                        } else {
                            inp.clicked[b] = true;
                            inp.down[b] = true;
                        }
                    }
                }
                sdl::EV_MOUSEUP => {
                    let b = ev.u8_at(16) as usize;
                    if b < 4 {
                        self.buttons[b] = false;
                        inp.down[b] = false;
                    }
                }
                sdl::EV_MOUSEWHEEL => {
                    let dy = ev.i32_at(20).signum();
                    if self.screen == Screen::Game {
                        if let Some(g) = self.session.as_mut() {
                            g.selected = (g.selected as i32 - dy).rem_euclid(HOTBAR as i32) as usize;
                        }
                    } else {
                        inp.wheel += dy;
                    }
                }
                _ => {}
            }
        }
    }

    // -------------------------------------------------------------- update

    /// Voice chat, once a frame: the microphone out, other players in.
    fn update_voice(&mut self, dt: f32) {
        let key = (self.settings.key(Action::Voice) as usize).min(511);
        let ptt = self.focus == 0 && self.keys[key] && matches!(self.screen, Screen::Game | Screen::Inventory | Screen::Craft | Screen::Chest | Screen::Map | Screen::FullMap);
        let meter = self.screen == Screen::Settings && self.tab == 6;
        let wire = self.session.as_mut().and_then(|g| {
            let allowed = g.perms.map_or(true, |f| f & crate::proto::PERM_VOICE != 0);
            let (pos, yaw) = (g.player.pos, g.player.yaw);
            g.net.as_mut().map(|client| crate::client::voice::Wire { client, pos, yaw, allowed })
        });
        self.voice.update(dt, &self.settings, ptt, wire, &mut self.audio, meter);
    }

    fn update(&mut self, r: &mut Renderer, dt: f32, real_dt: f32) {
        self.help_timer -= dt;
        self.ui_clock += real_dt;
        // In multiplayer the world never stops.
        let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
        let playing = online || matches!(self.screen, Screen::Game | Screen::Inventory | Screen::Dead | Screen::Chat | Screen::Map | Screen::FullMap | Screen::Craft | Screen::Chest | Screen::Emote);
        let in_game = self.screen == Screen::Game;
        let k = |a: Action| in_game && self.keys[(self.settings.key(a) as usize).min(511)];
        let controls = Controls {
            input: Input {
                forward: k(Action::Forward) as i32 as f32 - k(Action::Back) as i32 as f32,
                strafe: k(Action::Right) as i32 as f32 - k(Action::Left) as i32 as f32,
                jump: k(Action::Jump),
                descend: k(Action::Descend),
                sprint: k(Action::Sprint),
            },
            break_held: in_game && self.buttons[1],
            place_held: in_game && self.buttons[3],
            pick: in_game && self.pick,
            toggle_fly: self.toggle_fly,
            leave: self.leave,
            drop: self.drop_item,
        };
        self.pick = false;
        self.toggle_fly = false;
        self.leave = false;
        self.drop_item = false;
        let distance = self.distance;
        let mut died = false;
        if let Some(g) = self.session.as_mut() {
            // The player may move once the chunks right around them are in;
            // the rest of the world keeps streaming in behind them.
            if !g.playable {
                let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
                let (done, total) = g.world.progress(cx, cz, 2, distance);
                if total > 0 && done == total {
                    g.playable = true;
                    if self.loading_since > 0.0 {
                        logf!("voxelrust: playable {:.2} s after the world started", sys::now() - self.loading_since);
                    }
                    self.loading_since = 0.0;
                } else if self.loading_since == 0.0 {
                    self.loading_since = sys::now();
                }
            }
            if playing && g.playable {
                g.set_lod(self.settings.lod_real, self.settings.lod_trees, self.settings.lod_detail);
                g.update(dt, &controls, distance, self.settings.far_distance, r, &mut self.audio);
                if g.flags != self.settings.flags {
                    g.flags = self.settings.flags.clone();
                }
                g.weather_update(dt, self.settings.weather, &mut self.audio);
                died = g.dead;
            } else {
                // Paused: keep loading terrain, but nothing moves.
                let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
                g.world.update(cx, cz, distance, &mut g.pool, r, &mut g.store);
            }
        }
        // A new name may mean another of the built-in skins.
        if self.settings.skin != "custom" && self.skin_for != self.settings.player_name {
            self.load_skin(r);
        }
        if let Some(g) = self.session.as_mut() {
            if let (Some(net), false) = (g.net.as_mut(), self.skin_sent) {
                if self.skin_png.len() <= crate::proto::MAX_SKIN {
                    net.send_skin(&self.skin_png, self.settings.slim_arms);
                }
                self.skin_sent = true;
            }
            if let Some(pack) = g.server_pack.take() {
                // Play with the server's blocks and items, not our own.
                if pack.as_slice() != self.pack_text.bytes(read_mod_file) {
                    let new = server_content(&pack);
                    logf!("voxelrust: using the server's content ({} bytes of packs, {} block states)", pack.len(), new.blocks.len());
                    for w in &new.warnings {
                        logf!("voxelrust: server data: {}", w);
                    }
                    g.reload_content(new, r);
                    r.rebuild_atlas();
                    r.load_models();
                    self.pack_text = crate::client::pack_text::PackText::ready(pack);
                    self.server_content = true;
                    g.say("USING THE SERVER'S CONTENT");
                }
            }
            for (id, png, slim) in g.skin_events.drain(..) {
                self.remote_slim.insert(id, slim);
                if let Some(img) = crate::png::decode(&png).filter(|i| i.w == 64 && i.h == 64) {
                    // A player's new skin goes over their old picture on the card.
                    match self.remote_skins.get(&id) {
                        Some(&tex) => r.update_texture(tex, 64, 64, &img.rgba),
                        None => {
                            let tex = r.add_texture(64, 64, &img.rgba);
                            self.remote_skins.insert(id, tex);
                        }
                    }
                }
            }
        }
        self.audio.tick(real_dt);
        self.update_voice(real_dt);
        self.paint_minimap(r);
        if self.screen == Screen::FullMap {
            self.work_full_map(r);
        }
        if core::mem::take(&mut self.want_reload) {
            self.reload_packs(r);
        }
        if died && self.screen != Screen::Dead {
            self.set_screen(Screen::Dead);
        }
        if let Some(which) = self.session.as_mut().and_then(|g| g.opened_ext.take()) {
            self.chest_ext = Some(which);
            self.set_screen(Screen::Chest);
        }
        if let Some((what, at)) = self.session.as_mut().and_then(|g| g.opened.take()) {
            self.chest_ext = None;
            self.chest_at = at;
            self.inv_scroll = 0;
            self.at_oven = what == block::Usable::Oven;
            self.at_furnace = what == block::Usable::Furnace;
            self.craft.table_side = what.craft_side().unwrap_or(3);
            self.set_screen(if what == block::Usable::Chest { Screen::Chest } else { Screen::Craft });
        }
        // The chest being looked into has its lid up (game/chest_anim.rs).
        let looking = if self.screen == Screen::Chest && self.chest_ext.is_none() { Some(self.chest_at) } else { None };
        if let Some(g) = self.session.as_mut() {
            g.chest_view = looking;
        }
        if self.session.as_ref().map_or(false, |g| g.lost) {
            self.close_world(r);
            self.status = String::from("THE CONNECTION TO THE SERVER WAS LOST");
            self.set_screen(Screen::Join);
        }
        if !self.scripted {
            let every = self.settings.autosave_seconds as f32;
            if let Some(g) = self.session.as_mut() {
                if g.autosave_due(every) {
                    g.save();
                }
            }
            self.tune_distance(dt);
        }
    }

    /// Hold the target frame rate by trading view distance for speed.
    fn tune_distance(&mut self, dt: f32) {
        let target = self.settings.target_fps as f32;
        if target <= 0.0 || self.screen != Screen::Game {
            return;
        }
        self.tune_timer += dt;
        if self.tune_timer < 1.5 {
            return;
        }
        self.tune_timer = 0.0;
        if self.fps < target * 0.93 && self.distance > 3 {
            self.distance -= 1;
        } else if self.fps > target * 1.3 && self.distance < self.settings.distance {
            self.distance += 1;
        }
    }

    // ---------------------------------------------------------------- draw

    fn draw(&mut self, r: &mut Renderer, inp: &UiInput) {
        r.render_scale = self.settings.render_scale;
        let mut underwater = false;
        let mut new_tags = Vec::new();
        if let Some(g) = &self.session {
            let mut eye = g.player.eye();
            // Asleep: the view is from the eyes of the body lying in the bed, looking up.
            let lying = g.posing.lying.filter(|_| g.sleeping);
            if let Some(l) = lying {
                eye = crate::content::pose::sleep::eye_at(l.middle, l.yaw);
            }
            if self.settings.view_bobbing && g.player.on_ground && !g.player.flying && lying.is_none() {
                let speed = g.player.vel[0] * g.player.vel[0] + g.player.vel[2] * g.player.vel[2];
                if speed > 1.0 {
                    eye[1] += (sinf(g.clock * 11.0) * 0.035) as f64;
                }
            }
            if g.shake > 0.0 {
                // A `/shake` rattles the view.
                eye[0] += (sinf(g.clock * 47.0) * g.shake * 0.25) as f64;
                eye[1] += (sinf(g.clock * 61.0) * g.shake * 0.18) as f64;
            }
            if self.third_person {
                // Pull the camera back (or, from the front, out ahead), but not through walls.
                let back = crate::math::forward(g.player.yaw, g.player.pitch) * if self.third_front { 1.0 } else { -1.0 };
                let room = g.world.raycast(eye, back, 4.5).map_or(4.0, |hit| (hit.dist - 0.4).clamp(0.3, 4.0));
                eye = [eye[0] + (back.x * room) as f64, eye[1] + (back.y * room) as f64, eye[2] + (back.z * room) as f64];
            }
            // From the front the camera turns round to face the player.
            let (cam_yaw, cam_pitch) = if self.third_person && self.third_front {
                (g.player.yaw + PI, -g.player.pitch)
            } else if let (Some(l), false) = (lying, self.third_person) {
                (l.yaw, crate::content::pose::sleep::LOOK_UP)
            } else {
                (g.player.yaw, g.player.pitch)
            };
            let cam = Camera { pos: eye, yaw: cam_yaw, pitch: cam_pitch, fov: self.settings.fov as f32 };
            let head = g.world.get(floord(eye[0]), floord(eye[1]), floord(eye[2]));
            underwater = crate::block::is_water(head);
            let env = Env {
                time: g.time,
                distance: self.distance,
                clouds: self.settings.clouds,
                underwater,
                clock: g.clock,
                far_distance: if self.settings.far_distance > self.distance { self.settings.far_distance } else { 0 },
                rain: g.sky.rain,
                flash: g.flash,
                vision: crate::game::effects::apply::night_vision(&g.fx, &crate::game::effects::apply::lookup),
            };
            r.draw_world(&g.world, &cam, &env);
            let mut tags = Vec::new();
            self.draw_creatures(r, g, &cam, &mut tags);
            new_tags = tags;
            if let (Some(hit), true) = (&g.target, (!self.scripted || self.show_hud) && self.screen == Screen::Game) {
                let c = hit.cell;
                let b = g.world.get(c[0], c[1], c[2]);
                let mask = block::connect_mask(b, &|dx, dz| g.world.get(c[0] + dx, c[1], c[2] + dz));
                let mut shape = Vec::new();
                block::shape(b, mask, &mut shape);
                r.outline(&cam, c, &shape);
                if let Some((cell, p)) = g.mining.filter(|m| m.0 == c) {
                    r.crack(&cam, cell, &shape, p);
                }
            }
            if !underwater {
                r.draw_precipitation(&cam, g.sky.rain, g.sky.snow, g.clock);
            }
            if let Some(crate::session::Episode { x, z, .. }) = g.episode.filter(|d| reg().episodes.get(d.kind).map_or(false, |k| k.funnel)) {
                let ground = crate::worldgen::column(g.world.seed, x as i32, z as i32).height as f64;
                r.draw_funnel([(x as f64 - cam.pos[0]) as f32, (ground - cam.pos[1]) as f32, (z as f64 - cam.pos[2]) as f32], g.clock);
            }
            r.finish_world(&cam, &env);
            // The hand and what it holds, in their own close-up pass.
            // (While emoting only the arm shows, and only for emotes that move it; the item is put away.)
            let emoting = g.posing.emote.is_some();
            let emote_arm = !emoting || crate::content::pose::emote::shows_arm(&g.posing.body(g.activity()));
            if !self.third_person && self.screen == Screen::Game && !g.dead && !g.carried && lying.is_none() && emote_arm {
                let body = body_for(g, own_id(g), self.settings.slim_arms);
                if let Some(hand) = reg().model(arm_models(&body, self.settings.slim_arms).1) {
                    let v = g.player.vel;
                    let speed = crate::math::sqrtf(v[0] * v[0] + v[2] * v[2]);
                    let held = if emoting { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(g.held().map(|h| h.0)) };
                    let mut view = hand_view_of(g, held, speed);
                    if let Some((_, t)) = g.posing.emote {
                        // A wave: the hand swings to and fro.
                        view.swing = (0.4 + 0.35 * sinf(t * 9.0)).max(0.001);
                    }
                    let mut items = Vec::new();
                    crate::client::hand::first_person_view(held, &view, &mut items);
                    let to_view = |p: [f32; 3]| crate::client::hand::arm_point_view(p, &view, held);
                    r.draw_first_person(hand, self.skin_tex, cam.fov, 0.24, &to_view, &items);
                }
            }
        } else {
            r.clear([0.36, 0.56, 0.86]);
            crate::game::bow::show(None);
        }

        self.tags = new_tags;
        r.begin_2d();
        let (w, h) = (r.w as f32, r.h as f32);
        if self.session.is_none() {
            // Menus stand on a wall of the game's own blocks: stone below,
            // soil and turf above, dimmed so the buttons stand out, and
            // drifting slowly sideways.
            let tile = (h / 11.0).max(32.0);
            let wk = &reg().wk;
            let slid = self.ui_clock * 6.0;
            let drift = slid - ((slid / tile) as i32) as f32 * tile;
            let rows = (h / tile) as i32 + 1;
            let cols = (w / tile) as i32 + 2;
            for row in 0..rows {
                let (b, face) = match row {
                    0..=2 => (block::AIR, 0),
                    3 => (wk.grass, 1),
                    4 | 5 => (wk.dirt, 1),
                    _ => (wk.stone, 1),
                };
                if b == block::AIR {
                    continue;
                }
                let shade = (200 - row * 8).max(120) as u8;
                for col in 0..cols {
                    // A few ores in the rock, the same ones every time.
                    let roll = crate::noise::hash2(77, col + (self.ui_clock * 6.0 / tile) as i32, row);
                    let block = if row >= 6 && roll % 13 == 0 { [wk.coal_ore, wk.iron_ore, wk.copper_ore, wk.crystal_ore][(roll >> 8) as usize % 4] } else { b };
                    r.icon(col as f32 * tile - drift, row as f32 * tile, tile + 0.5, block::def(block).tex[face], [shade, shade, shade, 255]);
                }
            }
            // Evening light from above, and the corners drawn in.
            r.rect(0.0, 0.0, w, tile * 3.0, [255, 190, 120, 26]);
            r.rect(0.0, h * 0.55, w, h * 0.45, [0, 0, 10, 50]);
            r.rect(0.0, 0.0, w, h, [8, 10, 24, 60]);
        }
        let s = self.ui_scale(r);
        if underwater {
            r.rect(0.0, 0.0, w, h, [20, 70, 150, 90]);
        }
        if self.session.is_some() && (!self.scripted || self.show_hud) && !(self.hud_hidden && self.screen == Screen::Game) {
            self.draw_hud(r, w, h, s);
        }
        // A loading panel, but only when the wait is long enough to notice.
        if let Some(g) = self.session.as_ref() {
            if !g.playable && self.screen == Screen::Game && self.loading_since > 0.0 && sys::now() - self.loading_since > 0.35 {
                draw_loading(r, w, h, s, g, self.distance);
            }
            // The server's mods are still arriving.
            if let Some((done, total)) = g.mod_sync.as_ref().and_then(|m| m.progress()) {
                crate::client::mods::progress::draw(r, w, h, s, done, total, !g.playable);
            }
        }
        let before = self.screen;
        let mut focus = self.focus;
        let mut selected = self.text_selected;
        let clicked;
        {
            let mut ui = Ui { r: &mut *r, inp, s, focus: &mut focus, clicked_any: false, clock: self.ui_clock, selected: &mut selected };
            match self.screen {
                Screen::Game => {}
                Screen::Title => self.screen_title(&mut ui),
                Screen::Worlds => self.screen_worlds(&mut ui),
                Screen::EditWorld => self.screen_edit_world(&mut ui),
                Screen::NewWorld => self.screen_new_world(&mut ui),
                Screen::Settings => self.screen_settings(&mut ui),
                Screen::Pause => self.screen_pause(&mut ui),
                Screen::Inventory | Screen::Craft | Screen::Chest => self.screen_inventory(&mut ui),
                Screen::Dead => self.screen_dead(&mut ui),
                Screen::Map => self.screen_map(&mut ui),
                Screen::FullMap => self.screen_full_map(&mut ui),
                Screen::Join => self.screen_join(&mut ui),
                Screen::AddServer => self.screen_add_server(&mut ui),
                Screen::Chat => self.screen_chat(&mut ui),
                Screen::Picker => self.screen_picker(&mut ui),
                Screen::Emote => self.screen_emote(&mut ui),
                Screen::SkinUrl => self.screen_skin_url(&mut ui),
            }
            clicked = ui.clicked_any;
        }
        // Changing screen resets the focus; otherwise keep what the UI set.
        if self.screen == before && self.rebinding.is_none() {
            // A selection belongs to one box: moving to another drops it.
            self.text_selected = selected && focus == self.focus;
            self.focus = focus;
        }
        if clicked {
            self.audio.play(Sound::Click, 1.0);
        }
        r.end_2d();
    }

    /// Draw every creature near the camera, and the player's own body when
    /// the camera is outside it.
    fn draw_creatures(&self, r: &mut Renderer, g: &Session, cam: &Camera, tags: &mut Vec<(f32, f32, String)>) {
        let reg = reg();
        let (_, _, day) = crate::render::sky_colors(g.time);
        // Light at a spot: full daylight under open sky, dimmer under cover.
        let light_at = |pos: [f64; 3]| -> f32 {
            let (x, z) = (floord(pos[0]), floord(pos[2]));
            let covered = (floord(pos[1]) + 2..CH).any(|y| reg.opaque[g.world.get(x, y, z) as usize] != 0);
            (0.18 + 0.82 * day) * if covered { 0.55 } else { 1.0 }
        };
        let rel = |pos: [f64; 3]| [(pos[0] - cam.pos[0]) as f32, (pos[1] - cam.pos[1]) as f32, (pos[2] - cam.pos[2]) as f32];
        // What is behind the camera is not drawn at all.
        let ahead = crate::math::forward(cam.yaw, cam.pitch);
        let behind = |p: [f32; 3], size: f32| p[0] * ahead.x + p[1] * ahead.y + p[2] * ahead.z < -size;
        for m in &g.mobs.list {
            let p = rel(m.body.pos);
            if p[0] * p[0] + p[2] * p[2] > 72.0 * 72.0 || behind(p, 4.0) {
                continue;
            }
            // A creature whose ground is not loaded stands still in mid-air: not drawn.
            if !g.world.is_loaded(floord(m.body.pos[0]), floord(m.body.pos[2])) {
                continue;
            }
            let d = &reg.entities[m.kind];
            let (model, colour, height) = m.look();
            // Things that glow are not dimmed by the dark.
            let l = light_at(m.body.pos).max(d.x.glow as f32 / 15.0);
            let mut tint = [0, 1, 2].map(|i| (colour[i] as f32 * l) as u8);
            if m.hurt > 0.0 {
                tint = [255, tint[1] / 3, tint[2] / 3];
            }
            let mut pose = crate::model::Pose { phase: m.phase, stride: m.stride, anim: d.x.anim, clock: m.clock, ..Default::default() };
            // A person standing still on a seat sits; in a bed, sleeps.
            let kind = crate::client::pose_view::creature_posture(&|x, y, z| g.world.get(x, y, z), m.body.pos, m.stride);
            let mut at = p;
            if kind != crate::content::pose::Kind::Stand {
                pose.body = crate::content::pose::Body::steady(kind);
                at = rel(crate::client::pose_view::creature_at(kind, m.body.pos, m.body.yaw));
            }
            r.tex_override = r.entity_texture(m.tex());
            r.draw_model(model, &pose, at, m.body.yaw, height, tint);
        }
        // Props: blocks that are models (furniture, statues, plants). Only
        // those near enough to matter, and not the ones behind the camera.
        let (pcx, pcz) = crate::world::chunk_at(cam.pos[0], cam.pos[2]);
        let still = crate::model::Pose::default();
        for dx in -3..=3 {
            for dz in -3..=3 {
                let (cx, cz) = (pcx + dx, pcz + dz);
                let chunk = match g.world.chunks.get(&(cx, cz)) {
                    Some(c) if !c.props.is_empty() => c,
                    _ => continue,
                };
                for &(at, b) in &chunk.props {
                    let d = block::def(b);
                    let (model, height, lift) = match d.x.prop {
                        Some(p) => p,
                        None => continue,
                    };
                    let pos = [(cx * 16 + at[0] as i32) as f64 + 0.5, at[1] as f64 + lift as f64, (cz * 16 + at[2] as i32) as f64 + 0.5];
                    let p = rel(pos);
                    if p[0] * p[0] + p[2] * p[2] > 44.0 * 44.0 || behind(p, 3.0) {
                        continue;
                    }
                    // Placed facing the player: each state is a quarter turn.
                    let turn = ((b - d.first) % d.rotations.max(1) as u16) as f32;
                    let l = (light_at([pos[0], pos[1], pos[2]]) * 255.0) as u8;
                    r.draw_model(model, &still, p, turn * PI * 0.5, height, [l, l, l]);
                }
            }
        }
        let mut sparks: Vec<([f32; 3], [u8; 3])> = g.shots.iter().map(|s| (rel(s.pos), s.colour)).collect();
        // The sparks of fuses and explosions.
        sparks.extend(g.primed.sparks.iter().map(|s| (rel(s.pos), s.colour)));
        r.draw_sparks(&sparks);
        // Things lying on the ground are the things themselves, turning slowly.
        let white = crate::client::hand::white_tile();
        let mut lying = Vec::new();
        for d in &g.drops {
            let held = crate::client::hand::held_of(Some(d.item as usize));
            let spin = g.clock * 1.6 + d.pos[0] as f32;
            let bob = (sinf(g.clock * 2.5 + d.pos[2] as f32) * 0.04) as f32;
            crate::client::hand::dropped(held, white, rel([d.pos[0], d.pos[1] + 0.3, d.pos[2]]), spin + bob, &mut lying);
        }
        // Lit TNT, flashing white.
        crate::client::hand::primed_blocks(&g.primed.list, white, &rel, &mut lying);
        r.draw_quads(&lying, None);
        // Whatever the mods' own code has to show.
        {
            let view = crate::native::View { eye: cam.pos, yaw: cam.yaw, pitch: cam.pitch, third_person: self.third_person, light: &light_at };
            for (_, n) in crate::native::active() {
                if let Some(draw) = n.draw {
                    draw(g, r, &view);
                }
            }
        }
        // Other players, with their names over their heads.
        if let Some(net) = &g.net {
            for (id, p) in net.players.iter().filter(|p| p.1.seen) {
                let at = rel(p.pos);
                if at[0] * at[0] + at[2] * at[2] > 96.0 * 96.0 {
                    continue;
                }
                let slim = self.remote_slim.get(id).copied().unwrap_or(false);
                let body = body_for(g, *id, slim);
                let model = match reg.model(arm_models(&body, slim).0) {
                    Some(model) => model,
                    None => continue,
                };
                let l = (light_at(p.pos) * 255.0) as u8;
                let drawn = crate::client::pose_view::remote(p, g.clock);
                // A player whose skin has not arrived wears the built-in one their name picks.
                r.tex_override = match self.remote_skins.get(id) {
                    Some(t) => *t,
                    None => r.entity_texture(&crate::pack_entities::skin_picture(crate::pack_entities::default_skin_for(&p.name))),
                };
                r.draw_model(model, &drawn.pose, rel(drawn.at), drawn.yaw, crate::player::HEIGHT as f32, [l, l, l]);
                // What they hold, out to the right of the body (not while sitting, lying or emoting: the hand is elsewhere).
                let busy = drawn.pose.body.walk_weight() < 0.5 || drawn.pose.body.emote != 0;
                let held = if p.held == 0 || busy { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(Some(p.held as usize - 1)) };
                let mut held_quads = Vec::new();
                crate::client::hand::on_body(held, crate::client::hand::white_tile(), 0.0, at, p.yaw, crate::player::HEIGHT as f32, &mut held_quads);
                r.draw_quads(&held_quads, None);
                if let Some((sx, sy)) = r.project(cam, [at[0], at[1] + 2.15, at[2]]) {
                    tags.push((sx, sy, p.name.to_ascii_uppercase()));
                }
            }
        }
        if self.third_person {
            let body = body_for(g, own_id(g), self.settings.slim_arms);
            if let Some(model) = reg.model(arm_models(&body, self.settings.slim_arms).0) {
                let drawn = crate::client::pose_view::local(g);
                let l = (light_at(g.player.pos) * 255.0) as u8;
                r.tex_override = self.skin_tex;
                r.draw_model(model, &drawn.pose, rel(drawn.at), drawn.yaw, crate::player::HEIGHT as f32, [l, l, l]);
                let busy = drawn.pose.body.walk_weight() < 0.5 || drawn.pose.body.emote != 0;
                let held = if busy { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(g.held().map(|h| h.0)) };
                let mut held_quads = Vec::new();
                crate::client::hand::on_body(held, crate::client::hand::white_tile(), hand_swing(g), rel(g.player.pos), g.player.yaw, crate::player::HEIGHT as f32, &mut held_quads);
                r.draw_quads(&held_quads, None);
            }
        }
    }

    fn draw_hud(&mut self, r: &mut Renderer, w: f32, h: f32, s: f32) {
        let g = match &self.session {
            Some(g) => g,
            None => return,
        };
        if g.hurt_flash > 0.0 {
            r.rect(0.0, 0.0, w, h, [200, 20, 20, (g.hurt_flash * 300.0).min(110.0) as u8]);
        }
        if self.screen == Screen::Game {
            let c: Color = [255, 255, 255, 200];
            r.rect(w * 0.5 - 5.0 * s, h * 0.5 - 0.5 * s, 10.0 * s, s, c);
            r.rect(w * 0.5 - 0.5 * s, h * 0.5 - 5.0 * s, s, 10.0 * s, c);
            if let Some((_, p)) = g.mining {
                let bw = 40.0 * s;
                r.rect(w * 0.5 - bw * 0.5 - s, h * 0.5 + 11.0 * s, bw + 2.0 * s, 5.0 * s, [0, 0, 0, 150]);
                r.rect(w * 0.5 - bw * 0.5, h * 0.5 + 12.0 * s, bw * p.clamp(0.0, 1.0), 3.0 * s, [250, 250, 250, 230]);
            }
            crate::game::hooks::draw_hud(g, r, w, h, s);
        }
        // A bow being drawn shows its drawn picture in the hotbar and the inventory too.
        {
            let held = g.held().map(|h| h.0);
            let draw = held.map_or(0.0, |i| g.bow.shown(i as u16));
            crate::game::bow::show(crate::game::bow::pull_frame(draw).and_then(|f| held.map(|i| (i, f))));
        }
        // Hotbar. Empty slots stay empty: an empty hand is a valid choice.
        let slot = 22.0 * s;
        let x0 = (w - HOTBAR as f32 * slot) * 0.5;
        let y0 = h - slot - 4.0 * s;
        r.rect(x0 - 2.0 * s, y0 - 2.0 * s, HOTBAR as f32 * slot + 4.0 * s, slot + 4.0 * s, [16, 18, 24, 170]);
        for i in 0..HOTBAR {
            let x = x0 + i as f32 * slot;
            draw_slot(r, x, y0, slot, s, g.inv.slots[i], false);
            if i == g.selected {
                r.frame(x - s, y0 - s, slot + 2.0 * s, slot + 2.0 * s, 2.0 * s, [250, 250, 250, 255]);
            }
        }
        let survival = g.mode == GameMode::Survival;
        if let Some((_, item)) = g.held() {
            let tw = Renderer::text_width(&item.name, s);
            r.label((w - tw) * 0.5, y0 - (if survival { 24.0 } else { 13.0 }) * s, s, &item.name, WHITE);
        }
        if survival {
            let hy = y0 - 12.0 * s;
            // Hearts, drawn from the heart pictures (green, dark, blue by effect).
            if !crate::client::effects_hud::hearts(r, g, x0, hy, s) {
                for i in 0..MAX_HEALTH / 2 {
                    heart(r, x0 + i as f32 * 9.0 * s, hy, s, (g.health - i * 2).clamp(0, 2));
                }
            }
            // Hunger, as ten drumstick-coloured pips on the right.
            for i in 0..crate::session::MAX_HUNGER / 2 {
                let x = x0 + HOTBAR as f32 * slot - (i + 1) as f32 * 9.0 * s;
                let fill = (g.hunger - i * 2).clamp(0, 2);
                r.rect(x + s, hy + s, 6.0 * s, 5.0 * s, [40, 26, 14, 210]);
                if fill > 0 {
                    r.rect(x + s, hy + s, if fill == 2 { 6.0 } else { 3.0 } * s, 5.0 * s, [214, 142, 62, 255]);
                }
            }
            if g.air < 9.9 {
                let hy = hy - 9.0 * s;
                let bubbles = (g.air + 0.99) as i32;
                for i in 0..bubbles.clamp(0, 10) {
                    let x = x0 + HOTBAR as f32 * slot - (i + 1) as f32 * 9.0 * s;
                    r.rect(x + s, hy + s, 6.0 * s, 6.0 * s, [120, 190, 255, 230]);
                    r.rect(x + 2.0 * s, hy + 2.0 * s, 2.0 * s, 2.0 * s, [230, 245, 255, 255]);
                }
            }
        }
        // Active effects, with their icons and the time left, top right (under the minimap).
        if matches!(self.screen, Screen::Game | Screen::Chat) && !g.fx.list.is_empty() {
            let under_map = if self.settings.minimap && self.minimap.tex != 0 { 70.0 * s } else { 0.0 };
            crate::client::effects_hud::list(r, g, w, 4.0 * s + under_map, s);
        }
        for (x, y, name) in &self.tags {
            let tw = Renderer::text_width(name, s);
            r.rect(x - tw * 0.5 - 2.0 * s, y - 2.0 * s, tw + 4.0 * s, 11.0 * s, [0, 0, 0, 110]);
            r.label(x - tw * 0.5, *y, s, name, WHITE);
        }
        // Voice chat: your microphone, and who is talking.
        {
            let names: Vec<String> = match g.net.as_ref() {
                Some(n) => self.voice.talking().filter_map(|id| n.players.get(&id)).map(|p| p.name.to_ascii_uppercase()).collect(),
                None => Vec::new(),
            };
            let shown: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
            crate::client::voice::hud::draw(r, s, h, self.voice.mic(), &shown, &self.tags);
        }
        // Chat and notices, newest at the bottom, fading out.
        let lines: Vec<&(String, f32)> = g.chat.iter().filter(|c| c.1 > 0.0 || self.screen == Screen::Chat).collect();
        for (i, (line, left)) in lines.iter().rev().enumerate() {
            let a = if self.screen == Screen::Chat { 255 } else { (left * 255.0).clamp(0.0, 255.0) as u8 };
            let y = h - 52.0 * s - i as f32 * 10.0 * s;
            // "&c" and the like change colour and are not shown themselves.
            let plain: String = strip_colours(line);
            r.rect(2.0 * s, y - s, Renderer::text_width(&plain, s) + 4.0 * s, 10.0 * s, [0, 0, 0, a / 3]);
            label_coloured(r, 4.0 * s, y, s, line, a);
        }
        if g.note.1 > 0.0 {
            let tw = Renderer::text_width(&g.note.0, s * 2.0);
            r.label((w - tw) * 0.5, h * 0.3, s * 2.0, &g.note.0, [255, 240, 170, 255]);
        }
        // Subtitles: what was just heard, newest lowest, fading out.
        if self.settings.subtitles {
            for (i, (text, left)) in self.audio.captions.iter().rev().enumerate() {
                let t = text.to_ascii_uppercase();
                let tw = Renderer::text_width(&t, s);
                let a = (left * 200.0).clamp(0.0, 255.0) as u8;
                let y = h - 60.0 * s - i as f32 * 11.0 * s;
                r.rect(w - tw - 10.0 * s, y - 2.0 * s, tw + 6.0 * s, 10.0 * s, [0, 0, 0, a / 2]);
                r.label(w - tw - 7.0 * s, y, s, &t, [255, 255, 255, a]);
            }
        }
        // The minimap: north up, the player a dot with a nose.
        if self.settings.minimap && self.minimap.tex != 0 && matches!(self.screen, Screen::Game | Screen::Chat) {
            let size = 64.0 * s;
            let (mx, my) = (w - size - 4.0 * s, 4.0 * s);
            r.rect(mx - s, my - s, size + 2.0 * s, size + 2.0 * s, [16, 18, 24, 200]);
            r.image(self.minimap.tex, mx, my, size, size);
            // The minimap shows 2 blocks per texel, centred where its picture began.
            let per = size / (super::fullmap::MINIMAP_TEXELS as f32 * self.minimap.shown_step as f32);
            let to_map = |bx: f32, bz: f32| (mx + size * 0.5 + (bx - self.minimap.shown_origin.0 as f32) * per, my + size * 0.5 + (bz - self.minimap.shown_origin.1 as f32) * per);
            if self.settings.map_waypoints {
                for wp in &g.waypoints {
                    let (wx, wz) = to_map(wp.x as f32 + 0.5, wp.z as f32 + 0.5);
                    if wx > mx && wx < mx + size && wz > my && wz < my + size {
                        let c = crate::game::waypoints::COLOURS[wp.colour.min(7) as usize];
                        r.rect(wx - 1.5 * s, wz - 1.5 * s, 3.0 * s, 3.0 * s, [c[0], c[1], c[2], 255]);
                    }
                }
            }
            // Other players first, so the player's own arrow is on top.
            if let Some(net) = g.net.as_ref() {
                for p in net.players.values().filter(|p| p.seen) {
                    let (ox, oz) = to_map(p.pos[0] as f32, p.pos[2] as f32);
                    if ox > mx && ox < mx + size && oz > my && oz < my + size {
                        minimap_arrow(r, ox, oz, p.yaw, s, [110, 220, 255, 255]);
                    }
                }
            }
            let (cx, cy) = to_map(g.player.pos[0] as f32, g.player.pos[2] as f32);
            if cx > mx && cx < mx + size && cy > my && cy < my + size {
                minimap_arrow(r, cx, cy, g.player.yaw, s, [255, 255, 255, 255]);
            }
            let place = alloc::format!("{} {} {}", floord(g.player.pos[0]), floord(g.player.pos[1]), floord(g.player.pos[2]));
            let tw = Renderer::text_width(&place, s);
            r.label(mx + size - tw, my + size + 2.0 * s, s, &place, [230, 232, 240, 220]);
        }
        if self.settings.show_fps && !self.show_debug {
            r.label(4.0 * s, 4.0 * s, s, &alloc::format!("{:.0} FPS", self.fps), WHITE);
        }
        if self.show_debug {
            let p = g.player.pos;
            let st = r.stats;
            let lines = [
                alloc::format!("VOXELRUST {} ({})   {:.0} FPS", env!("CARGO_PKG_VERSION"), BUILD.to_ascii_uppercase(), self.fps),
                alloc::format!("POS {:.1} {:.1} {:.1}   CHUNK {} {}   FACING {}", p[0], p[1], p[2], floord(p[0]) >> 4, floord(p[2]) >> 4, ["NORTH (-Z)", "WEST (-X)", "SOUTH (+Z)", "EAST (+X)"][crate::interact::facing(g.player.yaw) as usize]),
                alloc::format!(
                    "BIOME {:?}   SEED {}   {}   {} CREATURES",
                    crate::worldgen::column(g.world.seed, floord(p[0]), floord(p[2])).biome,
                    g.world.seed,
                    if g.mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" },
                    g.mobs.list.len()
                )
                .to_ascii_uppercase(),
                match &g.target {
                    Some(hit) => alloc::format!("LOOKING AT {} ({} {} {})", block::def(g.world.get(hit.cell[0], hit.cell[1], hit.cell[2])).name, hit.cell[0], hit.cell[1], hit.cell[2]).to_ascii_uppercase(),
                    None => String::from("LOOKING AT NOTHING"),
                },
                alloc::format!(
                    "CHUNKS {} LOADED, {} DRAWN, {} QUADS, {} KB   VIEW {}",
                    g.world.chunks.len(),
                    st.chunks_drawn,
                    st.quads,
                    r.vbo_bytes / 1024,
                    self.distance
                ),
                alloc::format!("JOBS {} ON {} THREADS   TIME {:.2}", g.pool.in_flight, g.pool.thread_count(), g.time),
                gl::get_string(gl::RENDERER),
            ];
            for (i, l) in lines.iter().enumerate() {
                r.label(4.0 * s, 4.0 * s + i as f32 * 10.0 * s, s, l, WHITE);
            }
        }
    }

    // ------------------------------------------------------------- screens

    /// A centred column of full-width rows: (x, first y, width, row height, step).
    fn column(ui: &Ui, rows: usize) -> (f32, f32, f32, f32, f32) {
        let s = ui.s;
        // Below 1000 pixels wide the buttons widen, so their labels (and
        // the halves of rows) still fit at the small interface size.
        let wide = if ui.w() < 1000.0 { 1.6 } else { 1.0 };
        let (bw, bh, step) = (200.0 * s * wide, 16.0 * s, 19.0 * s);
        ((ui.w() - bw) * 0.5, ((ui.h() - rows as f32 * step) * 0.5 + 14.0 * s).max(40.0 * s), bw, bh, step)
    }

    fn screen_title(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, _, bw, bh, step) = Self::column(ui, 3);
        let cx = ui.w() * 0.5;
        let y = ui.h() * 0.5 + 20.0 * s;
        // The game's emblem above its name.
        let size = 64.0 * s;
        ui.r.emblem(cx - size * 0.5, y - 132.0 * s, size);
        ui.centered(cx, y - 60.0 * s, 4.0, "VOXELRUST", WHITE);
        if ui.button(x, y, bw, bh, "SINGLEPLAYER") {
            self.worlds = session::list_worlds(&self.base);
            self.status.clear();
            self.set_screen(Screen::Worlds);
        }
        if ui.button(x, y + step, bw, bh, "MULTIPLAYER") {
            self.status.clear();
            self.set_screen(Screen::Join);
            self.focus = 30;
        }
        if ui.button(x, y + step * 2.0, bw, bh, "SETTINGS") {
            self.set_screen(Screen::Settings);
        }
        if ui.button(x, y + step * 3.0, bw, bh, "QUIT") {
            self.quit = true;
        }
        let version = alloc::format!("VERSION {}  {}  {}", env!("CARGO_PKG_VERSION"), BUILD.to_ascii_uppercase(), crate::update::COMMIT.to_ascii_uppercase());
        ui.centered(cx, ui.h() - 12.0 * s, 1.0, &version, ui::DIM);
        // What the look for a newer version found.
        ui.centered(cx, ui.h() - 23.0 * s, 1.0, crate::update::message(), ui::ACCENT);
        // A credit in the bottom right corner, with a pixel heart (the
        // font has no emoji).
        let (a, b) = ("MADE WITH", "BY KYTTU");
        let (wa, wb) = (Renderer::text_width(a, s), Renderer::text_width(b, s));
        let heart = 7.0 * s;
        let total = wa + 4.0 * s + heart + 4.0 * s + wb;
        let x0 = ui.w() - 8.0 * s - total;
        let y0 = ui.h() - 12.0 * s;
        ui.text(x0, y0, a, ui::DIM);
        let hx = x0 + wa + 4.0 * s;
        // 7 x 6 pixels, one `s` square each.
        const HEART: [&str; 6] = [".XX.XX.", "XXXXXXX", "XXXXXXX", ".XXXXX.", "..XXX..", "...X..."];
        for (row, line) in HEART.iter().enumerate() {
            for (col, c) in line.chars().enumerate() {
                if c == 'X' {
                    ui.r.rect(hx + col as f32 * s, y0 - 0.5 * s + row as f32 * s, s, s, [228, 48, 78, 255]);
                }
            }
        }
        ui.text(hx + heart + 4.0 * s, y0, b, ui::DIM);
    }

    /// One world's options: export it, rename it, copy it, or delete it.
    fn screen_edit_world(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let name = self.edit_world.clone();
        let (x, y, bw, bh, _step) = Self::column(ui, 9);
        let side = 46.0 * s;
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, &name.to_ascii_uppercase(), WHITE);
        if ui.button(x, y, bw, bh, "EXPORT TO A FILE") {
            // One file holding the whole world, to copy to another machine.
            sys::make_dir(&alloc::format!("{}exports/", self.base));
            let path = alloc::format!("{}exports/{}.vxworld", self.base, name);
            self.status = match crate::save::export_world(&mut crate::save::DiskIo, &self.base, &name) {
                Some(data) if sys::write_file(&path, &data) => alloc::format!("WROTE {}", path.to_ascii_uppercase()),
                _ => String::from("COULD NOT EXPORT THAT WORLD"),
            };
        }
        // Each field has its label above it and room below, so nothing overlaps.
        ui.text(x, y + 46.0 * s, "NEW NAME (RENAME)", ui::DIM);
        ui.text_field(42, x, y + 56.0 * s, bw - side - 3.0 * s, bh, &mut self.edit_name, 24, ui::world_char);
        if ui.button(x + bw - side, y + 56.0 * s, side, bh, "RENAME") {
            let to = crate::settings::clean_world_name(&self.edit_name, 24);
            if to.is_empty() || to == name {
                self.status = String::from("TYPE A DIFFERENT NAME");
            } else if crate::session::world_exists(&self.base, &to) {
                self.status = String::from("A WORLD WITH THAT NAME EXISTS");
            } else if crate::session::rename_world(&self.base, &name, &to) {
                self.worlds = crate::session::list_worlds(&self.base);
                self.edit_world = to.clone();
                self.status = alloc::format!("RENAMED TO {}", to.to_ascii_uppercase());
            } else {
                self.status = String::from("COULD NOT RENAME THAT WORLD");
            }
        }
        ui.text(x, y + 92.0 * s, "COPY AS", ui::DIM);
        ui.text_field(43, x, y + 102.0 * s, bw - side - 3.0 * s, bh, &mut self.form_import, 24, ui::world_char);
        if ui.button(x + bw - side, y + 102.0 * s, side, bh, "COPY") {
            let to = crate::settings::clean_world_name(&self.form_import, 24);
            if to.is_empty() || to == name {
                self.status = String::from("TYPE A DIFFERENT NAME");
            } else if crate::session::copy_world(&self.base, &name, &to) {
                self.worlds = crate::session::list_worlds(&self.base);
                self.status = alloc::format!("COPIED AS {}", to.to_ascii_uppercase());
            } else {
                self.status = String::from("COULD NOT COPY THAT WORLD (THE NAME MAY BE TAKEN)");
            }
        }
        // Deleting takes two clicks, so one slip does not lose a world.
        let label = if self.edit_confirm { "CLICK AGAIN TO DELETE" } else { "DELETE THIS WORLD" };
        if ui.button(x, y + 140.0 * s, bw, bh, label) {
            if self.edit_confirm {
                if crate::session::delete_world(&self.base, &name) {
                    self.worlds = crate::session::list_worlds(&self.base);
                    self.status = alloc::format!("DELETED {}", name.to_ascii_uppercase());
                    self.set_screen(Screen::Worlds);
                    return;
                }
                self.status = String::from("COULD NOT DELETE THAT WORLD");
                self.edit_confirm = false;
            } else {
                self.edit_confirm = true;
                self.status = String::from("CLICK AGAIN TO DELETE IT FOR GOOD");
            }
        }
        if ui.button(x, y + 166.0 * s, bw, bh, "BACK") {
            self.edit_confirm = false;
            self.worlds = crate::session::list_worlds(&self.base);
            self.set_screen(Screen::Worlds);
        }
        ui.centered(ui.w() * 0.5, y + 192.0 * s, 1.0, &self.status, ui::ACCENT);
    }

    fn screen_worlds(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let shown = self.worlds.len().min(8);
        let (x, y, bw, bh, step) = Self::column(ui, shown.max(1) + 6);
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, "WORLDS", WHITE);
        let mut open = None;
        let mut edit = None;
        let side = 46.0 * s;
        // More worlds than fit: the wheel scrolls the list.
        let hidden = self.worlds.len().saturating_sub(8) as i32;
        self.worlds_scroll = (self.worlds_scroll - ui.inp.wheel).clamp(0, hidden);
        if hidden > 0 {
            ui.centered(ui.w() * 0.5, y - 9.0 * s, 1.0, "SCROLL FOR MORE", ui::DIM);
        }
        for (i, name) in self.worlds.iter().skip(self.worlds_scroll as usize).take(8).enumerate() {
            if ui.button(x, y + i as f32 * step, bw - side - 3.0 * s, bh, &name.to_ascii_uppercase()) {
                open = Some(name.clone());
            }
            if ui.button(x + bw - side, y + i as f32 * step, side, bh, "EDIT") {
                edit = Some(name.clone());
            }
        }
        if let Some(name) = edit {
            self.edit_name = name.clone();
            self.edit_world = name;
            self.edit_confirm = false;
            self.status.clear();
            self.set_screen(Screen::EditWorld);
            return;
        }
        if self.worlds.is_empty() {
            ui.centered(ui.w() * 0.5, y + 4.0 * s, 1.0, "NO WORLDS YET", ui::DIM);
        }
        let by = y + shown.max(1) as f32 * step + 6.0 * s;
        if ui.button(x, by, bw, bh, "CREATE A NEW WORLD") {
            self.form_name = alloc::format!("world{}", self.worlds.len() + 1);
            self.form_seed.clear();
            self.status.clear();
            self.set_screen(Screen::NewWorld);
        }
        ui.text(x, by + step + 5.0 * s, "IMPORT A .VXWORLD FILE (PATH)", ui::DIM);
        ui.text_field(30, x, by + step * 2.0, bw - side - 3.0 * s, bh, &mut self.form_import, 200, ui::any_char);
        if ui.button(x + bw - side, by + step * 2.0, side, bh, "IMPORT") {
            let path = String::from(self.form_import.trim());
            // The world is named after the file.
            let stem = path.rsplit(|c| c == '/' || c == '\\').next().unwrap_or("").trim_end_matches(".vxworld");
            let mut name = crate::settings::clean_world_name(stem, 24);
            if name.is_empty() {
                name = String::from("imported");
            }
            let mut n = 2;
            let first = name.clone();
            while crate::session::world_exists(&self.base, &name) {
                name = alloc::format!("{}-{}", first, n);
                n += 1;
            }
            // A bare name means a file in the data folder.
            let data = sys::read_file(&path).or_else(|| sys::read_file(&alloc::format!("{}{}", self.base, path))).or_else(|| sys::read_file(&alloc::format!("{}{}.vxworld", self.base, path)));
            self.status = match data {
                Some(d) if crate::save::import_world(&mut crate::save::DiskIo, &self.base, &name, &d) => {
                    crate::session::remember_world(&self.base, &name);
                    self.worlds = crate::session::list_worlds(&self.base);
                    alloc::format!("IMPORTED AS {}", name.to_ascii_uppercase())
                }
                Some(_) => String::from("THAT IS NOT A VOXELRUST WORLD FILE"),
                None => String::from("CANNOT READ THAT FILE"),
            };
        }
        if ui.button(x, by + step * 3.0, bw, bh, "BACK") {
            self.set_screen(Screen::Title);
        }
        let status = self.status.clone();
        ui.centered(ui.w() * 0.5, by + step * 4.2, 1.0, &status, ui::ACCENT);
        if let Some(name) = open {
            // The saved world decides its own seed, mode and kind.
            let new = NewWorld { seed: 0, mode: GameMode::Survival, kind: WorldKind::Normal };
            self.start_world(ui.r, &name, &new);
        }
    }

    fn screen_new_world(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, "NEW WORLD", WHITE);
        ui.text(x, y - 9.0 * s, "NAME", ui::DIM);
        ui.text_field(1, x, y, bw, bh, &mut self.form_name, 24, ui::world_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "SEED (EMPTY = RANDOM)", ui::DIM);
        ui.text_field(2, x, y + step * 1.5, bw, bh, &mut self.form_seed, 24, ui::any_char);
        let mode = alloc::format!("GAME MODE  {}", if self.form_mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
        if ui.button_lr(x, y + step * 2.7, bw, bh, &mode, true) != 0 {
            self.form_mode = if self.form_mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival };
        }
        // Normal, flat, and whatever terrain the mods' own code offers.
        let offered: Vec<(usize, &'static str)> = crate::native::active().filter_map(|(k, n)| n.terrain_label.and_then(|label| label()).map(|l| (k, l))).collect();
        let chosen = self.form_native.and_then(|k| offered.iter().position(|o| o.0 == k));
        if chosen.is_none() {
            self.form_native = None;
        }
        let kind = alloc::format!("TERRAIN  {}", chosen.map_or(if self.form_flat { "FLAT" } else { "NORMAL" }, |c| offered[c].1));
        let turn = ui.button_lr(x, y + step * 3.7, bw, bh, &kind, true);
        if turn != 0 {
            let now = chosen.map_or(self.form_flat as i32, |c| 2 + c as i32);
            let next = (now + turn).rem_euclid(2 + offered.len() as i32);
            self.form_flat = next == 1;
            self.form_native = if next >= 2 { Some(offered[next as usize - 2].0) } else { None };
        }
        // That code's own options for it, under the button.
        let form = self.form_native.and_then(|k| crate::native::all()[k].terrain_form);
        let rows = form.map_or(0.0, |form| form(ui, x, y + step * 4.7, bw, bh, step));
        let y = y + step * rows;
        if ui.button(x, y + step * 5.0, bw, bh, "CREATE") {
            let name = crate::settings::clean_world_name(&self.form_name, 24);
            if name.is_empty() {
                self.status = String::from("GIVE THE WORLD A NAME");
            } else if session::world_exists(&self.base, &name) {
                self.status = String::from("A WORLD WITH THAT NAME EXISTS");
            } else {
                let seed = if self.form_seed.trim().is_empty() {
                    (unsafe { (sdl::sdl().SDL_GetPerformanceCounter)() } as u32) ^ 0x5EED_1234
                } else {
                    seed_from(self.form_seed.trim())
                };
                let custom = self.form_native.and_then(|k| crate::native::all()[k].terrain_chosen).map(|chosen| chosen());
                let kind = if let Some(kind) = custom.and_then(|text| WorldKind::parse(&text)) {
                    kind
                } else if self.form_flat {
                    WorldKind::Flat
                } else {
                    WorldKind::Normal
                };
                self.start_world(ui.r, &name, &NewWorld { seed, mode: self.form_mode, kind });
                return;
            }
        }
        if ui.button(x, y + step * 6.0, bw, bh, "BACK") {
            self.set_screen(Screen::Worlds);
        }
        ui.centered(ui.w() * 0.5, y + step * 7.2, 1.0, &self.status, ui::ACCENT);
    }

    fn screen_pause(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 150]);
        let (x, y, bw, bh, step) = Self::column(ui, 3);
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, "PAUSED", WHITE);
        if ui.button(x, y, bw, bh, "BACK TO THE GAME") {
            self.set_screen(Screen::Game);
        }
        if ui.button(x, y + step, bw, bh, "SETTINGS") {
            self.set_screen(Screen::Settings);
        }
        if ui.button(x, y + step * 2.0, bw, bh, "SAVE AND QUIT TO TITLE") {
            self.close_world(ui.r);
            self.set_screen(Screen::Title);
        }
    }

    fn screen_dead(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [90, 8, 8, 150]);
        let (x, y, bw, bh, step) = Self::column(ui, 2);
        ui.centered(ui.w() * 0.5, y - 34.0 * s, 3.0, "YOU DIED", WHITE);
        let respawn = ui.button(x, y, bw, bh, "RESPAWN");
        let leave = ui.button(x, y + step, bw, bh, "SAVE AND QUIT TO TITLE");
        if respawn || leave {
            if let Some(g) = self.session.as_mut() {
                g.respawn();
            }
            if leave {
                self.close_world(ui.r);
            }
            self.set_screen(if leave { Screen::Title } else { Screen::Game });
        }
    }

    fn screen_settings(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 190]);
        let (_, y, _, bh, step) = Self::column(ui, 13);
        // Wider than the other menus: key names need the room.
        let bw = (360.0 * s).min(ui.w() - 8.0 * s);
        let x = (ui.w() - bw) * 0.5;
        let top = (y - 10.0 * s).max(24.0 * s);
        ui.centered(ui.w() * 0.5, top - 20.0 * s, 2.0, "SETTINGS", WHITE);
        let tw = bw / TABS.len() as f32;
        for (i, t) in TABS.iter().enumerate() {
            let tx = x + i as f32 * tw;
            if ui.button(tx, top, tw - 2.0 * s, bh, t) {
                self.tab = i;
                self.video_detail = 0;
                self.rebinding = None;
                *ui.focus = 0;
            }
            if i == self.tab {
                ui.r.rect(tx, top + bh, tw - 2.0 * s, 2.0 * s, ui::ACCENT);
            }
        }
        let y = top + bh + 8.0 * s;
        let row = |i: usize| y + i as f32 * step;
        let mut changed = false;
        let mut fullscreen_clicked = false;
        let mut mode_changed = false;
        let (mut skin_changed, mut skin_pick, mut skin_web) = (false, false, false);
        {
            let st = &mut self.settings;
            match self.tab {
                0 if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO VIDEO") {
                        self.video_detail = 0;
                    }
                    changed |= ui.stepper(x, row(1), bw, bh, "VIEW DISTANCE", &mut st.distance, 2, 48, 1);
                    changed |= ui.stepper(x, row(2), bw, bh, "DISTANT TERRAIN", &mut st.far_distance, 0, 256, 16);
                    let real = if st.lod_real == 0 { String::from("DISTANT BUILDS AND TREES  < OFF >") } else { alloc::format!("DISTANT BUILDS AND TREES  < {} TILES >", st.lod_real) };
                    changed |= ui.stepper_text(x, row(3), bw, bh, &real, &mut st.lod_real, 0, 8, 1);
                    let trees = ["NONE", "SOME", "ALL"][st.lod_trees.clamp(0, 2) as usize];
                    changed |= ui.stepper_text(x, row(4), half, bh, &alloc::format!("TREES  < {} >", trees), &mut st.lod_trees, 0, 2, 1);
                    let detail = ["COARSE", "MEDIUM", "FINE"][st.lod_detail.clamp(0, 2) as usize];
                    changed |= ui.stepper_text(x + half + 4.0 * s, row(4), half, bh, &alloc::format!("DETAIL  < {} >", detail), &mut st.lod_detail, 0, 2, 1);
                    let rs = alloc::format!("3D DETAIL  < {}% >", st.render_scale);
                    changed |= step_list(ui, x, row(5), bw, bh, &rs, &mut st.render_scale, &[33, 50, 67, 75, 100]);
                    changed |= ui.toggle(x, row(6), bw, bh, "CLOUDS", &mut st.clouds);
                }
                0 => {
                    if ui.button(x, row(0), bw, bh, "DETAIL AND DISTANCE...") {
                        self.video_detail = 1;
                    }
                    changed |= ui.stepper(x, row(1), bw, bh, "FIELD OF VIEW", &mut st.fov, 50, 110, 5);
                    let gs = if st.gui_scale == 0 { String::from("INTERFACE SIZE  < AUTO >") } else { alloc::format!("INTERFACE SIZE  < {} >", st.gui_scale) };
                    changed |= ui.stepper_text(x, row(2), bw, bh, &gs, &mut st.gui_scale, 0, 6, 1);
                    let tf = if st.target_fps == 0 { String::from("HOLD FRAME RATE  < OFF >") } else { alloc::format!("HOLD FRAME RATE  < {} >", st.target_fps) };
                    changed |= step_list(ui, x, row(3), bw, bh, &tf, &mut st.target_fps, &[0, 30, 60, 85, 120, 144, 250]);
                    let mf = if st.max_fps == 0 { String::from("FRAME RATE LIMIT  < NONE >") } else { alloc::format!("FRAME RATE LIMIT  < {} >", st.max_fps) };
                    changed |= step_list(ui, x, row(4), bw, bh, &mf, &mut st.max_fps, &[0, 30, 60, 85, 120, 144, 250, 500]);
                    changed |= ui.toggle(x, row(5), bw, bh, "VSYNC", &mut st.vsync);
                    changed |= ui.toggle(x, row(6), bw, bh, "SHOW FRAME RATE", &mut st.show_fps);
                    let half = (bw - 4.0 * s) * 0.5;
                    let mut fs = st.fullscreen;
                    fullscreen_clicked = ui.toggle(x, row(7), half, bh, "FULL SCREEN", &mut fs);
                    let (mw, mh) = crate::settings::FULLSCREEN_MODES[st.fullscreen_mode as usize];
                    let label = if mw == 0 { String::from("AT  < DESKTOP SIZE >") } else { alloc::format!("AT  < {}X{} >", mw, mh) };
                    mode_changed = ui.stepper_text(x + half + 4.0 * s, row(7), half, bh, &label, &mut st.fullscreen_mode, 0, crate::settings::FULLSCREEN_MODES.len() as i32 - 1, 1);
                }
                1 => {
                    changed |= ui.stepper(x, row(0), bw, bh, "MASTER VOLUME", &mut st.master_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(1), bw, bh, "EFFECTS", &mut st.effects_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(2), bw, bh, "FOOTSTEPS", &mut st.steps_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(3), bw, bh, "INTERFACE", &mut st.ui_volume, 0, 10, 1);
                    changed |= ui.toggle(x, row(5), bw, bh, "SUBTITLES FOR SOUNDS", &mut st.subtitles);
                    if !self.audio.available() {
                        ui.centered(ui.w() * 0.5, row(4) + 4.0 * s, 1.0, "NO SOUND DEVICE IS IN USE", ui::ACCENT);
                    }
                }
                2 if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO CONTROLS") {
                        self.video_detail = 0;
                    }
                    for (i, a) in ACTIONS.iter().enumerate() {
                        // Two columns of key bindings.
                        let (cx, cy) = (x + (i % 2) as f32 * (half + 4.0 * s), row(1 + i / 2));
                        let label = if self.rebinding == Some(i) {
                            String::from("PRESS A KEY")
                        } else {
                            alloc::format!("{}: {}", a.2, key_name(st.keys[i]))
                        };
                        if ui.button(cx, cy, half, bh, &label) {
                            self.rebinding = Some(i);
                        }
                    }
                    if ui.button(x, row(10), bw, bh, "RESET KEYS") {
                        st.keys = Settings::default().keys;
                    }
                }
                2 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    changed |= ui.stepper(x, row(0), half, bh, "MOUSE SPEED", &mut st.sens, 1, 20, 1);
                    changed |= ui.toggle(x + half + 4.0 * s, row(0), half, bh, "INVERT MOUSE", &mut st.invert_y);
                    if ui.button(x, row(1), bw, bh, "KEY BINDINGS...") {
                        self.video_detail = 1;
                    }
                }
                4 => {
                    // A turning preview on the left, choices on the right.
                    let body = match self.session.as_ref() {
                        Some(g) => body_for(g, own_id(g), st.slim_arms),
                        None => String::from(if st.slim_arms { "player_slim" } else { "player" }),
                    };
                    let model = reg().model(arm_models(&body, st.slim_arms).0);
                    ui.panel(x, row(0), 100.0 * s, 150.0 * s);
                    if let Some(model) = model {
                        let pose = crate::model::Pose { phase: ui.clock * 3.0, stride: 0.5, ..Default::default() };
                        ui.r.tex_override = self.skin_tex;
                        ui.r.draw_model_2d(model, &pose, x + 50.0 * s, row(0) + 138.0 * s, ui.clock * 0.6, 120.0 * s);
                    }
                    let (cx, cw) = (x + 108.0 * s, bw - 108.0 * s);
                    let list = crate::settings::SKINS;
                    let mut i = list.iter().position(|k| *k == st.skin).map_or(list.len() as i32, |i| i as i32);
                    let label = alloc::format!("LOOK  < {} >", st.skin.to_ascii_uppercase());
                    if ui.stepper_text(cx, row(0), cw, bh, &label, &mut i, 0, list.len() as i32, 1) {
                        st.skin = String::from(if (i as usize) < list.len() { list[i as usize] } else { "custom" });
                        skin_changed = true;
                    }
                    ui.text(cx, row(1) + 2.0 * s, "YOUR OWN: A 64 X 64 PICTURE IN", ui::DIM);
                    ui.text(cx, row(2) - 6.0 * s, "THE USUAL SKIN LAYOUT (64 X 32 TOO).", ui::DIM);
                    if ui.button(cx, row(3) - 4.0 * s, cw, bh, "CHOOSE A SKIN FILE...") {
                        skin_pick = true;
                    }
                    if ui.button(cx, row(4) - 2.0 * s, cw, bh, "IMPORT FROM WEB ADDRESS...") {
                        skin_web = true;
                    }
                    skin_changed |= ui.toggle(cx, row(5) + 4.0 * s, cw, bh, "SLIM ARMS (ALEX SKINS)", &mut st.slim_arms);
                }
                5 => {
                    // Every mod in the mods folder, with its version and a
                    // switch (changes take effect at once), then the ones
                    // the server sent for this session, which stay as they are.
                    let installed = all_mods(&self.pack_paths);
                    let sent: Vec<(String, String)> = self.session.as_ref().and_then(|g| g.mod_sync.as_ref()).map(|m| m.taken.iter().map(|t| (t.name.clone(), t.version.clone())).collect()).unwrap_or_default();
                    let list = crate::client::mods::rows::rows(&installed, &st.mods_off, &sent);
                    let hidden = list.len().saturating_sub(6) as i32;
                    self.mods_scroll = (self.mods_scroll - ui.inp.wheel).clamp(0, hidden);
                    ui.text(x, row(0) - 9.0 * s, if list.is_empty() { "NO MODS INSTALLED (SEE THE README IN THE MODS FOLDER)" } else { "MODS (PUT YOUR OWN IN THE MODS FOLDER)" }, ui::DIM);
                    for (k, m) in list.iter().skip(self.mods_scroll as usize).take(6).enumerate() {
                        let mut on = m.on;
                        let flipped = ui.toggle(x, row(k), bw, bh, &m.label(), &mut on);
                        if flipped && !m.from_server {
                            st.mods_off = crate::client::mods::rows::toggled(&st.mods_off, &m.name, on);
                            changed = true;
                            self.want_reload = true;
                        }
                    }
                    if hidden > 0 {
                        ui.text(x + bw - 70.0 * s, row(0) - 9.0 * s, "SCROLL FOR MORE", ui::DIM);
                    }
                    // Bring one in from elsewhere: a folder, a .zip or a pack file.
                    ui.text(x, row(7) - 9.0 * s, "ADD A MOD FROM A FOLDER OR .ZIP (TYPE ITS PATH)", ui::DIM);
                    ui.text_field(41, x, row(7), bw - 44.0 * s, bh, &mut self.form_mod_path, 200, ui::any_char);
                    if ui.button(x + bw - 41.0 * s, row(7), 41.0 * s, bh, "ADD") {
                        self.status = import_mod(&self.mods_dir, &self.form_mod_path);
                        if self.status.starts_with("ADDED") {
                            self.form_mod_path.clear();
                            self.want_reload = true;
                        }
                    }
                    let note = self.status.clone();
                    ui.text(x, row(8) + 2.0 * s, &note, ui::ACCENT);
                    changed |= ui.toggle(x, row(9), bw, bh, "UPDATE THE GAME AUTOMATICALLY", &mut st.auto_update);
                }
                6 => {
                    let at = ACTIONS.iter().position(|a| a.0 == Action::Voice).unwrap_or(0);
                    let label = if self.rebinding == Some(at) { String::from("PRESS A KEY") } else { key_name(st.keys[at]) };
                    let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
                    let allowed = self.session.as_ref().map_or(false, |g| g.perms.map_or(true, |f| f & crate::proto::PERM_VOICE != 0));
                    let asks = crate::client::voice::menu::draw(ui, st, &mut self.voice, (x, row(0), bw, bh, step), &label, online, allowed);
                    changed |= asks.changed;
                    if asks.rebind {
                        self.rebinding = Some(at);
                    }
                }
                _ if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO GAME") {
                        self.video_detail = 0;
                    }
                    changed |= ui.toggle(x, row(1), half, bh, "VIEW BOBBING", &mut st.view_bobbing);
                    changed |= ui.toggle(x + half + 4.0 * s, row(1), half, bh, "WEATHER", &mut st.weather);
                    changed |= ui.toggle(x, row(2), bw, bh, "MINIMAP", &mut st.minimap);
                }
                _ if self.video_detail == 2 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO GAME") {
                        self.video_detail = 0;
                    }
                    if ui.button(x, row(1), bw, bh, "RELOAD MODS NOW") {
                        self.want_reload = true;
                    }
                    // Switches the mods add (`setting` lines in their packs)...
                    let toggles = &reg().toggles;
                    for (k, (id, default, label)) in toggles.iter().take(4).enumerate() {
                        let mut on = st.flags.iter().find(|f| &f.0 == id).map_or(*default, |f| f.1);
                        let wide = if toggles.len() == 1 { bw } else { half };
                        if ui.toggle(x + (k % 2) as f32 * (half + 4.0 * s), row(2 + k / 2), wide, bh, label, &mut on) {
                            st.flags.retain(|f| &f.0 != id);
                            st.flags.push((id.clone(), on));
                            changed = true;
                        }
                    }
                    // ...and the rows their own code adds.
                    let mut at = row(2 + (toggles.len().min(4) + 1) / 2);
                    for (_, n) in crate::native::active() {
                        if let Some(rows) = n.settings {
                            let (used, touched) = rows(ui, st, x, at, bw, bh, step);
                            at += used * step;
                            changed |= touched;
                        }
                    }
                }
                _ => {
                    ui.text(x, row(0) + 4.0 * s, "PLAYER NAME", ui::DIM);
                    ui.text_field(10, x + 80.0 * s, row(0), bw - 80.0 * s, bh, &mut st.player_name, 16, ui::name_char);
                    changed |= ui.stepper(x, row(1), bw, bh, "AUTOSAVE EVERY (SECONDS)", &mut st.autosave_seconds, 10, 600, 10);
                    if let Some(g) = self.session.as_mut() {
                        let label = alloc::format!("GAME MODE  {}", if g.mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
                        if ui.button_lr(x, row(2), bw, bh, &label, true) != 0 {
                            g.set_mode(if g.mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival });
                        }
                    }
                    if ui.button(x, row(3), bw, bh, "VIEW, WEATHER AND MAP...") {
                        self.video_detail = 1;
                    }
                    if ui.button(x, row(4), bw, bh, "MOD SWITCHES AND RELOAD...") {
                        self.video_detail = 2;
                    }
                }
            }
        }
        if skin_pick {
            self.open_skin_picker();
        }
        if skin_web {
            self.open_skin_url();
        }
        if skin_changed {
            self.load_skin(ui.r);
        }
        if self.tab == 4 {
            let status = self.status.clone();
            ui.centered(ui.w() * 0.5, row(9), 1.0, &status, ui::ACCENT);
        }
        if fullscreen_clicked {
            self.toggle_fullscreen();
        } else if mode_changed && self.settings.fullscreen {
            self.apply_fullscreen();
        }
        if changed {
            self.apply_settings();
        }
        if ui.button(x, row(10) + 4.0 * s, bw, bh, "DONE") {
            self.leave_settings();
        }
    }

    fn save_servers(&self) {
        let text: String = self.servers.iter().map(|(n, a)| alloc::format!("{} {}\n", n, a)).collect();
        sys::write_file_atomic(&alloc::format!("{}servers.txt", self.base), text.as_bytes());
    }

    fn join_address(&mut self, r: &mut Renderer, address: &str) -> bool {
        let name = crate::settings::clean_name(&self.settings.player_name, 16);
        match Session::join(&self.base, address, if name.is_empty() { "Player" } else { &name }, self.threads) {
            Ok(g) => {
                self.close_world(r);
                let mut g = g;
                g.who = crate::settings::clean_name(&self.settings.player_name, 16);
                self.session = Some(g);
                self.distance = self.settings.distance;
                self.set_screen(Screen::Game);
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    /// The multiplayer screen: the servers you have added, as a list.
    fn screen_join(&mut self, ui: &mut Ui) {
        let s = ui.s;
        if !self.servers_loaded {
            // Saved servers: one "name address" per line.
            self.servers_loaded = true;
            self.servers = sys::read_file(&alloc::format!("{}servers.txt", self.base))
                .map(|b| String::from_utf8_lossy(&b).lines().filter_map(|l| l.rsplit_once(' ').map(|(n, a)| (String::from(n.trim()), String::from(a.trim())))).collect())
                .unwrap_or_default();
        }
        const ROWS: usize = 6;
        let (bw, bh, step) = (260.0 * s, 16.0 * s, 19.0 * s);
        let x = (ui.w() - bw) * 0.5;
        let y0 = ((ui.h() - (ROWS as f32 + 5.5) * step) * 0.5 + 14.0 * s).max(40.0 * s);
        ui.centered(ui.w() * 0.5, y0 - 30.0 * s, 2.0, "MULTIPLAYER", WHITE);
        ui.panel(x - 4.0 * s, y0 - 4.0 * s, bw + 8.0 * s, ROWS as f32 * step + 5.0 * s);
        let total = self.servers.len();
        if ui.hot(x, y0, bw, ROWS as f32 * step) {
            self.server_scroll -= ui.inp.wheel;
        }
        self.server_scroll = self.server_scroll.clamp(0, total.saturating_sub(ROWS) as i32);
        if total == 0 {
            ui.centered(ui.w() * 0.5, y0 + 2.2 * step, 1.0, "NO SERVERS YET", ui::DIM);
            ui.centered(ui.w() * 0.5, y0 + 3.0 * step, 1.0, "ADD ONE BELOW, OR HOST YOUR OWN", ui::DIM);
        }
        let (edit_w, del_w, bar) = (34.0 * s, 16.0 * s, 5.0 * s);
        let row_w = bw - edit_w - del_w - bar - 9.0 * s;
        let mut join: Option<String> = None;
        for row in 0..ROWS.min(total) {
            let k = row + self.server_scroll as usize;
            if k >= self.servers.len() {
                break;
            }
            let (name, address) = self.servers[k].clone();
            let ry = y0 + row as f32 * step;
            if ui.button(x, ry, row_w, bh, &alloc::format!("{}   {}", name, address).to_ascii_uppercase()) {
                join = Some(address.clone());
            }
            if ui.button(x + row_w + 3.0 * s, ry, edit_w, bh, "EDIT") {
                self.form_server = name;
                self.form_address = address;
                self.server_edit = Some(k);
                self.status.clear();
                self.set_screen(Screen::AddServer);
                self.focus = 30;
                return;
            }
            if ui.button(x + row_w + edit_w + 6.0 * s, ry, del_w, bh, "X") {
                self.servers.remove(k);
                self.save_servers();
                break;
            }
        }
        ui.scrollbar(x + bw - bar, y0, bar, ROWS as f32 * step - 3.0 * s, &mut self.server_scroll, ROWS, total);
        if let Some(address) = join {
            if self.join_address(ui.r, &address) {
                return;
            }
        }
        let y = y0 + ROWS as f32 * step + 12.0 * s;
        let half = bw * 0.5 - 2.0 * s;
        if ui.button(x, y, half, bh, "ADD SERVER") {
            self.form_server.clear();
            self.form_address.clear();
            self.server_edit = None;
            self.status.clear();
            self.set_screen(Screen::AddServer);
            self.focus = 33;
            return;
        }
        ui.text(x + half + 4.0 * s, y - 9.0 * s, "YOUR NAME", ui::DIM);
        ui.text_field(31, x + half + 4.0 * s, y, half, bh, &mut self.settings.player_name, 16, ui::name_char);
        // Hosting: a server runs inside the game for as long as you play.
        ui.text(x + half + 4.0 * s, y + step * 1.5 - 9.0 * s, "SHARED WORLD NAME", ui::DIM);
        ui.text_field(32, x + half + 4.0 * s, y + step * 1.5, half, bh, &mut self.form_host, 24, ui::world_char);
        if ui.button(x, y + step * 1.5, half, bh, "HOST A GAME") {
            self.status.clear();
            self.host_game(ui.r);
            if self.session.is_some() {
                return;
            }
        }
        if ui.button(x, y + step * 2.7, bw, bh, "BACK") {
            self.set_screen(Screen::Title);
        }
        // The project's own server, underneath everything else.
        if ui.button(x, y + step * 3.5, bw, bh, "PUBLIC SERVER") {
            self.status.clear();
            if self.join_address(ui.r, PUBLIC_SERVER) {
                return;
            }
        }
        ui.centered(ui.w() * 0.5, y + step * 4.4, 1.0, &self.status, ui::ACCENT);
        let mine = self.my_address.get_or_insert_with(sys::net::local_address).as_ref().map(|a| alloc::format!("YOUR ADDRESS ON THIS NETWORK: {}", a)).unwrap_or_default();
        ui.centered(ui.w() * 0.5, y + step * 5.2, 1.0, &mine, ui::DIM);
    }

    /// Add a server to the list, or change one already there.
    fn screen_add_server(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, if self.server_edit.is_some() { "EDIT SERVER" } else { "ADD SERVER" }, WHITE);
        ui.text(x, y - 9.0 * s, "SERVER NAME", ui::DIM);
        ui.text_field(33, x, y, bw, bh, &mut self.form_server, 24, ui::world_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "SERVER ADDRESS (HOST OR HOST:PORT)", ui::DIM);
        let enter = ui.text_field(30, x, y + step * 1.5, bw, bh, &mut self.form_address, 60, ui::any_char);
        if ui.inp.pressed(ui::KEY_TAB) {
            *ui.focus = if *ui.focus == 33 { 30 } else { 33 };
        }
        let half = bw * 0.5 - 2.0 * s;
        let save = ui.button(x, y + step * 2.8, half, bh, "SAVE") || enter;
        let save_join = ui.button(x + half + 4.0 * s, y + step * 2.8, half, bh, "SAVE AND JOIN");
        if save || save_join {
            let address: String = self.form_address.chars().filter(|c| !c.is_whitespace()).collect();
            if address.is_empty() {
                self.status = String::from("TYPE THE SERVER'S ADDRESS");
            } else {
                let name = if self.form_server.is_empty() { alloc::format!("server{}", self.servers.len() + 1) } else { self.form_server.clone() };
                match self.server_edit {
                    Some(k) if k < self.servers.len() => self.servers[k] = (name, address.clone()),
                    _ if self.servers.len() < 64 => self.servers.push((name, address.clone())),
                    _ => {}
                }
                self.save_servers();
                self.status.clear();
                self.set_screen(Screen::Join);
                if save_join {
                    self.join_address(ui.r, &address);
                }
                return;
            }
        }
        if ui.button(x, y + step * 4.0, bw, bh, "CANCEL") {
            self.status.clear();
            self.set_screen(Screen::Join);
        }
        ui.centered(ui.w() * 0.5, y + step * 5.2, 1.0, &self.status, ui::ACCENT);
        ui.centered(ui.w() * 0.5, y + step * 6.0, 1.0, "CTRL+V PASTES AN ADDRESS", ui::DIM);
    }

    /// Paint a little more of the minimap (see client::fullmap).
    fn paint_minimap(&mut self, r: &mut Renderer) {
        let g = match self.session.as_ref() {
            Some(g) if self.settings.minimap => g,
            _ => return,
        };
        let frame = super::fullmap::Frame {
            seed: g.world.seed,
            world: super::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        if crate::render::EXPERIMENT.load(core::sync::atomic::Ordering::Relaxed) & 16 != 0 {
            return;
        }
        // A little each frame keeps this off the frame time.
        super::fullmap::minimap_work(&mut self.minimap, r, &frame, 40);
    }

    /// Paint the full map while it is open.
    fn work_full_map(&mut self, r: &mut Renderer) {
        let (Some(g), Some(full)) = (self.session.as_ref(), self.full.as_mut()) else { return };
        let frame = super::fullmap::Frame {
            seed: g.world.seed,
            world: super::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        full.work(r, &frame);
    }

    /// Read the pack files again and switch to them without restarting.
    fn reload_packs(&mut self, r: &mut Renderer) {
        if self.session.as_ref().map_or(false, |g| g.net.is_some()) {
            if let Some(g) = self.session.as_mut() {
                g.say("ON A SERVER, THE SERVER DECIDES THE CONTENT");
            }
            return;
        }
        let (new, text) = load_content(&self.pack_paths, &self.settings.mods_off);
        for w in &new.warnings {
            logf!("voxelrust: data: {}", w);
        }
        let note = if new.warnings.is_empty() {
            alloc::format!("PACKS RELOADED: {} BLOCKS, {} ITEMS", new.blocks.len(), new.items.len())
        } else {
            alloc::format!("PACKS RELOADED, {} PROBLEMS (SEE THE LOG)", new.warnings.len())
        };
        if new.items.is_empty() {
            return;
        }
        match self.session.as_mut() {
            Some(g) => {
                g.reload_content(new, r);
                g.push_chat(note.clone());
            }
            None => block::replace(new),
        }
        r.rebuild_atlas();
        r.load_models();
        self.pack_text = text;
        self.server_content = false;
        self.status = note;
    }

    fn screen_chat(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (w, h) = (ui.w(), ui.h());
        let y = h - 40.0 * s;
        if ui.text_field(20, 4.0 * s, y, w * 0.6, 14.0 * s, &mut self.form_chat, 100, ui::any_char) {
            let text = core::mem::take(&mut self.form_chat);
            let name = self.settings.player_name.clone();
            let command = text.trim().to_ascii_lowercase();
            if command == "/reload" {
                self.want_reload = true;
            } else if command == "/help" {
                if let Some(g) = self.session.as_mut() {
                    g.push_chat(String::from("/RELOAD   /GIVE ITEM [N]   /TIME DAY|NIGHT   /TP X Y Z"));
                    g.push_chat(String::from("/SPAWN CREATURE [N] [YOUNG]   /MODE CREATIVE|SURVIVAL   /WEATHER CLEAR|RAIN|STORM   /SEED"));
                    g.push_chat(String::from("/SET X Y Z BLOCK [TURNS]   /DIM NAME|HOME   /SAY   /SOUND   /HEAL   /FEED   /HURT"));
                    g.push_chat(String::from("/LAUNCH UP [AHEAD]   /SHOOT DAMAGE [SPEED] [BLOCK]   /AIM BLOCK   /STRIKE   (~ = WHERE YOU STAND)"));
                    g.push_chat(String::from("/EFFECT GIVE ID [SECS] [LEVEL]   /EFFECT CLEAR [ID]   /EFFECT LIST   /EFFECT MOB ID"));
                    g.push_chat(String::from("/VAR NAME = N   /IF COND THEN CMD [ELSE CMD]   /AFTER SECS CMD   /REPEAT N CMD   /TAKE ITEM [N]   /TITLE TEXT   /BLAST R"));
                }
            } else if let Some(g) = self.session.as_mut() {
                // Scripts hear `chat` and `command` first, and a mod's
                // `on_chat` may swallow the line (game/events.rs).
                let go_on = crate::game::events::chat_line(g, &text);
                // Commands are typed in lower case ids; anything else is talk.
                match if go_on && command.starts_with('/') { g.command(&command) } else { None } {
                    _ if !go_on => {}
                    Some(reply) if reply.is_empty() => {}
                    Some(reply) => g.push_chat(reply),
                    // On a server, what the game does not know the server may
                    // (/list, /op, /kick...): pass it on.
                    None if command.starts_with('/') && g.net.is_some() => {
                        if let Some(n) = g.net.as_mut() {
                            n.send_chat(&text);
                        }
                    }
                    None if command.starts_with('/') => g.push_chat(String::from("NO SUCH COMMAND. TRY /HELP")),
                    None => g.send_chat(&name, &text),
                }
            }
            self.set_screen(Screen::Game);
        }
    }

    /// The map key: a mod's own map for this world if it has one, else
    /// the minimap grows.
    fn open_map(&mut self) {
        self.map_native = None;
        if let Some(g) = self.session.as_mut() {
            for (k, n) in crate::native::active() {
                if n.map_open.map_or(false, |open| open(g)) {
                    self.map_native = Some(k);
                    break;
                }
            }
        }
        if self.map_native.is_some() {
            self.set_screen(Screen::Map);
        } else {
            self.open_full_map();
        }
    }

    /// The block map: the world's terrain at any zoom, with waypoints.
    fn open_full_map(&mut self) {
        if self.session.is_some() {
            self.set_screen(Screen::FullMap);
        }
    }

    /// Leave the block map, keeping its zoom and toggles for next time.
    fn close_full_map(&mut self) {
        if let Some(full) = self.full.as_ref() {
            self.settings.map_zoom = super::fullmap::scale_index(full.view.step) as i32;
            self.settings.map_follow = full.follow;
            self.settings.map_grid = full.grid;
            self.settings.map_waypoints = full.show_waypoints;
            if !self.scripted {
                sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
            }
        }
        self.set_screen(Screen::Game);
    }

    /// The block map's screen: its picture, panel and waypoints.
    fn screen_full_map(&mut self, ui: &mut Ui) {
        if self.session.is_none() {
            self.set_screen(Screen::Game);
            return;
        }
        if self.full.is_none() {
            let st = &self.settings;
            self.full = Some(super::fullmap::FullMap::new(st.map_zoom.max(0) as usize, st.map_follow, st.map_grid, st.map_waypoints));
        }
        let (Some(g), Some(full)) = (self.session.as_mut(), self.full.as_mut()) else { return };
        let frame = super::fullmap::Frame {
            seed: g.world.seed,
            world: super::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        let asked = super::fullmap::screen(full, ui, &frame, &mut g.waypoints);
        if let Some(i) = asked.preset {
            let step = super::fullmap::MAP_SCALES[(self.settings.map_preset[i].clamp(0, 6)) as usize];
            full.view.step = step;
            full.follow = true;
        }
    }

    /// The screen a mod's code put up for the map key.
    fn screen_map(&mut self, ui: &mut Ui) {
        let screen = self.map_native.and_then(|k| crate::native::all()[k].map_screen);
        let stay = match (screen, self.session.as_mut()) {
            (Some(screen), Some(g)) => screen(ui, g),
            _ => false,
        };
        if !stay {
            self.set_screen(Screen::Game);
        }
    }

    /// The emote wheel: let go of the key or click to play the emote the pointer is on.
    fn screen_emote(&mut self, ui: &mut Ui) {
        let names: Vec<String> = reg().emotes.list.iter().map(|e| e.name.to_ascii_uppercase()).collect();
        let key = (self.settings.key(Action::Emote) as usize).min(511);
        let (held, pressed) = (self.keys[key], ui.inp.pressed(key as i32));
        match self.wheel.show(ui, &names, held, pressed) {
            crate::client::emote_wheel::Outcome::Open => {}
            crate::client::emote_wheel::Outcome::Cancel => self.set_screen(Screen::Game),
            crate::client::emote_wheel::Outcome::Play(i) => {
                if let Some(g) = self.session.as_mut() {
                    g.play_emote(i);
                }
                self.set_screen(Screen::Game);
            }
        }
    }

    fn screen_inventory(&mut self, ui: &mut Ui) {
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
                        tip = Some(r.items[st.item as usize].name.clone());
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
                            draw_slot(ui.r, x, y, cell, s, Some(Stack { item: item as u16, count: 1 }), hot);
                            if hot {
                                tip = Some(r.items[item].name.clone());
                                if click_l || click_r {
                                    pick = Some(Stack { item: item as u16, count: if click_l { r.items[item].stack } else { 1 } });
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
            let frame = super::screens::crafting::Frame {
                gx,
                uy,
                width: cols as f32 * cell,
                cell,
                rows: upper_rows,
                side,
                shift: self.keys[225] || self.keys[229],
            };
            if let Some(t) = super::screens::crafting::draw(ui, g, &mut self.craft, &frame) {
                tip = Some(t);
            }
        } else {
            // An oven's or a furnace's list: click a recipe to make it.
            let at_oven = at_table && self.at_oven;
            let at_furnace = at_table && self.at_furnace;
            ui.text(gx, py + 6.0 * s, if at_furnace { "FURNACE (CLICK SOMETHING TO SMELT IT)" } else { "OVEN (CLICK SOMETHING TO BAKE IT)" }, ui::DIM);
            // Recipes you can make now come first.
            let can = |rc: &block::Recipe| rc.inputs.iter().all(|&(i, n)| g.inv.count(i as u16) >= n as u32);
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
                if g.inv.add(rc.out.0 as u16, rc.out.1) > 0 {
                    g.say("INVENTORY FULL");
                }
                crate::game::events::crafted(g, rc.out.0);
                ui.clicked_any = true;
            }
        }

        // A portrait of the player beside the panel, where there is room.
        if px > 70.0 * s {
            let body = body_for(g, own_id(g), self.settings.slim_arms);
            if let Some(model) = reg().model(arm_models(&body, self.settings.slim_arms).0) {
                ui.panel(px - 66.0 * s, py, 60.0 * s, 124.0 * s);
                let pose = crate::model::Pose { phase: g.clock * 2.0, stride: 0.12, ..Default::default() };
                ui.r.tex_override = self.skin_tex;
                ui.r.draw_model_2d(model, &pose, px - 36.0 * s, py + 112.0 * s, PI - 0.45, 96.0 * s);
                let name = self.settings.player_name.to_ascii_uppercase();
                ui.centered(px - 36.0 * s, py + 114.0 * s, 1.0, &name, WHITE);
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
                    tip = Some(r.items[st.item as usize].name.clone());
                }
                if click_l || click_r {
                    g.inv.click(i, &mut g.cursor, click_r);
                    ui.clicked_any = true;
                }
            }
        }
        // Clicking outside the panel with something in hand drops it.
        if (click_l || click_r) && !ui.hot(px, py, pw, ph) {
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

    /// Stand one of every creature in rows in front of the
    /// player (for screenshots, and for checking a pack's models).
    fn line_up_creatures(&mut self) {
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let p = g.player.pos;
        let fwd = crate::math::forward(g.player.yaw, 0.0);
        let right = [-fwd.z, fwd.x];
        // One of each model is enough: skip colour variants. With "looks:NAME", every look of
        // the creatures with NAME in their id, the plain one first.
        let mut seen = Vec::new();
        let mut kinds: Vec<(usize, u8)> = Vec::new();
        for i in 0..reg().entities.len() {
            if let Some(name) = &self.show_looks {
                if reg().entities[i].id.contains(name.as_str()) {
                    for v in 0..=reg().entities[i].x.variants.len() {
                        kinds.push((i, v as u8));
                    }
                }
                continue;
            }
            let m = reg().entities[i].model;
            if !seen.contains(&m) {
                seen.push(m);
                kinds.push((i, 0));
            }
        }
        logf!("voxelrust: line-up of {} creatures", kinds.len());
        let per_row = 8;
        let total = kinds.len();
        for n in 0..total {
            let (row, col) = (n / per_row, n % per_row);
            let side = (col as f32 - (per_row as f32 - 1.0) * 0.5) * 2.6;
            let ahead = 6.0 + row as f32 * 3.2;
            let (x, z) = (p[0] as f32 + fwd.x * ahead + right[0] * side, p[2] as f32 + fwd.z * ahead + right[1] * side);
            let world = &g.world;
            let solid = |x: i32, y: i32, z: i32| block::is_solid(world.get(x, y, z));
            let c = match crate::path::ground_near(&solid, floord(x as f64), floord(p[1]), floord(z as f64), 12) {
                Some(c) => c,
                None => continue,
            };
            let at = [c[0] as f64 + 0.5, c[1] as f64 + 0.01, c[2] as f64 + 0.5];
            if n < kinds.len() {
                let i = g.mobs.spawn(kinds[n].0, at);
                g.mobs.list[i].variant = kinds[n].1;
                // "walking" shows them mid-stride, each at its own point.
                if self.show_walking {
                    g.mobs.list[i].stride = 1.0;
                    g.mobs.list[i].phase = 0.9 + n as f32 * 0.7;
                    g.mobs.list[i].clock = n as f32 * 0.37;
                }
                // Turned a little, so shape and face both show.
                g.mobs.list[i].body.yaw = g.player.yaw + PI - 0.6;
                g.mobs.list[i].phase = 1.2;
                g.mobs.list[i].stride = 0.6;
            }
        }
    }

    /// Place one of every block on a grid in front of the player.
    fn build_showcase(&mut self) {
        let r = reg();
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let p = g.player.pos;
        let (x0, y0, z0) = (floord(p[0]) - 19, floord(p[1]) + 1, floord(p[2]) - 12);
        let mut n = 0;
        for it in &r.items {
            if let (Some(first), 0) = (it.place, it.brush) {
                let (col, row) = (n % 20, n / 20);
                g.world.set_block(x0 + col * 2, (y0 + row * 2).min(CH - 1), z0, r.placed_state(first, 0, false));
                n += 1;
            }
        }
    }
}

fn key_name(scancode: i32) -> String {
    let n = unsafe { sys::from_cstr((sdl::sdl().SDL_GetScancodeName)(scancode)) };
    if n.is_empty() {
        alloc::format!("KEY {}", scancode)
    } else {
        n.to_ascii_uppercase()
    }
}

/// A stepper that moves through a fixed list of values.
fn step_list(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, shown: &str, v: &mut i32, list: &[i32]) -> bool {
    let mut i = list.iter().position(|x| x == v).unwrap_or(0) as i32;
    let changed = ui.stepper_text(x, y, w, h, shown, &mut i, 0, list.len() as i32 - 1, 1);
    *v = list[i as usize];
    changed
}

/// The sixteen chat colours: `&0`..`&9`, `&a`..`&f`.
const CHAT_COLOURS: [[u8; 3]; 16] = [
    [0, 0, 0], [0, 0, 170], [0, 170, 0], [0, 170, 170], [170, 0, 0], [170, 0, 170], [255, 170, 0], [170, 170, 170],
    [85, 85, 85], [85, 85, 255], [85, 255, 85], [85, 255, 255], [255, 85, 85], [255, 85, 255], [255, 255, 85], [255, 255, 255],
];

fn colour_code(c: char) -> Option<usize> {
    c.to_digit(16).map(|d| d as usize)
}

/// A chat line without its colour codes.
fn strip_colours(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek().copied().and_then(colour_code).is_some() {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// Draw a chat line, switching colour at each `&` code.
fn label_coloured(r: &mut Renderer, x: f32, y: f32, s: f32, line: &str, alpha: u8) {
    let mut col = [255u8, 255, 255];
    let mut at = x;
    let mut run = String::new();
    let mut chars = line.chars().peekable();
    loop {
        let c = chars.next();
        let code = match c {
            Some('&') => chars.peek().copied().and_then(colour_code),
            _ => None,
        };
        if code.is_some() || c.is_none() {
            if !run.is_empty() {
                r.label(at, y, s, &run, [col[0], col[1], col[2], alpha]);
                at += Renderer::text_width(&run, s);
                run.clear();
            }
            match (c, code) {
                (None, _) => break,
                (_, Some(k)) => {
                    col = CHAT_COLOURS[k];
                    chars.next();
                }
                _ => {}
            }
        } else if let Some(c) = c {
            run.push(c);
        }
    }
}

/// How far the hand has swung: a blow or a use, 0..1 (the cooldowns that
/// follow them).
fn hand_swing(g: &session::Session) -> f32 {
    (g.cooldown[0] / 0.35).max(g.cooldown[1] / 0.5).clamp(0.0, 1.0)
}

/// What the first-person hand is doing: the swing of a blow or use (from its
/// start, as LastRoute's tweens run), the walking bob, a bow's draw and eating.
fn hand_view_of(g: &session::Session, held: crate::client::hand::Held, speed: f32) -> crate::client::hand::View {
    use crate::client::hand::Held;
    let index = g.held().map(|h| h.0);
    let id = index.map_or(0, |i| i as u16);
    // The cooldowns count down from a blow's or a use's start; the swing runs up from it.
    let left = (g.cooldown[0] / 0.35).max(g.cooldown[1] / 0.5).clamp(0.0, 1.0);
    let swing = if left > 0.0 { (1.0 - left).max(0.001) } else { 0.0 };
    let draw = if matches!(held, Held::Bow(_)) { g.bow.shown(id) } else { 0.0 };
    let eat = g.eating.shown(id);
    // The bow's own motion is its draw; eating is not a blow.
    let swing = if matches!(held, Held::Bow(_)) || eat > 0.0 { 0.0 } else { swing };
    crate::client::hand::View {
        swing,
        bob: crate::client::hand::walk_bob(g.clock, speed, g.player.on_ground),
        draw,
        eat,
        drink: index.map_or(false, |i| crate::game::eating::is_drink(&reg().items[i].id)),
    }
}

pub(crate) fn draw_item(r: &mut Renderer, x: f32, y: f32, size: f32, index: usize, tint: Color) {
    let it = &reg().items[index];
    // A spawn egg shows the creature itself, standing in half an eggshell.
    if let Some(e) = it.x.egg.and_then(|k| reg().entities.get(k)) {
        let shell = |r: &mut Renderer, k: f32, c: Color| {
            // A rounded bowl out of three bars.
            r.rect(x + size * (0.5 - 0.42 * k), y + size * 0.70, size * 0.84 * k, size * 0.12, c);
            r.rect(x + size * (0.5 - 0.36 * k), y + size * 0.80, size * 0.72 * k, size * 0.10, c);
            r.rect(x + size * (0.5 - 0.24 * k), y + size * 0.88, size * 0.48 * k, size * 0.08, c);
        };
        shell(r, 1.0, [(232u16 * tint[0] as u16 / 255) as u8, (224u16 * tint[1] as u16 / 255) as u8, (200u16 * tint[2] as u16 / 255) as u8, tint[3]]);
        let (tall, wide) = r.model_extent(e.model);
        // As big as fits the slot, whichever way the creature is long.
        let height = (size * 0.78).min(size * 0.92 * tall / wide);
        let pose = crate::model::Pose::default();
        r.tint_2d = e.tint;
        r.tex_override = r.entity_texture(&e.x.tex);
        r.draw_model_2d(e.model, &pose, x + size * 0.5, y + size * 0.84, PI - 0.6, height);
        return;
    }
    // A prop shows as its model.
    if let Some((model, _, _)) = it.place.and_then(|b| block::def(b).x.prop) {
        let (tall, wide) = r.model_extent(model);
        let height = (size * 0.9).min(size * 0.95 * tall / wide);
        r.tint_2d = [tint[0], tint[1], tint[2]];
        r.draw_model_2d(model, &crate::model::Pose::default(), x + size * 0.5, y + size * 0.95, PI - 0.6, height);
        return;
    }
    // Blocks are shown as small 3D blocks; everything else as its picture.
    match it.place.map(block::def).filter(|_| it.brush == 0 && it.x.turns.is_empty()) {
        Some(d) if d.kind == Kind::Boxes => {
            let mut shape = Vec::new();
            // Fences and walls are shown with a neighbour on each side.
            block::shape(d.first, if d.connect_group != 0 { 0b0101 } else { 0 }, &mut shape);
            r.icon_block(x, y, size, d.tex, &shape, tint)
        }
        Some(d) if matches!(d.kind, Kind::Cube | Kind::Cutout | Kind::Translucent) && d.first != block::AIR => r.icon_block(x, y, size, d.tex, &[], tint),
        // A bow being drawn shows the picture of it drawn.
        _ => r.icon(x, y, size, crate::game::bow::icon_for(index, it.icon), tint),
    }
}

pub(crate) fn draw_slot(r: &mut Renderer, x: f32, y: f32, size: f32, s: f32, stack: Option<Stack>, hot: bool) {
    // A slot is a shallow well: dark top and left, light bottom and right.
    let (ix, iy, iw) = (x + s, y + s, size - 2.0 * s);
    r.rect(ix, iy, iw, iw, if hot { [92, 126, 204, 240] } else { [44, 47, 58, 225] });
    r.rect(ix, iy, iw, s, [18, 19, 26, 230]);
    r.rect(ix, iy, s, iw, [18, 19, 26, 230]);
    r.rect(ix, iy + iw - s, iw, s, [86, 92, 112, 230]);
    r.rect(ix + iw - s, iy, s, iw, [86, 92, 112, 230]);
    if hot {
        r.frame(ix, iy, iw, iw, s, [255, 255, 255, 150]);
    }
    if let Some(st) = stack {
        // (A small slot, as in a 7 x 7 grid, has a thinner margin.)
        let m = (3.0 * s).min(size * 0.15);
        draw_item(r, x + m, y + m, size - 2.0 * m, st.item as usize, WHITE);
        if st.count > 1 {
            let t = alloc::format!("{}", st.count);
            let tw = Renderer::text_width(&t, s);
            r.label(x + size - tw - 1.5 * s, (y + size - 9.0 * s).max(y + size * 0.45), s, &t, WHITE);
        }
    }
}

/// A 7x6 pixel heart: 0 empty, 1 half, 2 full.
fn heart(r: &mut Renderer, x: f32, y: f32, s: f32, fill: i32) {
    const ROWS: [u8; 6] = [0b0110110, 0b1111111, 0b1111111, 0b0111110, 0b0011100, 0b0001000];
    for (j, row) in ROWS.iter().enumerate() {
        for i in 0..7 {
            if row & (0x40 >> i) != 0 {
                let lit = fill == 2 || (fill == 1 && i < 4);
                let c: Color = if lit { [226, 44, 52, 255] } else { [40, 16, 18, 210] };
                r.rect(x + i as f32 * s, y + j as f32 * s, s, s, c);
            }
        }
    }
}

/// Where the player's mods are: `--mods-dir`, else `<data>/mods` (no
/// trailing slash).
fn mods_folder(opts: &Opts, base: &str) -> String {
    match opts.mods_dir.as_deref().map(|d| d.trim_end_matches(|c| c == '/' || c == '\\')).filter(|d| !d.is_empty()) {
        Some(d) => String::from(d),
        None => alloc::format!("{}mods", base),
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let opts = parse_args(&args)?;
    crate::update::remember_args(&args);
    if opts.help {
        sys::log(USAGE);
        for n in crate::native::all().iter().filter(|n| !n.help.is_empty()) {
            sys::log(&alloc::format!("\nFrom the mod `{}`:\n{}", n.name, n.help));
        }
        return Ok(());
    }
    if opts.version {
        sys::log(&alloc::format!("voxelrust {} ({}, commit {})", env!("CARGO_PKG_VERSION"), BUILD, crate::update::COMMIT));
        return Ok(());
    }
    if let Some(path) = &opts.stamp {
        // Files go through SDL. Loading it opens no window.
        sdl::load()?;
        if !sys::write_file(path, crate::update::stamp_text(BUILD).as_bytes()) {
            return Err(alloc::format!("could not write {}", path));
        }
        return Ok(());
    }
    sdl::load()?;
    if let Some(path) = &opts.script_check {
        // Reads a script the way the game would and says what is wrong (no window).
        return crate::script::check::check_file(path);
    }
    if let Some(path) = &opts.log {
        sys::log_to_file(path);
    }
    let mut phases = crate::client::startup::Phases::new();
    let api = sdl::sdl();
    if let Some(path) = &opts.write_icon {
        // Needs SDL only for writing the file.
        let size = if opts.icon_size == 0 { 256 } else { opts.icon_size };
        let png = crate::png::encode(size, size, &crate::icon::rgba(size));
        return if sys::write_file(path, &png) { Ok(()) } else { Err(alloc::format!("could not write {}", path)) };
    }
    if unsafe { (api.SDL_Init)(sdl::INIT_VIDEO) } != 0 {
        return Err(alloc::format!("SDL could not start its video system: {}", sdl::error()));
    }

    // Where worlds and settings live.
    let mut base = match &opts.data {
        Some(d) => d.clone(),
        None => match api.SDL_GetPrefPath {
            // The per-user place for program data, under KYTTU/VoxelRust:
            // AppData\Roaming on Windows, Library/Application Support on a
            // Mac, ~/.local/share on Linux.
            Some(f) => unsafe {
                let folder = |org: &[u8]| {
                    let p = f(org.as_ptr() as *const c_char, b"VoxelRust\0".as_ptr() as *const c_char);
                    let s = sys::from_cstr(p);
                    if !p.is_null() {
                        (api.SDL_free)(p as *mut c_void);
                    }
                    s
                };
                let new = folder(b"KYTTU\0");
                // Worlds and settings kept where earlier versions put them
                // come along, once.
                if !new.is_empty() && sys::read_file(&alloc::format!("{}settings.txt", new)).is_none() && sys::read_file(&alloc::format!("{}index.txt", crate::session::world_dir(&new))).is_none() {
                    let old = new.replace("KYTTU", "VoxelRust");
                    if old != new {
                        let mut moved = 0;
                        for name in sys::list_dir(&old) {
                            if let Some(bytes) = sys::read_file(&alloc::format!("{}{}", old, name)) {
                                moved += sys::write_file(&alloc::format!("{}{}", new, name), &bytes) as u32;
                            }
                        }
                        if moved > 0 {
                            logf!("voxelrust: brought {} files over from {}", moved, old);
                        }
                    }
                }
                new
            },
            None => String::new(),
        },
    };
    if !base.is_empty() && !base.ends_with('/') && !base.ends_with('\\') {
        base.push('/');
    }

    // Worlds live in worlds/, exports in exports/, pictures in screenshots/.
    for dir in ["worlds/", "exports/", "screenshots/"] {
        sys::make_dir(&alloc::format!("{}{}", base, dir));
    }
    crate::session::migrate_worlds(&base);
    // Where mods' scripts keep what they save (script/mod.rs).
    crate::script::ScriptHost::set_data_dir(&base);
    // Scripts of mods a server sends are run by the script engine.
    crate::content::mod_runtime::set_host(alloc::boxed::Box::new(crate::script::Engine::new()));
    if let Some(from) = &opts.import_mod {
        let said = import_mod(&mods_folder(&opts, &base), from);
        logf!("voxelrust: {}", said.to_ascii_lowercase());
        return if said.starts_with("ADDED") { Ok(()) } else { Err(String::from("the mod was not added")) };
    }
    // Moving a world in or out as one file needs no window.
    if let Some((op, name, to)) = &opts.world_op {
        if name.is_empty() || *name != crate::settings::clean_world_name(name, 24) {
            return Err(String::from("--world-op: the world name must be letters, digits, spaces, - and _"));
        }
        let done = match op.as_str() {
            "delete" => crate::session::delete_world(&base, name),
            "new" => {
                if crate::session::world_exists(&base, name) {
                    false
                } else {
                    let new = crate::session::NewWorld { seed: 13, mode: GameMode::Creative, kind: WorldKind::Normal };
                    let mut s = Session::open(&base, name, &new, 0);
                    let ok = s.save();
                    crate::session::remember_world(&base, name);
                    ok
                }
            }
            "copy" => crate::session::copy_world(&base, name, to),
            "rename" => crate::session::rename_world(&base, name, to),
            _ => return Err(alloc::format!("--world-op: unknown operation {} (delete, copy, rename)", op)),
        };
        logf!("voxelrust: {} {} {} -> {}", op, name, to, if done { "done" } else { "failed" });
        return if done { Ok(()) } else { Err(alloc::format!("could not {} {}", op, name)) };
    }
    if opts.export.is_some() || opts.import.is_some() {
        let name = opts.world.clone().unwrap_or_default();
        if name.is_empty() || name != crate::settings::clean_world_name(&name, 24) {
            return Err(String::from("--export/--import need --world NAME (letters, digits, - and _)"));
        }
        if let Some(path) = &opts.export {
            let data = crate::save::export_world(&mut crate::save::DiskIo, &base, &name).ok_or_else(|| alloc::format!("there is no world called {}", name))?;
            if !sys::write_file(path, &data) {
                return Err(alloc::format!("could not write {}", path));
            }
            logf!("voxelrust: wrote {} ({} bytes)", path, data.len());
        }
        if let Some(path) = &opts.import {
            let data = sys::read_file(path).ok_or_else(|| alloc::format!("cannot read {}", path))?;
            if crate::session::world_exists(&base, &name) {
                return Err(alloc::format!("a world called {} already exists", name));
            }
            if !crate::save::import_world(&mut crate::save::DiskIo, &base, &name, &data) {
                return Err(alloc::format!("{} is not a VoxelRust world file", path));
            }
            crate::session::remember_world(&base, &name);
            logf!("voxelrust: imported {} as the world {}", path, name);
        }
        return Ok(());
    }

    // Content: the built-in data, then pack.txt from the data folder, then
    // any --pack files.
    let auto_pack = alloc::format!("{}pack.txt", base);
    // A data folder named on the command line may not be there yet.
    #[cfg(not(target_arch = "wasm32"))]
    if opts.data.is_some() && !base.is_empty() && !base.contains('"') && sys::list_dir(&base).is_empty() {
        let dir = base.trim_end_matches(|c| c == '/' || c == '\\');
        sys::run_command(&if cfg!(windows) { alloc::format!("mkdir \"{}\" 2>nul", dir) } else { alloc::format!("mkdir -p \"{}\"", dir) });
    }
    // Then every mod in the mods folder (`<data>/mods`, or --mods-dir), and,
    // unless another folder was named, in a `mods` folder where the game was
    // started (handy while making one). No mods come with the game or sit
    // beside the program: the player installs them, or a server sends them.
    let mods_dir = mods_folder(&opts, &base);
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::client::mods::folder::ensure(&mods_dir);
        // Mods a server sent last time (a crash can leave them behind) go.
        crate::net::modsync::tmp::clean_all(&base);
    }
    let mut pack_paths = alloc::vec![auto_pack.clone(), alloc::format!("{}/*", mods_dir)];
    if opts.mods_dir.is_none() && mods_dir != "mods" {
        pack_paths.push(String::from("mods/*"));
    }
    pack_paths.extend(opts.packs.iter().cloned());
    for p in &opts.packs {
        if sys::read_file(p).is_none() {
            return Err(alloc::format!("cannot read the pack file {}", p));
        }
    }
    // (The settings are read properly further down; which mods are off is
    // needed now.)
    let mods_off: String = sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| crate::settings::Settings::parse(&String::from_utf8_lossy(&b)).mods_off).unwrap_or_default();
    let (first, mut pack_text) = load_content(&pack_paths, &mods_off);
    block::install(first);
    phases.mark("content: base data and mods parsed");
    // Pictures the player has edited replace the game's own: textures/<id>.png.
    sys::make_dir(&alloc::format!("{}textures/", base));
    crate::textures::set_override_dir(&alloc::format!("{}textures/", base));
    // Texture packs: the settings' list, then any named on the command line.
    {
        let early = sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| crate::settings::Settings::parse(&String::from_utf8_lossy(&b))).unwrap_or_default();
        crate::textures::set_pack_tweak(early.pack_hue, early.pack_saturation, early.pack_brightness);
        let saved = early.texture_pack;
        let mut list: Vec<String> = saved.split('|').map(|s| String::from(s.trim())).filter(|s| !s.is_empty()).collect();
        list.extend(opts.texture_packs.iter().cloned());
        if !list.is_empty() {
            let n = crate::textures::set_texture_packs(&list);
            logf!("voxelrust: texture pack{} {}: {} pictures", if list.len() == 1 { "" } else { "s" }, list.join(", "), n);
        }
    }
    phases.mark("texture packs indexed");
    if let Some(dir) = &opts.export_textures {
        let n = export_textures(reg(), dir)?;
        logf!("voxelrust: wrote {} textures to {}", n, dir);
        return Ok(());
    }
    let registry = reg();
    for w in &registry.warnings {
        logf!("voxelrust: data: {}", w);
    }
    if registry.items.is_empty() {
        return Err(String::from("the data files define no items"));
    }

    // A kind of world asked for through some mod's own option.
    let mut asked_kind: Option<String> = None;
    for n in crate::native::all() {
        if let Some(kind) = n.option_kind.and_then(|kind| kind()) {
            if !registry.has(n.name) {
                return Err(alloc::format!("that option needs the mod `{}`, which is switched off (Settings > MODS)", n.name));
            }
            asked_kind = Some(kind);
        }
    }
    if opts.server {
        return run_server(&opts, &base, pack_text.bytes(read_mod_file), &pack_paths, &mods_off);
    }
    let scripted_early = opts.screenshot.is_some() || opts.bench.is_some();
    let settings_path = alloc::format!("{}settings.txt", base);
    let mut settings = Settings::parse(&sys::read_file(&settings_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default());
    if let Some(d) = opts.distance {
        settings.distance = d;
    }
    // Where the program is, for a restart after an update; and whether this
    // copy was itself started by one (then it does not restart in turn).
    #[cfg(not(target_arch = "wasm32"))]
    let (restart_exe, restart_folder) = (sys::exe_path(), base.clone());
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(not(feature = "update-test"), allow(unused_variables))]
    let restarted = crate::update::consume_restart_marker(&base);
    #[cfg(feature = "update-test")]
    if opts.update_now {
        crate::update::run_now(BUILD, &base, opts.update_mirror.as_deref().unwrap_or(&settings.update_mirror));
        crate::update::restart(&restart_exe, &restart_folder);
        return Ok(());
    }
    #[cfg(feature = "update-test")]
    if restarted {
        // The copy an update started: say so in the log, then stop.
        sys::write_file(&alloc::format!("{}update-child.txt", base), BUILD.as_bytes());
        return Ok(());
    }
    // Look for a newer version in the background (never in scripted runs,
    // nor in a browser, where the page is the program).
    #[cfg(not(target_arch = "wasm32"))]
    if !scripted_early && !opts.server && !opts.no_update {
        let (scratch, install) = (base.clone(), settings.auto_update);
        let mirror = opts.update_mirror.clone().unwrap_or_else(|| settings.update_mirror.clone());
        sys::thread::spawn(alloc::boxed::Box::new(move || crate::update::run(BUILD, &scratch, install, &mirror)));
    }
    if let Some(f) = opts.far {
        settings.far_distance = f;
    }
    if let Some(p) = opts.render_scale {
        settings.render_scale = p;
    } else if scripted_early {
        settings.render_scale = 100;
    }
    if opts.fullscreen {
        settings.fullscreen = true;
    }
    if opts.novsync {
        settings.vsync = false;
    }
    let scripted = opts.screenshot.is_some() || opts.bench.is_some();
    if scripted {
        // Scripted runs use a window unless full screen was asked for.
        settings.fullscreen = opts.fullscreen;
        settings.target_fps = 0;
        settings.max_fps = 0;
    }

    // Window and OpenGL context. No version is requested: the default is the
    // legacy / compatibility context, which is what fixed-function needs.
    let (w, h) = opts.size.unwrap_or((1280, 720));
    let mut flags = sdl::WINDOW_OPENGL | sdl::WINDOW_RESIZABLE;
    if opts.hidden && scripted {
        flags |= sdl::WINDOW_HIDDEN;
    }
    // Full screen with an explicit --size means "switch the display to that".
    let exclusive = opts.fullscreen && opts.size.is_some();
    let mode = crate::settings::FULLSCREEN_MODES[settings.fullscreen_mode as usize];
    let (w, h) = if settings.fullscreen && !exclusive && mode.0 > 0 { mode } else { (w, h) };
    if settings.fullscreen {
        flags |= if exclusive || mode.0 > 0 { sdl::WINDOW_FULLSCREEN } else { sdl::WINDOW_FULLSCREEN_DESKTOP };
    }
    let mut window = core::ptr::null_mut();
    for depth in [24, 16] {
        unsafe {
            (api.SDL_GL_SetAttribute)(sdl::GL_DOUBLEBUFFER, 1);
            (api.SDL_GL_SetAttribute)(sdl::GL_DEPTH_SIZE, depth);
            // "Centred on display N" is the centred constant plus N.
            let at = sdl::WINDOWPOS_CENTERED | opts.display;
            window = (api.SDL_CreateWindow)(b"VoxelRust\0".as_ptr() as *const c_char, at, at, w, h, flags);
        }
        if !window.is_null() {
            break;
        }
    }
    if window.is_null() {
        return Err(alloc::format!("could not open a window: {}", sdl::error()));
    }
    let ctx = unsafe { (api.SDL_GL_CreateContext)(window) };
    if ctx.is_null() {
        return Err(alloc::format!("could not create an OpenGL context: {}", sdl::error()));
    }
    gl::load()?;
    phases.mark("window and OpenGL context");
    logf!("voxelrust: OpenGL {} on {}", gl::get_string(gl::VERSION), gl::get_string(gl::RENDERER));
    crate::icon::set_window_icon(window);
    let mut r = Renderer::new();
    phases.mark("renderer: atlas painted and uploaded, fonts, clouds");
    r.load_models();
    phases.mark("models parsed and uploaded");
    let (mut ww, mut wh) = (w, h);
    unsafe { (api.SDL_GetWindowSize)(window, &mut ww, &mut wh) };
    r.w = ww.max(1);
    r.h = wh.max(1);
    unsafe { (api.SDL_StartTextInput)() };
    let mut inp = UiInput::default();

    let threads = opts.threads.unwrap_or_else(|| thread::cpu_count().saturating_sub(1).clamp(1, 8));
    let mut app = App {
        window,
        audio: if scripted { Audio::silent() } else { Audio::new() },
        voice: crate::client::voice::Voice::new(),
        distance: settings.distance,
        settings,
        settings_path,
        base: base.clone(),
        threads,
        screen: Screen::Title,
        session: None,
        focus: 0,
        keys: [false; 512],
        buttons: [false; 4],
        pick: false,
        toggle_fly: false,
        leave: false,
        drop_item: false,
        last_jump_tap: 0.0,
        quit: false,
        fps: 0.0,
        tune_timer: 0.0,
        show_debug: opts.show.contains("debug"),
        help_timer: 0.0,
        loading_since: 0.0,
        wheel: Default::default(),
        third_person: opts.show.contains("third") || opts.show.contains("front"),
        third_front: opts.show.contains("front"),
        swallow_text: false,
        hud_hidden: false,
        ui_clock: 0.0,
        worlds: Vec::new(),
        form_name: String::new(),
        form_seed: String::new(),
        form_mode: GameMode::Survival,
        form_native: None,
        form_flat: false,
        form_address: String::new(),
        form_host: String::from("shared"),
        servers: Vec::new(),
        servers_loaded: false,
        form_server: String::new(),
        server_edit: None,
        server_scroll: 0,
        host: None,
        my_address: None,
        form_chat: String::new(),
        text_selected: false,
        skin_tex: 0,
        skin_for: String::new(),
        remote_skins: BTreeMap::new(),
        remote_slim: BTreeMap::new(),
        skin_png: Vec::new(),
        skin_sent: false,
        pack_paths,
        mods_dir,
        serve_mods: opts.serve_mods.as_deref().and_then(crate::net::modsync::serve::parse_serve_list),
        pack_text,
        server_content: false,
        want_reload: false,
        at_oven: false,
        show_walking: opts.show.contains("walking"),
        show_looks: opts.show.split(',').find_map(|w| w.strip_prefix("looks:")).map(String::from),
        mods_scroll: 0,
        video_detail: 0,
        form_mod_path: String::new(),
        at_furnace: false,
        craft: Default::default(),
        tune: (0.0, 0),
        worlds_scroll: 0,
        edit_world: String::new(),
        edit_name: String::new(),
        edit_confirm: false,
        minimap: super::fullmap::MapPicture::new(super::fullmap::MINIMAP_TEXELS),
        full: None,
        form_import: String::new(),
        skin_url: String::new(),
        picker: crate::client::picker::Picker::new(),
        tags: Vec::new(),
        map_native: None,
        status: String::new(),
        tab: 0,
        rebinding: None,
        inv_tab: 0,
        inv_scroll: 0,
        inv_search: String::new(),
        chest_at: [0; 3],
        chest_ext: None,
        scripted,
        show_hud: opts.show.contains("hud"),
    };
    app.apply_settings();
    app.load_skin(&mut r);
    app.status.clear();
    // Sounds the player has put in the data folder's `sounds/` folder replace the game's own.
    let own_sounds = app.audio.load_folder(&base);
    if !own_sounds.is_empty() {
        logf!("voxelrust: own sounds from {}sounds/: {}", base, own_sounds.join(" "));
    }
    logf!("voxelrust: data in {}", if base.is_empty() { "." } else { &base });
    for (_, n) in crate::native::active() {
        if let Some(start) = n.start {
            start(&base, &app.settings);
        }
    }
    phases.mark("audio folder, skin and mods started");

    // Going straight into a world: asked for by name, or a scripted run.
    let menu_only = ["title", "worlds", "newworld", "multiplayer", "picker"].iter().any(|s| opts.show.contains(s)) || (opts.show.contains("settings") && opts.world.is_none());
    if let Some(name) = &opts.host {
        app.form_host = name.clone();
        app.host_game(&mut r);
        if app.session.is_none() {
            return Err(alloc::format!("could not host: {}", app.status.to_ascii_lowercase()));
        }
    } else if let Some(address) = &opts.join {
        let name = app.settings.player_name.clone();
        let g = Session::join(&app.base, address, &name, app.threads).map_err(|e| alloc::format!("could not join {}: {}", address, e.to_ascii_lowercase()))?;
        app.session = Some(g);
        app.set_screen(Screen::Game);
    } else if opts.world.is_some() || (scripted && !menu_only) {
        let name = opts.world.clone().unwrap_or_else(|| String::from("world"));
        if name.is_empty() || name != crate::settings::clean_world_name(&name, 24) {
            return Err(String::from("--world: use only letters, digits, spaces, - and _ (24 at most)"));
        }
        let seed = opts.seed.unwrap_or_else(|| unsafe { (api.SDL_GetPerformanceCounter)() } as u32 ^ 0x5EED_1234);
        let kind = asked_kind.as_deref().and_then(WorldKind::parse).unwrap_or(WorldKind::Normal);
        app.start_world(&mut r, &name, &NewWorld { seed, mode: opts.mode.unwrap_or(GameMode::Survival), kind });
    }
    if let Some(g) = app.session.as_mut() {
        if let Some(p) = opts.pos {
            g.player.pos = p;
            g.player.flying = g.mode == GameMode::Creative;
        }
        if let Some((yaw, pitch)) = opts.look {
            g.player.yaw = yaw;
            g.player.pitch = pitch;
        }
        if let Some(t) = opts.time {
            g.time = t;
        }
    }
    if scripted {
        // "items:<tab>" opens the creative list at that tab, or searching for that word.
        if let Some(tab) = opts.show.split(',').find_map(|w| w.strip_prefix("items:")) {
            match reg().categories.iter().position(|c| c.eq_ignore_ascii_case(tab)) {
                Some(i) => app.inv_tab = i,
                // Not a tab: what to search for ("items:bed").
                None => app.inv_search = String::from(tab),
            }
        }
        // "craft:<item>" opens the crafting table with the first recipe for that item laid out.
        if let Some(want) = opts.show.split(',').find_map(|w| w.strip_prefix("craft:")) {
            app.screen = Screen::Craft;
            let r = reg();
            let one = |it: u16| Some(crate::inventory::Stack { item: it, count: 1 });
            if let Some(rc) = r.grid_recipes.iter().find(|rc| r.items[rc.out.0 as usize].id == want) {
                // The bench or workshop the recipe needs (a table at least).
                app.craft.table_side = rc.min_side().max(3);
                let stride = app.craft.table_side;
                match &rc.pattern {
                    crate::crafting::Pattern::Shaped { w, h, cells } => {
                        for y in 0..*h {
                            for x in 0..*w {
                                if let Some(ing) = &cells[y * w + x] {
                                    app.craft.grid.cells[y * stride + x] = ing.items.first().and_then(|&it| one(it));
                                }
                            }
                        }
                    }
                    crate::crafting::Pattern::Shapeless(list) => {
                        for (i, ing) in list.iter().enumerate() {
                            app.craft.grid.cells[i] = ing.items.first().and_then(|&it| one(it));
                        }
                    }
                }
            }
        }
        // "pose:NAME" holds the player's posture (stand, crouch, sit, sleep, swim, climb, fall, ride);
        // "emote:ID" or "emote:ID@SECONDS" plays that emote and keeps it at that moment (default 0.5 s).
        // Both are for pictures taken with "third" or "front".
        if let Some(g) = app.session.as_mut() {
            if let Some(name) = opts.show.split(',').find_map(|w| w.strip_prefix("pose:")) {
                g.posing.forced = crate::content::pose::KINDS.iter().copied().find(|k| alloc::format!("{:?}", k).eq_ignore_ascii_case(name));
            }
            if let Some(spec) = opts.show.split(',').find_map(|w| w.strip_prefix("emote:")) {
                let (id, at) = match spec.split_once('@') {
                    Some((id, t)) => (id, t.parse::<f32>().unwrap_or(0.5)),
                    None => (spec, 0.5),
                };
                match reg().emotes.find(id) {
                    Some(i) if g.posing.start_emote(i) => {
                        g.posing.emote = Some((i, at));
                        g.posing.frozen = true;
                    }
                    _ => logf!("voxelrust: --show emote:{}: no such emote", id),
                }
            }
        }
        // "skinurl" opens the panel for a skin from a web address.
        if opts.show.split(',').any(|w| w == "skinurl") {
            app.open_skin_url();
        }
        // "emote" (alone) opens the emote wheel.
        if opts.show.split(',').any(|w| w == "emote") {
            app.set_screen(Screen::Emote);
        }
        for (word, screen) in [
            ("pause", Screen::Pause),
            ("items", Screen::Inventory),
            ("dead", Screen::Dead),
            ("settings", Screen::Settings),
            ("worlds", Screen::Worlds),
            ("newworld", Screen::NewWorld),
            ("multiplayer", Screen::Join),
            ("addserver", Screen::AddServer),
        ] {
            if opts.show.contains(word) {
                app.screen = screen;
            }
        }
        if let Some(t) = opts.show.split("settings:").nth(1) {
            let t = t.split(',').next().unwrap_or("").to_ascii_uppercase();
            app.tab = TABS.iter().position(|x| *x == t).unwrap_or(0);
        }
        // "picker" or "picker:<folder>" opens the file browser (at that folder);
        // "pickfile" also selects the first file in it.
        if opts.show.contains("picker") {
            let dir = opts.show.split("picker:").nth(1).map(|d| String::from(d.split(',').next().unwrap_or("")));
            app.tab = 4;
            app.open_skin_picker();
            if let Some(d) = dir {
                app.picker.go(&d, None);
            }
            if opts.show.contains("pickfile") {
                app.picker.sel = app.picker.entries.iter().position(|e| !e.dir);
            }
            // "pickgo" chooses the selected file, as a click on SELECT would.
            if opts.show.contains("pickgo") {
                if let Some(f) = app.picker.selected_file() {
                    if app.close_picker(Some(f)) {
                        app.load_skin(&mut r);
                    }
                }
            }
        }
        app.worlds = session::list_worlds(&app.base);
        if opts.show.contains("editworld") {
            if let Some(first) = app.worlds.first().cloned() {
                app.edit_name = first.clone();
                app.edit_world = first;
                app.screen = Screen::EditWorld;
            }
        }
        inp.mouse = (-100.0, -100.0);
        if opts.show.split(|c: char| !c.is_alphanumeric()).any(|w| w == "map") {
            app.open_map();
        }
        // "atlas" opens the block map itself, on any world.
        if opts.show.contains("atlas") {
            app.open_full_map();
        }
    }

    let mut frame_count = 0u32;
    let mut showcase_pending = opts.showcase && scripted;
    let mut creatures_pending = scripted && opts.show.contains("creatures");
    let mut run_pending = opts.run.clone().filter(|_| scripted);
    let mut unloaded_frames = 0u32;
    let mut last = sys::now();
    let mut settled_frames = 0u32;
    let mut first_frame_logged = false;
    let mut bench: Option<(u32, f64, f64)> = None; // frames left, start, worst frame
    let load_start = sys::now();
    // The main loop and what follows it are a closure, left at the loop's
    // own indentation. Here it is called once and returns when the game
    // ends. A browser cannot be kept waiting in a loop, so there each call
    // runs one frame and returns, and the page calls again for the next
    // (see the end of this function, and docs/WEB.md).
    let frames = move || -> Result<bool, String> {
    loop {
        let frame_start = sys::now();
        app.handle_events(&mut r, &mut inp);
        if app.quit {
            break;
        }
        let now = sys::now();
        let real_dt = (now - last) as f32;
        last = now;
        let dt = real_dt.clamp(0.0, 0.1);
        if real_dt > 0.0 {
            app.fps += (1.0 / real_dt - app.fps) * 0.05;
        }
        // Scripted runs freeze time so pictures are repeatable, except
        // online, where the conversation with the server needs a clock.
        let online = app.session.as_ref().map_or(false, |g| g.net.is_some());
        app.update(&mut r, if scripted && !online { opts.step.unwrap_or(0.0) } else { dt }, dt);
        app.draw(&mut r, &inp);
        if !first_frame_logged {
            first_frame_logged = true;
            phases.mark("first frame drawn");
        }
        app.auto_tune(real_dt, r.w);

        if scripted {
            let settled = app.session.as_ref().map_or(true, |g| g.world.settled);
            settled_frames = if settled { settled_frames + 1 } else { 0 };
            if settled_frames == 3 && showcase_pending {
                showcase_pending = false;
                settled_frames = 0;
                app.build_showcase();
            } else if settled_frames == 3 && creatures_pending {
                creatures_pending = false;
                settled_frames = 0;
                app.line_up_creatures();
            } else if settled_frames == 3 && run_pending.is_some() {
                settled_frames = 0;
                if let (Some(cmds), Some(g)) = (run_pending.take(), app.session.as_mut()) {
                    for c in cmds.split(';').map(|c| c.trim()).filter(|c| !c.is_empty()) {
                        let reply = g.command(&c.to_ascii_lowercase()).unwrap_or_else(|| String::from("NO SUCH COMMAND"));
                        logf!("voxelrust: {} -> {}", c, reply.to_ascii_lowercase());
                    }
                }
            } else if settled_frames >= 3 + opts.wait_frames && bench.is_none() {
                if let Some(g) = &app.session {
                    let ready = g.world.chunks.values().filter(|c| c.mesh.is_some()).count();
                    logf!("voxelrust: {} chunks ready in {:.2} s", ready, sys::now() - load_start);
                }
                if let Some(g) = app.session.as_ref().filter(|g| g.net.is_some()) {
                    let extra: String = crate::native::active().filter_map(|(_, n)| n.status).map(|status| alloc::format!("{}, ", status(g))).collect();
                    logf!("voxelrust: shared: {} creatures, {} chests, {}{}", g.mobs.list.len(), g.chests.len(), extra, if g.runs_the_world() { "run from here" } else { "run by another player" });
                }
                if let Some(net) = app.session.as_ref().and_then(|g| g.net.as_ref()) {
                    for p in net.players.values() {
                        logf!("voxelrust: player {} at {:.1} {:.1} {:.1}{}", p.name, p.pos[0], p.pos[1], p.pos[2], if p.seen { "" } else { " (no position yet)" });
                    }
                }
                if let Some(path) = &opts.screenshot {
                    if !sys::write_file(path, &r.screenshot()) {
                        return Err(alloc::format!("could not write {}", path));
                    }
                    logf!("voxelrust: wrote {}", path);
                }
                match opts.bench {
                    Some(n) => bench = Some((n, sys::now(), 0.0)),
                    None => break,
                }
            } else if let Some((left, start, worst)) = bench.as_mut() {
                let total = opts.bench.unwrap_or(1);
                if let (Some(_), Some(g)) = (opts.flypass, app.session.as_mut()) {
                    // Straight ahead, well above the ground, half a block a
                    // frame: new terrain has to keep arriving.
                    let f = crate::math::forward(g.player.yaw, 0.0);
                    g.player.flying = true;
                    g.player.pos[0] += (f.x * 0.5) as f64;
                    g.player.pos[2] += (f.z * 0.5) as f64;
                    let ground = crate::worldgen::column(g.world.seed, floord(g.player.pos[0]), floord(g.player.pos[2])).height;
                    g.player.pos[1] += ((ground + 24) as f64 - g.player.pos[1]) * 0.05;
                    g.player.vel = [0.0; 3];
                    if !g.world.is_loaded(floord(g.player.pos[0] + (f.x * 48.0) as f64), floord(g.player.pos[2] + (f.z * 48.0) as f64)) {
                        unloaded_frames += 1;
                    }
                    let done = total - *left;
                    if done > 0 && done % (total / 4).max(1) == 0 && done / (total / 4).max(1) <= 4 {
                        sys::make_dir(&alloc::format!("{}screenshots/", app.base));
                        let path = alloc::format!("{}screenshots/flypass-{}.bmp", app.base, done / (total / 4).max(1));
                        sys::write_file(&path, &r.screenshot());
                    }
                } else if let Some(g) = app.session.as_mut() {
                    // Turn slowly so the whole surroundings get drawn.
                    g.player.yaw += 0.02;
                }
                // (No glFinish here: the game itself never calls it, and the
                // buffer swap already paces the frames.)
                // The first frame's time still includes loading.
                if *left < opts.bench.unwrap_or(1) {
                    *worst = worst.max(real_dt as f64);
                }
                *left -= 1;
                if *left == 0 {
                    let n = opts.bench.unwrap_or(1);
                    let total = sys::now() - *start;
                    logf!(
                        "voxelrust: bench: {} frames, {:.2} ms average ({:.0} FPS), {:.2} ms worst, {} quads, view distance {}",
                        n,
                        total * 1000.0 / n as f64,
                        n as f64 / total,
                        *worst * 1000.0,
                        r.stats.quads,
                        app.distance
                    );
                    if opts.flypass.is_some() {
                        logf!("voxelrust: flypass: {} blocks flown, terrain three chunks ahead was missing for {} of {} frames", n / 2, unloaded_frames, n);
                    }
                    break;
                }
            }
        }
        unsafe { (api.SDL_GL_SwapWindow)(window) };
        // On Mac OS X 10.6 a swap interval set while the window is still
        // appearing is sometimes lost, leaving the game locked to the
        // display's refresh rate at random. Saying it again is free.
        frame_count = frame_count.wrapping_add(1);
        if frame_count % 30 == 0 {
            unsafe { (api.SDL_GL_SetSwapInterval)((app.settings.vsync && !scripted) as c_int) };
        }
        // Do not spin the CPU in menus, and honour the frame-rate limit.
        let in_menu = !matches!(app.screen, Screen::Game | Screen::Inventory);
        let limit = if in_menu && !scripted { 60 } else { app.settings.max_fps };
        if limit > 0 && (in_menu || !app.settings.vsync) {
            let spent = sys::now() - frame_start;
            let want = 1.0 / limit as f64;
            if spent < want {
                unsafe { (api.SDL_Delay)(((want - spent) * 1000.0) as u32) };
            }
        }
        #[cfg(target_arch = "wasm32")]
        return Ok(true);
    }

    app.close_world(&mut r);
    if !scripted {
        sys::write_file_atomic(&app.settings_path, app.settings.to_text().as_bytes());
    }
    // Close the sound device before SDL goes.
    drop(core::mem::replace(&mut app.audio, Audio::silent()));
    unsafe {
        (api.SDL_GL_DeleteContext)(ctx);
        (api.SDL_DestroyWindow)(window);
        (api.SDL_Quit)();
    }
    // An update installed during play starts now, with everything saved and closed.
    #[cfg(not(target_arch = "wasm32"))]
    crate::update::restart(&restart_exe, &restart_folder);
    Ok(false)
    };
    #[cfg(target_arch = "wasm32")]
    return sys::web::run_frames(alloc::boxed::Box::new(frames));
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut frames = frames;
        frames().map(|_| ())
    }
}

/// Read the pack files and build the content from them (after the built-in
/// data). Also returns the packs' text joined together, which is what a
/// server sends to its players.
/// Every mod in a mods folder, in name order: `<folder>/<mod>/pack.txt`
/// for a mod with its own folder of textures and models, or a lone
/// `<folder>/<mod>.txt`.
fn mods_in(folder: &str) -> Vec<String> {
    let mut found = Vec::new();
    for name in sys::list_dir(folder) {
        let path = alloc::format!("{}{}", folder, name);
        if name.to_ascii_lowercase().ends_with(".zip") {
            // A mod in a zip: each pack inside it, by a path that reads through the archive.
            if let Some(entries) = sys::read_file(&path).and_then(|z| crate::util::zip::entries(&z)) {
                for e in entries {
                    if e.name == "pack.txt" || e.name.ends_with("/pack.txt") {
                        found.push(alloc::format!("{}!/{}", path, e.name));
                    }
                }
            }
        } else if name.ends_with(".txt") && name != "mods.txt" && !name.eq_ignore_ascii_case("README.txt") {
            found.push(path);
        } else {
            let pack = alloc::format!("{}/pack.txt", path);
            if sys::read_file(&pack).is_some() {
                found.push(pack);
            } else {
                // A mod unpacked from an archive often sits one folder down.
                for inner in sys::list_dir(&path) {
                    let pack = alloc::format!("{}/{}/pack.txt", path, inner);
                    if sys::read_file(&pack).is_some() {
                        found.push(pack);
                        break;
                    }
                }
            }
        }
    }
    found
}

/// The name a mod goes by: its folder, or its file without `.txt`, or the
/// name of its zip.
fn mod_name(path: &str) -> String {
    let p = path.strip_suffix("/pack.txt").unwrap_or(path);
    // Inside a zip (`mods/name.zip!/folder`): the zip's name.
    let p = match p.find(".zip!") {
        Some(i) => &p[..i + 4],
        None => p,
    };
    let p = p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p);
    let p = p.strip_suffix(".zip").unwrap_or(p);
    String::from(p.strip_suffix(".txt").unwrap_or(p))
}

/// A file a mod names: from disk, or from inside a zip (`mods/name.zip!/inner/file`).
fn read_mod_file(path: &str) -> Option<Vec<u8>> {
    match path.find(".zip!/") {
        Some(i) => crate::util::zip::read(&sys::read_file(&path[..i + 4])?, &path[i + 6..]),
        None => sys::read_file(path),
    }
}

/// Bring a mod in from somewhere else on the machine: a folder, a .zip,
/// or a lone pack file. It is copied into the data folder's `mods`, where
/// the game then finds it. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
fn import_mod(mods: &str, from: &str) -> String {
    let from = from.trim().trim_end_matches(|c| c == '/' || c == '\\');
    // The path goes on a command line: nothing that could end the quotes.
    if from.is_empty() || from.chars().any(|c| matches!(c, '"' | '`' | '$' | '\n' | '\r' | '%' | '!')) {
        return String::from("TYPE WHERE THE MOD IS: A FOLDER, A .ZIP OR A PACK FILE");
    }
    let name: String = mod_name(from).trim_end_matches(".zip").chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).take(32).collect();
    if name.is_empty() {
        return String::from("THAT NAME WILL NOT DO FOR A MOD");
    }
    let dest = alloc::format!("{}/{}", mods, name);
    let win = cfg!(windows);
    sys::run_command(&if win { alloc::format!("mkdir \"{}\" 2>nul", mods.replace('/', "\\")) } else { alloc::format!("mkdir -p \"{}\"", mods) });
    let lower = from.to_ascii_lowercase();
    let ok = if lower.ends_with(".txt") {
        match sys::read_file(from) {
            Some(bytes) => sys::write_file(&alloc::format!("{}.txt", dest), &bytes),
            None => false,
        }
    } else if lower.ends_with(".zip") {
        // Windows 10 and later, macOS and most Linux systems can unpack a zip.
        let cmd = if win {
            alloc::format!("mkdir \"{0}\" 2>nul & tar -xf \"{1}\" -C \"{0}\"", dest.replace('/', "\\"), from)
        } else {
            alloc::format!("mkdir -p \"{0}\" && (unzip -o -q \"{1}\" -d \"{0}\" || tar -xf \"{1}\" -C \"{0}\")", dest, from)
        };
        sys::run_command(&cmd) == 0
    } else {
        let cmd = if win { alloc::format!("xcopy /E /I /Y /Q \"{}\" \"{}\" >nul", from, dest.replace('/', "\\")) } else { alloc::format!("rm -rf \"{1}\" && cp -R \"{0}\" \"{1}\"", from, dest) };
        sys::run_command(&cmd) == 0
    };
    if !ok {
        return String::from("COULD NOT COPY THAT. IS THE PATH RIGHT?");
    }
    let found = mods_in(&alloc::format!("{}/", mods)).iter().any(|m| mod_name(m) == name || m.contains(&alloc::format!("/{}/", name)));
    if found {
        alloc::format!("ADDED THE MOD {}", name.to_ascii_uppercase())
    } else {
        String::from("COPIED, BUT THERE IS NO PACK.TXT IN IT")
    }
}

#[cfg(target_arch = "wasm32")]
fn import_mod(_: &str, _: &str) -> String {
    String::from("NOT POSSIBLE IN A BROWSER")
}

/// Every mod there is to switch on or off, as (name, version): those in
/// the mods folders. (No mod is built into the program.)
fn all_mods(pack_paths: &[String]) -> Vec<(String, String)> {
    let mut mods: Vec<(String, String)> = Vec::new();
    for p in pack_paths {
        if let Some(folder) = p.strip_suffix("/*") {
            for m in mods_in(&alloc::format!("{}/", folder)) {
                let n = mod_name(&m);
                if !mods.iter().any(|x| x.0 == n) {
                    let version = read_mod_file(&m).map(|b| crate::net::modsync::bundle::read_meta(&String::from_utf8_lossy(&b)).version).unwrap_or_default();
                    mods.push((n, version));
                }
            }
        }
    }
    mods
}

fn load_content(pack_paths: &[String], off: &str) -> (Registry, crate::client::pack_text::PackText) {
    let mut sources: Vec<(String, String)> = alloc::vec![(String::from("base.txt"), String::from(block::BASE_DATA))];
    let is_off = |name: &str| off.split(',').any(|o| o == name);
    // Mods are looked for afresh each time, so one dropped into the folder
    // while the game runs is there after a reload.
    let mut all: Vec<String> = Vec::new();
    for p in pack_paths {
        match p.strip_suffix("/*") {
            Some(folder) => all.extend(mods_in(&alloc::format!("{}/", folder)).into_iter().filter(|m| !is_off(&mod_name(m)))),
            None => all.push(p.clone()),
        }
    }
    // The same mod found in two places (beside the program and in the
    // folder the game was started from, say) is loaded once.
    let mut named: Vec<String> = Vec::new();
    all.retain(|p| {
        let n = mod_name(p);
        let fresh = !crate::native::in_mods_folder(p) || !named.contains(&n);
        named.push(n);
        fresh
    });
    for p in &all {
        if let Some(bytes) = read_mod_file(p) {
            logf!("voxelrust: content from {}", p);
            sources.push((p.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    // Remember which files the packs pull in (textures, models, includes), to send to
    // players who join: only their names, the bundle reads them if it is ever made.
    let mut used: Vec<String> = Vec::new();
    let mut seen: alloc::collections::BTreeSet<String> = alloc::collections::BTreeSet::new();
    let registry = Registry::from_sources(&refs, &mut |p| {
        let data = read_mod_file(p);
        if data.is_some() && seen.insert(String::from(p)) {
            used.push(String::from(p));
        }
        data
    });
    drop(refs);
    // The scripts of the mods in use (`scripts/*.rs` in a mod's folder; not
    // read from inside a zip).
    let mut scripted: Vec<(String, Vec<(String, Vec<u8>)>)> = Vec::new();
    for p in &all {
        let Some(folder) = p.strip_suffix("pack.txt") else { continue };
        if p.contains(".zip!") {
            continue;
        }
        let dir = alloc::format!("{}scripts/", folder);
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for f in sys::list_dir(&dir) {
            if f.ends_with(".rs") {
                if let Some(b) = sys::read_file(&alloc::format!("{}{}", dir, f)) {
                    files.push((alloc::format!("scripts/{}", f), b));
                }
            }
        }
        if !files.is_empty() {
            scripted.push((mod_name(p), files));
        }
    }
    crate::script::ScriptHost::load_local(&scripted);
    let packs: Vec<(String, String)> = sources.into_iter().skip(1).collect();
    (registry, crate::client::pack_text::PackText::lazy(packs, used))
}

/// The content a server dictates: the built-in data and what it sent.
fn server_content(pack: &[u8]) -> Registry {
    let files = if pack.is_empty() {
        Vec::new()
    } else {
        crate::save::unbundle_labels(pack).unwrap_or_else(|| {
            logf!("voxelrust: the server's content could not be read; using the built-in content");
            Vec::new()
        })
    };
    let mut sources: Vec<(&str, String)> = alloc::vec![("base.txt", String::from(block::BASE_DATA))];
    for (name, data) in &files {
        if let Some(path) = name.strip_prefix("pack:") {
            sources.push((path, String::from_utf8_lossy(data).into_owned()));
        }
    }
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let mut registry = Registry::from_sources(&refs, &mut |p| files.iter().find(|f| f.0.strip_prefix("file:") == Some(p)).map(|f| f.1.clone()));
    // What a server sends may define things, but the lines kept for the
    // mods' own code (addresses to fetch from, for one) stay our own.
    registry.lines = block::reg().lines.clone();
    registry
}

/// Write every texture as `<id>.png` (16 x 16, RGBA) in `dir`, with the
/// whole atlas as `atlas.png`, a list of the ids, and a note on editing them.
fn export_textures(r: &block::Registry, dir: &str) -> Result<usize, String> {
    let dir = if dir.ends_with('/') || dir.ends_with('\\') { String::from(dir) } else { alloc::format!("{}/", dir) };
    sys::run_command(&if cfg!(windows) { alloc::format!("mkdir \"{}\" 2>nul", dir.replace('/', "\\")) } else { alloc::format!("mkdir -p \"{}\"", dir) });
    let mut list = String::new();
    for (i, t) in r.tiles.iter().enumerate() {
        // The picture the data describes, not one already in textures/.
        let px = crate::textures::described_pixels(i, t);
        if !sys::write_file(&alloc::format!("{}{}.png", dir, t.id), &crate::png::encode(16, 16, &px)) {
            return Err(alloc::format!("cannot write {}{}.png", dir, t.id));
        }
        list.push_str(&alloc::format!("{} {}\n", i, t.id));
    }
    sys::write_file(&alloc::format!("{}atlas.png", dir), &crate::png::encode(crate::textures::ATLAS, crate::textures::ATLAS, &crate::textures::build_atlas_described(r)));
    sys::write_file(&alloc::format!("{}textures.txt", dir), list.as_bytes());
    sys::write_file(&alloc::format!("{}README.txt", dir), b"One PNG per texture, 16 x 16 pixels, RGBA (transparent pixels are holes).\nEdit any of them with an image editor and copy the ones you changed into the\n`textures` folder of the game's data folder (it is made when the game starts):\nthey replace the game's own the next time the game starts, or when packs are\nreloaded. PNG, JPEG, GIF, BMP and TGA files of any size are read (a picture of\nanother size is fitted to 16 x 16). atlas.png is every texture in one\npicture, for looking at; textures.txt lists the number and id of each one.\n");
    Ok(r.tiles.len())
}

/// Set to stop a server started from the multiplayer menu.
static HOST_STOP: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Open the server for the world `name`: its small save (seed, kind and
/// every edit) is read, or made on first use.
fn open_server(name: &str, seed: Option<u32>, new_kind: Option<String>, port: u16, players: usize, base: &str, pack: &[u8], mods: Vec<crate::net::modsync::serve::ServedMod>, settings: &Settings) -> Result<(crate::server::Server, String), String> {
    use crate::server::{Config, Server};
    if name.is_empty() || name != crate::settings::clean_world_name(name, 24) {
        return Err(String::from("world names use only letters, digits, spaces, - and _ (24 at most)"));
    }
    let meta_path = alloc::format!("{}{}.server.txt", crate::session::world_dir(base), name);
    let edits_path = alloc::format!("{}{}.server-edits.txt", crate::session::world_dir(base), name);
    // A server world saved before `worlds/` existed sits loose in the data
    // folder: bring its files over, or the server would start a new world.
    if sys::read_file(&meta_path).is_none() {
        sys::make_dir(&crate::session::world_dir(base));
        for f in sys::list_dir(base) {
            if f.starts_with(&alloc::format!("{}.server", name)) {
                if let Some(data) = sys::read_file(&alloc::format!("{}{}", base, f)) {
                    if sys::write_file(&alloc::format!("{}{}", crate::session::world_dir(base), f), &data) {
                        sys::remove_file(&alloc::format!("{}{}", base, f));
                    }
                }
            }
        }
    }
    let meta = sys::read_file(&meta_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let seed = crate::settings::kv(&meta, "seed").and_then(|v| v.parse().ok()).unwrap_or_else(|| seed.unwrap_or(sys::now().to_bits() as u32 ^ 0x5EED_1234));
    let kind = crate::settings::kv(&meta, "kind").map(String::from).unwrap_or_else(|| new_kind.unwrap_or_else(|| WorldKind::Normal.to_text()));
    sys::write_file(&meta_path, alloc::format!("seed={}\nkind={}\n", seed, kind).as_bytes());
    let cfg = Config {
        port,
        seed,
        kind: kind.clone(),
        max_players: players,
        content: crate::proto::content_checksum(),
        states: reg().blocks.len() as u16,
        pack: pack.to_vec(),
        mods,
    };
    let mut server = Server::bind(cfg).map_err(|e| alloc::format!("could not listen on port {}: {:?}", port, e))?;
    // Operators and bans are kept beside the world: "op NAME" / "ban NAME".
    if let Some(text) = sys::read_file(&alloc::format!("{}{}.server-ops.txt", crate::session::world_dir(base), name)) {
        for line in String::from_utf8_lossy(&text).lines() {
            match line.split_once(' ') {
                Some(("op", n)) => server.ops.push(String::from(n)),
                Some(("ban", n)) => server.banned.push(String::from(n)),
                _ => {
                    server.load_perm_line(line);
                }
            }
        }
    }
    if let Some(text) = sys::read_file(&edits_path) {
        server.load_text(&String::from_utf8_lossy(&text));
    }
    // The player's own settings when hosting from the menu, so the map relay
    // they set keeps working; a dedicated server passes its defaults.
    for (_, n) in crate::native::active() {
        if let Some(start) = n.start {
            start(base, settings);
        }
    }
    logf!("voxelrust: serving world `{}` (seed {}, {}) on port {}, {} saved edits", name, seed, kind, server.port(), server.edits.len());
    Ok((server, edits_path))
}

/// Run a server until `HOST_STOP` is set (never, for `--server`). Edits
/// are written every 20 seconds and once more on the way out.
/// Returns true when it stops because an update is installed and nobody is
/// playing: the caller then restarts the server into the new version.
fn server_loop(mut server: crate::server::Server, edits_path: String, quiet: bool) -> bool {
    use core::sync::atomic::Ordering;
    let mut last = sys::now();
    let mut save_timer = 0.0f32;
    while !HOST_STOP.load(Ordering::Relaxed) && !update_ready_and_empty(&server) {
        let now = sys::now();
        let dt = ((now - last) as f32).clamp(0.0, 1.0);
        last = now;
        let lines = server.tick(dt, &mut |line| {
            // Something for a mod's own code to answer (map data it can
            // reach and the asker cannot, say).
            let w: Vec<&str> = line.split(' ').collect();
            crate::native::active().filter_map(|(_, n)| n.serve).find_map(|serve| serve(&w))
        });
        if !quiet {
            for l in lines {
                logf!("voxelrust: {}", l);
            }
        }
        if server.ops_dirty {
            server.ops_dirty = false;
            let mut text = String::new();
            for o in &server.ops {
                text.push_str(&alloc::format!("op {}\n", o));
            }
            for b in &server.banned {
                text.push_str(&alloc::format!("ban {}\n", b));
            }
            text.push_str(&server.perm_text());
            sys::write_file(&edits_path.replace(".server-edits.txt", ".server-ops.txt"), text.as_bytes());
        }
        save_timer += dt;
        if save_timer > 20.0 && server.dirty {
            save_timer = 0.0;
            server.dirty = false;
            if !sys::write_file_atomic(&edits_path, server.save_text().as_bytes()) {
                logf!("voxelrust: could not write {}", edits_path);
            }
        }
        sys::sleep_ms(4);
    }
    if server.dirty {
        sys::write_file_atomic(&edits_path, server.save_text().as_bytes());
    }
    update_ready_and_empty(&server)
}

/// An update is installed and no player is connected: time to restart.
fn update_ready_and_empty(server: &crate::server::Server) -> bool {
    crate::update::state() == crate::update::UPDATED && server.players() == 0
}

/// Asked to stop (kill, a service manager, Ctrl+C): leave the server loop,
/// which saves the world's edits and frees the port.
extern "C" fn on_terminate(_: c_int) {
    HOST_STOP.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// `voxelrust --server`: host a world for others, with no window.
fn run_server(opts: &Opts, base: &str, pack_text: &[u8], pack_paths: &[String], mods_off: &str) -> Result<(), String> {
    let name = opts.world.clone().unwrap_or_else(|| String::from("server"));
    let new_kind = crate::native::active().filter_map(|(_, n)| n.option_kind).find_map(|kind| kind());
    let port = opts.port.unwrap_or(crate::proto::DEFAULT_PORT);
    let only = opts.serve_mods.as_deref().and_then(crate::net::modsync::serve::parse_serve_list);
    // The server keeps itself up to date too: an update is installed as soon
    // as it is found, and the server restarts into it once nobody is playing.
    sys::handle_termination(on_terminate);
    #[allow(unused_variables)] // the web build never restarts
    let restart_exe = sys::exe_path();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = crate::update::consume_restart_marker(base);
    if !opts.no_update {
        let (scratch, install) = (String::from(base), true);
        // The mirror: `--update-mirror`, or the one in the data folder's settings.txt.
        let mirror = opts.update_mirror.clone().unwrap_or_else(|| {
            sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| Settings::parse(&String::from_utf8_lossy(&b)).update_mirror).unwrap_or_default()
        });
        sys::thread::spawn(alloc::boxed::Box::new(move || crate::update::watch(BUILD, &scratch, install, &mirror)));
    }
    let (server, edits_path) = open_server(&name, opts.seed, new_kind, port, opts.players.unwrap_or(16), base, pack_text, crate::client::mods::serving::served(pack_paths, mods_off, only.as_deref()), &Settings::default())?;
    let address = sys::net::local_address().unwrap_or_else(|| String::from("<this machine's address>"));
    logf!("voxelrust: players join with:  voxelrust --join {}:{}", address, server.port());
    if server_loop(server, edits_path, false) {
        logf!("voxelrust: restarting into the new version");
        #[cfg(not(target_arch = "wasm32"))]
        crate::update::restart(&restart_exe, base);
    }
    Ok(())
}

pub fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut args = Vec::new();
    for i in 0..argc.max(0) as usize {
        args.push(unsafe { sys::from_cstr(*argv.add(i)) });
    }
    match run(args) {
        Ok(()) => 0,
        Err(msg) => {
            logf!("voxelrust: {}", msg);
            if sdl::loaded() {
                let m = sys::cstr(&msg);
                unsafe {
                    (sdl::sdl().SDL_ShowSimpleMessageBox)(0x10, b"VoxelRust\0".as_ptr() as *const c_char, m.as_ptr() as *const c_char, core::ptr::null_mut());
                }
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_click_on_the_creative_list_picks_up_or_lets_go_but_not_both() {
        let stone = Stack { item: 3, count: 64 };
        let dirt = Stack { item: 4, count: 1 };
        // Empty hand on a palette cell: the item is picked up, and stays.
        assert_eq!(creative_hand(None, Some(stone), true, true), Some(stone));
        // Carrying something and clicking a palette cell: thrown away.
        assert_eq!(creative_hand(Some(dirt), Some(stone), true, true), None);
        // Carrying something and clicking empty space in the list: deleted.
        assert_eq!(creative_hand(Some(dirt), None, true, true), None);
        // Empty hand on empty space: nothing happens.
        assert_eq!(creative_hand(None, None, true, true), None);
        // No click: the hand is unchanged, carried or not.
        assert_eq!(creative_hand(Some(dirt), Some(stone), true, false), Some(dirt));
        assert_eq!(creative_hand(None, None, false, false), None);
        // Carrying something and clicking outside the list: kept (it is dropped elsewhere).
        assert_eq!(creative_hand(Some(dirt), None, false, true), Some(dirt));
    }

    use super::*;
    use alloc::string::ToString;

    fn args(s: &str) -> Vec<String> {
        core::iter::once("voxelrust").chain(s.split_whitespace()).map(|s| s.to_string()).collect()
    }

    #[test]
    fn arguments_parse() {
        let o = parse_args(&args("--seed 42 --distance 99 --size 800x600 --pos 1 2.5 -3 --look 90 -100 --pack a.txt --pack b.txt --mode survival")).unwrap();
        assert_eq!(o.seed, Some(42));
        assert_eq!(o.distance, Some(48));
        assert_eq!(o.size, Some((800, 600)));
        assert_eq!(o.pos, Some([1.0, 2.5, -3.0]));
        assert!((o.look.unwrap().1 + 89.0 * PI / 180.0).abs() < 1e-5);
        assert_eq!(o.packs, ["a.txt", "b.txt"]);
        // A double-click from Finder must not be refused.
        assert!(parse_args(&["voxelrust".to_string(), "-psn_0_12345".to_string()]).is_ok());
        assert_eq!(o.mode, Some(GameMode::Survival));
        // Words work as seeds too, and the same word gives the same seed.
        assert_eq!(seed_from("hello"), seed_from("hello"));
        assert_ne!(seed_from("hello"), seed_from("world"));
        assert_eq!(seed_from("77"), 77);
        assert!(parse_args(&args("--size big")).is_err());
        assert!(parse_args(&args("--seed")).is_err());
        assert!(parse_args(&args("--mode hard")).is_err());
        assert!(parse_args(&args("--nope")).is_err());
    }
}
