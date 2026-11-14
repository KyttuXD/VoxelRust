//! The server's half of land from the server (see `net/chunks/mod.rs`).
//!
//! Declared inside `net/server.rs` (`#[path]`) so that it reaches the server's
//! own fields. A server that makes land for its players (a world whose
//! terrain only the server's code can make, see [`Server::enable_chunk_stream`])
//! answers [`Msg::ChunkRequest`] from clients of protocol 15 or later with the
//! chunk's blocks, with the edits made so far on top; every other server, and
//! every other client, never sees these messages used.

use super::{Server, SEND_TIMEOUT};
use crate::block::Block;
use crate::net::chunks::cache::Pieces;
use crate::net::chunks::gen::{ChunkGen, Edits, GenFn, Taken};
use crate::net::chunks::limiter::{Verdict, LIMIT_BLOCKS};
use crate::proto::{Msg, CHUNKS_SERVED};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The edits of chunk (cx, cz) as the chunk's own coordinates (x, y, z, state).
pub(crate) fn edits_in(edits: &BTreeMap<(i32, i32, i32), u16>, cx: i32, cz: i32) -> Edits {
    let (x0, z0) = (cx.saturating_mul(16), cz.saturating_mul(16));
    // (Keys go by x first: one slab of the world, of which only this chunk's z are wanted.)
    edits
        .range((x0, i32::MIN, i32::MIN)..=(x0.saturating_add(15), i32::MAX, i32::MAX))
        .filter(|(k, _)| k.2 >= z0 && k.2 <= z0.saturating_add(15) && (0..crate::chunk::CH).contains(&k.1))
        .map(|(k, &b)| (k.0 - x0, k.1, k.2 - z0, b as Block))
        .collect()
}

impl Server {
    /// Make this server send land to the players who ask for it, if the world is one only
    /// this server can make (its kind is one a mod's code supplies, and that code is in
    /// this program and switched on). Uses the world generator on a worker thread.
    /// False (and nothing changes) for any other world.
    pub fn enable_chunk_stream(&mut self) -> bool {
        let kind = match crate::worldgen::WorldKind::parse(&self.cfg.kind) {
            Some(k @ crate::worldgen::WorldKind::Custom(_)) => k,
            _ => return false,
        };
        // The generator reads the process-wide terrain source: the server has no world of its own.
        crate::worldgen::set_kind(kind);
        if crate::worldgen::source().is_none() {
            return false;
        }
        self.use_chunk_source(crate::worldgen::generate, true);
        true
    }

    /// Serve land made by `gen` (the tests give their own). With `threaded` the chunks are
    /// made on a worker thread; otherwise a couple on each tick.
    pub fn use_chunk_source(&mut self, gen: GenFn, threaded: bool) {
        self.chunkgen = Some(ChunkGen::new(self.cfg.seed, gen, threaded));
    }

    /// Whether this server sends land when asked.
    pub fn serves_chunks(&self) -> bool {
        self.chunkgen.is_some()
    }

    /// What clients are told after the mod list.
    pub(super) fn chunk_caps(&self) -> Msg {
        Msg::ChunkCaps { flags: if self.chunkgen.is_some() { CHUNKS_SERVED } else { 0 }, limit: LIMIT_BLOCKS as u32 }
    }

    /// Connection `i` (joined, speaking protocol 15 or later) asks for chunk (cx, cz).
    pub(super) fn on_chunk_request(&mut self, i: usize, cx: i32, cz: i32, log: &mut Vec<String>) {
        // A server that sends no land says so in its caps; a client that asks anyway is ignored.
        if self.chunkgen.is_none() {
            return;
        }
        match self.conns[i].chunks.ask(cx, cz) {
            Verdict::Accept => {}
            Verdict::Duplicate | Verdict::Refuse | Verdict::Invalid => {
                // Ignored; one that keeps on far beyond what an honest client does is cut.
                if self.conns[i].chunks.abusive() {
                    log.push(alloc::format!("{} was asking for far too much land and was dropped", self.conns[i].name));
                    self.conns[i].gone = true;
                }
                return;
            }
        }
        let id = self.conns[i].id;
        let Some(gen) = self.chunkgen.as_mut() else { return };
        let edits = &self.edits;
        match gen.request(id, cx, cz, || edits_in(edits, cx, cz)) {
            Taken::Hit(pieces) => self.send_chunk(i, cx, cz, &pieces),
            Taken::Queued => {}
            // Too much waiting: this one is dropped, and the client asks again after a while.
            Taken::Full => self.conns[i].chunks.done(cx, cz),
        }
    }

    /// Send a chunk's pieces to connection `i` and free its place in their allowance.
    fn send_chunk(&mut self, i: usize, cx: i32, cz: i32, pieces: &Pieces) {
        let parts = pieces.len().min(255) as u8;
        let mut bytes = Vec::new();
        for (k, p) in pieces.iter().enumerate().take(parts as usize) {
            bytes.extend(Msg::ChunkData { cx, cz, part: k as u8, parts, data: p.clone() }.encode());
        }
        let c = &mut self.conns[i];
        c.chunks.done(cx, cz);
        if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
            c.gone = true;
        }
    }

    /// Send what the chunk maker has finished.
    pub(super) fn chunk_tick(&mut self) {
        let ready = match self.chunkgen.as_mut() {
            Some(g) => g.poll(),
            None => return,
        };
        for r in ready {
            let Some(k) = self.conns.iter().position(|c| c.id == r.to && c.joined && !c.gone) else { continue };
            match &r.pieces {
                Some(p) => self.send_chunk(k, r.cx, r.cz, p),
                None => self.conns[k].chunks.done(r.cx, r.cz),
            }
        }
    }

    /// A player changed the block at (x, z): a chunk that was kept is out of date.
    pub(super) fn chunk_edited(&mut self, x: i32, z: i32) {
        if let Some(g) = self.chunkgen.as_mut() {
            g.edited(x >> 4, z >> 4);
        }
    }
}
