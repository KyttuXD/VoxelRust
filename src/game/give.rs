//! Handing the player something without losing it: what does not fit in the
//! inventory is set down at the player's feet instead of being thrown away.

use crate::game::session::Session;

impl Session {
    /// Put `count` of `item` into the inventory; whatever does not fit lies
    /// on the ground by the player's feet (picked up again when there is
    /// room). Returns how many went to the ground.
    pub fn give_or_drop(&mut self, item: u16, count: u16) -> u16 {
        let left = self.inv.add(item, count);
        if left > 0 {
            let p = self.player.pos;
            self.drop_item([p[0], p[1] + 0.8, p[2]], [0.0, 2.0, 0.0], item, left);
        }
        left
    }
}
