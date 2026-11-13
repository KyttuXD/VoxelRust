# Playing

## Screens

The title screen has four buttons:

- **SINGLEPLAYER**: the list of your worlds. Click a world to open it,
  CREATE A NEW WORLD to make one, or use EXPORT / IMPORT (see
  [Moving a world](#moving-a-world-to-another-machine)).
- **MULTIPLAYER**: type a server's address and your name, then JOIN; or
  type a world name and HOST A GAME to run a server inside the game while
  you play. See [MULTIPLAYER.md](MULTIPLAYER.md).
- **SETTINGS**: see [Settings](#settings).
- **QUIT**.

A new world needs a name (letters, digits, `-` and `_`), optionally a seed (a
number or any word; empty means random), a game mode, and a terrain: NORMAL
(invented from the seed) or EARTH (see [EARTH.md](EARTH.md)).

In a world, Esc opens the pause menu: BACK TO THE GAME, SETTINGS, SAVE AND
QUIT TO TITLE. The game also saves by itself (every 30 seconds unless you
change it). Singleplayer pauses in menus; a multiplayer world never stops.

## Controls

The keys marked * can be changed in SETTINGS > CONTROLS.

| Key | Action |
| --- | --- |
| W A S D * | move |
| Mouse | look |
| Space * | jump; fly up when flying; in creative, press twice quickly to start or stop flying |
| Left Shift * | fly down when flying |
| Left Ctrl * | sprint |
| F * | toggle flying (creative mode only); get out of a vehicle |
| Left click | break a block, hit a creature, pick up a vehicle |
| Right click | place a block; use a crafting table, chest, oven or bed; open or shut a door; till soil with a hoe; eat; get into a vehicle; collect from or trade with a creature |
| Middle click | pick the block you are looking at |
| 1-9, mouse wheel | choose a hotbar slot |
| E * | inventory |
| M * | Earth worlds: the world map. Other worlds: enlarge or shrink the minimap |
| T * | chat |
| Esc | menu / close a screen |
| F1 | hide or show the HUD |
| F2 | save a screenshot (`screenshot-<number>.bmp` in the data folder) |
| F3 | details: version and build name, frame rate, position, facing, biome, seed, number of creatures, what you are looking at, chunk counts |
| F5 | third-person view |
| F11 | full screen |

A short reminder of the main keys is shown for a few seconds when you first
enter a new world.

### The minimap

A small map sits in the top right corner, north up, with you as a dot that
shows which way you face. It only shows ground you have been near; the rest
stays dark. In normal worlds M switches it between small and large; in Earth
worlds M opens the full map instead (see [EARTH.md](EARTH.md#the-map-m)).
SETTINGS > GAME > MINIMAP turns it off.

## Survival and creative

**Survival.** You start with empty hands. You have 20 health (ten hearts) and
20 hunger. Blocks take time to break and give items; you can reach 4.5
blocks. Falling more than 3.5 blocks hurts, and so does running out of air
under water (the bubbles appear when you are submerged). Health slowly comes
back when you have not been hurt for a few seconds and hunger is above 6.
Hostile creatures come out at night (and live in caves at any hour) and only
hunt survival players. When you die you can RESPAWN with full health at the
last bed you used, or at the world's starting point if you have used none;
you keep your inventory. As you dig a block, cracks spread over it until it
breaks.

**Creative.** The inventory screen offers every item, sorted into tabs. Each
tab is a small picture of something it holds; point at it to see its name.
Blocks are drawn as little 3D blocks, other items as flat pictures. Blocks
break at once, nothing hurts you, and F (or pressing jump twice quickly)
toggles flying. You reach 6 blocks. Area tools such as the Builder's Wand (which works on a ball
of blocks at a time) only work in creative.

The mode is chosen when the world is made and can be switched at any time in
SETTINGS > GAME while the world is open.

### Hunger and food

Hunger drops one point every 45 seconds, twice as fast while moving quickly.
At zero you lose health instead. Right click with food in hand to eat: it
restores its points of hunger and half as much health.

| Food | Points | Where from |
| --- | --- | --- |
| Apple | 3 | sometimes drops from oak leaves |
| Berries | 2 | sometimes drop from tall grass and glow vines |
| Mushroom | 2 | brown mushrooms in mushroom caves |
| Raw Meat, Raw Fish | 2 | animals; fish and squid |
| Carrot, Tomato | 3 | farming |
| Potato | 2 | farming |
| Milk | 2 | cows and goats |
| Egg | 1 | chickens and ducks |
| Cooked Fish | 6 | oven, or crafting table with coal |
| Cooked Meat | 8 | oven, or crafting table with coal |

More filling food is made at a crafting table or baked in an oven; see
[The oven and baking](#the-oven-and-baking).

### Tools and ores

A tool digs the blocks that suit it several times faster: picks for stone and
ores, axes for wood, shovels for dirt, grass, sand, gravel, clay and snow. Any
block can still be broken by hand, just slowly. Hoes and swords come in the
same six materials; see [Farming](#farming) and [Swords](#swords).

| Tool material | Speed |
| --- | --- |
| Wood | 2x |
| Stone (cobble) | 3x |
| Copper | 4x |
| Iron | 6x |
| Crystal | 8x |
| Ruby | 11x |

Ores are found in stone: coal at any depth, iron below height 60, copper
below 44, crystal below 24 and ruby below 16 (sea level is 48). Coal, crystal
and ruby ore drop coal, crystals and rubies; iron and copper ore are used in
recipes as they are.

### Crafting

Press E. In survival the upper part of the inventory lists what you can make;
recipes you have the ingredients for come first, and pointing at one shows
what it needs. Click a recipe to make it.

- **By hand** only a few things can be made: planks from logs, sticks, a
  wooden pickaxe, the crafting table (4 planks), flour and dough.
- **At a crafting table** (place it, then right click it) everything that is
  not baked is offered: tools, hoes, swords, stairs, slabs, fences, walls,
  doors, beds, glass, coloured blocks, chests, rails, vehicles, the oven and
  so on.
- **At an oven** (right click it) the baking recipes are offered; see
  [The oven and baking](#the-oven-and-baking).

In the inventory, left click picks up or puts down a whole stack, right click
picks up half or puts down one. Clicking outside the panel throws away what
the mouse carries.

### Chests

Right click a chest to open its 27 slots. Each chest remembers its contents;
breaking a chest in survival gives you what was inside. Chests come in six
woods and sixteen colours; they all work the same.

### Sand and gravel

Sand and gravel fall when nothing is under them, one block at a time, so
digging under them brings them down.

### Doors, trapdoors and gates

Each of the six woods has a door (two blocks high), a trapdoor and a gate.
Right click opens and shuts them. A door needs room for both halves; opening
or breaking either half does the same to the other. At a crafting table: 6
planks make a door, 3 planks make 2 trapdoors, 2 planks and 4 sticks make a
gate. They are in the Doors tab in creative.

### Beds

A bed is made from 3 wool and 3 oak planks, in any of the sixteen colours.
Right click it:

- It becomes the place you come back to after dying.
- At night, in singleplayer, you sleep: it is morning at once.
- By day it tells you `YOU CAN SLEEP WHEN IT IS DARK`. On a server the night
  is not skipped.

### Farming

1. **Make a hoe** at a crafting table (2 of the material and 2 sticks) and
   right click dirt or grass to turn it into **farmland**.
2. **Find seeds.** Breaking tall grass sometimes gives Wheat Seeds, a red
   flower Tomato Seeds, a yellow flower Carrot Seeds, and digging dirt now
   and then turns up Potato Seeds.
3. **Sow** by right clicking farmland with seeds in hand.
4. **Wait.** A crop grows through four stages, each taking about 45 to 60
   seconds depending on the crop. It only grows under open sky (leaves and
   glass above do not stop it) and only while you are within about 24
   blocks.
5. **Harvest** by breaking the ripe plant: you get the produce and two
   seeds. A plant broken early gives one seed back.

Crops stand only on farmland; dig the farmland away and the plant comes
loose. The four crops are wheat, carrot, potato and tomato. Carrots,
potatoes and tomatoes can be eaten as they are; wheat is for flour.

### The oven and baking

An oven is made at a crafting table from 8 cobble and 1 coal. Right click it
for the baking list. Baking needs no fuel and takes no time: click a recipe
you have the ingredients for.

By hand: 2 Wheat make Flour, 2 Flour make Dough. At a crafting table: 2 Milk
make Cheese, 2 Tomatoes make Tomato Sauce, a Tomato and a Carrot make a Salad
(6 points), and Bread, Cooked Meat and Cheese make a Sandwich (11).

| Baked in the oven | Points | From |
| --- | --- | --- |
| Bread | 6 | 1 Dough |
| Baked Potato | 6 | 1 Potato |
| Fried Egg | 4 | 1 Egg |
| Cooked Meat | 8 | 1 Raw Meat |
| Cooked Fish | 6 | 1 Raw Fish |
| Cookie (makes 4) | 2 | 1 Flour, 1 Berries |
| Cheese Pizza | 12 | 1 Dough, 1 Tomato Sauce, 1 Cheese |
| Meat Lover's Pizza | 16 | 1 Cheese Pizza, 2 Cooked Meat |
| Garden Pizza | 14 | 1 Cheese Pizza, 1 Tomato, 1 Carrot |
| Apple Pie | 9 | 1 Dough, 2 Apple |
| Berry Pie | 9 | 1 Dough, 3 Berries |
| Meat Pie | 11 | 1 Dough, 1 Cooked Meat, 1 Potato |
| Stew | 10 | 1 Cooked Meat, 1 Carrot, 1 Potato; or 3 Mushroom, 1 Potato |
| Fish Soup | 9 | 1 Cooked Fish, 1 Potato, 1 Milk |
| Cake | 12 | 2 Flour, 1 Egg, 1 Milk, 1 Berries |

Cheese is worth 4 points, Tomato Sauce 2 and Dough 1; Flour cannot be eaten.
Pizzas, pies, stew and soup stack to 16.

### Swords

A sword is 2 of the material and 1 stick at a crafting table. A bare hand (or
anything that is not a sword) does 3 damage to a creature.

| Sword | Damage |
| --- | --- |
| Wooden | 6 |
| Stone | 7 |
| Copper | 8 |
| Iron | 10 |
| Crystal | 12 |
| Ruby | 15 |

### Caves

Underground the caves differ from region to region:

| Cave | What you find |
| --- | --- |
| Plain | stalagmites |
| Mushroom | moss floors, brown Mushrooms (food) and Glowcaps (which give light) |
| Crystal | Crystal Clusters that light the walls and drop Crystals |
| Lush | moss, tall grass, and Glow Vines hanging from the roof (they glow and sometimes drop Berries) |
| Frozen | ice floors, snow and Icicles; found under tundra and taiga |
| Magma | basalt floors with glowing Magma, deep down (below height 26). Standing on Magma burns: 2 health a second |
| Webbed | Cobwebs on floor and roof. Walking through one slows you to a quarter speed; breaking it gives String |

Each kind of cave has its own creatures; see
[CREATURES.md](CREATURES.md#cave-dwellers).

### Water

Water is animated, and the whole surface rises and falls slightly in a slow
swell.

## Vehicles

Vehicles are items (the Travel tab in creative; crafting table recipes in
survival). Right click with one in hand to set it down on the block you aim
at, or on water.

| Vehicle | Travels on | Notes |
| --- | --- | --- |
| Boat | water | |
| Cargo Boat | water | has storage |
| Rail Cart | rails | follows the track, including corners and slopes |
| Chest Cart | rails | has storage |
| Buggy | land | has storage |
| Plane | air | |

- **Get in**: right click it. **Get out**: F.
- **Drive**: W / S for forward and reverse, A / D to steer. Carts steer
  themselves along the track; lay it with Rail blocks.
- **Plane**: hold W to gather speed. Once fast enough it flies, and climbs or
  dives as you look up or down. Let go and it slows and sinks.
- **Storage**: hold the fly-down key (Left Shift) and right click a vehicle
  that has storage, from outside or while riding.
- **Pick up**: left click it from outside. You get the item back, and
  anything in its storage.

Vehicles are saved with the world. They are single items that you place, not
ships built from blocks.

## Creatures

There are 58 kinds: farm animals that give milk, wool and eggs, pets that
follow you for food, wildlife with their own temperaments, birds and fish,
night monsters, cave dwellers, and villagers who trade and work. Right click
a creature to collect from it or trade; left click to hit it. All of it is in
[CREATURES.md](CREATURES.md).

## Weather and disasters

Rain, snow and thunderstorms come and go by region: walk far enough and you
leave a storm behind. Cold biomes get snow, which settles on the ground;
deserts stay almost dry. Turn weather off with SETTINGS > GAME > WEATHER.

NATURAL DISASTERS (same tab, **off** by default) adds:

- **Earthquakes**: the view shakes for a few seconds.
- **Tsunamis**: after an earthquake near the sea, the water rises about three
  blocks over low ground, then drains away again.
- **Tornadoes**: form under thunderstorms over plains and savanna, pull you
  in and throw you upward, and tear up leaves and plants along their path.

Disasters only happen in singleplayer.

## Chat and commands

T opens the chat line; Enter sends, Esc cancels. A line that starts with `/`
is a command. Commands and ids are typed in lower case (the line is lowered
for you); ids are the ones in the data files, such as `iron_pick` or `wolf`.

| Command | Effect | On a server |
| --- | --- | --- |
| `/help` | list the commands | works |
| `/reload` | read the content packs and the mods folders again without restarting | no: the server decides the content |
| `/give ITEM [N]` | put N of an item in your inventory (1 if N is left out, 999 at most) | works |
| `/time WHEN` | set the time: `day` or `noon`, `night` or `midnight`, `dawn` or `morning`, `dusk` or `evening`, or a number from 0 up to (not including) 1, where 0 is dawn and 0.25 noon | no |
| `/tp X Y Z` | move to that position | works |
| `/spawn CREATURE [N] [young]` | put N of a creature (1 to 20) in front of you; with `young`, young ones if that kind has them | no |
| `/mode creative` or `/mode survival` | switch game mode | no |
| `/weather clear`, `rain` or `storm` | hold the weather that way; any other word (or none) gives it back to the game | no |
| `/set X Y Z BLOCK [TURNS]` | put a block at that position (whole numbers; the place must be loaded). TURNS, 0 to 3, picks which way a block with `rotate` faces. Meant for building test scenes | no |
| `/seed` | show the world's seed | works |

`/time`, `/spawn`, `/mode`, `/weather` and `/set` answer `NOT ON A SERVER`
there. `/give` and `/tp` work in survival too; there are no permissions.

## Settings

Settings are kept in `settings.txt` in the data folder.

- **VIDEO**: view distance (2-48 chunks), distant terrain (0-256 chunks, 0 is
  off), field of view, interface size, HOLD FRAME RATE (lowers the view
  distance by itself to keep a rate you choose), frame rate limit, vsync,
  clouds, 3D DETAIL (draws the 3D view at a lower resolution; a big help on
  old graphics cards), show frame rate, full screen and its resolution.
  Vsync is off unless you turn it on.
- **AUDIO**: master, effects, footsteps and interface volume.
- **CONTROLS**: mouse speed, invert mouse, every key marked * above, RESET
  KEYS. Click a binding, then press the new key (Esc cancels).
- **GAME**: player name, autosave interval, view bobbing, weather, natural
  disasters, place notices (Earth worlds), game mode of the open world, the
  map data relay (see [EARTH.md](EARTH.md)), the minimap, and RELOAD CONTENT
  PACKS NOW.
- **SKIN**: pick one of the built-in looks or use your own 64 x 64 PNG. See
  [MODDING.md](MODDING.md#player-skins).

### Full screen on a slow graphics card

The first time you play in full screen at the desktop's own resolution, the
game times itself for four seconds. If it manages fewer than 170 frames a
second on a display wider than 1440 pixels, it switches full screen to 1280
x 800, which an old graphics card draws much faster, and says so in the chat
area. This happens once; it does not happen with vsync on. Change it back
under SETTINGS > VIDEO whenever you like.

## The data folder

Worlds, settings, screenshots, `pack.txt` and the `mods` folder (see
[MODDING.md](MODDING.md)) live in one folder, chosen by SDL for your system (for example `~/.local/share/VoxelRust/VoxelRust/` on
Linux). The game prints `voxelrust: data in ...` when it starts; `--data DIR`
uses another folder.

## Command-line options

`voxelrust --help` prints this list.

| Option | Meaning |
| --- | --- |
| `--world NAME` | open (or create) this world straight away |
| `--seed N` | seed for a new world (a number or any word) |
| `--mode MODE` | `survival` or `creative`, for a new world |
| `--join HOST[:PORT]` | join a multiplayer server straight away |
| `--host NAME` | host the shared world NAME from inside the game, and play in it; others join with this machine's address |
| `--server` | run a multiplayer server instead of the game (no window); uses `--world`, `--seed`, `--earth`, `--port` and `--players` |
| `--port N` | server port (default 25590) |
| `--players N` | most players at once on the server (default 16) |
| `--earth LAT LON` | make the new world real-world terrain starting there |
| `--scale M V` | with `--earth`: metres per block across and up (default 30 75) |
| `--distance N` | view distance in chunks, 2..48 |
| `--far N` | distant terrain out to N chunks, 0 = off (up to 256) |
| `--threads N` | worker threads, 0 = none (default: one per spare core) |
| `--size WxH` | window size (default 1280x720) |
| `--fullscreen` | start in full screen |
| `--display N` | open on display N (0 is the main one) |
| `--render-scale P` | draw the 3D view at P percent of full resolution (25..100); much faster on old graphics cards |
| `--novsync` | do not wait for the display refresh |
| `--data DIR` | where worlds and settings are kept |
| `--log FILE` | also write what the game prints to FILE |
| `--pack FILE` | extra content file (blocks, items, textures); may be given several times |
| `--screenshot FILE` | load the world, save one BMP frame, and exit |
| `--bench N` | load the world, time N frames, and exit |
| `--flypass N` | like `--bench`, but flying straight ahead over the land for N frames; saves four pictures (`flypass-1.bmp` to `flypass-4.bmp` in the data folder) and reports frames spent waiting for terrain. A render test |
| `--run 'CMDS'` | with `--screenshot` / `--bench`: chat commands to carry out first, separated by semicolons, for example `--run '/time night; /spawn wolf 3'` |
| `--write-icon FILE` | save the game's emblem as a PNG and exit |
| `--icon-size N` | with `--write-icon`: its size in pixels (default 256) |
| `--version` | print the version and which build this is, and exit |
| `--export FILE` | with `--world NAME`: write that world as one file, and exit |
| `--import FILE` | with `--world NAME`: add a world from such a file, and exit |
| `--pos X Y Z` | start position |
| `--look YAW PITCH` | start direction, in degrees |
| `--time T` | time of day, 0..1 (0.25 = noon) |
| `--hidden` | do not show the window (with `--screenshot` / `--bench`) |
| `--showcase` | with `--screenshot`: lay out every block in front of the camera first (handy for checking a pack) |
| `--show LIST` | with `--screenshot`: also draw `hud`, `debug`, `pause`, `items`, `title`, `worlds`, `newworld`, `dead`, `map`, `settings:TAB`, `third` (camera behind the player) or `creatures` (one of each) |

`--version` prints, for example, `voxelrust 0.1.0 (linux-x86_64)`; the word in
brackets is the build name (see [BUILDING.md](BUILDING.md#build-names-and-updating)).

`--flypass`, `--bench`, `--run` and `--screenshot` are aids for developers
and pack makers. So is `--exp N`, which `--help` does not list: it switches
off parts of the drawing so their cost can be timed (add up 1 for the alpha
test, 2 for fog, 4 for the light texture, 16 for minimap painting).

A world created with `--world` and no `--mode` is a creative world; one
created from the menu is survival unless you change it.

## Moving a world to another machine

A whole world can be written as one `.vxworld` file.

**From the menu.** On the SINGLEPLAYER screen, EXPORT beside a world writes
`<data folder>/<name>.vxworld`. To bring one in, type its path in the IMPORT
field and click IMPORT. A bare file name is looked for in the data folder.
The world is named after the file; if that name is taken, `-2`, `-3`... is
added.

**From the command line.**

```sh
voxelrust --world myworld --export myworld.vxworld
```

```sh
voxelrust --world copy --import myworld.vxworld
```

`--import` refuses to overwrite an existing world. Both commands exit when
done and open no window.
