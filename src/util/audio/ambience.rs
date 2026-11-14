//! What the world sounds like around the player, as a plan: how loud each
//! continuous bed ([`super::bed`]) should be and how muffled, and the single
//! sounds that happen now and then on top (a drop, a gust, a bubble, a pop,
//! a crackle, a drip, thunder). Pure logic with no sound device in it: the
//! game feeds it an [`Env`] and the time, and takes back targets and
//! one-shots.
//!
//! Everything moves slowly. Levels fade over a couple of seconds when the
//! weather changes, the volume and the brightness wander on slow random
//! walks so the beds breathe, and single sounds come at randomised
//! intervals, never on a beat.

use super::bed::{BedKind, BEDS};
use super::rand::Rng;
use super::variation::NoRepeat;
use crate::math::{clampf, cosf, lerp, smoothstep, sqrtf, TAU};
use alloc::vec::Vec;

/// What is around the player (all 0 to 1 unless it says otherwise).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Env {
    /// How hard it rains or snows.
    pub rain: f32,
    pub snow: bool,
    pub storm: bool,
    /// How much roof there is overhead (0 open sky, 1 fully covered).
    pub shelter: f32,
    /// How deep inside rock: 0 outdoors or under a roof, 1 deep in a cave.
    pub cave: f32,
    /// The head is under water.
    pub underwater: bool,
    /// How windy it is where the player stands (height and open ground).
    pub wind: f32,
    /// How much open water is near.
    pub water: f32,
    /// How much lava is near.
    pub lava: f32,
    /// How much fire is near.
    pub fire: f32,
}

/// A single sound the mixer plays once: a burst of noise and a gliding tone,
/// the way every made sound is (see `sound` in docs/DATA.md).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OneShot {
    pub secs: f32,
    pub noise: f32,
    pub hz0: f32,
    pub hz1: f32,
    pub lp: f32,
    pub gain: f32,
}

/// A one-shot to start `delay` seconds from now. Ambience one-shots follow
/// the AMBIENCE volume; thunder (`effects`) follows EFFECTS.
#[derive(Clone, Copy, Debug)]
pub struct Timed {
    pub delay: f32,
    pub shot: OneShot,
    pub effects: bool,
}

/// Where a bed should be: loudness 0..1 and where its low-pass starts.
#[derive(Clone, Copy, Debug, Default)]
pub struct BedTarget {
    pub level: f32,
    pub top_hz: f32,
}

/// A slow wander inside `lo..=hi`: it drifts a little each step and is
/// pulled gently back to the middle, so it never leaves its range.
#[derive(Clone, Copy, Debug)]
pub struct RandomWalk {
    pub v: f32,
    lo: f32,
    hi: f32,
    /// Drift per square-root second.
    speed: f32,
    /// Pull towards the middle, per second.
    pull: f32,
}

impl RandomWalk {
    pub fn new(lo: f32, hi: f32, speed: f32, pull: f32) -> RandomWalk {
        RandomWalk { v: (lo + hi) * 0.5, lo, hi, speed, pull }
    }

    pub fn step(&mut self, dt: f32, rng: &mut Rng) -> f32 {
        let mid = (self.lo + self.hi) * 0.5;
        let dt = clampf(dt, 0.0, 1.0);
        self.v += (mid - self.v) * self.pull * dt + rng.signed() * self.speed * sqrtf(dt) * 1.7;
        self.v = clampf(self.v, self.lo, self.hi);
        self.v
    }
}

// ---- Intensity to sound ----

/// How loud the rain bed is for a rain of `rain` (0 to 1). Nothing under a
/// drizzle that is hardly there.
pub fn rain_level(rain: f32) -> f32 {
    if rain < 0.08 {
        0.0
    } else {
        0.15 + 0.85 * smoothstep(0.08, 1.0, rain)
    }
}

/// Single drops per second on top of the bed: none in a mist, a few in a
/// light rain, a patter in a downpour. Never goes down as the rain grows.
pub fn rain_drop_rate(rain: f32) -> f32 {
    if rain < 0.1 {
        0.0
    } else {
        let r = clampf(rain, 0.0, 1.0);
        0.4 + 8.6 * r * r
    }
}

/// The wind bed's loudness from how exposed the place is, and the weather.
pub fn wind_level(wind: f32, rain: f32, snow: bool, storm: bool) -> f32 {
    clampf(wind * 0.55 + rain * 0.25 + if snow { 0.1 } else { 0.0 } + if storm { 0.25 } else { 0.0 }, 0.0, 1.0)
}

pub fn water_level(water: f32) -> f32 {
    0.8 * smoothstep(0.05, 0.6, water)
}

pub fn lava_level(lava: f32) -> f32 {
    smoothstep(0.02, 0.35, lava)
}

pub fn fire_level(fire: f32) -> f32 {
    smoothstep(0.02, 0.3, fire)
}

pub fn cave_level(cave: f32) -> f32 {
    0.5 * clampf(cave, 0.0, 1.0)
}

/// Where a bed's low-pass sits: `base_hz` in the open, much lower under a
/// roof (`shelter`), and muffled to a dull wash under water (`under`).
pub fn muffled_hz(base_hz: f32, shelter: f32, under: f32) -> f32 {
    let roofed = base_hz * (1.0 - 0.82 * clampf(shelter, 0.0, 1.0));
    lerp(roofed, roofed.min(380.0), clampf(under, 0.0, 1.0))
}

/// Seconds until the next of something that happens `rate` times a second
/// on average: random, between a quarter and one and three quarters of the
/// mean, so it never comes on a beat.
pub fn next_gap(rate: f32, rng: &mut Rng) -> f32 {
    if rate < 0.02 {
        return 0.5;
    }
    (1.0 / rate) * (0.25 + 1.5 * rng.unit())
}

/// A gust of wind: a slow swell that comes now and then.
#[derive(Clone, Copy, Debug)]
struct Gust {
    wait: f32,
    t: f32,
    dur: f32,
    amp: f32,
}

impl Gust {
    fn value(&self) -> f32 {
        if self.t < self.dur {
            self.amp * (0.5 - 0.5 * cosf(TAU * self.t / self.dur))
        } else {
            0.0
        }
    }

    fn step(&mut self, dt: f32, wind: f32, rng: &mut Rng) {
        if self.t < self.dur {
            self.t += dt;
            return;
        }
        self.wait -= dt;
        if self.wait <= 0.0 {
            if wind > 0.08 {
                self.dur = rng.range(2.0, 5.5);
                self.t = 0.0;
                self.amp = clampf(rng.range(0.3, 0.7) * (0.5 + wind), 0.0, 1.0);
            }
            self.wait = rng.range(4.0, 12.0);
        }
    }
}

/// How long a full fade takes when the weather changes, in seconds.
pub const FADE_SECONDS: f32 = 2.5;

/// Thunders: (seconds, noise, hz from, hz to, low-pass, loudness). Four
/// takes; one is never followed by itself.
const THUNDERS: [(f32, f32, f32, f32, f32, f32); 4] = [
    (1.8, 1.0, 60.0, 40.0, 0.040, 0.95),
    (2.5, 1.0, 52.0, 32.0, 0.030, 0.9),
    (1.4, 0.95, 78.0, 46.0, 0.060, 0.85),
    (3.1, 0.9, 46.0, 28.0, 0.025, 0.95),
];

/// Seconds a lightning flash is heard late per block of distance (80 blocks a second).
pub const THUNDER_BLOCKS_PER_SECOND: f32 = 80.0;

/// The plan for the sounds around the player.
pub struct Ambience {
    rng: Rng,
    shelter: f32,
    cave: f32,
    under: f32,
    fade: [f32; BEDS],
    walk: [RandomWalk; BEDS],
    tone: [RandomWalk; BEDS],
    cut: [f32; BEDS],
    /// Where each bed should be now.
    pub targets: [BedTarget; BEDS],
    /// Countdowns: rain drops, roof drips, water bubbles, lava, fire, cave drips.
    timers: [f32; 6],
    gust: Gust,
    thunder_pick: NoRepeat,
}

impl Ambience {
    pub fn new(seed: u32) -> Ambience {
        let mut cut = [0.0; BEDS];
        for k in BedKind::ALL {
            cut[k.index()] = k.recipe().top_hz;
        }
        let walk = [
            RandomWalk::new(0.85, 1.12, 0.05, 0.3),
            RandomWalk::new(0.7, 1.2, 0.12, 0.25),
            RandomWalk::new(0.6, 1.3, 0.12, 0.15),
            RandomWalk::new(0.8, 1.2, 0.06, 0.3),
            RandomWalk::new(0.75, 1.2, 0.15, 0.5),
            RandomWalk::new(0.8, 1.1, 0.04, 0.3),
        ];
        Ambience {
            rng: Rng::new(seed),
            shelter: 0.0,
            cave: 0.0,
            under: 0.0,
            fade: [0.0; BEDS],
            walk,
            tone: [RandomWalk::new(0.88, 1.12, 0.05, 0.3); BEDS],
            cut,
            targets: [BedTarget::default(); BEDS],
            timers: [1.0; 6],
            gust: Gust { wait: 3.0, t: 1.0, dur: 0.0, amp: 0.0 },
            thunder_pick: NoRepeat::new(),
        }
    }

    /// How sheltered the player is now (smoothed).
    pub fn shelter(&self) -> f32 {
        self.shelter
    }

    /// Whether anything is audible or on its way (a bed is up or fading).
    pub fn active(&self) -> bool {
        self.fade.iter().any(|&f| f > 0.001) || self.targets.iter().any(|t| t.level > 0.001)
    }

    /// Stop at once (a new world): everything starts from silence.
    pub fn reset(&mut self) {
        self.fade = [0.0; BEDS];
        self.shelter = 0.0;
        self.cave = 0.0;
        self.under = 0.0;
        self.targets = [BedTarget::default(); BEDS];
        for k in BedKind::ALL {
            self.cut[k.index()] = k.recipe().top_hz;
        }
    }

    /// Move on by `dt` seconds with the surroundings `env`. The bed targets
    /// are updated; single sounds are added to `out`.
    pub fn update(&mut self, dt: f32, env: &Env, out: &mut Vec<Timed>) {
        let dt = clampf(dt, 0.0, 0.25);
        if dt <= 0.0 {
            return;
        }
        // The surroundings arrive in jumps (every few tenths of a second):
        // blend them so a step into a house is a short muffling, not a click.
        let a = dt / (0.7 + dt);
        self.shelter += (clampf(env.shelter, 0.0, 1.0) - self.shelter) * a;
        self.cave += (clampf(env.cave, 0.0, 1.0) - self.cave) * a;
        self.under += ((env.underwater as i32 as f32) - self.under) * (dt / (0.25 + dt));
        let (sh, sub) = (self.shelter, self.under);
        let wet = if env.snow { 0.0 } else { clampf(env.rain, 0.0, 1.0) };
        self.gust.step(dt, env.wind, &mut self.rng);
        let gust = self.gust.value();

        let mut want = [0.0f32; BEDS];
        want[BedKind::Rain.index()] = rain_level(wet) * (1.0 - 0.55 * sh) * (1.0 - sub);
        want[BedKind::Wind.index()] = wind_level(env.wind, wet, env.snow, env.storm) * (1.0 - 0.8 * sh) * (1.0 - sub);
        want[BedKind::Water.index()] = (water_level(env.water) * (1.0 - 0.6 * sh)).max(0.7 * sub);
        want[BedKind::Lava.index()] = lava_level(env.lava) * (1.0 - 0.5 * sub);
        want[BedKind::Fire.index()] = fire_level(env.fire) * (1.0 - 0.7 * sub);
        want[BedKind::Cave.index()] = cave_level(self.cave) * (1.0 - sub);

        let slew = dt / FADE_SECONDS;
        let blend = dt / (0.6 + dt);
        for k in BedKind::ALL {
            let i = k.index();
            let f = &mut self.fade[i];
            if want[i] > *f {
                *f = (*f + slew).min(want[i]);
            } else {
                *f = (*f - slew).max(want[i]);
            }
            let m = self.walk[i].step(dt, &mut self.rng);
            let t = self.tone[i].step(dt, &mut self.rng);
            let mut level = self.fade[i] * m;
            let mut bright = t;
            match k {
                BedKind::Wind => {
                    level = self.fade[i] * (m + 0.6 * gust);
                    bright += 0.35 * gust;
                }
                // A swell: bigger and brighter as it comes in.
                BedKind::Water => bright *= 0.75 + 0.35 * m,
                _ => {}
            }
            let hz = muffled_hz(k.recipe().top_hz * bright, sh, sub);
            self.cut[i] += (hz - self.cut[i]) * blend;
            self.targets[i] = BedTarget { level: clampf(level, 0.0, 1.0), top_hz: self.cut[i] };
        }
        self.single_sounds(dt, env, wet, out);
    }

    fn single_sounds(&mut self, dt: f32, env: &Env, wet: f32, out: &mut Vec<Timed>) {
        let (sh, sub) = (self.shelter, self.under);
        let rng = &mut self.rng;
        let mut put = |shot: OneShot| out.push(Timed { delay: 0.0, shot, effects: false });
        // Rain: single drops over the bed, thinner and duller under a roof.
        self.timers[0] -= dt;
        if self.timers[0] <= 0.0 {
            let rate = rain_drop_rate(wet) * (1.0 - sub);
            if rate >= 0.02 {
                let hz0 = rng.range(1500.0, 2600.0) * (1.0 - 0.5 * sh);
                put(OneShot { secs: rng.range(0.025, 0.06), noise: 0.2, hz0, hz1: hz0 * rng.range(0.5, 0.8), lp: 0.35, gain: rng.range(0.05, 0.12) * (1.0 - 0.5 * sh) });
            }
            self.timers[0] = next_gap(rate, rng);
        }
        // Rain dripping off a roof, heard from under it.
        self.timers[1] -= dt;
        if self.timers[1] <= 0.0 {
            let rate = wet * sh * (1.0 - sub) * 2.5;
            if rate >= 0.02 {
                let hz0 = rng.range(700.0, 1100.0);
                put(OneShot { secs: rng.range(0.05, 0.09), noise: 0.1, hz0, hz1: hz0 * 0.6, lp: 0.3, gain: rng.range(0.08, 0.14) });
            }
            self.timers[1] = next_gap(rate, rng);
        }
        // Water: bubbles and plops.
        self.timers[2] -= dt;
        if self.timers[2] <= 0.0 {
            let amount = water_level(env.water).max(self.under);
            let rate = if amount > 0.02 { 0.15 + 1.0 * amount } else { 0.0 };
            if rate >= 0.02 {
                let hz0 = rng.range(280.0, 520.0);
                put(OneShot { secs: rng.range(0.06, 0.14), noise: 0.08, hz0, hz1: hz0 * rng.range(1.6, 2.4), lp: 0.3, gain: rng.range(0.05, 0.11) * (1.0 - 0.5 * sh) });
            }
            self.timers[2] = next_gap(rate, rng);
        }
        // Lava: slow pops and bubbles.
        self.timers[3] -= dt;
        if self.timers[3] <= 0.0 {
            let amount = lava_level(env.lava);
            let rate = if amount > 0.02 { 0.3 + 1.0 * amount } else { 0.0 };
            if rate >= 0.02 {
                let hz0 = rng.range(80.0, 140.0);
                if rng.unit() < 0.5 {
                    put(OneShot { secs: rng.range(0.1, 0.22), noise: 0.55, hz0, hz1: hz0 * 0.55, lp: rng.range(0.1, 0.18), gain: rng.range(0.22, 0.4) });
                } else {
                    put(OneShot { secs: rng.range(0.12, 0.2), noise: 0.15, hz0, hz1: hz0 * 1.8, lp: 0.15, gain: rng.range(0.15, 0.25) });
                }
            }
            self.timers[3] = next_gap(rate, rng);
        }
        // Fire: crackles, and now and then a bigger pop.
        self.timers[4] -= dt;
        if self.timers[4] <= 0.0 {
            let amount = fire_level(env.fire);
            let rate = if amount > 0.02 { 2.0 + 5.0 * amount } else { 0.0 };
            if rate >= 0.02 {
                if rng.unit() < 0.12 {
                    put(OneShot { secs: rng.range(0.05, 0.09), noise: 0.7, hz0: 200.0, hz1: 120.0, lp: 0.3, gain: rng.range(0.15, 0.25) });
                } else {
                    put(OneShot { secs: rng.range(0.012, 0.04), noise: 0.95, hz0: 0.0, hz1: 0.0, lp: rng.range(0.25, 0.5), gain: rng.range(0.08, 0.2) });
                }
            }
            self.timers[4] = next_gap(rate, rng);
        }
        // A cave: a drip, and its echo.
        self.timers[5] -= dt;
        if self.timers[5] <= 0.0 {
            let rate = self.cave * (1.0 - sub) * 0.25;
            if rate >= 0.02 {
                let hz0 = rng.range(800.0, 1300.0);
                let shot = OneShot { secs: rng.range(0.08, 0.12), noise: 0.01, hz0, hz1: hz0 * rng.range(0.55, 0.75), lp: 0.5, gain: rng.range(0.08, 0.14) };
                put(shot);
                out.push(Timed { delay: rng.range(0.14, 0.3), shot: OneShot { hz0: shot.hz0 * 0.95, hz1: shot.hz1 * 0.95, gain: shot.gain * 0.45, ..shot }, effects: false });
            }
            self.timers[5] = next_gap(rate, rng);
        }
    }

    /// A lightning strike `dist` blocks away: a crack that comes first (only
    /// when it is near), the rumble, and a softer roll behind it. The
    /// farther it is, the later, the quieter and the duller. One of four
    /// takes, never the one before.
    pub fn thunder(&mut self, dist: f32, out: &mut Vec<Timed>) {
        let dist = clampf(dist, 0.0, 400.0);
        let v = self.thunder_pick.pick(THUNDERS.len(), &mut self.rng);
        let (secs, noise, hz0, hz1, lp, loud) = THUNDERS[v];
        let far = smoothstep(10.0, 260.0, dist);
        let delay = clampf(dist / THUNDER_BLOCKS_PER_SECOND, 0.0, 4.0);
        let indoors = clampf(self.shelter, 0.0, 1.0);
        let pitch = 1.0 + self.rng.signed() * 0.04;
        let gain = loud * (1.0 - 0.7 * far) * (1.0 - 0.4 * indoors) * self.rng.range(0.92, 1.08);
        let lp = (lerp(lp, lp * 0.35, far) * (1.0 - 0.5 * indoors)).max(0.012);
        let rumble = OneShot { secs: secs * (1.0 + 0.5 * far), noise, hz0: hz0 * pitch, hz1: hz1 * pitch, lp, gain };
        if far < 0.6 {
            let near = 1.0 - far;
            let crack = OneShot { secs: 0.14, noise: 1.0, hz0: 0.0, hz1: 0.0, lp: 0.28 * near * (1.0 - 0.5 * indoors) + 0.03, gain: 0.7 * near * (1.0 - 0.4 * indoors) };
            out.push(Timed { delay, shot: crack, effects: true });
        }
        out.push(Timed { delay: delay + 0.05, shot: rumble, effects: true });
        let roll = OneShot { secs: rumble.secs * 0.8, hz0: rumble.hz0 * 1.1, hz1: rumble.hz1 * 1.1, lp: lp * 0.7, gain: gain * 0.5, ..rumble };
        out.push(Timed { delay: delay + self.rng.range(0.7, 1.6), shot: roll, effects: true });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: &mut Ambience, env: &Env, seconds: f32) -> Vec<Timed> {
        let mut out = Vec::new();
        let mut t = 0.0;
        while t < seconds {
            a.update(1.0 / 60.0, env, &mut out);
            t += 1.0 / 60.0;
        }
        out
    }

    #[test]
    fn the_walk_never_leaves_its_range() {
        let mut rng = Rng::new(3);
        for (lo, hi, speed, pull) in [(0.5, 1.5, 0.5, 0.0), (0.0, 1.0, 5.0, 0.0), (0.85, 1.12, 0.05, 0.3)] {
            let mut w = RandomWalk::new(lo, hi, speed, pull);
            let (mut mn, mut mx) = (f32::MAX, f32::MIN);
            for _ in 0..50_000 {
                let v = w.step(1.0 / 60.0, &mut rng);
                assert!(v >= lo && v <= hi, "{v}");
                mn = mn.min(v);
                mx = mx.max(v);
            }
            assert!(mx - mn > (hi - lo) * 0.05, "it does not move: {mn}..{mx}");
        }
    }

    #[test]
    fn the_walk_moves_slowly() {
        // Between two frames it changes by far less than its range.
        let mut rng = Rng::new(8);
        let mut w = RandomWalk::new(0.7, 1.2, 0.12, 0.25);
        let mut last = w.v;
        for _ in 0..5000 {
            let v = w.step(1.0 / 60.0, &mut rng);
            assert!((v - last).abs() < 0.05);
            last = v;
        }
    }

    #[test]
    fn heavier_rain_means_more_drops_and_more_bed() {
        assert_eq!(rain_drop_rate(0.0), 0.0);
        assert_eq!(rain_drop_rate(0.09), 0.0);
        assert_eq!(rain_level(0.05), 0.0);
        let mut last = (0.0f32, 0.0f32);
        for i in 0..=100 {
            let r = i as f32 / 100.0;
            let now = (rain_drop_rate(r), rain_level(r));
            assert!(now.0 >= last.0 && now.1 >= last.1, "not monotonic at {r}");
            last = now;
        }
        assert!(rain_drop_rate(0.2) > 0.3 && rain_drop_rate(0.2) < 1.5);
        assert!(rain_drop_rate(1.0) > 6.0 && rain_drop_rate(1.0) < 12.0);
        assert!(rain_level(1.0) <= 1.0001);
    }

    #[test]
    fn gaps_are_random_and_average_the_mean() {
        let mut rng = Rng::new(21);
        let mut total = 0.0;
        let (mut lo, mut hi) = (f32::MAX, 0.0f32);
        for _ in 0..4000 {
            let g = next_gap(2.0, &mut rng);
            assert!(g >= 0.125 - 1e-4 && g <= 0.875 + 1e-4);
            total += g;
            lo = lo.min(g);
            hi = hi.max(g);
        }
        assert!((total / 4000.0 - 0.5).abs() < 0.03);
        assert!(hi - lo > 0.5, "gaps are not varied");
        assert_eq!(next_gap(0.0, &mut rng), 0.5);
    }

    #[test]
    fn rain_fades_in_over_a_couple_of_seconds_and_out_again() {
        let mut a = Ambience::new(1);
        let rain = Env { rain: 1.0, ..Env::default() };
        let i = BedKind::Rain.index();
        run(&mut a, &rain, 0.5);
        let early = a.targets[i].level;
        assert!(early > 0.0 && early < 0.4, "{early}");
        run(&mut a, &rain, 3.0);
        let full = a.targets[i].level;
        assert!(full > 0.7, "{full}");
        // The weather clears: gone in a few seconds, not at once.
        run(&mut a, &Env::default(), 0.5);
        assert!(a.targets[i].level > 0.4);
        run(&mut a, &Env::default(), 3.5);
        assert!(a.targets[i].level < 0.001);
    }

    #[test]
    fn rain_is_quieter_and_duller_under_a_roof_and_a_wash_under_water() {
        let open = Env { rain: 1.0, ..Env::default() };
        let roof = Env { shelter: 1.0, ..open };
        let sub = Env { underwater: true, ..open };
        let i = BedKind::Rain.index();
        let (mut a, mut b, mut c) = (Ambience::new(2), Ambience::new(2), Ambience::new(2));
        run(&mut a, &open, 8.0);
        run(&mut b, &roof, 8.0);
        run(&mut c, &sub, 8.0);
        assert!(b.targets[i].level < a.targets[i].level * 0.7, "{} {}", b.targets[i].level, a.targets[i].level);
        assert!(b.targets[i].top_hz < a.targets[i].top_hz * 0.4);
        assert!(c.targets[i].level < 0.05);
        // Under water you hear the water, muffled.
        assert!(c.targets[BedKind::Water.index()].level > 0.3);
        assert!(c.targets[BedKind::Water.index()].top_hz < 500.0);
        // The open rain's low-pass is the soft 5-7 kHz one (give or take its slow wander).
        assert!(a.targets[i].top_hz > 4800.0 && a.targets[i].top_hz < 7500.0, "{}", a.targets[i].top_hz);
    }

    #[test]
    fn dry_open_calm_places_make_no_sound_at_all() {
        let mut a = Ambience::new(4);
        let out = run(&mut a, &Env::default(), 30.0);
        assert!(out.is_empty());
        assert!(!a.active());
    }

    #[test]
    fn drops_come_at_the_density_of_the_rain() {
        let count = |rain: f32| {
            let mut a = Ambience::new(5);
            let out = run(&mut a, &Env { rain, ..Env::default() }, 20.0);
            out.len() as f32 / 20.0
        };
        let (light, heavy) = (count(0.25), count(1.0));
        assert!(heavy > light * 3.0, "{light} {heavy}");
        assert!(heavy > 5.0 && heavy < 12.0, "{heavy}");
        // They are not on a beat: the gaps between them vary.
        let mut a = Ambience::new(6);
        let mut times = Vec::new();
        let mut out = Vec::new();
        for f in 0..1200 {
            out.clear();
            a.update(1.0 / 60.0, &Env { rain: 0.5, ..Env::default() }, &mut out);
            if !out.is_empty() {
                times.push(f);
            }
        }
        let gaps: Vec<i32> = times.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().max().unwrap() - gaps.iter().min().unwrap() >= 6, "{gaps:?}");
    }

    #[test]
    fn every_bed_comes_up_where_it_belongs() {
        let cases: [(BedKind, Env); 5] = [
            (BedKind::Wind, Env { wind: 0.8, ..Env::default() }),
            (BedKind::Water, Env { water: 0.8, ..Env::default() }),
            (BedKind::Lava, Env { lava: 0.8, ..Env::default() }),
            (BedKind::Fire, Env { fire: 0.8, ..Env::default() }),
            (BedKind::Cave, Env { cave: 1.0, shelter: 1.0, ..Env::default() }),
        ];
        for (kind, env) in cases {
            let mut a = Ambience::new(7);
            let out = run(&mut a, &env, 12.0);
            assert!(a.targets[kind.index()].level > 0.15, "{kind:?}: {}", a.targets[kind.index()].level);
            for other in BedKind::ALL {
                if other != kind && other != BedKind::Wind {
                    assert!(a.targets[other.index()].level < 0.05, "{kind:?} raised {other:?}");
                }
            }
            if kind != BedKind::Wind {
                assert!(!out.is_empty(), "{kind:?} makes no single sounds");
            }
        }
    }

    #[test]
    fn single_sounds_stay_soft_and_inside_their_ranges() {
        let env = Env { rain: 1.0, shelter: 0.5, water: 1.0, lava: 1.0, fire: 1.0, cave: 1.0, wind: 1.0, ..Env::default() };
        let mut a = Ambience::new(8);
        let out = run(&mut a, &env, 60.0);
        assert!(out.len() > 100);
        for t in out {
            let s = t.shot;
            assert!(s.gain > 0.0 && s.gain <= 0.45, "{s:?}");
            assert!(s.secs >= 0.01 && s.secs <= 0.3, "{s:?}");
            assert!(s.hz0 <= 4000.0 && s.hz1 <= 4000.0, "{s:?}");
            assert!(s.lp > 0.0 && s.lp <= 0.6);
            assert!(t.delay >= 0.0 && t.delay < 0.5 && !t.effects);
        }
    }

    #[test]
    fn thunder_is_later_quieter_and_duller_when_far_and_never_repeats() {
        // The same seed picks the same take first, so near and far compare like for like.
        let (mut a, mut b) = (Ambience::new(9), Ambience::new(9));
        let mut near = Vec::new();
        let mut far = Vec::new();
        a.thunder(20.0, &mut near);
        b.thunder(250.0, &mut far);
        let rumble = |v: &Vec<Timed>| *v.iter().filter(|t| t.shot.secs > 1.0).max_by(|a, b| a.shot.gain.total_cmp(&b.shot.gain)).unwrap();
        let (n, f) = (rumble(&near), rumble(&far));
        assert!(f.delay > n.delay + 1.5, "{} {}", n.delay, f.delay);
        assert!(f.shot.gain < n.shot.gain * 0.6);
        assert!(f.shot.lp < n.shot.lp * 0.7);
        assert!(near.iter().any(|t| t.shot.secs < 0.2), "a near strike has a crack");
        assert!(far.iter().all(|t| t.shot.secs > 1.0), "a far one is only a rumble");
        assert!(near.iter().all(|t| t.effects));
        // Never the same take twice running (the takes differ in length).
        let mut last = 0.0f32;
        let mut seen = Vec::new();
        for _ in 0..200 {
            let mut o = Vec::new();
            a.thunder(100.0, &mut o);
            let main = o.iter().filter(|t| t.shot.secs > 1.0).max_by(|x, y| x.shot.gain.total_cmp(&y.shot.gain)).unwrap().shot.secs;
            assert!(main != last, "the same thunder twice: {main}");
            last = main;
            if !seen.contains(&main) {
                seen.push(main);
            }
        }
        assert_eq!(seen.len(), THUNDERS.len());
    }

    #[test]
    fn thunder_indoors_is_softer() {
        let mut out_a = Vec::new();
        let mut out_b = Vec::new();
        let mut a = Ambience::new(10);
        let mut b = Ambience::new(10);
        run(&mut b, &Env { shelter: 1.0, ..Env::default() }, 5.0);
        a.thunder(60.0, &mut out_a);
        b.thunder(60.0, &mut out_b);
        let loud = |v: &Vec<Timed>| v.iter().map(|t| t.shot.gain).fold(0.0f32, f32::max);
        assert!(loud(&out_b) < loud(&out_a) * 0.95);
    }

    #[test]
    fn muffling_only_lowers_the_cutoff() {
        assert_eq!(muffled_hz(6000.0, 0.0, 0.0), 6000.0);
        assert!(muffled_hz(6000.0, 1.0, 0.0) < 1200.0);
        assert!(muffled_hz(6000.0, 0.0, 1.0) <= 380.0);
        assert!(muffled_hz(200.0, 0.0, 1.0) <= 200.0);
    }
}
