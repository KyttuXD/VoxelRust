//! The sandbox's limits: what a script call may spend before the engine
//! stops it with an error instead of letting it hang the game or eat its
//! memory.

/// What one call into a script (a hook, `on_tick`, a command) may use.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Evaluation steps: one for each expression node evaluated and one for
    /// each loop iteration. A call that spends them all stops with an error.
    pub max_steps: u64,
    /// How deep functions may call one another (recursion).
    pub max_depth: usize,
    /// How deep expressions may nest while evaluating, over all calls.
    pub max_nesting: usize,
    /// How many "cells" a call may allocate in all (one for each 16 bytes of
    /// string, each Vec element, struct, tuple and closure). The count is of
    /// what was made, not of what is alive, so it also bounds the memory a
    /// call can reach.
    pub max_cells: usize,
    /// The longest a string may grow, in bytes.
    pub max_string: usize,
    /// The most elements a Vec may hold.
    pub max_vec: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { max_steps: 200_000, max_depth: 40, max_nesting: 500, max_cells: 400_000, max_string: 100_000, max_vec: 50_000 };
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// Source files bigger than this are refused (bytes).
pub const MAX_SOURCE: usize = 256 * 1024;

/// How deep the parser lets expressions, blocks and patterns nest.
pub const MAX_PARSE_DEPTH: usize = 64;
