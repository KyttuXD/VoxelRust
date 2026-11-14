# Content files

Textures, blocks, items, recipes, models, creatures, vehicles, sounds,
dimensions, chat commands and scripts are declared in plain text. The game's own content is a small **core**,
[data/base.txt](../data/base.txt), which includes [data/creatures.txt](../data/creatures.txt) (the hand models) and
[data/effects.txt](../data/effects.txt) (status effects); all of them are built into the program and written by
`tools/gen_base.py`. The core is what the game needs to start and be played with no mod installed:
terrain and natural blocks, the oak and pine, ores, glass, a lamp, the crafting table, the furnace, a chest, a door,
stairs and slabs of stone, cobble and planks, wooden, stone and iron tools and swords, a bow and arrows, basic food,
the player model and the sounds. **Everything else is a mod**, in the `VoxelRust-Mods` folder:

| Mod | What it holds |
| --- | --- |
| `colours` | sixteen colours of concrete, wool, terracotta, glass, boards, chests, beds, couches |
| `woods` | birch, maple, palm, blossom, acacia, dark oak, and the oak's and pine's extras: doors, gates, trapdoors, stairs, fences... |
| `stonework` | marble, slate, granite, basalt, tiles, bricks; stairs, slabs and walls of them |
| `furniture` | chairs, shelves, kitchen and bathroom, tables, lanterns, glass panes |
| `farming` | farmland, crops, seeds, hoes |
| `cooking` | ovens, kilns, smokers, campfires, flour to cake, dishes |
| `caves` | moss, glowcaps, crystal clusters, icicles, magma, vines, cobwebs |
| `explosives` | TNT, gunpowder, flint and steel |
| `tools-extra` | copper, crystal and ruby tools and swords, longbow, crossbow |
| `wildlife`, `monsters`, `villagers`, `wanderer` | every creature (the game has none of its own) |

With none of them the game still runs: a bare world of core blocks and no creatures. The world generator asks for
what it needs by name and falls back to a core block for what is missing (the extra woods grow oak, caves are
undressed, there are no fields). `tools/export_mods.py` writes the mods from `tools/gen_base.py` and
`tools/gen_creatures.py` (see [BUILDING.md](BUILDING.md#regenerating-data)). Packs are read after the core:

1. `pack.txt` in the game's data folder, if it exists (the folder is printed
   at start-up; `--data DIR` chooses another).
2. Every mod, unless switched off, in the data folder's `mods` folder (or the
   one named by `--mods-dir`) and, without `--mods-dir`, in a `mods` folder
   where the game was started. No mod comes with the game or is part of the
   program (the author's are in the separate `VoxelRust-Mods` folder: see
   [BUILDING.md](BUILDING.md#mod-code)). A mod found twice is loaded once.
   A pack may also say `version 1.2` and `client no` at the top level; the
   mods list and the server's mod download read them (see
   [MODDING.md](MODDING.md#switching-mods-on-and-off)).
3. Every file given with `--pack FILE`.

Mods are switched on and off in SETTINGS > MODS (see
[MODDING.md](MODDING.md#switching-mods-on-and-off)).

A pack can add new things, or redefine anything by reusing its id. There is a
worked example in [examples/pack.txt](../examples/pack.txt). How packs are
loaded, reloaded and shared by servers is in [MODDING.md](MODDING.md).

Problems in a pack never stop the game: the line is skipped and a message
such as ``pack.txt:19: unknown block property (`bogus`)`` is printed.

## Syntax

One declaration per line; `#` starts a comment (but not a `#rrggbb` colour in a `texture` or `palette` line). Words are separated by spaces
or tabs; indentation is optional.

These keywords start something new: `texture`, `block`, `item`, `recipe`,
`craft`, `shapeless`, `shaped`, `group`, `bake`, `smelt`, `ore`, `plant`, `model`, `picture`, `entity`, `sound`,
`command`, `when`, `dimension`, `feature`, `episode`, `setting`, `include`.
Any other line sets a property of the block, item or entity most recently
started, or of the mod section described below. (`sound` with a single word,
inside a block or an entity, is a property; see [Sounds](#sounds).)

Mod code can claim words of its own. A mod's code (see [Features](#features))
can claim a top-level line, such as `map` for the Earth mod, or an indented
section starting at the left margin, such as `vehicle` for the Vehicles mod.
Claimed lines and sections are kept as written for that code to read; the
indented lines under a section are its properties. Every section also becomes
an item, with the section's `name`, `icon` and `category` (`Things` if none),
and a stack size of 1.

`picture <name> file <picture.png>` names a creature's picture (see [Creatures](#creatures));
the path is relative to the file the line is in. `group <name> <items...>` adds to a group
that an earlier file started, so a mod can put its own planks among `@planks`.

Ids are single words. Declaring an id a second time replaces the first
declaration completely (it keeps its place in the order).

Properties that are switches (`rotate`, `flip`, `hidden`, `unbreakable`,
`night`, `storage`, `powered` and `solid`) are on when written alone or with
`yes`, and off when followed by `no`, `false` or `0`. `falls`, `tall`,
`tamable` and `rideable` are on when written alone and off only when
followed by `no`.

```
include other-file.txt        # path relative to this file
```

Includes may nest eight deep.

## Textures

Textures are 16 x 16 pixels. Up to 1024 can be defined (they share one 512 x
512 picture).

```
texture <id> builtin <painter>            one of the built-in painted textures
texture <id> paint <style> <r> <g> <b>    a built-in pattern in any colour
texture <id> paint <style> palette <name>[+<name>...] [gradient <dir>] [host <stone>]
                                          the same with a palette: several colours, roles, a gradient
texture <id> paint <style> colors <#rrggbb|r g b|role=#rrggbb>... [gradient <dir>]
                                          a palette written in place
palette <name> <colours...> [gradient vertical|horizontal|diagonal|radial]
                                          declare a palette (before the textures that use it)
texture <id> flat <r> <g> <b> [noise <n>] a flat colour; with `noise` (any number) a block face with a darker frame
texture <id> file <image>                 a PNG, JPEG, GIF, BMP or TGA file (any size, fitted to 16 x 16), path relative to this file
```

Colours are three numbers 0..255, or (in `texture` and `palette` lines) `#rrggbb`.

**Palettes.** A palette is up to six colours; in order they are the *base*,
*dark*, *light*, *accent*, *highlight* and *outline* colours (or written
`accent=#ffffff` by name). With `gradient` the colours are instead spread over
the picture in flat bands (first colour at the top, left, top left or middle).
`palette fire+oak` uses the second palette as the tool's handle, or as the next
gem of an ore. `host` is the stone of an ore (`stone deepstone sandstone dirt
gravel netherrack`, a palette, or `#rrggbb`). A `paint ... r g b` line is a
palette of one colour and draws exactly what it always did. Every role,
style and example is in [TEXTURING.md](TEXTURING.md#palettes-and-gradients).
```
palette ruby #e2406e #a82454 #ff9cb8
texture ruby_ore paint ore palette ruby
texture fire_axe paint axe palette fire+oak
```

**`builtin` painters:** `stone dirt grass_top grass_side sand gravel water
log_side log_top pine_side leaves pine_leaves planks cobble brick
stone_bricks glass coal_ore iron_ore copper_ore crystal_ore ruby_ore snow
snow_side ice sandstone cactus_side cactus_top lamp flower_red flower_yellow
tall_grass deepstone clay sun moon wand`, all drawn flat in the OCD style
(see [TEXTURING.md](TEXTURING.md#the-painters)). An unknown painter name gives a
pink and black checkerboard.

**`paint` styles for blocks:** `concrete wool terracotta glass rock polished
bricks tiles planks bark rings leaves blossom farmland magma door glassdoor`.
(`rings` is the cut end of a log; `glass`, `leaves`, `blossom` and
`glassdoor` have holes.) One line per colour is how
the base data makes sixteen colours of several materials and six kinds of
wood.

**`paint` styles for plants and other see-through blocks** (use with `shape
cross`): `mushroom cluster icicle vine web crop1 crop2 crop3 crop4` (the four
stages of a crop).

**`paint` styles for item pictures** (a shape in the given colour on a clear
background): `stick pick axe shovel hoe sword gem meat fruit fishfood rail
machine hull wings chest workbench seeds sheaf loaf pizza wedge root tuber jar
egg sack cookie pie bowl bow crossbow arrow ingot`.
The crafting blocks' tops and sides are `bench5`, `bench7` and `bench_side`.

**`paint` styles for ores** (stone with the ore's colour; they take palettes):
`ore` (four blobs; several palettes make a multi-gem ore) and `ore_vein` (two
diagonal bands).

**`file`:** TGA files must be 16 x 16, uncompressed true colour, 24 or 32
bits per pixel. An alpha of zero is a hole (use with `shape cutout` or
`shape cross`).

## Blocks

```
block <id>
  name <display name>
  shape cube | cutout | translucent | cross | liquid | slab | stairs | fence | wall
  tex <texture>                  the same on every side
  tex <top> <side> <bottom>
  light <0..15>                  light given off
  skyloss <0..15>                sunlight lost passing down through it
  solid yes|no                   whether the player collides with it
  box x0 y0 z0 x1 y1 z1          a custom shape (see below)
  prop <model> [<height>] [<lift>]        draw it as this model instead of its boxes (see Props)
  rotate                         four facings
  flip                           also upside-down
  hardness <seconds>             time to break by hand in survival, 0..600
  tool pick | axe | shovel | hoe | none   the kind of tool that digs it faster
  drops <item> [<chance>]        what breaking it gives in survival
  drops nothing
  also <item> [<chance>] [<count>]   a second thing given when broken
  use craft [3|5|7] | chest | oven | furnace | bed   what right-clicking it does
  toggle <block>                 right click swaps it for that block
  seat <sixteenths>              right click sits on it, that high
  eat <points>                   right click eats it where it stands
  tag <name> [number]            a mark that mod code reads (see below)
  fills <wide> <tall>            a big prop: the room it takes (see Big props)
  part                           the hidden filler of a big prop
  long ahead|behind <block>      two blocks long: the other half, one block ahead of this one or behind it
  front <texture>                a `rotate` block's picture for the face that looks the way it was placed (below)
  rest <block>                   read as this block (same facing) when a saved world is loaded
  connect <group> [cubes]        joins neighbouring blocks of its group (and full cubes with `cubes`)
  arm x0 y0 z0 x1 y1 z1          drawn toward a joined neighbour, written for the north (-z) side (up to 4)
  react <next to> <becomes>      turns into the second block when the first is beside it
  sound <sound id>               heard when it is broken, placed or toggled
  tall                           two blocks high
  falls                          drops when nothing is under it
  on <blocks...>                 may only stand on these blocks
  grows <block> <seconds>        turns into that block after about that long
  hurts <n>                      health lost per second by a player in or on it
  slow <percent>                 a player inside it moves at this percent of normal speed
  category <tab name>            its tab in the creative item list
  unbreakable
  hidden                         no item is made for it
```

Defaults: the name is the id; `shape cube`; `hardness 1` (0 for `cross`, 0.3
for `cutout` and `translucent`); `tool none`; it drops itself, always; the
category is `Blocks`; `solid` is yes for everything except `cross` and
`liquid`.

- `drops <item> <chance>`: the chance is a percentage, 1..100 (default 100).
  `drops apple 7` gives an apple about one time in fourteen and nothing
  otherwise.
- `also <item> [chance] [count]`: a second, separate roll when the block is
  broken in survival. The chance is a percentage (default 100) and the count
  is how many are given (default 1). `also wheat_seeds 25` on tall grass
  gives seeds one time in four; `also carrot_seeds 100 2` on a ripe crop
  always gives two.
- `tool`: only matters together with items that have a `tool` property (see
  Items). Without the right tool the block still breaks, at hand speed.
- `use craft [3|5|7]`: right click opens a crafting grid of that size: 3 x 3
  (the crafting table; plain `use craft` means this), 5 x 5 (`use craft 5`,
  the crafting bench) or 7 x 7 (`use craft 7`, the workshop). Any block may
  have any of the three, so a pack can add its own benches. See Recipes.
- `use chest`: right click opens 27 slots of storage belonging to that block.
- `use oven`: right click opens the `bake` recipes.
- `use furnace`: right click opens the `smelt` recipes. In the base data the
  furnace, blast furnace and kiln have it; the oven, stove, smoker and
  campfire have `use oven`.
- `use bed`: right click makes this the place the player comes back to after
  dying, and at night (in singleplayer) skips to morning.
- `seat <sixteenths>`: 1..16 (8 if written alone): the height of the seat
  above the bottom of the block. Right click sits the player there; jump or
  the get-out key stands them up. The base chair has `seat 8`, a couch
  `seat 7`.
- `eat <points>`: 1..40 (4 if written alone). Right click restores that much
  hunger and half as much health, and removes the block.
- `tag <name> [number]`: a mark on the block, the number being 1 if left out.
  A block can carry up to 16 tags, and saying a name again replaces its
  number. The game itself reads only `tnt`, `fire` and `blastproof` (below);
  what any other mark means is up to the mod code that looks for it.
- `tag tnt [power]`: the block is TNT (default power 2.5, at most 8). It is
  lit by lava or a `tag fire` block beside it, by an explosion, or by the
  `flint_and_steel` item, burns four seconds and explodes with that power
  (radius twice the power). The base game's `tnt` has `tag tnt 2.5`. See
  [Explosions](MODDING.md#explosions).
- `tag fire`: the block sets TNT beside it alight (the base campfire has it;
  lava does without, it is known by being lava).
- `tag blastproof`: explosions leave the block standing. (Water, lava and
  `unbreakable` blocks already do.)
- `tag track <percent>`: the block is track. Rail vehicles on it move at
  that percentage of their speed (10..400; 100 if no number is given). The
  Trains mod's `speed_rail` has 220 and `slow_rail` 40.
- `tag slope <1..4>`: the block is a ramp for rail vehicles; the number says
  which side is the high one (1 north, 2 west, 3 south, 4 east), and the
  vehicle rides up it a block across the cell. The Trains mod's slopes have it.
- `tag chest_lid <1..3>`: a stage of a chest lid being opened (see
  [Chests that open](#chests-that-open)). Written on hidden blocks that
  `rest` as the shut chest.
- `tag station <seconds>`: 1..120 (1 if no number is given). A rail vehicle
  that nobody is driving and that is not chained behind another stops on the
  block for that long, then goes on. The Trains mod's `station_rail` has 6
  and `depot_rail` 30.
- `sound <sound id>`: the sound played when the block is broken, placed, or
  toggled. Without it a block uses `break`, `place` and (for `toggle`)
  `door`.
- `toggle <block>`: for doors, hatches and gates. Declare two blocks that
  name each other; the open one is usually `hidden` and `drops` the shut
  one. If both have the same `rotate` and `flip`, the facing is kept.
- `tall`: the block is placed, toggled and broken as a pair, one above the
  other, and needs two free cells to place.
- `falls`: like sand, it drops one cell at a time until something holds it.
- `on <blocks>`: it can only be placed on one of the listed blocks, and
  comes loose (giving its drop in survival) when that block goes. Every
  name must be a known block.
- `grows <block> <seconds>`: for crop stages. The time is an average; it
  only passes while the plant is under open sky (plants, leaves, glass and
  other see-through full blocks above it do not count as cover) and within
  about 24 blocks of the player.
- `hurts <n>`: a survival player standing in or on the block loses that much
  health every second (magma has `hurts 2`). Written alone it means 1.
- `slow <percent>`: 5..100; written alone it means 50. Meant for blocks one
  can walk into (`shape cross`, like the cobweb, which has `slow 25`).

A doorway that opens, from the base data:

```
block oak_trapdoor
  name Oak Trapdoor
  tex oak_door
  category Doors
  hardness 1.2
  tool axe
  rotate
  box 0 0 0 16 3 16
  toggle oak_trapdoor_open
block oak_trapdoor_open
  name Oak Trapdoor
  tex oak_door
  hardness 1.2
  tool axe
  rotate
  hidden
  box 0 0 13 16 16 16
  toggle oak_trapdoor
  drops oak_trapdoor
```

A crop is a chain of `hidden` blocks, each with `on farmland` and `grows`
pointing at the next, plus a seed item that places the first (see Items).
[examples/pack.txt](../examples/pack.txt) has a complete one.

Shapes:

- `cube`: a full opaque block. The default. `rotate` and `flip` are ignored,
  since a cube looks the same every way round.
- `cutout`: a full block whose texture has holes (leaves, glass).
- `cross`: two crossed quads (flowers). Not solid.
- `liquid`: translucent, not solid, can be swum in.
- `translucent`: a solid block you can see through (stained glass).
- `fence`, `wall`: a post that grows arms toward neighbouring full cubes and
  toward other fences (for a fence) or walls (for a wall).
- `slab`: the lower half of a block; always has `flip`.
- `stairs`: two steps; always has `rotate` and `flip`.

`rotate` and `flip` only have an effect on custom shapes; the other shapes
set them as described.

### Custom shapes

One or more `box` lines (up to eight) give a block any shape made of boxes.
Coordinates are whole sixteenths of a block, from `0 0 0` to `16 16 16`; y is
up; each box needs x0 < x1, y0 < y1 and z0 < z1. The boxes are used for
drawing, for collision and for aiming.

```
block planks_table
  name Table
  tex planks
  box 0 13 0 16 16 16
  box 1 0 1 3 13 3
  box 13 0 1 15 13 3
  box 1 0 13 3 13 15
  box 13 0 13 15 13 15
```

With `rotate`, describe the block as it should look when the player faces
north (toward -z) while placing it; the game makes the other three facings.
With `flip`, the block is placed upside-down when you aim at the underside of
a block or at the upper half of a wall.

Each facing is stored as its own block state, so rotated blocks cost nothing
extra in memory or in saves. There can be 4096 states in total (air is one;
a plain block uses one, a `slab` two, a `rotate` block four, `stairs` eight).

### Faces of a rotating block

`tex <top> <side> <bottom>` gives the four sides of a block one picture. A
`rotate` block made of boxes can also say `front <texture>`: the face that
looks the way the player was facing when placing it (toward the player) uses
that picture instead of the side one, on every box it has. The front goes with
the block's turns, so a chest's latch is on one face only. The block's item
shows the front picture. `front` needs `rotate` and `box` lines and is
ignored otherwise.

`rest <block>` names the block this one is read as when a saved world is
loaded, facing the same way (both blocks need the same number of turns). It is
for states that only exist while something is going on: an open chest.

### Chests that open

The base data's chests (`tools/gen_base.py`, `oak_chest` and the other
woods and colours) are four blocks each: the shut chest, which has the item, and
hidden `<id>_lid1`, `_lid2`, `_lid3`, whose boxes lift the lid a little more
each (the last at 36 degrees; boxes cannot tilt, so the lid is three steps).
The lid stages carry `tag chest_lid <n>` and `rest <the shut chest>`, and
drop the shut chest. The game steps through them while the chest screen is open
([`src/game/chest_anim.rs`](../src/game/chest_anim.rs)), plays the sounds
`chest_open` and `chest_close`, and writes each stage as an ordinary block edit
(so other players see it). A pack can give its own `use chest` blocks the same
by naming blocks `<id>_lid1..3` in the same way; a chest block without them just
does not animate.

### Props

```
  prop <model> [<height>] [<lift>]
```

A block with `prop` is drawn as a model (see [Models](#models)) instead of
as its boxes: furniture, statues, plants. `<model>` is the id of a model
declared before the block; with an unknown id the line is reported and
skipped. `<height>` is how tall the model is drawn, in blocks: 0.1 to 24, and
1 if left out. `<lift>` raises the model off the floor by that many blocks
(-1 to 4, 0 if left out). The model stands on the bottom of the block, in the
middle.

```
model hg_garden_bench file models/garden_bench.bbmodel

block garden_bench
  name Garden Bench
  tex planks
  prop hg_garden_bench 1.0
  rotate
  box 0 0 4 16 8 12
  seat 7
```

- `prop` applies to a block with a custom shape, so give it at least one
  `box` line. The boxes are not drawn; they still give the solid shape, for
  collision and aiming. Add `solid no` for a prop one can walk through.
- `rotate` turns the model a quarter turn for each facing, so it faces the
  way it was placed.
- Its item is shown as the model in the inventory. `tex` is still needed as
  for any block.
- The model is drawn in its rest pose, lit by the light where it stands.
- Props are drawn out to about 44 blocks from the player, in the chunks
  within three of the one the player is in; a chunk draws 4096 of them at
  most.

### Long blocks, connections and reactions

`long ahead <block>` on the block you place, and `long behind <block>` on
the block it adds, make a thing two blocks long (the base data's beds): the
second half goes one block the way the player faces, needs its room free,
and breaking either half takes both. Give the second half `hidden` and
`drops <the first>`.

`connect <group>` with `arm` boxes makes a custom block join its neighbours
the way fences do: an arm is drawn toward each side (north, west, south, east)
that holds a block of the same group, or with `cubes` a full cube. The groups
`fence` and `wall` are the built-in ones, so a custom block can join them.

`react <block> <becomes>` (up to 8 lines) turns the block into `<becomes>`
whenever `<block>` is beside it (any of the six sides), checked whenever a
block nearby changes, including changes made by other reactions. Make
reactions one-way: two blocks that turn into each other would flip for ever.

### Big props

`fills <wide> <tall>` gives a prop more room than one block. The room is a
square `<wide>` blocks across (1..7) and `<tall>` blocks high (1..24),
starting at the block it is placed in. Placing it needs all of that room
free. The other cells are filled with hidden solid blocks that have the
`part` property. They are never drawn. Breaking or aiming at any part acts
on the whole prop. The base data's `golem_statue` is `fills 1 3`.

## Items

Every block that is not `hidden` gets an item that places it. Declare an item
for anything else: materials, tools, food, or an item that places a block in
a special way.

Two more kinds of item are made for you: one for each section that mod code
has claimed, such as each vehicle (see [Vehicles](#vehicles)), and a spawn
egg for each creature (see [Spawn eggs](#spawn-eggs)).

```
item <id>
  name <display name>
  icon <texture>
  place <block id>      what right click places
  brush <0..6>          act on a ball of this radius instead of one block
  reach <1..64>         how far it reaches, in blocks
  stack <1..999>        how many fit in one inventory slot (default 64)
  tool pick | axe | shovel | hoe <speed>   digs matching blocks <speed> times faster (1..100)
  food <1..20>          eating it restores this much hunger and half as much health
  damage <n>            what it does to a creature per hit (a bare hand does 3)
  turns <from block> <to block>   right click turns one block into the other
  shoots <damage> [speed] [seconds between shots] [ammunition item]
  category <tab name>   default: Tools
```

Left click breaks, right click places.

- Without `reach`, the reach is 4.5 blocks in survival and 6 in creative.
- Without `icon`, an item with `place` uses that block's side texture.
- A block keeps its own automatic item unless a plain item (one with no
  `brush`) names it with `place`; then that item is the only one that places
  it. This is how seeds stand in for the first stage of a crop. A brush that
  paints with a block does not take the block's item away.
- `brush` items work only in creative mode. One with a `brush` but no `place`
  copies the block it is pointed at. Brush items cannot be used in recipes or
  as drops.
- A `tool` speeds up only blocks whose own `tool` property names the same
  kind. Tools usually also want `stack 1`.
- `damage`: 0..1000. Anything below 3 hits like a bare hand.
- `turns`: may be given several times. The base hoes have
  `turns dirt farmland` and `turns grass farmland`.
- `shoots`: holding right click draws the item back and letting go looses a
  shot the way the player looks, with speed and damage that grow with the
  draw (a full draw gives the numbers below; the weakest shot a fifth of the
  power). Damage is
  0..500; speed is in blocks per second, 4..80 (32 if left out); the wait
  between shots is 0.1..10 seconds (0.6 if left out), and sets how long a
  full draw takes (the wait x 1.4, between 0.5 and 2 seconds; a crossbow, whose
  id contains `crossbow`, loads in the wait x 0.6 and fires on the next click).
  The three pictures of it drawn back are the textures `<icon>_pulling_0`,
  `_1` and `_2` (the item keeps its own picture if it has none; a texture pack's
  `bow_pulling_0..2` and `crossbow_pulling_0..2` are used). Sounds `bow_draw`,
  `bow_load` and `bow` are played as it is drawn, loaded and shot. With an ammunition
  item, each shot uses one in survival, when the string is let go and the item says `OUT OF AMMUNITION`
  when there are none; without one it never runs out. Shots fall as they
  fly, last five seconds at most, stop at solid blocks and hurt the first
  creature they meet. The base data has:

  ```
  item bow
    shoots 6 34 0.7 arrow
  item longbow
    shoots 9 46 1.1 arrow
  item crossbow
    shoots 13 54 1.8 arrow
  ```

- **Explosives.** The base data has the block `tnt` (`tag tnt 2.5`, in the
  Tools tab; made at a crafting table from `gunpowder 5 sand 4`), the
  item `gunpowder` (`gunpowder 2` from `coal 2 sand 1`) and the item
  `flint_and_steel` (from `iron_ingot 1 gravel 1`). The flint and steel is
  recognised by its id: right click a TNT block with it to light it. Their
  textures are `tnt_top`, `tnt_side`, `tnt_bottom`, `gunpowder` and
  `flint_and_steel` (a texture pack's pictures of those names are used). The
  explosion plays the sound `explosion` and the fuse `hiss`. See
  [Explosions](MODDING.md#explosions).

```
item copper_pick
  name Copper Pickaxe
  icon copper_pick
  category Tools
  tool pick 4
  stack 1

item cooked_fish
  name Cooked Fish
  icon cooked_fish
  category Food
  food 6
```

The creative inventory shows one tab per category, in the order categories
first appear (block items, then declared items, then the sections of mods,
then spawn eggs). A tab is
drawn as the first item of its category; its name appears when pointed at.

## Recipes

Things are made in a **crafting grid**: 2 x 2 in the inventory screen, 3 x 3
at a crafting table, 5 x 5 at a crafting bench and 7 x 7 at a workshop (see
`use craft`). A recipe says what items must be in the grid, and what comes
out.

```
craft     <item> <count> <ingredient> <count> [<ingredient> <count> ...]
shaped    <item> <count> <row> [<row> ...] with <letter> <ingredient> [<letter> <ingredient> ...]
group     <name> <item> [<item> ...]
bake      <item> <count> <ingredient> <count> [<ingredient> <count> ...]
smelt     <item> <count> <ingredient> <count> [<ingredient> <count> ...]
```

- `craft` (also spelt `shapeless`, or `recipe`, which older packs use): a
  **shapeless** recipe. The ingredients can be put in the grid anywhere, one
  to a cell. `craft stick 4 planks 2` is two planks anywhere in the grid.
- `shaped`: a **shaped** recipe. The rows are the pattern, top to bottom; the
  letters after `with` say what each letter is. See below.
- `group`: a name for a set of items, so that a recipe can take any of them
  (see Groups).
- `bake`: needs an **oven** (a block with `use oven`) and is offered only
  there, as a list to click. The oven and `smelt` have no grid.
- `smelt`: needs a **furnace** (a block with `use furnace`), offered only
  there as a list. Fuel is not special: list coal as an ingredient if the
  recipe should need it.

Which grid a recipe needs is not written down: it follows from its size. What
fits the 2 x 2 grid (a shaped pattern no bigger than 2 x 2, or a shapeless
list of at most 4 items) can be made by hand. A shaped pattern up to 3 x 3 (or
a shapeless list of up to 9 cells) needs a crafting table; one wider or taller
than 3 (up to 5 x 5) needs a **crafting bench**; one wider or taller than 5
(up to 7 x 7) needs a **workshop**. Each tier makes everything the smaller
ones do, and a pattern bigger than 7 in either direction is refused with a
warning. A shapeless `craft` line never needs more than a table.

### Shaped recipes

```
shaped iron_pick 1 "III" ".S." ".S." with I iron_ingot S stick
shaped stone_stairs 4 "M.." "MM." "MMM" with M stone
shaped bookshelf 1 "PPP" "..." "PPP" with P @planks
shaped glass_lamp 2 III .L. with I glass L lamp
```

- After the item and the count made come one to three **rows**, each one to
  three cells wide, all the same width. A row is written in double quotes (it
  may then hold spaces) or, when it has no spaces, bare. A cell is a letter
  for an ingredient, or `.` or a space for **nothing**.
- After the word `with` come pairs: a single letter and the ingredient it
  stands for. Every letter used in the rows must be given.
- Letters are any character except `.`, quotes, `#` and blanks. The word
  `with` cannot be used as a row.
- Empty rows and columns around the pattern do not matter: `"I.."` and `"I"`
  are the same recipe.
- The pattern may be put **anywhere** in the grid, and **mirrored** left to
  right (so an axe `II`/`IS`/`.S` also works the other way round). It may not
  be turned upside down, and no other items may be in the grid.
- The recipe is as big as its pattern: a pattern of 3 x 3 or 3 x 2 needs a
  table, 5 x 3 or 4 x 4 a bench, 7 x 1 a workshop (the longer side counts).
  Only the trimmed pattern counts: `"I.."` is 1 x 1 whatever its rows say.

### Ingredients, counts and groups

An ingredient is an item id (blocks count, through their automatic items),
or `@name`, any item of the group `name`, and may end in `*N` to ask for N
of it in its one cell: `shaped heavy_door 1 DD DD with D iron_ingot*5`.
Counts are 1..999, but nothing can ask for more than one stack holds.

`craft` lines count units: `craft rail 16 iron_ore 3 stick 2` is five
cells, three with ore and two with sticks.

**Too big for the grid.** When the units add up to more than nine, there is
no way to give each its own cell. The rule is then: **each kind of
ingredient takes one cell and that cell must hold the whole amount.**
`craft plane 1 iron_ore 10 planks 8 white_wool 4` is three cells, holding at
least 10 ore, 8 planks and 4 wool; taking the result uses exactly those
amounts. (Stack sizes limit this: a non-stacking item cannot be asked for
more than one.) The recipe still needs the table, being three cells wide
in no particular pattern, and it costs what it always cost. If the units
come to nine or fewer, each unit gets its own cell as usual.

**Groups.** `group planks planks pine_planks birch_planks` names a set;
`@planks` in a recipe then means any of them. Naming a group again adds to
it, so a pack can add its own planks to the ones the base data has. The
base data defines `@planks` (all eight woods' planks).

### Making things

- The result appears when the grid holds a recipe; if two recipes could both
  match, the one that was defined first is made, so give recipes that make
  different things different shapes. (The tests check that none in the base
  data and the shipped mods clash.)
- Taking the result uses up one of each ingredient, or the amount asked for
  with `*N` or by a recipe that was too big for the grid. Shift-click makes
  as many as the grid and the inventory allow.
- The recipe book lists the recipes that fit the open grid (so a bench lists
  its own and every smaller recipe) and can be made with the items you carry;
  a click fills the grid from the inventory.

```
group gems ruby crystal
shaped glow_ring 1 ".G." "G.G" ".G." with G @gems
craft lamp 1 glass 1 crystal 1
craft lantern 2 lamp 1 iron_ore 1
bake bread 1 dough 1
smelt iron_ingot 1 iron_ore 1 coal 1
```

Every name must be an existing item. Recipes are added, never replaced: a pack
cannot remove a base recipe. A recipe that does not read is left out, with a
warning that names its file and line.

The base game's own tiers, as an example of the syntax (`tools/gen_base.py`):

```
block crafting_bench            # tex bench5_top bench5_side bench5_side
  use craft 5
block workshop
  use craft 7
shaped crafting_bench 1 "III" "ITI" "PPP" with I iron_ingot T crafting_table P @planks
shaped workshop 1 "IIIII" "IPPPI" "IPBPI" "IPPPI" "IIIII" with I iron_ingot P @planks B crafting_bench
shaped big_chest 1 "PPPPP" "P...P" "P.C.P" "P...P" "PPPPP" with P @planks C oak_chest
shaped banquet_table 1 "PPPPPPP" ".S...S." ".S...S." with P @planks S stick
```

The bench is made at a table and the workshop at a bench, so the tiers must
be climbed in order. The painters `bench5`, `bench7` and `bench_side`
(`src/content/painters/benches.rs`) draw the top with a 5 x 5 or 7 x 7 grid
of cells and the plated side, in any colour.

`tools/craft_convert.py` rewrites `craft` lines into `shaped` ones where the
shape is well known (tools, stairs, slabs, doors, beds, chests, furnaces
...), and `tools/craft_convert.py --check FILE...` looks for recipes that
could clash. `tools/gen_base.py` runs the converter on `data/base.txt`.

## Models

Models are [Blockbench](https://www.blockbench.net) files (`.bbmodel`,
generic/"free" format): cubes, the bone tree, per-face UVs and the embedded
texture are read directly, on every platform, with no conversion step.

```
model <id> builtin <name>            a model shipped with the game
model <id> file <model.bbmodel>      a file, path relative to this file
```

Built-in names (every file in `data/models/`): `player player_slim grazer hand
hand_slim`. The creature models (`cow`, `wolf`, `zombie`...) are files of the
wildlife, monsters, villagers and wanderer mods, declared with `model <id> file
models/<id>.bbmodel`.

Bones are animated by name (capitals do not matter):

| Bone name | What it does |
| --- | --- |
| `Head` | turns to look |
| `Body` | sways, bobs or bends, depending on the creature's `anim` |
| names containing `Thigh`, `Leg` (but not `Lower`) or `Arm` (but not `Forearm`) | swing when walking. The side comes from `L-`/`R-` or `Left`/`Right` at the start, or `-L`/`-R` at the end |
| `Leg-FL`, `Leg-FR`, `Leg-BL`, `Leg-BR` | four-legged walkers: diagonal pairs move together |
| `Leg-L1`, `Leg-R1`, `Leg-L2`, ... | many-legged creatures: odd and even pairs step in turn |
| names starting with `Shin`: `Shin-FL` under `Leg-FL`, `Shin-L1` under `Leg-L1`, ... | the lower half of a leg. Make it a child bone of its leg, pivoted at the knee, with the same ending as the leg's name. It folds as the leg comes forward and straightens as it pushes back |
| names starting with `Forearm`: `Forearm-L` under `Arm-L`, ... | the lower half of an arm, a child bone of its arm, pivoted at the elbow. It keeps a slight bend and swings a little |
| `tail`, or `tail-a`, `tail-b` ... `tail-h` | wags; lettered pieces follow one another along the tail |
| `wing-l`, `wing-r` | flap |
| `ear-l`, `ear-r` | twitch |

Other bones keep still. Shins and forearms are optional: a leg or an arm
without one is a single stiff piece, as before. The name has to start with
`Shin` or `Forearm` (a bone called `L-Forearm` keeps still). The base dog
model (`models/dog.bbmodel` of the wildlife mod) is one with shins.

Redefining the model `player` replaces the player's own body (and the
Lurker's and Wanderer's, which use it).

## Creatures

```
entity <id>
  name <display name>
  model <model id>
  tint <r> <g> <b>          colour the model is multiplied by
  tex <picture>             the picture it is painted with (default: the model's own)
  height <0.1..6>           blocks; the model is scaled to this height (default 1.8)
  width <0.1..3>            blocks, for collision (default 0.6)
  speed <0..12>             blocks per second (default 2.5)
  health <1..1000>          default 10
  damage <0..100>           per hit, when it attacks (default 2)
  behavior wander | npc | hostile | swim | fly | villager
  biomes <names...>         where it appears; none = anywhere on the surface
  night                     appears only after dark
  weight <0..1000>          relative chance of spawning (default 10; 0 = never)
  drops <item> [<count>]    left behind when killed in survival (count 1..64, default 1)
  trait <names...>          flees herd jumps territorial hunter prey lazy pack follows
  anim walk | hop | slither | flap | scuttle | waddle | float
  variant <name> [<r> <g> <b>] [model <id>] [size <factor>] [tex <picture>] [biomes <names...>]   another look
  baby <size> <seconds>     it has young: how small, and how long they take to grow
  gives <item> <seconds>    right click collects one, then again after that long
  likes <item>              the food it follows a player for (with trait follows)
  tamable                   feeding it what it likes makes it a pet
  rideable                  right click with an empty hand to ride it
  shoots <damage>           it looses shots at the player from a distance
  sound <sound id>          its call
  trade <item> <n> <item> <n>   takes the first from a player, hands over the second
  job <word>                what a villager does
  glow <0..15>              not dimmed by the dark
  explodes <power> <seconds> <range>   it blows up when a player stays within range for that long
```

Behaviours:

- `wander`: strolls about. The default.
- `npc`: strolls, and walks along with a nearby player.
- `hostile`: hunts survival players and hits them. Appears only when the
  player is in survival mode.
- `swim`: lives in open water and stays there. Only `swim` creatures appear
  in water and they never appear on land.
- `fly`: flutters about above the ground, steering round obstacles.
- `villager`: keeps near the place it first stood, goes back there at dusk,
  and by day does its `job`.

Traits (one `trait` line may list several; what each does in play is in
[CREATURES.md](CREATURES.md#temperaments)): `flees`, `herd`, `jumps`,
`territorial`, `hunter`, `prey`, `lazy`, `pack`, `follows`. A `territorial`
or `pack` creature uses its `damage` when it turns on a player; a `hunter`
uses it on `prey`.

`anim` chooses how the model moves: `walk` (the default), `hop`, `slither`
(no leg swing; body and tail weave), `flap` (wings beat), `scuttle` (quick
short steps), `waddle` (the body rocks), `float` (drifts; tail pieces
trail).

`variant` may be given several times. Each creature that appears is given
one look at random: the entity's own, or one of its variants. The name is
shown before the creature's name (`Black Sheep`); an underscore in it
becomes a space. The three numbers, if present, are the variant's tint;
`model <id>` gives it another model; `size` (0.2..3) scales it relative to
the entity's `height`; `tex <picture>` paints it with another picture.

```
entity sheep
  ...
  variant Black tex sheep_black
  variant Ram model sheep_ram size 1.1
```

`biomes <names...>` (last on the line; it takes every word up to the next
keyword) makes the look a camouflage. A creature appearing in one of those
biomes (surface or cave names, as for the entity's own `biomes`) is given one
of the looks that name its biome, and always one of them; in any other biome
it gets the entity's own look or a variant that names no biome. So the
entity's own look is the fallback, and a spawn egg or `/spawn`, which have
no biome, give any look at random.

```
entity bush
  ...
  variant Sandy model bush_sand biomes desert savanna
  variant Snowy model bush_snow biomes tundra
```

**Pictures.** A creature is painted with the picture called `tex` (on the
entity or on a variant), or else with the picture named after its model.
A picture name is looked for in a texture pack's `textures/entity` folder,
then among the pictures that mods brought, then among the game's own
(`data/textures/entity/<picture>.png`: the player and the skins).
A mod brings a picture with a line `picture <name> file <picture.png>` (the path
is relative to the pack file; the file goes to players of a server with the rest
of the pack). The pictures are written by `tools/entity_textures.py`. The picture must be laid out like the model's own
texture (the same shape, with each part where the model's faces say); a
picture of another shape is ignored for a model's own picture. The tint still
multiplies the picture, so a creature with a picture of its own usually has no
tint. How the pictures are made, and how a pack replaces them, is in
[TEXTURING.md](TEXTURING.md#creatures-and-players).

`baby <size> <seconds>`: the size is relative to an adult (0.2..0.9) and the
time at least 5 seconds. Young have half the health, grow steadily, and are
shown as `Young <name>`. Young ones sometimes arrive with adults; they do
not `give`, hunt or defend territory until grown.

`explodes <power> <seconds> <range>`: for a `hostile` creature. When a
player in survival mode stays within `range` blocks (0.5..16, also within 3
blocks up or down) its fuse is lit: it stops, hisses, swells by a third and
flashes. After `seconds` (0.2..30) it explodes with `power` (0.1..16: the
blast reaches about twice that many blocks) and is gone; the explosion is the
game's own (`crate::explosion::explode` with the source `Creature(<its
kind>)`), so blocks, drops, damage and the script events `explode` and
`blockexploded` are as for TNT. If the player gets further than 1.6 times the
range, the fuse goes out and starts over. Tamed and young ones never explode.
The wildlife mod's Boomer is `explodes 2.0 1.5 3.0` (see
[CREATURES.md](CREATURES.md#boomers)).

A creature whose `drops` names an item no pack defines still works: it
just drops nothing, and the load says `drops unknown item` in its warnings.

`gives <item> <seconds>`: at least 1 second (120 if left out).

`trade` may be given several times; counts must be at least 1. A player
holding enough of the first item who right clicks the creature gets the
second. Any creature can trade, not only villagers.

`tamable`: needs a `likes` item. A player who right clicks a grown one
with that item in hand tames it (the item is used up in survival). A pet
follows its owner, catches up when left far behind, no longer flees or
attacks them, and, if it has a `damage`, goes for hostile creatures within
12 blocks. Right click it with anything else in hand to make it wait or
follow again (for a `rideable` pet an empty hand means riding, so hold
something). Pets are saved with the world (`<world>.pets.txt`) and come
along into other dimensions.

`rideable`: a player who right clicks a grown one with an empty hand gets
on. If the creature is also `tamable`, it has to be tamed first; if it is
not, anyone can ride it straight away. The creature turns to face the way
the rider looks and moves at its own `speed` with the rider's movement
keys; jump makes it jump (or swim upward in water). A `fly` creature flies
while ridden: jump climbs and the fly-down key descends. The get-out key
gets off. Getting on fires the script event `ride`. In the base data the
horse, donkey, camel and elephant are `tamable` and `rideable`.

`shoots <damage>`: 0..100 (3 if written alone). A creature that is
attacking the player (a `hostile` one, or an angry `territorial` or `pack`
one) looses a shot from 3 to 20 blocks away, about every two seconds, as
well as hitting up close; the base Skeleton has `shoots 3`. A `fly`
creature with `shoots` circles the player and shoots from above, from up
to 26 blocks away. Like other attackers they only go for survival players.

`sound <sound id>`: now and then a creature within 24 blocks of the player
makes its call, quieter the further away it is.

`job`: any word. `farmer`, `builder` and `guard` do work in the world
(tilling and sowing, building a hut, fighting hostile creatures; see
[CREATURES.md](CREATURES.md#villagers)). Any other word is just a title,
shown when the villager is right clicked.

`glow`: the creature is drawn at least this bright whatever the light
around it (15 is full brightness). It does not light the blocks around it.

Biome names for the surface: `plains forest desert tundra taiga mountain
swamp savanna jungle badlands`.

Cave names: `cave mushroom_cave crystal_cave lush_cave frozen_cave magma_cave
webbed_cave`. A creature with any cave name in its `biomes` lives
underground only, and one that lists plain `cave` appears in every kind of
cave. A creature cannot live both on the surface and underground: with one
cave name in the list, its surface biomes are ignored.

Inside an `entity` or a mod section (such as `vehicle`), a `model` line with
one word names the model to use; anywhere else (or with more words) `model`
declares a model.

### Spawn eggs

Every creature gets an item called `<id>_egg` (`wolf_egg`, `horse_egg`),
named after it (`Wolf Egg`), in the tab `Eggs`. Its picture is the creature
itself standing in half an eggshell. Right click with it and one of that
creature appears on the block aimed at; in survival the egg is used up.
Nothing needs declaring. A pack that declares an item with that id itself
keeps its own item, and no egg is made for that creature. Eggs can be used
in recipes, trades and `/give` like any other item.

## Vehicles

The game has no vehicles of its own. The Vehicles mod (`mods/vehicles`) is
the code that moves them: boats float, carts follow track, cars drive, and
planes fly. It adds no vehicles itself. Each vehicle is a `vehicle` section
in another mod's pack. Boats (`mods/boats`), Trains (`mods/trains`), Planes
(`mods/planes`) and Land Vehicles (`mods/landvehicles`) declare theirs. A
`vehicle` section needs the Vehicles mod switched on; otherwise the loader
warns that the section needs the mod `vehicles`.

```
vehicle <id>
  name <display name>
  category <tab name>       its tab in the creative item list (Things if left out)
  model <model id>
  icon <texture>            picture of its item
  medium water | rail | land | air     land if left out
  speed <0.5..60>           top speed, blocks per second (default 6)
  height <0.2..6>           blocks; the model is scaled to this (default 1)
  width <0.2..3>            blocks, for collision (default 1.2)
  seat <0..5>               height of the rider's eyes above its base (default 0.9)
  seats <1..16>             how many can ride (default 1). One drives; the others get in with
                            /passenger when close, and get out with the get-out key
  storage                   it carries 27 slots of cargo
  powered                   a rail vehicle that keeps going with nobody driving
```

Any other line in a `vehicle` section is reported as an unknown property.
Each vehicle also gets an item with the same id, a stack of 1, in its
category, that sets it down, so recipes can make it: `craft boat 1 planks 5`.

The chat commands `/vehicle ID` (sets one down two and a half blocks ahead),
`/board` (gets into the nearest one within 6 blocks) and `/vehicles` (says
how many there are) are the Vehicles mod's. Its condition `driving` is true
while the player is in a vehicle; `riding` is true there too.

- `water`: floats on the block `water`; barely moves aground.
- `rail`: runs only along track, which is any block tagged `tag track` (see
  [Blocks](#blocks)). It turns with the track and climbs one block at a
  time. With `powered` it keeps going at the speed and direction it was last
  driven, waits at blocks tagged `tag station`, and turns back at the end of
  the line when nobody drives it. Rail vehicles can be chained into trains
  with the item `chain`, from the Trains mod (see
  [PLAYING.md](PLAYING.md#railways)).
- `land`: drives on the ground. The default.
- `air`: rolls slowly until the throttle is past half, then flies where the
  rider looks.

How they are driven is in [PLAYING.md](PLAYING.md#vehicles).

The Land Vehicles mod, `mods/landvehicles/pack.txt`, is a short complete
example of a model, textures, vehicles and recipes.

## Sounds

Every sound is made when it is played, from a burst of noise and a tone
that glides from one pitch to another, unless the sound is a recording
(see [Sound files](#sound-files)).

```
sound <id> <effects|steps|ui> <seconds> <noise> <hz from> <hz to> <low-pass> <loudness> [caption]
```

| Part | Meaning | Range |
| --- | --- | --- |
| `effects`, `steps` or `ui` | which volume setting governs it (EFFECTS, FOOTSTEPS, INTERFACE) | |
| seconds | how long it lasts | 0.01..6 |
| noise | how much of it is noise rather than tone | 0..1 |
| hz from, hz to | the tone's pitch at the start and at the end | 10..8000 |
| low-pass | how much is let through: small numbers are dull and rumbling, 1 is bright | 0.005..1 |
| loudness | | 0..2 |
| caption | the rest of the line: what SUBTITLES FOR SOUNDS shows. A sound without one has no subtitle | |

All eight parts before the caption are needed. Pitch varies by a tenth
either way each time, so repeats do not sound identical. Twelve sounds can
play at once.

```
sound thunder effects 1.80 1.00 60 40 0.04 1.00 Thunder
sound chirp effects 0.10 0.02 2400 3200 0.90 0.30 Bird chirps
```

Declaring an id again retunes that sound. The game itself plays these ids,
so a pack may restyle them: `step break place hurt click pop splash thunder
rain rumble wave door bow bow_draw bow_load munch gulp rail`. The base data also defines `glass wood moo
baa oink cluck bark meow howl growl hiss chirp buzz groan rattle roar chime
whoosh sizzle` for blocks, creatures and packs to use. The Natural Disasters
mod (`mods/disasters`) defines `quake` and `twister`.

### Sound files

A sound can be a recording instead of a made noise:

```
sound <id> <effects|steps|ui> file <path> [loudness] [caption]
```

`path` is relative to the pack file, as for textures. The file is a WAV (PCM of
8, 16, 24 or 32 bits, 32-bit float, A-law, mu-law, MS ADPCM or IMA ADPCM), an
AIFF or AIFF-C (PCM), a FLAC, an Ogg Vorbis file (`.ogg`: Vorbis I, any
number of channels up to 8, mono and stereo being the usual; floor types 0 and
1, residue types 0, 1 and 2, all codebook types; only the first stream of the
file is read), or an MP3; any number of channels, any rate, at most 12
seconds. An MP3 is MPEG-1, 2 or 2.5 layer III at any constant or variable bitrate (mono, stereo,
joint stereo); ID3 tags are skipped, and when the encoder's LAME tag says how much silence it added
at the start and end, that is cut off. Free-format MP3s and the older layers I and II are not read.
It is turned into one channel at 22050 Hz when the
pack is read, and its pitch varies a little less than a made sound's.
`loudness` is 0..2 (default 1). A file that cannot be read is a warning in
the log, and that sound does not exist. Zip packs may hold the files too.

```
sound ding ui file sounds/ding.wav 0.8 A ding
```

Players can also replace any sound, made or recorded, without a pack: put
`<id>.wav` (or `.flac`, `.aiff`, `.ogg`, `.mp3`) in the `sounds/` folder inside the game's data folder (next to
`worlds/`), for example `sounds/step.wav` or `sounds/thunder.wav`. They are
read when the game starts and the log lists them.

A sound is used by naming it:

- in a block: `sound <id>` (broken, placed, toggled);
- in an entity: `sound <id>` (its call);
- from a script or the chat line: `/sound <id>`.

A name that no `sound` line defines plays nothing.

## Dimensions

```
dimension <name> <normal|flat|void|islands>
```

A dimension is another world inside each world, with its own land, its own
saved chunks and its own vehicles. The name is letters and digits, 12 at
most. The base data declares four:

```
dimension sky islands
dimension flatland flat
dimension void void
dimension mirror normal
```

- `normal`: ordinary terrain, from a seed derived from the world's seed and
  the dimension's name.
- `flat`: flat ground.
- `void`: nothing; a player arriving gets a 5 x 5 stone platform.
- `islands`: islands in the sky.

Players move with `/dim <name>` and `/dim home`; a pack usually wraps that
in a script (`when use my_portal do /dim sky`). Not available on a server.

## Features

```
feature <name> [<name> ...]
```

Switches on the Rust code that a mod brings with it, in `mods/<name>/code`.
That code is built into the program, and it runs only while a loaded pack
says `feature <name>`. Changing it needs a rebuild. Its hooks are the table
`Native` in [src/native.rs](../src/native.rs).

| Feature | What it switches on | Declared by |
| --- | --- | --- |
| `earth` | the EARTH terrain choice, the `--earth` and `--scale` options, the map screen and place-name notices | the Earth mod (`mods/earth`) |
| `vehicles` | how vehicles move, board and are kept, the `/vehicle`, `/board` and `/vehicles` commands and the `driving` condition | the Vehicles mod (`mods/vehicles`) |

Both are mods of the author's (not shipped with the game) and their code is
in the program only when it is built with `--features native-mods`. Their
`feature` lines are on unless the mod is switched off. Any other name is
accepted and does nothing: a `feature` line can only switch on code that is
already built in.

## Effects

A status effect (poison, burning, regeneration, slowness...) is declared
with an `effect <id>` line and the indented lines under it. The game's own
are in [data/effects.txt](../data/effects.txt); a pack may add effects or
redefine one by reusing its id (the new block replaces the old, it does not
add to it). Ids are lower case letters, digits and `_`, 32 at most.

```
effect poison
  name Poison
  kind bad
  damage 1
  lethal no
  heart poison
  colour 120 200 60
  secs 12
  from entity spider 8 1
  from eat raw_fish 8 1 15
```

| Line | Meaning |
| --- | --- |
| `name <text>` | the name shown in the effect list (the id if left out) |
| `kind good\|bad\|neutral` | how the list colours the time left |
| `damage <n>` | health points lost a second at level 1, times the level (0 to 1000; decimals are kept until they add up to a whole point) |
| `lethal yes\|no` | whether that damage may take the last health point (`no`: it stops at half a heart; yes if left out) |
| `heal <n>` | health points gained a second, likewise |
| `ticks <n>` | how many lumps a second damage and healing are dealt in (0.5 to 20; 1 if left out). `damage 2` with `ticks 4` takes a point every quarter second and a half |
| `speed <percent>` | change of walking speed per level; `-15` slows, `20` quickens. Added over all active effects; never below 5% or above 400% |
| `jump <percent>` | the same for the speed of a jump |
| `melee <percent>` | the same for the damage you deal with a left click (never below 1 point) |
| `hunger <n>` | hunger points lost a second at level 1; negative feeds |
| `absorb <n>` | health points of padding per level, taken before health when hurt; it fills when the effect starts and goes when it ends |
| `vision` | night vision: the night and dark places are drawn lighter |
| `flame` | a flame is drawn beside the hearts while it lasts |
| `extinguish` | being in water ends it (burning) |
| `heart normal\|poison\|wither\|frozen` | the style of the hearts while it lasts; with several, wither beats poison beats frozen |
| `colour <r> <g> <b>` | its colour, for icons that have no picture |
| `secs <n>` | how long it lasts when a command or a source does not say (30 if left out) |
| `icon <texture>` | the icon's picture (`effect_<id>` if left out; the base game paints `effect_poison` and the others itself) |
| `from <what> <id> [seconds] [level] [chance]` | something that starts it (below) |

Several effects at once add up. Giving an effect that is already active keeps
the stronger: a higher level replaces it (and its time), the same level takes
the longer time, a lower level changes nothing. Level 1 is the base
strength; at level 3 the damage, healing, speed change and the rest are three
times as much.

**Sources**, the `from` lines. Seconds default to the effect's `secs`, level
to 1, chance (a percent) to 100.

| Line | Starts it when |
| --- | --- |
| `from block <block> [secs] [level]` | a survival player is in, on or against that block (`lava` and also `lava_1` to `lava_8`, the flowing states). It is kept going while they are, and runs on for `secs` after |
| `from entity <creature> [secs] [level] [chance]` | that creature bites the player or its shot hits them |
| `from hit <item> [secs] [level] [chance]` | the player strikes a creature with that item in hand: the creature gets the effect |
| `from eat <item> [secs] [level] [chance]` | the player eats or drinks that item (the `eat` event) |
| `from event <name>[:<what>] [secs] [level] [chance]` | the script event is raised (`wake`, `kill:spider`, or anything raised with `/fire`) |

An id that names no block, creature or item just never matches. Creatures
carry effects too, given by `from hit` or `/effect mob`: poison, burning and
wither take their health (they never drop or count as kills), regeneration
heals them, slowness slows them, and water puts a fire out. Effects on the
player are saved with the world (`effects=` and `absorb=` in its meta file);
an effect a pack no longer defines is dropped when the world is opened.

The pictures for the hearts and icons are textures: `heart_empty`,
`heart_full`, `heart_half`, `heart_<poison|wither|frozen|absorb|hardcore>_<full|half>`,
and `effect_<id>`, `effect_flame` (see
[TEXTURING.md](TEXTURING.md#hearts-and-effect-icons)). Events and conditions:
`when effectstart poison do ...`, `/if effect poison [level] then ...`
(see [MODDING.md](MODDING.md)).

## Emotes

An emote is a short animation of the player's body that the emote wheel (B)
plays. The game ships exactly one, `wave` (written by `tools/gen_base.py`
into `data/base.txt`); every other comes from a pack or a mod. An emote is an
`emote` line and the `key` lines under it:

```
emote <id> <name> <seconds> [loop]
  key <time> <bone> <x> <y> <z>
```

| Part | Meaning |
| --- | --- |
| `<id>` | lower case letters, digits and `_`, 24 at most. Redefining an id replaces the emote |
| `<name>` | what the wheel shows (one or more words, 16 characters at most, shown in capitals) |
| `<seconds>` | how long it lasts, 0.2 to 60 |
| `loop` | plays until the player moves or stops it, instead of ending after `<seconds>` |
| `key <time> <bone> <x> <y> <z>` | at `<time>` seconds (0 to the emote's length) the bone is turned by x, y and z **degrees** on top of its own turn. `<bone>` is the name of a bone of the model (`R-Arm`, `L-Forearm`, `Head`, `Chest`, `L-Thigh`... case does not matter) |

Between the keys of a bone the turn eases smoothly from one to the next.
Before its first key and after its last, the bone eases from and back to no
turn at all, so a key at 0 and one at the end are only needed to hold a pose;
in a `loop` emote the last key eases round to the first. A key at a time a
bone already has one replaces it. Up to 64 emotes, 24 bones in one and 64
keys on a bone. Emotes are listed in the order they are defined (the base
game's first), which is also what the network sends: everyone on a server
reads the same pack, so the numbers agree.

Turns are about the bone's pivot, in the model's own axes (front is -z):
**x** swings the lower end of a hanging limb forward (and raises the face of
the head), **y** turns the front toward the left, **z** swings a hanging limb
toward +x, which is out to the side for the right arm (`R-Arm`) and across the
body for the left one. The player model's bones are `Pelvis`, `Belly`,
`Chest`, `Head`, `L-Arm`, `L-Forearm`, `L-Hand`, `R-Arm`, `R-Forearm`, `R-Hand`,
`L-Thigh`, `L-Calf`, `L-Foot`, `R-Thigh`, `R-Calf` and `R-Foot`; a bone is the
child of the one above it in that list's nesting (a forearm turns with its
arm).

An emote that moves an arm or a hand (a bone whose name has `arm` or `hand`
in it) is shown in the first-person view, as the arm only. Others play on the
body that other players, and you in the third-person view, see.

The base game's wave:

```
emote wave Wave 2.4
  key 0.0 R-Arm 0 0 0
  key 0.45 R-Arm 0 0 150
  key 1.95 R-Arm 0 0 150
  key 2.4 R-Arm 0 0 0
  key 0.7 R-Forearm 0 0 -24
  key 0.95 R-Forearm 0 0 24
  key 1.2 R-Forearm 0 0 -24
  key 1.45 R-Forearm 0 0 24
```

A pack of two more (a bow, and a dance that goes on until you move):

```
emote bow Bow 2.0
  key 0.5 Belly -45 0 0
  key 0.5 Head 20 0 0
  key 1.4 Belly -45 0 0
  key 1.4 Head 20 0 0

emote dance Dance 1.6 loop
  key 0.0 Chest 0 -12 0
  key 0.4 Chest 0 12 0
  key 0.8 Chest 0 -12 0
  key 0.4 L-Arm 0 0 -120
  key 0.4 R-Arm 0 0 40
  key 1.2 L-Arm 0 0 -40
  key 1.2 R-Arm 0 0 120
  key 0.0 Belly 0 8 0
  key 0.8 Belly 0 -8 0
```

Problems (an unknown line, a time outside the emote, a number that is not a
number) are reported in the log as pack warnings and the line is skipped.

### Poses

Bodies of a person's shape (a model with arms and legs: the player, and
creatures such as the zombie or the skeleton) take postures by themselves,
with no data: crouching, sitting, sleeping, swimming, climbing, falling, riding,
and while drawing a bow, eating or striking. Bones are recognised by name
(`content/pose`). A block with `tag climb` can be climbed (the base game's
`cave_vine` has it). A block with `seat` (see Blocks) is sat on; a block
with `use bed` is slept in; creatures standing still on them sit or lie.

## Commands and scripts

```
command <name> <commands separated by ;>
when <event> [what] do <commands separated by ;>
```

`command` adds a chat command (`$1`..`$9` and `$*` stand for what was typed
after it). `when` runs commands when something happens: `start`, `every
<seconds>`, `break <block>`, `place <block>`, `use <block>`, `useitem
<item>`, `eat <item>`, `hit <entity>`, `kill <entity>`, `tame <entity>`,
`ride <entity>`, `dimension <name>`, `night`, `day`, `death`, `wake`,
`effectstart <effect>`, `effectend <effect>`, `episode <id>`,
`episode_end <id>`, and the newer events listed in
[MODDING.md](MODDING.md#hooks-reference) (`entitydeath`, `playerhurt`, `chat`,
`craft`, `weather`...). `when` lines
are added, never replaced; a `command` with a name already used replaces the
earlier one.

The commands themselves can use numbered variables (`/var`, `$NAME`, and
`$x $y $z` for where the player stands), conditions (`/if ... then ...
[else ...]`), delays (`/after`) and repeats (`/repeat`). The full
description, worked examples, and what this language cannot do, are in
[MODDING.md](MODDING.md#scripts-and-commands).

### Episodes and switches

```
episode <id> [every <seconds>] [lasts <seconds>] [moves <speed>] [funnel]
        [then <id>] [if <condition> [and <condition>]...] do <commands separated by ;>
setting <id> <on|off> <LABEL>
```

An episode is something that goes on for a while, such as an earthquake. Its
commands run about every 0.4 seconds while it lasts.

- `every`: how often it may start on its own, roughly. Without it, an
  episode starts only from a script or from `/episode ID`.
- `lasts`: 1..600 seconds (10 if left out).
- `moves`: 0..20 blocks a second. The episode starts 30 blocks away and
  travels across the player's place.
- `funnel`: drawn as a funnel (a tornado).
- `then`: an episode that starts when this one ends, if its conditions hold.
- `if`: conditions, as for `/if` (see [MODDING.md](MODDING.md#conditions)),
  joined with `and`.

Episodes do not run in multiplayer. Commands made for episodes:

- `/episode ID` starts one; `/episode stop` ends the one running.
- `/shake S`: shakes the view, 0..1, dying away by itself.
- `/pull REACH STRENGTH`: for about 0.6 seconds, drags the player towards the
  episode and lifts them when close (14 and 2.2 if left out).
- `/strip R N`: tears up to N plants and leaves (40 if left out, 400 at most)
  within R blocks (1..16, 3 if left out) of the episode.
- `/flood PEAK`: while an episode runs, the sea stands up to PEAK blocks
  (0..24) over its level near the player, rising and then draining away.

A `setting` line adds a switch to SETTINGS > GAME, on or off to start. Its
label is shown in capitals, and the first four switches from packs are
shown. A condition reads it as `setting <id>`, and the game saves it in the
settings file as `flag_<id>`. The Natural Disasters mod
(`mods/disasters/pack.txt`) is a complete example of episodes and a setting.

## Terrain

A pack can add to what the terrain generator makes:

```
ore <block> <lowest> <highest> <one in N>
plant <block> <one in N> [<biomes...>]
```

- `ore`: the block replaces about one stone block in N between the two
  heights (inclusive). Sea level is 48 and the world is 256 high, but
  invented terrain tops out at height 114, so there is no stone to replace
  above that. `ore amethyst 5 40 400`.
- `plant`: the block is set on about one grass block in N, in the listed
  surface biomes, or everywhere if none are listed. `plant glowberry_3 300
  jungle swamp`.

Both are worked out from the world's seed, so they change terrain that has
not been visited yet and, in a saved world, chunks nobody has edited.

The terrain generator asks for these blocks by id, so a pack may restyle them
but should not remove them (a missing one is reported and stone is used):

`stone dirt grass sand gravel water log leaves coal_ore iron_ore copper_ore
crystal_ore snow snowy_grass ice sandstone cactus flower_red flower_yellow
tall_grass pine_log pine_leaves deepstone clay`

These are used when they exist and quietly replaced when they do not:

| Ids | Used for | Otherwise |
| --- | --- | --- |
| `birch_log birch_leaves maple_log maple_leaves palm_log palm_leaves blossom_log blossom_leaves` | the other trees | oak (`log`, `leaves`) |
| `ruby_ore` | deep ore | stone |
| `snow_layer` | snow settling in snowfall | no settling |
| `moss glow_mushroom brown_mushroom crystal_cluster icicle magma cave_vine stalagmite cobweb` | dressing the seven kinds of cave | left out |
| `basalt` | floors of magma caves | stone |
| `farmland` | fields in Earth worlds; what a farmer villager tills | no fields, no tilling |
| `wheat_4 carrot_4 potato_4 tomato_4` | the crops standing in Earth fields | bare farmland |
| `gray_concrete` | road surface in Earth worlds | gravel |
| `yellow_concrete white_concrete` | road markings in Earth worlds | plain road |
| `planks`, `planks_stairs` | floors and stairs inside Earth buildings | stone floors, no stairs |
| `planks_table oak_chest crafting_table red_bed lamp` | furniture and ceiling lamps in Earth buildings | left out |
| `planks`, `glass` | the hut a builder villager puts up | stone, no windows |
| `light_gray_concrete brick white_concrete stone_bricks` | building walls in Earth worlds | stone |
| `light_blue_glass`, else `glass` | building windows in Earth worlds | stone |
| `orange_terracotta red_terracotta yellow_terracotta white_terracotta` | banded badlands hills | sandstone |

Likewise the textures `sun` and `moon`, and the model `player`.

## Limits

| What | Limit |
| --- | --- |
| Textures | 1024, each 16 x 16 (one 512 x 512 atlas) |
| Block states | 4096 |
| Boxes in a custom shape | 8 |
| Nested includes | 8 |
| Stack size | 999 |
| World height | 256 blocks (heights 0 to 255). There are no vertical chunk sections: a chunk is one 16 x 16 column of that height |
| Script variables | 256 in a world |
| Commands waiting on `/after` | 64 |
| Nesting of pack commands, `/if` and `/repeat` | 6 |

## Saves and changing content

Save files record block states, items and vehicles by name. Adding, removing
or reordering content does not damage a world: blocks that no longer exist
turn into air when their chunk is next loaded. More in
[MODDING.md](MODDING.md#worlds-survive-content-changes).

## Checking a pack

```sh
voxelrust --pack my-pack.txt --showcase --show hud --screenshot out.bmp
```

lays one of every block out in front of the camera and writes a picture.
`--show creatures` instead lines up one of every creature model.

## Editing the game's textures

New pictures are drawn in the OCD style: see [TEXTURING.md](TEXTURING.md).


`voxelrust --export-textures DIR` writes every texture (the game's and every
mod's) as `<id>.png`, 16 x 16 pixels, RGBA, in `DIR`, with `atlas.png` (all of
them in one picture), `textures.txt` (number and id of each) and a note on
editing. Change any of them in an image editor and copy the ones you changed
into the `textures/` folder of the data folder (the game makes it when it
starts). They replace the data's picture of the same id the next time the
game starts or packs are reloaded, in singleplayer and on servers alike. A
picture can be a PNG, JPEG, GIF, BMP or TGA (`<id>.png`, `<id>.jpg`, ...) of any size, fitted to 16 x 16; delete the file to get the original back. JPEG is read as baseline only (not progressive); a GIF gives its first picture.

### Texture packs

`voxelrust --texture-pack DIR` (repeat it for several; later ones win), or
`texture_pack=DIR|DIR` in `settings.txt`, uses a folder of pictures in the
usual Minecraft-style layout (`assets/<name>/textures/block/*.png` and
`.../item/*.png`, or a plain folder of PNG files). A texture is taken from a
pack when the pack has a picture of that name, or one the game knows it by
(a table of names in `src/content/textures.rs`: `grass_top` is `grass_block_top`,
`wood_pick` is `wooden_pickaxe`, `crystal_*` tools are `diamond_*`, `pine` is
`spruce`, crops by stage, and so on). Tall pictures (animation strips) give
their first frame, other sizes are fitted to 16 x 16, and the grey textures
the packs expect a biome to colour (grass, leaves, water) are tinted green and
blue. A picture in the data folder's `textures/` folder beats the pack's; what
neither has is made as before.
