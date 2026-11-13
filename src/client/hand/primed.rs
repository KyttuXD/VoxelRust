//! Lit TNT in the world: the block itself, flashing white as its fuse runs
//! down and swelling a little at the end (see `game::primed`).

use super::model::{boxq, Quad};
use crate::block;
use crate::primed::{flash_on, fuse_scale, PrimedBlock};
use alloc::vec::Vec;

/// How far off lit blocks are drawn.
const RANGE: f32 = 96.0;

/// The quads of every lit block in `list`. `rel` turns a place in the world
/// into one relative to the camera; `white` is the plain white tile.
pub fn primed_blocks(list: &[PrimedBlock], white: u16, rel: &dyn Fn([f64; 3]) -> [f32; 3], out: &mut Vec<Quad>) {
    for p in list {
        let c = rel(p.pos);
        if c[0] * c[0] + c[2] * c[2] > RANGE * RANGE {
            continue;
        }
        let [top, side, bottom] = block::def(p.block).tex;
        let tiles = if flash_on(p.fuse) { [white; 6] } else { [side, side, side, side, top, bottom] };
        let h = 0.5 * fuse_scale(p.fuse);
        boxq(out, [c[0] - h, c[1] - h, c[2] - h], [c[0] + h, c[1] + h, c[2] + h], tiles, [255, 255, 255]);
    }
}
