//! Blocks a mod threw (`spawn_falling_block`, see `game::fx::falling`): a
//! small cube with the block's pictures.

use super::model::{boxq, Quad};
use crate::block;
use crate::game::fx::falling::{Falling, HALF};
use alloc::vec::Vec;

/// How far off they are drawn.
const RANGE: f32 = 96.0;

/// The quads of every flying block in `list`. `rel` turns a place in the
/// world into one relative to the camera.
pub fn falling_blocks(list: &[Falling], rel: &dyn Fn([f64; 3]) -> [f32; 3], out: &mut Vec<Quad>) {
    for f in list {
        let c = rel(f.pos);
        if c[0] * c[0] + c[2] * c[2] > RANGE * RANGE {
            continue;
        }
        let [top, side, bottom] = block::def(f.block).tex;
        let h = HALF as f32;
        boxq(out, [c[0] - h, c[1] - h, c[2] - h], [c[0] + h, c[1] + h, c[2] + h], [side, side, side, side, top, bottom], [255, 255, 255]);
    }
}
