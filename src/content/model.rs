//! Jointed box models (loaded from Blockbench files) and their posing.
//!
//! Models are posed on the CPU each frame: a few dozen boxes per creature
//! is nothing even for an old machine, and it keeps the renderer to plain
//! textured quads.

use crate::bbmodel::{self, Model};
use crate::content::pose::{self, Role, Walk};
use crate::math::{cosf, sinf, PI};
use alloc::string::String;
use alloc::vec::Vec;

/// A 3x4 affine transform: rotation/scale in the first three columns,
/// translation in the fourth.
type M34 = [[f32; 4]; 3];

const IDENT: M34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

fn mul(a: &M34, b: &M34) -> M34 {
    let mut m = [[0.0f32; 4]; 3];
    for r in 0..3 {
        for c in 0..4 {
            m[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c] + if c == 3 { a[r][3] } else { 0.0 };
        }
    }
    m
}

fn apply(m: &M34, p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

/// Rotation by Blockbench Euler angles (degrees) about a pivot.
fn rotate_about(pivot: [f32; 3], deg: [f32; 3]) -> M34 {
    if deg == [0.0, 0.0, 0.0] {
        return IDENT;
    }
    let (rx, ry, rz) = (deg[0] * PI / 180.0, deg[1] * PI / 180.0, deg[2] * PI / 180.0);
    let (sx, cx, sy, cy, sz, cz) = (sinf(rx), cosf(rx), sinf(ry), cosf(ry), sinf(rz), cosf(rz));
    // R = Rz * Ry * Rx, the order Blockbench uses.
    let r = [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ];
    let mut m = IDENT;
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j];
        }
        // p' = R (p - pivot) + pivot
        m[i][3] = pivot[i] - (r[i][0] * pivot[0] + r[i][1] * pivot[1] + r[i][2] * pivot[2]);
    }
    m
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelVert {
    pub pos: [f32; 3],
    /// 0..1 across the model's texture.
    pub uv: [f32; 2],
    /// Brightness of the face this vertex belongs to, 0..255.
    pub shade: u8,
}

/// How a bone moves when the creature walks.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Swing {
    None,
    /// Swings with this sign (legs and arms on opposite sides alternate).
    Limb(f32),
    Head,
    /// A tail, or the n-th piece of a long one.
    Tail(f32),
    /// A wing on this side (+1 left, -1 right).
    Wing(f32),
    Ear(f32),
    Body,
    /// The lower half of a leg (bends at the knee), same side sign as its leg.
    Shin(f32),
    /// The lower half of an arm (bends at the elbow).
    Forearm(f32),
}

pub struct ModelDef {
    pub id: String,
    pub model: Model,
    swing: Vec<Swing>,
    /// What each bone is to a posed body, its lower-case name (for emotes),
    /// and what the model is built like (see content/pose).
    roles: Vec<Role>,
    names: Vec<String>,
    rig: pose::Rig,
    /// OpenGL texture, set by the renderer (0 = plain white).
    pub tex: u32,
}

/// What a creature is doing, as far as its looks go.
#[derive(Clone, Copy, Default, Debug)]
pub struct Pose {
    /// Walk cycle position, radians.
    pub phase: f32,
    /// 0 standing still .. 1 full stride.
    pub stride: f32,
    /// Where the head looks relative to the body, degrees.
    pub head_pitch: f32,
    pub head_yaw: f32,
    /// The creature's way of moving.
    pub anim: crate::block::Anim,
    /// Seconds it has been alive: tails wag and ears twitch on their own.
    pub clock: f32,
    /// Its posture, what it does with its arms, the emote it plays (content/pose).
    pub body: pose::Body,
}

fn classify(name: &str) -> Swing {
    let n = name.to_ascii_lowercase();
    if n == "head" {
        return Swing::Head;
    }
    let side = |n: &str| if n.ends_with("-l") || n.ends_with("_l") || n.contains("left") { 1.0 } else { -1.0 };
    if n.starts_with("tail") {
        // "tail", then "tail-a", "tail-b"... further and further along.
        let piece = n.as_bytes().last().copied().filter(|c| n.len() == 6 && (b'a'..=b'h').contains(c)).map_or(0.0, |c| (c - b'a') as f32);
        return Swing::Tail(piece);
    }
    if n.starts_with("wing") {
        return Swing::Wing(side(&n));
    }
    if n.starts_with("ear") {
        return Swing::Ear(side(&n));
    }
    if n == "body" {
        return Swing::Body;
    }
    if n.starts_with("shin") || n.starts_with("forearm") {
        // The same side-and-pair sign as the limb it hangs from.
        let limb = alloc::format!("leg{}", &n[if n.starts_with("shin") { 4 } else { 7 }..]);
        let sign = match classify(&limb) {
            Swing::Limb(s) => s,
            _ => 1.0,
        };
        return if n.starts_with("shin") { Swing::Shin(sign) } else { Swing::Forearm(-sign) };
    }
    let leg = n.contains("thigh") || (n.contains("leg") && !n.contains("lower"));
    let arm = n.contains("arm") && !n.contains("forearm");
    if !leg && !arm {
        return Swing::None;
    }
    // Side from names like "L-Arm", "Left Leg", "Leg-FL", "leg_br".
    let tail = n.rsplit(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
    // Many-legged creatures number theirs: "Leg-L1", "Leg-R3"; odd and
    // even pairs step in turn.
    let numbered = tail.len() == 2 && tail.as_bytes()[1].is_ascii_digit() && (tail.starts_with('l') || tail.starts_with('r'));
    if numbered {
        let odd = tail.as_bytes()[1] % 2 == 1;
        return Swing::Limb(if tail.starts_with('l') == odd { 1.0 } else { -1.0 });
    }
    let left = n.starts_with("l-") || n.starts_with("l_") || n.starts_with("left") || tail == "l" || tail == "fl" || tail == "bl";
    let back = tail == "bl" || tail == "br" || n.contains("back") || n.contains("hind");
    let mut sign = if left { 1.0 } else { -1.0 };
    if back {
        sign = -sign; // four-legged walkers move diagonal pairs together
    }
    if arm {
        sign = -sign; // arms swing against the legs
    }
    Swing::Limb(sign)
}

/// Which worn slot covers a bone: 0 head, 1 chest (body and arms), 2 legs
/// (thighs and the belt), 3 feet (calves and feet).
pub fn armor_slot_of(role: Role) -> Option<usize> {
    match role {
        Role::Head | Role::Neck => Some(0),
        Role::Chest | Role::Torso | Role::Belly | Role::Arm(_) | Role::Forearm(_) | Role::Hand(_) => Some(1),
        Role::Pelvis | Role::Thigh(_) => Some(2),
        Role::Calf(_) | Role::Foot(_) => Some(3),
        Role::Other => None,
    }
}

impl ModelDef {
    pub fn from_bbmodel(id: &str, text: &str) -> Result<ModelDef, String> {
        let model = bbmodel::load(text)?;
        if model.cubes.is_empty() {
            return Err(String::from("the model has no cubes"));
        }
        let swing = model.bones.iter().map(|b| classify(&b.name)).collect();
        let roles: Vec<Role> = model.bones.iter().map(|b| pose::classify(&b.name)).collect();
        let names = model.bones.iter().map(|b| b.name.to_ascii_lowercase()).collect();
        let hips: Vec<f32> = model.bones.iter().zip(&roles).filter(|(_, r)| matches!(r, Role::Thigh(_))).map(|(b, _)| b.origin[1] - model.min[1]).collect();
        let rig = pose::rig_of(&roles, &hips, (model.max[1] - model.min[1]).max(0.001));
        Ok(ModelDef { id: String::from(id), model, swing, roles, names, rig, tex: 0 })
    }

    /// Height of the model in its own units.
    pub fn height(&self) -> f32 {
        (self.model.max[1] - self.model.min[1]).max(0.001)
    }

    /// Emit the posed model as quads (four vertices each). The model is
    /// scaled so that it stands `height` tall, turned by `yaw` (same
    /// convention as the player: 0 faces -z) and placed with its feet at
    /// `at`. `flip_y` mirrors it vertically, for drawing in screen space.
    pub fn quads(&self, pose: &Pose, at: [f32; 3], yaw: f32, height: f32, flip_y: bool, out: &mut Vec<ModelVert>) {
        self.quads_inner(pose, None, None, at, yaw, height, flip_y, None, out);
    }

    /// The layer of armour worn in `slot` (0 head, 1 chest, 2 legs, 3 feet):
    /// copies of the cubes of the bones that slot covers, a little bigger
    /// than the body, posed the same. Drawn with the armour's own picture
    /// (laid out like a skin) or a plain colour. Bounded: no more quads than
    /// the body itself has.
    pub fn armor_quads(&self, slot: usize, pose: &Pose, at: [f32; 3], yaw: f32, height: f32, flip_y: bool, out: &mut Vec<ModelVert>) {
        // About 0.6 of a skin pixel (the player is 32 pixels tall).
        let grow = self.height() * (0.6 / 32.0);
        self.quads_inner(pose, None, None, at, yaw, height, flip_y, Some((slot, grow)), out);
    }

    /// The place of the bone called `name` (any case) in this model's bone
    /// list, for [`crate::bone_pose::BoneTable`]. The first, if two share a name.
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n.eq_ignore_ascii_case(name))
    }

    /// The names of the bones, as the model file gives them.
    pub fn bone_names(&self) -> Vec<String> {
        self.model.bones.iter().map(|b| b.name.clone()).collect()
    }

    /// As [`ModelDef::quads`], with a creature's own say on top of the
    /// animation: `bones` turn, move and scale single bones about their
    /// pivots (children follow), and `orient` is (pitch, roll) in degrees for
    /// the whole model about its centre (pitch up lifts the nose, roll leans
    /// the top to the creature's left).
    pub fn quads_with(&self, pose: &Pose, bones: Option<&crate::bone_pose::BoneTable>, orient: Option<(f32, f32)>, at: [f32; 3], yaw: f32, height: f32, flip_y: bool, out: &mut Vec<ModelVert>) {
        self.quads_inner(pose, bones, orient, at, yaw, height, flip_y, None, out);
    }

    fn quads_inner(&self, pose: &Pose, bones: Option<&crate::bone_pose::BoneTable>, orient: Option<(f32, f32)>, at: [f32; 3], yaw: f32, height: f32, flip_y: bool, armor: Option<(usize, f32)>, out: &mut Vec<ModelVert>) {
        let m = &self.model;
        let scale = height / self.height();
        let (sy, cy) = (sinf(yaw), cosf(yaw));
        let ys = if flip_y { -scale } else { scale };
        // Model space -> world: centre on x/z, feet at y = 0, scale, turn, move.
        let (ox, oy, oz) = ((m.min[0] + m.max[0]) * 0.5, m.min[1], (m.min[2] + m.max[2]) * 0.5);
        let root: M34 = [
            [cy * scale, 0.0, sy * scale, at[0] - (cy * ox + sy * oz) * scale],
            [0.0, ys, 0.0, at[1] - oy * ys],
            [-sy * scale, 0.0, cy * scale, at[2] - (-sy * ox + cy * oz) * scale],
        ];
        use crate::block::Anim;
        let a = pose.anim;
        // Humanoids may sit, lie, swim...: the still postures replace their walk.
        let body = pose.body;
        let humanoid = self.rig.humanoid;
        let st = if humanoid { pose.stride * body.walk_weight() } else { pose.stride };
        let (ph, ck) = (pose.phase, pose.clock);
        let walk = Walk { phase: ph, stride: st, clock: ck };
        // The whole body, turned and lowered for lying, swimming, sitting...
        let base = if humanoid {
            let r = pose::root(&body, &self.rig);
            if r.turn != 0.0 || r.shift != [0.0; 3] {
                let turned = rotate_about([ox, m.min[1] + self.rig.hip, oz], [r.turn, 0.0, 0.0]);
                let moved: M34 = [[1.0, 0.0, 0.0, r.shift[0]], [0.0, 1.0, 0.0, r.shift[1]], [0.0, 0.0, 1.0, r.shift[2]]];
                mul(&root, &mul(&moved, &turned))
            } else {
                root
            }
        } else {
            root
        };
        let base = match orient {
            Some((pitch, roll)) if (pitch != 0.0 || roll != 0.0) && pitch.is_finite() && roll.is_finite() => {
                let centre = [ox, (m.min[1] + m.max[1]) * 0.5, oz];
                mul(&base, &rotate_about(centre, [pitch, 0.0, roll]))
            }
            _ => base,
        };
        let reach = match a {
            Anim::Walk => 38.0,
            Anim::Hop => 34.0,
            Anim::Slither => 0.0,
            Anim::Flap => 8.0,
            Anim::Scuttle => 50.0,
            Anim::Waddle => 26.0,
            Anim::Float => 16.0,
        };
        let swing_deg = sinf(if a == Anim::Scuttle { ph * 2.0 } else { ph }) * reach * st;
        let mut bone_m: Vec<M34> = Vec::with_capacity(m.bones.len());
        for (i, b) in m.bones.iter().enumerate() {
            let mut rot = b.rotation;
            match self.swing[i] {
                // Hoppers push off with all their legs at once.
                Swing::Limb(sign) => rot[0] += if a == Anim::Hop { swing_deg } else { sign * swing_deg },
                Swing::Head => {
                    rot[0] += pose.head_pitch + if a == Anim::Waddle { sinf(ph * 2.0) * 7.0 * st } else { 0.0 };
                    rot[1] += pose.head_yaw;
                }
                Swing::Tail(piece) => {
                    rot[1] += match a {
                        // The wave runs down the body, tail last.
                        Anim::Slither => sinf(ph * 2.0 + ck * 1.5 - piece * 1.1) * 24.0 * (0.35 + st),
                        Anim::Float => sinf(ph - piece) * 22.0,
                        // A wag: livelier on the move.
                        _ => sinf(ck * 3.4 - piece * 0.9) * (7.0 + 9.0 * st),
                    };
                }
                Swing::Wing(sign) => {
                    rot[2] += sign * if a == Anim::Flap { sinf(ck * 19.0) * 52.0 * st.max(0.25) + 8.0 } else { sinf(ck * 2.1) * 4.0 };
                }
                Swing::Ear(sign) => {
                    // A flick now and then.
                    let t = sinf(ck * 1.7 + sign);
                    if t > 0.92 {
                        rot[2] += sign * (t - 0.92) * 200.0;
                    }
                }
                Swing::Body => match a {
                    Anim::Waddle => rot[2] += sinf(ph) * 7.0 * st,
                    Anim::Hop => rot[0] -= sinf(ph) * sinf(ph) * 9.0 * st,
                    Anim::Slither => rot[1] += sinf(ph * 2.0 + ck * 1.5) * 9.0 * (0.35 + st),
                    Anim::Float => rot[0] += sinf(ck * 1.5) * 4.0,
                    _ => {}
                },
                // A knee folds as its leg comes forward and straightens as
                // it pushes back; elbows keep a slight bend and swing a little.
                Swing::Shin(sign) => {
                    let lift = if a == Anim::Hop { sinf(ph) } else { sign * sinf(if a == Anim::Scuttle { ph * 2.0 } else { ph } + 0.9) };
                    rot[0] += lift.max(0.0) * reach * 0.95 * st;
                }
                Swing::Forearm(sign) => {
                    rot[0] -= (8.0 + (sign * sinf(ph)).max(0.0) * reach * 0.8) * st.max(0.15);
                }
                Swing::None => {}
            }
            if humanoid {
                pose::apply(self.roles[i], self.swing[i] != Swing::None, &body, &walk, &mut rot);
            }
            if body.emote != 0 {
                pose::emote::add(&self.names[i], &body, &mut rot);
            }
            let over = bones.and_then(|t| t.get(i));
            if let Some(e) = over {
                let r = e.rot_deg();
                rot = [rot[0] + r[0], rot[1] + r[1], rot[2] + r[2]];
            }
            let mut local = rotate_about(b.origin, rot);
            if let Some(e) = over {
                // Scaled about the pivot, then moved: p' = s (p - pivot) + pivot + offset.
                let (s, o) = (e.scale_f(), e.off_units());
                if s != 1.0 || o != [0.0; 3] {
                    let t: M34 = [
                        [s, 0.0, 0.0, o[0] + b.origin[0] * (1.0 - s)],
                        [0.0, s, 0.0, o[1] + b.origin[1] * (1.0 - s)],
                        [0.0, 0.0, s, o[2] + b.origin[2] * (1.0 - s)],
                    ];
                    local = mul(&t, &local);
                }
            }
            let parent = match b.parent {
                Some(p) if p < bone_m.len() => bone_m[p],
                _ => base,
            };
            bone_m.push(mul(&parent, &local));
        }
        for (bi, b) in m.bones.iter().enumerate() {
            // An armour layer is drawn only for the bones its slot covers.
            if let Some((slot, _)) = armor {
                if !self.rig.humanoid || armor_slot_of(self.roles[bi]) != Some(slot) {
                    continue;
                }
            }
            for &ci in &b.cubes {
                let c = match m.cubes.get(ci) {
                    Some(c) => c,
                    None => continue,
                };
                let t = mul(&bone_m[bi], &rotate_about(c.origin, c.rotation));
                let g = armor.map_or(0.0, |a| a.1);
                let (lo, hi) = ([c.from[0] - g, c.from[1] - g, c.from[2] - g], [c.to[0] + g, c.to[1] + g, c.to[2] + g]);
                // Corners of each face as seen from outside: top-left,
                // top-right, bottom-right, bottom-left. Faces are in the
                // loader's order: east, west, up, down, south, north.
                let faces: [([[f32; 3]; 4], u8); 6] = [
                    ([[hi[0], hi[1], hi[2]], [hi[0], hi[1], lo[2]], [hi[0], lo[1], lo[2]], [hi[0], lo[1], hi[2]]], 200),
                    ([[lo[0], hi[1], lo[2]], [lo[0], hi[1], hi[2]], [lo[0], lo[1], hi[2]], [lo[0], lo[1], lo[2]]], 200),
                    ([[lo[0], hi[1], lo[2]], [hi[0], hi[1], lo[2]], [hi[0], hi[1], hi[2]], [lo[0], hi[1], hi[2]]], 255),
                    ([[lo[0], lo[1], hi[2]], [hi[0], lo[1], hi[2]], [hi[0], lo[1], lo[2]], [lo[0], lo[1], lo[2]]], 140),
                    ([[lo[0], hi[1], hi[2]], [hi[0], hi[1], hi[2]], [hi[0], lo[1], hi[2]], [lo[0], lo[1], hi[2]]], 228),
                    ([[hi[0], hi[1], lo[2]], [lo[0], hi[1], lo[2]], [lo[0], lo[1], lo[2]], [hi[0], lo[1], lo[2]]], 228),
                ];
                for (f, (corners, shade)) in faces.iter().enumerate() {
                    let (uv, turns) = match c.faces[f] {
                        Some(x) => x,
                        None => continue,
                    };
                    let (u0, v0, u1, v1) = (uv[0] / m.tex_w, uv[1] / m.tex_h, uv[2] / m.tex_w, uv[3] / m.tex_h);
                    let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
                    for k in 0..4 {
                        out.push(ModelVert { pos: apply(&t, corners[k]), uv: uvs[(k + 4 - (turns as usize % 4)) % 4], shade: *shade });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knees_and_elbows_bend_with_the_stride() {
        assert_eq!(classify("Shin-FL"), Swing::Shin(match classify("Leg-FL") {
            Swing::Limb(s) => s,
            _ => panic!("Leg-FL is a limb"),
        }));
        assert!(matches!(classify("Forearm-L"), Swing::Forearm(_)));
        assert!(matches!(classify("tail-b"), Swing::Tail(k) if k == 1.0));
        // A model with jointed legs: walking moves it, and nothing goes wild.
        // (The cow is the wildlife mod's: with no shipped mods in this build there is no model to try.)
        let Some(cow_file) = crate::native::shipped().iter().find(|f| f.0 == "mods/wildlife/models/cow.bbmodel") else {
            eprintln!("SKIPPED: no wildlife mod in this build (set VOXELRUST_MODS_DIR)");
            return;
        };
        let text = String::from_utf8_lossy(cow_file.1).into_owned();
        let cow = ModelDef::from_bbmodel("cow", &text).unwrap();
        assert!(cow.model.bones.iter().any(|b| b.name.to_ascii_lowercase().starts_with("shin")), "the cow has knees");
        let (mut still, mut walking) = (Vec::new(), Vec::new());
        cow.quads(&Pose::default(), [0.0; 3], 0.0, 1.4, false, &mut still);
        cow.quads(&Pose { phase: 1.0, stride: 1.0, ..Default::default() }, [0.0; 3], 0.0, 1.4, false, &mut walking);
        assert_eq!(still.len(), walking.len());
        assert!(still.iter().zip(&walking).any(|(a, b)| a.pos != b.pos));
        assert!(walking.iter().all(|v| v.pos.iter().all(|c| c.is_finite() && c.abs() < 4.0)));
    }

    const PLAYER: &str = include_str!("../../data/models/player.bbmodel");
    const PLAYER_SLIM: &str = include_str!("../../data/models/player_slim.bbmodel");

    fn bounds(v: &[ModelVert]) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in v {
            for i in 0..3 {
                lo[i] = lo[i].min(p.pos[i]);
                hi[i] = hi[i].max(p.pos[i]);
            }
        }
        (lo, hi)
    }

    #[test]
    fn slim_arms_are_narrower_and_keep_their_uvs_on_the_atlas() {
        let classic = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        let slim = ModelDef::from_bbmodel("player_slim", PLAYER_SLIM).unwrap();
        let mut a = Vec::new();
        let mut b = Vec::new();
        classic.quads(&Pose::default(), [0.0, 0.0, 0.0], 0.0, 1.8, false, &mut a);
        slim.quads(&Pose::default(), [0.0, 0.0, 0.0], 0.0, 1.8, false, &mut b);
        let ((lo_a, hi_a), (lo_b, hi_b)) = (bounds(&a), bounds(&b));
        // The outer edge of each arm (and its sleeve) moves in by one skin
        // pixel: 3.56 model units, which is 1.8 / 113.92 of the height per unit.
        let pixel = 3.56 * 1.8 / 113.92;
        assert!(hi_a[0] - hi_b[0] > 0.9 * pixel && lo_b[0] - lo_a[0] > 0.9 * pixel, "{lo_a:?} {hi_a:?} / {lo_b:?} {hi_b:?}");
        assert!((hi_a[1] - hi_b[1]).abs() < 1e-3 && (lo_a[1] - lo_b[1]).abs() < 1e-3, "only the width changes");
        assert!(b.iter().all(|p| (0.0..=1.0).contains(&p.uv[0]) && (0.0..=1.0).contains(&p.uv[1])));
    }

    #[test]
    fn player_model_stands_at_the_requested_height() {
        let m = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        let mut v = Vec::new();
        m.quads(&Pose::default(), [10.0, 64.0, -5.0], 0.0, 1.8, false, &mut v);
        assert_eq!(v.len() % 4, 0);
        assert!(v.len() >= 43 * 4, "expected at least a face per cube, got {} vertices", v.len());
        let (lo, hi) = bounds(&v);
        assert!((lo[1] - 64.0).abs() < 1e-3 && (hi[1] - 65.8).abs() < 1e-3, "{lo:?} {hi:?}");
        // Centred on the given spot, and narrower than it is tall.
        assert!((lo[0] + hi[0] - 20.0).abs() < 0.02 && hi[0] - lo[0] < 1.2);
        assert!(v.iter().all(|p| (0.0..=1.0).contains(&p.uv[0]) && (0.0..=1.0).contains(&p.uv[1])));
    }

    #[test]
    fn turning_and_walking_move_the_right_parts() {
        let m = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        let (mut still, mut turned, mut walking) = (Vec::new(), Vec::new(), Vec::new());
        m.quads(&Pose::default(), [0.0; 3], 0.0, 1.8, false, &mut still);
        m.quads(&Pose::default(), [0.0; 3], PI * 0.5, 1.8, false, &mut turned);
        m.quads(&Pose { phase: PI * 0.5, stride: 1.0, ..Default::default() }, [0.0; 3], 0.0, 1.8, false, &mut walking);
        // A quarter turn swaps the footprint's width and depth.
        let (a, b) = (bounds(&still), bounds(&turned));
        assert!(((a.1[0] - a.0[0]) - (b.1[2] - b.0[2])).abs() < 1e-3);
        // Walking swings the legs forward and back: the model gets deeper,
        // and the head stays about where it was (the torso only sways a little).
        let (w, s) = (bounds(&walking), bounds(&still));
        assert!(w.1[2] - w.0[2] > (s.1[2] - s.0[2]) + 0.2, "legs did not swing");
        assert!((w.1[1] - s.1[1]).abs() < 0.03);
        // The two legs move in opposite directions.
        assert!(w.0[2] < s.0[2] - 0.1 && w.1[2] > s.1[2] + 0.1);
    }

    #[test]
    fn postures_change_the_shape_of_the_player() {
        use crate::content::pose::{Body, Kind};
        let m = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        assert!(m.rig.humanoid && m.rig.hip > 40.0 && m.rig.hip < 45.0);
        let draw = |body: Body| {
            let mut v = Vec::new();
            m.quads(&Pose { body, ..Default::default() }, [0.0; 3], 0.0, 1.8, false, &mut v);
            assert!(v.iter().all(|p| p.pos.iter().all(|c| c.is_finite())));
            bounds(&v)
        };
        let stand = draw(Body::default());
        // Sitting: lower, with the thighs out in front.
        let sit = draw(Body::steady(Kind::Sit));
        assert!(sit.1[1] < stand.1[1] - 0.4 && sit.1[1] > 1.0, "{sit:?}");
        assert!(sit.0[2] < stand.0[2] - 0.12, "{sit:?}");
        // Lying: flat on the bed, as long as the body is tall.
        let lie = draw(Body::steady(Kind::Sleep));
        assert!(lie.1[1] < 0.6 && lie.0[1] > -0.1, "{lie:?}");
        assert!(lie.1[2] - lie.0[2] > 1.5, "{lie:?}");
        // Crouching: lower than standing.
        let crouch = draw(Body::steady(Kind::Crouch));
        assert!(crouch.1[1] < stand.1[1] - 0.15, "{crouch:?}");
        // Swimming: horizontal too.
        let swim = draw(Body::steady(Kind::Swim));
        assert!(swim.1[2] - swim.0[2] > 1.2 && swim.1[1] - swim.0[1] < 1.3, "{swim:?}");
        // A creature that is not built like a person does not take the poses.
        let cow_text = String::from_utf8_lossy(crate::block::embedded::file("models/grazer.bbmodel").expect("grazer model")).into_owned();
        let grazer = ModelDef::from_bbmodel("grazer", &cow_text).unwrap();
        assert!(!grazer.rig.humanoid);
    }

    #[test]
    fn bone_overrides_and_tilt_move_what_they_say() {
        use crate::bone_pose::{BoneEntry, BoneTable};
        let m = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        let draw = |t: Option<&BoneTable>, orient: Option<(f32, f32)>| {
            let mut v = Vec::new();
            m.quads_with(&Pose::default(), t, orient, [0.0; 3], 0.0, 1.8, false, &mut v);
            assert!(v.iter().all(|p| p.pos.iter().all(|c| c.is_finite())));
            v
        };
        let plain = draw(None, None);
        let head = m.bone_index("head").expect("the player has a head");
        assert_eq!(m.bone_index("HEAD"), Some(head), "names match in any case");
        assert!(m.bone_index("no such bone").is_none());
        assert_eq!(m.bone_names().len(), m.model.bones.len());
        // Nothing set: exactly the animation.
        assert_eq!(draw(Some(&BoneTable::default()), None), plain);
        assert_eq!(draw(None, Some((0.0, 0.0))), plain);
        // The head moved 20 model units up: the top of the body rises, the feet stay.
        let mut t = BoneTable::default();
        t.set(BoneEntry::new(head, [0.0; 3], [0.0, 20.0, 0.0], 1.0).unwrap());
        let (a, b) = (bounds(&plain), bounds(&draw(Some(&t), None)));
        assert!(b.1[1] > a.1[1] + 0.2 && (b.0[1] - a.0[1]).abs() < 1e-3, "{a:?} {b:?}");
        // Scaled about its pivot: the head grows, so does the whole.
        let mut t = BoneTable::default();
        t.set(BoneEntry::new(head, [0.0; 3], [0.0; 3], 3.0).unwrap());
        let b = bounds(&draw(Some(&t), None));
        assert!(b.1[1] > a.1[1] + 0.1 && b.1[0] - b.0[0] >= a.1[0] - a.0[0], "{a:?} {b:?}");
        // Turned on top of the animation: the head leaves its place.
        let mut t = BoneTable::default();
        t.set(BoneEntry::new(head, [90.0, 0.0, 0.0], [0.0; 3], 1.0).unwrap());
        let turned = draw(Some(&t), None);
        assert_eq!(turned.len(), plain.len());
        assert!(turned.iter().zip(&plain).any(|(p, q)| p.pos != q.pos));
        // Pitched a quarter turn about its centre: it lies down along z.
        let lying = bounds(&draw(None, Some((90.0, 0.0))));
        assert!(lying.1[1] - lying.0[1] < 1.0 && lying.1[2] - lying.0[2] > 1.5, "{lying:?}");
        // Rolled: it lies along x.
        let rolled = bounds(&draw(None, Some((0.0, 90.0))));
        assert!(rolled.1[1] - rolled.0[1] < 1.0 && rolled.1[0] - rolled.0[0] > 1.5, "{rolled:?}");
        // A tilt that is not a number is ignored.
        assert_eq!(draw(None, Some((f32::NAN, 0.0))), plain);
    }

    #[test]
    fn limb_names_are_recognised() {
        assert_eq!(classify("Head"), Swing::Head);
        assert_eq!(classify("L-Thigh"), Swing::Limb(1.0));
        assert_eq!(classify("R-Thigh"), Swing::Limb(-1.0));
        assert_eq!(classify("L-Arm"), Swing::Limb(-1.0));
        assert_eq!(classify("L-Forearm"), Swing::None);
        assert_eq!(classify("Leg-FL"), Swing::Limb(1.0));
        assert_eq!(classify("Leg-BL"), Swing::Limb(-1.0));
        assert_eq!(classify("Leg-BR"), Swing::Limb(1.0));
        assert_eq!(classify("Chest"), Swing::None);
        assert!(ModelDef::from_bbmodel("x", "{}").is_err());
    }

    #[test]
    fn an_armour_layer_is_a_bigger_copy_of_the_cubes_of_its_slot() {
        let m = ModelDef::from_bbmodel("player", PLAYER).unwrap();
        let (mut body, mut head, mut chest, mut legs, mut feet) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let pose = Pose::default();
        m.quads(&pose, [0.0; 3], 0.0, 1.8, false, &mut body);
        for (slot, out) in [(0, &mut head), (1, &mut chest), (2, &mut legs), (3, &mut feet)] {
            m.armor_quads(slot, &pose, [0.0; 3], 0.0, 1.8, false, out);
            assert!(!out.is_empty(), "slot {} covers nothing", slot);
            assert!(out.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
        }
        // Every cube is in one slot only: the layers together are no more than the body.
        assert!(head.len() + chest.len() + legs.len() + feet.len() <= body.len());
        // The head layer is bigger than the head: it reaches higher than the body does.
        let top = |v: &Vec<ModelVert>| v.iter().map(|q| q.pos[1]).fold(f32::MIN, f32::max);
        assert!(top(&head) > top(&body));
        assert_eq!(armor_slot_of(Role::Head), Some(0));
        assert_eq!(armor_slot_of(Role::Foot(1)), Some(3));
        assert_eq!(armor_slot_of(Role::Other), None);
    }
}
