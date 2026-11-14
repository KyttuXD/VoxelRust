//! See-through things: glass (a white frame round a see-through fill), ice,
//! glass doors, icicles and cobwebs.

use super::*;

const WHITE: [u8; 3] = [246, 250, 255];

/// The glints on a pane: a short bar in the upper left and a shorter one in the lower right.
const GLINTS: [(i32, i32); 5] = [(3, 3), (4, 3), (3, 4), (11, 12), (12, 12)];

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "glass" => {
            if edge(x, y) || at_any(x, y, &GLINTS) {
                px(WHITE)
            } else {
                CLEAR
            }
        }
        "ice" => {
            // Opaque pale blue with a frame, the shadow in the corner and two glints, each three long.
            let fill = [146, 196, 244];
            if edge(x, y) || (x, y) == (14, 14) {
                px(scale(fill, DARK))
            } else if [(3, 5), (4, 4), (5, 4), (10, 10), (11, 10), (12, 9)].contains(&(x, y)) {
                px([204, 234, 255])
            } else {
                px(fill)
            }
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "glass" => {
            // Stained glass: a white frame round a see-through fill in the colour.
            if edge(x, y) || at_any(x, y, &GLINTS) {
                [WHITE[0], WHITE[1], WHITE[2], 235]
            } else {
                [rgb[0], rgb[1], rgb[2], 105]
            }
        }
        "glassdoor" => {
            // A framed door whose upper half is a pane of clear glass.
            let frame = edge(x, y) || y == 8 || ((x == 7 || x == 8) && y < 8);
            if x == 12 && (y == 10 || y == 11) {
                px([222, 200, 110])
            } else if frame {
                px(scale(rgb, 65))
            } else if y < 8 {
                if [(3, 5), (4, 4), (5, 3)].contains(&(x, y)) {
                    px([220, 240, 250])
                } else {
                    CLEAR
                }
            } else {
                px(rgb)
            }
        }
        "icicle" => {
            // Spikes hanging from the ceiling, the left side lit.
            for (cx, len, half) in [(4, 11, 2), (9, 15, 3), (13, 8, 2)] {
                if y < len && (x - cx).abs() * len <= half * (len - y) {
                    let c = if x < cx { lift(rgb) } else { rgb };
                    return Some([c[0], c[1], c[2], 215]);
                }
            }
            CLEAR
        }
        "web" => {
            // Spokes and rings of thread.
            let (dx, dy) = (x - 8, y - 8);
            let spoke = dx == 0 || dy == 0 || dx == dy || dx == -dy;
            let ring = matches!(dx.abs().max(dy.abs()), 2 | 5) && (dx.abs() + dy.abs()) % 2 == 0;
            if spoke || ring {
                [rgb[0], rgb[1], rgb[2], 220]
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}
