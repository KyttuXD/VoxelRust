//! The dedicated server and the server hosted from the menu (moved out of app.rs).

#[allow(unused_imports)]
use super::*;

/// Set to stop a server started from the multiplayer menu.
pub(super) static HOST_STOP: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Open the server for the world `name`: its small save (seed, kind and
/// every edit) is read, or made on first use.
pub(super) fn open_server(name: &str, seed: Option<u32>, new_kind: Option<String>, port: u16, players: usize, base: &str, pack: &[u8], mods: Vec<crate::net::modsync::serve::ServedMod>, settings: &Settings) -> Result<(crate::server::Server, String), String> {
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
pub(super) fn server_loop(mut server: crate::server::Server, edits_path: String, quiet: bool) -> bool {
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
pub(super) fn update_ready_and_empty(server: &crate::server::Server) -> bool {
    crate::update::state() == crate::update::UPDATED && server.players() == 0
}

/// Asked to stop (kill, a service manager, Ctrl+C): leave the server loop,
/// which saves the world's edits and frees the port.
extern "C" fn on_terminate(_: c_int) {
    HOST_STOP.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// `voxelrust --server`: host a world for others, with no window.
pub(super) fn run_server(opts: &Opts, base: &str, pack_text: &[u8], pack_paths: &[String], mods_off: &str) -> Result<(), String> {
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
