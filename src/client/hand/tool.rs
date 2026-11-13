//! How a picture of a tool sits in the hand. The game's own tools are drawn
//! on the diagonal and a texture pack's (the OCD style) stand upright; both
//! must be held by the handle with the head up, so the picture's long axis
//! and its held end are found from which pixels are solid.

/// Side of a tool's picture in the first-person model (blocks).
pub(super) const TOOL_SIZE: f32 = 0.30;

/// How a picture sits as a held tool: the turn (radians, about z) that puts its
/// long axis upright, and the held end (the handle's end) as a fraction of the
/// picture from its bottom left. Found from which pixels are solid: the axis is
/// the direction they spread most, and the handle is at the end nearer the
/// picture's bottom.
pub(super) fn tool_frame(tile: u16) -> (f32, [f32; 2]) {
    let diagonal = (core::f32::consts::FRAC_PI_4, [0.0, 0.0]);
    let Some(alpha) = crate::textures::tile_alpha(tile as usize) else { return diagonal };
    let (mut n, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                n += 1.0;
                sx += x as f32 + 0.5;
                sy += 15.5 - y as f32;
            }
        }
    }
    if n < 3.0 {
        return diagonal;
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut cxx, mut cyy, mut cxy) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                let (dx, dy) = (x as f32 + 0.5 - mx, 15.5 - y as f32 - my);
                cxx += dx * dx;
                cyy += dy * dy;
                cxy += dx * dy;
            }
        }
    }
    // The principal axis: its angle from the x axis, folded so that it points up.
    let mut theta = 0.5 * crate::math::atan2f(2.0 * cxy, cxx - cyy);
    if theta < 0.0 {
        theta += core::f32::consts::PI;
    }
    // Rotating by (pi/2 - theta) turns the axis to point up (+y).
    let angle = core::f32::consts::FRAC_PI_2 - theta;
    let axis = [crate::math::cosf(theta), crate::math::sinf(theta)];
    // The solid pixel furthest back along the axis, the end with the lower
    // picture position, is where it is held.
    let (mut best, mut at) = (f32::MAX, [0.0f32, 0.0]);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                let (px, py) = (x as f32 + 0.5, 15.5 - y as f32);
                let along = px * axis[0] + py * axis[1];
                if along < best {
                    best = along;
                    at = [px / 16.0, py / 16.0];
                }
            }
        }
    }
    (angle, at)
}
