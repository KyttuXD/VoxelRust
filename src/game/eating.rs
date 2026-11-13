//! Eating and drinking take a moment: the use button is held while the food
//! is raised to the mouth, bitten and swallowed, and the food is used up at
//! the end. Letting go, changing slot or pressing break stops it with
//! nothing eaten. The client draws the hand's motion from `Eating::shown`.

/// Seconds from the first bite to the last.
pub const EAT_TIME: f32 = 0.9;
/// Bites (or gulps) in that time.
pub const BITES: u32 = 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct Eating {
    /// How far through, 0..1.
    pub progress: f32,
    pub item: u16,
    pub slot: usize,
    /// Whether it was going on last step (an idle step keeps nothing).
    on: bool,
}

/// What a step did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bite {
    Nothing,
    /// One more bite was taken.
    Bite,
    /// The last one: swallow, and use the food up.
    Done,
}

/// Whether an item is something drunk rather than eaten.
pub fn is_drink(item_id: &str) -> bool {
    ["milk", "juice", "potion", "tea", "coffee", "water", "soda", "wine", "beer"].iter().any(|w| item_id.contains(w))
}

impl Eating {
    /// Move on by `dt` seconds. `active` is whether the player is eating this
    /// item from this slot now (button held, food in hand, hungry enough).
    pub fn step(&mut self, dt: f32, active: bool, item: u16, slot: usize) -> Bite {
        if !active || (self.on && (self.item != item || self.slot != slot)) {
            *self = Eating::default();
            if !active {
                return Bite::Nothing;
            }
        }
        if !self.on {
            *self = Eating { progress: 0.0, item, slot, on: true };
        }
        let before = (self.progress * BITES as f32) as u32;
        self.progress += dt.max(0.0) / EAT_TIME;
        if self.progress >= 1.0 {
            *self = Eating::default();
            return Bite::Done;
        }
        if (self.progress * BITES as f32) as u32 > before { Bite::Bite } else { Bite::Nothing }
    }

    /// How far through `item` is being eaten (0 if it is not).
    pub fn shown(&self, item: u16) -> f32 {
        if self.on && self.item == item { self.progress } else { 0.0 }
    }

    pub fn reset(&mut self) {
        *self = Eating::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_is_used_up_at_the_end_after_a_few_bites() {
        let mut e = Eating::default();
        let (mut bites, mut done) = (0, false);
        for _ in 0..20 {
            match e.step(0.1, true, 5, 2) {
                Bite::Bite => bites += 1,
                Bite::Done => {
                    done = true;
                    break;
                }
                Bite::Nothing => {}
            }
        }
        assert!(done);
        assert!(bites >= 2, "{bites}");
        assert_eq!(e.progress, 0.0);
    }

    #[test]
    fn letting_go_or_changing_slot_starts_over() {
        let mut e = Eating::default();
        e.step(0.5, true, 5, 2);
        assert!(e.shown(5) > 0.4);
        e.step(0.1, false, 5, 2);
        assert_eq!(e.shown(5), 0.0);
        e.step(0.5, true, 5, 2);
        e.step(0.1, true, 5, 3);
        assert!(e.progress < 0.2, "{}", e.progress);
    }

    #[test]
    fn drinks_are_known() {
        assert!(is_drink("milk") && !is_drink("bread"));
    }
}
