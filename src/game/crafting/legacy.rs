//! The old list recipes (`recipe`, `bake`, `smelt` lines, clicked in a list
//! rather than laid out in a grid): whether the inventory can pay for one.

use crate::inventory::Inventory;
use alloc::vec::Vec;

/// Whether the inventory holds everything `inputs` (item, count) asks for
/// *together*: a recipe that names an item twice needs both amounts, not
/// the bigger of the two.
pub fn affordable(inv: &Inventory, inputs: &[(usize, u16)]) -> bool {
    let mut totals: Vec<(usize, u32)> = Vec::new();
    for &(item, n) in inputs {
        match totals.iter_mut().find(|t| t.0 == item) {
            Some(t) => t.1 += n as u32,
            None => totals.push((item, n as u32)),
        }
    }
    totals.iter().all(|&(item, n)| item <= u16::MAX as usize && inv.count(item as u16) >= n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Stack;

    #[test]
    fn an_item_named_twice_is_paid_for_twice() {
        let mut inv = Inventory::new();
        inv.slots[0] = Some(Stack { item: 3, count: 1 });
        assert!(affordable(&inv, &[(3, 1)]));
        assert!(!affordable(&inv, &[(3, 1), (3, 1)]));
        inv.slots[1] = Some(Stack { item: 3, count: 1 });
        assert!(affordable(&inv, &[(3, 1), (3, 1)]));
        assert!(!affordable(&inv, &[(4, 1)]));
        assert!(affordable(&inv, &[]));
    }
}
