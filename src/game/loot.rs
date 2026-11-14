//! What a creature leaves behind when it dies: its `drops` and any `loot`,
//! lying in the world to be picked up (survival only).

use crate::block::reg;
use crate::game::session::{GameMode, Session};

/// Set the dead creature's drops down around `at`.
pub fn leave(g: &mut Session, kind: usize, at: [f64; 3]) {
    if g.mode != GameMode::Survival {
        return;
    }
    let d = match reg().entities.get(kind) {
        Some(d) => d,
        None => return,
    };
    let mut seed = (at[0] * 31.0) as i32 as u32 ^ ((at[2] * 17.0) as i32 as u32).rotate_left(11) ^ (g.time * 100.0) as u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) % 1000
    };
    let mut out: alloc::vec::Vec<(u16, u16)> = alloc::vec::Vec::new();
    if let Some((item, n)) = d.drop {
        out.push((item as u16, n));
    }
    for &(item, most, chance) in &d.x.loot {
        if next() % 100 < chance as u32 {
            out.push((item as u16, 1 + (next() % most as u32) as u16));
        }
    }
    for (item, count) in out.into_iter().take(8) {
        let (a, b) = (next() as f32 / 1000.0 * 6.2832, 1.0 + next() as f32 / 1000.0 * 2.0);
        let (s, c) = (crate::math::sinf(a), crate::math::cosf(a));
        g.drop_item([at[0], at[1] + 0.5, at[2]], [c * b, 3.0, s * b], item, count);
    }
}
