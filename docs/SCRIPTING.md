# Scripting

A mod can carry plain Rust source files that the game **reads and runs
while it plays**. Nothing is compiled: the files can be written in any
text editor, and they come down from a server like the rest of a mod
(see [MULTIPLAYER.md](MULTIPLAYER.md)). The language is a small, deliberately
limited subset of Rust, so a script file is (nearly always) also valid Rust.

Scripts are for behaviour that pack lines cannot say (see
[MODDING.md](MODDING.md): `when ... do ...`): "give a reward for killing a boss",
"add a `/heal` command", "react to TNT", and also for whole mods that used to
need compiled code: a stalker that appears at night, a layer of rails, vehicles
that float, roll and fly (see the mods `manfromthefog`, `trains` and `vehicles`).
A mod that needs new terrain, an HTTP fetch or screens of its own still brings
compiled code ([Code that mods bring](MODDING.md#code-that-mods-bring)).

- [Where scripts live](#where-scripts-live)
- [A first script](#a-first-script)
- [The language](#the-language)
- [The library](#the-library)
- [Hooks: what the game calls](#hooks-what-the-game-calls)
- [The host API: what scripts may do](#the-host-api-what-scripts-may-do)
- [More host calls](#more-host-calls)
- [Controls, HUD and screens](#controls-hud-and-screens)
- [Saved state](#saved-state)
- [Limits, errors and safety](#limits-errors-and-safety)
- [Checking a script](#checking-a-script)
- [Scripts and compiled code](#scripts-and-compiled-code)
- [Worked examples](#worked-examples)
- [For the game's own code](#for-the-games-own-code)

## Where scripts live

In the mod's folder, under `scripts/`:

```
mods/my-mod/
  pack.txt
  scripts/
    main.rs
    boss.rs
```

Every `scripts/*.rs` file is a script. Files are loaded in name order and
**each file stands alone**: its functions, constants and types are its own, and
it cannot call another file's functions. Several files can define the same
hook (say `on_tick`); all of them are called. Files share data through
[saved state](#saved-state).

A mod found in `<data folder>/mods/<name>/`, or one a server sent
(`<data folder>/tmp/mods/<server>/<name>/`), is read the same way.

## A first script

```rust
// scripts/main.rs
use std::fmt;            // `use` lines are ignored

const REWARD: i64 = 5;

fn on_entity_death(kind: String, x: f64, y: f64, z: f64) {
    if kind == "zombie" {
        give("emerald", REWARD);
        say(&format!("A zombie fell at {:.0}, {:.0}", x, z));
    }
}
```

The game calls `on_entity_death` whenever a creature dies. Everything a
script can do to the game goes through the [host API](#the-host-api-what-scripts-may-do)
(`give`, `say`...).

## The language

Anything not listed here is not in the language; the game says so with the
file and line when it meets it.

### Items

| Item | Notes |
| --- | --- |
| `fn name(a: T, b: U) -> R { ... }` | Generics (`<T>`) and `where` are read and ignored. |
| `const NAME: T = expr;` and `static NAME: T = expr;` | Worked out once, when the file is loaded; may use earlier constants. |
| `struct P { x: i64, y: i64 }`, `struct W(i64);`, `struct U;` | Plain data. |
| `enum E { A, B(i64), C { n: i64 }, D = 7 }` | Unit, tuple and struct variants; `as i64` gives the number. |
| `impl P { fn new() -> Self {..} fn len(&self) -> i64 {..} fn bump(&mut self) {..} const K: i64 = 1; }` | Methods and associated functions and constants. `impl Trait for P` is read as `impl P` (the trait is only a name). |
| `use ...;`, `#[...]`, `type X = ...;` | Read and ignored. |
| `fn`, `const`, `struct`, `enum` inside a function | Allowed; they belong to the whole file. |

Not available: traits, modules, generics that matter, lifetimes that matter,
`unsafe`, `async`, macros of your own, `HashMap`.

### Values and types

Types are written but **not checked**: a script is dynamically typed. They
matter in three places only: `f32`/`f64` turns a whole number into a float
(`let x: f32 = 2;` is `2.0`), `as` converts, and `String`/`Vec`/a number type
in a `let` tells `collect()`, `parse()` and `sum()` what to make.

| Rust type | Here |
| --- | --- |
| `i8` ... `i64`, `isize`, `u8` ... `u64`, `usize` | all one 64-bit integer. Arithmetic that overflows 64 bits is an error. `as i32`, `as u8`... wrap like Rust. `u64::MAX` is `i64::MAX`. |
| `f32`, `f64` | one 64-bit float. (Values the game hands over as `f32`, like `dt`, print with the digits an `f32` would.) |
| `bool`, `char` | as in Rust. |
| `&str`, `String` | one kind of string; positions are byte positions. |
| `Vec<T>`, arrays `[T; N]`, slices | one kind: a growable list. `vec![0; n]`, `[1, 2, 3]`, `&v[1..3]`. |
| tuples | `(1, "a")`, `t.0`. |
| `Option<T>`, `Result<T, E>` | `Some`, `None`, `Ok`, `Err`. |
| `std::cmp::Ordering` | `Less`, `Equal`, `Greater`. |

Number literals: `42`, `1_000`, `0xff`, `0b1010`, `0o17`, `7u8`, `1.5`, `2e3`,
`0.25f32`. Strings: `"a\n"`, raw `r#"..."#`, `'c'`, `b'x'`.

Values behave like Rust's by-value values: `let b = a.clone()` and `let b = a`
are separate copies (they are shared underneath until one changes, so copies
cost nothing).

### Statements and expressions

Everything below is as in Rust.

- `let x = 1;`, `let mut x = 1;`, `let (a, b) = pair;`, `let Some(x) = opt else { return };`
- Operators `+ - * / %`, `== != < > <= >=`, `&& ||`, `! -`, `& | ^ << >>`,
  `+= -= *= /= %= &= |= ^= <<= >>=`, with Rust's precedence. Integer division
  by zero and overflow are errors. A whole number and a float mix (`1 + 0.5`).
- Casts: `x as f64`, `y as i32`, `n as u8 as char`, `c as u32`, `flag as i64`,
  `Dir::East as i64`.
- Ranges `a..b`, `a..=b`, `..b`, `a..` (in `for` and in slices).
- `if` / `else if` / `else` as an expression, `if let`, and `&&` between
  conditions (`if let Some(x) = o && x > 3` also works).
- `match` with literals, ranges (`1..=9`, `'a'..='z'`), `|` alternatives,
  `_`, names, `x @ 1..=5`, tuples, `Some(x)`, `Shape::Circle(r)`, `Shape::Rect { w, h }`,
  `[first, .., last]` and `if` guards.
- `while`, `while let`, `loop` (with `break value`), `for x in v`, `for i in 0..n`,
  `for (i, x) in v.iter().enumerate()`, labels `'outer: for ...` with
  `break 'outer` and `continue 'outer`.
- `return`, and `?` on `Option` and `Result` (no conversion of the error).
- Blocks as values, tuples, struct literals with `..base`, `Self`.
- `&x` is just `x` (a copy: nothing can change while you look). `&mut x`
  is a real reference: `fn inc(n: &mut i64) { *n += 1 }` works, as do `v.iter_mut()`,
  `for x in &mut v`, `if let Some(x) = v.get_mut(1) { *x = 5 }`,
  `match &mut self.state { State::Count(n) => *n += 1, .. }` and
  `std::mem::swap(&mut a, &mut b)`. References inside other values
  (a struct that holds a `&mut`) are not supported.
- Closures: `|a, b| a + b`, `|x: i64| -> i64 { x * 2 }`, `move || ...`.
  **A closure captures a copy of the variables it can see**, so one that
  counts (`let mut n = 0; let mut inc = || n += 1;`) does not change `n`. Pass
  what it should change as an argument.
- Macros: `format!`, `println!` / `print!` (to the game's log), `panic!`,
  `assert!`, `assert_eq!`, `assert_ne!`, `vec!`, `matches!`, `unreachable!`, `todo!`.

### format!

`{}`, `{:?}`, `{0}`, `{name}` (a variable), `{name}` with `name = expr`,
`{{` `}}`, and the spec `{:>8.2}`: fill and alignment (`<` `^` `>`), `+`, `#`,
`0`, width, `.precision`, and the types `?`, `x`, `X`, `b`, `o`, `e`.
Not supported: `{:width$}` and `{:.*}`. A struct has no `{}` (there is no
`Display`); `{:?}` prints it like derived `Debug`.

## The library

Iterator methods are **eager**: `v.iter().map(f)` makes the whole list at once.
`iter()`, `into_iter()` and friends give the same list, so
`v.iter().map(|x| x * 2).collect::<Vec<_>>()` and `v.iter().sum::<i64>()` read
as in Rust. A closure that is given an element gets a copy; `*x` and `&x` on a
copy do nothing, so Rust-style `|x| *x > 2` and `|&x| x > 2` both work.

| On | Methods |
| --- | --- |
| integers | `abs pow signum min max clamp rem_euclid div_euclid is_positive is_negative abs_diff wrapping_add/sub/mul saturating_add/sub/mul checked_add/sub/mul/div/rem/pow count_ones is_power_of_two to_string cmp` |
| floats | `abs sqrt floor ceil round trunc fract sin cos tan asin acos atan atan2 exp ln log log2 log10 powi powf hypot min max clamp signum recip is_nan is_finite to_radians to_degrees rem_euclid mul_add partial_cmp` |
| `char` | `is_alphabetic is_numeric is_alphanumeric is_whitespace is_uppercase is_lowercase is_ascii_digit ... to_ascii_uppercase to_ascii_lowercase to_uppercase to_lowercase (one char) to_digit is_digit len_utf8` |
| strings | `len is_empty push push_str pop clear insert insert_str remove truncate retain to_uppercase to_lowercase trim trim_start trim_end trim_matches contains starts_with ends_with find rfind replace split rsplit splitn split_once split_whitespace lines chars char_indices bytes repeat parse strip_prefix strip_suffix get matches eq_ignore_ascii_case to_string clone` and `+`, `+=`, `s[a..b]`. A pattern is a string or a char, never a closure. |
| `Vec` | `len is_empty push pop insert remove swap_remove clear truncate get get_mut first last first_mut last_mut contains reverse sort sort_by sort_by_key swap extend extend_from_slice append dedup retain drain resize fill rotate_left rotate_right join concat split_at windows chunks to_vec` |
| iterator methods | `iter iter_mut into_iter map filter filter_map flat_map flatten enumerate zip chain rev take skip step_by take_while skip_while map_while cloned copied sum product count min max min_by_key max_by_key min_by max_by fold reduce any all find find_map position partition unzip for_each nth last collect` (`collect::<String>()`, `collect::<Vec<_>>()`, and `Result`/`Option` of a list) |
| ranges | `contains`, and everything above (`(0..n).map(..)`, `.rev()`, `.sum()`) |
| `Option` | `is_some is_none unwrap expect unwrap_or unwrap_or_else map and_then or or_else ok_or ok_or_else filter is_some_and map_or map_or_else take replace insert get_or_insert flatten copied cloned iter` |
| `Result` | `is_ok is_err unwrap expect unwrap_err unwrap_or unwrap_or_else map map_err and_then or_else ok err` |
| `Ordering` | `reverse then then_with is_lt is_le is_gt is_ge is_eq is_ne` |
| any value | `clone to_owned to_string eq ne cmp partial_cmp min max clamp` |

Functions and constants: `String::new()`, `String::from(s)`, `Vec::new()`,
`Vec::with_capacity(n)`, `char::from_u32(n)`, `char::from_digit(n, radix)`,
`i64::from_str_radix(s, radix)`, `f64::from(x)`, `i32::MAX`, `i32::MIN`,
`u8::MAX`, `f64::MAX`, `f64::EPSILON`, `f64::INFINITY`, `f64::NAN`,
`std::f64::consts::PI` (also `TAU`, `E`, `SQRT_2`, `LN_2`, `FRAC_PI_2`...),
`std::cmp::{min, max}`, `std::mem::{swap, replace, take}`, `drop`.

`sin`, `cos` and `tan` are single precision (the same on every machine);
`sqrt`, `exp`, `ln` and `powf` are double precision.

## Hooks: what the game calls

A script gets called by defining a function **with one of these names**.
It may leave off parameters from the end it does not need
(`fn on_block_break(x: i64, y: i64, z: i64)`), but may not declare more
than the game passes. A hook that returns nothing is the same as one that
lets things go ahead. Hooks run on the game's main thread, one at a time:
a hook that causes another hook (say `run_command("/fire ...")` raising an event) does
not call scripts again.

| Hook | When | Return |
| --- | --- | --- |
| `fn on_load()` | the world was opened (or the script reached a running game) | |
| `fn on_save()` | the world is being saved | |
| `fn on_tick(dt: f32)` | once a frame; `dt` is seconds since the last | |
| `fn on_block_break(x: i64, y: i64, z: i64, id: String) -> bool` | the player is about to break a block | `false` keeps it |
| `fn on_block_place(x: i64, y: i64, z: i64, id: String) -> bool` | the player is about to place a block (at x, y, z) | `false` stops it |
| `fn on_entity_spawn(kind: String) -> bool` | a creature appeared (`kind` is its id) | `false` takes it away |
| `fn on_entity_death(kind: String, x: f64, y: f64, z: f64)` | a creature died, whatever the cause | |
| `fn on_entity_hit(id: i64, kind: String, x: f64, y: f64, z: f64)` | the player hit a creature (it may have died of it): its id (the one `mob_info` and the other creature calls take), its kind and where it was | |
| `fn on_player_hurt(amount: i64, cause: String) -> i64` | the player is about to take damage (`fall`, `drown`, `hunger`, `block`, `arrow`, `command`, `other`, or the id of the creature that bit) | the amount to take instead; `0` or `false` cancels. The first script to answer decides |
| `fn on_chat(text: String) -> bool` | the player typed a line (commands too, with their `/`) | `false` swallows it |
| `fn on_item_use(item: String) -> bool` | the player pressed use holding an item (once per press) | `false` stops it |
| `fn on_craft(item: String)` | the player crafted something | |
| `fn on_event(name: String, what: String)` | any event pack scripts can answer with `when` (the table in [MODDING.md](MODDING.md#events)) | |
| `fn on_effect_start(id: String, level: i64)` | a status effect started on the player (not when it is only made longer or stronger) | |
| `fn on_effect_tick(id: String, secs_left: f32, level: i64)` | about once a second for each status effect the player has | |
| `fn on_effect_end(id: String)` | an effect ran out, was put out by water or was cleared (death included) | |
| `fn on_command(name: String, args: String) -> bool` | a `/command` the game does not know (`name` without the `/`, lower case; `args` the rest of the line) | `true` if it is yours; a string is shown as the reply |
| `fn on_explode(x: f64, y: f64, z: f64, power: f64, source: String) -> bool` | an explosion is about to happen (`source` is `tnt`, `command`, a creature's id, or `mod<N>`) | `false` cancels; a number changes the power (0.5 to 8) |
| `fn on_update(dt: f32)` | once a frame, after the game has moved the creatures and before the player moves: the place to move things, to carry the player (`hold_player`) and to ask for pictures | |
| `fn on_interact(reach: f64) -> bool` | the player's hands, before the game's own handling of a click (`reach` is how far the block looked at is; `input()` says which buttons are down) | `true` if the click was used up |
| `fn on_condition(name: String, args: String) -> bool` | an `/if` condition the game does not know | `true` or `false` if it is yours; return nothing if it is not |
| `fn on_net(key: String, text: String)` | another player's copy of this mod sent a message with `net_send` | |
| `fn on_message(channel: String, from: String, text: String)` | another player's copy of this mod sent a message with `send_message` or `send_message_to` (`from` is their player name; see "Other players") | |
| `fn on_register()` | once, when the script file is read (before any world): the place to declare key bindings and switches | |
| `fn on_key_down(id: String)` | a key binding this mod declared was pressed (while playing, not in a menu or a text box) | |
| `fn on_key_up(id: String)` | that key was let go | |
| `fn on_hud(dt: f32)` | once a frame while playing: draw the HUD | |
| `fn on_ui_click(id: String)` | a button of the mod's own screen was pressed | |
| `fn on_ui_change(id: String, value: bool)` | a toggle (`bool`), stepper (a number), input (the text) or list (the line's number) of the screen changed | |
| `fn on_ui_close(name: String)` | the mod's screen closed (the player closed it, or the mod did) | |

Ids (`kind`, `item`, `id`) are the ids from the data files (`zombie`,
`diamond`, `stone`). Where more than one script is asked, the first to
cancel ends the asking.

## The host API: what scripts may do

These are the only ways out of the sandbox. Positions are block coordinates
(`y` up). All of them work in a world of your own; on a server, the ones that
change the world or the player are refused (they return `false`, or do
nothing) unless the player may run them there.

| Function | What it does |
| --- | --- |
| `say(text: &str)` | show a line in the chat |
| `log(text: &str)` | write a line to the game's log |
| `give(item_id: &str, n: i64) -> bool` | put items in the player's pack (what does not fit is set down at the player's feet); `false` if there is no such item |
| `take_item(item_id: &str, n: i64) -> bool` | remove items from the pack; `false` if the player has fewer |
| `item_count(item_id: &str) -> i64` | how many of an item the player carries |
| `held_item() -> String` | the id of the item in the player's hand (empty if none) |
| `spawn(creature_id: &str, x: f64, y: f64, z: f64) -> bool` | make a creature appear; `false` when the world already holds 200 creatures |
| `set_block(x: i64, y: i64, z: i64, block_id: &str) -> bool` | put a block somewhere (`false` if that is not loaded or not allowed) |
| `get_block(x: i64, y: i64, z: i64) -> String` | the id of the block at a place (`"air"` where nothing is loaded) |
| `player_pos() -> (f64, f64, f64)` | where the player is |
| `tp(x: f64, y: f64, z: f64)` | move the player |
| `dimension() -> String` | the dimension the player is in (empty for the home world) |
| `time() -> f64` | the time of day, 0 to 1 (0 is sunrise, 0.25 noon) |
| `set_time(t: f64)` | set the time of day |
| `explode(x: f64, y: f64, z: f64, power: f64)` | set off an explosion (power 0.5 to 8); goes through `on_explode` of the other mods |
| `effect(id: &str, secs: f64, level: i64) -> bool` | give the player a status effect |
| `apply_effect(who: &str, id: &str, secs: f64, level: i64) -> bool` | give a status effect to `"player"` or `"target"` (the creature in the crosshair, up to 8 blocks); 0 seconds is the effect's own time; false if there is no such effect or target, or a mod refused it |
| `remove_effect(who: &str, id: &str) -> bool` | take a status effect (all of them for an empty id) from `"player"` or `"target"`; false if it was not there |
| `has_effect(who: &str, id: &str) -> bool` | whether `"player"` or `"target"` has that status effect |
| `effect_level(who: &str, id: &str) -> i64` | the level of a status effect on `"player"` or `"target"`; 0 if it is not active |
| `play_sound(id: &str)` | play a sound |
| `run_command(text: &str) -> String` | carry out a chat command as the player (with or without the `/`); returns what it answers |
| `rand(n: i64) -> i64` | a random whole number from 0 up to `n` (not including `n`) |
| `player_health() -> i64` | the player's health (20 is full) |
| `heal(n: i64)` | give health back |
| `hurt(n: i64)` | hurt the player (survival; armour and hooks apply) |
| `state_get(key: &str) -> String` | what this mod saved under a key (empty if nothing) |
| `state_set(key: &str, value: &str)` | save text under a key (kept per world, in the mod's data folder) |
| `state_get_int(key: &str, default: i64) -> i64` | a saved whole number (the default if there is none) |
| `state_set_int(key: &str, value: i64)` | save a whole number |
| `mod_name() -> String` | the name of the mod this script belongs to |
| `set_shader_uniform(name: &str, value: f64) -> bool` | set the number `u_<name>` that the player's post-processing shader reads (a shader mod can declare `uniform float u_<name>;`); the value is kept within -1000 to 1000, at most 16 names; `false` for a bad name (lower-case letters, digits and `_`, starting with a letter, up to 24 long, and not one the game provides). See [SHADERS.md](SHADERS.md) |
| `block_light(block_id: &str) -> i64` | how much light a block gives off, 0 to 15 |
| `block_drop(block_id: &str) -> String` | the id of the item a block gives when broken in survival (empty if none) |
| `survival() -> bool` | whether the player plays in survival mode |
| `sneaking() -> bool` | whether the player is crouching (the sneak key is held) |
| `hud(text: &str)` | show a line of text at the top left of the screen (under the frame rate); it stays 1.5 seconds, so call it again within a second to keep it; empty text takes it away. One line per mod, at most 6 lines at once, 48 characters |
| `slot_item(slot: i64) -> String` | the id of the item in a pack slot (0 to 35, the first nine are the hotbar; empty if none) |
| `slot_count(slot: i64) -> i64` | how many items are in a pack slot |
| `move_slot(from: i64, to: i64) -> bool` | move a pack slot's items to another slot: into an empty one, onto the same item as far as it fits, else the two swap; `false` if nothing moved |
| `stack_to_chest(x: i64, y: i64, z: i64) -> i64` | put the items of the pack (not the hotbar) that the chest at that block already holds into it; how many moved (0 if there is no chest, or in a shared game) |

The calls for key bindings, the HUD and screens are in
[Controls, HUD and screens](#controls-hud-and-screens).

`run_command` is the way to everything the chat commands do (`/title`,
`/weather`, `/shake`, `/launch`, `/after 5 ...`, `/var`...): see
[MODDING.md](MODDING.md#pack-commands). It runs with exactly the player's rights.

## More host calls

The calls above are the basics. These give a mod the rest of what compiled
code could do: look at the world, handle creatures, carry the player, draw,
move things with the game's own physics, keep storage, read what packs say,
and talk to other players. Every one is bounded (counts, sizes, distances) and
works only in a running game; the ones that change the world are refused in a
shared game for a player who does not run it, as `set_block` is. They are listed
in `src/script/ext/` (one file for each table below); a test keeps the files and
this page in step.

Pictures are asked for, not drawn: in `on_update` a script calls `draw_model`,
`hud_frac_rect` and the like, and the game draws them every frame until the next
`on_update` replaces them. What a script keeps from one call to the next goes in
[saved state](#saved-state).

### The world (read only)

| Function | What it does |
| --- | --- |
| `is_loaded(x: i64, z: i64) -> bool` | whether the chunk column holding that place is loaded (`get_block` says "air" for places that are not) |
| `surface_y(x: i64, z: i64) -> i64` | the height a creature stands at on that column of the world's natural ground (one above its top block); works where nothing is loaded |
| `biome_at(x: i64, z: i64) -> String` | the natural biome of that column, lower case (`plains`, `forest`, `desert`, `tundra`, `taiga`, `mountain`, `swamp`, `savanna`, `jungle`, `badlands`) |
| `is_solid(x: i64, y: i64, z: i64) -> bool` | whether the block there stops a body |
| `block_tag(block_id: &str, tag: &str) -> f64` | the number after `tag <name>` in a block's pack lines (1 if none was given), or -1 if the block does not have the tag |
| `scan_blocks(x0: i64, y0: i64, z0: i64, x1: i64, y1: i64, z1: i64, prefix: &str) -> Vec<(i64, i64, i64, String)>` | every loaded block in the box whose id starts with `prefix` (empty: any but air), as (x, y, z, id), up to 2000 of them, in x then z then y order; the box may hold at most 40 000 cells |
| `block_ray(ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, max: f64) -> f64` | how far along a ray the first solid block is (up to `max`, at most 200), or -1 if none is in the way |
| `find_path(sx: f64, sy: f64, sz: f64, gx: f64, gy: f64, gz: f64) -> Vec<(i64, i64, i64)>` | a walkable way over the loaded blocks from the place a walker stands (feet at sx, sy, sz) toward a goal, as the cells to step on in order (up to 64; the search looks at 320 cells at most, so a far goal gives the way to the nearest point reached); empty if there is none; both ends within 150 blocks of each other |

### Creatures

| Function | What it does |
| --- | --- |
| `mobs_near(x: f64, y: f64, z: f64, radius: f64, kind: &str) -> Vec<(i64, f64, f64, f64)>` | the living creatures of one kind (empty: any) within a radius, nearest first, as (id, x, y, z); at most 64 |
| `spawn_mob(kind: &str, x: f64, y: f64, z: f64) -> i64` | make a creature appear and give its id, or -1 (no such kind, not allowed here, no room for a giant of that kind there, or the world is full of creatures: 300 at most) |
| `mob_info(id: i64) -> (String, i64, f64, f64, f64)` | (kind, health, x, y, z) of a creature; the kind is empty if it is gone or dying |
| `mob_set_pos(id: i64, x: f64, y: f64, z: f64)` | put a creature somewhere and stop it |
| `mob_set_yaw(id: i64, yaw: f64)` | turn a creature (yaw 0 faces -z; its forward is (-sin yaw, -cos yaw)) |
| `mob_set_size(id: i64, width: f64, height: f64)` | change the box a creature collides with (0.1 to 8 blocks); what it is drawn as does not change (`set_creature_size` changes both, and goes further, see "Giants, model control and state") |
| `mob_remove(id: i64)` | make a creature vanish at once: nothing drops, no `on_entity_death` |
| `mob_hurt(id: i64, n: i64)` | hurt a creature as the player's blow would (drops and `on_entity_death` follow if it dies) |
| `mob_in_view(max: f64) -> i64` | the id of the first creature on the player's look ray within `max` blocks (at most 100) with no block in the way, or -1 |

### Giants, model control and state

A creature may be far bigger than the data files allow for a kind (`height` up
to 64 and `width` up to 32 blocks, `health` up to 1,000,000 there too), and a
script may reshape a living one. The rules for the limits are the same for the
data files, these calls and native mods (`g.mobs.set_size(id, w, h)`,
`set_health`, `set_bone`, `clear_bones`, `set_orientation`, `set_state` on
`Mobs`, see [MODDING.md](MODDING.md#giant-creatures-and-model-control)); a
creature past the old limits (6 tall, 3 wide) is called a giant below.

| Function | What it does |
| --- | --- |
| `set_creature_size(id: i64, width: f64, height: f64) -> bool` | give a creature its own size in blocks (width 0.1 to 32, height 0.1 to 64): it collides and is drawn at it. A giant steps up a quarter of its width (6 blocks at the most), walks straight at its goal instead of searching a path, reaches an enemy with its whole body, stays alive and drawn from further off the bigger it is, and only appears where its body fits. False if there is no such creature, or in a shared game when this player does not run the creatures |
| `set_creature_health(id: i64, hp: i64, max_hp: i64) -> bool` | set a creature's health and its limit: the limit is kept to 1 to 1,000,000, the health to 0 (it dies) up to the limit. Healing effects stop at the limit. Same refusals as `set_creature_size` |
| `set_bone(id: i64, bone: &str, rx: f64, ry: f64, rz: f64, ox: f64, oy: f64, oz: f64, scale: f64) -> bool` | on top of the creature's animation, turn the model's bone called `bone` (any case; see `model_bones`) by `rx`, `ry`, `rz` degrees about its pivot (added to what the animation does with it), move it by `ox`, `oy`, `oz` model units and scale it (0.01 to 100; 1 is as it is). The bone's children follow it. Call it every frame to move a bone; the override stays until changed or cleared. False (and nothing happens) for an unknown bone name (the game logs it once), no such creature, a creature that already overrides 64 other bones, 256 creatures that already have overrides, or more than 4096 `set_bone` calls in a frame |
| `clear_bones(id: i64) -> bool` | take all of a creature's bone overrides away |
| `set_creature_orientation(id: i64, pitch: f64, roll: f64) -> bool` | tilt the whole model about its centre, in degrees: a positive pitch lifts the nose, a positive roll leans the top toward the creature's left (the angles are kept to -3000 to 3000; `0, 0` puts it upright). Only how it is drawn changes (not its box) |
| `model_bones(model_id: &str) -> Vec<String>` | the bone names of a model, in the model file's order and as written there (a model id, or a creature kind to ask for its model); empty if there is no such model |
| `set_creature_state(id: i64, text: &str) -> bool` | keep a short text on a creature (64 bytes at most; control characters are dropped): the mod's own record of what it is doing. Same refusals as `set_creature_size` |
| `creature_state(id: i64) -> String` | what `set_creature_state` left on a creature (empty if none, or no such creature) |

Numbers that are not finite are an error (they switch the script off, like any
bad argument); numbers that are too big or small are kept to the limits above.
Bone names that the model does not have are no error, because mods are written
against models that change.

In a shared game the player who runs the creatures sends each creature's
size, health limit, tilt, bone overrides and state to the others with the
creatures themselves (only when they change, and in full about once a second
for those who join later), so a giant is seen at its size and a mod's state is
readable on every machine with `creature_state`. Bone overrides and tilt that
a player sets for themselves are only seen on their own screen, until the host
sends overrides for that creature; set them again from `on_update` (or from
`on_message`, below) if they must last.

```rust
// A boss that rears up as its health falls, on every player's screen.
fn on_update(dt: f32) {
    for (id, x, y, z) in mobs_near(0.0, 70.0, 0.0, 200.0, "titan") {
        let (kind, hp, px, py, pz) = mob_info(id);
        if hp * 2 < 50000 {
            set_creature_orientation(id, 25.0, 0.0);
            set_bone(id, "head", 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 1.3);
            set_creature_state(id, "enraged");
        }
    }
}
```

### The player, input and the camera

| Function | What it does |
| --- | --- |
| `player_look() -> (f64, f64)` | (yaw, pitch) in radians; yaw 0 faces -z, positive pitch looks up |
| `look_dir() -> (f64, f64, f64)` | the unit vector the player looks along |
| `set_look(yaw: f64, pitch: f64)` | turn the player's view (pitch is kept within 1.5 radians of level) |
| `player_eye() -> (f64, f64, f64)` | where the player's eyes are |
| `place_player(x: f64, y: f64, z: f64)` | put the player at a place (the feet), stopped |
| `hold_player(x: f64, y: f64, z: f64)` | carry the player along: for this frame their eyes are at that place and they do not walk (set it again every frame, from `on_update`; this is how riding works) |
| `player_mode() -> String` | `survival` or `creative` |
| `player_dead() -> bool` | whether the player is dead (on the death screen) |
| `player_sleeping() -> bool` | whether the player is in bed |
| `wake()` | get the player out of bed |
| `flash(secs: f64)` | flash the screen white for a moment (up to 1 second) |
| `shake(amount: f64)` | shake the camera (0 to 3) |
| `input() -> (f64, f64, bool, bool, bool, bool, bool)` | what the player presses this frame, in `on_update` and `on_interact`: (forward -1..1, strafe -1..1, jump, descend, leave, use, attack) |
| `aim() -> (i64, i64, i64, i64, i64, i64, f64)` | the block the player looks at: (x, y, z, x, y, z of the cell in front of it, distance); the distance is -1 when nothing is in reach |
| `use_ready() -> bool` | whether the use button's cooldown is over (it is set again by `use_cooldown`) |
| `use_cooldown(secs: f64)` | start the use button's cooldown |
| `attack_ready() -> bool` | whether the attack button's cooldown is over |
| `attack_cooldown(secs: f64)` | start the attack button's cooldown |
| `take_held() -> bool` | use up one of the item in hand (nothing is used up in creative); false if the hand is empty |
| `who() -> String` | the name the player goes by |
| `is_host() -> bool` | whether this copy of the game runs the creatures and the like (always true alone; one player in a shared game) |
| `is_shared() -> bool` | whether this is a shared game (a server) |

### Pictures: HUD and models

| Function | What it does |
| --- | --- |
| `hud_frac_rect(x: f64, y: f64, w: f64, h: f64, r: i64, g: i64, b: i64, a: i64)` | fill a rectangle of the HUD this frame; x, y, w and h are fractions of the screen (0 to 1), the colour and opacity 0 to 255; at most 64 a frame |
| `hud_frac_text(x: f64, y: f64, scale: f64, text: &str, r: i64, g: i64, b: i64)` | write text on the HUD this frame; x and y are fractions of the screen, `scale` times the size of the game's own interface (0.5 to 8); at most 32 a frame |
| `draw_model(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, tint: f64)` | draw a model (a model id, or the id of a creature to use its model) in the world this frame, `height` blocks tall; `phase` is the walk or wheel cycle in radians; `tint` is 0 to 1 for a fixed brightness, or -1 for the light of the place; at most 200 a frame |
| `draw_model_own(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, tint: f64)` | as `draw_model`, for the thing the player sits in or wears: it is drawn only when the player sees themselves from outside (third person) |
| `draw_model_posed(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, stride: f64, head_pitch: f64, head_yaw: f64, clock: f64, tint: f64)` | as `draw_model` with the pose of a creature: `stride` 0 (still) to 1 (full stride), where the head looks relative to the body in degrees, and `clock`, the seconds it has been alive (tails wag and ears twitch on their own) |
| `draw_speck(x: f64, y: f64, z: f64, r: i64, g: i64, b: i64)` | draw one small opaque speck of colour in the world this frame; at most 300 a frame |

### Physics bodies

| Function | What it does |
| --- | --- |
| `body_new(width: f64, height: f64, x: f64, y: f64, z: f64) -> i64` | make a physics body (a box on the ground at x, y, z, `width` by `height` blocks, up to 6 each) and give its id, or -1 if there are too many bodies (256 over all mods) |
| `body_free(id: i64)` | forget a body |
| `body_step(id: i64, dt: f64, forward: f64, strafe: f64, jump: bool, descend: bool, flying: bool, speed: f64)` | move a body for `dt` seconds (at most 0.1) as a walker would: `forward` and `strafe` -1..1 along its yaw, `jump`, `descend`, `flying` (no gravity: up on `jump`, down on `descend`), `speed` a multiplier of the walking speed (0 to 8; the player walks 4.8 blocks a second at 1, flies at 11); does nothing where the world is not loaded |
| `body_pos(id: i64) -> (f64, f64, f64)` | where a body's feet are |
| `body_exists(id: i64) -> bool` | whether a body with that id is there (bodies live only as long as the game session: a saved id is stale after the world is opened again) |
| `body_set_pos(id: i64, x: f64, y: f64, z: f64)` | put a body somewhere; its velocity is kept (`body_set_vel` stops it) |
| `body_vel(id: i64) -> (f64, f64, f64)` | a body's velocity in blocks a second |
| `body_set_vel(id: i64, x: f64, y: f64, z: f64)` | set a body's velocity |
| `body_yaw(id: i64) -> f64` | which way a body faces (yaw 0 faces -z) |
| `body_set_yaw(id: i64, yaw: f64)` | turn a body |
| `body_state(id: i64) -> (bool, bool)` | (on the ground, in water or lava) |
| `body_ray(id: i64, ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, max: f64) -> f64` | how far along a ray (up to `max`, at most 50) the box of a body is hit, or -1 if it is missed |

### Storage on the chest screen

| Function | What it does |
| --- | --- |
| `storage_open(key: &str) -> bool` | open this mod's storage of that name on the chest screen (made empty if it is new); false if there are too many storages (128 over all mods) or the key is empty or longer than 64 bytes |
| `storage_dump(key: &str) -> String` | what a storage holds, as text to keep with `state_set` (empty if it holds nothing or there is none) |
| `storage_load(key: &str, text: &str)` | fill a storage from text made by `storage_dump` (made if it is new) |
| `storage_count(key: &str) -> i64` | how many items a storage holds |
| `storage_to_player(key: &str) -> i64` | move everything a storage holds into the player's pack and give how many items did not fit (they stay in the storage) |
| `storage_drop(key: &str)` | forget a storage and what it holds |

### What the packs say

| Function | What it does |
| --- | --- |
| `pack_lines(key: &str) -> Vec<Vec<String>>` | the words after each claimed top-level line that starts with `key`, in the order the packs were read (at most 500) |
| `pack_sections(kind: &str) -> Vec<String>` | the ids of the claimed sections of a kind (`vehicle`...), in the order the packs were read (at most 500) |
| `pack_prop(kind: &str, id: &str, key: &str, default: &str) -> String` | the words after the line `key` in a claimed section, joined with spaces, or `default` if the section or the line is not there |
| `pack_props(kind: &str, id: &str) -> Vec<(String, String)>` | every line of a claimed section as (key, words joined with spaces) |

### Files in `tmp`

| Function | What it does |
| --- | --- |
| `tmp_read(name: &str) -> String` | the text of a file in this mod's `tmp` folder (empty if there is none, or it is not text) |
| `world_file_read(name: &str) -> String` | the text of a file the world's save keeps for a mod's old compiled code (`<world>.<name>`, read only, and only `vehicles.txt` and `manfromthefog.txt`), empty if there is none or the game is shared: for a script that takes over from the code and reads what the code saved once |
| `tmp_write(name: &str, text: &str) -> bool` | write a file in this mod's `tmp` folder; false if the name is not plain (letters, digits, `.`, `_`, `-`, up to 64 bytes), the text is over 256 KiB, or the mod already has 64 files |

### Other players

| Function | What it does |
| --- | --- |
| `net_send(key: &str, text: &str) -> bool` | send a message to this mod's scripts on the other players' games (`on_net`); false when the game is not shared or the key is over 64 bytes (the mod's name and the key together) or the text over 8000 |

Messages with a channel and a player to answer (all the pieces are checked by
the server, so a modified game cannot flood or confuse the others):

| Function | What it does |
| --- | --- |
| `send_message(channel: &str, text: &str) -> bool` | send a message to this mod's scripts (and native code) on every other player's game; they hear it as `on_message(channel, from, text)`. `channel` is a name of your choosing (printable ASCII; with the mod's name at most 64 bytes together), the text at most 512 bytes without control characters (a newline and a tab are fine). Each player may send 8 a second; the server drops what is over. False when the game is not shared, the channel or text is not fit, or this player is over the rate. The sender does not hear its own message |
| `send_message_to(player: &str, channel: &str, text: &str) -> bool` | the same, only to the player with that name (any case); false if no such player is in the game |

`fn on_message(channel: String, from: String, text: String)` is called on the
receiving machine, once for each message, with the sender's player name. It
is how a mod whose logic runs only on the host (the player who runs the
creatures) shows its effects on the clients: the host calls
`send_message("boom", "10 70 3")`, each client answers in `on_message` with its
own sounds, flashes and `set_bone` calls. A client may send too (to the host
or to everyone); trust nothing in `text` more than you would trust a chat line.

```rust
fn on_entity_death(kind: String, x: f64, y: f64, z: f64) {
    // The game that runs the creatures sees the death; everyone else is told.
    if kind == "titan" {
        send_message("titan_died", &format!("{:.0} {:.0} {:.0}", x, y, z));
    }
}

fn on_message(channel: String, from: String, text: String) {
    if channel == "titan_died" {
        say(&format!("{} saw the titan fall at {}", from, text));
    }
}
```

### Shots, sky, pushes

| Function | What it does |
| --- | --- |
| `shoot(x: f64, y: f64, z: f64, dx: f64, dy: f64, dz: f64, speed: f64, damage: i64, hostile: bool, r: i64, g: i64, b: i64) -> bool` | send a shot flying from a place along a direction (blocks a second, up to 60; it hurts what it hits for `damage`, up to 100, and the player too when `hostile`), drawn in the colour; false if there are too many in flight (200) or it is not allowed here |
| `force_sky(rain: f64)` | force the sky to a fixed weather, 0 (clear) to 1 (storm), as `/weather` does; a negative number gives the sky back to the weather |
| `mob_set_vel(id: i64, x: f64, y: f64, z: f64)` | set a creature's velocity in blocks a second (up to 60 in each direction): a push or a leap |
| `clear_effect(id: &str) -> bool` | take a status effect off the player (a cure); false if they did not have it |
| `push_player(x: f64, y: f64, z: f64)` | add to the player's velocity in blocks a second (up to 40 in each direction) |

### Lines and sections for scripts

A pack can keep top-level lines and indented sections the game itself does not
know, for a script to read, by saying so in its `pack.txt`:

```
claims line manfromthefog
claims section vehicle
```

From then on, in every pack read in the same loading (whichever comes first),
`manfromthefog min_spawn_rate 1000` is a line and `vehicle boat` with its
indented lines is a section the game keeps instead of warning about. A script
reads them with `pack_lines("manfromthefog")` and `pack_sections("vehicle")`.
The words the game itself reads (`block`, `item`, `texture`...) cannot be claimed.

## Controls, HUD and screens

A mod can have keys of its own, draw on the HUD and open its own screens.
The same calls exist for native code ([MODDING.md](MODDING.md#controls-hud-and-screens));
they share their limits.

### Key bindings

Declare them in `on_register`, which runs once when the script is read (the
only place `bind_key` and `add_switch` work). Each gets an id, a label, a group
and a default key. They appear in **Settings > Controls > Key bindings**, after
the game's own, under a heading with the mod's name (and the group, if any);
the player can rebind them, the choice is saved in `settings.txt`, and two
bindings on one key are outlined in red but both still work.

| Function | What it does |
| --- | --- |
| `bind_key(id: &str, label: &str, group: &str, key: i64) -> bool` | in `on_register` only: add a binding. `key` is a key code or a name such as `"g"`, `"f5"`, `"space"`, `"up"`, `"shift"` |
| `add_switch(id: &str, label: &str, default: bool) -> bool` | in `on_register` only: add an on/off row to Settings > Game > Mod switches and reload |
| `key_scancode(name: &str) -> i64` | the key code of a key name, 0 if it is not one |
| `key_held(id: &str) -> bool` | whether the binding's key is down now |
| `setting_on(id: &str) -> bool` | whether one of the mod's switches is on (its default until the player changes it) |

`on_key_down(id)` and `on_key_up(id)` are called for the mod's own bindings
only, not while typing, in a menu or on a mod screen; a key let go is always
reported. A mod may declare 64 bindings and 16 switches; ids are letters,
digits, `_` and `-` up to 32 characters, labels are cut at 40.

### Drawing on the HUD

Draw from `on_hud(dt)`: it runs once a frame, and what was drawn last frame is
forgotten, so draw it all again. Every call returns `false` (and does nothing)
when the frame's drawing commands are used up: **512 in all, 256 for one mod**.
Text is cut to 120 characters; sizes and distances are in interface pixels (the
game's pixels, scaled with the interface size setting); everything is clipped to
the window, an icon is drawn only if it fits whole.

An `anchor` is one of `"top-left"`, `"top"`, `"top-right"`, `"left"`,
`"centre"`, `"right"`, `"bottom-left"`, `"bottom"`, `"bottom-right"`. The thing
drawn is put so that its matching corner, edge or middle sits on that point of
the screen, then moved `dx` right and `dy` down (so `hud_text("bottom-right",
-4.0, -4.0, ...)` is a line four pixels in from the corner). A `colour` is
`0xRRGGBB`, or `0xAARRGGBB` (a number above `0xFFFFFF` carries its own alpha).

| Function | What it does |
| --- | --- |
| `hud_text(anchor: &str, dx: f64, dy: f64, scale: f64, text: &str, colour: i64) -> bool` | text, `scale` times the normal size (0.5 to 4) |
| `hud_rect(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, colour: i64) -> bool` | a filled rectangle |
| `hud_outline(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, thick: f64, colour: i64) -> bool` | a rectangle's outline, `thick` pixels thick (1 to 16) |
| `hud_item(anchor: &str, dx: f64, dy: f64, size: f64, item_id: &str) -> bool` | an item's icon, `size` pixels square |
| `hud_block(anchor: &str, dx: f64, dy: f64, size: f64, block_id: &str) -> bool` | a block's icon |
| `hud_bar(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, fraction: f64, colour: i64) -> bool` | a progress bar, `fraction` from 0 to 1 |

A line that should stay (a status, a counter) can live in a **slot**, and is
laid out for you next to the game's own HUD. It stays until the mod changes or
removes it, so set it when something changes rather than every frame. A mod
may keep 16 lines (64 in all).

| Slot | Where |
| --- | --- |
| `"above_hotbar"` | centred above the hotbar, growing upward |
| `"beside_hearts"` | to the left of the hearts, growing upward |
| `"top_left_list"` | a list down the top left, under the frame rate |
| `"under_crosshair"` | centred under the crosshair, growing downward |

| Function | What it does |
| --- | --- |
| `hud_set(slot: &str, id: &str, text: &str, colour: i64) -> bool` | put a line in a slot, or change the line with this id |
| `hud_set_icon(id: &str, item_id: &str) -> bool` | give that line an item icon in front (an empty id takes it away) |
| `hud_remove(id: &str) -> bool` | take the line away |
| `toast(text: &str, secs: f64) -> bool` | a boxed message at the top that fades after `secs` (0.5 to 30); up to 8 wait at once |
| `action_bar(text: &str, secs: f64) -> bool` | one line above the hotbar, replaced by the next |

What a HUD may read, besides `player_health()`, `player_pos()`, `held_item()`,
`time()` and `dimension()`:

| Function | What it gives |
| --- | --- |
| `hunger() -> i64` | the player's hunger (20 is full) |
| `air() -> f64` | breath left (10 is full) |
| `biome() -> String` | the biome under the player, lower case |
| `selected_slot() -> i64` | the hotbar slot chosen, 0 to 8 |
| `heading() -> f64` | the way the player faces in degrees: 0 north (toward -z), 90 east (+x), 180 south, 270 west |
| `fps() -> f64` | frames a second |

### Screens of your own

A mod opens a screen (from a key binding, a command, anything), fills it row
by row with the widgets the settings use, and hears about the player's clicks.
One screen is open at a time; opening another closes the first. A screen takes
up to 48 widgets and scrolls when it is taller than the window; Escape or CLOSE
closes it. Widget ids are unique on a screen and follow the rules of key ids.

| Function | What it does |
| --- | --- |
| `ui_open(name: &str, title: &str) -> bool` | open an empty screen called `name`; add the widgets after it |
| `ui_close() -> bool` | close the mod's screen |
| `ui_text(text: &str) -> bool` | a line of text |
| `ui_button(id: &str, label: &str) -> bool` | a button: `on_ui_click(id)` |
| `ui_toggle(id: &str, label: &str, on: bool) -> bool` | an on/off toggle: `on_ui_change(id, value)` with a `bool` |
| `ui_stepper(id: &str, label: &str, value: i64, min: i64, max: i64) -> bool` | a number you step with the mouse: `on_ui_change` with the number |
| `ui_input(id: &str, label: &str, value: &str) -> bool` | a text box (60 characters): `on_ui_change` with the text, as it is typed |
| `ui_list(id: &str, items: &str, selected: i64) -> bool` | a scrolling list of 5 lines; `items` are separated by `\n` (up to 128): `on_ui_change` with the line's number |
| `ui_set_text(id: &str, text: &str) -> bool` | change a button's, toggle's or stepper's label, an input's text, or a list's items |
| `ui_set_line(n: i64, text: &str) -> bool` | change the text of the `n`th widget (counting from 0) if it is a text line |
| `ui_set_value(id: &str, n: i64) -> bool` | change a toggle (0 or 1), a stepper or a list's chosen line (-1 for none) |
| `ui_get_text(id: &str) -> String` | an input's text, or the text of a list's chosen line |
| `ui_get_value(id: &str) -> i64` | a toggle's (0 or 1), stepper's or list's (-1 for none) value |

`on_ui_click`, `on_ui_change` and `on_ui_close` are called after the frame the
player used the screen in, for the mod that owns it. A mod can change what is
shown at any time (from `on_tick` too) with `ui_set_text`, `ui_set_line` and
`ui_set_value`.

### Safety

Everything above is bounded in the game, not by the script's good manners:
drawing commands, key bindings, switches, HUD lines, toasts, widgets, list
lines, text and ids all have caps (the numbers above), text is cut to
printable ASCII and upper case, NaN and enormous numbers are brought into range
and a window position off the screen draws nothing. The calls take a constant
time, so a script cannot slow a frame by asking for too much; one that loops
for ever is stopped by the step budget of the call (see
[Limits](#limits-errors-and-safety)). An error in a script (a wrong anchor or
slot name, `bind_key` outside `on_register`, a key name that is not one) switches that
file off and nothing else, the same as any other error.

### A worked example: a compass and a stats screen

The scripting-demo mod has this: **G** switches a compass on the HUD, **H**
opens a small screen of facts.

```rust
fn on_register() {
    bind_key("compass", "Toggle compass", "Demo", "g");
    bind_key("stats", "Open stats", "Demo", "h");
    add_switch("compass_on_start", "Compass on at start", true);
}

fn compass_text(deg: f64) -> String {
    let names = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let i = ((deg + 22.5) / 45.0) as i64 % 8;
    format!("{} {:.0}", names[i as usize], deg)
}

fn on_key_down(id: String) {
    if id == "compass" {
        let shown = state_get_int("compass", if setting_on("compass_on_start") { 1 } else { 0 }) == 0;
        state_set_int("compass", if shown { 1 } else { 0 });
        toast(if shown { "COMPASS ON" } else { "COMPASS OFF" }, 1.5);
    } else if id == "stats" {
        ui_open("stats", "Stats");
        ui_text(&format!("HEALTH {}   HUNGER {}", player_health(), hunger()));
        ui_text(&format!("BIOME {}", biome()));
        ui_button("close", "Close");
    }
}

fn on_ui_click(id: String) {
    if id == "close" {
        ui_close();
    }
}

fn on_hud(dt: f32) {
    if state_get_int("compass", if setting_on("compass_on_start") { 1 } else { 0 }) == 0 {
        return;
    }
    let text = compass_text(heading());
    hud_rect("top", 0.0, 4.0, 60.0, 12.0, 0xA0000000);
    hud_text("top", 0.0, 7.0, 1.0, &text, 0xFFFFFF);
}
```

## Saved state

`state_set` and `state_get` keep text under a key, for **this mod** and
**this world**. It is written when the world is saved, to
`<data folder>/mods-data/<mod>/world-<world>.txt`, and read back when the world
is opened. At most 512 keys, keys up to 64 bytes, values up to 4096 bytes.
`state_get_int` and `state_set_int` keep whole numbers the same way. Several
script files of one mod see the same state, which is how they talk to each other.
Nothing else survives between calls: a script's variables exist only while
one call runs.

## Limits, errors and safety

A script has no way to read or write a file (except the plain text files of
`tmp_read` and `tmp_write` in its own `tmp` folder), reach the network (except
messages to the same mod on other players' games), start a program, or see
anything the host API does not give it.

Every call into a script (a hook, a command) has a budget:

| Limit | Value |
| --- | --- |
| evaluation steps (one per expression and per loop turn) | 200 000 |
| nested function calls (recursion) | 40 |
| nested expressions while running | 500 |
| values made (cells: list elements, structs, 16 bytes of string...) | 400 000 |
| longest string | 100 000 bytes |
| longest `Vec` | 50 000 elements |
| source file | 256 KiB, 64 levels of nesting |

Going over a limit is an error like any other; it never hangs or crashes the
game. **An error in a script is logged, with `file:line`, and that file is
switched off for the rest of the session** (the other files and mods carry
on). The log line looks like

```
voxelrust: script error in `on_chat`, the script is switched off: boss/scripts/main.rs:12: index out of bounds: the length is 1 but the index is 3
```

Errors found when the file is read (`boss/scripts/main.rs:5:13: expected an
expression, found `;``) leave that file out; the rest of the mod loads.

Scripts act with the player's rights. A mod from a server can therefore do
what the player could type in the chat, in the game you join; take mods from
servers you trust, as with any mod.

## Checking a script

```
voxelrust --script-check scripts/main.rs
```

reads a file the way the game would and prints the mistake (with its line
and column), or `ok` with the hooks it defines. It warns about a function named
like a hook (`on_blok_break`) that is not one, and about a hook that
declares more parameters than the game passes. It also looks at every call by
name (`script/lint.rs`): a call to a function that is not the file's own, a host
function or the library (`sayy("x")`), or a host function or the file's own
function given the wrong number of arguments, is an error with its line. It
opens no window.

## Scripts and compiled code

A mod can have both `scripts/` and `code/` (compiled Rust, see
[MODDING.md](MODDING.md#code-that-mods-bring)) with the same logic. The script is
the default: a normal build has no compiled mod code at all. The opt-in
`native-mods` build compiles `code/` in, and then it runs **instead of** the mod's
scripts, which are left unread (the game logs that), so the two never do the same
thing twice. `docs/SCRIPT-COMPAT.md` says which mods run as scripts and what is
left that scripts cannot do.

## Effects

Effects are defined in a pack with `effect` lines ([DATA.md](DATA.md#effects):
damage, healing, speed, mining speed, levitation, blindness, a screen tint,
hearts, a sound...), and a script gives them out, asks about them and answers
their start, tick and end. `effect(id, secs, level)` is the player-only short
form of `apply_effect("player", ...)`. The tick hook is called about once a
second, not each frame, so it is cheap.

```rust
fn on_effect_start(id: String, level: i64) {
    if id == "gloom" {
        say("THE LIGHT GOES OUT OF THE WORLD");
    }
}

fn on_effect_tick(id: String, secs_left: f32, level: i64) {
    // A lantern keeps the gloom off the player.
    if id == "gloom" && held_item() == "lantern" {
        remove_effect("player", "gloom");
    }
}

fn on_effect_end(id: String) {
    if id == "levitation" && !has_effect("player", "water_breathing") {
        apply_effect("player", "water_breathing", 5.0, 1);
    }
}
```

## Worked examples

### A reward for killing a boss

```rust
const BOSS: &str = "wither_boss";

fn on_entity_death(kind: String, x: f64, y: f64, z: f64) {
    if kind != BOSS {
        return;
    }
    let kills = state_get_int("boss_kills", 0) + 1;
    state_set_int("boss_kills", kills);
    give("diamond", 5);
    effect("regeneration", 20.0, 2);
    play_sound("levelup");
    say(&format!("THE BOSS FELL AT {:.0} {:.0}. KILLS SO FAR: {}", x, z, kills));
}
```

### A `/patchup` command

(`/heal` is already a command of the game, which wins; a script gets the
commands the game does not know.)

```rust
fn on_command(name: String, args: String) -> bool {
    if name != "patchup" {
        return false;
    }
    let amount: i64 = args.trim().parse().unwrap_or(6);
    if item_count("bandage") < 1 {
        say("YOU NEED A BANDAGE");
        return true;
    }
    take_item("bandage", 1);
    heal(amount.clamp(1, 20));
    say(&format!("PATCHED UP FOR {}", amount));
    true
}
```

To answer, call `say`. (A command hook may also return a string instead of
`true`, and the game shows it as the reply; types are not checked, so this
works if the function has no declared return type.)

### Reacting to TNT

```rust
// Big charges ask first; small ones get a boom of their own.
fn on_explode(x: f64, y: f64, z: f64, power: f64, source: String) -> bool {
    if source != "tnt" {
        return true;
    }
    let (px, py, pz) = player_pos();
    let (dx, dy, dz) = (px - x, py - y, pz - z);
    let near = (dx * dx + dy * dy + dz * dz).sqrt() < 6.0;
    if near && power > 3.0 {
        say("THAT ONE WAS TOO CLOSE. NO.");
        return false;
    }
    play_sound("boom");
    true
}

fn on_event(name: String, what: String) {
    if name == "explode" && what == "tnt" {
        let n = state_get_int("blasts", 0) + 1;
        state_set_int("blasts", n);
    }
}
```

### A little game of its own

```rust
struct Counter { name: String, hits: i64 }

impl Counter {
    fn new(name: &str) -> Counter { Counter { name: name.to_string(), hits: 0 } }
    fn hit(&mut self) -> bool { self.hits += 1; self.hits % 10 == 0 }
}

fn on_block_break(x: i64, y: i64, z: i64, id: String) {
    let mut c = Counter::new(&id);
    c.hits = state_get_int(&format!("broken_{}", id), 0);
    if c.hit() {
        say(&format!("{} {} BROKEN", c.hits, c.name.to_uppercase()));
    }
    state_set_int(&format!("broken_{}", id), c.hits);
}
```

## For the game's own code

- `ScriptHost::load_mod(name: &str, files: &[(String, Vec<u8>)]) -> Vec<String>`
  (`script/mod.rs`) reads the scripts of one mod. `files` are the mod's files as
  (path inside the mod folder, bytes); the ones named `scripts/*.rs` are
  taken, the rest ignored. It returns the problems found (each already
  `mod/scripts/file.rs:line:col: message`; a file with a problem is left out).
  Loading a mod again replaces it. It can be called before a world is open or
  during play (`on_load` runs when the next hook is called).
- `ScriptHost::unload_all()` forgets every script.
- `ScriptHost::set_data_dir(base)` says where saved state goes (done at start-up).
- `ScriptHost::is_live()` is one atomic read; every dispatcher in
  `game/hooks.rs` and `script/bridge.rs` tests it first, so a game with no scripts
  pays nothing.
- The scripts register as one more listener beside the native hooks of
  `native::Native`: `game/hooks.rs` calls `script/bridge.rs` after the native hooks
  of each kind; unknown chat commands (`Session::run`) and explosions
  (`explosion::explode`) have a call of their own. To add a hook, add a `HookDoc`
  to `script::HOOKS`, a function to `bridge.rs`, the call where the event happens,
  and a row to the table above (a test checks the docs list every hook).
- The later host calls (`heading` to `stack_to_chest`) are written in `script/host_qol.rs`, and `hud` keeps its lines in `script/hud.rs` (`Session::script_hud`, drawn by `client/app.rs`).
- To add a host function, add it to `Host` (`script/host.rs`), to
  `SessionHost`, to `call_host_fn`, to `HOST_FUNCTIONS` and to the table above (tests
  check all four agree). The functions for controls, the HUD and screens are in
  `script/host_ui.rs` (`UI_FUNCTIONS`, one thin call each into `ModApi` of
  `client/modui/api.rs`, which holds the limits); the state they change is a
  `ModUi` that the host hands out with `Host::ui()`.
- What a script declares in `on_register` (key bindings, switches) is kept in
  `ScriptHost::decls`; the Settings pages read it, with the native mods' own
  tables, through `client/modui/keys.rs`. Key presses reach a mod's scripts with
  `ScriptHost::call_hook_in`, which asks only the mods it is told to.
