//! Which pass draws where: the pure arithmetic of the shader chain.
//!
//! There are no framebuffer objects here (the game runs on OpenGL 1.5 and
//! WebGL alike), so the chain works through the window's own back buffer:
//! the 3D view is drawn into its bottom-left corner and copied to a texture;
//! every pass but the last draws the full picture (in that corner) from the
//! texture and copies the result back; the last draws it over the whole window.

use alloc::vec::Vec;

/// Most passes in a chain.
pub const MAX_PASSES: usize = 3;

/// One pass of the chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassPlan {
    /// Which shader of the chain.
    pub index: usize,
    pub last: bool,
    /// The viewport it draws into, from the bottom left corner: `(width, height)`.
    pub viewport: (i32, i32),
    /// `u_resolution`: the size of what it draws.
    pub resolution: (f32, f32),
    /// Whether the result is copied back into the texture for the next pass.
    pub copy_back: bool,
}

/// The passes for a chain of `passes` shaders (at most [`MAX_PASSES`]; more
/// are ignored, none gives an empty plan), when the 3D view was drawn
/// `scene` pixels big and the window is `window` big.
pub fn plan(passes: usize, scene: (i32, i32), window: (i32, i32)) -> Vec<PassPlan> {
    let n = passes.min(MAX_PASSES);
    let scene = (scene.0.max(1), scene.1.max(1));
    let window = (window.0.max(1), window.1.max(1));
    (0..n)
        .map(|index| {
            let last = index + 1 == n;
            let size = if last { window } else { scene };
            PassPlan { index, last, viewport: size, resolution: (size.0 as f32, size.1 as f32), copy_back: !last }
        })
        .collect()
}

/// Side of the square texture (a power of two, for old cards) that holds a
/// picture `w` x `h`.
pub fn texture_size(w: i32, h: i32) -> i32 {
    let mut need = 256;
    while need < w.max(h) && need < (1 << 16) {
        need *= 2;
    }
    need
}

/// `u_map`: how `scene(uv)` finds the picture in the texture. The first two
/// numbers scale the 0..1 coordinate to texels, the last two shift it half a
/// texel in, so 0 and 1 land on the centres of the first and last pixel.
pub fn uv_map(w: i32, h: i32, size: i32) -> [f32; 4] {
    let s = size.max(1) as f32;
    [(w.max(1) - 1) as f32 / s, (h.max(1) - 1) as f32 / s, 0.5 / s, 0.5 / s]
}

/// The time shaders see: seconds, wrapped every hour so single precision
/// keeps its fine steps in a long session.
pub fn wrap_time(clock: f32) -> f32 {
    if !(clock >= 0.0) {
        return 0.0;
    }
    let hours = (clock / 3600.0) as i64;
    clock - hours as f32 * 3600.0
}

/// A direction in the world, turned into the view's axes (x right, y up, z
/// towards the viewer) by the view rotation `m` (column-major, as in `math`).
pub fn to_view(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    [
        v[0] * m[0] + v[1] * m[4] + v[2] * m[8],
        v[0] * m[1] + v[1] * m[5] + v[2] * m[9],
        v[0] * m[2] + v[1] * m[6] + v[2] * m[10],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pass_draws_straight_to_the_window() {
        let p = plan(1, (960, 540), (1920, 1080));
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].index, p[0].last, p[0].viewport, p[0].copy_back), (0, true, (1920, 1080), false));
        assert_eq!(p[0].resolution, (1920.0, 1080.0));
    }

    #[test]
    fn a_chain_works_in_the_scene_corner_until_the_last_pass() {
        let p = plan(3, (960, 540), (1920, 1080));
        assert_eq!(p.iter().map(|x| x.index).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(p.iter().map(|x| x.last).collect::<Vec<_>>(), [false, false, true]);
        assert_eq!(p.iter().map(|x| x.copy_back).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!((p[0].viewport, p[1].viewport, p[2].viewport), ((960, 540), (960, 540), (1920, 1080)));
        assert_eq!(p[0].resolution, (960.0, 540.0));
    }

    #[test]
    fn the_chain_is_cut_at_three_and_may_be_empty() {
        assert_eq!(plan(7, (10, 10), (20, 20)).len(), MAX_PASSES);
        assert!(plan(0, (10, 10), (20, 20)).is_empty());
        // Nonsense sizes never make an empty viewport.
        assert_eq!(plan(1, (0, -3), (0, 0))[0].viewport, (1, 1));
    }

    #[test]
    fn the_texture_is_a_power_of_two_that_fits() {
        assert_eq!(texture_size(1, 1), 256);
        assert_eq!(texture_size(256, 100), 256);
        assert_eq!(texture_size(257, 100), 512);
        assert_eq!(texture_size(1920, 1080), 2048);
        assert_eq!(texture_size(3840, 2160), 4096);
    }

    #[test]
    fn the_map_puts_zero_and_one_on_pixel_centres() {
        let m = uv_map(100, 50, 256);
        assert_eq!(m[2], 0.5 / 256.0);
        assert_eq!(0.0 * m[0] + m[2], 0.5 / 256.0);
        assert_eq!(1.0 * m[0] + m[2], 99.5 / 256.0);
        assert_eq!(1.0 * m[1] + m[3], 49.5 / 256.0);
    }

    #[test]
    fn the_clock_wraps_every_hour() {
        assert_eq!(wrap_time(5.0), 5.0);
        assert_eq!(wrap_time(3600.0), 0.0);
        assert_eq!(wrap_time(3605.0), 5.0);
        assert_eq!(wrap_time(-1.0), 0.0);
        assert_eq!(wrap_time(f32::NAN), 0.0);
    }

    #[test]
    fn directions_follow_the_view() {
        // Looking along -z (yaw 0, pitch 0): the identity view.
        let id = crate::math::IDENTITY;
        assert_eq!(to_view(&id, [0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
        // A rotation that turns x into y.
        let mut m = id;
        m[0] = 0.0;
        m[1] = 1.0;
        m[4] = -1.0;
        m[5] = 0.0;
        assert_eq!(to_view(&m, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
    }
}
