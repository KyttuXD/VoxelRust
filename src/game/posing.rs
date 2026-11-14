//! What the player's own body is doing: its posture (sitting, lying,
//! swimming, climbing...), the emote being played, and the one byte of it
//! that other players are told. The shapes themselves are in content/pose.

use crate::content::pose::{self, Body, Kind, Smoother, State};

/// Where a sleeper lies: the middle of the bed's top, and the way the body faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lying {
    pub middle: [f64; 3],
    pub yaw: f32,
}

#[derive(Default)]
pub struct Posing {
    pub smooth: Smoother,
    /// The emote being played (its place in the registry) and for how long.
    pub emote: Option<(usize, f32)>,
    /// The emote other players were last told of (0 = none), so a change is sent once.
    pub told: u16,
    pub lying: Option<Lying>,
    /// Set by the session each frame: holding on to something climbable.
    pub climbing: bool,
    /// Carrying a load in both arms (for mods).
    pub carrying: bool,
    /// The stroke of swimming and climbing, which only runs while moving.
    pub stroke: f32,
    /// The walking cycle: advances with the distance covered, so the feet
    /// keep up with the ground whatever the speed.
    pub gait: f32,
    health: i32,
    /// How much of a run the body is in (0..1, eased; see `pose::gait`).
    pub run: f32,
    /// A gait held for a still (`--show bends:NAME`): phase, stride and run.
    pub shown_gait: Option<(f32, f32, f32)>,
    /// An arm swing held for a still (`--show swing:F`, F in 0..1 through the punch).
    pub shown_swing: Option<f32>,
    /// For scripted screenshots (`--show pose:NAME`, `--show emote:ID@SECONDS`): a posture
    /// held whatever the player does, and an emote kept at the time it was started at.
    pub forced: Option<Kind>,
    pub frozen: bool,
}

/// What the session knows, for [`Posing::update`].
pub struct Inputs {
    pub dt: f32,
    pub clock: f32,
    pub sleeping: bool,
    pub sitting: bool,
    pub riding: bool,
    pub in_water: bool,
    pub on_ground: bool,
    pub flying: bool,
    pub crouching: bool,
    pub vel: [f32; 3],
    /// Walking, jumping or crouching keys held: they end an emote.
    pub moving: bool,
    pub health: i32,
    pub dead: bool,
}

impl Posing {
    pub fn update(&mut self, i: &Inputs) {
        let speed = crate::math::sqrtf(i.vel[0] * i.vel[0] + i.vel[2] * i.vel[2]);
        let kind = self.forced.unwrap_or_else(|| pose::select(&State {
            sleeping: i.sleeping,
            sitting: i.sitting,
            riding: i.riding,
            climbing: self.climbing,
            in_water: i.in_water,
            on_ground: i.on_ground,
            flying: i.flying,
            speed,
            vy: i.vel[1],
            crouching: i.crouching,
        }));
        if self.forced.is_some() {
            self.smooth.snap(kind);
        }
        self.smooth.update(kind, i.dt);
        // The stroke of a swimmer or climber runs with how fast they go.
        let rate = match kind {
            Kind::Swim => 2.0 + speed * 2.4,
            Kind::Climb => crate::math::sqrtf(i.vel[1] * i.vel[1]) * 3.2,
            _ => 0.0,
        };
        self.stroke += rate * i.dt;
        if self.stroke > 1000.0 {
            self.stroke -= 1000.0;
        }
        // A run builds up and fades over a fifth of a second or so.
        let want_run = if i.on_ground && !i.crouching { pose::gait::run_amount(speed) } else { 0.0 };
        self.run += (want_run - self.run) * (i.dt * 8.0).clamp(0.0, 1.0);
        if let Some((_, _, r)) = self.shown_gait {
            self.run = r;
        }
        // About two radians of leg swing per block walked; a little drift in the air.
        self.gait += (if i.on_ground { speed * 2.1 } else { 1.0 }) * i.dt;
        if self.gait > 1000.0 {
            self.gait -= 1000.0;
        }
        if !i.sleeping {
            self.lying = None;
        }
        // An emote ends on moving, on being hurt, on its own end, or when it cannot be played.
        let hurt = i.health < self.health;
        self.health = i.health;
        if let Some((n, t)) = self.emote {
            let t = if self.frozen { t } else { t + i.dt };
            let over = pose::emote::of(pose::emote::code(n)).map_or(true, |e| e.at(t).is_none());
            if !self.frozen && (i.moving || hurt || over || i.dead || i.sleeping || i.in_water) {
                self.emote = None;
            } else {
                self.emote = Some((n, t));
            }
        }
    }

    /// Start playing the emote at place `index` of the registry, if there is one
    /// (and the body is in a state to).
    pub fn start_emote(&mut self, index: usize) -> bool {
        if index >= crate::block::reg().emotes.list.len() || self.kind() == Kind::Sleep {
            return false;
        }
        self.emote = Some((index, 0.0));
        true
    }

    pub fn stop_emote(&mut self) {
        self.emote = None;
    }

    pub fn kind(&self) -> Kind {
        self.smooth.target()
    }

    /// The code of the emote being played, to send to others (0 = none).
    pub fn emote_code(&self) -> u16 {
        self.emote.map_or(0, |e| pose::emote::code(e.0))
    }

    /// The body to draw: the posture so far, with the activities and the emote.
    pub fn body(&self, a: Activity) -> Body {
        let mut b = self.smooth.body();
        b.flags = a.flags | if self.carrying { pose::CARRY } else { 0 };
        b.swing = a.swing;
        if let Some(f) = self.shown_swing {
            b.flags |= pose::SWING;
            b.swing = f;
        }
        b.draw = a.draw;
        b.run = self.run.clamp(0.0, 1.0);
        if let Some((n, t)) = self.emote {
            b.emote = pose::emote::code(n);
            b.emote_t = t;
        }
        b
    }

    /// The pose byte that others are sent (see `proto::Msg::Move`).
    pub fn wire(&self, a: Activity) -> u8 {
        pose::pack(self.kind(), a.flags | if self.carrying { pose::CARRY } else { 0 })
    }
}

/// What the arms are doing besides the posture: drawing a bow, eating,
/// striking.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Activity {
    pub flags: u8,
    pub swing: f32,
    pub draw: f32,
}

impl Activity {
    /// From how far a bow is drawn, how far the food is eaten and the
    /// cooldowns of a blow (0.35 s) and of a use (0.5 s).
    pub fn of(draw: f32, eaten: f32, blow_left: f32, use_left: f32) -> Activity {
        let left = (blow_left / 0.35).max(use_left / 0.5).clamp(0.0, 1.0);
        let mut a = Activity::default();
        if draw > 0.0 {
            a.flags |= pose::BOW;
            a.draw = draw.clamp(0.0, 1.0);
        } else if eaten > 0.0 {
            a.flags |= pose::EAT;
        } else if left > 0.0 {
            a.flags |= pose::SWING;
            a.swing = (1.0 - left).max(0.001);
        }
        a
    }
}

impl crate::session::Session {
    /// What the arms are doing now, from the bow, the food and the blows.
    pub fn activity(&self) -> Activity {
        let id = self.held().map_or(u16::MAX, |h| h.0 as u16);
        Activity::of(self.bow.shown(id), self.eating.shown(id), self.blow_left(), self.cooldown[1])
    }

    /// Once a frame: settle the posture, run the emote, and tell the others of both.
    pub fn update_posing(&mut self, dt: f32, c: &crate::session::Controls) {
        let moving = c.input.forward != 0.0 || c.input.strafe != 0.0 || c.input.jump || c.input.descend;
        let p = &self.player;
        let inputs = Inputs {
            dt,
            clock: self.clock,
            sleeping: self.sleeping,
            sitting: self.sitting.is_some(),
            riding: self.riding_mob.is_some() || self.carried,
            in_water: p.in_water,
            on_ground: p.on_ground,
            flying: p.flying,
            crouching: p.crouch > 0.5,
            vel: p.vel,
            moving,
            health: self.health,
            dead: self.dead,
        };
        self.posing.update(&inputs);
        let a = self.activity();
        let (pose_byte, code) = (self.posing.wire(a), self.posing.emote_code());
        if let Some(n) = self.net.as_mut() {
            n.pose = pose_byte;
            if code != self.posing.told {
                n.send_emote(code);
            }
        }
        self.posing.told = code;
    }

    /// Play the emote at place `index` of the registry (from the wheel).
    pub fn play_emote(&mut self, index: usize) {
        if self.dead || self.sleeping {
            return;
        }
        self.posing.start_emote(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still(dt: f32) -> Inputs {
        Inputs { dt, clock: 0.0, sleeping: false, sitting: false, riding: false, in_water: false, on_ground: true, flying: false, crouching: false, vel: [0.0; 3], moving: false, health: 20, dead: false }
    }

    #[test]
    fn sitting_down_and_getting_up_blend() {
        let mut p = Posing::default();
        p.update(&Inputs { sitting: true, ..still(0.05) });
        assert_eq!(p.kind(), Kind::Sit);
        assert!(p.smooth.weight(Kind::Sit) < 1.0 && p.smooth.weight(Kind::Sit) > 0.0);
        for _ in 0..10 {
            p.update(&Inputs { sitting: true, ..still(0.05) });
        }
        assert_eq!(p.smooth.weight(Kind::Sit), 1.0);
        for _ in 0..10 {
            p.update(&still(0.05));
        }
        assert_eq!(p.kind(), Kind::Stand);
        assert_eq!(p.smooth.weight(Kind::Stand), 1.0);
    }

    #[test]
    fn sprinting_builds_a_run_and_walking_does_not() {
        let mut p = Posing::default();
        let going = |v: f32| Inputs { vel: [v, 0.0, 0.0], ..still(0.1) };
        for _ in 0..20 {
            p.update(&going(4.3));
        }
        assert_eq!(p.run, 0.0);
        for _ in 0..30 {
            p.update(&going(5.6));
        }
        assert!(p.run > 0.95 && p.body(Activity::default()).run > 0.95);
        // In the air or crouching there is no run.
        for _ in 0..60 {
            p.update(&Inputs { on_ground: false, ..going(5.6) });
        }
        assert!(p.run < 0.01);
    }

    #[test]
    fn the_stroke_only_runs_while_moving() {
        let mut p = Posing::default();
        let climbing = |vy: f32| Inputs { vel: [0.0, vy, 0.0], ..still(0.1) };
        p.climbing = true;
        p.update(&climbing(0.0));
        assert_eq!(p.stroke, 0.0);
        p.update(&climbing(3.0));
        assert!(p.stroke > 0.5);
    }

    #[test]
    fn an_emote_stops_on_movement_damage_and_its_end() {
        let reg = crate::block::reg();
        let wave = reg.emotes.find("wave").expect("the base data has a wave");
        let mut p = Posing::default();
        assert!(p.start_emote(wave));
        assert_eq!(p.emote_code(), pose::emote::code(wave));
        p.update(&still(0.1));
        assert!(p.emote.is_some());
        // Moving ends it.
        p.update(&Inputs { moving: true, ..still(0.1) });
        assert!(p.emote.is_none() && p.emote_code() == 0);
        // So does a hit.
        p.start_emote(wave);
        p.update(&Inputs { health: 17, ..still(0.1) });
        assert!(p.emote.is_none());
        // And so does running out.
        p.start_emote(wave);
        for _ in 0..100 {
            p.update(&Inputs { health: 17, ..still(0.1) });
        }
        assert!(p.emote.is_none());
        assert!(!p.start_emote(10_000));
        // No emotes in bed.
        let mut bed = Posing::default();
        bed.update(&Inputs { sleeping: true, ..still(0.1) });
        assert!(!bed.start_emote(wave));
    }

    #[test]
    fn the_wire_byte_carries_the_posture_and_the_arms() {
        let mut p = Posing::default();
        p.update(&Inputs { crouching: true, ..still(0.1) });
        let a = Activity::of(0.0, 0.0, 0.3, 0.0);
        assert_eq!(a.flags, pose::SWING);
        assert!(a.swing > 0.0 && a.swing < 1.0);
        assert_eq!(pose::unpack(p.wire(a)), (Kind::Crouch, pose::SWING));
        assert_eq!(Activity::of(0.6, 0.0, 0.3, 0.0).flags, pose::BOW);
        assert_eq!(Activity::of(0.0, 0.4, 0.0, 0.0).flags, pose::EAT);
        assert_eq!(Activity::of(0.0, 0.0, 0.0, 0.0), Activity::default());
    }
}
