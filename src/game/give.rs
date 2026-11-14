//! Handing the player something without losing it: what does not fit in the
//! inventory is set down at the player's feet instead of being thrown away.

use crate::game::session::Session;

impl Session {
    /// Put `count` of `item` into the inventory; whatever does not fit lies
    /// on the ground by the player's feet (picked up again when there is
    /// room). Returns how many went to the ground.
    pub fn give_or_drop(&mut self, item: u16, count: u16) -> u16 {
        self.give_stack_or_drop(crate::inventory::Stack::new(item, count))
    }

    /// As [`Session::give_or_drop`] for a stack that may be worn or carry data.
    pub fn give_stack_or_drop(&mut self, stack: crate::inventory::Stack) -> u16 {
        let left = self.inv.add_stack(stack);
        if left > 0 {
            let p = self.player.pos;
            self.drop_stack([p[0], p[1] + 0.8, p[2]], [0.0, 2.0, 0.0], stack.with_count(left));
        }
        left
    }
}
