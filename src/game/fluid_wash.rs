//! What flowing liquid sweeps away gives what breaking it would: in
//! survival, the same drop rules as digging it (the item and the `also`
//! item, by their chances), set down where the block stood, to float off
//! with the current. In creative nothing is given.

use crate::block;
use crate::session::{GameMode, Session};

/// Most things dropped from washed-away blocks in one go (a whole meadow
/// swept away does not fill the world with items; the rest are just gone).
const MAX_PER_TICK: usize = 24;

/// The items a washed-away block gives, given two random numbers: the item
/// and count of the main drop and of the second one, as the survival digging
/// code rolls them.
pub fn items_for(b: block::Block, roll_also: u32, roll_drop: u32) -> [Option<(u16, u16)>; 2] {
    let d = block::def(b);
    let also = d.x.also.and_then(|(item, chance, n)| (roll_also % 100 < chance as u32).then_some((item as u16, n)));
    let main = d.drop.and_then(|item| (roll_drop % 100 < d.drop_chance as u32).then_some((item as u16, 1)));
    [main, also]
}

/// Hand out what the liquids washed away since the last call.
pub fn collect(g: &mut Session) {
    if g.fluid.washed.is_empty() {
        return;
    }
    let washed = core::mem::take(&mut g.fluid.washed);
    if g.mode != GameMode::Survival {
        return;
    }
    for (c, b) in washed.into_iter().take(MAX_PER_TICK) {
        let (a, d) = (g.primed.roll(), g.primed.roll());
        let here = [c[0] as f64 + 0.5, c[1] as f64 + 0.3, c[2] as f64 + 0.5];
        for (item, count) in items_for(b, a, d).into_iter().flatten() {
            g.drop_item(here, [0.0, 1.5, 0.0], item, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    #[test]
    fn washing_gives_what_breaking_gives() {
        let r = reg();
        // Roll 0 passes every chance below 100.
        let got = items_for(r.wk.flower_red, 0, 0);
        let d = block::def(r.wk.flower_red);
        assert_eq!(got[0].is_some(), d.drop.is_some());
        // A roll of 99 fails a chance below 100 and passes a sure drop.
        let never = items_for(r.wk.tall_grass, 99, 99);
        let grass = block::def(r.wk.tall_grass);
        assert_eq!(never[0].is_some(), grass.drop.is_some() && grass.drop_chance == 100);
        // Blocks with no drop give nothing.
        assert_eq!(items_for(block::AIR, 0, 0), [None, None]);
    }
}
