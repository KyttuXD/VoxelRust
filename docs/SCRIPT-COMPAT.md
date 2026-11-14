# Mods and the script engine

Which mods run through the [script engine](SCRIPTING.md) with no compiled code,
what each one needs, and what is left that scripts cannot do. The game ships and
builds with no mods; a mod is a folder in `<data>/mods` (or sent by a server) and
runs from its pack text and its `scripts/*.rs`. The opt-in `native-mods` build
compiles in the `code/` of the mods that have it, and runs **that instead of** the
mod's script, never both (`script::native_wins`).

The table is printed by `tools/script_compat.py` (`--markdown` for this form; it
reads `VOXELRUST_MODS_DIR`), from the host calls the scripts use and from the
engine calls the compiled code makes. The game test `script/mods_tests.rs` loads
every script of every mod except the two protected ones through the runtime
(parse, the call check of `script/lint.rs`, hook names) and loads every pack with
the lines the scripts read kept; it is skipped when `VOXELRUST_MODS_DIR` is not set.

| mod | how it runs | blocks | creatures | player | commands | per-frame | clicks | block hooks | damage hooks | conditions | HUD | world drawing | sounds | saved state | files in tmp | pack lines | bodies | storage UI | networking | projectiles | sky | effects | pathfinding | custom screens | world generation | HTTP |
| --- | --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| earth | native only (terrain and HTTP are not scriptable) | c |  | c |  | c |  |  |  |  |  |  |  |  | c | c |  |  | c |  |  |  |  | X | X | X |
| manfromthefog | script (native code kept for native-mods) | S | S | S | S | S |  |  | S |  | S | S | S | S |  | S |  |  |  |  |  | S | S |  |  |  |
| orespawn | report only (protected) | c | c | c | c | c | c |  | c |  |  |  | c |  |  |  |  |  |  |  |  |  |  |  | X |  |
| scripting-demo | script |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| trains | script (native code kept for native-mods) | S |  |  | S | S |  | S |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| vehicles | script (native code kept for native-mods) | S |  | S | S | S | S |  |  | S |  | S | S | S |  | S | S | S | S |  |  |  |  |  |  |  |
| witherstorm | report only (protected) | c | c | c | c | c | c |  | c |  |  | c | c | c | c |  |  |  |  | c | c |  |  |  |  |  |

`S` a script of the mod uses it (the engine covers it). `c` the mod's compiled code uses it and the engine has host calls for it. `X` compiled code uses it and the engine has no way to do it.

Data only (nothing to run; every line loads through the runtime loader): boats, caves, colours, cooking, disasters, explosives, farming, furniture, hearthgarden, holidays, landvehicles, monsters, planes, stonework, tools-extra, villagers, wanderer, wildlife, woods.

## What each mod needs

| Mod | Needs | Runs as |
| --- | --- | --- |
| boats, planes, landvehicles | `vehicle` sections (read by the Vehicles script); data lines for blocks, items, models, recipes | data only; needs `vehicles` switched on |
| trains | rails: scan blocks, set blocks, place/break hooks, `/rails`; `vehicle` sections of its own (read by Vehicles) | `scripts/rails.rs`; the pack's `tag track/station/slope` lines are read by the Vehicles script |
| vehicles | physics bodies, input, carrying the player, drawing models, storage, networking, `/if driving`, saved state | `scripts/vehicles.rs` |
| manfromthefog | creatures (find, spawn, move, resize, remove), the camera, HUD and models, the pack's settings lines, effects, sleep event, saved timers, path finding | `scripts/fog.rs` |
| scripting-demo | `on_event`, `run_command` | `scripts/demo.rs` |
| earth | a terrain source for a new world kind and its new-world screen, a map screen, settings rows, and **HTTP** for the map tiles | **native only**, see below |
| orespawn, witherstorm | see "The two protected mods" | compiled code (report only) |
| caves, colours, cooking, disasters, explosives, farming, furniture, hearthgarden, holidays, monsters, stonework, tools-extra, villagers, wanderer, wildlife, woods | data lines only (blocks, items, creatures, recipes, models, `when` rules, episodes, dimensions, settings) | data only, loaded by the runtime pack loader |

How a ported mod differs from its compiled code:

- **trains**: the same rules (`shape.rs`) as a script; the rails are read into a grid with
  `scan_blocks` and at most 40 of them are looked at per call (the next look carries on), so a very
  long track settles over a few looks instead of one. The check that every piece of track has a block
  (the compiled code's `check`) is not run at load; the compiled code's test still covers it.
- **vehicles**: each vehicle is a physics body of the engine (`body_*`) plus a line of saved state
  (`v<id>`, `p<id>`, `sd<id>`); vehicles are named by a stable number instead of a list position.
  Worlds saved by the compiled code (`<world>.vehicles.txt`) are read once by `world_file_read` and
  taken over. The shared-game messages are the same ideas (the host's game sends the whole list, the
  others ask) over `net_send` and `on_net`; the vehicle's own model is drawn only in third person.
- **manfromthefog**: the same timer, stages, jumpscare and darkness. Random numbers come from the
  engine's `rand` instead of a fixed-seed hash (so a world does not repeat the same night). The
  timers are kept in the mod's saved state; the compiled code's `<world>.manfromthefog.txt` is read
  once and taken over. The walk of a stalker uses `find_path` and keeps six cells of the way.

## Earth stays compiled

Earth is a terrain source (`terrain_open`, a `worldgen::Source` asked for every chunk of land), a new-world
screen, a map screen and a settings page, and it fetches map tiles over **HTTP** into
`<data>/tmp/earth`. Scripts have none of these: terrain is asked for 65 536 columns at a time and
a script would be far too slow for it, the screens are drawn with the game's `Ui`, and there is
deliberately no network access for scripts, so a mod downloaded from a server cannot reach the
internet. It runs as native code in the `native-mods` build and is otherwise not available. The
parts that are only data (its pack) load as usual.

## The two protected mods

`orespawn` and `witherstorm` were read, not edited. What their code needs, and what the engine now
has for it (the host calls marked *new* were added in this round for them and for the ports):

| Feature | Engine |
| --- | --- |
| orespawn: the ORESPAWN world kind, the land of five dimensions (`terrain_*`, `dimension_terrain`) | **missing** (compiled `Source` only) |
| orespawn: creature abilities each frame (eating blocks, teleporting to the player, blowing up, tearing through blocks, the Molenoid) | `on_update`, `mobs_near`, `mob_set_pos`, `mob_remove`, `mob_hurt`, `set_block`, `explode` |
| orespawn: armour, poison, fire, slowness from blows; the King's and Queen's rage | `on_player_hurt` (the cause is the creature's id), `effect`, `on_entity_hit` (new), `mob_info` (new) |
| orespawn: Kraken from a slain Sea Squid, the Kraken carrying the player off, summons | `on_entity_death`, `spawn_mob` (new), `hold_player` and `place_player` (new) |
| orespawn: tree seeds, `/dungeon` | `on_item_use` with `aim()`, `on_command`, `set_block` |
| orespawn: bespoke targeting and pets | **missing**: no way to give a creature a goal or a target (only to move it by hand each frame) |
| witherstorm: the storm's body with one proxy creature to hit | `spawn_mob`, `mob_set_size`, `mob_set_pos`, `mob_hurt`, `mob_in_view`, `on_entity_hit` (new) |
| witherstorm: flight and aiming, tractor beams, skulls, bite, wither sickness | maths in the script; `shoot`, `push_player`, `hurt`, `effect`, `clear_effect` (new) |
| witherstorm: eating terrain in clusters, queued explosions | `scan_blocks` (40 000 cells a call, 2 000 results), `set_block`, `explode` |
| witherstorm: tainting blocks and creatures | `scan_blocks`, `set_block`, `mobs_near`, `mob_remove`, `spawn_mob`; the creature's look variant is not reachable |
| witherstorm: the Formidibomb, Super TNT, blade, Eye, Phasometer, amulet, cure | `on_item_use`, `aim()`, `explode`, `hud_frac_rect` and `hud_frac_text`, `clear_effect`; lit TNT with a fuse is **missing** |
| witherstorm: the summoning ritual, the Super Beacon, `/witherstorm` | `on_block_place`, `scan_blocks`, `on_command` |
| witherstorm: drawing the storm in pieces, heads, tentacles | `draw_model_posed` (new): one pose for a whole model; separate animation of parts is **missing** |
| witherstorm: beams (`draw_quads`) | **missing** |
| witherstorm: sparks, sky, shake, sounds | `draw_speck`, `force_sky` (rain only, not tint), `shake`, `play_sound` (not placed in space) |
| witherstorm: saved state (`witherstorm.txt`) | `state_*` (512 keys of 4 096 bytes); a storm's whole record fits |

Missing host calls, in the order they would unblock the most:

1. **World generation hooks** (a world kind and per-dimension terrain): needed by orespawn and by Earth.
   Not a host call but a different design: a data-driven land (noise parameters in the pack) would be
   the safe way, since a script cannot be asked for every column of a chunk.
2. **A goal for a creature** (`mob_goto(id, x, y, z)` / `mob_target(id, ...)`), so a script steers
   creatures with the game's own path finding and AI instead of moving them by hand: orespawn's targeting,
   pets and the bosses' phases.
3. **Drawing quads and beams** (`draw_beam(from, to, width, colour)`): witherstorm's tractor beams.
4. **Lit TNT** (`prime_tnt(x, y, z, fuse)`): the Formidibomb and Super TNT (`explode` gives the blast but
   not the burning fuse), and **parts of a model animated apart** (a pose for each named part).
5. **Bigger or streamed world scans** and a **sky tint**: the storm eating a whole hillside, and its dark sky.
6. **A positioned sound** (`play_sound_at(id, x, y, z)`).

The first, second and third cannot be done by a script on its own at all; the rest only make
witherstorm's behaviour exact. Nothing in either mod was edited, and the engine's host calls are written
so that neither mod needs to change to use them.
