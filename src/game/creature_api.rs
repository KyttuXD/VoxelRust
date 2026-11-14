//! Changing a living creature from outside the game's own rules: its size and
//! health, bone overrides, tilt and state text. The script calls of
//! `script/ext/mobs_model.rs` and native mods (`g.mobs.set_bone(...)`) both
//! come here, so the limits are the same for both: sizes and health inside
//! `creature_size`, 64 bone overrides a creature, 256 creatures with bone
//! overrides, a state of 64 bytes. A bad value never panics; it is clamped or
//! refused.
//!
//! Creatures are named by their `id`, which stays the same while they live.

use crate::block::reg;
use crate::bone_pose::{orient_tenths, BoneEntry};
use crate::creature_extra::{clean_state, Extra, MAX_WITH_BONES};
use crate::creature_size::{clamp_current, clamp_health, clamp_size};
use crate::entity::{Mob, Mobs};
use crate::world::World;

/// What `Mobs::set_bone` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoneResult {
    Set,
    /// No creature has this id.
    NoCreature,
    /// The creature's model has no bone of that name (logged once, ignored).
    UnknownBone,
    /// The creature already overrides 64 other bones.
    TableFull,
    /// 256 other creatures already have bone overrides.
    TooManyCreatures,
    /// A number was not finite.
    BadNumber,
}

impl Mob {
    /// The size its body has when nothing else sets it: its kind's (and
    /// variant's), or what a script gave it; a young one is smaller.
    pub fn natural_size(&self) -> (f64, f64) {
        if let Some((w, h)) = self.extra.as_ref().and_then(|e| e.size) {
            return (w as f64, h as f64);
        }
        let d = &reg().entities[self.kind];
        let size = (self.variant as usize).checked_sub(1).and_then(|i| d.x.variants.get(i)).map_or(1.0, |v| v.size);
        let baby = match d.x.baby {
            Some((scale, _)) if self.young > 0.0 => scale,
            _ => 1.0,
        };
        ((d.width * size * baby) as f64, (d.height * size * baby) as f64)
    }

    /// The most health it can have: its kind's, or what a script set.
    pub fn max_health(&self) -> i32 {
        self.extra.as_ref().and_then(|e| e.max_hp).unwrap_or(reg().entities[self.kind].health)
    }

    /// Bone overrides and tilt for drawing: `None` for a creature that has
    /// neither, which is nearly all of them.
    pub fn bone_view(&self) -> Option<(&crate::bone_pose::BoneTable, Option<(f32, f32)>)> {
        let e = self.extra.as_ref()?;
        if e.bones.is_empty() && e.pitch == 0 && e.roll == 0 {
            return None;
        }
        Some((&e.bones, e.orient_deg()))
    }

    /// The state text a script set ("" if none).
    pub fn state_text(&self) -> &str {
        self.extra.as_ref().map_or("", |e| e.state.as_str())
    }
}

impl Mobs {
    /// Where the creature with this id is in the list.
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.list.iter().position(|m| m.id == id)
    }

    fn extra_of(&mut self, i: usize) -> &mut Extra {
        self.list[i].extra.get_or_insert_with(Default::default)
    }

    /// Throw the extras away again when nothing is set (no memory kept).
    fn tidy(&mut self, i: usize) {
        if self.list[i].extra.as_ref().map_or(false, |e| e.is_plain()) {
            self.list[i].extra = None;
        }
    }

    /// How many creatures have bone overrides.
    pub fn with_bones(&self) -> usize {
        self.list.iter().filter(|m| m.extra.as_ref().map_or(false, |e| !e.bones.is_empty())).count()
    }

    /// Give a creature its own size (blocks; inside the limits of
    /// `creature_size`): it collides and is drawn at it.
    pub fn set_size(&mut self, id: u32, width: f64, height: f64) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        let (w, h) = clamp_size(width, height);
        self.extra_of(i).size = Some((w, h));
        self.extra_of(i).touch();
        self.list[i].body.resize(w as f64, h as f64);
        true
    }

    /// Set a creature's health and its limit (1 to 1,000,000; the health may
    /// be 0, which kills it, but never over the limit).
    pub fn set_health(&mut self, id: u32, hp: i64, max_hp: i64) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        let max = clamp_health(max_hp);
        let hp = clamp_current(hp).min(max);
        let e = self.extra_of(i);
        e.max_hp = Some(max);
        e.touch();
        self.list[i].health = hp;
        true
    }

    /// Override one bone of the creature's model by name, on top of its
    /// animation: rotation in degrees about the bone's pivot, an offset in
    /// model units, a uniform scale. An unknown name is logged once and ignored.
    pub fn set_bone(&mut self, id: u32, bone: &str, rot: [f64; 3], off: [f64; 3], scale: f64) -> BoneResult {
        let Some(i) = self.index_of(id) else { return BoneResult::NoCreature };
        let model = self.list[i].look().0;
        let Some(index) = self.bone_names.index(model, bone) else {
            self.bone_names.complain(model, bone);
            return BoneResult::UnknownBone;
        };
        let Some(entry) = BoneEntry::new(index, rot, off, scale) else { return BoneResult::BadNumber };
        let had_any = self.list[i].extra.as_ref().map_or(false, |e| !e.bones.is_empty());
        if !had_any && self.with_bones() >= MAX_WITH_BONES {
            return BoneResult::TooManyCreatures;
        }
        let e = self.extra_of(i);
        if !e.bones.set(entry) {
            return BoneResult::TableFull;
        }
        e.touch();
        BoneResult::Set
    }

    /// Take all of a creature's bone overrides away.
    pub fn clear_bones(&mut self, id: u32) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        if let Some(e) = self.list[i].extra.as_mut() {
            if !e.bones.is_empty() {
                e.bones.clear();
                e.touch();
            }
        }
        self.tidy(i);
        true
    }

    /// Tilt the whole model about its centre, in degrees (pitch up lifts the
    /// nose; roll leans the top toward the creature's left). `false` for no
    /// such creature or a number that is not finite.
    pub fn set_orientation(&mut self, id: u32, pitch: f64, roll: f64) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        if !pitch.is_finite() || !roll.is_finite() {
            return false;
        }
        let (p, r) = (orient_tenths(pitch), orient_tenths(roll));
        if self.list[i].extra.is_none() && p == 0 && r == 0 {
            return true;
        }
        let e = self.extra_of(i);
        if (e.pitch, e.roll) != (p, r) {
            e.pitch = p;
            e.roll = r;
            e.touch();
        }
        self.tidy(i);
        true
    }

    /// Set the short text a mod keeps on a creature (64 bytes at most; control
    /// characters are dropped). Shared games show it on every machine.
    pub fn set_state(&mut self, id: u32, text: &str) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        let text = clean_state(text);
        if self.list[i].extra.is_none() && text.is_empty() {
            return true;
        }
        let e = self.extra_of(i);
        if e.state != text {
            e.state = text;
            e.touch();
        }
        self.tidy(i);
        true
    }

    /// Force where a creature's head looks, in degrees relative to its body
    /// (yaw positive to its left, pitch positive up; within 90 and 60). The
    /// head follows smoothly and shared games show it on every machine.
    /// `false` for no such creature or a number that is not finite.
    pub fn set_look(&mut self, id: u32, yaw: f64, pitch: f64) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        if !yaw.is_finite() || !pitch.is_finite() {
            return false;
        }
        let (y, p) = crate::game::creature_look::clamp_look(yaw as f32, pitch as f32, crate::game::creature_look::SCRIPT_YAW, crate::game::creature_look::SCRIPT_PITCH);
        let want = Some(((y * 10.0) as i16, (p * 10.0) as i16));
        let e = self.extra_of(i);
        if e.look != want {
            e.look = want;
            e.touch();
        }
        true
    }

    /// Give the head back to the game (it looks at players again).
    pub fn clear_look(&mut self, id: u32) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        if let Some(e) = self.list[i].extra.as_mut() {
            if e.look.is_some() {
                e.look = None;
                e.touch();
            }
        }
        self.tidy(i);
        true
    }

    /// The state text of a creature ("" if it has none or is gone).
    pub fn state_of(&self, id: u32) -> &str {
        self.index_of(id).map_or("", |i| self.list[i].state_text())
    }

    /// Whether a body of this size fits at `at` (feet): only asked of big
    /// bodies (past the old size limits; ordinary ones always fit, as before).
    /// A coarse look at the cells the body would fill above what it steps over:
    /// at most about 700 of them, however big it is.
    pub fn room_for(world: &World, at: [f64; 3], width: f64, height: f64) -> bool {
        if !crate::creature_size::is_big(width as f32, height as f32) {
            return true;
        }
        if !(at[0].is_finite() && at[1].is_finite() && at[2].is_finite()) || !world.is_loaded(crate::math::floord(at[0]), crate::math::floord(at[2])) {
            return false;
        }
        let half = width * 0.5;
        let extent = if width > height { width } else { height };
        let k = ((extent / 8.0) as i32 + 1).max(1);
        let span = |lo: f64, hi: f64| {
            let (a, b) = (crate::math::floord(lo), crate::math::floord(hi));
            let mut v = alloc::vec::Vec::new();
            let mut c = a;
            while c < b {
                v.push(c);
                c += k;
            }
            v.push(b);
            v
        };
        let step = crate::creature_size::step_up(width);
        let xs = span(at[0] - half, at[0] + half);
        let zs = span(at[2] - half, at[2] + half);
        let ys = span(at[1] + step, at[1] + height);
        for &x in &xs {
            for &z in &zs {
                for &y in &ys {
                    if crate::block::is_solid(world.get(x, y, z)) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// The bone names of a model by its id, as its file gives them (`None` for
/// no such model). Reads the file the first time; asked of a model that is
/// not loaded in the renderer, it is parsed here just for its names.
pub fn model_bone_names(mobs: &mut Mobs, model_id: &str) -> Option<alloc::vec::Vec<alloc::string::String>> {
    let index = reg().model(model_id).or_else(|| reg().entities.iter().find(|e| e.id == model_id).map(|e| e.model))?;
    Some(mobs.bone_names.names(index).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some_kind() -> Option<usize> {
        if crate::native::skip_without_mods() || reg().entities.is_empty() {
            return None;
        }
        Some(0)
    }

    #[test]
    fn a_forced_look_is_bounded_and_can_be_given_back() {
        let Some(k) = some_kind() else { return };
        let mut mobs = Mobs::new(3);
        let i = mobs.spawn(k, [0.0, 100.0, 0.0]);
        let id = mobs.list[i].id;
        assert!(mobs.set_look(id, 200.0, -200.0));
        assert_eq!(mobs.list[i].extra.as_ref().unwrap().look, Some((900, -600)));
        assert!(mobs.set_look(id, 12.5, 3.0));
        assert_eq!(mobs.list[i].extra.as_ref().unwrap().look_deg(), Some((12.5, 3.0)));
        assert!(!mobs.set_look(id, f64::NAN, 0.0));
        assert!(!mobs.set_look(id + 1000, 0.0, 0.0));
        assert!(mobs.clear_look(id));
        assert!(mobs.list[i].extra.is_none(), "nothing kept");
        assert!(!mobs.clear_look(id + 1000));
    }

    #[test]
    fn size_and_health_are_clamped() {
        let Some(k) = some_kind() else { return };
        let mut mobs = Mobs::new(3);
        let i = mobs.spawn(k, [0.0, 100.0, 0.0]);
        let id = mobs.list[i].id;
        assert!(mobs.set_size(id, 1000.0, 1000.0));
        assert_eq!((mobs.list[i].body.half_width, mobs.list[i].body.height), (16.0, 64.0));
        assert_eq!(mobs.list[i].look().2, 64.0 * mobs.list[i].growth() * mobs.list[i].swell());
        assert!(mobs.list[i].body.step_up > 0.6, "a giant steps over more");
        assert!(mobs.set_size(id, 0.5, 1.0));
        assert_eq!(mobs.list[i].body.step_up, 0.6);
        assert!(!mobs.set_size(id + 1000, 1.0, 1.0));
        assert!(mobs.set_health(id, 5_000_000, 5_000_000));
        assert_eq!((mobs.list[i].health, mobs.list[i].max_health()), (1_000_000, 1_000_000));
        assert!(mobs.set_health(id, 50, 20));
        assert_eq!(mobs.list[i].health, 20, "never over the limit");
        assert!(mobs.set_health(id, -5, 0));
        assert_eq!((mobs.list[i].health, mobs.list[i].max_health()), (0, 1));
    }

    #[test]
    fn orientation_and_state_are_kept_only_when_set() {
        let Some(k) = some_kind() else { return };
        let mut mobs = Mobs::new(3);
        let i = mobs.spawn(k, [0.0, 100.0, 0.0]);
        let id = mobs.list[i].id;
        assert!(mobs.list[i].extra.is_none() && mobs.list[i].bone_view().is_none());
        // Setting nothing allocates nothing.
        assert!(mobs.set_orientation(id, 0.0, 0.0) && mobs.set_state(id, "") && mobs.clear_bones(id));
        assert!(mobs.list[i].extra.is_none());
        assert!(mobs.set_orientation(id, 15.0, -30.0));
        assert_eq!(mobs.list[i].bone_view().unwrap().1, Some((15.0, -30.0)));
        assert!(!mobs.set_orientation(id, f64::NAN, 0.0));
        assert!(mobs.set_orientation(id, 0.0, 0.0));
        assert!(mobs.list[i].extra.is_none(), "put back, nothing is kept");
        assert!(mobs.set_state(id, "phase\n2"));
        assert_eq!(mobs.state_of(id), "phase2");
        assert!(mobs.set_state(id, &"x".repeat(500)));
        assert_eq!(mobs.state_of(id).len(), 64);
        assert_eq!(mobs.state_of(id + 1000), "");
        assert!(!mobs.set_state(id + 1000, "x"));
    }

    #[test]
    fn bone_overrides_are_set_cleared_and_capped() {
        let Some(k) = some_kind() else { return };
        let mut mobs = Mobs::new(3);
        let i = mobs.spawn(k, [0.0, 100.0, 0.0]);
        let id = mobs.list[i].id;
        let model = mobs.list[i].look().0;
        let names: alloc::vec::Vec<alloc::string::String> = mobs.bone_names.names(model).to_vec();
        if names.is_empty() {
            return;
        }
        assert_eq!(mobs.set_bone(id, "no such bone", [0.0; 3], [0.0; 3], 1.0), BoneResult::UnknownBone);
        assert!(mobs.list[i].extra.is_none(), "an unknown bone changes nothing");
        assert_eq!(mobs.set_bone(id + 1000, &names[0], [0.0; 3], [0.0; 3], 1.0), BoneResult::NoCreature);
        assert_eq!(mobs.set_bone(id, &names[0], [f64::NAN, 0.0, 0.0], [0.0; 3], 1.0), BoneResult::BadNumber);
        // Any case finds it.
        assert_eq!(mobs.set_bone(id, &names[0].to_ascii_uppercase(), [10.0, 0.0, 0.0], [0.0; 3], 1.0), BoneResult::Set);
        assert_eq!(mobs.list[i].bone_view().unwrap().0.len(), 1);
        assert!(mobs.clear_bones(id));
        assert!(mobs.list[i].extra.is_none());
        // 256 creatures at most.
        let mut ids = alloc::vec::Vec::new();
        for n in 0..MAX_WITH_BONES + 1 {
            let j = mobs.spawn(k, [n as f64, 100.0, 0.0]);
            ids.push(mobs.list[j].id);
        }
        let results: alloc::vec::Vec<BoneResult> = ids.iter().map(|id| mobs.set_bone(*id, &names[0], [1.0, 0.0, 0.0], [0.0; 3], 1.0)).collect();
        assert_eq!(results.iter().filter(|r| **r == BoneResult::Set).count(), MAX_WITH_BONES);
        assert_eq!(results[MAX_WITH_BONES], BoneResult::TooManyCreatures);
        // One that already has overrides may still change them.
        assert_eq!(mobs.set_bone(ids[0], &names[0], [2.0, 0.0, 0.0], [0.0; 3], 1.0), BoneResult::Set);
    }

    #[test]
    fn a_creature_with_extras_travels_in_the_snapshot() {
        let Some(k) = some_kind() else { return };
        let mut host = Mobs::new(3);
        let i = host.spawn(k, [4.0, 100.0, 4.0]);
        let id = host.list[i].id;
        host.set_size(id, 12.0, 40.0);
        host.set_health(id, 700, 900);
        host.set_orientation(id, 10.0, 20.0);
        host.set_state(id, "enraged|50%");
        let j = host.spawn(k, [8.0, 100.0, 8.0]);
        let plain = host.list[j].id;
        let text = host.snapshot();
        assert!(text.lines().count() == 2 && text.lines().all(|l| l.split(' ').count() >= 10));
        let mut client = Mobs::new(4);
        client.apply_snapshot(&text);
        let c = &client.list[client.index_of(id).unwrap()];
        assert_eq!((c.body.half_width, c.body.height), (6.0, 40.0));
        assert_eq!((c.health, c.max_health()), (700, 900));
        assert_eq!(c.state_text(), "enraged|50%");
        assert_eq!(c.bone_view().unwrap().1, Some((10.0, 20.0)));
        assert!(client.list[client.index_of(plain).unwrap()].extra.is_none());
        // The next snapshot (nothing changed) says `|=`, and keeps everything.
        let again = host.snapshot();
        assert!(again.contains("|="), "{again}");
        client.apply_snapshot(&again);
        let c = &client.list[client.index_of(id).unwrap()];
        assert_eq!((c.body.height, c.state_text()), (40.0, "enraged|50%"));
        // A change is sent in full again; clearing it clears the client's copy.
        host.set_state(id, "calm");
        assert!(host.snapshot().contains("calm"));
        host.set_size(id, 1.0, 1.0);
        host.set_orientation(id, 0.0, 0.0);
        host.set_state(id, "");
        host.set_health(id, 5, 5);
        let ex = host.list[host.index_of(id).unwrap()].extra.as_ref().map(|e| e.max_hp);
        assert_eq!(ex, Some(Some(5)));
        client.apply_snapshot(&host.snapshot());
        let c = &client.list[client.index_of(id).unwrap()];
        assert_eq!((c.body.height, c.state_text(), c.bone_view().is_none()), (1.0, "", true));
    }

    #[test]
    fn a_full_set_of_extras_goes_out_every_few_snapshots() {
        let Some(k) = some_kind() else { return };
        let mut host = Mobs::new(3);
        let i = host.spawn(k, [4.0, 100.0, 4.0]);
        let id = host.list[i].id;
        host.set_state(id, "x");
        let full = (0..crate::creature_wire::FULL_EVERY * 2).filter(|_| host.snapshot().contains("|Tx")).count();
        assert!(full >= 2 && full <= 3, "{full}");
    }

    #[test]
    fn the_snapshot_stays_within_a_message() {
        let Some(k) = some_kind() else { return };
        let mut host = Mobs::new(3);
        let model = {
            let i = host.spawn(k, [0.0, 100.0, 0.0]);
            host.list[i].look().0
        };
        let names: alloc::vec::Vec<alloc::string::String> = host.bone_names.names(model).to_vec();
        for n in 0..64 {
            let i = host.spawn(k, [n as f64, 100.0, 0.0]);
            let id = host.list[i].id;
            host.set_state(id, &"y".repeat(64));
            for name in names.iter().take(64) {
                host.set_bone(id, name, [1.5, 2.5, 3.5], [0.5, 0.5, 0.5], 1.5);
            }
        }
        let text = host.snapshot();
        assert!(text.len() < crate::proto::MAX_STATE, "{}", text.len());
        assert_eq!(text.lines().count(), 64, "every creature still has its line");
    }

    #[test]
    fn only_big_bodies_ask_for_room() {
        let w = World::new(1);
        assert!(Mobs::room_for(&w, [0.0, 100.0, 0.0], 1.0, 2.0), "ordinary bodies are never asked");
        assert!(!Mobs::room_for(&w, [f64::NAN, 100.0, 0.0], 20.0, 40.0));
        // Nothing is loaded in a fresh world: no room for a giant.
        assert!(!Mobs::room_for(&w, [0.0, 100.0, 0.0], 20.0, 40.0));
    }
}
