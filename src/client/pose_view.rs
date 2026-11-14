//! Where and how the bodies of players and villagers are drawn, from what
//! the game knows of their posture (see content/pose for the shapes).

use crate::block::{def, Block, Usable};
use crate::content::pose::{self, Body, Kind};
use crate::math::{floord, sqrtf, PI};
use crate::model::Pose;

/// A body ready to draw: its pose, where its feet are (world) and which way it faces.
pub struct Drawn {
    pub pose: Pose,
    pub at: [f64; 3],
    pub yaw: f32,
}

fn lerp(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k)
}

/// Place a body for its posture: a seated one is drawn with its feet where
/// the player's position puts them, one drop below the seat, and rises to
/// the seat as it sits; a sleeper moves onto the bed.
fn place(pos: [f64; 3], yaw: f32, body: &Body, lying: Option<([f64; 3], f32)>) -> ([f64; 3], f32) {
    let mut at = pos;
    let mut yaw = yaw;
    at[1] += body.weight(Kind::Sit) as f64 * pose::sit::drop_blocks();
    let w = body.weight(Kind::Sleep) as f64;
    if let (true, Some((middle, face))) = (w > 0.0, lying) {
        at = lerp(at, pose::sleep::hip_at(middle, face), w);
        if w > 0.5 {
            yaw = face;
        }
    }
    (at, yaw)
}

/// The player's own body, seen from outside.
pub fn local(g: &crate::session::Session) -> Drawn {
    let body = g.posing.body(g.activity());
    let v = g.player.vel;
    let speed = sqrtf(v[0] * v[0] + v[2] * v[2]);
    let (phase, stride) = match g.posing.kind() {
        Kind::Swim => (g.posing.stroke, (speed / 2.4).min(1.0)),
        Kind::Climb => (g.posing.stroke, (sqrtf(v[1] * v[1]) / 3.0).min(1.0)),
        Kind::Crouch => (g.posing.gait, if g.player.on_ground { (speed / 1.3).min(1.0) } else { 0.2 }),
        _ => (g.posing.gait, if g.player.on_ground { (speed / 4.0).min(1.0) } else { 0.2 }),
    };
    let (phase, stride) = g.posing.shown_gait.map_or((phase, stride), |(p, s, _)| (p, s));
    // Looking up raises the head (positive turn); a sleeper stares at the ceiling whatever the mouse does.
    let head = if body.weight(Kind::Sleep) > 0.5 { 0.0 } else { g.player.pitch * 180.0 / PI };
    let pose = Pose { phase, stride, head_pitch: head, head_yaw: 0.0, clock: g.clock, body, ..Default::default() };
    let lying = g.posing.lying.map(|l| (l.middle, l.yaw));
    let (at, yaw) = place(g.player.pos, g.player.yaw, &body, lying);
    Drawn { pose, at, yaw }
}

/// Another player's body, from the position, pose byte and emote they sent.
pub fn remote(p: &crate::netplay::Remote, clock: f32) -> Drawn {
    let (_, flags) = pose::unpack(p.pose);
    let mut body = p.body.body();
    body.flags = flags;
    // Only that they strike is sent: the swing runs through from when the flag came up.
    match pose::swing::phase(p.swing_t, flags & pose::SWING != 0) {
        Some(phase) => {
            body.flags |= pose::SWING;
            body.swing = phase;
        }
        None => body.flags &= !pose::SWING,
    }
    // Their pace tells a sprint from a walk (the body is leaning into it only on foot).
    body.run = pose::gait::run_amount(p.pace);
    if p.emote != 0 {
        body.emote = p.emote;
        body.emote_t = p.emote_t;
    }
    let (phase, stride) = match body.kind {
        Kind::Swim => (p.phase + clock * 2.0, p.stride.max(0.4)),
        Kind::Climb => (clock * 5.0, 0.7),
        _ => (p.phase, p.stride),
    };
    let pose = Pose { phase, stride, head_pitch: p.pitch * 180.0 / PI, head_yaw: 0.0, clock, body, ..Default::default() };
    // Beds are not sent: a sleeper lies where they are (a little up, on the bed).
    let lying = Some(([p.pos[0], p.pos[1] + 0.56, p.pos[2]], p.yaw));
    let (at, yaw) = place(p.pos, p.yaw, &body, lying);
    Drawn { pose, at, yaw }
}

/// The posture of a villager or other creature standing still where it can
/// sit or lie: on a seat, or in a bed. `pos` is its feet, `get` the world.
pub fn creature_posture(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3], stride: f32) -> Kind {
    if stride > 0.05 {
        return Kind::Stand;
    }
    let (x, y, z) = (floord(pos[0]), floord(pos[1] + 0.02), floord(pos[2]));
    let d = def(get(x, y, z));
    let on_top = |height: f64| pos[1] - y as f64 <= height + 0.2;
    if d.usable == Usable::Bed && on_top(0.6) {
        Kind::Sleep
    } else if d.x.seat > 0 && on_top(d.x.seat as f64 / 16.0) {
        Kind::Sit
    } else {
        Kind::Stand
    }
}

/// Where a creature in a posture is placed: a sleeper has its body's middle where it stands.
pub fn creature_at(kind: Kind, pos: [f64; 3], yaw: f32) -> [f64; 3] {
    if kind == Kind::Sleep {
        pose::sleep::hip_at(pos, yaw)
    } else {
        pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seated_body_rises_to_the_seat_as_it_sits() {
        let pos = [1.0, 10.0, 1.0];
        let stand = place(pos, 0.0, &Body::steady(Kind::Stand), None).0;
        let sit = place(pos, 0.0, &Body::steady(Kind::Sit), None).0;
        assert_eq!(stand, pos);
        assert!((sit[1] - pos[1] - pose::sit::drop_blocks()).abs() < 1e-9);
        let half = place(pos, 0.0, &Body { kind: Kind::Sit, from: Kind::Stand, mix: 0.5, ..Body::default() }, None).0;
        assert!(half[1] > pos[1] && half[1] < sit[1]);
    }

    #[test]
    fn a_sleeper_lies_on_the_bed_facing_the_way_they_lay_down() {
        let bed = ([5.5, 64.56, 5.5], 1.0f32);
        let (at, yaw) = place([2.0, 64.0, 2.0], 0.3, &Body::steady(Kind::Sleep), Some(bed));
        assert_eq!(yaw, 1.0);
        assert!((at[1] - 64.56).abs() < 1e-9);
        assert!((at[0] - 5.5).abs() < 0.3 && (at[2] - 5.5).abs() < 0.3);
        // Not asleep: left where they are.
        assert_eq!(place([2.0, 64.0, 2.0], 0.3, &Body::default(), Some(bed)), ([2.0, 64.0, 2.0], 0.3));
    }

    #[test]
    fn creatures_sit_on_seats_and_lie_in_beds_only_when_still() {
        let reg = crate::block::reg();
        let seat = reg.blocks.iter().position(|b| b.x.seat > 0 && b.x.seat < 16); // a seat as tall as a block has no air above its top to stand in
        let bed = reg.blocks.iter().position(|b| b.usable == Usable::Bed);
        if let Some(s) = seat {
            let get = move |_: i32, y: i32, _: i32| if y == 10 { s as Block } else { 0 };
            let top = 10.0 + reg.blocks[s].x.seat as f64 / 16.0;
            assert_eq!(creature_posture(&get, [3.5, top, 3.5], 0.0), Kind::Sit);
            assert_eq!(creature_posture(&get, [3.5, top, 3.5], 0.5), Kind::Stand);
        }
        if let Some(b) = bed {
            let get = move |_: i32, y: i32, _: i32| if y == 10 { b as Block } else { 0 };
            assert_eq!(creature_posture(&get, [3.5, 10.56, 3.5], 0.0), Kind::Sleep);
        }
        let air = |_: i32, _: i32, _: i32| 0 as Block;
        assert_eq!(creature_posture(&air, [3.5, 10.0, 3.5], 0.0), Kind::Stand);
    }
}
