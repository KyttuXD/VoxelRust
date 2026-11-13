//! Vehicles: boats, rail carts, a buggy and a plane. Which vehicles exist,
//! what they look like and how fast they go comes from the data files; this
//! module moves them.

use crate::block::{reg, Medium};
use crate::inventory::Inventory;
use crate::math::{cosf, floord, sinf, Vec3, PI};
use crate::player::{Input, Player};
use crate::world::World;
use alloc::string::String;
use alloc::vec::Vec;

pub struct Vehicle {
    /// Index into the registry's vehicles.
    pub kind: usize,
    pub body: Player,
    /// -1 (reverse) ..= 1 (full ahead).
    pub throttle: f32,
    pub storage: Inventory,
    /// Wheel / propeller spin, for the model.
    pub phase: f32,
}

#[derive(Default)]
pub struct Vehicles {
    pub list: Vec<Vehicle>,
    /// The one the player is in.
    pub riding: Option<usize>,
}

/// Unit vector a body faces along the ground (yaw 0 is -z).
fn heading(yaw: f32) -> (f32, f32) {
    (-sinf(yaw), -cosf(yaw))
}

impl Vehicles {
    pub fn place(&mut self, kind: usize, pos: [f64; 3], yaw: f32) -> usize {
        let d = &reg().vehicles[kind];
        let mut body = Player::with_size(pos, d.width as f64, d.height as f64);
        body.yaw = yaw;
        self.list.push(Vehicle { kind, body, throttle: 0.0, storage: Inventory::new(), phase: 0.0 });
        self.list.len() - 1
    }

    /// The vehicle a ray hits first, and how far along the ray.
    pub fn hit_test(&self, origin: [f64; 3], dir: Vec3, reach: f32) -> Option<(usize, f32)> {
        let d = [dir.x as f64, dir.y as f64, dir.z as f64];
        let mut best: Option<(usize, f32)> = None;
        for (i, v) in self.list.iter().enumerate() {
            if self.riding == Some(i) {
                continue;
            }
            let (lo, hi) = v.body.aabb();
            let (mut t0, mut t1) = (0.0f64, reach as f64);
            let mut hit = true;
            for a in 0..3 {
                if d[a] > -1e-9 && d[a] < 1e-9 {
                    hit &= origin[a] >= lo[a] && origin[a] <= hi[a];
                    continue;
                }
                let (mut a0, mut a1) = ((lo[a] - origin[a]) / d[a], (hi[a] - origin[a]) / d[a]);
                if a0 > a1 {
                    core::mem::swap(&mut a0, &mut a1);
                }
                t0 = t0.max(a0);
                t1 = t1.min(a1);
            }
            if hit && t0 <= t1 && best.map_or(true, |b| (t0 as f32) < b.1) {
                best = Some((i, t0 as f32));
            }
        }
        best
    }

    /// Take a vehicle out of the world (picked back up, or scrapped).
    pub fn remove(&mut self, i: usize) -> Option<Vehicle> {
        if i >= self.list.len() {
            return None;
        }
        self.riding = match self.riding {
            Some(r) if r == i => None,
            Some(r) if r == self.list.len() - 1 => Some(i),
            other => other,
        };
        Some(self.list.swap_remove(i))
    }

    /// Move every vehicle. `input` and `look` (yaw, pitch) steer the one
    /// being ridden. Returns where the rider's eyes are, if riding.
    pub fn update(&mut self, dt: f32, world: &World, input: &Input, look: (f32, f32)) -> Option<[f64; 3]> {
        let r = reg();
        let (water, rail) = (r.wk.water, r.wk.rail);
        let riding = self.riding;
        for (i, v) in self.list.iter_mut().enumerate() {
            let d = &r.vehicles[v.kind];
            let driven = riding == Some(i);
            let (px, pz) = (floord(v.body.pos[0]), floord(v.body.pos[2]));
            if !world.is_loaded(px, pz) {
                continue;
            }
            let at = |dy: f64| world.get(px, floord(v.body.pos[1] + dy), pz);
            // Throttle follows the driver; an empty vehicle coasts to a stop.
            let want = if driven { input.forward } else { 0.0 };
            let ease = if d.medium == Medium::Air { 0.6 } else { 3.0 };
            v.throttle += (want - v.throttle).clamp(-ease * dt, ease * dt);
            if driven && d.medium != Medium::Rail {
                // Turning needs way on, except in the air where you bank freely.
                let grip = if d.medium == Medium::Air { 1.0 } else { v.throttle.abs_().min(1.0).max(0.25) };
                v.body.yaw -= input.strafe * 1.9 * dt * grip * if v.throttle < 0.0 { -1.0 } else { 1.0 };
                if v.body.yaw > PI {
                    v.body.yaw -= 2.0 * PI;
                } else if v.body.yaw < -PI {
                    v.body.yaw += 2.0 * PI;
                }
            }
            let mut drive = Input { forward: v.throttle, ..Default::default() };
            match d.medium {
                Medium::Water => {
                    // Float: rise while the hull is under water, hold at the
                    // surface, fall if there is no water at all.
                    v.body.flying = true;
                    v.body.speed_scale = d.speed / 11.0;
                    let (in_water, on_water) = (at(0.35) == water, at(-0.15) == water);
                    drive.jump = in_water;
                    drive.descend = !in_water && !on_water;
                    if !in_water && !on_water {
                        drive.forward *= 0.15; // aground
                    }
                }
                Medium::Land => {
                    v.body.flying = false;
                    v.body.speed_scale = d.speed / 4.4;
                }
                Medium::Rail => {
                    // A cart only goes where there is track: ahead if it can,
                    // otherwise round whichever corner the track takes.
                    v.body.flying = true;
                    v.body.speed_scale = d.speed / 11.0;
                    let cell_y = floord(v.body.pos[1] + 0.3);
                    let on_track = |x: i32, z: i32| -> Option<i32> { [0, 1, -1].into_iter().map(|dy| cell_y + dy).find(|&y| world.get(x, y, z) == rail) };
                    let here = on_track(px, pz);
                    let quarter = crate::interact::facing(v.body.yaw) as i32;
                    let dirs = [(0, -1), (-1, 0), (0, 1), (1, 0)];
                    let sign = if v.throttle < 0.0 { 2 } else { 0 };
                    let mut next = None;
                    for turn in [0, 1, 3] {
                        let q = ((quarter + sign + turn) % 4) as usize;
                        if let Some(y) = on_track(px + dirs[q].0, pz + dirs[q].1) {
                            next = Some((q, y));
                            break;
                        }
                    }
                    match (here, next) {
                        (Some(y), Some((q, ny))) => {
                            // Face along the track (keeping the cart's own
                            // front when reversing) and stay on its centre line.
                            let facing = (q as i32 + sign) % 4;
                            v.body.yaw = facing as f32 * PI * 0.5;
                            if v.body.yaw > PI {
                                v.body.yaw -= 2.0 * PI;
                            }
                            let (cx, cz) = (px as f64 + 0.5, pz as f64 + 0.5);
                            let k = (dt * 8.0).min(1.0) as f64;
                            if dirs[q].0 == 0 {
                                v.body.pos[0] += (cx - v.body.pos[0]) * k;
                            } else {
                                v.body.pos[2] += (cz - v.body.pos[2]) * k;
                            }
                            let target = y.max(ny) as f64 + 0.13;
                            drive.jump = v.body.pos[1] < target - 0.04;
                            drive.descend = v.body.pos[1] > target + 0.1;
                        }
                        (Some(y), None) => {
                            // End of the line.
                            drive.forward = 0.0;
                            v.throttle = 0.0;
                            drive.descend = v.body.pos[1] > y as f64 + 0.23;
                        }
                        _ => {
                            // Off the rails: it just sits (and settles).
                            drive.forward = 0.0;
                            v.throttle = 0.0;
                            v.body.flying = false;
                        }
                    }
                }
                Medium::Air => {
                    // Below flying speed a plane is a slow cart; above it,
                    // it goes where the pilot looks.
                    let airborne = v.throttle > 0.55;
                    v.body.flying = airborne;
                    v.body.speed_scale = if airborne { d.speed / 11.0 } else { d.speed * 0.6 / 4.4 };
                    if airborne && driven {
                        drive.jump = look.1 > 0.12;
                        drive.descend = look.1 < -0.12;
                    } else if airborne {
                        drive.descend = true;
                    }
                }
            }
            v.body.update(dt, &drive, &|x, y, z| world.get_for_physics(x, y, z));
            v.phase += v.throttle * dt * 12.0;
            if v.phase > 2.0 * PI {
                v.phase -= 2.0 * PI;
            } else if v.phase < 0.0 {
                v.phase += 2.0 * PI;
            }
        }
        let _ = heading;
        riding.and_then(|i| self.list.get(i)).map(|v| {
            let seat = reg().vehicles[v.kind].seat as f64;
            [v.body.pos[0], v.body.pos[1] + seat, v.body.pos[2]]
        })
    }

    /// One line per vehicle: `id x y z yaw <storage>`.
    pub fn save_text(&self) -> String {
        let r = reg();
        let mut s = String::new();
        for v in &self.list {
            let p = v.body.pos;
            s.push_str(&alloc::format!("{} {} {} {} {} {}\n", r.vehicles[v.kind].id, p[0], p[1], p[2], v.body.yaw, v.storage.save()));
        }
        s
    }

    pub fn load_text(&mut self, text: &str) {
        let r = reg();
        for line in text.lines() {
            let mut w = line.splitn(6, ' ');
            let kind = w.next().and_then(|id| r.vehicles.iter().position(|v| v.id == id));
            let mut n = || w.next().and_then(|v| v.parse::<f64>().ok());
            if let (Some(kind), Some(x), Some(y), Some(z), Some(yaw)) = (kind, n(), n(), n(), n()) {
                let i = self.place(kind, [x, y, z], yaw as f32);
                self.list[i].storage = Inventory::load(w.next().unwrap_or(""));
            }
        }
    }
}

trait AbsF {
    fn abs_(self) -> f32;
}
impl AbsF for f32 {
    fn abs_(self) -> f32 {
        if self < 0.0 {
            -self
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::Pool;
    use crate::mesh::MeshData;
    use crate::world::{GpuMesh, MeshSink, NullStore};

    struct NoSink;
    impl MeshSink for NoSink {
        fn upload(&mut self, _: &MeshData) -> GpuMesh {
            GpuMesh::default()
        }
        fn free(&mut self, _: GpuMesh) {}
    }

    /// A loaded world with a stone platform at y = 120 around the origin.
    fn arena() -> World {
        let mut w = World::new(5);
        let mut pool = Pool::new(0, 5);
        for _ in 0..6000 {
            w.update(0, 0, 2, &mut pool, &mut NoSink, &mut NullStore);
            if w.settled {
                break;
            }
        }
        assert!(w.settled);
        for x in -24..40 {
            for z in -24..40 {
                w.set_block(x, 120, z, reg().wk.stone);
            }
        }
        w
    }

    fn kind(id: &str) -> usize {
        reg().vehicles.iter().position(|v| v.id == id).unwrap_or_else(|| panic!("no vehicle {id}"))
    }

    fn drive(v: &mut Vehicles, w: &World, forward: f32, strafe: f32, pitch: f32, seconds: f32) {
        let input = Input { forward, strafe, ..Default::default() };
        for _ in 0..(seconds * 60.0) as i32 {
            v.update(1.0 / 60.0, w, &input, (0.0, pitch));
        }
    }

    #[test]
    fn a_buggy_drives_forward_and_turns() {
        let w = arena();
        let mut v = Vehicles::default();
        let i = v.place(kind("buggy"), [8.5, 121.01, 20.5], 0.0);
        v.riding = Some(i);
        drive(&mut v, &w, 1.0, 0.0, 0.0, 2.0);
        let p = v.list[i].body.pos;
        assert!(p[2] < 12.0 && (p[0] - 8.5).abs() < 0.2, "did not drive north: {p:?}");
        assert!((p[1] - 121.0).abs() < 0.1);
        let yaw = v.list[i].body.yaw;
        drive(&mut v, &w, 1.0, 1.0, 0.0, 0.5);
        assert!(v.list[i].body.yaw < yaw - 0.3, "did not turn right");
        // Left alone it rolls to a stop.
        v.riding = None;
        drive(&mut v, &w, 1.0, 0.0, 0.0, 3.0);
        let a = v.list[i].body.pos;
        drive(&mut v, &w, 1.0, 0.0, 0.0, 1.0);
        let b = v.list[i].body.pos;
        assert!((a[0] - b[0]).abs() + (a[2] - b[2]).abs() < 0.05);
    }

    #[test]
    fn a_boat_floats_on_water_and_a_cart_follows_the_track_round_a_bend() {
        let mut w = arena();
        let (water, rail) = (reg().wk.water, reg().wk.rail);
        // A pool: water two deep in a basin dug into the platform.
        for x in 0..10 {
            for z in 0..10 {
                w.set_block(x, 120, z, water);
                w.set_block(x, 119, z, water);
                w.set_block(x, 118, z, reg().wk.stone);
            }
        }
        let mut v = Vehicles::default();
        let boat = v.place(kind("boat"), [5.5, 123.0, 8.5], 0.0);
        drive(&mut v, &w, 0.0, 0.0, 0.0, 3.0);
        let y = v.list[boat].body.pos[1];
        assert!(y > 120.4 && y < 121.2, "the boat does not float at the surface: y = {y}");
        v.riding = Some(boat);
        drive(&mut v, &w, 1.0, 0.0, 0.0, 1.0);
        assert!(v.list[boat].body.pos[2] < 7.5, "the boat did not move");

        // Track north along x = 20 from z = 30 to 22, then west to x = 14.
        for z in 22..=30 {
            w.set_block(20, 121, z, rail);
        }
        for x in 14..20 {
            w.set_block(x, 121, 22, rail);
        }
        let cart = v.place(kind("cart"), [20.5, 121.2, 29.5], 0.0);
        v.riding = Some(cart);
        drive(&mut v, &w, 1.0, 0.0, 0.0, 6.0);
        let p = v.list[cart].body.pos;
        assert!(p[0] < 16.0 && (p[2] - 22.5).abs() < 0.3, "the cart did not take the bend: {p:?}");
        // It stops at the end of the line instead of running off.
        drive(&mut v, &w, 1.0, 0.0, 0.0, 4.0);
        assert!(v.list[cart].body.pos[0] > 13.9, "the cart left the track");
    }

    #[test]
    fn a_plane_takes_off_when_the_pilot_looks_up() {
        let w = arena();
        let mut v = Vehicles::default();
        let i = v.place(kind("plane"), [8.5, 121.01, 30.5], 0.0);
        v.riding = Some(i);
        drive(&mut v, &w, 1.0, 0.0, 0.4, 5.0);
        let p = v.list[i].body.pos;
        assert!(p[1] > 126.0, "the plane never left the ground: {p:?}");
        assert!(p[2] < 10.0, "the plane did not fly forward: {p:?}");
    }

    #[test]
    fn vehicles_survive_a_save_with_their_cargo() {
        let mut v = Vehicles::default();
        let i = v.place(kind("cargo_boat"), [1.5, 60.0, -2.5], 1.25);
        assert!(reg().vehicles[v.list[i].kind].storage);
        v.list[i].storage.add(0, 12);
        v.place(kind("plane"), [9.0, 70.0, 9.0], 0.0);
        let text = v.save_text();
        let mut again = Vehicles::default();
        again.load_text(&text);
        assert_eq!(again.list.len(), 2);
        assert_eq!(again.list[0].body.pos, [1.5, 60.0, -2.5]);
        assert_eq!(again.list[0].storage, v.list[0].storage);
        assert_eq!(again.save_text(), text);
        // Removing one keeps the rider on the right vehicle.
        again.riding = Some(1);
        again.remove(0);
        assert_eq!(again.riding, Some(0));
    }
}
