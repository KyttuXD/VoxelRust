//! Per-bone overrides for a creature's model: what a script sets with
//! `set_bone` and the renderer lays on top of the animation (see
//! `ModelDef::quads_with`). The values are kept as whole numbers (tenths of
//! a degree, hundredths of a model unit) so that what the host holds and what
//! it sends to the other players of a shared game are exactly the same, and
//! so a table is small: one entry is 24 bytes, at most [`MAX_ENTRIES`].

use alloc::string::String;
use alloc::vec::Vec;

/// Most bones one creature may override.
pub const MAX_ENTRIES: usize = 64;
/// The highest bone number a table accepts (a model has far fewer bones).
pub const MAX_BONE: u16 = 1023;
/// Limits of the values, in the unit a script uses.
pub const MAX_ROT: f64 = 3000.0;
pub const MAX_OFFSET: f64 = 1000.0;
pub const MIN_SCALE: f64 = 0.01;
pub const MAX_SCALE: f64 = 100.0;

/// One bone's override: turned, moved and scaled about its own pivot, on top
/// of whatever the animation does with it. Children of the bone follow it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneEntry {
    /// The bone's place in the model's bone list.
    pub bone: u16,
    /// Rotation about x, y, z in tenths of a degree, added to the animation's.
    pub rot: [i16; 3],
    /// Offset in hundredths of a model unit.
    pub off: [i32; 3],
    /// Uniform scale in hundredths (100 is life size).
    pub scale: u16,
}

fn tenths(v: f64) -> i16 {
    (v.clamp(-MAX_ROT, MAX_ROT) * 10.0 + if v < 0.0 { -0.5 } else { 0.5 }) as i16
}

fn hundredths(v: f64) -> i32 {
    (v.clamp(-MAX_OFFSET, MAX_OFFSET) * 100.0 + if v < 0.0 { -0.5 } else { 0.5 }) as i32
}

impl BoneEntry {
    /// An entry from a script's numbers (degrees, model units, scale), kept
    /// inside the limits. `None` if a number is not finite.
    pub fn new(bone: usize, rot: [f64; 3], off: [f64; 3], scale: f64) -> Option<BoneEntry> {
        if bone > MAX_BONE as usize || !rot.iter().chain(off.iter()).all(|v| v.is_finite()) || !scale.is_finite() {
            return None;
        }
        Some(BoneEntry {
            bone: bone as u16,
            rot: [tenths(rot[0]), tenths(rot[1]), tenths(rot[2])],
            off: [hundredths(off[0]), hundredths(off[1]), hundredths(off[2])],
            scale: (scale.clamp(MIN_SCALE, MAX_SCALE) * 100.0 + 0.5) as u16,
        })
    }

    /// Rotation in degrees.
    pub fn rot_deg(&self) -> [f32; 3] {
        [self.rot[0] as f32 / 10.0, self.rot[1] as f32 / 10.0, self.rot[2] as f32 / 10.0]
    }

    /// Offset in model units.
    pub fn off_units(&self) -> [f32; 3] {
        [self.off[0] as f32 / 100.0, self.off[1] as f32 / 100.0, self.off[2] as f32 / 100.0]
    }

    pub fn scale_f(&self) -> f32 {
        self.scale as f32 / 100.0
    }
}

/// A creature's overrides, sorted by bone. Empty tables cost nothing: no heap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoneTable {
    entries: Vec<BoneEntry>,
}

impl BoneTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, BoneEntry> {
        self.entries.iter()
    }

    /// The override of bone `bone`, if there is one.
    pub fn get(&self, bone: usize) -> Option<&BoneEntry> {
        if self.entries.is_empty() {
            return None;
        }
        self.entries.binary_search_by(|e| (e.bone as usize).cmp(&bone)).ok().map(|i| &self.entries[i])
    }

    /// Set (or replace) an override. `false` when the table is full of other
    /// bones: nothing changes.
    pub fn set(&mut self, e: BoneEntry) -> bool {
        match self.entries.binary_search_by(|x| x.bone.cmp(&e.bone)) {
            Ok(i) => {
                self.entries[i] = e;
                true
            }
            Err(i) => {
                if self.entries.len() >= MAX_ENTRIES {
                    return false;
                }
                self.entries.insert(i, e);
                true
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries = Vec::new();
    }

    /// The table as text for the snapshot of a shared game:
    /// `bone:rx,ry,rz,ox,oy,oz,scale` entries joined by `;`.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        for (k, e) in self.entries.iter().enumerate() {
            if k > 0 {
                out.push(';');
            }
            out.push_str(&alloc::format!("{}:{},{},{},{},{},{},{}", e.bone, e.rot[0], e.rot[1], e.rot[2], e.off[0], e.off[1], e.off[2], e.scale));
        }
        out
    }

    /// A table from [`BoneTable::encode`]'s text, as far as it is sound: an
    /// entry that is not seven whole numbers, a bone out of range, or one
    /// past the 64th is dropped; numbers are clamped into the limits.
    pub fn decode(text: &str) -> BoneTable {
        let mut t = BoneTable::default();
        for part in text.split(';').take(MAX_ENTRIES) {
            let Some((bone, rest)) = part.split_once(':') else { continue };
            let Ok(bone) = bone.parse::<u16>() else { continue };
            let mut n = [0i64; 7];
            let mut count = 0;
            let mut ok = true;
            for (k, v) in rest.split(',').enumerate() {
                match (k < 7, v.parse::<i64>()) {
                    (true, Ok(x)) => {
                        n[k] = x;
                        count += 1;
                    }
                    _ => ok = false,
                }
            }
            if !ok || count != 7 || bone > MAX_BONE {
                continue;
            }
            let lim_r = (MAX_ROT * 10.0) as i64;
            let lim_o = (MAX_OFFSET * 100.0) as i64;
            t.set(BoneEntry {
                bone,
                rot: [n[0].clamp(-lim_r, lim_r) as i16, n[1].clamp(-lim_r, lim_r) as i16, n[2].clamp(-lim_r, lim_r) as i16],
                off: [n[3].clamp(-lim_o, lim_o) as i32, n[4].clamp(-lim_o, lim_o) as i32, n[5].clamp(-lim_o, lim_o) as i32],
                scale: n[6].clamp((MIN_SCALE * 100.0) as i64, (MAX_SCALE * 100.0) as i64) as u16,
            });
        }
        t
    }
}

/// Whole-body pitch and roll, in tenths of a degree.
pub fn orient_tenths(deg: f64) -> i16 {
    tenths(deg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(bone: usize, rx: f64) -> BoneEntry {
        BoneEntry::new(bone, [rx, 0.0, 0.0], [0.0; 3], 1.0).unwrap()
    }

    #[test]
    fn setting_replacing_and_clearing() {
        let mut t = BoneTable::default();
        assert!(t.is_empty() && t.get(0).is_none());
        assert!(t.set(e(5, 10.0)) && t.set(e(2, 20.0)));
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(5).unwrap().rot[0], 100);
        assert!(t.set(e(5, -30.0)));
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(5).unwrap().rot[0], -300);
        // Sorted by bone.
        assert_eq!(t.iter().map(|x| x.bone).collect::<Vec<_>>(), [2, 5]);
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn a_table_holds_64_bones_and_no_more() {
        let mut t = BoneTable::default();
        for b in 0..MAX_ENTRIES {
            assert!(t.set(e(b, 1.0)));
        }
        assert!(!t.set(e(100, 1.0)), "the 65th bone is refused");
        assert_eq!(t.len(), MAX_ENTRIES);
        assert!(t.set(e(3, 9.0)), "a bone already in the table may change");
    }

    #[test]
    fn values_are_clamped_and_bad_ones_refused() {
        let x = BoneEntry::new(0, [1.0e9, -1.0e9, 0.0], [5000.0, -5000.0, 0.0], 1.0e9).unwrap();
        assert_eq!(x.rot[0], 30000);
        assert_eq!(x.rot[1], -30000);
        assert_eq!(x.off[0], 100_000);
        assert_eq!(x.off[1], -100_000);
        assert_eq!(x.scale, 10_000);
        assert_eq!(BoneEntry::new(0, [0.0; 3], [0.0; 3], 0.0).unwrap().scale, 1);
        assert!(BoneEntry::new(0, [f64::NAN, 0.0, 0.0], [0.0; 3], 1.0).is_none());
        assert!(BoneEntry::new(0, [0.0; 3], [0.0, f64::INFINITY, 0.0], 1.0).is_none());
        assert!(BoneEntry::new(0, [0.0; 3], [0.0; 3], f64::NAN).is_none());
        assert!(BoneEntry::new(5000, [0.0; 3], [0.0; 3], 1.0).is_none());
    }

    #[test]
    fn the_wire_text_round_trips() {
        let mut t = BoneTable::default();
        t.set(BoneEntry::new(3, [12.5, -40.0, 3.3], [0.25, -1.5, 8.0], 1.75).unwrap());
        t.set(e(10, -90.0));
        let text = t.encode();
        assert_eq!(BoneTable::decode(&text), t);
        assert_eq!(BoneTable::default().encode(), "");
        assert!(BoneTable::decode("").is_empty());
    }

    #[test]
    fn hostile_wire_text_is_survived() {
        assert!(BoneTable::decode("x:1,2,3").is_empty());
        assert!(BoneTable::decode("1:1,2,3,4,5,6").is_empty(), "six numbers");
        assert!(BoneTable::decode("1:1,2,3,4,5,6,7,8").is_empty(), "eight numbers");
        assert!(BoneTable::decode("99999:1,2,3,4,5,6,7").is_empty());
        let huge = BoneTable::decode("1:99999999999,0,0,99999999999,0,0,99999999999");
        assert_eq!(huge.get(1).unwrap().rot[0], 30000);
        assert_eq!(huge.get(1).unwrap().scale, 10_000);
        let many: String = (0..200).map(|b| alloc::format!("{}:0,0,0,0,0,0,100", b)).collect::<Vec<_>>().join(";");
        assert_eq!(BoneTable::decode(&many).len(), MAX_ENTRIES);
    }
}
