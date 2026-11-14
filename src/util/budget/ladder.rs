//! The order in which the game gives things up when memory runs short.
//! Pure logic: see the module above, and the tests below.

use super::MIN_VIEW;

/// How far the two kinds of cut can go.
pub const MAX_DETAIL_CUT: u8 = 3;
pub const MAX_FAR_CUT: u8 = 3;

/// Seconds between two steps down: freed memory takes a while to show (chunks
/// leave as the workers and the frame loop get to them).
const COOLDOWN: f32 = 4.0;
/// ... and when memory is far over the limit.
const COOLDOWN_URGENT: f32 = 1.0;
/// Over the limit by this much (percent of it), steps come at the quick pace.
const URGENT_PERCENT: usize = 125;
/// Seconds spent well under the limit before one step is taken back.
const CALM_NEEDED: f32 = 45.0;
/// "Well under": below this percent of the limit.
const CALM_PERCENT: usize = 60;

/// What the player asked for (the settings).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Wanted {
    pub distance: i32,
    pub far: i32,
    pub lod_real: i32,
    pub lod_trees: i32,
    pub lod_detail: i32,
}

/// What is used while the budget holds things back (same shape as what was asked for).
pub type Effective = Wanted;

/// What the caller must do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    /// Free the decoded sounds that are not playing.
    TrimAudio,
    /// A new `Effective` applies (a step down or back up): see [`Ladder::effective`].
    Changed,
}

/// The state of the giving up.
#[derive(Clone, Debug, Default)]
pub struct Ladder {
    /// Sounds were trimmed since memory last was under the limit.
    audio_done: bool,
    detail_cut: u8,
    far_cut: u8,
    view_cut: i32,
    since_step: f32,
    calm: f32,
    /// Nothing more to give up.
    pub exhausted: bool,
}

impl Ladder {
    pub fn new() -> Ladder {
        Ladder::default()
    }

    /// How many steps are taken now (0 = everything as asked).
    pub fn steps(&self) -> i32 {
        self.detail_cut as i32 + self.far_cut as i32 + self.view_cut
    }

    /// Whether anything is held back.
    pub fn degraded(&self) -> bool {
        self.steps() > 0
    }

    /// A short description of what is held back, for the F3 screen.
    pub fn describe(&self) -> alloc::string::String {
        if !self.degraded() {
            return alloc::string::String::from("none");
        }
        alloc::format!("detail -{} far -{} view -{}", self.detail_cut, self.far_cut, self.view_cut)
    }

    /// What to use, given what the player asked for.
    pub fn effective(&self, w: &Wanted) -> Effective {
        let c = self.detail_cut.min(MAX_DETAIL_CUT) as i32;
        let distance = (w.distance - self.view_cut).max(MIN_VIEW.min(w.distance));
        let mut far = if w.far > 0 && self.far_cut < MAX_FAR_CUT { w.far >> self.far_cut } else { 0 };
        // Distant terrain that no longer reaches past the view is off.
        if far <= distance {
            far = 0;
        }
        Effective {
            distance,
            far,
            lod_real: w.lod_real.max(0) >> c,
            lod_trees: (w.lod_trees - (c - 1).max(0)).max(0).min(w.lod_trees.max(0)),
            lod_detail: (w.lod_detail - c).max(0).min(w.lod_detail.max(0)),
        }
    }

    /// Call every frame with the seconds passed, the bytes held and the
    /// limit; `audio_bytes` is how much decoded sound could be freed and
    /// `wanted` what the player asked for (a cut that would change nothing
    /// for it is skipped).
    pub fn update(&mut self, dt: f32, total: usize, limit: usize, audio_bytes: usize, wanted: &Wanted) -> Action {
        self.since_step += dt;
        let percent = if limit == 0 { 1000 } else { ((total / 1024).saturating_mul(100) / (limit / 1024).max(1)).min(1000) };
        if total > limit {
            self.calm = 0.0;
            let gap = if percent >= URGENT_PERCENT { COOLDOWN_URGENT } else { COOLDOWN };
            if self.since_step < gap {
                return Action::Nothing;
            }
            self.since_step = 0.0;
            return self.step_down(audio_bytes, wanted);
        }
        // Under the limit: the next time it runs over, sounds may be trimmed again.
        if percent < 95 {
            self.audio_done = false;
            self.exhausted = false;
        }
        if percent < CALM_PERCENT && self.degraded() {
            self.calm += dt;
            if self.calm >= CALM_NEEDED {
                self.calm = 0.0;
                self.since_step = 0.0;
                return self.step_up();
            }
        } else {
            self.calm = 0.0;
        }
        Action::Nothing
    }

    fn step_down(&mut self, audio_bytes: usize, w: &Wanted) -> Action {
        if !self.audio_done {
            self.audio_done = true;
            if audio_bytes > 0 {
                return Action::TrimAudio;
            }
        }
        let before = self.effective(w);
        loop {
            if self.detail_cut < MAX_DETAIL_CUT {
                self.detail_cut += 1;
            } else if self.far_cut < MAX_FAR_CUT {
                self.far_cut += 1;
            } else if self.effective(w).distance > MIN_VIEW.min(w.distance) {
                self.view_cut += 1;
            } else {
                self.exhausted = true;
                return Action::Nothing;
            }
            if self.effective(w) != before {
                return Action::Changed;
            }
        }
    }

    fn step_up(&mut self) -> Action {
        if self.view_cut > 0 {
            self.view_cut -= 1;
        } else if self.far_cut > 0 {
            self.far_cut -= 1;
        } else if self.detail_cut > 0 {
            self.detail_cut -= 1;
        } else {
            return Action::Nothing;
        }
        self.exhausted = false;
        Action::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: usize = 1 << 20;
    const W: Wanted = Wanted { distance: 8, far: 32, lod_real: 3, lod_trees: 2, lod_detail: 1 };

    fn run(l: &mut Ladder, secs: f32, total: usize, limit: usize, audio: usize) -> Vec<Action> {
        let mut out = Vec::new();
        let mut t = 0.0;
        while t < secs {
            let a = l.update(0.5, total, limit, audio, &W);
            if a != Action::Nothing {
                out.push(a);
            }
            t += 0.5;
        }
        out
    }

    #[test]
    fn nothing_changes_while_under_the_limit() {
        let mut l = Ladder::new();
        assert!(run(&mut l, 100.0, 250 * MB, 300 * MB, 5 * MB).is_empty());
        assert_eq!(l.effective(&W), W);
        assert!(!l.degraded());
        assert_eq!(l.describe(), "none");
    }

    #[test]
    fn audio_goes_first_then_detail_then_far_then_view() {
        let mut l = Ladder::new();
        let acts = run(&mut l, 200.0, 320 * MB, 300 * MB, 10 * MB);
        assert_eq!(acts[0], Action::TrimAudio);
        assert!(acts[1..].iter().all(|a| *a == Action::Changed));
        // The first cut touches the detail only.
        let mut l = Ladder::new();
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 10 * MB, &W), Action::TrimAudio);
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 10 * MB, &W), Action::Changed);
        let e = l.effective(&W);
        assert_eq!((e.distance, e.far), (8, 32));
        assert_eq!(e.lod_detail, 0);
        // All the way down: the view shrinks to the least and far is off.
        let mut l = Ladder::new();
        run(&mut l, 400.0, 900 * MB, 300 * MB, 0);
        let e = l.effective(&W);
        assert_eq!(e.distance, MIN_VIEW);
        assert_eq!(e.far, 0);
        assert_eq!((e.lod_real, e.lod_trees, e.lod_detail), (0, 0, 0));
        assert!(l.exhausted);
    }

    #[test]
    fn the_view_is_not_cut_before_the_detail_and_far_are() {
        let mut l = Ladder::new();
        let mut seen_view_cut = false;
        for _ in 0..200 {
            l.update(5.0, 900 * MB, 300 * MB, 0, &W);
            let e = l.effective(&W);
            if e.distance < W.distance {
                seen_view_cut = true;
                assert_eq!(l.detail_cut, MAX_DETAIL_CUT);
                assert_eq!(l.far_cut, MAX_FAR_CUT);
            }
        }
        assert!(seen_view_cut);
    }

    #[test]
    fn steps_wait_for_the_cooldown_and_come_quicker_when_far_over() {
        let mut l = Ladder::new();
        assert_eq!(l.update(10.0, 310 * MB, 300 * MB, 0, &W), Action::Changed);
        // Just over: a second later is too soon.
        assert_eq!(l.update(1.0, 310 * MB, 300 * MB, 0, &W), Action::Nothing);
        assert_eq!(l.update(4.0, 310 * MB, 300 * MB, 0, &W), Action::Changed);
        // Far over: a second is enough.
        let mut l = Ladder::new();
        assert_eq!(l.update(10.0, 500 * MB, 300 * MB, 0, &W), Action::Changed);
        assert_eq!(l.update(1.0, 500 * MB, 300 * MB, 0, &W), Action::Changed);
    }

    #[test]
    fn steps_come_back_in_the_opposite_order_after_a_calm_spell() {
        let mut l = Ladder::new();
        run(&mut l, 400.0, 900 * MB, 300 * MB, 0);
        let down = l.steps();
        assert!(down > 5);
        // Not calm enough: 80 per cent of the limit changes nothing.
        assert!(run(&mut l, 200.0, 240 * MB, 300 * MB, 0).is_empty());
        assert_eq!(l.steps(), down);
        // Calm: one step per spell, the view first.
        let acts = run(&mut l, CALM_NEEDED + 1.0, 100 * MB, 300 * MB, 0);
        assert_eq!(acts, [Action::Changed]);
        assert_eq!(l.steps(), down - 1);
        assert!(l.effective(&W).distance > MIN_VIEW);
        run(&mut l, 2000.0, 100 * MB, 300 * MB, 0);
        assert_eq!(l.steps(), 0);
        assert_eq!(l.effective(&W), W);
    }

    #[test]
    fn going_over_again_trims_the_audio_again() {
        let mut l = Ladder::new();
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 1, &W), Action::TrimAudio);
        // Still over: the next step is not another trim.
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 1, &W), Action::Changed);
        // Under for a moment, over again.
        l.update(1.0, 100 * MB, 300 * MB, 1, &W);
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 1, &W), Action::TrimAudio);
    }

    #[test]
    fn no_audio_to_free_goes_straight_to_the_next_step() {
        let mut l = Ladder::new();
        assert_eq!(l.update(5.0, 320 * MB, 300 * MB, 0, &W), Action::Changed);
    }

    #[test]
    fn a_short_view_is_never_cut_below_itself_and_cuts_with_no_effect_are_skipped() {
        let small = Wanted { distance: 2, far: 0, lod_real: 0, lod_trees: 0, lod_detail: 0 };
        let mut l = Ladder::new();
        // Nothing to give up at all: no step ever changes anything.
        for _ in 0..50 {
            assert_eq!(l.update(5.0, 900 * MB, 300 * MB, 0, &small), Action::Nothing);
            assert_eq!(l.effective(&small), small);
        }
        assert!(l.exhausted);
        // Far at 32 with view 8: the first cut that matters is the far one (32 -> 16).
        let mut l = Ladder::new();
        let w = Wanted { distance: 8, far: 32, lod_real: 0, lod_trees: 0, lod_detail: 0 };
        assert_eq!(l.update(5.0, 900 * MB, 300 * MB, 0, &w), Action::Changed);
        assert_eq!(l.effective(&w).far, 16);
    }

    #[test]
    fn effective_values_follow_the_cuts() {
        let mut l = Ladder::new();
        l.detail_cut = 2;
        l.far_cut = 1;
        l.view_cut = 3;
        let e = l.effective(&W);
        assert_eq!((e.distance, e.far, e.lod_real, e.lod_trees, e.lod_detail), (5, 16, 0, 1, 0));
        l.far_cut = 3;
        assert_eq!(l.effective(&W).far, 0);
        l.view_cut = 100;
        assert_eq!(l.effective(&W).distance, MIN_VIEW);
        assert_eq!(l.describe(), "detail -2 far -3 view -100");
    }
}
