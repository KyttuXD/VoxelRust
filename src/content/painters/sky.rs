//! The sun and the moon: flat discs on a see-through ground. The house sun
//! has four short rays; the house moon is shaded on its lower right and has
//! two craters of its own.

use super::*;

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "sun" => {
            // A pale core in a golden disc, with a half see-through halo and a ray at each compass point.
            if disc(x, y, 2) {
                px([255, 248, 208])
            } else if disc(x, y, 6) {
                px([255, 218, 124])
            } else if rect(x, y, 7, 0, 2, 2) || rect(x, y, 7, 14, 2, 2) || rect(x, y, 0, 7, 2, 2) || rect(x, y, 14, 7, 2, 2) {
                px([255, 190, 84])
            } else if disc(x, y, 8) {
                [255, 218, 124, 120]
            } else {
                CLEAR
            }
        }
        "moon" => {
            // A pale disc, its lower right in shade, and two flat craters.
            if disc(x, y, 6) {
                if (x, y) == (5, 6) || (x, y) == (10, 7) || (x, y) == (6, 6) {
                    px([172, 182, 210])
                } else if x + y >= 17 {
                    px([196, 206, 228])
                } else {
                    px([228, 232, 244])
                }
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}
