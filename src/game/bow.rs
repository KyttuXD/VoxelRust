//! Drawing a bow. An item with a `shoots` property (a bow, a longbow, a
//! crossbow) is drawn back while the use button is held: the draw grows over
//! about a second, the picture of the bow and the arm follow it (three
//! pictures, `<bow>_pulling_0..2`), and letting go looses the arrow with a
//! speed and a damage that grow with the draw. A crossbow loads while the
//! button is held and fires with the next click.
//!
//! This file is the state machine and the pure functions; `Session` calls
//! it (see `Session::bow_step` and the `shoots` branch of `use_items`) and
//! the client reads `Bow::shown` to draw the pose.

use crate::block::reg;
use core::sync::atomic::{AtomicU32, Ordering};

/// The weakest a shot can be (a release the moment the string is touched),
/// as a share of the full power.
pub const MIN_POWER: f32 = 0.2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// Nothing drawn.
    #[default]
    Idle,
    /// The use button is held and the string is coming back.
    Drawing,
    /// A crossbow, loaded and waiting for the next click.
    Loaded,
}

/// What happened in a step.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event {
    Nothing,
    /// The button was let go: loose the arrow with this power (0..1).
    Release(f32),
    /// A crossbow is fully drawn and loaded.
    Ready,
    /// A loaded crossbow was clicked: fire it at full power.
    Fire,
    /// Slot changed, break pressed...: the draw is dropped, nothing shot.
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Bow {
    pub phase: Phase,
    /// How far back, 0..1 (1 while a crossbow is loaded).
    pub draw: f32,
    /// The item and hotbar slot being drawn.
    pub item: u16,
    pub slot: usize,
    pub crossbow: bool,
    /// Seconds for a full draw.
    total: f32,
    time: f32,
    /// A loaded crossbow fires on a click: the button must have been let go
    /// since it loaded.
    armed: bool,
    /// After a cancel with the button still down, no new draw starts until
    /// it has been let go.
    pub lockout: bool,
}

/// Whether an item is a crossbow (it loads and keeps its load).
pub fn is_crossbow(item_id: &str) -> bool {
    item_id.contains("crossbow")
}

/// Seconds a full draw takes, from the item's seconds between shots: about
/// a second for a bow, a little less than the old wait for a crossbow.
pub fn draw_time(wait: f32, crossbow: bool) -> f32 {
    let t = if crossbow { wait * 0.6 } else { wait * 1.4 };
    t.clamp(0.5, 2.0)
}

/// The share of full power that a draw of 0..1 gives: the string is easy
/// to start and hard to finish, and no release is quite powerless.
pub fn power_of(draw: f32) -> f32 {
    let d = draw.clamp(0.0, 1.0);
    let p = (d * d + d * 2.0) / 3.0;
    if p < MIN_POWER { MIN_POWER } else { p }
}

/// An arrow's damage and speed at `power`, from what the item's data gives
/// for a full draw. Always at least 1 damage when the item does any.
pub fn scaled(damage: i32, speed: f32, power: f32) -> (i32, f32) {
    let p = power.clamp(0.0, 1.0);
    let d = if damage <= 0 { 0 } else { ((damage as f32 * p + 0.5) as i32).max(1) };
    (d, speed * (0.35 + 0.65 * p))
}

/// Which of the three pictures of a bow being drawn to show (`None`: the
/// one at rest).
pub fn pull_frame(draw: f32) -> Option<usize> {
    if draw <= 0.0 {
        None
    } else if draw < 0.45 {
        Some(0)
    } else if draw < 0.9 {
        Some(1)
    } else {
        Some(2)
    }
}

/// The tile for the picture of the bow whose icon is `icon`, drawn to
/// `frame` (`<icon id>_pulling_<n>`), or the icon itself if it has none.
pub fn pulling_tile(icon: u16, frame: usize) -> u16 {
    let r = reg();
    let Some(base) = r.tiles.get(icon as usize) else { return icon };
    let want = alloc::format!("{}_pulling_{}", base.id, frame);
    r.tiles.iter().position(|t| t.id == want).map_or(icon, |i| i as u16)
}

impl Bow {
    /// Begin drawing `item` from hotbar slot `slot`; `wait` is the item's seconds
    /// between shots (see [`draw_time`]).
    pub fn start(&mut self, item: u16, slot: usize, wait: f32, crossbow: bool) {
        *self = Bow { phase: Phase::Drawing, draw: 0.0, item, slot, crossbow, total: draw_time(wait, crossbow), time: 0.0, armed: false, lockout: self.lockout };
    }

    /// Put everything away.
    pub fn reset(&mut self) {
        let lockout = self.lockout;
        *self = Bow::default();
        self.lockout = lockout;
    }

    /// Move on by `dt` seconds. `held` is the use button, `cancel` whether
    /// the slot or item changed or break was pressed.
    pub fn step(&mut self, dt: f32, held: bool, cancel: bool) -> Event {
        match self.phase {
            Phase::Idle => Event::Nothing,
            Phase::Drawing => {
                if cancel {
                    self.lockout = held;
                    self.reset();
                    return Event::Cancelled;
                }
                if held {
                    self.time += dt.max(0.0);
                    self.draw = (self.time / self.total.max(0.05)).clamp(0.0, 1.0);
                    if self.crossbow && self.draw >= 1.0 {
                        self.phase = Phase::Loaded;
                        self.armed = false;
                        return Event::Ready;
                    }
                    return Event::Nothing;
                }
                let power = power_of(self.draw);
                let partial = self.crossbow;
                self.reset();
                // A crossbow let go before it is loaded just relaxes.
                if partial { Event::Cancelled } else { Event::Release(power) }
            }
            Phase::Loaded => {
                if cancel {
                    self.lockout = held;
                    self.reset();
                    return Event::Cancelled;
                }
                if !held {
                    self.armed = true;
                    return Event::Nothing;
                }
                if self.armed {
                    self.reset();
                    // The click that fires must not start a new draw at once.
                    self.lockout = true;
                    return Event::Fire;
                }
                Event::Nothing
            }
        }
    }

    /// How far back `item` is drawn for the picture (0 if it is not the one being drawn).
    pub fn shown(&self, item: u16) -> f32 {
        if self.phase != Phase::Idle && self.item == item { self.draw } else { 0.0 }
    }
}

/// The bow picture the hotbar and inventory show for the item in hand: set
/// by the client every frame, read by whatever draws item icons.
static SHOWN_ITEM: AtomicU32 = AtomicU32::new(0);
static SHOWN_FRAME: AtomicU32 = AtomicU32::new(0);

/// Say which item (if any) is drawn back, to which picture.
pub fn show(item: Option<(usize, usize)>) {
    match item {
        Some((i, f)) => {
            SHOWN_FRAME.store(f as u32, Ordering::Relaxed);
            SHOWN_ITEM.store(i as u32 + 1, Ordering::Relaxed);
        }
        None => SHOWN_ITEM.store(0, Ordering::Relaxed),
    }
}

/// The picture to draw for item `index` whose own icon is `icon`.
pub fn icon_for(index: usize, icon: u16) -> u16 {
    let shown = SHOWN_ITEM.load(Ordering::Relaxed);
    if shown != 0 && shown as usize == index + 1 {
        pulling_tile(icon, SHOWN_FRAME.load(Ordering::Relaxed) as usize)
    } else {
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing(crossbow: bool) -> Bow {
        let mut b = Bow::default();
        b.start(3, 0, 0.7, crossbow);
        b
    }

    #[test]
    fn the_draw_grows_over_about_a_second_and_stops_at_one() {
        let mut b = drawing(false);
        assert_eq!(b.step(0.0, true, false), Event::Nothing);
        for _ in 0..5 {
            b.step(0.1, true, false);
        }
        assert!(b.draw > 0.4 && b.draw < 0.7, "{}", b.draw);
        for _ in 0..20 {
            b.step(0.1, true, false);
        }
        assert_eq!(b.draw, 1.0);
        assert_eq!(b.phase, Phase::Drawing);
    }

    #[test]
    fn letting_go_looses_with_the_power_of_the_draw() {
        let mut b = drawing(false);
        for _ in 0..4 {
            b.step(0.1, true, false);
        }
        let Event::Release(p) = b.step(0.0, false, false) else { panic!("released") };
        assert!(p > MIN_POWER && p < 1.0);
        assert_eq!(b.phase, Phase::Idle);
        // A full draw is full power; an instant release is the weakest shot.
        let mut full = drawing(false);
        full.step(5.0, true, false);
        assert_eq!(full.step(0.0, false, false), Event::Release(1.0));
        let mut quick = drawing(false);
        assert_eq!(quick.step(0.0, false, false), Event::Release(MIN_POWER));
    }

    #[test]
    fn power_rises_with_the_draw_and_scales_damage_and_speed() {
        let mut last = 0.0;
        for i in 0..=10 {
            let p = power_of(i as f32 / 10.0);
            assert!(p >= last);
            last = p;
        }
        assert_eq!(power_of(1.0), 1.0);
        let (d1, s1) = scaled(10, 40.0, 1.0);
        let (d0, s0) = scaled(10, 40.0, MIN_POWER);
        assert_eq!((d1, s1), (10, 40.0));
        assert!(d0 >= 1 && d0 < d1 && s0 < s1);
        assert_eq!(scaled(0, 40.0, 1.0).0, 0);
        assert_eq!(scaled(1, 40.0, MIN_POWER).0, 1);
    }

    #[test]
    fn changing_slot_or_pressing_break_cancels_without_a_shot() {
        let mut b = drawing(false);
        b.step(0.5, true, false);
        assert_eq!(b.step(0.1, true, true), Event::Cancelled);
        assert_eq!(b.phase, Phase::Idle);
        assert_eq!(b.draw, 0.0);
        // The button is still down: no new draw until it is let go.
        assert!(b.lockout);
        // Letting go afterwards shoots nothing.
        assert_eq!(b.step(0.1, false, false), Event::Nothing);
    }

    #[test]
    fn a_crossbow_loads_while_held_and_fires_on_the_next_click() {
        let mut b = drawing(true);
        let mut ready = false;
        for _ in 0..40 {
            if b.step(0.05, true, false) == Event::Ready {
                ready = true;
                break;
            }
        }
        assert!(ready && b.phase == Phase::Loaded);
        // Still holding: it does not fire by itself.
        assert_eq!(b.step(0.1, true, false), Event::Nothing);
        assert_eq!(b.phase, Phase::Loaded);
        // Let go, then click.
        assert_eq!(b.step(0.1, false, false), Event::Nothing);
        assert_eq!(b.phase, Phase::Loaded);
        assert_eq!(b.step(0.1, true, false), Event::Fire);
        assert_eq!(b.phase, Phase::Idle);
        assert!(b.lockout);
    }

    #[test]
    fn a_crossbow_let_go_early_relaxes_and_a_loaded_one_can_be_put_away() {
        let mut b = drawing(true);
        b.step(0.2, true, false);
        assert_eq!(b.step(0.0, false, false), Event::Cancelled);
        let mut b = drawing(true);
        b.step(10.0, true, false);
        assert_eq!(b.phase, Phase::Loaded);
        assert_eq!(b.step(0.0, false, true), Event::Cancelled);
        assert_eq!(b.phase, Phase::Idle);
    }

    #[test]
    fn pictures_follow_the_draw() {
        assert_eq!(pull_frame(0.0), None);
        assert_eq!(pull_frame(0.1), Some(0));
        assert_eq!(pull_frame(0.6), Some(1));
        assert_eq!(pull_frame(1.0), Some(2));
    }

    #[test]
    fn draw_times_and_names() {
        assert!((draw_time(0.7, false) - 0.98).abs() < 0.01);
        assert!(draw_time(1.8, true) > 0.9 && draw_time(1.8, true) <= 2.0);
        assert!(is_crossbow("crossbow") && !is_crossbow("longbow"));
    }

    #[test]
    fn only_the_item_drawn_is_shown_drawn() {
        let mut b = drawing(false);
        b.step(0.3, true, false);
        assert!(b.shown(3) > 0.0);
        assert_eq!(b.shown(4), 0.0);
    }

    #[test]
    fn the_base_bows_have_their_three_pictures() {
        if crate::native::skip_without_mods() {
            return;
        }
        let r = reg();
        for id in ["bow", "longbow", "crossbow"] {
            let item = r.items.iter().find(|i| i.id == id).expect(id);
            let a = pulling_tile(item.icon, 0);
            let c = pulling_tile(item.icon, 2);
            assert!(a != item.icon && c != item.icon && a != c, "{id}");
        }
    }
}
