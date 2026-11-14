//! The client's side of getting a server's mods: it reads the announcement,
//! decides what is needed, takes the bundles that arrive, checks and writes
//! them under `<data>/tmp/mods/<host_port>/` and hands scripts to the script
//! host. One of these lives in a multiplayer `Session`.

use super::announce::{plan, Cached};
use super::bundle;
use super::tmp;
use crate::content::mod_runtime;
use crate::proto::ModInfo;
use alloc::string::String;
use alloc::vec::Vec;

/// A mod taken from the server, for the settings list.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub name: String,
    pub version: String,
    /// It was already on disk from earlier in the session.
    pub cached: bool,
}

pub struct ClientSync {
    base: String,
    /// The folder of this server inside `tmp/mods/`.
    dir: String,
    /// The announcement, once it has arrived.
    announced: Vec<ModInfo>,
    listed: bool,
    /// Places in `announced` still to come, in the order asked for.
    waiting: Vec<usize>,
    /// The mod being received: its place and the bytes so far.
    current: Option<(usize, Vec<u8>)>,
    /// Mods settled so far (kept, received or given up on), and in all.
    done: usize,
    total: usize,
    /// Everything is settled.
    pub finished: bool,
    /// The mods in use from this server.
    pub taken: Vec<Taken>,
    /// What to write in the log (taken by the caller).
    pub log: Vec<String>,
    /// Manifest of the complete mods in the folder.
    manifest: Vec<Cached>,
}

impl ClientSync {
    /// For the server `host:port`; `base` is the data folder. Nothing is
    /// touched on disk until the announcement arrives.
    pub fn new(base: &str, host: &str, port: u16) -> ClientSync {
        let key = super::path::server_key(host, port);
        ClientSync { base: String::from(base), dir: tmp::server_dir(base, &key), announced: Vec::new(), listed: false, waiting: Vec::new(), current: None, done: 0, total: 0, finished: false, taken: Vec::new(), log: Vec::new(), manifest: Vec::new() }
    }

    /// `(settled, in all)` while mods are still arriving; `None` otherwise.
    /// What the loading screen shows as `DOWNLOADING MODS 3/12`.
    pub fn progress(&self) -> Option<(usize, usize)> {
        if self.finished || !self.listed || self.total == 0 {
            None
        } else {
            Some((self.done, self.total))
        }
    }

    /// The server's announcement. Returns the names to ask for (the caller
    /// sends `ModsWant`). Everything on disk for this server that is out of
    /// date is deleted, and what is up to date is loaded at once.
    pub fn on_list(&mut self, list: Vec<ModInfo>) -> Vec<String> {
        if self.listed {
            return Vec::new();
        }
        self.listed = true;
        // Whatever a crash left behind for other servers.
        let key = self.dir.trim_end_matches('/').rsplit('/').next().map(String::from).unwrap_or_default();
        tmp::clean_stale(&self.base, Some(&key));
        let cached = tmp::read_manifest(&self.dir);
        let p = plan(&list, &cached);
        for (name, why) in &p.skip {
            self.log.push(alloc::format!("mods from the server: {} not taken: {}", name, why));
        }
        // Delete what is out of date, and anything the manifest does not vouch for.
        let on_disk = crate::sys::list_dir(&self.dir);
        let mut stale = tmp::stale_mod_dirs(&on_disk, &p.keep);
        for n in &p.remove {
            if !stale.contains(n) {
                stale.push(n.clone());
            }
        }
        for n in &stale {
            if super::path::clean_mod_name(n).is_some() {
                tmp::remove_tree(&alloc::format!("{}{}", self.dir, n));
            }
        }
        self.manifest = cached.into_iter().filter(|c| p.keep.contains(&c.name)).collect();
        tmp::write_manifest(&self.dir, &self.manifest);
        self.total = p.keep.len() + p.fetch.len();
        for name in &p.keep {
            let files = tmp::read_tree(&alloc::format!("{}{}/", self.dir, name));
            let version = self.manifest.iter().find(|c| c.name == *name).map(|c| c.version.clone()).unwrap_or_default();
            self.log.push(alloc::format!("mods from the server: {} {} is already here ({} files)", name, version, files.len()));
            mod_runtime::load_mod(name, &version, &files);
            self.taken.push(Taken { name: name.clone(), version, cached: true });
            self.done += 1;
        }
        self.waiting = p.fetch.clone();
        let names: Vec<String> = p.fetch.iter().map(|&i| list[i].name.clone()).collect();
        self.announced = list;
        if self.waiting.is_empty() {
            self.finished = true;
        }
        names
    }

    /// A piece of a mod's bundle arrived.
    pub fn on_part(&mut self, index: u16, last: bool, data: &[u8]) {
        if !self.listed || self.finished {
            return;
        }
        let index = index as usize;
        // The server sends the mods in the order asked: only the next one is welcome.
        if self.waiting.first() != Some(&index) {
            return;
        }
        let want = self.announced[index].size as usize;
        let buf = &mut self.current.get_or_insert((index, Vec::new())).1;
        if buf.len() + data.len() > want {
            let name = self.announced[index].name.clone();
            self.log.push(alloc::format!("mods from the server: {} sent more than announced; left out", name));
            self.settle(index, false);
            return;
        }
        buf.extend_from_slice(data);
        if last {
            let bytes = self.current.take().map(|c| c.1).unwrap_or_default();
            self.receive(index, &bytes);
        }
    }

    /// A whole bundle is here: check it, write it, load it.
    fn receive(&mut self, index: usize, bytes: &[u8]) {
        let info = self.announced[index].clone();
        let verdict = if bytes.len() != info.size as usize || bundle::sum_of(bytes) != info.sum {
            Err("it did not match its checksum")
        } else {
            bundle::decode(bytes).map_err(|r| r.text())
        };
        match verdict {
            Ok(files) => {
                if tmp::write_mod(&self.dir, &info.name, &files) {
                    self.manifest.push(Cached { name: info.name.clone(), version: info.version.clone(), sum: info.sum });
                    tmp::write_manifest(&self.dir, &self.manifest);
                    self.log.push(alloc::format!("mods from the server: {} {} downloaded ({} files, {} bytes)", info.name, info.version, files.len(), bytes.len()));
                    mod_runtime::load_mod(&info.name, &info.version, &files);
                    self.taken.push(Taken { name: info.name, version: info.version, cached: false });
                    self.settle(index, true);
                } else {
                    self.log.push(alloc::format!("mods from the server: {} could not be written to disk", info.name));
                    tmp::remove_tree(&alloc::format!("{}{}", self.dir, info.name));
                    self.settle(index, false);
                }
            }
            Err(why) => {
                self.log.push(alloc::format!("mods from the server: {} refused: {}", info.name, why));
                self.settle(index, false);
            }
        }
    }

    /// One mod is dealt with, well or not.
    fn settle(&mut self, index: usize, _ok: bool) {
        self.current = None;
        self.waiting.retain(|&i| i != index);
        self.done += 1;
        if self.waiting.is_empty() {
            self.finished = true;
        }
    }

    /// The mods are no longer wanted (the player left): unload the scripts
    /// and delete this server's folder.
    pub fn end(&mut self) {
        mod_runtime::unload_all();
        tmp::clean_all(&self.base);
        self.taken.clear();
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_counts_mods_and_ends_with_the_last_one() {
        // No disk is involved when nothing is announced.
        let mut c = ClientSync::new("", "h", 1);
        assert_eq!(c.progress(), None);
        // Parts before the announcement are ignored.
        c.on_part(0, true, b"x");
        assert!(!c.finished);
    }
}
