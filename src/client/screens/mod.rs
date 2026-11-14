//! Screens that live in files of their own, apart from the big
//! `client/app.rs`.
//!
//! `crafting` is an ordinary module. The other files in this folder
//! (`title.rs`, `settings.rs`, `join.rs`, `inventory.rs`, `map.rs`,
//! `overlays.rs`, `skin.rs`) are methods of `App` and are declared in
//! `app.rs` with `#[path]`, as children of it, because they need its private
//! fields; see docs/BUILDING.md, "Faster builds".

pub mod armor_slots;
pub mod crafting;
