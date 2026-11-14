//! The place where a script host (the runtime script engine in `src/script/`)
//! plugs in to run the scripts of mods that were downloaded from a server.
//!
//! The game does not know any interpreter. A host implements [`ScriptHost`]
//! and registers itself once at start-up with [`set_host`]; after that
//! `net::modsync` calls [`load_mod`] with the files of every mod it took from
//! a server (scripts are the `.rs` files; the host picks what it needs) and
//! [`unload_all`] when the player leaves the server. With no host registered
//! both do nothing, so the game builds and runs without any script engine.
//!
//! The merge point for the script engine is one line at start-up:
//! `crate::content::mod_runtime::set_host(Box::new(crate::script::ScriptHost::new()))`
//! (with `impl ScriptHost for crate::script::ScriptHost`).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

/// What the game needs from a script engine.
pub trait ScriptHost {
    /// A mod's files arrived (paths are inside the mod's folder, already
    /// checked: relative, no `..`, an allowed file type). Called once per
    /// mod, in the server's order. `version` is what its pack.txt says
    /// (empty if nothing).
    fn load_mod(&mut self, name: &str, version: &str, files: &[(String, Vec<u8>)]);
    /// Forget every mod loaded with [`ScriptHost::load_mod`] (the session ended).
    fn unload_all(&mut self);
}

static HOST: AtomicPtr<Box<dyn ScriptHost>> = AtomicPtr::new(core::ptr::null_mut());

/// Register the script host. Only the first call has an effect (the host
/// lives for the rest of the program).
pub fn set_host(host: Box<dyn ScriptHost>) {
    let p = Box::into_raw(Box::new(host));
    if HOST.compare_exchange(core::ptr::null_mut(), p, Ordering::AcqRel, Ordering::Acquire).is_err() {
        // Already one: drop the newcomer.
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Whether a script host is registered.
pub fn has_host() -> bool {
    !HOST.load(Ordering::Acquire).is_null()
}

fn with_host(f: impl FnOnce(&mut dyn ScriptHost)) {
    let p = HOST.load(Ordering::Acquire);
    if !p.is_null() {
        // Only the game's main thread calls in here (the host is not
        // shared), so there is no other reference to it.
        f(unsafe { &mut **p });
    }
}

/// Whether any of the files is a script.
pub fn has_scripts(files: &[(String, Vec<u8>)]) -> bool {
    files.iter().any(|(p, _)| p.to_ascii_lowercase().ends_with(".rs"))
}

/// Hand a downloaded mod to the script host (if there is one and the mod
/// has scripts).
pub fn load_mod(name: &str, version: &str, files: &[(String, Vec<u8>)]) {
    if has_scripts(files) {
        with_host(|h| h.load_mod(name, version, files));
    }
}

/// Tell the script host the downloaded mods are gone.
pub fn unload_all() {
    with_host(|h| h.unload_all());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_found_by_extension() {
        let f = |p: &str| (String::from(p), Vec::new());
        assert!(has_scripts(&[f("pack.txt"), f("scripts/A.RS")]));
        assert!(!has_scripts(&[f("pack.txt"), f("textures/a.png")]));
        // With no host registered, these do nothing.
        load_mod("x", "", &[f("a.rs")]);
        unload_all();
    }
}
