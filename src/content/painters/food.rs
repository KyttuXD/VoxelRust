//! Food and farm goods: a bright main colour, a lighter highlight, a darker
//! lower edge, a dark outline. `paint <style> r g b` gives the main colour.

use super::*;

const BONE: [u8; 3] = [236, 230, 214];
const STEM: [u8; 3] = [96, 66, 36];
const LEAF: [u8; 3] = [72, 150, 60];
const LEAF_DARK: [u8; 3] = [50, 108, 44];

fn when(on: bool, c: [u8; 3]) -> Option<[u8; 3]> {
    if on {
        Some(c)
    } else {
        None
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    let (l, m, d) = (pal.light_of(rgb), rgb, pal.dark_of(rgb));
    Some(match style {
        "meat" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 13, 2 * y - 17);
            if dx * dx * 2 + dy * dy * 3 <= 150 {
                Some(if dx < -2 && dy < 0 {
                    l
                } else if dy > 4 {
                    d
                } else {
                    m
                })
            } else {
                when(rect(x, y, 11, 2, 4, 4) && (x - y - 9).abs() <= 1, BONE)
            }
        }),
        "fruit" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 15, 2 * y - 19);
            if dx * dx + dy * dy <= 90 {
                Some(if dx < -2 && dy < 0 {
                    l
                } else if dy > 5 {
                    d
                } else {
                    m
                })
            } else if (x == 7 || x == 8) && (2..5).contains(&y) {
                Some(STEM)
            } else {
                when(rect(x, y, 9, 2, 3, 2), LEAF)
            }
        }),
        "fishfood" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 13, 2 * y - 15);
            if dx * dx + dy * dy * 4 <= 110 {
                Some(if x == 4 && y == 7 {
                    OUTLINE
                } else if dy < 0 {
                    l
                } else if dy > 2 {
                    d
                } else {
                    m
                })
            } else {
                when((12..15).contains(&x) && (y - 7).abs() <= x - 11, d)
            }
        }),
        "loaf" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 15, 2 * y - 17);
            if dx * dx + dy * dy * 3 <= 190 {
                Some(if y <= 6 && (x == 7 || x == 11) {
                    l
                } else if y > 9 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "pizza" => outlined_with(x, y, pal.outline(), &|x, y| {
            // Crust, cheese, and four toppings in the sauce colour.
            let (dx, dy) = (2 * x - 15, 2 * y - 15);
            let d2 = dx * dx + dy * dy;
            if d2 > 210 {
                None
            } else if d2 > 150 {
                Some([214, 164, 96])
            } else if rect(x, y, 5, 5, 2, 2) || rect(x, y, 10, 4, 2, 2) {
                Some(m)
            } else if rect(x, y, 6, 9, 2, 2) || rect(x, y, 10, 9, 2, 2) {
                Some(d)
            } else {
                Some([246, 216, 120])
            }
        }),
        "wedge" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A slice with holes (cheese), the top face lighter.
            if (4..14).contains(&y) && (1..=14).contains(&x) && (x - 1) * 10 >= (13 - y) * 13 {
                Some(if rect(x, y, 9, 8, 2, 2) || rect(x, y, 5, 11, 2, 1) || rect(x, y, 11, 11, 2, 1) {
                    d
                } else if y < 6 {
                    l
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "root" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A tapering root with a tuft of leaves (carrot, beet).
            let w = ((y - 3) / 3).min(3);
            let cx = 12 - y * 2 / 3;
            if (4..15).contains(&y) && (x - cx).abs() <= 3 - w {
                Some(if x == cx - (3 - w) {
                    l
                } else if y >= 11 {
                    d
                } else {
                    m
                })
            } else if y == 1 {
                when(x == 11, LEAF)
            } else if (2..4).contains(&y) {
                when((x - 11).abs() <= 1, if x == 11 { LEAF_DARK } else { LEAF })
            } else {
                None
            }
        }),
        "tuber" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A lumpy oval (potato) with two eyes.
            let (dx, dy) = (2 * x - 15, 2 * y - 16);
            if dx * dx * 2 + dy * dy * 3 <= 330 {
                Some(if (x, y) == (5, 8) || (x, y) == (10, 10) {
                    d
                } else if (x, y) == (4, 7) || (x, y) == (5, 6) {
                    l
                } else if dy > 6 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "jar" => outlined_px(x, y, &|x, y| {
            // A pail or bottle of something, the contents in the given colour.
            let body = rect(x, y, 4, 5, 8, 10);
            if body && (x == 4 || x == 11 || y == 14) {
                Some(px([150, 154, 164]))
            } else if body && y == 7 {
                Some(px(l))
            } else if body && y > 7 {
                Some(px(m))
            } else if body {
                Some([196, 200, 210, 90])
            } else if (y == 3 && (5..11).contains(&x)) || (y == 4 && (x == 5 || x == 10)) {
                Some(px([196, 200, 210]))
            } else {
                None
            }
        }),
        "egg" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 15, 2 * y - 17);
            let wide = if dy < 0 { 4 } else { 3 };
            if dx * dx * wide + dy * dy * 2 <= 260 {
                Some(if dx < 0 && dy < 0 {
                    l
                } else if dy > 6 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "sack" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A tied sack (flour, sugar) with a band of the given colour.
            let (dx, dy) = (2 * x - 15, 2 * y - 19);
            if dx * dx + dy * dy <= 120 {
                Some(if (9..11).contains(&y) {
                    m
                } else if dx > 5 {
                    [200, 186, 156]
                } else {
                    [222, 210, 184]
                })
            } else {
                when(rect(x, y, 6, 1, 4, 3), [200, 186, 156])
            }
        }),
        "cookie" => outlined_with(x, y, pal.outline(), &|x, y| {
            let (dx, dy) = (2 * x - 15, 2 * y - 15);
            let d2 = dx * dx + dy * dy;
            if d2 <= 150 {
                Some(if [(5, 6), (9, 5), (7, 9), (10, 10), (4, 10)].contains(&(x, y)) {
                    [70, 44, 30]
                } else if d2 > 100 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "pie" => outlined_with(x, y, pal.outline(), &|x, y| {
            // Seen from the side: a filled crust in a tin.
            let (tin, crust) = ([206, 160, 100], [222, 178, 112]);
            if (9..13).contains(&y) && (x - 8).abs() <= 7 - (y - 9) {
                Some(if y == 12 { dark(tin) } else { tin })
            } else if (5..9).contains(&y) && (2 * x - 15).abs() <= 13 - (8 - y) * 3 {
                Some(if y == 5 || matches!(x, 5 | 8 | 11) { crust } else { m })
            } else {
                None
            }
        }),
        "bowl" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A bowl of stew or soup.
            if (7..9).contains(&y) && (x - 8).abs() <= 6 {
                Some(if y == 7 { l } else { m })
            } else if (9..13).contains(&y) && (x - 8).abs() <= 6 - (y - 9) {
                Some(if y >= 11 { [150, 108, 70] } else { [190, 140, 90] })
            } else {
                None
            }
        }),
        "seeds" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A scatter of small grains.
            for (gx, gy) in [(4, 6), (8, 5), (11, 7), (6, 9), (10, 10), (5, 12)] {
                if y == gy && (x == gx || x == gx + 1) {
                    return Some(if x == gx { l } else { m });
                }
            }
            None
        }),
        "sheaf" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A tied bundle: stalks fanning out above and below the band.
            let spread = (y - 8).abs() / 3 + 2;
            if (x - 8).abs() <= spread && (1..15).contains(&y) {
                Some(if y == 8 || y == 9 {
                    scale(rgb, 55)
                } else if y < 5 {
                    l
                } else {
                    m
                })
            } else {
                None
            }
        }),
        _ => return None,
    })
}
