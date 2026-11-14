//! Scripts: a small subset of Rust, run by the game while it plays, so that
//! a mod need not be compiled into the program. A mod ships plain `.rs`
//! files in its `scripts/` folder; the game reads and runs them (they can
//! come down from a server like any other mod file).
//!
//! The language is in docs/SCRIPTING.md. In this folder:
//!
//! | file | what |
//! | --- | --- |
//! | `lexer.rs`, `parser.rs`, `ast.rs` | text to tree |
//! | `value.rs`, `interp.rs`, `places.rs`, `ops.rs`, `numeric.rs`, `fmt.rs` | running the tree |
//! | `builtins*.rs` | the library: numbers, strings, Vec, Option... |
//! | `host.rs`, `host_ui.rs` | what scripts may do to the game (the sandbox's only door); `host_ui.rs`: controls, HUD and screens |
//! | `host_qol.rs`, `hud.rs` | the later host calls: facing, biome, pack slots, chests, a line on the screen |
//! | `limits.rs`, `error.rs`, `state.rs` | limits, `file:line` errors, saved state |
//! | `bridge.rs` | the game's hooks (`game/hooks.rs`) calling into scripts |
//!
//! Scripts cannot touch files or the network. A call into a script has a
//! budget of steps, memory and recursion depth; going over it, or any other
//! error, is logged with `file:line`, and that script is switched off for
//! the rest of the session. Nothing here panics on bad scripts.
//!
//! [`ScriptHost`] holds every loaded script. Load a mod's with
//! [`ScriptHost::load_mod`], drop them all with [`ScriptHost::unload_all`].
//! With none loaded, the hooks cost one flag test.

pub mod ast;
pub mod bridge;
pub mod builtins;
pub mod builtins_list;
pub mod builtins_text;
pub mod check;
pub mod effects_bridge;
pub mod error;
pub mod ext;
pub mod fmt;
pub mod host;
pub mod host_ui;
pub mod host_qol;
pub mod hud;
pub mod interp;
pub mod lexer;
pub mod limits;
pub mod lint;
pub mod numeric;
pub mod ops;
pub mod parser;
pub mod places;
pub mod state;
pub mod value;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod mods_tests;
#[cfg(test)]
mod tests_qol;
#[cfg(test)]
mod tests_audit;
#[cfg(test)]
mod tests_world_hooks;

use crate::script::ast::{Name, Program};
use crate::script::error::ScriptError;
use crate::script::host::Host;
use crate::script::interp::Interp;
use crate::script::limits::Limits;
use crate::script::state::ScriptState;
use crate::script::value::Value;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// A hook scripts may define: its name, its parameters, what it does.
pub struct HookDoc {
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub what: &'static str,
}

/// Every hook, in the order the docs list them. A script defines one as a
/// plain function of that name; it may leave off parameters from the end
/// (a hook that does not need `id` can be `fn on_block_break(x: i64, y: i64, z: i64)`).
pub const HOOKS: &[HookDoc] = &[
    HookDoc { name: "on_load", params: &[], what: "the world was opened (or the script was loaded into a running game)" },
    HookDoc { name: "on_save", params: &[], what: "the world is being saved" },
    HookDoc { name: "on_tick", params: &["dt"], what: "once a frame; dt is the seconds since the last" },
    HookDoc { name: "on_block_break", params: &["x", "y", "z", "id"], what: "the player is about to break a block; return false to keep it" },
    HookDoc { name: "on_block_place", params: &["x", "y", "z", "id"], what: "the player is about to place a block; return false to stop it" },
    HookDoc { name: "on_entity_spawn", params: &["kind"], what: "a creature appeared; return false to take it away again" },
    HookDoc { name: "on_entity_death", params: &["kind", "x", "y", "z"], what: "a creature died (any cause)" },
    HookDoc { name: "on_chunk_generated", params: &["cx", "cz", "seed"], what: "a chunk was just generated (not loaded from a save): place structures with `set_block` (4096 writes at most per call; `rand_chunk` makes it repeatable)" },
    HookDoc { name: "on_block_placed", params: &["x", "y", "z", "id", "cause"], what: "a block was placed, whatever the cause (`player`, `script`, `remote`, `fall`, `fluid`, `other`)" },
    HookDoc { name: "on_creature_death", params: &["id", "kind", "x", "y", "z", "killer"], what: "a creature died: its id, kind, place and who killed it (the player's name, a creature's id or a cause such as `lava`, `script`)" },
    HookDoc { name: "on_creature_removed", params: &["id", "kind", "reason"], what: "a creature left the world: `killed`, `despawned`, `unloaded` or `scripted`" },
    HookDoc { name: "on_creature_interact", params: &["id", "kind", "item"], what: "the player right-clicked a creature (item is what they hold, or empty); return false to cancel the game's own handling" },
    HookDoc { name: "on_creature_sunburn", params: &["id", "kind"], what: "a night creature would burn in daylight; return false to let it live (asked again after 20 seconds)" },
    HookDoc { name: "on_entity_hit", params: &["id", "kind", "x", "y", "z"], what: "the player hit a creature (it may have died of it): its id (see `mob_info`), its kind and where it was" },
    HookDoc { name: "on_player_hurt", params: &["amount", "cause"], what: "the player is about to take damage; return the amount to apply instead (0 or false cancels)" },
    HookDoc { name: "on_chat", params: &["text"], what: "the player typed a line (commands too); return false to swallow it" },
    HookDoc { name: "on_item_use", params: &["item"], what: "the player pressed use holding an item; return false to stop it" },
    HookDoc { name: "on_craft", params: &["item"], what: "the player crafted an item" },
    HookDoc { name: "on_event", params: &["name", "what"], what: "any event pack scripts can answer with `when` (see MODDING.md)" },
    HookDoc { name: "on_command", params: &["name", "args"], what: "a /command the game does not know; return true (or a reply string) if it is yours" },
    HookDoc { name: "on_effect_start", params: &["id", "level"], what: "a status effect started on the player" },
    HookDoc { name: "on_effect_tick", params: &["id", "secs_left", "level"], what: "about once a second for each status effect the player has" },
    HookDoc { name: "on_effect_end", params: &["id"], what: "a status effect ran out, was put out or was taken away" },
    HookDoc { name: "on_explode", params: &["x", "y", "z", "power", "source"], what: "an explosion is about to happen; return false to cancel or a number to change its power" },
    HookDoc { name: "on_update", params: &["dt"], what: "once a frame, before the player moves: the place to move things, carry the player (`hold_player`) and ask for pictures (`draw_model`, `hud_frac_rect`)" },
    HookDoc { name: "on_interact", params: &["reach"], what: "the player's hands, before the game's own handling of a click (`reach` is how far the block looked at is); return true if the click was used up" },
    HookDoc { name: "on_condition", params: &["name", "args"], what: "an `/if` condition the game does not know (`/if name args`); return true or false if it is yours, nothing if it is not" },
    HookDoc { name: "on_net", params: &["key", "text"], what: "another player's copy of this mod sent a message with `net_send`" },
    HookDoc { name: "on_register", params: &[], what: "once when the script is read: declare key bindings and switches (bind_key, add_switch)" },
    HookDoc { name: "on_key_down", params: &["id"], what: "a key binding of this mod was pressed (while playing)" },
    HookDoc { name: "on_key_up", params: &["id"], what: "a key binding of this mod was let go" },
    HookDoc { name: "on_hud", params: &["dt"], what: "once a frame while playing: draw on the HUD (hud_text, hud_rect...)" },
    HookDoc { name: "on_ui_click", params: &["id"], what: "a button of the mod's screen was pressed" },
    HookDoc { name: "on_ui_change", params: &["id", "value"], what: "a toggle, stepper, input or list of the mod's screen changed (value: bool, number, text, line number)" },
    HookDoc { name: "on_ui_close", params: &["name"], what: "the mod's screen closed" },
];

/// One script file, read and ready.
pub struct Script {
    /// `mod/scripts/file.rs`, for messages.
    pub file: String,
    pub prog: Program,
    pub consts: Vec<(Name, Value)>,
    /// Set when it failed: why. A disabled script is never called again.
    pub disabled: Option<String>,
    /// `on_load` is yet to be called (for this world).
    pub pending_load: bool,
}

/// All of one mod's scripts, and what they keep.
pub struct ModScripts {
    pub name: String,
    pub state: ScriptState,
    pub scripts: Vec<Script>,
    /// The world the state was read for.
    pub world: Option<String>,
}

/// A host that does nothing, for working out constants when a file is loaded
/// and for running `on_register` (it keeps what the script declares in `ui`).
#[derive(Default)]
struct NullHost {
    ui: crate::client::modui::ModUi,
}

impl Host for NullHost {
    fn ui(&mut self) -> &mut crate::client::modui::ModUi {
        &mut self.ui
    }
    fn say(&mut self, _: &str) {}
    fn give(&mut self, _: &str, _: i64) -> bool {
        false
    }
    fn spawn(&mut self, _: &str, _: f64, _: f64, _: f64) -> bool {
        false
    }
    fn set_block(&mut self, _: i64, _: i64, _: i64, _: &str) -> bool {
        false
    }
    fn get_block(&mut self, _: i64, _: i64, _: i64) -> String {
        String::from("air")
    }
    fn player_pos(&mut self) -> (f64, f64, f64) {
        (0.0, 0.0, 0.0)
    }
    fn time(&mut self) -> f64 {
        0.0
    }
    fn set_time(&mut self, _: f64) {}
    fn explode(&mut self, _: f64, _: f64, _: f64, _: f64) {}
    fn effect(&mut self, _: &str, _: f64, _: i64) -> bool {
        false
    }
    fn play_sound(&mut self, _: &str) {}
    fn run_command(&mut self, _: &str) -> String {
        String::new()
    }
    fn rand(&mut self, _: i64) -> i64 {
        0
    }
    fn log(&mut self, _: &str) {}
}

impl Script {
    /// Read `src` (called `file` in messages) and work out its constants.
    pub fn load(file: &str, src: &str) -> Result<Script, ScriptError> {
        let prog = parser::parse(file, src)?;
        let mut consts = Vec::new();
        {
            let mut state = ScriptState::default();
            let mut host = NullHost::default();
            let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "", Limits::DEFAULT);
            it.init_consts()?;
        }
        Ok(Script { file: String::from(file), prog, consts, disabled: None, pending_load: true })
    }

    pub fn has_hook(&self, name: &str) -> bool {
        self.disabled.is_none() && self.prog.has_fn(name)
    }
}

/// Is this path (inside a mod folder) a script?
pub fn is_script_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p.starts_with("scripts/") && p.ends_with(".rs") && !p.split('/').any(|c| c == ".." || c.is_empty())
}

/// Whether a mod's own compiled code (the opt-in `native-mods` feature) is
/// in this build: it then runs instead of the mod's scripts, never both.
pub fn native_wins(mod_name: &str) -> bool {
    crate::native::all().iter().any(|n| n.name == mod_name)
}

/// A mod's name as a file name.
fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    if s.is_empty() || s.chars().all(|c| c == '.') {
        String::from("_")
    } else {
        s
    }
}

/// Most script files in one mod.
const MAX_FILES: usize = 32;
/// Most problems kept for display.
const MAX_PROBLEMS: usize = 50;

/// Every script loaded, mod by mod.
pub struct ScriptHost {
    pub mods: Vec<ModScripts>,
    pub rng: u32,
    /// The data folder (ending with `/`), where mods' state is kept; empty: not kept.
    pub data_dir: String,
    /// The mods loaded from the player's own mods folder (see [`ScriptHost::load_local`]).
    pub local: Vec<String>,
    /// What went wrong, newest last.
    pub problems: Vec<String>,
    /// The key bindings and switches the scripts declared in `on_register`.
    pub decls: crate::client::modui::binds::Decls,
}

static HOST: AtomicPtr<ScriptHost> = AtomicPtr::new(core::ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicBool = AtomicBool::new(false);

/// Run `f` on the global host, if there is one and it is not already in use
/// (a hook that raises an event that reaches the scripts again is not called
/// a second time).
pub fn with_host<R>(f: impl FnOnce(&mut ScriptHost) -> R) -> Option<R> {
    let p = HOST.load(Ordering::Acquire);
    if p.is_null() {
        return None;
    }
    if BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return None;
    }
    // Safety: the host is never freed, and BUSY lets one user in at a time.
    let r = f(unsafe { &mut *p });
    BUSY.store(false, Ordering::Release);
    Some(r)
}

fn global() -> *mut ScriptHost {
    let p = HOST.load(Ordering::Acquire);
    if !p.is_null() {
        return p;
    }
    let fresh = Box::into_raw(Box::new(ScriptHost::new()));
    match HOST.compare_exchange(core::ptr::null_mut(), fresh, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => fresh,
        Err(old) => {
            // Another got there first.
            drop(unsafe { Box::from_raw(fresh) });
            old
        }
    }
}

impl ScriptHost {
    pub fn new() -> ScriptHost {
        ScriptHost { mods: Vec::new(), rng: 0x2545_F491, data_dir: String::new(), local: Vec::new(), problems: Vec::new(), decls: Default::default() }
    }

    // ---- the global host (what the rest of the game uses)

    /// Load the scripts of one mod: `files` are the mod's files as
    /// (path inside the mod folder, bytes); the ones under `scripts/` ending
    /// in `.rs` are read. A mod loaded twice is replaced. Returns what was
    /// wrong (each a `mod/scripts/file.rs:line:col: message`); a file with a
    /// mistake is left out and the rest still load.
    pub fn load_mod(name: &str, files: &[(String, Vec<u8>)]) -> Vec<String> {
        let p = global();
        let _ = p;
        match with_host(|h| {
            let problems = h.add_mod(name, files);
            refresh_live(h);
            h.publish_decls();
            problems
        }) {
            Some(p) => p,
            None => alloc::vec![String::from("scripts cannot be loaded from inside a script")],
        }
    }

    /// Load the scripts of the mods in the player's own mods folder, as (mod
    /// name, the mod's files): the mods loaded this way before are dropped
    /// first, so a reload leaves exactly the mods that are switched on. The
    /// scripts of mods a server sent are left alone.
    pub fn load_local(mods: &[(String, Vec<(String, Vec<u8>)>)]) {
        let _ = global();
        with_host(|h| {
            let old = core::mem::take(&mut h.local);
            h.mods.retain(|m| !old.contains(&m.name));
            for name in &old {
                h.drop_ui(name);
            }
            for (name, files) in mods {
                h.add_mod(name, files);
                h.local.push(name.clone());
            }
            refresh_live(h);
            h.publish_decls();
        });
    }

    /// Forget the named mods' scripts.
    pub fn unload_mods(names: &[String]) {
        with_host(|h| {
            h.mods.retain(|m| !names.contains(&m.name));
            for name in names {
                h.drop_ui(name);
            }
            refresh_live(h);
            h.publish_decls();
        });
    }

    /// Forget every script (a mod was switched off, or the player left a server).
    pub fn unload_all() {
        with_host(|h| {
            let names: Vec<String> = h.mods.iter().map(|m| m.name.clone()).collect();
            h.mods.clear();
            for name in &names {
                h.drop_ui(name);
            }
            refresh_live(h);
            h.publish_decls();
        });
    }

    /// Where the data folder is (with a `/` at the end): scripts keep their
    /// state in the mods' folders under it.
    pub fn set_data_dir(base: &str) {
        let _ = global();
        with_host(|h| h.data_dir = String::from(base));
    }

    /// Whether any script is loaded and working. One atomic read.
    pub fn is_live() -> bool {
        LIVE.load(Ordering::Relaxed)
    }

    /// What went wrong so far, for showing the player.
    pub fn all_problems() -> Vec<String> {
        with_host(|h| h.problems.clone()).unwrap_or_default()
    }

    // ---- on a host (what the tests use)

    /// Add (or replace) a mod's scripts.
    pub fn add_mod(&mut self, name: &str, files: &[(String, Vec<u8>)]) -> Vec<String> {
        // A mod that has its compiled code in this build (`native-mods`) runs
        // that, and its scripts stay off: the two would do everything twice.
        if native_wins(name) {
            self.mods.retain(|m| m.name != name);
            if files.iter().any(|f| is_script_path(&f.0)) {
                crate::sys::log(&format!("voxelrust: the mod `{}` has compiled code in this build, so its scripts are not run", name));
            }
            return Vec::new();
        }
        let mut problems: Vec<String> = Vec::new();
        let mut wanted: Vec<&(String, Vec<u8>)> = files.iter().filter(|f| is_script_path(&f.0)).collect();
        wanted.sort_by(|a, b| a.0.cmp(&b.0));
        if wanted.len() > MAX_FILES {
            problems.push(format!("{}: only the first {} script files are read", name, MAX_FILES));
            wanted.truncate(MAX_FILES);
        }
        let mut scripts = Vec::new();
        for (path, bytes) in wanted {
            let shown = format!("{}/{}", name, path.replace('\\', "/"));
            let src = match core::str::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    problems.push(format!("{}:1: the file is not valid UTF-8 text", shown));
                    continue;
                }
            };
            match Script::load(&shown, src) {
                Ok(s) => scripts.push(s),
                Err(e) => problems.push(format!("{}", e)),
            }
        }
        self.mods.retain(|m| m.name != name);
        self.decls.remove_mod(name);
        // `on_register`: what the scripts declare (key bindings, switches).
        for s in scripts.iter_mut() {
            if s.prog.has_fn("on_register") {
                let mut host = NullHost::default();
                host.ui.registering = true;
                let mut errors = Vec::new();
                invoke(&mut host, name, &mut ScriptState::default(), s, "on_register", &[], &mut errors);
                for b in core::mem::take(&mut host.ui.decls.binds) {
                    let _ = self.decls.add_bind(name, &b.id, &b.label, &b.group, b.default_key);
                }
                for sw in core::mem::take(&mut host.ui.decls.switches) {
                    let _ = self.decls.add_switch(name, &sw.id, &sw.label, sw.default);
                }
                problems.extend(errors);
            }
        }
        if !scripts.is_empty() {
            self.mods.push(ModScripts { name: String::from(name), state: ScriptState::default(), scripts, world: None });
        }
        for p in &problems {
            crate::sys::log(&format!("voxelrust: script problem: {}", p));
        }
        self.note(problems.iter().cloned());
        problems
    }

    /// Drop what a mod put on the interface (its HUD lines, its screen, its declarations).
    fn drop_ui(&mut self, name: &str) {
        self.decls.remove_mod(name);
        crate::client::modui::dispatch::forget(name);
    }

    /// Tell the interface what the scripts declared (the switches' defaults).
    fn publish_decls(&self) {
        crate::client::modui::dispatch::publish_defaults(&self.decls.switches);
    }

    fn note(&mut self, new: impl Iterator<Item = String>) {
        self.problems.extend(new);
        let n = self.problems.len();
        if n > MAX_PROBLEMS {
            self.problems.drain(..n - MAX_PROBLEMS);
        }
    }

    /// Whether any script could still be called.
    pub fn any_working(&self) -> bool {
        self.mods.iter().any(|m| m.scripts.iter().any(|s| s.disabled.is_none()))
    }

    fn state_path(&self, mod_name: &str, world: &str, create: bool) -> String {
        let dir = if create { crate::native::mod_data_dir(&self.data_dir, &safe_name(mod_name)) } else { format!("{}mods-data/{}/", self.data_dir, safe_name(mod_name)) };
        format!("{}world-{}.txt", dir, safe_name(world))
    }

    /// Call `on_load` for the scripts that have not had it, after reading
    /// each mod's state for this world.
    pub fn prepare(&mut self, host: &mut dyn Host, world: &str) {
        for i in 0..self.mods.len() {
            if self.mods[i].world.as_deref() != Some(world) {
                let state = if self.data_dir.is_empty() {
                    ScriptState::default()
                } else {
                    let path = self.state_path(&self.mods[i].name, world, false);
                    match crate::sys::read_file(&path) {
                        Some(b) => ScriptState::from_text(&String::from_utf8_lossy(&b)),
                        None => ScriptState::default(),
                    }
                };
                let m = &mut self.mods[i];
                m.state = state;
                m.world = Some(String::from(world));
                for s in m.scripts.iter_mut() {
                    s.pending_load = true;
                }
            }
        }
        let mut errors = Vec::new();
        for m in self.mods.iter_mut() {
            host.set_mod(&m.name);
            let ModScripts { name, state, scripts, .. } = m;
            for s in scripts.iter_mut() {
                if s.pending_load && s.disabled.is_none() {
                    s.pending_load = false;
                    if s.prog.has_fn("on_load") {
                        invoke(host, name, state, s, "on_load", &[], &mut errors);
                    }
                }
            }
        }
        self.finish_errors(errors);
    }

    fn finish_errors(&mut self, errors: Vec<String>) {
        if !errors.is_empty() {
            self.note(errors.into_iter());
            refresh_live(self);
        }
    }

    /// Call `hook` in every script that defines it. `on_result` hears what
    /// each returned (a script that returns nothing is not reported) and
    /// answers `true` to stop asking the rest.
    pub fn call_hook(&mut self, host: &mut dyn Host, world: &str, hook: &str, args: &[Value], on_result: &mut dyn FnMut(&Value) -> bool) {
        self.call_hook_in(host, world, None, hook, args, on_result);
    }

    /// Like [`ScriptHost::call_hook`], but only the mods `only` accepts (by name) are asked.
    pub fn call_hook_in(&mut self, host: &mut dyn Host, world: &str, only: Option<&dyn Fn(&str) -> bool>, hook: &str, args: &[Value], on_result: &mut dyn FnMut(&Value) -> bool) {
        self.prepare(host, world);
        if hook.is_empty() {
            return;
        }
        let mut errors = Vec::new();
        'mods: for m in self.mods.iter_mut() {
            if let Some(accept) = only {
                if !accept(&m.name) {
                    continue;
                }
            }
            host.set_mod(&m.name);
            let ModScripts { name, state, scripts, .. } = m;
            for s in scripts.iter_mut() {
                if !s.has_hook(hook) {
                    continue;
                }
                if let Some(r) = invoke(host, name, state, s, hook, args, &mut errors) {
                    if on_result(&r) {
                        break 'mods;
                    }
                }
            }
        }
        self.finish_errors(errors);
    }

    /// Write the state of every mod that changed it.
    pub fn save_states(&mut self, world: &str) {
        if self.data_dir.is_empty() {
            return;
        }
        for i in 0..self.mods.len() {
            if self.mods[i].state.dirty && self.mods[i].world.as_deref() == Some(world) {
                let path = self.state_path(&self.mods[i].name, world, true);
                let text = self.mods[i].state.to_text();
                if crate::sys::write_file(&path, text.as_bytes()) {
                    self.mods[i].state.dirty = false;
                } else {
                    crate::sys::log(&format!("voxelrust: could not save the script state to {}", path));
                }
            }
        }
    }
}

impl Default for ScriptHost {
    fn default() -> ScriptHost {
        ScriptHost::new()
    }
}

fn refresh_live(h: &ScriptHost) {
    LIVE.store(h.any_working(), Ordering::Relaxed);
}

/// Call one script's function; an error switches the script off.
fn invoke(host: &mut dyn Host, mod_name: &str, state: &mut ScriptState, s: &mut Script, hook: &str, args: &[Value], errors: &mut Vec<String>) -> Option<Value> {
    let r = {
        let mut it = Interp::new(&s.prog, &mut s.consts, host, state, mod_name, Limits::DEFAULT);
        it.call_named(hook, args.to_vec())
    };
    match r {
        Ok(Some(Value::Unit)) | Ok(None) => None,
        Ok(Some(v)) => Some(v),
        Err(e) => {
            let msg = format!("{}", e);
            crate::sys::log(&format!("voxelrust: script error in `{}`, the script is switched off: {}", hook, msg));
            s.disabled = Some(msg.clone());
            errors.push(msg);
            None
        }
    }
}

/// The script engine as the place downloaded mods' scripts go to (see
/// `content::mod_runtime`): it remembers which mods it was given, so that
/// leaving a server drops those and not the player's own mods' scripts.
pub struct Engine {
    downloaded: Vec<String>,
}

impl Engine {
    pub fn new() -> Engine {
        Engine { downloaded: Vec::new() }
    }
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

impl crate::content::mod_runtime::ScriptHost for Engine {
    fn load_mod(&mut self, name: &str, _version: &str, files: &[(String, Vec<u8>)]) {
        let _ = ScriptHost::load_mod(name, files);
        if !self.downloaded.iter().any(|n| n == name) {
            self.downloaded.push(String::from(name));
        }
    }

    fn unload_all(&mut self) {
        ScriptHost::unload_mods(&self.downloaded);
        self.downloaded.clear();
    }
}
