//! How lava looks: the numbers behind its glow, with no drawing in them.
//!
//! * The face colour the mesher gives a lava vertex: always full bright (no
//!   side shade, no corner darkening) with a hot tint; flowing lava a little
//!   dimmer than the source ([`vertex_look`]).
//! * The slow brightness pulse of the surface, 90 to 100 percent ([`pulse`]).
//! * The scrolling of its picture ([`scroll_rows`]) and one frame of it
//!   ([`frame`]), which the renderer paints into the atlas (client/render/lava_anim.rs).
//! * The orange screen when the camera is inside lava, fading in and out
//!   ([`overlay_step`], [`overlay_colour`]).

use crate::math::{sinf, smoothstep};
use alloc::vec::Vec;

/// A liquid that gives off at least this much light is "molten" for the look.
pub const MOLTEN_EMIT: u8 = 8;
/// The hot tint of the face colour of lava.
pub const HOT: [u8; 3] = [255, 238, 208];
/// Share of its brightness flowing lava keeps (per mille).
pub const FLOW_PERMILLE: u32 = 880;
/// Dimmest and brightest the pulse gets.
pub const PULSE_MIN: f32 = 0.90;
pub const PULSE_MAX: f32 = 1.0;
/// Steps the pulse is quantised to (a repaint of the picture happens when it changes).
pub const PULSE_STEPS: i32 = 10;
/// Rows of the picture the lava scrolls a second.
pub const SCROLL_ROWS_PER_SEC: f32 = 1.5;

/// The (vertex shade, vertex colour) of a face of a block, given what the
/// registry knows of it: `fluid` is `(lava, level 1..8)`, `emit` its light,
/// `shade` the shade the mesher worked out (what is returned for anything
/// but lava, as a grey colour).
pub fn vertex_look(fluid: Option<(bool, u8)>, emit: u8, shade: u8) -> (u8, [u8; 3]) {
    match fluid {
        Some((true, level)) if emit >= MOLTEN_EMIT => {
            let keep = if level >= 8 { 1000 } else { FLOW_PERMILLE };
            (255, HOT.map(|c| (c as u32 * keep / 1000).min(255) as u8))
        }
        _ => (shade, [shade; 3]),
    }
}

/// How bright the surface is now, `PULSE_MIN` to `PULSE_MAX`: two slow waves
/// that never line up, so it breathes rather than blinks.
pub fn pulse(clock: f32) -> f32 {
    let mid = (PULSE_MIN + PULSE_MAX) * 0.5;
    let v = mid + 0.035 * sinf(clock * 0.9) + 0.015 * sinf(clock * 0.37 + 1.3);
    if v < PULSE_MIN {
        PULSE_MIN
    } else if v > PULSE_MAX {
        PULSE_MAX
    } else {
        v
    }
}

/// [`pulse`] as a whole step, `0..=PULSE_STEPS`.
pub fn pulse_step(clock: f32) -> i32 {
    let t = (pulse(clock) - PULSE_MIN) / (PULSE_MAX - PULSE_MIN);
    ((t * PULSE_STEPS as f32 + 0.5) as i32).clamp(0, PULSE_STEPS)
}

/// The brightness of a pulse step, 0.90 to 1.0.
pub fn pulse_of_step(step: i32) -> f32 {
    PULSE_MIN + (PULSE_MAX - PULSE_MIN) * step.clamp(0, PULSE_STEPS) as f32 / PULSE_STEPS as f32
}

/// How many rows (0..15) the picture is moved down at `clock`.
pub fn scroll_rows(clock: f32) -> i32 {
    if !(clock >= 0.0) || clock > 1.0e6 {
        return 0;
    }
    (clock * SCROLL_ROWS_PER_SEC) as i32 & 15
}

/// One frame of the lava picture, 16 x 16 RGBA: the built-in picture moved
/// `rows` down (wrapping round) at `step` of the pulse.
pub fn frame(rows: i32, step: i32) -> Vec<u8> {
    let k = (pulse_of_step(step) * 256.0) as u32;
    let mut out = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let p = crate::painters::liquids::lava_px(x, (y - rows).rem_euclid(16));
            out.extend_from_slice(&[(p[0] as u32 * k / 256).min(255) as u8, (p[1] as u32 * k / 256).min(255) as u8, (p[2] as u32 * k / 256).min(255) as u8, p[3]]);
        }
    }
    out
}

/// How fast the screen fills with lava and drains of it, per second.
pub const FADE_IN: f32 = 7.0;
pub const FADE_OUT: f32 = 2.2;

/// The fade (0 to 1) after `dt` seconds, `inside` or not.
pub fn overlay_step(fade: f32, inside: bool, dt: f32) -> f32 {
    let dt = if dt >= 0.0 && dt < 0.5 { dt } else { 0.5 };
    let f = if inside { fade + FADE_IN * dt } else { fade - FADE_OUT * dt };
    if !(f > 0.0) {
        0.0
    } else if f > 1.0 {
        1.0
    } else {
        f
    }
}

/// The colour laid over the whole view: thick and bright at full fade (the
/// view beyond is hardly there), smooth in and out.
pub fn overlay_colour(fade: f32) -> [u8; 4] {
    let k = smoothstep(0.0, 1.0, fade);
    [(200.0 + 55.0 * k) as u8, (70.0 + 50.0 * k) as u8, (8.0 + 8.0 * k) as u8, (k * 232.0) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lava_is_full_bright_and_hot_whatever_the_shade() {
        for shade in [40u8, 128, 255] {
            let (c, t) = vertex_look(Some((true, 8)), 12, shade);
            assert_eq!(c, 255);
            assert_eq!(t, HOT);
        }
    }

    #[test]
    fn flowing_lava_is_a_little_dimmer_than_the_source() {
        let (_, src) = vertex_look(Some((true, 8)), 12, 100);
        let (_, flow) = vertex_look(Some((true, 3)), 12, 100);
        for i in 0..3 {
            assert!(flow[i] < src[i] && flow[i] as u32 * 100 >= src[i] as u32 * 80);
        }
    }

    #[test]
    fn other_blocks_keep_their_shade() {
        assert_eq!(vertex_look(None, 0, 77), (77, [77; 3]));
        // A lamp glows but is not lava; water does not glow.
        assert_eq!(vertex_look(None, 15, 90), (90, [90; 3]));
        assert_eq!(vertex_look(Some((false, 8)), 0, 90), (90, [90; 3]));
        // A lava the content made dark is not lit up.
        assert_eq!(vertex_look(Some((true, 8)), 0, 90), (90, [90; 3]));
    }

    #[test]
    fn the_pulse_stays_between_ninety_and_a_hundred_percent() {
        let (mut lo, mut hi) = (2.0f32, 0.0f32);
        let mut t = -5.0f32;
        while t < 600.0 {
            let p = pulse(t);
            assert!((PULSE_MIN..=PULSE_MAX).contains(&p), "{t}: {p}");
            lo = lo.min(p);
            hi = hi.max(p);
            assert!((0..=PULSE_STEPS).contains(&pulse_step(t)));
            t += 0.07;
        }
        // It really moves, and gently: a tenth of a second changes it by under two percent.
        assert!(hi - lo > 0.04, "{lo} {hi}");
        assert!((pulse(10.1) - pulse(10.0)).abs() < 0.02);
        assert_eq!(pulse_of_step(0), PULSE_MIN);
        assert_eq!(pulse_of_step(PULSE_STEPS), PULSE_MAX);
        assert_eq!(pulse_of_step(99), PULSE_MAX);
    }

    #[test]
    fn the_picture_scrolls_slowly_and_wraps() {
        assert_eq!(scroll_rows(0.0), 0);
        assert_eq!(scroll_rows(1.0), 1);
        assert_eq!(scroll_rows(2.0), 3);
        assert!((0..16).contains(&scroll_rows(12345.6)));
        assert_eq!(scroll_rows(f32::NAN), 0);
        assert_eq!(scroll_rows(-3.0), 0);
    }

    #[test]
    fn a_frame_is_a_whole_tile_and_dimmer_at_the_low_pulse() {
        let a = frame(0, PULSE_STEPS);
        let b = frame(0, 0);
        assert_eq!(a.len(), 16 * 16 * 4);
        assert_eq!(a[..4], crate::painters::liquids::lava_px(0, 0));
        assert!(b[0] < a[0] && b[1] < a[1]);
        assert!(b[0] as u32 * 100 >= a[0] as u32 * 88);
        // Moved down by a row: the row 1 of the moved one is the row 0 of the plain one.
        let m = frame(1, PULSE_STEPS);
        assert_eq!(m[16 * 4..16 * 4 + 64], a[..64]);
        // The frame wraps: moving a whole turn gives the plain picture.
        assert_eq!(frame(16, 5), frame(0, 5));
    }

    #[test]
    fn the_screen_fades_in_quickly_and_out_slowly() {
        let mut f = 0.0;
        let mut steps = 0;
        while f < 1.0 {
            f = overlay_step(f, true, 0.016);
            steps += 1;
            assert!(steps < 100);
        }
        let in_steps = steps;
        steps = 0;
        while f > 0.0 {
            f = overlay_step(f, false, 0.016);
            steps += 1;
            assert!(steps < 200);
        }
        assert!(steps > in_steps);
        assert_eq!(overlay_step(0.0, false, 1.0), 0.0);
        assert_eq!(overlay_step(0.2, true, f32::NAN), 1.0);
    }

    #[test]
    fn the_overlay_is_thick_at_the_end_and_clear_at_the_start() {
        assert_eq!(overlay_colour(0.0)[3], 0);
        let full = overlay_colour(1.0);
        assert!(full[3] >= 225 && full[0] >= 250);
        let mut last = 0;
        for i in 0..=20 {
            let a = overlay_colour(i as f32 / 20.0)[3];
            assert!(a >= last);
            last = a;
        }
        // Out of range fades are safe.
        assert_eq!(overlay_colour(-4.0)[3], 0);
        assert_eq!(overlay_colour(9.0), full);
    }
}
