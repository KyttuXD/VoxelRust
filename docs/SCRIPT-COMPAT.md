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
  others ask) over `net_send` and `on_net` (a new mod would use `send_message` and `on_message`, which name the sender and are rationed by the server); the vehicle's own model is drawn only in third person.
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
| orespawn: the ORESPAWN world kind, the land of five dimensions (`terrain_*`, `dimension_terrain`) | **missing** (compiled `Source` only; its structures are covered by `on_chunk_generated` above) |
| orespawn: creature abilities each frame (eating blocks, teleporting to the player, blowing up, tearing through blocks, the Molenoid) | `on_update`, `mobs_near`, `mob_set_pos`, `mob_remove`, `mob_hurt`, `set_block`, `explode` |
| orespawn: armour, poison, fire, slowness from blows; the King's and Queen's rage | `on_player_hurt` (the cause is the creature's id), `effect`, `on_entity_hit` (new), `mob_info` (new) |
| orespawn: Kraken from a slain Sea Squid, the Kraken carrying the player off, summons | `on_entity_death`, `spawn_mob` (new), `hold_player` and `place_player` (new) |
| orespawn: tree seeds, `/dungeon` | `on_item_use` with `aim()`, `on_command`, `set_block` |
| orespawn: structures placed as chunks are generated (villages, dungeons, ruins; ore and plant clusters near them) | `on_chunk_generated` (*new*: a couple of chunks a frame, 4096 writes a call), `rand_chunk` and `world_seed` (*new*: the same value for the same chunk every time), `surface_y`, `biome_at`, `set_block` |
| orespawn: reacting to every block placed, to a creature's death and removal, to a right click on a creature, keeping night creatures alive by day | `on_block_placed` (*new*: causes `player`, `script`, `remote`, `fall`, `fluid`, `other`), `on_creature_death` and `on_creature_removed` (*new*), `on_creature_interact` (*new*, can cancel), `on_creature_sunburn` (*new*, can veto) |
| orespawn: hostile creatures that bite from their own reach (big ones) | the creature step closes to a reach derived from the creature's width (`reach` in the pack to set it): no host call needed |
| orespawn, witherstorm: opening the crafting grid or the pack from a key or an item | `open_crafting(3, 5 or 7)` and `open_inventory()` (*new*) |
| scripted runs (`--screenshot`/`--bench` with `--run`) moving mod timers and creatures | `--step` (seconds a frame, alone 1/60): the same `dt` for the clock, `on_tick` and the creatures; without it a scripted frame passes no time, as before |
| orespawn: bespoke targeting and pets | **missing**: no way to give a creature a goal or a target (only to move it by hand each frame) |
| witherstorm: the storm's body with one proxy creature to hit | `spawn_mob`, `set_creature_size` (up to 32 x 64 blocks, drawn and colliding at that size; `mob_set_size` only changes the box), `set_creature_health` (up to 1,000,000), `mob_set_pos`, `mob_hurt`, `mob_in_view`, `on_entity_hit` (new) |
| orespawn, witherstorm: giants (a creature taller than 6 or wider than 3 blocks, more than 1000 health) in a shared game | `height` up to 64, `width` up to 32 and `health` up to 1,000,000 in the data; `set_creature_size`, `set_creature_health`; the host's snapshot carries size, health limit, tilt, bones and state to the other players (**closed**) |
| witherstorm: flight and aiming, tractor beams, skulls, bite, wither sickness | maths in the script; `shoot`, `push_player`, `hurt`, `effect`, `clear_effect` (new) |
| witherstorm: eating terrain in clusters, queued explosions | `scan_blocks` (40 000 cells a call, 2 000 results), `set_block`, `explode` |
| witherstorm: tainting blocks and creatures | `scan_blocks`, `set_block`, `mobs_near`, `mob_remove`, `spawn_mob`; the creature's look variant is not reachable |
| witherstorm: the Formidibomb, Super TNT, blade, Eye, Phasometer, amulet, cure | `on_item_use`, `aim()`, `explode`, `hud_frac_rect` and `hud_frac_text`, `clear_effect`; lit TNT with a fuse is **missing** |
| witherstorm: the summoning ritual, the Super Beacon, `/witherstorm` | `on_block_place`, `scan_blocks`, `on_command` |
| witherstorm: drawing the storm in pieces, heads, tentacles | `draw_model_posed` (new): one pose for a whole model; the parts of a creature's own model are moved apart with `set_bone` (rotation, offset and scale of a named bone, children follow; `model_bones` lists the names) and `set_creature_orientation` (pitch and roll of the whole model) (**closed**; a model drawn by `draw_model` still has only one pose) |
| orespawn, witherstorm: effects of host-only logic seen by the other players (a boss's phase, a blast, a state) | `set_creature_state` / `creature_state` (64 bytes on a creature, sent with the snapshot), `send_message` / `send_message_to` and `on_message(channel, from, text)` (512 bytes, 8 a second, checked by the server) (**closed**) |
| witherstorm: beams (`draw_quads`) | `quad_create`, `quad_move`, `quad_tint`, `quad_remove` (new): up to 256 translucent, textured, alpha or additive quads a mod, kept until removed |
| witherstorm: sparks, sky, shake, sounds | `spawn_particles` (new: spark, smoke, flame, bubble, dust, magic, heart, cloud), `draw_speck`, `force_sky` (rain), `set_dimension_sky` (new: sky and fog colours, stars, clouds, sun, moon, least light, fixed time of day, per dimension), `shake`, `play_sound`, `play_sound_at` and `loop_sound_at` (new: placed, panned, pitched, looping, with handles to stop, fade and move them), `play_music` (new) |
| witherstorm and orespawn: flying debris, drops, blasts that burn or only throw | `spawn_falling_block`, `drop_item`, `on_item_pickup` (new), `explode(x, y, z, power, flags)` (new: `fire`, `no_blocks`, `no_damage`, `silent`; answers the blocks destroyed) |
| orespawn: the sky of each of its five dimensions | a `dimension` line's sky properties (new, DATA.md) or `set_dimension_sky`; its terrain stays **missing** |
| witherstorm: saved state (`witherstorm.txt`) | `state_*` (512 keys of 4 096 bytes); a storm's whole record fits |

### The man from the fog: darkness, a head that turns, pictures (new)

What the port of `manfromthefog` needs of the engine that a mod's own drawing could not do
(all in `docs/SCRIPTING.md`; nothing in either protected mod was touched):

| Gap | Engine |
| --- | --- |
| translucent darkness: the engine sets the fog before the world and a mod draws after it, so a mod's fog could not blend with the sky's | `set_fog(start, end, r, g, b, density)`: a fog override mixed with the dimension's fog colour and range at the next world pass, gone half a second after the last call (**supported**) |
| translucent specks of darkness and mist (the particle kinds are flat, opaque-edged pictures with fixed looks) | `spawn_soft`: a soft round sprite with its own opacity (start and end), size growth, velocity and life, `alpha` or `additive`; 512 alive, 256 a mod, the frame rule of `spawn_particles` (**supported**) |
| a tint over the screen after the world | `hud_tint(r, g, b, a)` (**supported**) |
| a stalker that watches the player: every creature was drawn with its head straight | creatures with a `head` bone look at the nearest player within 12 blocks (70 degrees to the sides, 40 up and down), asleep and fleeing ones do not; trait `watches` for up to 48 blocks; `set_creature_look(id, yaw, pitch)` and `clear_creature_look(id)` force it, and a forced look is sent with the creature snapshot in a shared game (**supported**) |
| a jumpscare picture and a counter strip on the HUD (it could only fill rectangles and write text) | `hud_image`, `hud_image_at`, `hud_image_uv`, `hud_image_uv_at`, `hud_image_fit`: a png or tga from the mod's folder, 16 a mod, 1024 x 1024 and 4 MB decoded each, alpha, tint, sprite-sheet parts, `cover` and `contain` for whole-screen pictures (**supported**) |

### Armour, wear, item data, attributes (new)

Gaps the ports found in the engine, now closed (all in `docs/SCRIPTING.md`):

| Gap | Engine |
| --- | --- |
| armour slots: carried pieces stood in for worn ones | four worn slots, `armor` item data, `get_armor`, `set_armor`, `armor_defense`, `armor_toughness`, `on_armor_change` (**supported**) |
| durability: tools never wore out; a mod's wear counters | `durability` item data, wear per block, hit, shot and blow, `item_damage`, `item_wear`, `item_durability`, `repair_item`, repair by crafting two worn ones (**supported**) |
| spawn eggs named by recipes, trades and loot of any pack | `<id>_egg` items exist before references are resolved, order-independent, with a warning for a creature that does not exist (**supported**) |
| per-stack state and item tags | `item_get`, `item_set`, `item_clear`, `item_keys`, `has_tag`, `item_tag_value`, item data kept with stacks in the pack, chests, drops and saves (**supported**) |
| a mod's own timers for status effects; a fixed 20 health | `effect_secs`, `list_effects`, `effect_defined` with `effect_level`; attributes `set_attribute`, `get_attribute`, `add_attribute_modifier`, `remove_attribute_modifier`, `max_health` up to 100 with rows of hearts, `walk_speed`, `jump_height`, `armor`, `damage` (**supported**) |

Not done in this round: other players do not see a worn piece (the position
message carries only the held item; armour is drawn on your own body, in third
person and in the inventory), and a creature's own most health
(`set_creature_health`) belongs to the creatures work.

Missing host calls, in the order they would unblock the most:

1. **A world kind and per-dimension terrain** (the land itself, not what stands on it): needed by
   orespawn and by Earth. *Structures* on generated terrain are now covered (`on_chunk_generated`,
   `rand_chunk`). The terrain is not a host call but a different design: a data-driven land (noise
   parameters in the pack) would be the safe way, since a script cannot be asked for every column of a chunk.
2. **A goal for a creature** (`mob_goto(id, x, y, z)` / `mob_target(id, ...)`), so a script steers
   creatures with the game's own path finding and AI instead of moving them by hand: orespawn's targeting,
   pets and the bosses' phases.
3. **Lit TNT** (`prime_tnt(x, y, z, fuse)`): the Formidibomb and Super TNT (`explode` gives the blast but
   not the burning fuse). (Parts of a model animated apart are done: `set_bone`.)
4. **Bigger or streamed world scans**: the storm eating a whole hillside (its dark sky is now `set_dimension_sky`, and its beams, sparks and debris are the effects above).
5. ~~**A positioned sound**~~ closed: `play_sound_at`, `loop_sound_at`, handles (`stop_sound`, `fade_sound`, `set_sound`, `is_playing`) and one streamed music channel (`play_music`, `stop_music`, `set_music_volume`, `music_playing`), with a MUSIC VOLUME setting (see SCRIPTING.md, "Sound and music").

The first and second cannot be done by a script on its own at all; the rest only make
witherstorm's behaviour exact. Nothing in either mod was edited, and the engine's host calls are written
so that neither mod needs to change to use them.
