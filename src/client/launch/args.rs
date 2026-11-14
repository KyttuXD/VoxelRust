//! Command line: the help text, the options and their parsing (moved out of app.rs).

#[allow(unused_imports)]
use super::*;

pub(super) const USAGE: &str = "\
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

#[derive(Default)]
pub(super) struct Opts {
    pub(super) world: Option<String>,
    pub(super) seed: Option<u32>,
    pub(super) mode: Option<GameMode>,
    pub(super) join: Option<String>,
    pub(super) host: Option<String>,
    pub(super) server: bool,
    pub(super) port: Option<u16>,
    pub(super) players: Option<usize>,
    pub(super) distance: Option<i32>,
    pub(super) far: Option<i32>,
    pub(super) threads: Option<usize>,
    pub(super) size: Option<(i32, i32)>,
    pub(super) display: i32,
    pub(super) render_scale: Option<i32>,
    pub(super) fullscreen: bool,
    pub(super) novsync: bool,
    pub(super) data: Option<String>,
    pub(super) log: Option<String>,
    pub(super) packs: Vec<String>,
    /// `--mods-dir DIR`: where the player's mods are (default `<data>/mods`).
    pub(super) mods_dir: Option<String>,
    /// `--serve-mods a,b`: the mods a server sends to players.
    pub(super) serve_mods: Option<String>,
    /// `--texture-pack DIR` (repeatable): texture pack folders to use this time.
    pub(super) texture_packs: Vec<String>,
    pub(super) screenshot: Option<String>,
    pub(super) bench: Option<u32>,
    /// Fly straight ahead for this many frames, timing them (a render test).
    pub(super) flypass: Option<u32>,
    /// Chat commands to carry out once the world has loaded.
    pub(super) run: Option<String>,
    pub(super) write_icon: Option<String>,
    pub(super) no_update: bool,
    /// `--update-mirror URL`: fetch updates from this web folder (see the settings' `update_mirror`).
    pub(super) update_mirror: Option<String>,
    pub(super) import_mod: Option<String>,
    /// `--script-check FILE`: read a mod script and report its mistakes, and exit.
    pub(super) script_check: Option<String>,
    pub(super) icon_size: usize,
    pub(super) version: bool,
    /// `--stamp FILE`: write which build this is to FILE, and exit (the
    /// updater runs a new program this way before installing it).
    pub(super) stamp: Option<String>,
    /// `--update-now` (only in the update-test build): check and install at once.
    #[cfg_attr(not(feature = "update-test"), allow(dead_code))]
    pub(super) update_now: bool,
    pub(super) export: Option<String>,
    /// `--export-textures DIR`: write every texture as a PNG file, and exit.
    pub(super) export_textures: Option<String>,
    /// `--world-op delete|copy|rename NAME [TO]`: change a world's files, and exit.
    pub(super) world_op: Option<(String, String, String)>,
    pub(super) import: Option<String>,
    pub(super) pos: Option<[f64; 3]>,
    pub(super) look: Option<(f32, f32)>,
    pub(super) time: Option<f32>,
    /// `--wait-frames N`: with `--screenshot`, wait N more frames after the world settles (for remeshing).
    pub(super) wait_frames: u32,
    /// `--step [SECONDS]`: in a scripted run, how much game time each frame lasts (default none: the game stands still; `--step` alone is 1/60).
    pub(super) step: Option<f32>,
    pub(super) hidden: bool,
    pub(super) showcase: bool,
    pub(super) show: String,
    pub(super) help: bool,
}

/// Turn seed text into a number: plain numbers are used as they are, and
/// any other text is hashed.
pub(super) fn seed_from(text: &str) -> u32 {
    text.parse::<u32>().unwrap_or_else(|_| text.bytes().fold(2_166_136_261u32, |h, b| (h ^ b as u32).wrapping_mul(16_777_619)))
}

pub(super) fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut i = 1;
    let next = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| alloc::format!("{} needs a value", flag))
    };
    fn num<T: core::str::FromStr>(s: &str, flag: &str) -> Result<T, String> {
        s.parse::<T>().map_err(|_| alloc::format!("{}: `{}` is not a valid number", flag, s))
    }
    // `nan` and `inf` parse as floats; no option means them.
    fn real<T: core::str::FromStr + Into<f64> + Copy>(s: &str, flag: &str) -> Result<T, String> {
        let v = num::<T>(s, flag)?;
        if Into::<f64>::into(v).is_finite() {
            Ok(v)
        } else {
            Err(alloc::format!("{}: `{}` is not a valid number", flag, s))
        }
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
                    (Some(w), Some(h)) if (160..=16384).contains(&w) && (120..=16384).contains(&h) => o.size = Some((w, h)),
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
                    *v = real::<f64>(&next(&mut i, a)?, a)?;
                }
                o.pos = Some(p);
            }
            "--look" => {
                let yaw = real::<f32>(&next(&mut i, a)?, a)?;
                let pitch = real::<f32>(&next(&mut i, a)?, a)?;
                o.look = Some((yaw * PI / 180.0, pitch.clamp(-89.0, 89.0) * PI / 180.0));
            }
            "--time" => o.time = Some(real::<f32>(&next(&mut i, a)?, a)?.clamp(0.0, 1.0)),
            "--wait-frames" => o.wait_frames = num::<u32>(&next(&mut i, a)?, a)?,
            "--step" => {
                // With no number after it: a sixtieth of a second (see launch/step.rs).
                let (v, used) = super::scripted_step::parse(args.get(i + 1).map(|s| s.as_str()))?;
                if used {
                    i += 1;
                }
                o.step = Some(v);
            }
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
