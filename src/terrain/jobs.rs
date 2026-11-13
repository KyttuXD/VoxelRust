//! Background work: generating chunks and building meshes.
//!
//! With one or more worker threads, jobs run in the background and results
//! are collected each frame. With zero threads the same jobs run on the
//! calling thread a few at a time, which is also how the tests run them.

use crate::chunk::ChunkData;
use crate::mesh::{self, MeshData};
use crate::sys::thread::{self, Mutex, Sem, Thread};
use crate::worldgen;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub enum Job {
    Gen { cx: i32, cz: i32 },
    /// The 3 x 3 neighbourhood, indexed `(dz + 1) * 3 + (dx + 1)`.
    Mesh { cx: i32, cz: i32, n: [Arc<ChunkData>; 9] },
    /// A distant-terrain tile, sampled every `step` blocks.
    Far { tx: i32, tz: i32, step: i32 },
    /// Slow work for a mod's own code (fetching something, say): `run`
    /// is called with `arg` on a worker, and what it returns turns up in
    /// `World::task_results` under the same `tag`.
    Task { tag: u32, arg: Vec<u8>, run: fn(&[u8]) -> Vec<u8> },
}

pub enum JobResult {
    Gen { cx: i32, cz: i32, data: ChunkData },
    Mesh { cx: i32, cz: i32, mesh: MeshData },
    Far { tx: i32, tz: i32, mesh: MeshData },
    Task { tag: u32, arg: Vec<u8>, out: Vec<u8> },
}

pub fn run_job(seed: u32, job: Job) -> JobResult {
    match job {
        Job::Gen { cx, cz } => JobResult::Gen { cx, cz, data: worldgen::generate(seed, cx, cz) },
        Job::Task { tag, arg, run } => {
            let out = run(&arg);
            JobResult::Task { tag, arg, out }
        }
        Job::Far { tx, tz, step } => JobResult::Far { tx, tz, mesh: crate::lod::build(seed, tx, tz, step) },
        Job::Mesh { cx, cz, n } => {
            let refs: [&ChunkData; 9] = [&n[0], &n[1], &n[2], &n[3], &n[4], &n[5], &n[6], &n[7], &n[8]];
            JobResult::Mesh { cx, cz, mesh: mesh::build(&refs) }
        }
    }
}

struct Inner {
    queue: VecDeque<Job>,
    results: Vec<JobResult>,
    quit: bool,
    /// Jobs being worked on right now.
    running: usize,
    /// While set, workers start nothing new.
    paused: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    wake: Sem,
    seed: u32,
}

pub struct Pool {
    shared: Option<Arc<Shared>>,
    threads: Vec<Thread>,
    local: VecDeque<Job>,
    seed: u32,
    /// Jobs submitted whose results have not been collected yet.
    pub in_flight: usize,
}

fn worker(shared: Arc<Shared>) {
    loop {
        shared.wake.wait();
        let job = {
            let mut g = shared.inner.lock();
            if g.quit {
                return;
            }
            let job = if g.paused { None } else { g.queue.pop_front() };
            if job.is_some() {
                g.running += 1;
            }
            job
        };
        if let Some(job) = job {
            let result = run_job(shared.seed, job);
            let mut g = shared.inner.lock();
            g.results.push(result);
            g.running -= 1;
        }
    }
}

impl Pool {
    /// `threads == 0` runs everything on the caller's thread and needs no
    /// SDL, so it works in tests.
    pub fn new(threads: usize, seed: u32) -> Pool {
        let mut pool = Pool { shared: None, threads: Vec::new(), local: VecDeque::new(), seed, in_flight: 0 };
        if threads == 0 {
            return pool;
        }
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner { queue: VecDeque::new(), results: Vec::new(), quit: false, running: 0, paused: false }),
            wake: Sem::new(),
            seed,
        });
        for _ in 0..threads {
            let s = shared.clone();
            if let Some(t) = thread::spawn(Box::new(move || worker(s))) {
                pool.threads.push(t);
            }
        }
        // If the system would not give us any thread, stay single-threaded.
        if !pool.threads.is_empty() {
            pool.shared = Some(shared);
        }
        pool
    }

    /// Stop workers from starting jobs and wait for the running ones, so
    /// the content registry can be swapped with nothing half-made.
    pub fn pause(&mut self) {
        if let Some(s) = &self.shared {
            s.inner.lock().paused = true;
            while s.inner.lock().running > 0 {
                crate::sys::sleep_ms(1);
            }
        }
    }

    /// Carry on after [`Pool::pause`]. Terrain that was generated but not
    /// yet collected is translated with `map` (old block state -> new).
    pub fn resume(&mut self, map: &[crate::block::Block]) {
        if let Some(s) = &self.shared {
            let waiting = {
                let mut g = s.inner.lock();
                g.paused = false;
                for r in g.results.iter_mut() {
                    if let JobResult::Gen { data, .. } = r {
                        for b in data.blocks.iter_mut() {
                            *b = map.get(*b as usize).copied().unwrap_or(0);
                        }
                    }
                }
                g.queue.len()
            };
            for _ in 0..waiting {
                s.wake.post();
            }
        }
    }

    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Queue a job; `urgent` jobs jump the queue.
    pub fn submit(&mut self, job: Job, urgent: bool) {
        self.in_flight += 1;
        match &self.shared {
            Some(s) => {
                {
                    let mut g = s.inner.lock();
                    if urgent {
                        g.queue.push_front(job);
                    } else {
                        g.queue.push_back(job);
                    }
                }
                s.wake.post();
            }
            None => {
                if urgent {
                    self.local.push_front(job);
                } else {
                    self.local.push_back(job);
                }
            }
        }
    }

    /// Collect finished work. Without threads, this is where up to
    /// `inline_budget` jobs actually run.
    pub fn drain(&mut self, inline_budget: usize) -> Vec<JobResult> {
        let results = match &self.shared {
            Some(s) => core::mem::take(&mut s.inner.lock().results),
            None => {
                let mut out = Vec::new();
                for _ in 0..inline_budget {
                    match self.local.pop_front() {
                        Some(job) => out.push(run_job(self.seed, job)),
                        None => break,
                    }
                }
                out
            }
        };
        self.in_flight -= results.len();
        results
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(s) = self.shared.take() {
            s.inner.lock().quit = true;
            for _ in 0..self.threads.len() {
                s.wake.post();
            }
            for t in self.threads.drain(..) {
                t.join();
            }
        }
    }
}
