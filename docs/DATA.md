# Content files

Textures, blocks, items, recipes, models, creatures, vehicles, sounds,
dimensions, chat commands and scripts are declared in plain text. The game's own content is [data/base.txt](../data/base.txt),
which includes [data/creatures.txt](../data/creatures.txt) (models) and
[data/wildlife.txt](../data/wildlife.txt) (every creature); all three are
built into the program. Packs are read after it:

1. The three mods that come inside the program (`data/packs/disasters`,
   `data/packs/earth`, `data/packs/landvehicles`), unless switched off.
2. `pack.txt` in the game's data folder, if it exists (the folder is printed
   at start-up; `--data DIR` chooses another).
3. Every mod in the data folder's `mods` folder, then every mod in a `mods`
   folder beside where the game was started, unless switched off.
4. Every file given with `--pack FILE`.

Mods are switched on and off in SETTINGS > MODS (see
[MODDING.md](MODDING.md#switching-mods-on-and-off)).

A pack can add new things, or redefine anything by reusing its id. There is a
worked example in [examples/pack.txt](../examples/pack.txt). How packs are
loaded, reloaded and shared by servers is in [MODDING.md](MODDING.md).

Problems in a pack never stop the game: the line is skipped and a message
such as ``pack.txt:19: unknown block property (`bogus`)`` is printed.

## Syntax

One declaration per line; `#` starts a comment. Words are separated by spaces
or tabs; indentation is optional.

These keywords start something new: `texture`, `block`, `item`, `recipe`,
`craft`, `bake`, `smelt`, `ore`, `plant`, `model`, `entity`, `vehicle`,
`sound`, `command`, `when`, `dimension`, `feature`, `include`. Any other line sets a
property of the block, item, entity or vehicle most recently started.
(`sound` with a single word, inside a block or an entity, is a property; see
[Sounds](#sounds).)

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
texture <id> flat <r> <g> <b> [noise <n>] a colour, with optional grain 0..255
texture <id> file <image.tga>             a TGA file, path relative to this file
```

Colours are three numbers 0..255.

**`builtin` painters:** `stone dirt grass_top grass_side sand gravel water
log_side log_top pine_side leaves pine_leaves planks cobble brick
stone_bricks glass coal_ore iron_ore copper_ore crystal_ore ruby_ore snow
snow_side ice sandstone cactus_side cactus_top lamp flower_red flower_yellow
tall_grass deepstone clay sun moon wand`. An unknown painter name gives a
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
vehicle hull wings chest workbench seeds sheaf loaf pizza wedge root tuber jar
egg sack cookie pie bowl bow crossbow arrow ingot`.

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
  rotate                         four facings
  flip                           also upside-down
  hardness <seconds>             time to break by hand in survival, 0..600
  tool pick | axe | shovel | hoe | none   the kind of tool that digs it faster
  drops <item> [<chance>]        what breaking it gives in survival
  drops nothing
  also <item> [<chance>] [<count>]   a second thing given when broken
  use craft | chest | oven | furnace | bed   what right-clicking it does
  toggle <block>                 right click swaps it for that block
  seat <sixteenths>              right click sits on it, that high
  eat <points>                   right click eats it where it stands
  track <percent>                rail vehicles run on it, at this share of their speed
  station <seconds>              track where a cart running by itself stops that long
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
- `use craft`: right click opens the crafting-table recipe list.
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
- `track <percent>`: 10..400 (100 if written alone). The block counts as
  rail, and rail vehicles on it move at that percentage of their speed
  (`speed_rail` has 220, `slow_rail` 40).
- `station <seconds>`: 1..120 (5 if written alone). Also makes the block
  track (at 100 percent unless `track` says otherwise). A rail vehicle that
  nobody is driving and that is not chained behind another stops on it for
  that long, then goes on. `station_rail` has 6, `depot_rail` 30.
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

## Items

Every block that is not `hidden` gets an item that places it. Declare an item
for anything else: materials, tools, food, or an item that places a block in
a special way.

Two more kinds of item are made for you: one for each vehicle (see
[Vehicles](#vehicles)) and a spawn egg for each creature (see
[Spawn eggs](#spawn-eggs)).

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
- `shoots`: right click looses a shot the way the player looks. Damage is
  0..500; speed is in blocks per second, 4..80 (32 if left out); the wait
  between shots is 0.1..10 seconds (0.6 if left out). With an ammunition
  item, each shot uses one in survival and the item says `OUT OF AMMUNITION`
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
first appear (block items, then declared items, then vehicles). A tab is
drawn as the first item of its category; its name appears when pointed at.

## Recipes

```
recipe <item> <count> <ingredient> <count> [<ingredient> <count> ...]
craft  <item> <count> <ingredient> <count> [<ingredient> <count> ...]
bake   <item> <count> <ingredient> <count> [<ingredient> <count> ...]
smelt  <item> <count> <ingredient> <count> [<ingredient> <count> ...]
```

- `recipe`: can be made **by hand**, in the inventory screen (and also at a
  crafting table).
- `craft`: needs a **crafting table** (a block with `use craft`).
- `bake`: needs an **oven** (a block with `use oven`), and is offered only
  there.
- `smelt`: needs a **furnace** (a block with `use furnace`), and is offered
  only there. Fuel is not special: list coal as an ingredient if the recipe
  should need it.

```
recipe planks 4 log 1
craft lamp 1 glass 1 crystal 1
craft rail 16 iron_ore 3 stick 2
bake bread 1 dough 1
smelt iron_ingot 1 iron_ore 1 coal 1
```

Counts are 1..999. Every name must be an existing item (blocks count, through
their automatic items). Any number of ingredients is allowed; the recipe list
draws the first three and names them all when pointed at. Recipes are added,
never replaced: a pack cannot remove a base recipe.

## Models

Models are [Blockbench](https://www.blockbench.net) files (`.bbmodel`,
generic/"free" format): cubes, the bone tree, per-face UVs and the embedded
texture are read directly, on every platform, with no conversion step.

```
model <id> builtin <name>            a model shipped with the game
model <id> file <model.bbmodel>      a file, path relative to this file
```

Built-in names (every file in `data/models/`): `player grazer ant bat bear
bee beetle bird bison boat bull butterfly camel car cargo_boat cart cat
cat_fluffy cat_lynx chest_cart chicken chicken_rooster cow cow_highland
cow_longhorn crab crawler deer dog dog_hound dog_shepherd dog_terrier donkey
duck elephant fish fox frog gator ghoul goat golem goose horse horse_pony
llama monkey mouse ogre owl penguin pig pig_boar plane rabbit rabbit_lop
scarecrow sheep sheep_merino sheep_ram skeleton skeleton_hound slime snake
spider squid tiger turkey turtle werewolf wolf worm wraith yak zombie`.

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
model (`data/models/dog.bbmodel`) is one with shins.

Redefining the model `player` replaces the player's own body (and the
Lurker's and Wanderer's, which use it).

## Creatures

```
entity <id>
  name <display name>
  model <model id>
  tint <r> <g> <b>          colour the model is multiplied by
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
  variant <name> [<r> <g> <b>] [model <id>] [size <factor>]   another look
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
the entity's `height`.

```
entity sheep
  ...
  variant Black 88 84 88
  variant Ram model sheep_ram size 1.1
```

`baby <size> <seconds>`: the size is relative to an adult (0.2..0.9) and the
time at least 5 seconds. Young have half the health, grow steadily, and are
shown as `Young <name>`. Young ones sometimes arrive with adults; they do
not `give`, hunt or defend territory until grown.

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

Inside an `entity` or `vehicle`, a `model` line with one word names the model
to use; anywhere else (or with more words) `model` declares a model.

### Spawn eggs

Every creature gets an item called `<id>_egg` (`wolf_egg`, `horse_egg`),
named after it (`Wolf Egg`), in the tab `Eggs`. Its picture is the creature
itself standing in half an eggshell. Right click with it and one of that
creature appears on the block aimed at; in survival the egg is used up.
Nothing needs declaring. A pack that declares an item with that id itself
keeps its own item, and no egg is made for that creature. Eggs can be used
in recipes, trades and `/give` like any other item.

## Vehicles

```
vehicle <id>
  name <display name>
  model <model id>
  icon <texture>            picture of its item
  medium water | rail | land | air
  speed <0.5..60>           top speed, blocks per second (default 6)
  height <0.2..6>           blocks; the model is scaled to this (default 1)
  width <0.2..3>            blocks, for collision (default 1.2)
  seat <0..5>               height of the rider's eyes above its base (default 0.9)
  storage                   it carries 27 slots of cargo
  powered                   a rail vehicle that keeps going with nobody driving
```

Each vehicle gets an item with the same id (stack of 1, tab `Travel`) that
sets it down, so recipes can make it: `craft boat 1 planks 5`.

- `water`: floats on the block `water`; barely moves aground.
- `rail`: runs only along the block `rail` and blocks with a `track`
  property, turning with the track and climbing one block at a time. With
  `powered` it holds the direction and throttle it was last driven at,
  waits on `station` blocks, and turns back at the end of the line. Rail
  vehicles can be chained into trains with the item `chain` (see
  [PLAYING.md](PLAYING.md#railways)).
- `land`: drives on the ground. The default.
- `air`: rolls slowly until near full throttle, then flies where the rider
  looks.

How they are driven is in [PLAYING.md](PLAYING.md#vehicles).

The base data declares the boats, the rail carts, the locomotive and the
plane. The `land` vehicles (`buggy`, `truck`, `kart`) are declared in the
Land Vehicles mod, `data/packs/landvehicles/pack.txt`, which is a short
complete example of textures, vehicles and recipes.

## Sounds

Every sound is made when it is played, from a burst of noise and a tone
that glides from one pitch to another. There are no sound files.

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
rain rumble wave door bow rail`. The base data also defines `glass wood moo
baa oink cluck bark meow howl growl hiss chirp buzz groan rattle roar chime
whoosh sizzle` for blocks, creatures and packs to use. The Natural Disasters
mod defines `quake` and `twister`.

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

Switches on a part of the program that is otherwise left off. The program
asks for two names:

| Feature | What it switches on | Declared by |
| --- | --- | --- |
| `disasters` | earthquakes, tsunamis and tornadoes (which still need NATURAL DISASTERS on in SETTINGS > GAME) | the Natural Disasters mod |
| `earth` | the EARTH terrain choice and the `--earth` option | the Earth mod |

Both are declared by mods that come with the game, so they are on unless
that mod is switched off. Any other name is accepted and does nothing: a
`feature` line cannot add behaviour the program does not already have.

## Commands and scripts

```
command <name> <commands separated by ;>
when <event> [what] do <commands separated by ;>
```

`command` adds a chat command (`$1`..`$9` and `$*` stand for what was typed
after it). `when` runs commands when something happens: `start`, `every
<seconds>`, `break <block>`, `place <block>`, `use <block>`, `useitem
<item>`, `eat <item>`, `hit <entity>`, `kill <entity>`, `tame <entity>`,
`ride <entity>`, `dimension <name>`, `night`, `day`, `death`. `when` lines
are added, never replaced; a `command` with a name already used replaces the
earlier one.

The commands themselves can use numbered variables (`/var`, `$NAME`, and
`$x $y $z` for where the player stands), conditions (`/if ... then ...
[else ...]`), delays (`/after`) and repeats (`/repeat`). The full
description, worked examples, and what this language cannot do, are in
[MODDING.md](MODDING.md#scripts-and-commands).

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
| `rail` | what rail vehicles run on | rail vehicles do not move |
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
`--show creatures` instead lines up one of every creature and vehicle model.
