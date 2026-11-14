//! Translucent quads in the world (`quad_create`, `quad_move`, `quad_remove`):
//! storm clouds, beams and auras a mod keeps on show.
//!
//! A quad belongs to the mod that made it and has a number of that mod's
//! choosing. It stays until the mod removes it, switches off, or the world
//! closes. Each mod may keep [`MAX_PER_MOD`] and all mods together
//! [`MAX_TOTAL`]; its corners are four places in the world (in order round
//! the quad), and it is drawn with the picture named, tinted by the colour
//! and opacity, either laid over the world (`alpha`) or added to it
//! (`additive`), tested against the depth buffer, farthest first
//! (`client/render/fx.rs`).
//!
//! The picture is `block:<id>` (the top picture of a block of the atlas), the
//! name of a picture a mod brought (`picture <name> file ...`), or empty for a
//! plain colour. A name that is not found draws a plain colour.

use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Most quads one mod keeps.
pub const MAX_PER_MOD: usize = 256;
/// Most quads of all the mods together.
pub const MAX_TOTAL: usize = 1024;
/// Longest picture name.
pub const MAX_NAME: usize = 48;
/// The largest a quad may be: no corner more than this far from the first.
pub const MAX_SIZE: f64 = 512.0;
/// As far from the middle of the world as a corner may be.
pub const EDGE: f64 = 3.0e7;
/// Farthest from the camera a quad is drawn.
pub const DRAW_RANGE: f64 = 192.0;

/// How a quad joins the picture behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// Laid over it by its opacity.
    Alpha,
    /// Added to it (glows).
    Additive,
}

impl Blend {
    pub fn from_name(s: &str) -> Option<Blend> {
        match s.to_ascii_lowercase().as_str() {
            "alpha" | "normal" | "blend" | "" => Some(Blend::Alpha),
            "additive" | "add" | "glow" => Some(Blend::Additive),
            _ => None,
        }
    }
}

/// A quad on show.
#[derive(Clone, Debug, PartialEq)]
pub struct FxQuad {
    /// The mod that made it.
    pub owner: String,
    pub id: i64,
    /// `block:<id>`, a picture's name, or empty.
    pub tex: String,
    pub corners: [[f64; 3]; 4],
    pub rgba: [u8; 4],
    pub blend: Blend,
}

/// Why a quad could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuadError {
    /// The mod has [`MAX_PER_MOD`] already.
    ModFull,
    /// All the mods together have [`MAX_TOTAL`].
    AllFull,
}

/// The quads on show.
#[derive(Default)]
pub struct QuadSet {
    pub list: Vec<FxQuad>,
}

/// Whether the four corners make a quad the game will draw: finite, inside the
/// world's edge, and none more than [`MAX_SIZE`] from the first.
pub fn corners_ok(c: &[[f64; 3]; 4]) -> bool {
    for p in c {
        if !p.iter().all(|v| v.is_finite()) || p[0] < -EDGE || p[0] > EDGE || p[2] < -EDGE || p[2] > EDGE {
            return false;
        }
    }
    for p in &c[1..] {
        for a in 0..3 {
            let d = p[a] - c[0][a];
            if d > MAX_SIZE || d < -MAX_SIZE {
                return false;
            }
        }
    }
    true
}

/// Whether a picture name is acceptable: short, with letters, digits and
/// `_ - : .` only.
pub fn name_ok(name: &str) -> bool {
    name.len() <= MAX_NAME && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.')
}

impl QuadSet {
    fn find(&self, owner: &str, id: i64) -> Option<usize> {
        self.list.iter().position(|q| q.id == id && q.owner == owner)
    }

    /// How many quads a mod keeps.
    pub fn count_of(&self, owner: &str) -> usize {
        self.list.iter().filter(|q| q.owner == owner).count()
    }

    /// Make a quad, or replace the mod's quad of that number. Returns whether
    /// it replaced one.
    pub fn create(&mut self, q: FxQuad) -> Result<bool, QuadError> {
        if let Some(i) = self.find(&q.owner, q.id) {
            self.list[i] = q;
            return Ok(true);
        }
        if self.count_of(&q.owner) >= MAX_PER_MOD {
            return Err(QuadError::ModFull);
        }
        if self.list.len() >= MAX_TOTAL {
            return Err(QuadError::AllFull);
        }
        self.list.push(q);
        Ok(false)
    }

    /// Move a quad (give it new corners); false if the mod has none of that number.
    pub fn move_to(&mut self, owner: &str, id: i64, corners: [[f64; 3]; 4]) -> bool {
        match self.find(owner, id) {
            Some(i) => {
                self.list[i].corners = corners;
                true
            }
            None => false,
        }
    }

    /// Change a quad's colour and opacity.
    pub fn tint(&mut self, owner: &str, id: i64, rgba: [u8; 4]) -> bool {
        match self.find(owner, id) {
            Some(i) => {
                self.list[i].rgba = rgba;
                true
            }
            None => false,
        }
    }

    /// Take a quad away; false if the mod has none of that number.
    pub fn remove(&mut self, owner: &str, id: i64) -> bool {
        match self.find(owner, id) {
            Some(i) => {
                self.list.remove(i);
                true
            }
            None => false,
        }
    }

    /// Take all of a mod's quads away; how many went.
    pub fn remove_mod(&mut self, owner: &str) -> usize {
        let before = self.list.len();
        self.list.retain(|q| q.owner != owner);
        before - self.list.len()
    }

    /// Keep only the quads of mods `keep` says are still there; how many went.
    pub fn retain_mods(&mut self, keep: &dyn Fn(&str) -> bool) -> usize {
        let before = self.list.len();
        self.list.retain(|q| keep(&q.owner));
        before - self.list.len()
    }
}

/// Once a frame: take away the quads of mods whose scripts are no longer
/// running (switched off, or stopped by an error).
pub fn tidy(g: &mut Session) {
    let any = super::state_ref(g).map_or(false, |s| !s.quads.list.is_empty());
    if !any {
        return;
    }
    let working: Option<Vec<String>> = crate::script::with_host(|h| h.mods.iter().filter(|m| m.scripts.iter().any(|s| s.disabled.is_none())).map(|m| m.name.clone()).collect());
    // The host busy (we are inside a script): look again next frame.
    let Some(working) = working else { return };
    // (A mod's compiled code looks after its own quads: those of `native::all()` stay.)
    super::state(g).quads.retain_mods(&|owner| working.iter().any(|w| w == owner) || crate::native::all().iter().any(|n| n.name == owner));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(owner: &str, id: i64) -> FxQuad {
        FxQuad { owner: String::from(owner), id, tex: String::new(), corners: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], rgba: [255, 255, 255, 128], blend: Blend::Alpha }
    }

    #[test]
    fn create_move_tint_remove() {
        let mut s = QuadSet::default();
        assert_eq!(s.create(quad("a", 1)), Ok(false));
        assert_eq!(s.list.len(), 1);
        let c = [[0.0, 5.0, 0.0], [2.0, 5.0, 0.0], [2.0, 5.0, 2.0], [0.0, 5.0, 2.0]];
        assert!(s.move_to("a", 1, c));
        assert_eq!(s.list[0].corners, c);
        assert!(s.tint("a", 1, [1, 2, 3, 4]));
        assert_eq!(s.list[0].rgba, [1, 2, 3, 4]);
        assert!(!s.move_to("a", 2, c));
        assert!(!s.move_to("b", 1, c));
        assert!(!s.tint("b", 1, [0; 4]));
        assert!(!s.remove("b", 1));
        assert!(s.remove("a", 1));
        assert!(s.list.is_empty());
        assert!(!s.remove("a", 1));
    }

    #[test]
    fn making_the_same_number_again_replaces_it() {
        let mut s = QuadSet::default();
        assert_eq!(s.create(quad("a", 7)), Ok(false));
        let mut q = quad("a", 7);
        q.rgba = [9, 9, 9, 9];
        assert_eq!(s.create(q), Ok(true));
        assert_eq!(s.list.len(), 1);
        assert_eq!(s.list[0].rgba, [9, 9, 9, 9]);
        // Two mods may use the same number.
        assert_eq!(s.create(quad("b", 7)), Ok(false));
        assert_eq!(s.list.len(), 2);
    }

    #[test]
    fn a_mod_keeps_at_most_256() {
        let mut s = QuadSet::default();
        for i in 0..MAX_PER_MOD as i64 {
            assert_eq!(s.create(quad("a", i)), Ok(false));
        }
        assert_eq!(s.create(quad("a", 9999)), Err(QuadError::ModFull));
        // Replacing one of its own is still fine, and another mod is not held back.
        assert_eq!(s.create(quad("a", 3)), Ok(true));
        assert_eq!(s.create(quad("b", 1)), Ok(false));
        assert_eq!(s.count_of("a"), MAX_PER_MOD);
        assert_eq!(s.count_of("b"), 1);
        // Taking one away makes room.
        assert!(s.remove("a", 0));
        assert_eq!(s.create(quad("a", 9999)), Ok(false));
    }

    #[test]
    fn all_the_mods_together_have_a_limit_too() {
        let mut s = QuadSet::default();
        let mut n = 0;
        'fill: for m in 0..8 {
            let name = alloc::format!("mod{}", m);
            for i in 0..MAX_PER_MOD as i64 {
                match s.create(quad(&name, i)) {
                    Ok(_) => n += 1,
                    Err(QuadError::AllFull) => break 'fill,
                    Err(QuadError::ModFull) => panic!("the mod's limit came first"),
                }
            }
        }
        assert_eq!(n, MAX_TOTAL);
        assert_eq!(s.create(quad("late", 1)), Err(QuadError::AllFull));
    }

    #[test]
    fn a_mod_switched_off_takes_its_quads_with_it() {
        let mut s = QuadSet::default();
        for i in 0..5 {
            s.create(quad("a", i)).unwrap();
            s.create(quad("b", i)).unwrap();
        }
        assert_eq!(s.remove_mod("a"), 5);
        assert_eq!(s.count_of("a"), 0);
        assert_eq!(s.count_of("b"), 5);
        assert_eq!(s.remove_mod("a"), 0);
        assert_eq!(s.retain_mods(&|m| m == "nobody"), 5);
        assert!(s.list.is_empty());
    }

    #[test]
    fn corners_must_make_sense() {
        let good = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]];
        assert!(corners_ok(&good));
        let mut bad = good;
        bad[2][1] = f64::NAN;
        assert!(!corners_ok(&bad));
        let mut bad = good;
        bad[1][0] = f64::INFINITY;
        assert!(!corners_ok(&bad));
        let mut far = good;
        far[1][0] = MAX_SIZE + 1.0;
        assert!(!corners_ok(&far));
        let mut edge = good;
        for p in edge.iter_mut() {
            p[0] += 4.0e7;
        }
        assert!(!corners_ok(&edge));
        let mut big = good;
        big[3][1] = -MAX_SIZE - 1.0;
        assert!(!corners_ok(&big));
        let mut max = good;
        max[1][0] = MAX_SIZE;
        assert!(corners_ok(&max));
    }

    #[test]
    fn names_and_blends() {
        assert!(name_ok(""));
        assert!(name_ok("block:stone"));
        assert!(name_ok("storm_cloud-2.png"));
        assert!(!name_ok("../etc"));
        assert!(!name_ok("a b"));
        assert!(!name_ok(&"x".repeat(MAX_NAME + 1)));
        assert!(name_ok(&"x".repeat(MAX_NAME)));
        assert_eq!(Blend::from_name("alpha"), Some(Blend::Alpha));
        assert_eq!(Blend::from_name("ADDITIVE"), Some(Blend::Additive));
        assert_eq!(Blend::from_name("glow"), Some(Blend::Additive));
        assert_eq!(Blend::from_name("multiply"), None);
    }
}
