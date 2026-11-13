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
                      uses --world, --seed, --earth, --port and --players
  --port N            server port (default 25590)
  --players N         most players at once on the server (default 16)
  --earth LAT LON     make the new world real-world terrain starting there
  --scale M V         with --earth: metres per block across and up (30 75)
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
  --export FILE       with --world NAME: write that world as one file, and exit
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
                      (one of each) or walking (the same, mid-stride)

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
    earth: Option<(f64, f64)>,
    join: Option<String>,
    host: Option<String>,
    server: bool,
    port: Option<u16>,
    players: Option<usize>,
    scale: Option<(u32, u32)>,
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
    screenshot: Option<String>,
    bench: Option<u32>,
    /// Fly straight ahead for this many frames, timing them (a render test).
    flypass: Option<u32>,
    /// Chat commands to carry out once the world has loaded.
    run: Option<String>,
    write_icon: Option<String>,
    no_update: bool,
    icon_size: usize,
    version: bool,
    export: Option<String>,
    import: Option<String>,
    pos: Option<[f64; 3]>,
    look: Option<(f32, f32)>,
    time: Option<f32>,
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
            "--earth" => {
                let lat = num::<f64>(&next(&mut i, a)?, a)?;
                let lon = num::<f64>(&next(&mut i, a)?, a)?;
                if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    return Err(String::from("--earth: latitude must be -85..85 and longitude -180..180"));
                }
                o.earth = Some((lat, lon));
            }
            "--scale" => {
                let m = num::<u32>(&next(&mut i, a)?, a)?;
                let v = num::<u32>(&next(&mut i, a)?, a)?;
                o.scale = Some((m.clamp(1, 100_000), v.clamp(1, 10_000)));
            }
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
            "--screenshot" => o.screenshot = Some(next(&mut i, a)?),
            "--write-icon" => o.write_icon = Some(next(&mut i, a)?),
            "--icon-size" => o.icon_size = num::<usize>(&next(&mut i, a)?, a)?.clamp(8, 1024),
            "--version" => o.version = true,
            "--no-update" => o.no_update = true,
            "--export" => o.export = Some(next(&mut i, a)?),
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
            // Finder on old Mac OS X adds a process serial number when an
            // app is double-clicked; it is not for us.
            _ if a.starts_with("-psn_") => {}
            _ => return Err(alloc::format!("unknown option `{}` (try --help)", a)),
        }
        i += 1;
    }
    Ok(o)
}

/// A map tile on its way to the screen.
enum MapTile {
    /// Asked for; a worker is fetching it.
    Loading,
    Ready(u32),
    Missing,
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
    Join,
    Chat,
    /// The crafting table's recipe list.
    Craft,
    /// A chest's contents, at this position.
    Chest,
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

/// The minimap is this many pixels square.
const MINIMAP: usize = 64;
const TABS: [&str; 6] = ["VIDEO", "AUDIO", "CONTROLS", "GAME", "SKIN", "MODS"];

/// The player's skin as 64 x 64 RGBA: a tinted copy of the default body, or
/// `skin.png` from the data folder. `None` if a custom skin cannot be used.
fn skin_pixels(base: &str, r: &Renderer, name: &str) -> Option<Vec<u8>> {
    if name == "custom" {
        let img = crate::png::decode(&sys::read_file(&alloc::format!("{}skin.png", base))?)?;
        return if img.w == 64 && img.h == 64 { Some(img.rgba) } else { None };
    }
    let model = r.models.iter().find(|m| m.id == "player")?;
    let img = model.model.texture.as_ref()?;
    let tint = crate::settings::SKINS.iter().find(|s| s.0 == name).map_or([255, 255, 255], |s| s.1);
    let mut px = img.rgba.clone();
    for p in px.chunks_exact_mut(4) {
        for c in 0..3 {
            // "snow" brightens instead of tinting.
            p[c] = if name == "snow" { 128 + p[c] / 2 } else { (p[c] as u32 * tint[c] as u32 / 255) as u8 };
        }
    }
    Some(px)
}

/// Everything except the renderer and the frame's input, which are passed
/// alongside so that screens can borrow them freely.
struct App {
    window: *mut c_void,
    audio: Audio,
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
    /// Camera behind the player instead of in their head.
    third_person: bool,
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
    form_earth: bool,
    form_flat: bool,
    form_place: usize,
    form_address: String,
    /// World name typed for hosting, and the server thread while hosting.
    form_host: String,
    host: Option<sys::thread::Thread>,
    /// This machine's network address, looked up once.
    my_address: Option<Option<String>>,
    form_chat: String,
    /// The player's own skin texture, other players' by id, and the PNG to
    /// send to a server.
    skin_tex: u32,
    remote_skins: BTreeMap<u16, u32>,
    skin_png: Vec<u8>,
    skin_sent: bool,
    /// Pack files read at start-up, for reading again on request.
    pack_paths: Vec<String>,
    /// The packs in use, bundled; empty when there are none.
    pack_text: Vec<u8>,
    /// Playing with content a server sent rather than our own packs.
    server_content: bool,
    /// Set by the settings button or `/reload`; acted on between frames.
    want_reload: bool,
    /// The crafting screen was opened at an oven, not a table.
    at_oven: bool,
    show_walking: bool,
    mods_scroll: i32,
    at_furnace: bool,
    /// Seconds and frames counted while timing full screen.
    tune: (f32, u32),
    worlds_scroll: i32,
    /// The minimap: its texture, the picture being painted bit by bit,
    /// how far that has got, and where each is centred (blocks).
    minimap_tex: u32,
    minimap_px: Vec<u8>,
    minimap_cursor: usize,
    minimap_origin: (i32, i32),
    minimap_shown: (i32, i32),
    /// Blocks per pixel of the picture on show, and whether M enlarged it.
    minimap_scale: i32,
    minimap_big: bool,
    /// World file name typed on the worlds screen.
    form_import: String,
    form_skin_path: String,
    /// Name tags to draw this frame: screen position and text.
    tags: Vec<(f32, f32, String)>,
    form_scale: i32,
    form_features: bool,
    form_vscale: i32,
    /// The map screen: centre in "unit" map coordinates (0..1) and zoom.
    map_center: (f64, f64),
    map_zoom: i32,
    map_tiles: BTreeMap<(u32, u32, u32), MapTile>,
    status: String,
    tab: usize,
    /// Index into ACTIONS of the key being rebound.
    rebinding: Option<usize>,
    inv_tab: usize,
    inv_scroll: i32,
    chest_at: [i32; 3],
    /// When the chest screen shows a vehicle's hold instead of a chest.
    chest_vehicle: Option<usize>,
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
        self.screen = screen;
        self.focus = 0;
        self.rebinding = None;
        self.buttons = [false; 4];
        if !self.scripted {
            unsafe { (sdl::sdl().SDL_SetRelativeMouseMode)((screen == Screen::Game) as c_int) };
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
        let px = match skin_pixels(&self.base, r, &self.settings.skin) {
            Some(px) => px,
            None => {
                self.status = String::from("NO USABLE SKIN.PNG (64 X 64) IN THE DATA FOLDER");
                self.settings.skin = String::from("default");
                match skin_pixels(&self.base, r, "default") {
                    Some(px) => px,
                    None => return,
                }
            }
        };
        self.skin_tex = r.add_texture(64, 64, &px);
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
        sys::write_file(&self.settings_path, self.settings.to_text().as_bytes());
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
        self.session = Some(s);
        self.distance = self.settings.distance;
        self.set_screen(Screen::Game);
    }

    fn close_world(&mut self, r: &mut Renderer) {
        if let Some(mut s) = self.session.take() {
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
        let name = crate::settings::clean_name(&self.form_host, 24);
        let port = crate::proto::DEFAULT_PORT;
        let (server, edits) = match open_server(if name.is_empty() { "shared" } else { &name }, None, None, port, 16, &self.base, &self.pack_text) {
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
            server_loop(p.0, p.1, true)
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
        let path = alloc::format!("{}screenshot-{}.bmp", self.base, (sys::now() * 1000.0) as u64);
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
                } else if sc == key::F5 {
                    self.third_person = !self.third_person;
                } else if sc == key::F1 {
                    self.hud_hidden = !self.hud_hidden;
                } else if (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Inventory | Screen::Craft | Screen::Chest => {
                if sc == key::ESCAPE || sc == k_inv {
                    self.set_screen(Screen::Game);
                } else if (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Pause if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Chat if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Join if sc == key::ESCAPE && !typing => self.set_screen(Screen::Title),
            Screen::Map if sc == key::ESCAPE || sc == k_map => self.set_screen(Screen::Game),
            Screen::Settings if sc == key::ESCAPE && !typing => self.leave_settings(),
            Screen::NewWorld if sc == key::ESCAPE => self.set_screen(Screen::Worlds),
            Screen::Worlds if sc == key::ESCAPE => self.set_screen(Screen::Title),
            _ => {}
        }
    }

    fn leave_settings(&mut self) {
        self.settings.player_name = crate::settings::clean_name(&self.settings.player_name, 16);
        if self.settings.player_name.is_empty() {
            self.settings.player_name = String::from("Player");
        }
        if !self.scripted {
            sys::write_file(&self.settings_path, self.settings.to_text().as_bytes());
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
                sdl::EV_TEXTINPUT if self.swallow_text => self.swallow_text = false,
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

    fn update(&mut self, r: &mut Renderer, dt: f32, real_dt: f32) {
        self.help_timer -= dt;
        self.ui_clock += real_dt;
        // In multiplayer the world never stops.
        let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
        let playing = online || matches!(self.screen, Screen::Game | Screen::Inventory | Screen::Dead | Screen::Chat | Screen::Map | Screen::Craft | Screen::Chest);
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
            if playing {
                g.update(dt, &controls, distance, self.settings.far_distance, r, &mut self.audio);
                g.weather_update(dt, self.settings.weather, self.settings.disasters && reg().has("disasters"), &mut self.audio);
                g.place_level = self.settings.place_notices;
                died = g.dead;
            } else {
                // Paused: keep loading terrain, but nothing moves.
                let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
                g.world.update(cx, cz, distance, &mut g.pool, r, &mut g.store);
            }
        }
        if let Some(g) = self.session.as_mut() {
            if let (Some(net), false) = (g.net.as_mut(), self.skin_sent) {
                if self.skin_png.len() <= crate::proto::MAX_SKIN {
                    net.send_skin(&self.skin_png);
                }
                self.skin_sent = true;
            }
            if let Some(pack) = g.server_pack.take() {
                // Play with the server's blocks and items, not our own.
                if pack != self.pack_text {
                    let new = server_content(&pack);
                    logf!("voxelrust: using the server's content ({} bytes of packs, {} block states)", pack.len(), new.blocks.len());
                    for w in &new.warnings {
                        logf!("voxelrust: server data: {}", w);
                    }
                    g.reload_content(new, r);
                    r.rebuild_atlas();
                    r.load_models();
                    self.pack_text = pack;
                    self.server_content = true;
                    g.say("USING THE SERVER'S CONTENT");
                }
            }
            for (id, png) in g.skin_events.drain(..) {
                if let Some(img) = crate::png::decode(&png).filter(|i| i.w == 64 && i.h == 64) {
                    let tex = r.add_texture(64, 64, &img.rgba);
                    self.remote_skins.insert(id, tex);
                }
            }
        }
        self.audio.tick(real_dt);
        self.paint_minimap(r);
        if core::mem::take(&mut self.want_reload) {
            self.reload_packs(r);
        }
        if died && self.screen != Screen::Dead {
            self.set_screen(Screen::Dead);
        }
        if let Some(i) = self.session.as_mut().and_then(|g| g.opened_vehicle.take()) {
            self.chest_vehicle = Some(i);
            self.set_screen(Screen::Chest);
        }
        if let Some((what, at)) = self.session.as_mut().and_then(|g| g.opened.take()) {
            self.chest_vehicle = None;
            self.chest_at = at;
            self.inv_scroll = 0;
            self.at_oven = what == block::Usable::Oven;
            self.at_furnace = what == block::Usable::Furnace;
            self.set_screen(if what == block::Usable::Chest { Screen::Chest } else { Screen::Craft });
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
            if self.settings.view_bobbing && g.player.on_ground && !g.player.flying {
                let speed = g.player.vel[0] * g.player.vel[0] + g.player.vel[2] * g.player.vel[2];
                if speed > 1.0 {
                    eye[1] += (sinf(g.clock * 11.0) * 0.035) as f64;
                }
            }
            if g.shake > 0.0 {
                // An earthquake rattles the view.
                eye[0] += (sinf(g.clock * 47.0) * g.shake * 0.25) as f64;
                eye[1] += (sinf(g.clock * 61.0) * g.shake * 0.18) as f64;
            }
            if self.third_person {
                // Pull the camera back, but not through walls.
                let back = crate::math::forward(g.player.yaw, g.player.pitch) * -1.0;
                let room = g.world.raycast(eye, back, 4.5).map_or(4.0, |hit| (hit.dist - 0.4).clamp(0.3, 4.0));
                eye = [eye[0] + (back.x * room) as f64, eye[1] + (back.y * room) as f64, eye[2] + (back.z * room) as f64];
            }
            let cam = Camera { pos: eye, yaw: g.player.yaw, pitch: g.player.pitch, fov: self.settings.fov as f32 };
            let head = g.world.get(floord(eye[0]), floord(eye[1]), floord(eye[2]));
            underwater = reg().kind[head as usize] == Kind::Liquid;
            let env = Env {
                time: g.time,
                distance: self.distance,
                clouds: self.settings.clouds,
                underwater,
                clock: g.clock,
                far_distance: if self.settings.far_distance > self.distance { self.settings.far_distance } else { 0 },
                rain: g.sky.rain,
                flash: g.flash,
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
            if let Some(crate::weather::Disaster::Tornado { x, z, .. }) = g.disaster {
                let ground = crate::worldgen::column(g.world.seed, x as i32, z as i32).height as f64;
                r.draw_funnel([(x as f64 - cam.pos[0]) as f32, (ground - cam.pos[1]) as f32, (z as f64 - cam.pos[2]) as f32], g.clock);
            }
            r.finish_world(&cam, &env);
        } else {
            r.clear([0.36, 0.56, 0.86]);
        }

        self.tags = new_tags;
        r.begin_2d();
        let (w, h) = (r.w as f32, r.h as f32);
        if self.session.is_none() {
            // Menus stand on a wall of the game's own blocks: stone below,
            // earth and turf above, dimmed so the buttons stand out, and
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
        let before = self.screen;
        let mut focus = self.focus;
        let clicked;
        {
            let mut ui = Ui { r: &mut *r, inp, s, focus: &mut focus, clicked_any: false, clock: self.ui_clock };
            match self.screen {
                Screen::Game => {}
                Screen::Title => self.screen_title(&mut ui),
                Screen::Worlds => self.screen_worlds(&mut ui),
                Screen::NewWorld => self.screen_new_world(&mut ui),
                Screen::Settings => self.screen_settings(&mut ui),
                Screen::Pause => self.screen_pause(&mut ui),
                Screen::Inventory | Screen::Craft | Screen::Chest => self.screen_inventory(&mut ui),
                Screen::Dead => self.screen_dead(&mut ui),
                Screen::Map => self.screen_map(&mut ui),
                Screen::Join => self.screen_join(&mut ui),
                Screen::Chat => self.screen_chat(&mut ui),
            }
            clicked = ui.clicked_any;
        }
        // Changing screen resets the focus; otherwise keep what the UI set.
        if self.screen == before && self.rebinding.is_none() {
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
            let d = &reg.entities[m.kind];
            let (model, colour, height) = m.look();
            // Things that glow are not dimmed by the dark.
            let l = light_at(m.body.pos).max(d.x.glow as f32 / 15.0);
            let mut tint = [0, 1, 2].map(|i| (colour[i] as f32 * l) as u8);
            if m.hurt > 0.0 {
                tint = [255, tint[1] / 3, tint[2] / 3];
            }
            let pose = crate::model::Pose { phase: m.phase, stride: m.stride, anim: d.x.anim, clock: m.clock, ..Default::default() };
            r.draw_model(model, &pose, p, m.body.yaw, height, tint);
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
        // Things lying on the ground glint.
        sparks.extend(g.drops.iter().map(|d| (rel([d.pos[0], d.pos[1] + 0.25, d.pos[2]]), [255, 236, 150])));
        r.draw_sparks(&sparks);
        for (i, v) in g.vehicles.list.iter().enumerate() {
            let p = rel(v.body.pos);
            if p[0] * p[0] + p[2] * p[2] > 96.0 * 96.0 || behind(p, 4.0) {
                continue;
            }
            // From inside, your own vehicle is only drawn in third person.
            if g.vehicles.riding == Some(i) && !self.third_person {
                continue;
            }
            let d = &reg.vehicles[v.kind];
            let l = (light_at(v.body.pos) * 255.0) as u8;
            let pose = crate::model::Pose { phase: v.phase, stride: 0.0, ..Default::default() };
            r.draw_model(d.model, &pose, p, v.body.yaw, d.height, [l, l, l]);
        }
        // Other players, with their names over their heads.
        if let (Some(net), Some(model)) = (&g.net, reg.models.iter().position(|m| m.id == "player")) {
            for (id, p) in net.players.iter().filter(|p| p.1.seen) {
                let at = rel(p.pos);
                if at[0] * at[0] + at[2] * at[2] > 96.0 * 96.0 {
                    continue;
                }
                let l = (light_at(p.pos) * 255.0) as u8;
                let pose = crate::model::Pose { phase: p.phase, stride: p.stride, head_pitch: -p.pitch * 180.0 / PI, head_yaw: 0.0, ..Default::default() };
                r.tex_override = self.remote_skins.get(id).copied().unwrap_or(0);
                r.draw_model(model, &pose, at, p.yaw, crate::player::HEIGHT as f32, [l, l, l]);
                if let Some((sx, sy)) = r.project(cam, [at[0], at[1] + 2.15, at[2]]) {
                    tags.push((sx, sy, p.name.to_ascii_uppercase()));
                }
            }
        }
        if self.third_person {
            if let Some(model) = reg.models.iter().position(|m| m.id == "player") {
                let v = g.player.vel;
                let speed = crate::math::sqrtf(v[0] * v[0] + v[2] * v[2]);
                let pose = crate::model::Pose {
                    phase: g.clock * 9.0,
                    stride: if g.player.on_ground { (speed / 4.4).min(1.0) } else { 0.2 },
                    head_pitch: -g.player.pitch * 180.0 / PI,
                    head_yaw: 0.0,
                    ..Default::default()
                };
                let l = (light_at(g.player.pos) * 255.0) as u8;
                r.tex_override = self.skin_tex;
                r.draw_model(model, &pose, rel(g.player.pos), g.player.yaw, crate::player::HEIGHT as f32, [l, l, l]);
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
            for i in 0..MAX_HEALTH / 2 {
                heart(r, x0 + i as f32 * 9.0 * s, hy, s, (g.health - i * 2).clamp(0, 2));
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
        for (x, y, name) in &self.tags {
            let tw = Renderer::text_width(name, s);
            r.rect(x - tw * 0.5 - 2.0 * s, y - 2.0 * s, tw + 4.0 * s, 11.0 * s, [0, 0, 0, 110]);
            r.label(x - tw * 0.5, *y, s, name, WHITE);
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
        if self.settings.minimap && self.minimap_tex != 0 && matches!(self.screen, Screen::Game | Screen::Chat) {
            let size = if self.minimap_big { (h * 0.6).min(w * 0.5) } else { 64.0 * s };
            let (mx, my) = (w - size - 4.0 * s, 4.0 * s);
            r.rect(mx - s, my - s, size + 2.0 * s, size + 2.0 * s, [16, 18, 24, 200]);
            r.image(self.minimap_tex, mx, my, size, size);
            let per = size / (MINIMAP as f32 * self.minimap_scale as f32);
            let cx = mx + size * 0.5 + (g.player.pos[0] as f32 - self.minimap_shown.0 as f32) * per;
            let cy = my + size * 0.5 + (g.player.pos[2] as f32 - self.minimap_shown.1 as f32) * per;
            if cx > mx && cx < mx + size && cy > my && cy < my + size {
                let (fx, fz) = (-crate::math::sinf(g.player.yaw), -crate::math::cosf(g.player.yaw));
                r.rect(cx + fx * 3.0 * s - s * 0.5, cy + fz * 3.0 * s - s * 0.5, s, s, [255, 255, 255, 255]);
                r.rect(cx - s, cy - s, 2.0 * s, 2.0 * s, [255, 70, 60, 255]);
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
        let (bw, bh, step) = (200.0 * s, 16.0 * s, 19.0 * s);
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
        ui.centered(cx, y - 24.0 * s, 1.0, "A VOXEL SANDBOX FOR OLD AND NEW MACHINES", ui::DIM);
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
    }

    fn screen_worlds(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let shown = self.worlds.len().min(8);
        let (x, y, bw, bh, step) = Self::column(ui, shown.max(1) + 6);
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, "WORLDS", WHITE);
        let mut open = None;
        let mut export = None;
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
            if ui.button(x + bw - side, y + i as f32 * step, side, bh, "EXPORT") {
                export = Some(name.clone());
            }
        }
        if let Some(name) = export {
            // One file holding the whole world, to copy to another machine.
            let path = alloc::format!("{}{}.vxworld", self.base, name);
            self.status = match crate::save::export_world(&mut crate::save::DiskIo, &self.base, &name) {
                Some(data) if sys::write_file(&path, &data) => alloc::format!("WROTE {}", path.to_ascii_uppercase()),
                _ => String::from("COULD NOT EXPORT THAT WORLD"),
            };
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
            let mut name = crate::settings::clean_name(stem, 20);
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
        ui.text_field(1, x, y, bw, bh, &mut self.form_name, 24, ui::name_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "SEED (EMPTY = RANDOM)", ui::DIM);
        ui.text_field(2, x, y + step * 1.5, bw, bh, &mut self.form_seed, 24, ui::any_char);
        let mode = alloc::format!("GAME MODE  {}", if self.form_mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
        if ui.button_lr(x, y + step * 2.7, bw, bh, &mode, true) != 0 {
            self.form_mode = if self.form_mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival };
        }
        let kind = alloc::format!("TERRAIN  {}", if self.form_earth { "EARTH" } else if self.form_flat { "FLAT" } else { "NORMAL" });
        let turn = ui.button_lr(x, y + step * 3.7, bw, bh, &kind, true);
        if turn != 0 {
            // Normal, flat, and (when the Earth mod is on) the real Earth.
            let kinds = if reg().has("earth") { 3 } else { 2 };
            let now = if self.form_earth { 2 } else { self.form_flat as i32 };
            let next = (now + turn).rem_euclid(kinds);
            self.form_flat = next == 1;
            self.form_earth = next == 2;
        }
        if self.form_earth {
            let half = (bw - 4.0 * s) * 0.5;
            let place = alloc::format!("START AT  < {} >", crate::earth::PLACES[self.form_place].0);
            let mut i = self.form_place as i32;
            ui.stepper_text(x, y + step * 4.7, bw, bh, &place, &mut i, 0, crate::earth::PLACES.len() as i32 - 1, 1);
            self.form_place = i as usize;
            // Scale is "1:N": one block stands for N metres. 1:1 is life size.
            let sc = alloc::format!("SCALE  < 1:{} >", self.form_scale);
            step_list(ui, x, y + step * 5.7, half, bh, &sc, &mut self.form_scale, &[1, 2, 5, 10, 30, 100, 300, 1000]);
            let vs = alloc::format!("HEIGHT  < 1:{} >", self.form_vscale);
            step_list(ui, x + half + 4.0 * s, y + step * 5.7, half, bh, &vs, &mut self.form_vscale, &[1, 2, 5, 10, 25, 50, 75, 150, 300]);
            ui.toggle(x, y + step * 6.7, bw, bh, "ROADS AND BUILDINGS", &mut self.form_features);
            ui.centered(ui.w() * 0.5, y + step * 7.8, 1.0, "NEEDS THE INTERNET. PRESS M IN THE WORLD FOR THE MAP.", ui::DIM);
        }
        let y = if self.form_earth { y + step * 3.9 } else { y };
        if ui.button(x, y + step * 5.0, bw, bh, "CREATE") {
            let name = crate::settings::clean_name(&self.form_name, 24);
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
                let kind = if self.form_earth {
                    let p = crate::earth::PLACES[self.form_place];
                    WorldKind::Earth { lat: p.1, lon: p.2, scale: self.form_scale as u32, vscale: self.form_vscale as u32, features: self.form_features }
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
        let bw = (300.0 * s).min(ui.w() - 8.0 * s);
        let x = (ui.w() - bw) * 0.5;
        let top = (y - 10.0 * s).max(24.0 * s);
        ui.centered(ui.w() * 0.5, top - 20.0 * s, 2.0, "SETTINGS", WHITE);
        let tw = bw / TABS.len() as f32;
        for (i, t) in TABS.iter().enumerate() {
            let tx = x + i as f32 * tw;
            if ui.button(tx, top, tw - 2.0 * s, bh, t) {
                self.tab = i;
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
        let (mut skin_changed, mut skin_import) = (false, false);
        {
            let st = &mut self.settings;
            match self.tab {
                0 => {
                    changed |= ui.stepper(x, row(0), bw, bh, "VIEW DISTANCE", &mut st.distance, 2, 48, 1);
                    changed |= ui.stepper(x, row(1), bw, bh, "DISTANT TERRAIN", &mut st.far_distance, 0, 256, 16);
                    changed |= ui.stepper(x, row(2), bw, bh, "FIELD OF VIEW", &mut st.fov, 50, 110, 5);
                    let gs = if st.gui_scale == 0 { String::from("INTERFACE SIZE  < AUTO >") } else { alloc::format!("INTERFACE SIZE  < {} >", st.gui_scale) };
                    changed |= ui.stepper_text(x, row(3), bw, bh, &gs, &mut st.gui_scale, 0, 6, 1);
                    let tf = if st.target_fps == 0 { String::from("HOLD FRAME RATE  < OFF >") } else { alloc::format!("HOLD FRAME RATE  < {} >", st.target_fps) };
                    changed |= step_list(ui, x, row(4), bw, bh, &tf, &mut st.target_fps, &[0, 30, 60, 85, 120, 144, 250]);
                    let mf = if st.max_fps == 0 { String::from("FRAME RATE LIMIT  < NONE >") } else { alloc::format!("FRAME RATE LIMIT  < {} >", st.max_fps) };
                    changed |= step_list(ui, x, row(5), bw, bh, &mf, &mut st.max_fps, &[0, 30, 60, 85, 120, 144, 250, 500]);
                    changed |= ui.toggle(x, row(6), bw, bh, "VSYNC", &mut st.vsync);
                    let half = (bw - 4.0 * s) * 0.5;
                    changed |= ui.toggle(x, row(7), half, bh, "CLOUDS", &mut st.clouds);
                    let rs = alloc::format!("3D DETAIL  < {}% >", st.render_scale);
                    changed |= step_list(ui, x + half + 4.0 * s, row(7), half, bh, &rs, &mut st.render_scale, &[33, 50, 67, 75, 100]);
                    changed |= ui.toggle(x, row(8), bw, bh, "SHOW FRAME RATE", &mut st.show_fps);
                    let half = (bw - 4.0 * s) * 0.5;
                    let mut fs = st.fullscreen;
                    fullscreen_clicked = ui.toggle(x, row(9), half, bh, "FULL SCREEN", &mut fs);
                    let (mw, mh) = crate::settings::FULLSCREEN_MODES[st.fullscreen_mode as usize];
                    let label = if mw == 0 { String::from("AT  < DESKTOP SIZE >") } else { alloc::format!("AT  < {}X{} >", mw, mh) };
                    mode_changed = ui.stepper_text(x + half + 4.0 * s, row(9), half, bh, &label, &mut st.fullscreen_mode, 0, crate::settings::FULLSCREEN_MODES.len() as i32 - 1, 1);
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
                2 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    changed |= ui.stepper(x, row(0), half, bh, "MOUSE SPEED", &mut st.sens, 1, 20, 1);
                    changed |= ui.toggle(x + half + 4.0 * s, row(0), half, bh, "INVERT MOUSE", &mut st.invert_y);
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
                    if ui.button(x, row(8), bw, bh, "RESET KEYS") {
                        st.keys = Settings::default().keys;
                    }
                }
                4 => {
                    // A turning preview on the left, choices on the right.
                    let model = ui.r.models.iter().position(|m| m.id == "player");
                    ui.panel(x, row(0), 100.0 * s, 150.0 * s);
                    if let Some(model) = model {
                        let pose = crate::model::Pose { phase: ui.clock * 3.0, stride: 0.5, ..Default::default() };
                        ui.r.tex_override = self.skin_tex;
                        ui.r.draw_model_2d(model, &pose, x + 50.0 * s, row(0) + 138.0 * s, ui.clock * 0.6, 120.0 * s);
                    }
                    let (cx, cw) = (x + 108.0 * s, bw - 108.0 * s);
                    let list = crate::settings::SKINS;
                    let mut i = list.iter().position(|k| k.0 == st.skin).map_or(list.len() as i32, |i| i as i32);
                    let label = alloc::format!("LOOK  < {} >", st.skin.to_ascii_uppercase());
                    if ui.stepper_text(cx, row(0), cw, bh, &label, &mut i, 0, list.len() as i32, 1) {
                        st.skin = String::from(if (i as usize) < list.len() { list[i as usize].0 } else { "custom" });
                        skin_changed = true;
                    }
                    ui.text(cx, row(1) + 6.0 * s, "YOUR OWN: A 64 X 64 PNG, THE USUAL SKIN LAYOUT.", ui::DIM);
                    ui.text(cx, row(2) - 2.0 * s, "TYPE WHERE THE FILE IS, THEN USE IT:", ui::DIM);
                    ui.text_field(40, cx, row(3) - 6.0 * s, cw, bh, &mut self.form_skin_path, 200, ui::any_char);
                    if ui.button(cx, row(4) - 3.0 * s, cw, bh, "USE THIS FILE") {
                        skin_import = true;
                    }
                }
                5 => {
                    // Every mod, built in or from a mods folder, with a
                    // switch. Changes take effect at once.
                    let names = all_mods(&self.pack_paths);
                    let hidden = names.len().saturating_sub(8) as i32;
                    self.mods_scroll = (self.mods_scroll - ui.inp.wheel).clamp(0, hidden);
                    ui.text(x, row(0) - 9.0 * s, if names.is_empty() { "NO MODS FOUND" } else { "MODS (PUT YOUR OWN IN THE MODS FOLDER)" }, ui::DIM);
                    for (k, name) in names.iter().skip(self.mods_scroll as usize).take(8).enumerate() {
                        let mut on = !st.mods_off.split(',').any(|o| o == name);
                        if ui.toggle(x, row(k), bw, bh, &name.to_ascii_uppercase().replace('_', " "), &mut on) {
                            let mut off: Vec<&str> = st.mods_off.split(',').filter(|o| !o.is_empty() && o != name).collect();
                            if !on {
                                off.push(name);
                            }
                            st.mods_off = off.join(",");
                            changed = true;
                            self.want_reload = true;
                        }
                    }
                    if hidden > 0 {
                        ui.text(x, row(8) + 4.0 * s, "SCROLL FOR MORE", ui::DIM);
                    }
                    changed |= ui.toggle(x, row(9), bw, bh, "UPDATE THE GAME AUTOMATICALLY", &mut st.auto_update);
                }
                _ => {
                    ui.text(x, row(0) + 4.0 * s, "PLAYER NAME", ui::DIM);
                    ui.text_field(10, x + 80.0 * s, row(0), bw - 80.0 * s, bh, &mut st.player_name, 16, ui::name_char);
                    changed |= ui.stepper(x, row(1), bw, bh, "AUTOSAVE EVERY (SECONDS)", &mut st.autosave_seconds, 10, 600, 10);
                    let half = (bw - 4.0 * s) * 0.5;
                    changed |= ui.toggle(x, row(2), half, bh, "VIEW BOBBING", &mut st.view_bobbing);
                    changed |= ui.toggle(x + half + 4.0 * s, row(2), half, bh, "WEATHER", &mut st.weather);
                    if reg().has("disasters") {
                        changed |= ui.toggle(x, row(6), half, bh, "NATURAL DISASTERS", &mut st.disasters);
                    }
                    let pn = alloc::format!("PLACE NOTICES  < {} >", ["OFF", "COUNTRIES", "REGIONS", "TOWNS"][st.place_notices as usize]);
                    changed |= ui.stepper_text(x + half + 4.0 * s, row(6), half, bh, &pn, &mut st.place_notices, 0, 3, 1);
                    if let Some(g) = self.session.as_mut() {
                        let label = alloc::format!("GAME MODE  {}", if g.mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
                        if ui.button_lr(x, row(3), bw, bh, &label, true) != 0 {
                            g.set_mode(if g.mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival });
                        }
                    }
                    changed |= ui.toggle(x, row(8), bw, bh, "MINIMAP (M ENLARGES IT)", &mut st.minimap);
                    if ui.button(x, row(7), bw, bh, "RELOAD CONTENT PACKS NOW") {
                        self.want_reload = true;
                    }
                    ui.text(x, row(4) + 6.0 * s, "MAP DATA RELAY (HOST:PORT), FOR OLD MACHINES", ui::DIM);
                    ui.text_field(11, x, row(5), bw, bh, &mut st.relay, 80, ui::any_char);
                }
            }
        }
        if skin_import {
            // Copy the chosen file into the data folder as the custom skin.
            let ok = sys::read_file(self.form_skin_path.trim())
                .filter(|b| crate::png::decode(b).map_or(false, |i| i.w == 64 && i.h == 64))
                .map_or(false, |b| sys::write_file(&alloc::format!("{}skin.png", self.base), &b));
            if ok {
                self.settings.skin = String::from("custom");
                self.status = String::from("SKIN LOADED");
                skin_changed = true;
            } else {
                self.status = String::from("THAT IS NOT A 64 X 64 PNG FILE");
            }
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

    fn screen_join(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 9);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, "MULTIPLAYER", WHITE);
        ui.text(x, y - 9.0 * s, "SERVER ADDRESS (HOST OR HOST:PORT)", ui::DIM);
        let enter = ui.text_field(30, x, y, bw, bh, &mut self.form_address, 60, ui::any_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "YOUR NAME", ui::DIM);
        ui.text_field(31, x, y + step * 1.5, bw, bh, &mut self.settings.player_name, 16, ui::name_char);
        if ui.button(x, y + step * 2.8, bw, bh, "JOIN") || enter {
            let name = crate::settings::clean_name(&self.settings.player_name, 16);
            match Session::join(&self.base, &self.form_address, if name.is_empty() { "Player" } else { &name }, self.threads) {
                Ok(g) => {
                    self.close_world(ui.r);
                    self.session = Some(g);
                    self.distance = self.settings.distance;
                    self.set_screen(Screen::Game);
                    return;
                }
                Err(e) => self.status = e,
            }
        }
        // Hosting: a server runs inside the game for as long as you play.
        ui.text(x, y + step * 4.4 - 9.0 * s, "OR HOST YOUR OWN: SHARED WORLD NAME", ui::DIM);
        ui.text_field(32, x, y + step * 4.4, bw, bh, &mut self.form_host, 24, ui::name_char);
        if ui.button(x, y + step * 5.5, bw, bh, "HOST A GAME") {
            self.status.clear();
            self.host_game(ui.r);
            if self.session.is_some() {
                return;
            }
        }
        if ui.button(x, y + step * 6.7, bw, bh, "BACK") {
            self.set_screen(Screen::Title);
        }
        ui.centered(ui.w() * 0.5, y + step * 7.9, 1.0, &self.status, ui::ACCENT);
        let mine = self.my_address.get_or_insert_with(sys::net::local_address).as_ref().map(|a| alloc::format!("YOUR ADDRESS ON THIS NETWORK: {}", a)).unwrap_or_default();
        ui.centered(ui.w() * 0.5, y + step * 8.8, 1.0, &mine, ui::DIM);
        ui.centered(ui.w() * 0.5, y + step * 9.6, 1.0, "NO SCREEN? HOST WITH  VOXELRUST --SERVER", ui::DIM);
    }

    /// Paint a little more of the minimap; when a picture is finished it
    /// goes on show and the next one starts where the player is now.
    fn paint_minimap(&mut self, r: &mut Renderer) {
        let g = match self.session.as_ref() {
            Some(g) if self.settings.minimap => g,
            _ => return,
        };
        let scale = if self.minimap_big { 8 } else { 2 };
        if self.minimap_cursor == 0 {
            self.minimap_origin = (floord(g.player.pos[0]), floord(g.player.pos[2]));
        }
        let seed = g.world.seed;
        let half = (MINIMAP / 2) as i32;
        if crate::render::EXPERIMENT.load(core::sync::atomic::Ordering::Relaxed) & 16 != 0 {
            return;
        }
        // A little each frame keeps this off the frame time: the whole
        // picture comes round every couple of seconds.
        for _ in 0..40 {
            let i = self.minimap_cursor;
            if i >= MINIMAP * MINIMAP {
                break;
            }
            let (px, pz) = ((i % MINIMAP) as i32, (i / MINIMAP) as i32);
            let (x, z) = (self.minimap_origin.0 + (px - half) * scale, self.minimap_origin.1 + (pz - half) * scale);
            let seen = g.explored.contains(&(x.div_euclid(crate::session::EXPLORE), z.div_euclid(crate::session::EXPLORE)));
            // An Earth world has a map to begin with: ground not yet
            // visited shows too, only dimmer.
            let charted = crate::worldgen::earth_proj().is_some() && crate::worldgen::shape() == 0;
            let c = if seen || charted {
                let col = crate::worldgen::column(seed, x, z);
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
                // Higher ground is lighter, and slopes facing north-west
                // catch the light, which makes hills readable.
                let north = crate::worldgen::column(seed, x - scale, z - scale).height;
                let lift = ((col.height - crate::worldgen::SEA).clamp(0, 60) / 3 + (col.height - north).clamp(-3, 3) * 9) as i32;
                let lit = [0, 1, 2].map(|k| (base[k] as i32 + if col.height >= crate::worldgen::SEA { lift } else { 0 }).clamp(0, 255) as u8);
                if seen {
                    lit
                } else {
                    lit.map(|v| (v as u16 * 45 / 100 + 14) as u8)
                }
            } else {
                [24, 26, 32]
            };
            self.minimap_px[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            self.minimap_cursor += 1;
        }
        if self.minimap_cursor >= MINIMAP * MINIMAP {
            if self.minimap_tex == 0 {
                self.minimap_tex = r.add_texture(MINIMAP, MINIMAP, &self.minimap_px);
            } else {
                r.update_texture(self.minimap_tex, MINIMAP, MINIMAP, &self.minimap_px);
            }
            self.minimap_shown = self.minimap_origin;
            self.minimap_scale = scale;
            self.minimap_cursor = 0;
        }
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
                    g.push_chat(String::from("/VAR NAME = N   /IF COND THEN CMD [ELSE CMD]   /AFTER SECS CMD   /REPEAT N CMD   /TAKE ITEM [N]   /TITLE TEXT   /BLAST R"));
                }
            } else if let Some(g) = self.session.as_mut() {
                // Commands are typed in lower case ids; anything else is talk.
                match if command.starts_with('/') { g.command(&command) } else { None } {
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

    fn open_map(&mut self) {
        let unit = match (self.session.as_ref(), crate::worldgen::earth_proj()) {
            (Some(g), Some(p)) => {
                let size = (256u64 << p.zoom) as f64;
                let (px, py) = p.block_to_px(g.player.pos[0], g.player.pos[2]);
                Some((px / size, py / size, p.zoom as i32))
            }
            _ => None,
        };
        match unit {
            Some((x, y, zoom)) => {
                self.map_center = (x, y);
                self.map_zoom = (zoom - 2).clamp(2, 16);
                crate::earth::retry_failed();
                self.set_screen(Screen::Map);
            }
            None => {
                // No atlas for an invented world: the minimap grows instead.
                self.minimap_big = !self.minimap_big;
                self.minimap_cursor = 0;
            }
        }
    }

    /// The world map: drag to move, wheel to zoom, click to travel there.
    fn screen_map(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (w, h) = (ui.w(), ui.h());
        ui.r.rect(0.0, 0.0, w, h, [14, 18, 26, 255]);
        let p = match crate::worldgen::earth_proj() {
            Some(p) => *p,
            None => return,
        };
        // Zoom toward the middle of the screen.
        if ui.inp.wheel != 0 {
            self.map_zoom = (self.map_zoom + ui.inp.wheel).clamp(2, 16);
        }
        let zoom = self.map_zoom as u32;
        let size = (256u64 << zoom) as f64;
        // Tiles are drawn at whole-number scale so they stay crisp.
        let scale = if h > 900.0 { 2.0 } else { 1.0 };
        let top = 27.0 * s;
        if ui.inp.down[1] && ui.inp.mouse.1 > top {
            self.map_center.0 -= ui.inp.moved.0 as f64 / scale as f64 / size;
            self.map_center.1 -= ui.inp.moved.1 as f64 / scale as f64 / size;
        }
        self.map_center.1 = self.map_center.1.clamp(0.0, 1.0);
        self.map_center.0 -= crate::math::floord(self.map_center.0) as f64;
        let (cx, cy) = (self.map_center.0 * size, self.map_center.1 * size);
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
                match self.map_tiles.get(&key) {
                    Some(MapTile::Ready(tex)) => ui.r.image(*tex, sx, sy, 256.0 * scale, 256.0 * scale),
                    Some(MapTile::Loading) => loading += 1,
                    Some(MapTile::Missing) => ui.r.rect(sx + 1.0, sy + 1.0, 256.0 * scale - 2.0, 256.0 * scale - 2.0, [40, 30, 34, 255]),
                    None => {
                        // Ask a worker to fetch it; at most a few at a time.
                        if loading < 4 {
                            if let Some(g) = self.session.as_mut() {
                                g.pool.submit(crate::jobs::Job::MapTile { z: key.0, x: key.1, y: key.2 }, true);
                                self.map_tiles.insert(key, MapTile::Loading);
                                loading += 1;
                            }
                        }
                    }
                }
            }
        }
        // Finished tiles become textures.
        if let Some(g) = self.session.as_mut() {
            for (key, img) in g.world.map_results.drain(..) {
                let entry = match img {
                    Some(rgba) => MapTile::Ready(ui.r.add_texture(256, 256, &rgba)),
                    None => MapTile::Missing,
                };
                self.map_tiles.insert(key, entry);
            }
        }
        // The player, as a marker.
        let mut travel = None;
        if let Some(g) = self.session.as_ref() {
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
            if ui.inp.clicked[3] && my > top {
                travel = Some((bx, bz));
            }
        }
        ui.r.rect(0.0, h - 11.0 * s, w, 11.0 * s, [10, 12, 18, 200]);
        ui.text(4.0 * s, h - 9.0 * s, "MAP DATA (C) OPENSTREETMAP CONTRIBUTORS - ELEVATION: MAPZEN / AWS TERRAIN TILES", ui::DIM);
        if let Some((bx, bz)) = travel {
            if let Some(g) = self.session.as_mut() {
                // Arrive in the air above the spot; survival players are set
                // down gently once the ground has loaded.
                g.travel_to(bx, bz);
            }
            self.set_screen(Screen::Game);
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
        let ph = 32.0 * s + upper_rows as f32 * cell + 16.0 * s + 4.0 * cell + 14.0 * s;
        let (px, py) = ((ui.w() - pw) * 0.5, ((ui.h() - ph) * 0.5).max(12.0 * s));
        ui.panel(px, py, pw, ph);
        let gx = px + 6.0 * s;
        let mut tip: Option<String> = None;
        let (click_l, click_r) = (ui.inp.clicked[1], ui.inp.clicked[3]);

        // Upper part: every item (creative) or what can be made (survival).
        let uy = py + 32.0 * s;
        let at_table = self.screen == Screen::Craft;
        if self.screen == Screen::Chest {
            // The chest's own 27 slots.
            ui.text(gx, py + 6.0 * s, if self.chest_vehicle.is_some() { "CARGO" } else { "CHEST" }, ui::DIM);
            let chest = match self.chest_vehicle.filter(|&i| i < g.vehicles.list.len()) {
                Some(i) => &mut g.vehicles.list[i].storage,
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
            let list: Vec<usize> = (0..r.items.len()).filter(|&i| r.items[i].category == tab).collect();
            let total_rows = (list.len() + cols - 1) / cols;
            let max_scroll = total_rows.saturating_sub(upper_rows) as i32;
            self.inv_scroll = (self.inv_scroll - ui.inp.wheel).clamp(0, max_scroll);
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
                                    // Clicking with something in hand throws it away.
                                    g.cursor = match g.cursor {
                                        Some(_) => None,
                                        None => Some(Stack { item: item as u16, count: if click_l { r.items[item].stack } else { 1 } }),
                                    };
                                    ui.clicked_any = true;
                                }
                            }
                        }
                        None => draw_slot(ui.r, x, y, cell, s, None, false),
                    }
                }
            }
            // The wheel scrolls a row at a time; the bar at the side shows
            // where in the list this is, and can be dragged.
            ui.scrollbar(px + pw - 4.5 * s, uy, 3.0 * s, upper_rows as f32 * cell, &mut self.inv_scroll, upper_rows, total_rows);
        } else {
            let at_oven = at_table && self.at_oven;
            let at_furnace = at_table && self.at_furnace;
            ui.text(gx, py + 6.0 * s, if at_furnace { "FURNACE (CLICK SOMETHING TO SMELT IT)" } else if at_oven { "OVEN (CLICK SOMETHING TO BAKE IT)" } else if at_table { "CRAFTING TABLE (CLICK A RECIPE)" } else { "MAKE BY HAND - A CRAFTING TABLE MAKES MORE" }, ui::DIM);
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
                ui.clicked_any = true;
            }
        }

        // A portrait of the player beside the panel, where there is room.
        if px > 70.0 * s {
            if let Some(model) = r.models.iter().position(|m| m.id == "player") {
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

    /// Stand one of every creature and vehicle in rows in front of the
    /// player (for screenshots, and for checking a pack's models).
    fn line_up_creatures(&mut self) {
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let p = g.player.pos;
        let fwd = crate::math::forward(g.player.yaw, 0.0);
        let right = [-fwd.z, fwd.x];
        // One of each model is enough: skip colour variants.
        let mut seen = Vec::new();
        let kinds: Vec<usize> = (0..reg().entities.len()).filter(|&i| {
            let m = reg().entities[i].model;
            if seen.contains(&m) {
                false
            } else {
                seen.push(m);
                true
            }
        }).collect();
        let per_row = 8;
        let total = kinds.len() + reg().vehicles.len();
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
                let i = g.mobs.spawn(kinds[n], at);
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
            } else {
                g.vehicles.place(n - kinds.len(), at, g.player.yaw + PI - 0.6);
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

fn draw_item(r: &mut Renderer, x: f32, y: f32, size: f32, index: usize, tint: Color) {
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
        _ => r.icon(x, y, size, it.icon, tint),
    }
}

fn draw_slot(r: &mut Renderer, x: f32, y: f32, size: f32, s: f32, stack: Option<Stack>, hot: bool) {
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
        draw_item(r, x + 3.0 * s, y + 3.0 * s, size - 6.0 * s, st.item as usize, WHITE);
        if st.count > 1 {
            let t = alloc::format!("{}", st.count);
            let tw = Renderer::text_width(&t, s);
            r.label(x + size - tw - 1.5 * s, y + size - 9.0 * s, s, &t, WHITE);
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

fn run(args: Vec<String>) -> Result<(), String> {
    let opts = parse_args(&args)?;
    if opts.help {
        sys::log(USAGE);
        return Ok(());
    }
    if opts.version {
        sys::log(&alloc::format!("voxelrust {} ({}, commit {})", env!("CARGO_PKG_VERSION"), BUILD, crate::update::COMMIT));
        return Ok(());
    }
    sdl::load()?;
    if let Some(path) = &opts.log {
        sys::log_to_file(path);
    }
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
                if !new.is_empty() && sys::read_file(&alloc::format!("{}settings.txt", new)).is_none() && sys::read_file(&alloc::format!("{}worlds.txt", new)).is_none() {
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

    // Moving a world in or out as one file needs no window.
    if opts.export.is_some() || opts.import.is_some() {
        let name = opts.world.clone().unwrap_or_default();
        if name.is_empty() || name != crate::settings::clean_name(&name, 24) {
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
    // Then every mod in the data folder's `mods`, and in a `mods` folder
    // beside where the game was started (handy while making one).
    // Mods are looked for beside the program (where the ones that come
    // with the game are; inside the app on a Mac), in the data folder, and
    // in a `mods` folder where the game was started (handy while making one).
    let exe = sys::exe_dir();
    let mut pack_paths = alloc::vec![auto_pack.clone()];
    for folder in [alloc::format!("{}mods", exe), alloc::format!("{}../Resources/mods", exe), alloc::format!("{}mods", base), String::from("mods")] {
        let pattern = alloc::format!("{}/*", folder);
        if !pack_paths.contains(&pattern) && (folder == "mods" || !exe.is_empty() || folder.starts_with(base.as_str())) {
            pack_paths.push(pattern);
        }
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
    let (first, pack_text) = load_content(&pack_paths, &mods_off);
    block::install(first);
    let registry = reg();
    for w in &registry.warnings {
        logf!("voxelrust: data: {}", w);
    }
    if registry.items.is_empty() {
        return Err(String::from("the data files define no items"));
    }

    if opts.earth.is_some() && !registry.has("earth") {
        return Err(String::from("--earth needs the Earth mod, which is switched off (Settings > MODS)"));
    }
    if opts.server {
        return run_server(&opts, &base, &pack_text);
    }
    let scripted_early = opts.screenshot.is_some() || opts.bench.is_some();
    let settings_path = alloc::format!("{}settings.txt", base);
    let mut settings = Settings::parse(&sys::read_file(&settings_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default());
    if let Some(d) = opts.distance {
        settings.distance = d;
    }
    // Look for a newer version in the background (never in scripted runs,
    // nor in a browser, where the page is the program).
    #[cfg(not(target_arch = "wasm32"))]
    if !scripted_early && !opts.server && !opts.no_update {
        let (scratch, install) = (base.clone(), settings.auto_update);
        sys::thread::spawn(alloc::boxed::Box::new(move || crate::update::run(BUILD, &scratch, install)));
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
    logf!("voxelrust: OpenGL {} on {}", gl::get_string(gl::VERSION), gl::get_string(gl::RENDERER));
    crate::icon::set_window_icon(window);
    let mut r = Renderer::new();
    r.load_models();
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
        third_person: opts.show.contains("third"),
        swallow_text: false,
        hud_hidden: false,
        ui_clock: 0.0,
        worlds: Vec::new(),
        form_name: String::new(),
        form_seed: String::new(),
        form_mode: GameMode::Survival,
        form_earth: false,
        form_flat: false,
        form_place: 0,
        form_address: String::new(),
        form_host: String::from("shared"),
        host: None,
        my_address: None,
        form_chat: String::new(),
        skin_tex: 0,
        remote_skins: BTreeMap::new(),
        skin_png: Vec::new(),
        skin_sent: false,
        pack_paths,
        pack_text,
        server_content: false,
        want_reload: false,
        at_oven: false,
        show_walking: opts.show.contains("walking"),
        mods_scroll: 0,
        at_furnace: false,
        tune: (0.0, 0),
        worlds_scroll: 0,
        minimap_tex: 0,
        minimap_px: alloc::vec![0u8; MINIMAP * MINIMAP * 4],
        minimap_cursor: 0,
        minimap_origin: (0, 0),
        minimap_shown: (0, 0),
        minimap_scale: 2,
        minimap_big: false,
        form_import: String::new(),
        form_skin_path: String::new(),
        tags: Vec::new(),
        form_scale: 30,
        form_features: true,
        form_vscale: 75,
        map_center: (0.5, 0.5),
        map_zoom: 10,
        map_tiles: BTreeMap::new(),
        status: String::new(),
        tab: 0,
        rebinding: None,
        inv_tab: 0,
        inv_scroll: 0,
        chest_at: [0; 3],
        chest_vehicle: None,
        scripted,
        show_hud: opts.show.contains("hud"),
    };
    app.apply_settings();
    app.load_skin(&mut r);
    app.status.clear();
    logf!("voxelrust: data in {}", if base.is_empty() { "." } else { &base });
    crate::earth::configure(&base, &app.settings.relay);

    // Going straight into a world: asked for by name, or a scripted run.
    let menu_only = ["title", "worlds", "newworld", "multiplayer"].iter().any(|s| opts.show.contains(s)) || (opts.show.contains("settings") && opts.world.is_none());
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
        if name.is_empty() || name != crate::settings::clean_name(&name, 24) {
            return Err(String::from("--world: use only letters, digits, - and _ (24 at most)"));
        }
        let seed = opts.seed.unwrap_or_else(|| unsafe { (api.SDL_GetPerformanceCounter)() } as u32 ^ 0x5EED_1234);
        let kind = match opts.earth {
            Some((lat, lon)) => {
                let (scale, vscale) = opts.scale.unwrap_or((30, 75));
                WorldKind::Earth { lat, lon, scale, vscale, features: true }
            }
            None => WorldKind::Normal,
        };
        app.start_world(&mut r, &name, &NewWorld { seed, mode: opts.mode.unwrap_or(GameMode::Creative), kind });
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
        // "items:<tab>" opens the creative list at that tab.
        if let Some(tab) = opts.show.split(',').find_map(|w| w.strip_prefix("items:")) {
            app.inv_tab = reg().categories.iter().position(|c| c.eq_ignore_ascii_case(tab)).unwrap_or(0);
        }
        for (word, screen) in [
            ("pause", Screen::Pause),
            ("items", Screen::Inventory),
            ("dead", Screen::Dead),
            ("settings", Screen::Settings),
            ("worlds", Screen::Worlds),
            ("newworld", Screen::NewWorld),
            ("multiplayer", Screen::Join),
        ] {
            if opts.show.contains(word) {
                app.screen = screen;
            }
        }
        if let Some(t) = opts.show.split("settings:").nth(1) {
            let t = t.split(',').next().unwrap_or("").to_ascii_uppercase();
            app.tab = TABS.iter().position(|x| *x == t).unwrap_or(0);
        }
        app.worlds = session::list_worlds(&app.base);
        inp.mouse = (-100.0, -100.0);
        if opts.show.contains("map") {
            app.open_map();
        }
    }

    let mut frame_count = 0u32;
    let mut showcase_pending = opts.showcase && scripted;
    let mut creatures_pending = scripted && opts.show.contains("creatures");
    let mut run_pending = opts.run.clone().filter(|_| scripted);
    let mut unloaded_frames = 0u32;
    let mut last = sys::now();
    let mut settled_frames = 0u32;
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
        app.update(&mut r, if scripted && !online { 0.0 } else { dt }, dt);
        app.draw(&mut r, &inp);
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
            } else if settled_frames == 3 && bench.is_none() {
                if let Some(g) = &app.session {
                    let ready = g.world.chunks.values().filter(|c| c.mesh.is_some()).count();
                    logf!("voxelrust: {} chunks ready in {:.2} s", ready, sys::now() - load_start);
                }
                if let Some(g) = app.session.as_ref().filter(|g| g.net.is_some()) {
                    logf!("voxelrust: shared: {} creatures, {} vehicles, {} chests, {}", g.mobs.list.len(), g.vehicles.list.len(), g.chests.len(), if g.runs_the_world() { "run from here" } else { "run by another player" });
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
                        let path = alloc::format!("{}flypass-{}.bmp", app.base, done / (total / 4).max(1));
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
        sys::write_file(&app.settings_path, app.settings.to_text().as_bytes());
    }
    // Close the sound device before SDL goes.
    drop(core::mem::replace(&mut app.audio, Audio::silent()));
    unsafe {
        (api.SDL_GL_DeleteContext)(ctx);
        (api.SDL_DestroyWindow)(window);
        (api.SDL_Quit)();
    }
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
        if name.ends_with(".txt") && name != "mods.txt" {
            found.push(path);
        } else {
            let pack = alloc::format!("{}/pack.txt", path);
            if sys::read_file(&pack).is_some() {
                found.push(pack);
            }
        }
    }
    found
}

/// The name a mod goes by: its folder, or its file without `.txt`.
fn mod_name(path: &str) -> String {
    let p = path.strip_suffix("/pack.txt").unwrap_or(path);
    let p = p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p);
    String::from(p.strip_suffix(".txt").unwrap_or(p))
}

/// The mods that come inside the program (`data/packs/<name>/pack.txt`).
fn bundled_mods() -> Vec<(String, &'static [u8])> {
    block::embedded::FILES.iter().filter(|f| f.0.starts_with("packs/") && f.0.ends_with("/pack.txt")).map(|f| (String::from(f.0), f.1)).collect()
}

/// Every mod there is to switch on or off: those built in, then those in
/// the mods folders.
fn all_mods(pack_paths: &[String]) -> Vec<String> {
    let mut names: Vec<String> = bundled_mods().iter().map(|m| mod_name(&m.0)).collect();
    for p in pack_paths {
        if let Some(folder) = p.strip_suffix("/*") {
            for m in mods_in(&alloc::format!("{}/", folder)) {
                let n = mod_name(&m);
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    names
}

fn load_content(pack_paths: &[String], off: &str) -> (Registry, Vec<u8>) {
    let mut sources: Vec<(String, String)> = alloc::vec![(String::from("base.txt"), String::from(block::BASE_DATA))];
    let is_off = |name: &str| off.split(',').any(|o| o == name);
    // The mods built into the program come first.
    for (path, bytes) in bundled_mods() {
        if !is_off(&mod_name(&path)) {
            sources.push((path, String::from_utf8_lossy(bytes).into_owned()));
        }
    }
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
        let fresh = !p.contains("/mods/") && !p.starts_with("mods/") || !named.contains(&n);
        named.push(n);
        fresh
    });
    for p in &all {
        if let Some(bytes) = sys::read_file(p) {
            logf!("voxelrust: content from {}", p);
            sources.push((p.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    // Remember every file the packs pull in (textures, models, includes).
    let mut used: Vec<(String, Vec<u8>)> = Vec::new();
    let registry = Registry::from_sources(&refs, &mut |p| {
        let data = sys::read_file(p);
        if let Some(d) = &data {
            if !used.iter().any(|u| u.0 == p) {
                used.push((String::from(p), d.clone()));
            }
        }
        data
    });
    (registry, pack_bundle(&sources[1..], &used))
}

/// The packs and the files they use as one blob a server can send. Folder
/// names are replaced by `p0/`, `p1/`...: players need the files, not to
/// know where the host keeps them.
fn pack_bundle(packs: &[(String, String)], used: &[(String, Vec<u8>)]) -> Vec<u8> {
    if packs.is_empty() {
        return Vec::new();
    }
    let dir = |p: &str| String::from(&p[..p.rfind(|c| c == '/' || c == '\\').map_or(0, |i| i + 1)]);
    let mut dirs: Vec<String> = Vec::new();
    for (p, _) in packs {
        let d = dir(p);
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    // Longest first, so a folder inside another keeps its own name.
    dirs.sort_by(|a, b| b.len().cmp(&a.len()));
    let rename = |p: &str| match dirs.iter().position(|d| p.starts_with(d.as_str())) {
        Some(i) => alloc::format!("p{}/{}", i, &p[dirs[i].len()..]),
        None => alloc::format!("x/{}", p.replace(|c| c == '/' || c == '\\' || c == ':', "_")),
    };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (p, text) in packs {
        files.push((alloc::format!("pack:{}", rename(p)), text.as_bytes().to_vec()));
    }
    for (p, data) in used {
        files.push((alloc::format!("file:{}", rename(p)), data.clone()));
    }
    crate::save::bundle(&files)
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
    Registry::from_sources(&refs, &mut |p| files.iter().find(|f| f.0.strip_prefix("file:") == Some(p)).map(|f| f.1.clone()))
}

/// Set to stop a server started from the multiplayer menu.
static HOST_STOP: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Open the server for the world `name`: its small save (seed, kind and
/// every edit) is read, or made on first use.
fn open_server(name: &str, seed: Option<u32>, earth: Option<String>, port: u16, players: usize, base: &str, pack: &[u8]) -> Result<(crate::server::Server, String), String> {
    use crate::server::{Config, Server};
    if name.is_empty() || name != crate::settings::clean_name(name, 24) {
        return Err(String::from("world names use only letters, digits, - and _ (24 at most)"));
    }
    let meta_path = alloc::format!("{}{}.server.txt", base, name);
    let edits_path = alloc::format!("{}{}.server-edits.txt", base, name);
    let meta = sys::read_file(&meta_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let seed = crate::settings::kv(&meta, "seed").and_then(|v| v.parse().ok()).unwrap_or_else(|| seed.unwrap_or(sys::now().to_bits() as u32 ^ 0x5EED_1234));
    let kind = crate::settings::kv(&meta, "kind").map(String::from).unwrap_or_else(|| earth.unwrap_or_else(|| WorldKind::Normal.to_text()));
    sys::write_file(&meta_path, alloc::format!("seed={}\nkind={}\n", seed, kind).as_bytes());
    let cfg = Config {
        port,
        seed,
        kind: kind.clone(),
        max_players: players,
        content: crate::proto::content_checksum(),
        states: reg().blocks.len() as u16,
        pack: pack.to_vec(),
    };
    let mut server = Server::bind(cfg).map_err(|e| alloc::format!("could not listen on port {}: {:?}", port, e))?;
    // Operators and bans are kept beside the world: "op NAME" / "ban NAME".
    if let Some(text) = sys::read_file(&alloc::format!("{}{}.server-ops.txt", base, name)) {
        for line in String::from_utf8_lossy(&text).lines() {
            match line.split_once(' ') {
                Some(("op", n)) => server.ops.push(String::from(n)),
                Some(("ban", n)) => server.banned.push(String::from(n)),
                _ => {}
            }
        }
    }
    if let Some(text) = sys::read_file(&edits_path) {
        server.load_text(&String::from_utf8_lossy(&text));
    }
    crate::earth::configure(base, "");
    logf!("voxelrust: serving world `{}` (seed {}, {}) on port {}, {} saved edits", name, seed, kind, server.port(), server.edits.len());
    Ok((server, edits_path))
}

/// Run a server until `HOST_STOP` is set (never, for `--server`). Edits
/// are written every 20 seconds and once more on the way out.
fn server_loop(mut server: crate::server::Server, edits_path: String, quiet: bool) {
    use core::sync::atomic::Ordering;
    let mut last = sys::now();
    let mut save_timer = 0.0f32;
    while !HOST_STOP.load(Ordering::Relaxed) {
        let now = sys::now();
        let dt = ((now - last) as f32).clamp(0.0, 1.0);
        last = now;
        let lines = server.tick(dt, &mut |line| {
            // "TILE <elev|map> <z> <x> <y>": fetch it the way the game would.
            let w: Vec<&str> = line.split(' ').collect();
            if w.first() == Some(&"GEO") {
                // "GEO <lat> <lon> <level>": a place-name question.
                let f = |i: usize| w.get(i).and_then(|v| v.parse::<f64>().ok());
                return match (f(1), f(2), w.get(3).and_then(|v| v.parse::<i32>().ok())) {
                    (Some(lat), Some(lon), Some(level)) if (-85.0..=85.0).contains(&lat) && (-180.0..=180.0).contains(&lon) => {
                        crate::earth::fetch_place_json(lat, lon, level.clamp(1, 3))
                    }
                    _ => None,
                };
            }
            let layer = match w.get(1) {
                Some(&"elev") => crate::earth::Layer::Elevation,
                Some(&"map") => crate::earth::Layer::Map,
                _ => return None,
            };
            let n = |i: usize| w.get(i).and_then(|v| v.parse::<u32>().ok());
            match (n(2), n(3), n(4)) {
                (Some(z), Some(x), Some(y)) if z <= 16 && x < (1 << z) && y < (1 << z) => crate::earth::fetch_bytes((layer, z, x, y)),
                _ => None,
            }
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
            sys::write_file(&edits_path.replace(".server-edits.txt", ".server-ops.txt"), text.as_bytes());
        }
        save_timer += dt;
        if save_timer > 20.0 && server.dirty {
            save_timer = 0.0;
            server.dirty = false;
            if !sys::write_file(&edits_path, server.save_text().as_bytes()) {
                logf!("voxelrust: could not write {}", edits_path);
            }
        }
        sys::sleep_ms(4);
    }
    if server.dirty {
        sys::write_file(&edits_path, server.save_text().as_bytes());
    }
}

/// `voxelrust --server`: host a world for others, with no window.
fn run_server(opts: &Opts, base: &str, pack_text: &[u8]) -> Result<(), String> {
    let name = opts.world.clone().unwrap_or_else(|| String::from("server"));
    let earth = opts.earth.map(|(lat, lon)| {
        let (scale, vscale) = opts.scale.unwrap_or((30, 75));
        WorldKind::Earth { lat, lon, scale, vscale, features: true }.to_text()
    });
    let port = opts.port.unwrap_or(crate::proto::DEFAULT_PORT);
    let (server, edits_path) = open_server(&name, opts.seed, earth, port, opts.players.unwrap_or(16), base, pack_text)?;
    let address = sys::net::local_address().unwrap_or_else(|| String::from("<this machine's address>"));
    logf!("voxelrust: players join with:  voxelrust --join {}:{}", address, server.port());
    server_loop(server, edits_path, false);
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
