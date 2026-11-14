//! Gear: armour slots, durability, item data and the player's attributes.
//!
//! | file | subject |
//! | --- | --- |
//! | `armor.rs` | the four worn slots, defense maths, putting on and taking off |
//! | `wear.rs` | durability: wearing, breaking, the wear bar, repair by crafting |
//! | `item_data.rs` | the small string map a stack can carry |
//! | `stack_text.rs` | a stack as one word of the save, with wear and data |
//! | `attributes.rs` | max health, walk speed, jump height, armour points, damage |
//! | `hearts.rs` | where the hearts of the health meter go (rows of ten) |
//! | `session_gear.rs` | what the session does with them: blows, wear per use, equipping |
//! | `eggs.rs` | spawn eggs made for every creature before recipes are read |

pub mod armor;
pub mod attributes;
pub mod eggs;
pub mod hearts;
pub mod item_data;
pub mod session_gear;
pub mod stack_text;
pub mod wear;

#[cfg(test)]
mod data_tests;
