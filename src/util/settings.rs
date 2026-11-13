//! Player settings: video, audio, controls and game options, kept as a
//! small `key=value` text file.

use alloc::string::String;
use alloc::vec::Vec;

/// Things a key can be bound to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Descend,
    Sprint,
    Fly,
    Inventory,
    Map,
    Waypoints,
    Chat,
    Drop,
}

pub const ACTIONS: [(Action, &str, &str, i32); 13] = [
    (Action::Forward, "forward", "MOVE FORWARD", 26),
    (Action::Back, "back", "MOVE BACK", 22),
    (Action::Left, "left", "MOVE LEFT", 4),
    (Action::Right, "right", "MOVE RIGHT", 7),
    (Action::Jump, "jump", "JUMP / FLY UP", 44),
    (Action::Descend, "descend", "FLY DOWN", 225),
    (Action::Sprint, "sprint", "SPRINT", 224),
    (Action::Fly, "leave", "GET OFF / GET OUT", 9),
    (Action::Inventory, "inventory", "INVENTORY", 8),
    (Action::Map, "map", "WORLD MAP", 16),
    (Action::Waypoints, "waypoints", "WAYPOINT MAP", 17),
    (Action::Chat, "chat", "CHAT", 23),
    (Action::Drop, "drop", "DROP THE HELD ITEM", 20),
];

/// Built-in skins: a name and the colour the default body is tinted with.
pub const SKINS: [(&str, [u8; 3]); 8] = [
    ("default", [255, 255, 255]),
    ("clay", [230, 150, 130]),
    ("moss", [150, 210, 140]),
    ("slate", [150, 170, 215]),
    ("charcoal", [95, 95, 105]),
    ("snow", [255, 255, 255]),
    ("rose", [240, 160, 200]),
    ("gold", [255, 220, 120]),
];

/// Full-screen resolutions on offer; (0, 0) is "whatever the desktop uses".
pub const FULLSCREEN_MODES: [(i32, i32); 8] = [(0, 0), (1600, 900), (1280, 800), (1280, 720), (1024, 768), (960, 600), (800, 600), (640, 480)];

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    // Video
    /// Full-detail view distance, in chunks.
    pub distance: i32,
    /// How far simplified distant terrain is drawn, in chunks (0 = off).
    pub far_distance: i32,
    /// How many tiles (of 4 x 4 chunks) out the distant terrain is made from
    /// the world's real blocks (builds and trees as they are), 0..=8; beyond
    /// that it is made from the terrain function alone. 0 = never from blocks.
    pub lod_real: i32,
    /// Trees in the distance: 0 none, 1 some, 2 all.
    pub lod_trees: i32,
    /// How finely distant terrain is sampled: 0 coarse, 1 medium, 2 fine.
    pub lod_detail: i32,
    pub fov: i32,
    pub clouds: bool,
    pub vsync: bool,
    pub fullscreen: bool,
    /// Index into [`FULLSCREEN_MODES`]: 0 keeps the desktop's resolution,
    /// others switch the display to a smaller one (much faster on old cards).
    pub fullscreen_mode: i32,
    /// Percentage of full resolution the 3D view is drawn at (25..=100).
    pub render_scale: i32,
    /// 0 = automatic.
    pub gui_scale: i32,
    /// Frames per second to hold by adjusting the view distance (0 = off).
    pub target_fps: i32,
    /// Frame-rate cap when vsync is off (0 = none).
    pub max_fps: i32,
    pub show_fps: bool,
    // Audio, 0..=10
    pub master_volume: i32,
    pub effects_volume: i32,
    pub steps_volume: i32,
    pub ui_volume: i32,
    /// Show in words what is heard.
    pub subtitles: bool,
    // Controls
    pub sens: i32,
    pub invert_y: bool,
    /// SDL scancode per entry of [`ACTIONS`].
    pub keys: [i32; ACTIONS.len()],
    // Game
    /// `default`, a preset's name, or `custom` (skin.png in the data folder).
    pub skin: String,
    pub player_name: String,
    pub autosave_seconds: i32,
    pub view_bobbing: bool,
    /// Rain, snow and storms.
    /// Full screen has been timed on this machine and its resolution chosen.
    pub auto_tuned: bool,
    pub weather: bool,
    /// The small map in the corner of the screen.
    pub minimap: bool,
    /// The full map (M): zoom step (index into `client::fullmap::MAP_SCALES`),
    /// the three presets (indices), and what it shows when it opens.
    pub map_zoom: i32,
    pub map_preset: [i32; 3],
    pub map_follow: bool,
    pub map_grid: bool,
    pub map_waypoints: bool,
    /// Slim (Alex-style) arms for the player model, instead of the classic
    /// four-pixel arms. Other players see it too.
    pub slim_arms: bool,
    /// The switches packs add (`setting <id> ...`), as last set here.
    pub flags: Vec<(String, bool)>,
    /// Settings the game itself has no name for: what the mods' own code
    /// keeps here (see [`Settings::extra`]). Kept as written.
    pub extras: Vec<(String, String)>,
    /// Names of mods that are switched off, separated by commas.
    pub mods_off: String,
    /// Install newer versions from the nightly release by itself.
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        let mut keys = [0; ACTIONS.len()];
        for (i, a) in ACTIONS.iter().enumerate() {
            keys[i] = a.3;
        }
        Settings {
            distance: 8,
            far_distance: 32,
            lod_real: 3,
            lod_trees: 2,
            lod_detail: 1,
            fov: 70,
            clouds: true,
            vsync: false,
            fullscreen: false,
            fullscreen_mode: 0,
            render_scale: 100,
            gui_scale: 0,
            target_fps: 0,
            max_fps: 0,
            show_fps: false,
            master_volume: 8,
            effects_volume: 8,
            steps_volume: 6,
            ui_volume: 6,
            subtitles: false,
            sens: 5,
            invert_y: false,
            keys,
            skin: String::from("default"),
            player_name: String::from("Player"),
            autosave_seconds: 30,
            view_bobbing: true,
            auto_tuned: false,
            weather: true,
            minimap: true,
            map_zoom: 2,
            map_preset: [0, 2, 4],
            map_follow: true,
            map_grid: false,
            map_waypoints: true,
            slim_arms: false,
            flags: Vec::new(),
            extras: Vec::new(),
            mods_off: String::new(),
            auto_update: true,
        }
    }
}

/// Value of `key=` in a small text file.
pub fn kv<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let l = l.trim();
        if l.len() > key.len() && l.starts_with(key) && l.as_bytes()[key.len()] == b'=' {
            Some(&l[key.len() + 1..])
        } else {
            None
        }
    })
}

/// Keep only characters that are safe in names shown on screen and sent to
/// other players.
/// A world or server name: letters of either case, digits, spaces, `_` and
/// `-`, with the ends trimmed.
pub fn clean_world_name(s: &str, max: usize) -> String {
    let kept: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == ' ').take(max).collect();
    String::from(kept.trim())
}

pub fn clean_name(s: &str, max: usize) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(max).collect()
}

impl Settings {
    pub fn key(&self, a: Action) -> i32 {
        self.keys[ACTIONS.iter().position(|x| x.0 == a).unwrap_or(0)]
    }

    /// A setting kept for a mod's own code (`""` if never set).
    pub fn extra(&self, key: &str) -> &str {
        self.extras.iter().find(|e| e.0 == key).map_or("", |e| e.1.as_str())
    }

    pub fn set_extra(&mut self, key: &str, value: &str) {
        self.extras.retain(|e| e.0 != key);
        if !value.is_empty() {
            self.extras.push((String::from(key), String::from(value)));
        }
    }

    pub fn parse(text: &str) -> Settings {
        let d = Settings::default();
        let int = |k: &str, def: i32, lo: i32, hi: i32| kv(text, k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(def).clamp(lo, hi);
        let flag = |k: &str, def: bool| kv(text, k).map(|v| v == "1").unwrap_or(def);
        let mut keys = d.keys;
        for (i, a) in ACTIONS.iter().enumerate() {
            keys[i] = int(&alloc::format!("key_{}", a.1), a.3, 1, 511);
        }
        let name = clean_name(kv(text, "player_name").unwrap_or(&d.player_name), 16);
        Settings {
            distance: int("distance", d.distance, 2, 48),
            far_distance: int("far_distance", d.far_distance, 0, 256),
            lod_real: int("lod_real", d.lod_real, 0, 8),
            lod_trees: int("lod_trees", d.lod_trees, 0, 2),
            lod_detail: int("lod_detail", d.lod_detail, 0, 2),
            fov: int("fov", d.fov, 50, 110),
            clouds: flag("clouds", d.clouds),
            vsync: flag("vsync", d.vsync),
            fullscreen: flag("fullscreen", d.fullscreen),
            fullscreen_mode: int("fullscreen_mode", 0, 0, FULLSCREEN_MODES.len() as i32 - 1),
            render_scale: int("render_scale", 100, 25, 100),
            gui_scale: int("gui_scale", d.gui_scale, 0, 6),
            target_fps: int("target_fps", d.target_fps, 0, 1000),
            max_fps: int("max_fps", d.max_fps, 0, 1000),
            show_fps: flag("show_fps", d.show_fps),
            master_volume: int("master_volume", d.master_volume, 0, 10),
            effects_volume: int("effects_volume", d.effects_volume, 0, 10),
            steps_volume: int("steps_volume", d.steps_volume, 0, 10),
            ui_volume: int("ui_volume", d.ui_volume, 0, 10),
            subtitles: flag("subtitles", false),
            sens: int("sens", d.sens, 1, 20),
            invert_y: flag("invert_y", d.invert_y),
            keys,
            skin: clean_name(kv(text, "skin").unwrap_or("default"), 16),
            player_name: if name.is_empty() { d.player_name } else { name },
            autosave_seconds: int("autosave_seconds", d.autosave_seconds, 10, 600),
            view_bobbing: flag("view_bobbing", d.view_bobbing),
            auto_tuned: flag("auto_tuned", false),
            weather: flag("weather", true),
            minimap: flag("minimap", true),
            map_zoom: int("map_zoom", d.map_zoom, 0, 6),
            map_preset: [int("map_preset_1", d.map_preset[0], 0, 6), int("map_preset_2", d.map_preset[1], 0, 6), int("map_preset_3", d.map_preset[2], 0, 6)],
            map_follow: flag("map_follow", true),
            map_grid: flag("map_grid", false),
            map_waypoints: flag("map_waypoints", true),
            slim_arms: flag("slim_arms", false),
            flags: text.lines().filter_map(|l| l.strip_prefix("flag_")).filter_map(|l| l.split_once('=')).map(|(k, v)| (String::from(k.trim()), v.trim() == "1")).take(64).collect(),
            extras: {
                // Whatever is not one of the game's own settings.
                let own = Settings::default().to_text();
                text.lines()
                    .filter_map(|l| l.trim().split_once('='))
                    .filter(|(k, _)| !k.is_empty() && k.len() <= 32 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|(k, _)| !k.starts_with("key_") && !k.starts_with("flag_") && kv(&own, k).is_none())
                    .map(|(k, v)| (String::from(k), v.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(120).collect::<String>()))
                    .take(32)
                    .collect()
            },
            auto_update: flag("auto_update", true),
            mods_off: kv(text, "mods_off").unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '_' | '-')).take(400).collect(),
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = alloc::format!(
            "distance={}\nfar_distance={}\nlod_real={}\nlod_trees={}\nlod_detail={}\nfov={}\nclouds={}\nvsync={}\nfullscreen={}\ngui_scale={}\ntarget_fps={}\nmax_fps={}\nshow_fps={}\n\
             master_volume={}\neffects_volume={}\nsteps_volume={}\nui_volume={}\nsens={}\ninvert_y={}\nplayer_name={}\nautosave_seconds={}\nview_bobbing={}\n",
            self.distance,
            self.far_distance,
            self.lod_real,
            self.lod_trees,
            self.lod_detail,
            self.fov,
            self.clouds as i32,
            self.vsync as i32,
            self.fullscreen as i32,
            self.gui_scale,
            self.target_fps,
            self.max_fps,
            self.show_fps as i32,
            self.master_volume,
            self.effects_volume,
            self.steps_volume,
            self.ui_volume,
            self.sens,
            self.invert_y as i32,
            self.player_name,
            self.autosave_seconds,
            self.view_bobbing as i32,
        );
        s.push_str(&alloc::format!("fullscreen_mode={}\nrender_scale={}\nskin={}\nweather={}\nminimap={}\nslim_arms={}\n", self.fullscreen_mode, self.render_scale, self.skin, self.weather as i32, self.minimap as i32, self.slim_arms as i32));
        s.push_str(&alloc::format!("map_zoom={}\nmap_preset_1={}\nmap_preset_2={}\nmap_preset_3={}\nmap_follow={}\nmap_grid={}\nmap_waypoints={}\n", self.map_zoom, self.map_preset[0], self.map_preset[1], self.map_preset[2], self.map_follow as i32, self.map_grid as i32, self.map_waypoints as i32));
        s.push_str(&alloc::format!("auto_tuned={}\nsubtitles={}\nmods_off={}\nauto_update={}\n", self.auto_tuned as i32, self.subtitles as i32, self.mods_off, self.auto_update as i32));
        for (i, a) in ACTIONS.iter().enumerate() {
            s.push_str(&alloc::format!("key_{}={}\n", a.1, self.keys[i]));
        }
        for (k, v) in &self.extras {
            s.push_str(&alloc::format!("{}={}\n", k, v));
        }
        for (id, on) in &self.flags {
            s.push_str(&alloc::format!("flag_{}={}\n", id, *on as i32));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_map_settings_default_clamp_and_round_trip() {
        let d = Settings::default();
        assert_eq!((d.key(Action::Waypoints), d.map_zoom, d.map_preset, d.map_follow, d.map_grid, d.map_waypoints), (17, 2, [0, 2, 4], true, false, true));
        let s = Settings::parse("map_zoom=99\nmap_preset_1=-4\nmap_preset_2=3\nmap_grid=1\nmap_follow=0\n");
        assert_eq!((s.map_zoom, s.map_preset, s.map_grid, s.map_follow), (6, [0, 3, 4], true, false));
        let back = Settings::parse(&s.to_text());
        assert_eq!((back.map_zoom, back.map_preset, back.map_grid, back.map_follow, back.map_waypoints), (6, [0, 3, 4], true, false, true));
        assert!(back.extra("map_zoom").is_empty(), "the map settings are typed, not extras");
    }

    use super::*;

    #[test]
    fn round_trip_and_clamping() {
        let mut s = Settings::default();
        s.distance = 20;
        s.invert_y = true;
        s.keys[0] = 82;
        s.player_name = String::from("Kyttu_1");
        s.set_extra("relay", "10.0.0.5:25590");
        s.flags.push((String::from("storms"), true));
        assert_eq!(Settings::parse(&s.to_text()), s);
        let odd = Settings::parse("distance=9999\nfov=abc\nplayer_name=<b>x y</b>\nkey_jump=0\n");
        assert_eq!(odd.distance, 48);
        assert_eq!(odd.fov, 70);
        assert_eq!(odd.player_name, "bxyb");
        assert_eq!(odd.key(Action::Jump), 1);
        assert_eq!(Settings::parse(""), Settings::default());
    }

    #[test]
    fn key_value_lookup() {
        let t = "seed=7\npos=1 2 3\nseedling=9\n";
        assert_eq!(kv(t, "seed"), Some("7"));
        assert_eq!(kv(t, "pos"), Some("1 2 3"));
        assert_eq!(kv(t, "see"), None);
    }
}
