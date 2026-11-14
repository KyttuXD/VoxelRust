//! How fast the main loop may run when there is nothing to hurry for: a menu
//! with no world running, a window that is not in front, a window that is
//! minimised. Without this the loop draws as fast as the card allows and one
//! core is busy for nothing (a window behind others on a machine without
//! vsync can reach thousands of frames a second).

/// What the loop knows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    /// A menu is open (any screen but the game and the inventory).
    pub in_menu: bool,
    /// The window has the keyboard focus.
    pub focused: bool,
    /// The window is minimised or hidden.
    pub minimised: bool,
    pub vsync: bool,
    /// The player's own cap when vsync is off (0 = none).
    pub max_fps: i32,
    /// A scripted run (pictures, benchmarks): nothing is slowed down.
    pub scripted: bool,
}

/// Frames per second in a menu.
pub const MENU_FPS: i32 = 60;
/// ... with the window not in front.
pub const BACKGROUND_FPS: i32 = 20;
/// ... minimised or hidden.
pub const HIDDEN_FPS: i32 = 5;

/// The most frames a second to draw now; `None` for no limit of the loop's own
/// (the swap paces it under vsync).
pub fn cap(f: &Facts) -> Option<i32> {
    let own = if f.max_fps > 0 && !f.vsync { Some(f.max_fps) } else { None };
    if f.scripted {
        return own;
    }
    let lowest = |a: Option<i32>, b: i32| Some(a.map_or(b, |a| a.min(b)));
    if f.minimised {
        return lowest(own, HIDDEN_FPS);
    }
    if !f.focused {
        return lowest(own, BACKGROUND_FPS);
    }
    if f.in_menu {
        return lowest(own, MENU_FPS);
    }
    own
}

/// Milliseconds to wait at the end of a frame that took `spent` seconds, to
/// keep to `cap`; 0 when it is slow enough already.
pub fn wait_ms(cap: Option<i32>, spent: f64) -> u32 {
    match cap {
        Some(c) if c > 0 => {
            let want = 1.0 / c as f64;
            if spent < want {
                // Rounded up to a whole millisecond, so the cap is a cap.
                let ms = (want - spent) * 1000.0;
                let whole = ms as u32;
                if ms > whole as f64 {
                    whole + 1
                } else {
                    whole
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts { in_menu: false, focused: true, minimised: false, vsync: false, max_fps: 0, scripted: false }
    }

    #[test]
    fn playing_in_front_is_not_held_back() {
        assert_eq!(cap(&facts()), None);
        assert_eq!(cap(&Facts { max_fps: 144, ..facts() }), Some(144));
        // Under vsync the swap paces the frames.
        assert_eq!(cap(&Facts { max_fps: 144, vsync: true, ..facts() }), None);
    }

    #[test]
    fn menus_are_held_to_60_even_with_vsync() {
        assert_eq!(cap(&Facts { in_menu: true, ..facts() }), Some(60));
        assert_eq!(cap(&Facts { in_menu: true, vsync: true, ..facts() }), Some(60));
        // A lower cap of the player's own still wins.
        assert_eq!(cap(&Facts { in_menu: true, max_fps: 30, ..facts() }), Some(30));
    }

    #[test]
    fn a_window_behind_others_or_minimised_is_held_far_lower() {
        assert_eq!(cap(&Facts { focused: false, ..facts() }), Some(BACKGROUND_FPS));
        assert_eq!(cap(&Facts { focused: false, in_menu: true, vsync: true, ..facts() }), Some(BACKGROUND_FPS));
        assert_eq!(cap(&Facts { minimised: true, focused: false, ..facts() }), Some(HIDDEN_FPS));
        assert_eq!(cap(&Facts { minimised: true, vsync: true, ..facts() }), Some(HIDDEN_FPS));
    }

    #[test]
    fn scripted_runs_are_left_alone() {
        assert_eq!(cap(&Facts { scripted: true, in_menu: true, focused: false, ..facts() }), None);
        assert_eq!(cap(&Facts { scripted: true, max_fps: 30, ..facts() }), Some(30));
    }

    #[test]
    fn the_wait_fills_the_rest_of_the_frame() {
        assert_eq!(wait_ms(None, 0.001), 0);
        assert_eq!(wait_ms(Some(60), 0.020), 0);
        let w = wait_ms(Some(100), 0.004);
        assert!(w == 6 || w == 7, "{}", w);
        assert_eq!(wait_ms(Some(60), 0.0), 17);
        assert_eq!(wait_ms(Some(0), 0.0), 0);
    }
}
