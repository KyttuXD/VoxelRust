//! How jointed models stand, sit, lie, swim, climb and emote.
//!
//! A [`Body`] says what a model is doing (a posture, some activities, an
//! emote); `model::ModelDef::quads` asks [`apply`] for the extra rotation of
//! every bone, and [`root`] for how the whole body is turned and lowered
//! (lying down, swimming, sitting). Bones are recognised by name, so the
//! player model (Pelvis, Belly, Chest, Head, L-Arm, L-Forearm, L-Hand,
//! L-Thigh, L-Calf, L-Foot...) and the creature humanoids (Body, Arm-L,
//! Forearm-L, Leg-L, Shin-L...) are both posed; animals are left alone.
//!
//! Rotation conventions (degrees, as `model.rs` applies them): a positive x
//! turn swings the lower end of a limb forward and raises the face of a head;
//! a positive z turn swings a hanging limb toward +x (out, for the right
//! arm); a positive y turn turns the front toward the model's left. Sides
//! are +1 for the left limbs and -1 for the right ones.

pub mod activity;
pub mod bends;
pub mod climb;
pub mod crouch;
pub mod emote;
pub mod fall;
pub mod ride;
pub mod sit;
pub mod sleep;
pub mod swim;

/// The posture of a body. The numbers are what multiplayer sends.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Kind {
    #[default]
    Stand,
    Crouch,
    Sit,
    Sleep,
    Swim,
    Climb,
    Fall,
    Ride,
}

pub const KINDS: [Kind; 8] = [Kind::Stand, Kind::Crouch, Kind::Sit, Kind::Sleep, Kind::Swim, Kind::Climb, Kind::Fall, Kind::Ride];

impl Kind {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Kind {
        KINDS.get(c as usize).copied().unwrap_or(Kind::Stand)
    }

    /// Whether the posture has its own motion instead of the walk cycle.
    pub fn still(self) -> bool {
        !matches!(self, Kind::Stand | Kind::Crouch)
    }
}

/// Things done on top of a posture (bits of [`Body::flags`]).
pub const BOW: u8 = 1;
pub const EAT: u8 = 2;
pub const SWING: u8 = 4;
pub const CARRY: u8 = 8;

/// The pose as one byte for multiplayer: the posture in the low four bits,
/// the activities above.
pub fn pack(kind: Kind, flags: u8) -> u8 {
    kind.code() | (flags & 15) << 4
}

pub fn unpack(b: u8) -> (Kind, u8) {
    (Kind::from_code(b & 15), b >> 4)
}

/// What a body is doing, for [`apply`].
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub kind: Kind,
    /// The posture it is leaving, and how far the change has come
    /// (0 = still `from`, 1 = `kind`).
    pub from: Kind,
    pub mix: f32,
    pub flags: u8,
    /// How far through a blow (0..1) and how far a bow is drawn (0..1).
    pub swing: f32,
    pub draw: f32,
    /// The emote being played, as its place in the registry plus one (0 = none), and for how long.
    pub emote: u16,
    pub emote_t: f32,
}

impl Default for Body {
    fn default() -> Body {
        Body::steady(Kind::Stand)
    }
}

impl Body {
    pub fn steady(kind: Kind) -> Body {
        Body { kind, from: kind, mix: 1.0, flags: 0, swing: 0.0, draw: 0.0, emote: 0, emote_t: 0.0 }
    }

    /// How much of `k` the body is in at the moment (0..1).
    pub fn weight(&self, k: Kind) -> f32 {
        let mut w = 0.0;
        if self.kind == k {
            w += self.mix;
        }
        if self.from == k {
            w += 1.0 - self.mix;
        }
        w
    }

    /// How much of the walk cycle is left (the still postures replace it).
    pub fn walk_weight(&self) -> f32 {
        let mut w = 1.0;
        for k in KINDS {
            if k.still() {
                w -= self.weight(k);
            }
        }
        w.clamp(0.0, 1.0)
    }
}

/// Eases a body from one posture into the next, so a change takes a
/// fraction of a second instead of snapping.
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    from: Kind,
    to: Kind,
    mix: f32,
}

impl Default for Smoother {
    fn default() -> Smoother {
        Smoother { from: Kind::Stand, to: Kind::Stand, mix: 1.0 }
    }
}

impl Smoother {
    pub fn update(&mut self, target: Kind, dt: f32) {
        if target != self.to {
            if target == self.from && self.mix < 1.0 {
                // Back to where it was coming from: run the change in reverse.
                core::mem::swap(&mut self.from, &mut self.to);
                self.mix = 1.0 - self.mix;
            } else {
                self.from = self.to;
                self.to = target;
                self.mix = 0.0;
            }
        }
        self.mix = (self.mix + dt.clamp(0.0, 0.25) * 5.0).min(1.0);
    }

    pub fn target(&self) -> Kind {
        self.to
    }

    pub fn body(&self) -> Body {
        Body { kind: self.to, from: self.from, mix: ease(self.mix), ..Body::default() }
    }

    /// How much of `k` there is now (0..1).
    pub fn weight(&self, k: Kind) -> f32 {
        self.body().weight(k)
    }
}

/// Smooth 0..1 -> 0..1.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What decides a posture.
#[derive(Clone, Copy, Default, Debug)]
pub struct State {
    pub sleeping: bool,
    pub sitting: bool,
    /// On a creature's back or in a vehicle's seat.
    pub riding: bool,
    pub climbing: bool,
    pub in_water: bool,
    pub on_ground: bool,
    pub flying: bool,
    /// Blocks a second: sideways speed, and vertical speed (down is negative).
    pub speed: f32,
    pub vy: f32,
    pub crouching: bool,
}

/// Swimming needs some speed in the water; a drop this fast is a fall.
pub const SWIM_SPEED: f32 = 0.5;
pub const FALL_SPEED: f32 = -9.0;

/// The posture that goes with a state. The more particular wins.
pub fn select(s: &State) -> Kind {
    if s.sleeping {
        Kind::Sleep
    } else if s.sitting {
        Kind::Sit
    } else if s.riding {
        Kind::Ride
    } else if s.climbing {
        Kind::Climb
    } else if s.in_water && !s.flying {
        if s.speed > SWIM_SPEED {
            Kind::Swim
        } else {
            Kind::Stand
        }
    } else if !s.on_ground && !s.flying && s.vy < FALL_SPEED {
        Kind::Fall
    } else if s.crouching && s.on_ground && !s.flying {
        Kind::Crouch
    } else {
        Kind::Stand
    }
}

/// The part of the body a bone is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Other,
    Pelvis,
    Belly,
    Chest,
    /// The one piece of a creature's body.
    Torso,
    Neck,
    Head,
    Arm(i8),
    Forearm(i8),
    Hand(i8),
    Thigh(i8),
    Calf(i8),
    Foot(i8),
}

fn side_of(n: &str) -> i8 {
    let right = n.starts_with("r-") || n.starts_with("r_") || n.starts_with("right") || n.ends_with("-r") || n.ends_with("_r") || n.ends_with("right");
    if right {
        -1
    } else {
        1
    }
}

/// What a bone is, by its name (case does not matter).
pub fn classify(name: &str) -> Role {
    let n = name.to_ascii_lowercase();
    let side = side_of(&n);
    if n.contains("forearm") {
        Role::Forearm(side)
    } else if n.contains("hand") {
        Role::Hand(side)
    } else if n.contains("arm") {
        Role::Arm(side)
    } else if n.contains("thigh") {
        Role::Thigh(side)
    } else if n.contains("calf") || n.contains("shin") {
        Role::Calf(side)
    } else if n.contains("foot") {
        Role::Foot(side)
    } else if n.contains("leg") && !n.contains("lower") {
        Role::Thigh(side)
    } else if n.contains("pelvis") {
        Role::Pelvis
    } else if n.contains("belly") || n.contains("waist") {
        Role::Belly
    } else if n.contains("chest") {
        Role::Chest
    } else if n == "body" || n == "torso" {
        Role::Torso
    } else if n.contains("neck") {
        Role::Neck
    } else if n == "head" {
        Role::Head
    } else {
        Role::Other
    }
}

/// What a model is built like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    /// Has arms and legs it can pose; animals are left to their own walk.
    pub humanoid: bool,
    /// Height of the hip joint above the feet, and of the whole model, in model units.
    pub hip: f32,
    pub height: f32,
}

/// `hips` are the heights (above the model's feet) of the pivots of the thighs.
pub fn rig_of(roles: &[Role], hips: &[f32], height: f32) -> Rig {
    let arms = roles.iter().any(|r| matches!(r, Role::Arm(_)));
    let legs = roles.iter().any(|r| matches!(r, Role::Thigh(_)));
    let hip = hips.iter().copied().fold(f32::MAX, f32::min);
    Rig { humanoid: arms && legs, hip: if hip.is_finite() && hip < height && hip > 0.0 { hip } else { height * 0.4 }, height }
}

/// How a walking body moves its joints: the walk cycle and the idle sway.
#[derive(Clone, Copy, Default, Debug)]
pub struct Walk {
    pub phase: f32,
    /// 0 standing still .. 1 full stride.
    pub stride: f32,
    pub clock: f32,
}

pub(crate) fn add(a: &mut [f32; 3], b: [f32; 3], k: f32) {
    for i in 0..3 {
        a[i] += b[i] * k;
    }
}

/// The offsets one posture gives a bone.
fn posture(kind: Kind, role: Role, handled: bool, w: &Walk) -> [f32; 3] {
    let mut o = match kind {
        Kind::Stand => [0.0; 3],
        Kind::Crouch => crouch::offsets(role),
        Kind::Sit => sit::offsets(role),
        Kind::Sleep => sleep::offsets(role, w.clock),
        Kind::Swim => swim::offsets(role, w),
        Kind::Climb => climb::offsets(role, w),
        Kind::Fall => fall::offsets(role, w.clock),
        Kind::Ride => ride::offsets(role),
    };
    if !kind.still() {
        add(&mut o, bends::walk(role, handled, w), 1.0);
    }
    o
}

/// The extra rotation of a bone for what the body is doing, in degrees,
/// added to the bone's own. `handled` says the walk cycle already swings
/// this bone (see `model::Swing`). Bones that are not part of a humanoid
/// body are left alone.
pub fn apply(role: Role, handled: bool, b: &Body, w: &Walk, rot: &mut [f32; 3]) {
    if role == Role::Other {
        return;
    }
    let mut o = [0.0f32; 3];
    // Walking is damped by the standing still postures.
    let walk = Walk { stride: w.stride * b.walk_weight(), ..*w };
    if b.mix < 1.0 {
        add(&mut o, posture(b.from, role, handled, &walk), 1.0 - b.mix);
    }
    add(&mut o, posture(b.kind, role, handled, &walk), if b.mix < 1.0 { b.mix } else { 1.0 });
    activity::add(role, b, &walk, &mut o);
    for i in 0..3 {
        rot[i] += o[i];
    }
}

/// How the whole body is turned and moved for a posture, in model units:
/// a turn about the hip (degrees, about x) and a shift.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Root {
    pub turn: f32,
    pub shift: [f32; 3],
}

fn root_of(kind: Kind, rig: &Rig) -> Root {
    match kind {
        Kind::Crouch => crouch::root(rig),
        Kind::Sit => sit::root(rig),
        Kind::Sleep => sleep::root(rig),
        Kind::Swim => swim::root(rig),
        Kind::Ride => ride::root(rig),
        _ => Root::default(),
    }
}

/// The turn and shift of the body, blended between postures.
pub fn root(b: &Body, rig: &Rig) -> Root {
    let (from, to) = (root_of(b.from, rig), root_of(b.kind, rig));
    let k = b.mix.clamp(0.0, 1.0);
    Root { turn: from.turn + (to.turn - from.turn) * k, shift: [0, 1, 2].map(|i| from.shift[i] + (to.shift[i] - from.shift[i]) * k) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posture_follows_the_state() {
        let on_foot = State { on_ground: true, ..Default::default() };
        assert_eq!(select(&on_foot), Kind::Stand);
        assert_eq!(select(&State { crouching: true, ..on_foot }), Kind::Crouch);
        assert_eq!(select(&State { sitting: true, ..on_foot }), Kind::Sit);
        assert_eq!(select(&State { sleeping: true, sitting: true, ..on_foot }), Kind::Sleep);
        assert_eq!(select(&State { riding: true, ..on_foot }), Kind::Ride);
        assert_eq!(select(&State { climbing: true, crouching: true, ..on_foot }), Kind::Climb);
        let wet = State { in_water: true, speed: 2.0, ..Default::default() };
        assert_eq!(select(&wet), Kind::Swim);
        assert_eq!(select(&State { speed: 0.1, ..wet }), Kind::Stand);
        let falling = State { vy: -14.0, ..Default::default() };
        assert_eq!(select(&falling), Kind::Fall);
        assert_eq!(select(&State { flying: true, ..falling }), Kind::Stand);
        assert_eq!(select(&State { vy: -2.0, ..Default::default() }), Kind::Stand);
    }

    #[test]
    fn pose_bytes_round_trip() {
        for k in KINDS {
            for flags in [0, BOW, EAT | SWING, 15] {
                assert_eq!(unpack(pack(k, flags)), (k, flags));
            }
        }
        // A byte from a newer game is a standing body, not a crash.
        assert_eq!(unpack(0x0F).0, Kind::Stand);
    }

    #[test]
    fn the_smoother_blends_and_can_turn_back() {
        let mut s = Smoother::default();
        s.update(Kind::Sit, 0.0);
        assert_eq!(s.body().mix, 0.0);
        s.update(Kind::Sit, 0.1);
        let mid = s.body();
        assert!(mid.mix > 0.0 && mid.mix < 1.0 && mid.from == Kind::Stand && mid.kind == Kind::Sit);
        s.update(Kind::Stand, 0.0);
        assert_eq!((s.body().kind, s.body().from), (Kind::Stand, Kind::Sit));
        for _ in 0..20 {
            s.update(Kind::Stand, 0.1);
        }
        assert_eq!(s.body().weight(Kind::Stand), 1.0);
        assert_eq!(s.weight(Kind::Sit), 0.0);
    }

    #[test]
    fn bones_are_recognised_by_name() {
        assert_eq!(classify("L-Forearm"), Role::Forearm(1));
        assert_eq!(classify("R-Hand"), Role::Hand(-1));
        assert_eq!(classify("R-Arm"), Role::Arm(-1));
        assert_eq!(classify("Arm-L"), Role::Arm(1));
        assert_eq!(classify("L-Thigh"), Role::Thigh(1));
        assert_eq!(classify("R-Calf"), Role::Calf(-1));
        assert_eq!(classify("Shin-R"), Role::Calf(-1));
        assert_eq!(classify("Leg-L"), Role::Thigh(1));
        assert_eq!(classify("L-Foot"), Role::Foot(1));
        assert_eq!(classify("Pelvis"), Role::Pelvis);
        assert_eq!(classify("Body"), Role::Torso);
        assert_eq!(classify("Head"), Role::Head);
        assert_eq!(classify("tail-a"), Role::Other);
        assert_eq!(classify("ear-l"), Role::Other);
    }

    #[test]
    fn walking_is_replaced_by_the_still_postures() {
        assert_eq!(Body::steady(Kind::Stand).walk_weight(), 1.0);
        assert_eq!(Body::steady(Kind::Crouch).walk_weight(), 1.0);
        assert_eq!(Body::steady(Kind::Sit).walk_weight(), 0.0);
        let half = Body { kind: Kind::Sit, from: Kind::Stand, mix: 0.25, ..Body::default() };
        assert!((half.walk_weight() - 0.75).abs() < 1e-6);
    }
}
