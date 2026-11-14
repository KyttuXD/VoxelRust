//! Which pictures can be skins, and how a skin of any size is read.
//!
//! A skin is a square picture whose side is a multiple of 64, from 64 up to
//! 4096 pixels (and the older 64 x 32 layout, which is brought up to a square
//! at its own scale, see `client::skinfile`). No other rectangle is a skin:
//! 128 x 64 and the like are refused. The player models are laid out
//! on a 64 x 64 grid: a skin W pixels wide is that grid with every cell
//! `W / 64` pixels across, so a face at (x, y) of the grid is at
//! (x * W / 64, y * W / 64) in the picture. The models' texture coordinates
//! are fractions of their declared size (64 x 64), which is the same thing
//! as long as the picture is square; `uv` and `texel` say it in numbers.

/// The smallest and the largest side of a skin, in pixels.
pub const MIN_SIDE: usize = 64;
pub const MAX_SIDE: usize = 4096;

/// Whether a skin may be this many pixels on a side.
pub fn valid_side(side: usize) -> bool {
    side >= MIN_SIDE && side <= MAX_SIDE && side % 64 == 0
}

/// Whether a picture of this size is a skin: a square one of a valid side,
/// or the old 64 x 32 layout (and nothing else that is not square).
pub fn valid_size(w: usize, h: usize) -> bool {
    (valid_side(w) && h == w) || is_old_layout(w, h)
}

/// A skin as `w` x `h` pixels is in the old 64 x 32 layout.
pub fn is_old_layout(w: usize, h: usize) -> bool {
    w == 64 && h == 32
}

/// The number of pixels in one cell of the 64 x 64 grid (1 for a 64 skin).
pub fn scale(side: usize) -> Option<usize> {
    if valid_side(side) {
        Some(side / 64)
    } else {
        None
    }
}

/// The side of a square picture of `len` bytes of RGBA, if it is a skin.
pub fn side_of_rgba_len(len: usize) -> Option<usize> {
    if len % 4 != 0 {
        return None;
    }
    let px = len / 4;
    // (Sides are few: no square root needed.)
    (1..=MAX_SIDE / 64).map(|k| k * 64).find(|&s| s * s == px)
}

/// Texture coordinate (0..1) of a point of the 64-cell grid on a skin `side` pixels wide.
pub fn uv(grid: f32, side: usize) -> f32 {
    grid * (side / 64) as f32 / side as f32
}

/// The pixel of a skin `side` wide at a point of the 64-cell grid.
pub fn texel(grid: f32, side: usize) -> f32 {
    grid * (side / 64) as f32
}

/// The words for a picture that cannot be a skin.
pub fn refusal(w: usize, h: usize) -> alloc::string::String {
    alloc::format!("A SKIN MUST BE SQUARE, A MULTIPLE OF 64 PIXELS FROM 64 X 64 TO {0} X {0}, OR THE OLD 64 X 32 LAYOUT. THIS IS {1} X {2}", MAX_SIDE, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelDef;

    #[test]
    fn sizes_are_multiples_of_64_up_to_4096() {
        for s in [64usize, 128, 192, 256, 512, 1024, 2048, 4096] {
            assert!(valid_side(s) && valid_size(s, s), "{s}");
        }
        for s in [0usize, 32, 63, 65, 100, 4160, 8192] {
            assert!(!valid_side(s), "{s}");
        }
        assert!(valid_size(64, 32) && is_old_layout(64, 32));
        // Other 2:1 sizes are not skins, nor are other rectangles.
        assert!(!valid_size(128, 64) && !valid_size(256, 128) && !is_old_layout(128, 64));
        assert!(!valid_size(64, 48) && !valid_size(128, 32) && !valid_size(32, 16) && !is_old_layout(64, 64));
        assert_eq!(scale(128), Some(2));
        assert_eq!(scale(100), None);
        assert_eq!(side_of_rgba_len(64 * 64 * 4), Some(64));
        assert_eq!(side_of_rgba_len(1024 * 1024 * 4), Some(1024));
        assert_eq!(side_of_rgba_len(64 * 32 * 4), None);
        assert_eq!(side_of_rgba_len(7), None);
        assert!(refusal(100, 50).contains("100 X 50"));
    }

    #[test]
    fn the_grid_lands_on_whole_pixels_at_every_scale() {
        assert_eq!(uv(8.0, 64), 0.125);
        assert_eq!(uv(8.0, 128), 0.125);
        assert_eq!(uv(8.0, 1024), 0.125);
        assert_eq!(texel(8.0, 64), 8.0);
        assert_eq!(texel(8.0, 128), 16.0);
        assert_eq!(texel(8.0, 1024), 128.0);
        // The head's front face of the player (8, 8) - (16, 16) on a 128 skin is (16, 16) - (32, 32).
        assert_eq!((texel(8.0, 128), texel(16.0, 128)), (16.0, 32.0));
    }

    #[test]
    fn the_player_models_are_laid_out_on_the_64_grid_so_hd_skins_line_up() {
        for (id, text) in [("player", include_str!("../../data/models/player.bbmodel")), ("player_slim", include_str!("../../data/models/player_slim.bbmodel"))] {
            let m = ModelDef::from_bbmodel(id, text).unwrap();
            assert_eq!((m.model.tex_w, m.model.tex_h), (64.0, 64.0), "{id}");
            for c in &m.model.cubes {
                for (uv_rect, _) in c.faces.iter().flatten() {
                    for v in uv_rect {
                        // On the grid, and so on a whole pixel of a skin of any size.
                        assert!(*v >= 0.0 && *v <= 64.0, "{id}: {v}");
                        for side in [128usize, 1024] {
                            let t = texel(*v, side);
                            assert_eq!(t, (t as usize) as f32, "{id}: {v} at {side}");
                            assert_eq!(uv(*v, side) * side as f32, t, "{id}: {v} at {side}");
                        }
                    }
                }
            }
        }
    }
}
