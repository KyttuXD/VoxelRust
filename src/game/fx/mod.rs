//! Effects for mods: dropped items, explosions with choices, falling blocks,
//! particles, translucent quads and the look of a dimension's sky.
//!
//! The host calls scripts use are in `script/ext/fx.rs`; everything they
//! reach lives here, so that a mod's compiled code (`native-mods`) can call
//! the same functions and get the same limits:
//!
//! | file | subject |
//! | --- | --- |
//! | `blast.rs` | the flags of an explosion, the power limit, fires left behind |
//! | `drops.rs` | spawning a dropped item with a velocity (128 lie about, 1 to 64 in a stack) |
//! | `falling.rs` | blocks flying and falling: the physics and what happens on landing (64 at most) |
//! | `particles.rs` | the kinds, their looks, the budget (2000 live, fewer when the frame is slow) |
//! | `lava_embers.rs` | embers rising off lava near the player (40 a patch, 120 in all, none past 24 blocks) |
//! | `fog.rs` | the fog a mod holds over the dimension's, for the next world pass (0.5 s per call) |
//! | `soft.rs` | soft round particles with their own alpha and size growth (512 live, 256 a mod) |
//! | `quads.rs` | the registry of translucent world-space quads (256 a mod) |
//! | `sky.rs` | the sky of a dimension: the data from the pack plus what scripts changed |
//!
//! What is kept for a running game is [`FxState`], in `Session::ext`. It
//! lives and dies with the session, so a closed world takes it all along.
//! Particles are the session's sparks (`Primed::sparks`, see `primed.rs`):
//! there is one particle system, which explosions and scripts both use.

pub mod blast;
pub mod drops;
pub mod falling;
pub mod fog;
pub mod lava_embers;
pub mod particles;
pub mod quads;
pub mod sky;
pub mod soft;

use crate::session::Session;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// The key in `Session::ext`.
pub const KEY: &str = "fx";

/// What the session keeps for the effects.
#[derive(Default)]
pub struct FxState {
    pub falling: Vec<falling::Falling>,
    pub quads: quads::QuadSet,
    /// The fog a mod asked for, and the soft particles.
    pub fog: fog::FogHold,
    pub soft: soft::SoftSet,
    /// The skies scripts changed, by dimension name (`""` is the home world).
    pub skies: Vec<(String, crate::dim_sky::DimSky)>,
    /// Particles asked for in this frame, and how long the last frame took.
    pub spawned: u32,
    pub frame_dt: f32,
    /// Explosions and dropped items scripts made in this frame.
    pub blasts: u32,
    pub dropped: u32,
    /// Seconds since embers were last let up over lava (`lava_embers.rs`).
    pub ember_clock: f32,
}

/// The session's effects, made if there are none yet.
pub fn state(g: &mut Session) -> &mut FxState {
    let at = match g.ext.iter().position(|e| e.0 == KEY) {
        Some(i) => i,
        None => {
            g.ext.push((KEY, Box::new(FxState::default())));
            g.ext.len() - 1
        }
    };
    if g.ext[at].1.downcast_ref::<FxState>().is_none() {
        g.ext[at].1 = Box::new(FxState::default());
    }
    match g.ext[at].1.downcast_mut::<FxState>() {
        Some(s) => s,
        None => unreachable!("the entry was just made an FxState"),
    }
}

/// What is kept, if anything has been made yet.
pub fn state_ref(g: &Session) -> Option<&FxState> {
    g.ext_ref::<FxState>(KEY)
}

/// Whether the game may change the world here (a game of one's own, or the
/// host of a shared one). Everything that breaks, places or hurts asks.
pub fn may_change(g: &Session) -> bool {
    g.net.is_none() || g.runs_the_world()
}

/// The player is changing dimension: what flies in the old one is gone (a
/// flying block must not land in the new land). Quads stay: they are the mod's.
pub fn leave_dimension(g: &mut Session) {
    if let Some(s) = g.ext_mut::<FxState>(KEY) {
        s.falling.clear();
        s.soft.list.clear();
    }
    g.primed.sparks.clear();
}

/// Once a frame, from the session: move the falling blocks, tidy the quads
/// of mods that went away, start the counts of the frame afresh.
pub fn tick(g: &mut Session, dt: f32) {
    if g.ext.iter().all(|e| e.0 != KEY) {
        return;
    }
    {
        let s = state(g);
        s.spawned = 0;
        s.blasts = 0;
        s.dropped = 0;
        s.frame_dt = dt;
        s.fog.tick(dt);
        soft::tick(&mut s.soft, dt);
    }
    falling::tick(g, dt);
    quads::tidy(g);
}
