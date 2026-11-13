//! The sun and the moon: flat discs on a see-through ground.

use super::*;

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "sun" => {
            // A pale core in a golden disc, with a half see-through halo.
            if disc(x, y, 2) {
                px([255, 250, 214])
            } else if disc(x, y, 6) {
                px([255, 224, 140])
            } else if disc(x, y, 8) {
                [255, 224, 140, 120]
            } else {
                CLEAR
            }
        }
        "moon" => {
            // A pale disc with two flat craters.
            if disc(x, y, 6) {
                if (x, y) == (6, 7) || (x, y) == (9, 8) {
                    px([180, 188, 206])
                } else {
                    px([224, 230, 242])
                }
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}
