//! The program: start-up, the main loop, input, and every screen.

use crate::audio::{Audio, Sound};
use crate::block::{self, reg, Kind, Registry};
use crate::chunk::CH;
use crate::inventory::{Stack, HOTBAR, SLOTS};
use crate::math::{floord, sinf, PI, TAU};
use crate::player::Input;
use crate::render::{Camera, Color, Env, Renderer, WHITE};
use crate::session::{self, Controls, GameMode, NewWorld, Session};
use crate::settings::{Action, Settings, ACTIONS};
use crate::sys::{self, gl, sdl, thread};
use crate::ui::{self, Ui, UiInput};
use crate::worldgen::WorldKind;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

// This file was one 4600-line file; the parts of `App` and of the start-up
// code now live in files of their own, as children of this module (so they
// reach its private items with `use super::*`). The files sit in the
// folder of their subject: `screens/` (one per screen), `frame/` (input,
// update, drawing) and `launch/` (command line, content, server).
#[path = "launch/args.rs"]
mod args;
#[path = "launch/mods_load.rs"]
mod mods_load;
#[path = "launch/server_run.rs"]
mod server_run;
#[path = "launch/step.rs"]
mod scripted_step;
#[path = "frame/input.rs"]
mod frame_input;
#[path = "frame/update.rs"]
mod frame_update;
#[path = "frame/draw.rs"]
mod frame_draw;
#[path = "frame/creatures.rs"]
mod frame_creatures;
#[path = "frame/lights.rs"]
mod frame_lights;
#[path = "frame/hud.rs"]
mod frame_hud;
#[path = "frame/scripted.rs"]
mod frame_scripted;
#[path = "screens/title.rs"]
mod screen_title;
#[path = "screens/settings.rs"]
mod screen_settings;
#[path = "screens/join.rs"]
mod screen_join;
#[path = "screens/pin.rs"]
mod screen_pin;
#[path = "screens/inventory.rs"]
mod screen_inventory;
#[path = "screens/map.rs"]
mod screen_map;
#[path = "screens/overlays.rs"]
mod screen_overlays;
#[path = "screens/skin.rs"]
mod screen_skin;
#[allow(unused_imports)]
use self::{args::*, mods_load::*, server_run::*};
use self::screen_pin::PinPrompt;


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
    /// The PIN of an account, asked for by a server (screens/pin.rs).
    Pin,
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
    /// A screen a mod opened (see `client::modui`).
    ModUi,
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
impl App {
    /// Slim arms: asked for in the settings, or what the skin picture is made for.
    pub(crate) fn slim(&self) -> bool {
        self.settings.slim_arms || self.skin_slim
    }
}

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
/// the data folder). The error is a message for the player. A PNG that is
/// already a square skin is kept as it is (nothing is packed again here: a big
/// one would take seconds); any other picture is written as a PNG.
fn import_skin(base: &str, path: &str) -> Result<(), String> {
    let bytes = sys::read_file_limit(path, crate::image::MAX_BYTES).ok_or_else(|| String::from("COULD NOT READ THAT FILE (OR IT IS TOO BIG)"))?;
    let img = crate::image::decode(&bytes).ok_or_else(|| String::from("THAT IS NOT A PICTURE THE GAME CAN READ (UP TO 4096 X 4096)"))?;
    let keep_as_is = bytes.starts_with(&[0x89, b'P', b'N', b'G']) && img.w == img.h && crate::skin_grid::valid_size(img.w, img.h);
    let file = if keep_as_is {
        bytes
    } else {
        let px = crate::client::skinfile::from_image(&img)?;
        crate::png::encode(img.w, img.w, &px)
    };
    if sys::write_file(&alloc::format!("{}skin.png", base), &file) {
        Ok(())
    } else {
        Err(String::from("COULD NOT SAVE THE SKIN IN THE DATA FOLDER"))
    }
}

/// The skin to wear as (side, pixels): `skin.png` of the data folder for the
/// custom one (any size a skin may be, see `skin_grid`), else a built-in one,
/// or one a pack or mod brings.
fn skin_pixels(base: &str, name: &str, player: &str) -> Option<(usize, Vec<u8>)> {
    if name == "custom" {
        let img = crate::png::decode(&sys::read_file_limit(&alloc::format!("{}skin.png", base), crate::image::MAX_BYTES)?)?;
        // (A file put there by hand in the old 64 x 32 layout is brought up to a square.)
        let px = crate::client::skinfile::from_image(&img).ok()?;
        return Some((img.w, px));
    }
    // `default` is one of the built-in skins, picked by the player's name; the others are chosen by name.
    // A built-in skin, or any skin a pack or mod brings a picture for (`skin_<name>`).
    // Decode the named skin once: it is both the test and the picture.
    let named = if name != "default" { crate::pack_entities::skin_image(name) } else { None };
    let known = named.is_some() || crate::settings::SKINS.iter().any(|s| *s == name);
    if let (true, Some(img)) = (known, named) {
        return Some((img.w, img.rgba));
    }
    let img = crate::pack_entities::skin_image(crate::pack_entities::default_skin_for(player))?;
    Some((img.w, img.rgba))
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
    /// The memory budget (client/memory) and what it lets the game use now.
    mem: crate::client::memory::MemWatch,
    eff: crate::budget::Effective,
    /// The window is not in front / is minimised: the loop slows down (client/pacing).
    unfocused: bool,
    minimised: bool,
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
    /// Occlusion culling of the world's sections is off (F3 + O, `--show nocull`).
    no_occlusion: bool,
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
    /// How much of the view the lava of the head's block has filled, 0 to 1 (terrain/lava_look.rs), and when it was stepped.
    lava_fade: f32,
    lava_stepped: f32,
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
    /// The 64 x 64 PNG of the own skin and the side of `skin_png` (packed off the main thread, see `client/skins`).
    skin_small: Vec<u8>,
    skin_side: usize,
    /// The side the own skin texture has on the card.
    skin_tex_side: usize,
    /// Other players' skins on the card, within a budget, and the packing of the own one.
    skins: crate::client::skins::SkinHub,
    /// The skin picture is for slim arms (found from the picture, see `skinfile::is_slim`).
    skin_slim: bool,
    /// For pictures (`--show eat:F`): hold the hand at this point of eating.
    show_eat: Option<f32>,
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
    tags: Vec<(f32, f32, String, f32)>,
    /// Hearts to draw under other players' name tags this frame: screen position, health, most health, scale.
    tag_hearts: core::cell::RefCell<Vec<(f32, f32, i32, i32, f32)>>,
    /// The map screen: centre in "unit" map coordinates (0..1) and zoom.
    /// Whose screen the map key has opened (a mod's code).
    map_native: Option<usize>,
    status: String,
    /// The PIN screen's state (screens/pin.rs).
    pin: PinPrompt,
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
                        s.give_or_drop(c.item, c.count);
                    }
                }
            }
        }
        // Leaving a mod's screen: its mod hears that it closed.
        if self.screen == Screen::ModUi && screen != Screen::ModUi {
            crate::client::modui::dispatch::closed_by_player();
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
        self.audio.set_ambience_volume(s.ambience_volume);
        self.audio.set_music_volume_setting(s.music_volume);
        self.distance = self.distance.min(s.distance).max(2);
        if s.target_fps == 0 {
            self.distance = s.distance;
        }
        unsafe { (sdl::sdl().SDL_GL_SetSwapInterval)((s.vsync && !self.scripted) as c_int) };
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
        self.skin_sent = false; // a new session knows nothing of our skin yet
        self.distance = self.settings.distance;
        self.set_screen(Screen::Game);
    }

    fn close_world(&mut self, r: &mut Renderer) {
        self.loading_since = 0.0;
        // Other players' skins belong to the players of that visit (their numbers are used again
        // on the next one): forget them, so nobody wears someone else's skin.
        let stale: Vec<u32> = self.remote_skins.values().copied().collect();
        r.delete_textures(&stale);
        self.remote_skins.clear();
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
        // The world's memory goes back to the system rather than sitting in the allocator.
        crate::sys::heap::trim();
        // Nothing of the world left may go on sounding.
        self.audio.silence();
        // A game hosted from the menu ends when its host leaves.
        if let Some(t) = self.host.take() {
            HOST_STOP.store(true, core::sync::atomic::Ordering::Relaxed);
            t.join();
            HOST_STOP.store(false, core::sync::atomic::Ordering::Relaxed);
        }
    }


    fn save_screenshot(&mut self, r: &Renderer) {
        sys::make_dir(&alloc::format!("{}screenshots/", self.base));
        // The clock restarts with the machine, so the name may have been used before: never write over an old picture.
        let stamp = (sys::now() * 1000.0) as u64;
        let mut path = alloc::format!("{}screenshots/screenshot-{}.bmp", self.base, stamp);
        let mut n = 2;
        while sys::file_exists(&path) && n < 1000 {
            path = alloc::format!("{}screenshots/screenshot-{}-{}.bmp", self.base, stamp, n);
            n += 1;
        }
        let ok = sys::write_file(&path, &r.screenshot());
        if let Some(s) = self.session.as_mut() {
            s.say(if ok { "SCREENSHOT SAVED" } else { "COULD NOT SAVE SCREENSHOT" });
        }
        if ok {
            logf!("screenshot: {}", path);
        }
    }

    // --------------------------------------------------------------- input










    // -------------------------------------------------------------- update




    // ---------------------------------------------------------------- draw




    // ------------------------------------------------------------- screens

























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
                r.label_cased(at, y, s, &run, [col[0], col[1], col[2], alpha]);
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
    (g.blow_left() / 0.35).max(g.cooldown[1] / 0.5).clamp(0.0, 1.0)
}

/// What the first-person hand is doing: the swing of a blow or use (from its
/// start, as LastRoute's tweens run), the walking bob, a bow's draw and eating.
fn hand_view_of(g: &session::Session, held: crate::client::hand::Held, speed: f32) -> crate::client::hand::View {
    use crate::client::hand::Held;
    let index = g.held().map(|h| h.0);
    let id = index.map_or(0, |i| i as u16);
    // The cooldowns count down from a blow's or a use's start; the swing runs up from it.
    let left = (g.blow_left() / 0.35).max(g.cooldown[1] / 0.5).clamp(0.0, 1.0);
    let swing = if left > 0.0 { (1.0 - left).max(0.001) } else { 0.0 };
    let draw = if matches!(held, Held::Bow(_)) { g.bow.shown(id) } else { 0.0 };
    let eat = g.eating.shown(id);
    // The bow's own motion is its draw; eating is not a blow.
    let swing = if matches!(held, Held::Bow(_)) || eat > 0.0 { 0.0 } else { swing };
    crate::client::hand::View {
        swing,
        bob: crate::client::hand::walk_bob(g.posing.gait / 9.0, speed, g.player.on_ground),
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
        crate::client::wear_bar::draw(r, x, y, size, s, &st);
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
                    let s = sys::from_cstr_utf8(p);
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
    // Long sounds and model files are read again from their files when wanted (see util/data/reread).
    crate::util::data::reread::set_reader(read_mod_file);
    let (first, mut pack_text) = load_content(&pack_paths, &mods_off);
    block::install(first);
    phases.mark("content: base data and mods parsed");
    // Pictures the player has edited replace the game's own: textures/<id>.png.
    sys::make_dir(&alloc::format!("{}textures/", base));
    crate::textures::set_override_dir(&alloc::format!("{}textures/", base));
    // Texture packs named on the command line (they are added to the game's own textures, which stay the default).
    {
        let early = sys::read_file(&alloc::format!("{}settings.txt", base)).map(|b| crate::settings::Settings::parse(&String::from_utf8_lossy(&b))).unwrap_or_default();
        // The most pixels on a side an HD picture is kept at (content/hd).
        crate::hd::set_cap_setting(early.texture_size);
        let list: Vec<String> = opts.texture_packs.iter().cloned().collect();
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

    let threads = opts.threads.unwrap_or_else(|| thread::cpu_count().saturating_sub(1).clamp(1, 6));
    let mut app = App {
        window,
        audio: if scripted { Audio::silent() } else { Audio::new() },
        voice: crate::client::voice::Voice::new(),
        distance: settings.distance,
        settings,
        settings_path,
        base: base.clone(),
        threads,
        mem: crate::client::memory::MemWatch::new(false, opts.log.is_some()),
        eff: crate::budget::Effective { distance: 8, far: 0, lod_real: 0, lod_trees: 0, lod_detail: 0 },
        unfocused: false,
        minimised: false,
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
        no_occlusion: opts.show.contains("nocull"),
        help_timer: 0.0,
        loading_since: 0.0,
        wheel: Default::default(),
        third_person: opts.show.contains("third") || opts.show.contains("front") || opts.show.contains("bends:") || opts.show.contains("swing:"),
        third_front: opts.show.contains("front"),
        swallow_text: false,
        hud_hidden: false,
        ui_clock: 0.0,
        lava_fade: 0.0,
        lava_stepped: 0.0,
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
        skin_small: Vec::new(),
        skin_side: 64,
        skin_tex_side: 64,
        skins: Default::default(),
        skin_slim: false,
        skin_sent: false,
        pack_paths,
        mods_dir,
        serve_mods: opts.serve_mods.as_deref().and_then(crate::net::modsync::serve::parse_serve_list),
        pack_text,
        server_content: false,
        want_reload: false,
        at_oven: false,
        show_walking: opts.show.contains("walking"),
        // "eat:F": the first-person hand held F (0 to 1) of the way through eating, for pictures.
        show_eat: opts.show.split(',').find_map(|w| w.strip_prefix("eat:")).and_then(|f| f.parse::<f32>().ok()).map(|f| f.clamp(0.0, 1.0)),
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
        tag_hearts: core::cell::RefCell::new(Vec::new()),
        map_native: None,
        status: String::new(),
        pin: PinPrompt::default(),
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
    // The sound and music files of mods are found in their own folders.
    app.audio.set_mods_dir(&app.mods_dir);
    crate::client::modui::hud_image::set_mods_dir(&app.mods_dir);
    app.refresh_mem_limit();
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
            let one = |it: u16| Some(crate::inventory::Stack::new(it, 1));
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
            // "bends:NAME[@PHASE]": one posture and gait (idle walk sprint crouchwalk swim climb sit sleep fall ride), third person.
            crate::client::show_bends::apply(&opts.show, &mut g.posing);
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
        // "page" opens the sub-page of that tab (video: detail and distance;
        // controls: key bindings); "bindscroll:N" scrolls the key bindings.
        if opts.show.split(',').any(|w| w == "page") {
            app.video_detail = 1;
        }
        if let Some(n) = opts.show.split(',').find_map(|w| w.strip_prefix("bindscroll:")).and_then(|n| n.parse::<i32>().ok()) {
            crate::client::modui::with_ui(|u| u.scroll_binds = n);
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
        app.update(&mut r, scripted_step::frame_dt(scripted && !online, opts.step, dt), dt);
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
                        "voxelrust: bench: {} frames, {:.2} ms average ({:.0} FPS), {:.2} ms worst, {} quads, view distance {}, {} sections drawn ({} frustum culled, {} occluded), {} draw calls, {:.2} ms culling",
                        n,
                        total * 1000.0 / n as f64,
                        n as f64 / total,
                        *worst * 1000.0,
                        r.stats.quads,
                        app.distance,
                        r.culler.stats.drawn,
                        r.culler.stats.frustum,
                        r.culler.stats.occluded,
                        r.stats.draw_calls,
                        r.culler.stats.micros as f32 / 1000.0
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
        // Do not spin the CPU in menus, with the window behind others or minimised, and honour
        // the frame-rate limit (client/pacing).
        let in_menu = !matches!(app.screen, Screen::Game | Screen::Inventory);
        let facts = crate::client::pacing::Facts { in_menu, focused: !app.unfocused, minimised: app.minimised, vsync: app.settings.vsync, max_fps: app.settings.max_fps, scripted, gpu_limit: app.settings.gpu_limit };
        let spent = sys::now() - frame_start;
        let mut wait = crate::client::pacing::wait_ms(crate::client::pacing::cap(&facts), spent);
        if !scripted && !app.settings.vsync && !facts.minimised {
            wait = wait.max(crate::client::pacing::gpu_wait_ms(app.settings.gpu_limit, spent));
        }
        if wait > 0 {
            unsafe { (api.SDL_Delay)(wait) };
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



pub fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut args = Vec::new();
    for i in 0..argc.max(0) as usize {
        args.push(unsafe { sys::from_cstr_utf8(*argv.add(i)) });
    }
    sys::heap::tune();
    match run(args) {
        Ok(()) => crate::client::server_update::restart::exit_code(),
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
        let stone = Stack::new(3, 64);
        let dirt = Stack::new(4, 1);
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
