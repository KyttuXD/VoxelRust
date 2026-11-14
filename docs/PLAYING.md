# Playing

## Screens

The title screen has four buttons:

- **SINGLEPLAYER**: the list of your worlds. Click a world to open it,
  CREATE A NEW WORLD to make one, or use EXPORT / IMPORT (see
  [Moving a world](#moving-a-world-to-another-machine)).
- **MULTIPLAYER**: a list of saved servers. Click one to join it, or use
  EDIT and X beside it. ADD SERVER opens a screen for a name and an address
  (SAVE, SAVE AND JOIN, CANCEL; Tab moves between the boxes). YOUR NAME is
  the name you play under. Type a world name in SHARED WORLD NAME and use
  HOST A GAME to run a server inside the game while you play. See
  [MULTIPLAYER.md](MULTIPLAYER.md).
- **SETTINGS**: see [Settings](#settings).
- **QUIT**.

The bottom of the title screen shows the version, the build name and the
commit, and above that what the update check found (see
[Updating](#updating)). The menus stand in front of a backdrop made of the
game's own blocks.

A new world needs a name (letters, digits, `-` and `_`), optionally a seed (a
number or any word; empty means random), a game mode, and a terrain. The
TERRAIN button steps through:

- **NORMAL**: terrain invented from the seed.
- **FLAT**: level grassland as far as you go (grass at height 51 over dirt
  and stone; no hills, water, trees, caves or ores). A place to build. The
  distant terrain of a flat world has no trees either (earlier versions drew
  far-off trees there, which showed as pink blobs).
- **EARTH**: the real planet (see [EARTH.md](EARTH.md)). Offered only while
  the Earth mod is on (see [Mods](#mods)).

Every world is 256 blocks high, from height 0 to 255. Sea level is 48 and
invented terrain never rises above 114, so there is plenty of room to build
upward. A chunk is one column of the full height: there are no vertical
chunk sections, and nothing above 255 or below 0. Worlds saved when the
height was 128 still open; their chunks become the lower half.

In a world, Esc opens the pause menu: BACK TO THE GAME, SETTINGS, SAVE AND
QUIT TO TITLE. The game also saves by itself (every 30 seconds unless you
change it). Singleplayer pauses on the pause menu and in Settings; a
multiplayer world never stops.

## Controls

The keys marked * can be changed in SETTINGS > CONTROLS.

| Key | Action |
| --- | --- |
| W A S D * | move |
| Mouse | look |
| Space * | jump; fly up when flying; in creative, press twice quickly to start or stop flying; get up from a seat |
| Left Shift * | fly down when flying |
| Left Ctrl * | sprint |
| F * | get out of a vehicle; get off a creature; get up from a seat |
| (chat) /passenger | ride along in a nearby vehicle with a free seat (buses, ferries, jeeps...) |
| (chat) /lock, /unlock | lock the vehicle you are in or looking at to you, or release it |
| Left click | break a block, hit a creature, pick up a vehicle |
| Right click | place a block; use a crafting table, chest, oven, furnace or bed; open or shut a door; sit on a chair; till soil with a hoe; eat (hold); draw a bow (hold) and shoot it (let go); get into a vehicle; collect from, trade with, tame or ride a creature; hatch a spawn egg |
| Middle click | pick the block you are looking at |
| 1-9, mouse wheel | choose a hotbar slot |
| E * | inventory |
| M | The world map: the whole map on every world (Earth worlds: the Earth map) |
| N | The world map with waypoints, on every world |
| T * | chat |
| Esc | menu / close a screen |
| F1 | hide or show the HUD |
| F2 | save a screenshot (`screenshot-<number>.bmp` in the data folder) |
| F3 | details: version and build name, frame rate, position, facing, biome, seed, number of creatures, what you are looking at, chunk counts |
| F5 | third-person view |
| F11 | full screen |

Flying is creative only and has no key of its own: press jump twice
quickly to take off, and twice again to land. Nothing on screen lists the
keys; SETTINGS > CONTROLS does.

### Text boxes

Every text box (the player name, a server address, a shared world name, a
path to import) takes these keys while it has the keyboard:

- **Ctrl** or **Cmd** with **A** selects all, **C** copies, **X** cuts and
  **V** pastes.
- With nothing selected, C and X take the whole box.
- Typing or pasting over a selection replaces it. Backspace empties a
  selected box; otherwise it deletes the last character.
- There is no cursor movement and no partial selection.

In the browser version, copy and paste work inside the game only.

### The minimap

A small map sits in the top right corner, north up, with you as a dot that
shows which way you face. It only shows ground you have been near; the rest
stays dark. Waypoints show on it as small squares. In Earth worlds the minimap
shows the whole map from the start, dimmed where you have not been yet.
SETTINGS > GAME > MINIMAP turns it off.

### The world map

M opens the world map, and N opens it with waypoints. It shows the terrain
you have explored, on every world; in Earth worlds M opens the Earth map
instead (see [EARTH.md](EARTH.md#the-map-m)), and N still opens the world map.

- Drag to move, the wheel to zoom (1, 2 and 3 jump to the presets, + and -
  step), C centres on you, F toggles FOLLOW ME, G toggles the grid, H the
  waypoints. Escape or M or N closes it.
- Right click adds a waypoint where the pointer is; ADD HERE adds one at the
  centre. Click a waypoint to select it: rename it in the NAME box, GO TO
  centres the view on it, COLOUR cycles its colour, DELETE asks once more.
- Waypoints are saved with the world (`worlds/<name>.waypoints.txt`), and go
  with it when it is exported or copied. Up to 64 per world.
- The zoom, presets and toggles are kept in the settings file.

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
Blocks are drawn as little cubes in a true isometric view, other items as
flat pictures. Blocks break at once, nothing hurts you, and pressing jump
twice quickly starts or stops flying. You reach 6 blocks. Area tools that packs add (items with `brush`, which work on a ball
of blocks at a time) only work in creative. The Eggs tab holds a spawn egg
for every creature (see [CREATURES.md](CREATURES.md#spawn-eggs)). The mouse
wheel scrolls the list, and a scroll bar at its right edge can be dragged
(or its track clicked) to move through it.

The mode is chosen when the world is made and can be switched at any time in
SETTINGS > GAME while the world is open.

### Hunger and food

Hunger drops one point every 45 seconds, twice as fast while moving quickly.
At zero you lose health instead. Right click with food in hand to eat: it
restores its points of hunger and half as much health. Eating takes about a
second with the button held (see [The hand](#the-hand)).

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
[The oven and baking](#the-oven-and-baking). Some food can also be set out
as a block and eaten where it stands; see
[Furniture and food on the table](#furniture-and-food-on-the-table).

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
and ruby ore drop coal, crystals and rubies. Iron and copper ore go into
tool recipes as they are; a furnace turns them into ingots, which other
recipes need (see [Furnaces](#furnaces)).

### Crafting

Press E. In survival the upper part of the inventory lists what you can make;
recipes you have the ingredients for come first, and pointing at one shows
what it needs. Click a recipe to make it. A long list scrolls with the mouse
wheel or by dragging the scroll bar at its right edge.

- **By hand** only a few things can be made: planks from logs, sticks, a
  wooden pickaxe, the crafting table (4 planks), flour and dough.
- **At a crafting table** (place it, then right click it) everything that is
  not baked is offered: tools, hoes, swords, stairs, slabs, fences, walls,
  doors, beds, glass, coloured blocks, chests, rails, vehicles, the oven and
  so on.
- **At an oven** (right click it) the baking recipes are offered; see
  [The oven and baking](#the-oven-and-baking).
- **At a furnace** the smelting recipes are offered; see
  [Furnaces](#furnaces).

There is no crafting grid: nothing is laid out in a pattern. The lists are
the recipe book.

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

Each wood also has four more doors, all two blocks high:

| Door | Made from | What is different |
| --- | --- | --- |
| Centred | 1 door | stands in the middle of its block instead of on the edge |
| Glazed | 1 door, 2 glass | has a window |
| Sliding | 1 glazed door, 1 iron ore | slides into the wall beside it |
| Revolving | 2 glazed doors | never shuts: walk through it |

Trapdoors, gates and fences also come in the sixteen colours (an oak
trapdoor, oak gate or oak fence plus white wool, red wool and so on).

Doors do not swing: a door switches at once between its shut shape and its
open shape.

### Beds

A bed is made from 3 wool and 3 oak planks, in any of the sixteen colours
of wool.
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

The **Stove** (an oven and 3 iron ingots), the **Smoker** (an oven and 4
logs) and the **Campfire** (3 logs, 3 sticks, 1 coal) offer the same baking
list as the oven.

### Furnaces

A **Furnace** is 8 cobble at a crafting table. Right click it for the
smelting list; like baking, smelting takes no time, and coal is simply one
of the ingredients.

| Smelted | From |
| --- | --- |
| 1 Iron Ingot | 1 Iron Ore, 1 Coal |
| 3 Iron Ingots | 2 Iron Ore, 2 Coal |
| 1 Copper Ingot | 1 Copper Ore, 1 Coal |
| 3 Copper Ingots | 2 Copper Ore, 2 Coal |
| 4 Glass | 4 Sand, 1 Coal |
| 4 Stone | 4 Cobble, 1 Coal |
| 4 Brick | 4 Clay, 1 Coal |
| 2 Crystal | 1 Crystal Ore, 1 Coal |
| 2 Ruby | 1 Ruby Ore, 1 Coal |
| 2 Coal | 2 Oak Log |
| 2 Cooked Meat | 2 Raw Meat, 1 Coal |
| 2 Cooked Fish | 2 Raw Fish, 1 Coal |

The **Blast Furnace** (a furnace, 5 iron ingots, 3 stone bricks) and the
**Kiln** (8 brick) offer the same list.

### Furniture and food on the table

Furniture is made at a crafting table and is in the Furniture tab in
creative.

- **Seats**: chair, stool, bench, toilet, bathtub, and a couch and an
  armchair in each of the sixteen colours (wool and planks). Right click to
  sit down; jump, or the get-out key (F), to stand up.
- **Storage**: cupboard, kitchen counter, wardrobe and fridge open like
  chests (27 slots each). The wardrobe and the fridge are two blocks high.
- **Kitchen and bathroom**: stove (bakes like an oven), kitchen sink, wash
  basin, towel rail.
- **The rest**: desk, shelf, bookshelf, television and floor lamp (both give
  light).
- **Food set out** (Food tab): a Plate (2 clay make 4), Pizza on a Plate, Cake, Loaf of
  Bread and Fruit Bowl are blocks. Right click one to eat it where it
  stands: it restores its points like other food and the block is gone. A
  Pizza Box opens and shuts.

### Bows

| Weapon | Damage | Made from |
| --- | --- | --- |
| Bow | 6 | 3 sticks, 3 string |
| Longbow | 9 | 1 bow, 2 sticks, 2 string |
| Crossbow | 13 | 1 bow, 2 iron ingots, 2 planks |

Hold right click to draw the bow, and let go to shoot where you look. The
string comes back over about a second (a little longer for the longbow): the
bow in your hand and its picture in the hotbar change through three stages,
and your arm comes up and pulls back with it. The speed and damage of the
arrow grow with the draw, to the numbers in the table at a full draw; letting
go at once is a weak shot (a fifth of the power). The Arrow is used up when
you let go (1 stick and 1 gravel make 4); none are needed in creative, and
with none left the bow says `OUT OF AMMUNITION` and does not draw.
Changing slot or pressing left click (break) while drawing lets the string
down without shooting. The **crossbow** loads while you hold right click
(it draws fully, with a click) and shoots with the next right click. A bow
makes a creak as it is drawn and a crossbow clicks when loaded. Arrows drop
as they fly, stop at blocks and hurt the first creature they meet; in
multiplayer they fly as before for everyone.

### The hand

In first person the forearm stays in view and grips what you hold, as in
Minecraft but with the hand shown:

- a **tool or weapon** is big and thick at the lower right, its head up and to
  the left (the game's own diagonal pictures and a texture pack's upright ones
  are both gripped by the handle);
- a **block** is a small cube turned to show its top and two sides;
- a **flat item** (food, a torch, a stick) is a card with thickness;
- a **bow** is drawn back as above;
- **eating and drinking** take about a second with the button held: the hand
  rises to the mouth, takes bites (or gulps, for milk and the like) and the
  food is used up at the end. Letting go, changing slot or pressing break
  stops it with nothing eaten;
- the hand **bobs** and sways as you walk, and **swings** when you hit or
  use things: a tool winds up, strikes and recovers, anything else twists
  in and back (the timings are LastRoute's).

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

### TNT

Gunpowder (2 from 2 coal and a sand, at a crafting table) and sand make TNT
(1 from 5 gunpowder and 4 sand). Right click a block of TNT with a flint and
steel (1 from an iron ingot and some gravel) to light it. TNT is also lit by
lava or a campfire beside it, and by any explosion. A lit block flashes white
for four seconds, then explodes: it clears a ball of blocks about ten blocks
across (water, lava and unbreakable blocks stay), drops some of them in
survival, sets off TNT near it a moment later, and hurts and throws whoever
is close, you included. TNT is in the creative list's Tools tab. Lit TNT is
not saved with the world.

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

Vehicles come from four mods: Boats, Trains, Planes and Land Vehicles. Each
of them needs the Vehicles mod (`mods/vehicles`), which is on unless you
switch it off. The Vehicles mod adds no vehicle itself; it is the code that
floats boats, runs carts on track, drives cars and flies planes. With the
Vehicles mod off, the vehicle items stay in the list but cannot be set down
or ridden, and the game warns about each one. Switching one of the four
mods off removes its vehicles, recipes and (for Trains) track from the game
(see [Mods](#mods)).

| Vehicle | Travels on | Notes | From the mod |
| --- | --- | --- | --- |
| Boat | water | | Boats |
| Cargo Boat | water | has storage | Boats |
| Rail Cart | rails | follows the track, including corners and slopes | Trains |
| Chest Cart | rails | has storage | Trains |
| Locomotive | rails | keeps going by itself; see [Railways](#railways) | Trains |
| Passenger Cart | rails | | Trains |
| Freight Cart | rails | has storage | Trains |
| Plane | air | | Planes |
| Buggy | land | has storage | Land Vehicles |
| Truck | land | slower than the buggy; has storage | Land Vehicles |
| Kart | land | the fastest of the three; no storage | Land Vehicles |

The base game has no vehicles and no rails.

- **Get in**: right click it. **Get out**: F (GET OFF / GET OUT in the
  controls).
- **Drive**: W / S for forward and reverse. A / D steer boats, cars and
  planes. Rail carts do not steer: they follow the track, so lay it with
  Rail blocks.
- **Plane**: hold W to gather speed. Once fast enough it flies, and climbs or
  dives as you look up or down. Let go and it slows and sinks.
- **Storage**: hold the fly-down key (Left Shift) and right click a vehicle
  that has storage, from outside or while riding.
- **Pick up**: left click it from outside. You get the item back, and
  anything in its storage.
- **Commands**: `/vehicle ID` sets one down 2.5 blocks ahead, `/board` gets
  into a vehicle within 6 blocks, and `/vehicles` says how many there
  are.

Vehicles are saved with the world. They are single items that you place, not
ships built from blocks.

### Railways

The track and stations come from the Trains mod. All of it is in the Travel
tab in creative and has crafting table recipes. The speeds and stop times
are the `tag track` and `tag station` numbers in the Trains pack (see
[DATA.md](DATA.md)).

| Track or block | What it does |
| --- | --- |
| Rail | ordinary track |
| Speed Rail | carts run at 220% of their speed on it |
| Slow Rail | carts run at 40% |
| Station Rail | a cart running by itself stops here for 6 seconds |
| Depot Rail | the same, for 30 seconds |
| Station Platform | a low stone block to stand on beside the track |
| Buffer Stop | marks the end of a line |
| Signal Post | a lit post |

The platform, buffer stop and signal post are for looks: they do not control
anything. A cart stops wherever the track ends.

- **Trains.** A **Chain** joins rail carts. Hold it and right click the cart
  in front, then the cart to pull behind it; each chained cart follows the
  one ahead. Right click a chained cart with a chain to unhook it. A chain
  comes off by itself if the carts end up more than 12 blocks apart.
- **The Locomotive** remembers the way and speed it was last driven. Drive
  it, get out, and it carries on by itself, pulling whatever is chained
  behind. It waits at station and depot rails for their time, and at the end
  of the line it turns back the way it came. A cart you are driving does not
  stop at stations.
- A **Passenger Cart** is a cart to ride in and a **Freight Cart** carries
  27 slots of cargo; chain them behind a locomotive.

This is simple automation: timed stops and shuttling back and forth along a
line. There are no timetables, routes, junction switching or working
signals.

## Creatures

There are 75 kinds: farm animals that give milk, wool and eggs, animals you
can tame with their favourite food, wildlife with their own temperaments,
birds and fish, night monsters (some of which shoot), cave dwellers, and
villagers who trade and work. Right click a creature to collect from it,
trade or tame it; left click to hit it. Creatures behind the camera, or more
than 72 blocks away, are not drawn. Vehicles are drawn out to 96 blocks
whichever way you face; your own vehicle is only seen from outside (third
person). All of it is in [CREATURES.md](CREATURES.md).

**Riding.** A horse, donkey, camel or elephant can be ridden once it is
tamed: right click it with an empty hand. It faces the way you look; the
movement keys move it, jump makes it jump or climb, and the get-out key (F)
gets you off. See [CREATURES.md](CREATURES.md#riding).

## Dimensions

Every world has other worlds inside it, each with its own land:

| Name | Land |
| --- | --- |
| `sky` | islands in the sky |
| `flatland` | flat ground |
| `void` | nothing at all; you arrive on a small stone platform |
| `mirror` | ordinary terrain from a different seed |

`/dim NAME` takes you there and `/dim home` (or `/dim` alone) brings you
back; `/dim` with an unknown name lists them. You arrive at the same x and z
you left from. Each dimension keeps its own blocks and its own vehicles in
the same world (files named `<world>.<dimension>.*` in the data folder), and
they are included when the world is exported. Wild creatures stay behind;
pets come with you. Your inventory, the chests' contents and the explored
map belong to the world as a whole. Packs can add dimensions and portals to
them (see [MODDING.md](MODDING.md#scripts-and-commands)).

Far-off trees are drawn in the distant terrain only where the land is
ordinary terrain. Flat land, the sky islands and the void have none.

Dimensions are not available on a server.

## Weather and disasters

Rain, snow and thunderstorms come and go by region: walk far enough and you
leave a storm behind. Cold biomes get snow, which settles on the ground;
deserts stay almost dry. Turn weather off with SETTINGS > GAME > WEATHER.

Disasters come from the Disasters mod (`mods/disasters`), which comes with
the game. With the mod on (it is, unless you switch it off under SETTINGS >
MODS), the GAME tab has a NATURAL DISASTERS switch, **off** by default. Turned
on, these can happen:

- **Earthquakes**: the view shakes for seven seconds.
- **Tsunamis**: after an earthquake near the sea, the sea rises three blocks
  above its level and floods the low ground within 24 blocks of you, then
  drains away again.
- **Tornadoes**: come during thunderstorms over low plains and savanna (land
  less than 24 blocks above sea level). A funnel passes close by, pulls you
  in and throws you upward, and tears up leaves and plants along its path.

Disasters only happen in singleplayer. With the mod switched off the switch
is not shown and they cannot happen at all.

## Chat and commands

T opens the chat line; Enter sends, Esc cancels. Chat is shown in capitals.
Each player's name is shown in a colour of its own, the same every time.

**Colours.** `&` followed by one of sixteen characters changes the colour of
the text after it, in chat and in anything a pack says with `/say`:

| Code | Colour | Code | Colour |
| --- | --- | --- | --- |
| `&0` | black | `&8` | dark grey |
| `&1` | dark blue | `&9` | blue |
| `&2` | dark green | `&a` | green |
| `&3` | dark cyan | `&b` | cyan |
| `&4` | dark red | `&c` | red |
| `&5` | purple | `&d` | pink |
| `&6` | orange | `&e` | yellow |
| `&7` | grey | `&f` | white |

A line that starts with `/` is a command. Commands and ids are typed in
lower case (the line is lowered for you); ids are the ones in the data
files, such as `iron_pick` or `wolf`. In `/tp` and `/set`, a coordinate may
be written `~` for the block you stand in, or `~3` or `~-2` for that many
blocks from it.

| Command | Effect | On a server |
| --- | --- | --- |
| `/help` | list the commands in short form, every one in this table except `/help` itself | works |
| `/reload` | read the content packs and the mods folders again without restarting | no: the server decides the content |
| `/give ITEM [N]` | put N of an item in your inventory (1 if N is left out, 999 at most) | works |
| `/time WHEN` | set the time: `day` or `noon`, `night` or `midnight`, `dawn` or `morning`, `dusk` or `evening`, or a number from 0 up to (not including) 1, where 0 is dawn and 0.25 noon | no |
| `/tp X Y Z` | move to that position | works |
| `/set X Y Z BLOCK [TURNS]` | put a block at that position (whole numbers; the place must be loaded). TURNS, 0 to 3, picks which way a block with `rotate` faces | no |
| `/spawn CREATURE [N] [young]` | put N of a creature (1 to 20) in front of you; with `young`, young ones if that kind has them | only for the player whose game runs the creatures (see [MULTIPLAYER.md](MULTIPLAYER.md#what-is-shared-and-what-is-not)) |
| `/mode creative` or `/mode survival` | switch game mode | no |
| `/weather clear`, `rain` or `storm` | hold the weather that way; any other word (or none) gives it back to the game | no |
| `/seed` | show the world's seed | works |
| `/dim NAME`, `/dim home` | go to a dimension, or back (see [Dimensions](#dimensions)) | no |
| `/say TEXT` | show a line in your own chat area (it is not sent to other players) | works |
| `/sound ID` | play a sound by its id, for example `/sound chime` | works |
| `/heal [N]` | give yourself N health (4 if left out) | works |
| `/feed [N]` | give yourself N hunger points (4 if left out) | works |
| `/hurt [N]` | take N damage (1 if left out, 100 at most) | works |
| `/launch [UP] [FORWARD]` | throw yourself upward and the way you face, in blocks per second (8 and 0 if left out; -40 to 40) | works |
| `/shoot [DAMAGE] [SPEED] [BLOCK]` | loose a shot the way you look (damage 4 and speed 30 if left out). BLOCK, if given, is left where the shot lands | no |
| `/aim BLOCK` | put that block against the face you are pointing at | no |
| `/strike [DAMAGE]` | lightning on the block you point at: thunder, a flash, and DAMAGE (12 if left out) to every creature within 4 blocks | no |
| `/blast [R]` | an explosion of radius R (1 to 5; 2 if left out) round the block you point at: it clears a rough ball of blocks, drops some of them in survival, and hurts and throws you if you are near. Unbreakable blocks, water, lava and height 0 stay | no |
| `/take ITEM [N]` | remove N of an item from your inventory (1 if left out), if you have it | works |
| `/title TEXT` | show a notice in large letters in the middle of the screen for two seconds | works |
| `/var NAME = N`, `+ N`, `- N`, `random N` | set, add to or subtract from a numbered variable, or set it to a random whole number from 0 to N-1 | works |
| `/if CONDITION then COMMAND [else COMMAND]` | carry out a command only if the condition holds | works |
| `/after SECONDS COMMAND` | carry out a command later (up to 3600 seconds) | works |
| `/repeat N COMMAND` | carry out a command N times (64 at most) | works |

The commands marked "no" answer `NOT ON A SERVER` there. `/give` and `/tp`
work in survival too; there are no permissions. `/say` and everything after
it are mostly there for packs, which run them from scripts and can add
commands of their own. Variables, conditions and the rest of the scripting
commands are explained in [MODDING.md](MODDING.md#scripts-and-commands).

## Settings

Settings are kept in `settings.txt` in the data folder. A switch a pack adds
is kept there as `flag_<id>=1` or `flag_<id>=0`, and a mod's own settings
(such as the Earth mod's) sit beside them.

- **VIDEO**: view distance (2-48 chunks), distant terrain (0-256 chunks, 0 is
  off), field of view, interface size, HOLD FRAME RATE (lowers the view
  distance by itself to keep a rate you choose), frame rate limit, vsync,
  clouds, 3D DETAIL (draws the 3D view at a lower resolution; a big help on
  old graphics cards), show frame rate, full screen and its resolution.
  Vsync is off unless you turn it on.
  Three rows set how the distant terrain is made: **DISTANT BUILDS AND TREES**
  (0-8 tiles; a tile is 4 x 4 chunks), **TREES** (none, some, all) and
  **DETAIL** (coarse, medium, fine). Within that many tiles of you the far
  terrain is made from the world's real blocks, so what you have built, and
  the trees that grow there, show from afar as they are (roofs, walls, trunks
  and crowns); beyond it the land is made from the terrain function alone, as
  before. Detail is how finely it is sampled (8, 4 or 2 blocks near you,
  coarser farther off). Fine detail and many tiles cost time on a slow
  computer. The settings are `lod_real`, `lod_trees` and `lod_detail` in
  `settings.txt`.
- **AUDIO**: master, effects, footsteps and interface volume, and SUBTITLES
  FOR SOUNDS, which names what you hear (`THUNDER`, `DOOR`, `WOLF HOWLS`) at
  the right of the screen for a couple of seconds. All sound stops when you
  leave a world.
- **CONTROLS**: mouse speed, invert mouse, every key marked * above, RESET
  KEYS. Click a binding, then press the new key (Esc cancels).
- **GAME**: player name, autosave interval, view bobbing, weather, the
  minimap, RELOAD MODS NOW, and GAME MODE while a world is open. Below those
  come the switches the packs add (the first four are shown; NATURAL
  DISASTERS is one, from the Disasters mod, see
  [Weather and disasters](#weather-and-disasters)). Then, while the Earth
  mod is on, its two rows: PLACE NOTICES (off, countries, regions or towns)
  and MAP DATA RELAY (host:port, for old machines; see
  [EARTH.md](EARTH.md)).
- **SKIN**: pick one of the built-in looks or choose your own 64 x 64 picture with the built-in file browser. See
  [MODDING.md](MODDING.md#player-skins).
- **MODS**: a switch for every mod, and UPDATE THE GAME AUTOMATICALLY. See
  [Mods](#mods) and [Updating](#updating).

### Mods

SETTINGS > MODS lists every mod the game has found, with an on/off switch:
the eight that come with the game and any you have added (see
[MODDING.md](MODDING.md#where-packs-are-read-from)). Six are shown at a
time; the mouse wheel scrolls. Under ADD A MOD FROM A FOLDER OR .ZIP, type the
path of a folder, a .zip or a pack file to bring one in. A change takes
effect at once: the content is read again, as with `/reload`. The mods
switched off are remembered in `settings.txt` (`mods_off`).

| Mod | What it holds | With it off |
| --- | --- | --- |
| DISASTERS | earthquakes, tsunamis and tornadoes, the NATURAL DISASTERS switch, and two sounds | the switch is not shown and disasters cannot happen |
| EARTH | real-world terrain, the world map, place notices, the map data relay, and the `--earth` option | the EARTH terrain choice is not offered and `--earth` is refused. Earth worlds already made stay, but load no new ground |
| VEHICLES | the code that moves vehicles, and the `/vehicle`, `/board` and `/vehicles` commands. It adds no vehicle | the Boats, Trains, Planes and Land Vehicles mods warn that they need it, and no vehicle can be set down or ridden |
| BOATS | a rowing boat and a cargo boat | both boats are gone from the item list, recipes and the world |
| TRAINS | rail track and stations, rail carts, the locomotive and the chain | the rails, carts, locomotive, chain and stations are gone |
| PLANES | a small plane | the plane is gone |
| LANDVEHICLES | the buggy, the truck and the kart, with their recipes | those vehicles are gone from the item list, recipes and the world |
| HOLIDAYS | decorations and treats for the year's feasts, and some big props | those blocks and items are gone |

All eight are on to begin with. Each is a folder with a `pack.txt`:
`mods/disasters`, `mods/earth`, `mods/vehicles`, `mods/boats`, `mods/trains`,
`mods/planes`, `mods/landvehicles` and `mods/holidays`. They sit in a `mods`
folder beside the program (inside the app on a Mac, in
`Contents/Resources/mods`), in your data folder's `mods` folder, or in a
`mods` folder where you started the game. Mod code is compiled into the
program, but it runs only while that mod's pack is loaded. If the mods
cannot be found, they are not listed and their content is not in the game.

### Updating

When it starts, the game looks at the project's nightly release for a newer
version. If there is one, it downloads the program for its own kind of
machine, checks it, and puts it in place of the one you have; the new
version is used the next time you start the game. The title screen says
which of these happened:

- CHECKING FOR A NEWER VERSION...
- THIS IS THE NEWEST VERSION
- A NEWER VERSION IS INSTALLED - START THE GAME AGAIN TO USE IT
- A NEWER VERSION IS OUT (AUTOMATIC UPDATES ARE OFF)

UPDATE THE GAME AUTOMATICALLY in SETTINGS > MODS (on by default) decides
whether a newer version is installed or only announced. `--no-update` skips
the check for one run.

Things to know:

- On Mac OS X 10.6 the system's `curl` cannot make modern HTTPS
  connections, so the old-Mac build cannot update itself.
- Only the program is replaced, not the `mods` folder beside it.
- Worlds and settings are not touched.

More in [BUILDING.md](BUILDING.md#how-the-game-updates-itself).

### Full screen on a slow graphics card

The first time you play in full screen at the desktop's own resolution, the
game times itself for four seconds. If it manages fewer than 170 frames a
second on a display wider than 1440 pixels, it switches full screen to 1280
x 800, which an old graphics card draws much faster, and says so in the chat
area. This happens once; it does not happen with vsync on. Change it back
under SETTINGS > VIDEO whenever you like.

## The data folder

Worlds, settings, screenshots, `pack.txt` and your own `mods` folder (see
[MODDING.md](MODDING.md)) live in one folder:

| System | Folder |
| --- | --- |
| Windows | `%APPDATA%\KYTTU\VoxelRust` |
| macOS | `~/Library/Application Support/KYTTU/VoxelRust` |
| Linux | `~/.local/share/KYTTU/VoxelRust` |

The game prints `voxelrust: data in ...` when it starts. `--data DIR` uses
another folder, and makes it if it does not exist.

Earlier versions used `VoxelRust/VoxelRust` in the same place (for example
`~/.local/share/VoxelRust/VoxelRust`). The first time a newer version
starts and finds no settings or worlds in the new folder, it copies the
files of the old one across, once, and prints `voxelrust: brought N files
over from ...`. The old folder is left as it was. Only the files directly in
it are copied, not folders inside it such as `mods`: move those yourself.

## Command-line options

`voxelrust --help` prints this list, then the options that mods add (here
marked "From the Earth mod").

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
| `--earth LAT LON` | From the Earth mod: make the new world real-world terrain starting there. Refused, with a message, when the Earth mod is switched off |
| `--scale M V` | From the Earth mod: with `--earth`, metres per block across and up (default 30 75) |
| `--distance N` | view distance in chunks, 2..48 |
| `--far N` | distant terrain out to N chunks, 0 = off (up to 256) |
| `--threads N` | worker threads, 0 = none (default: one per spare core) |
| `--size WxH` | window size (default 1280x720) |
| `--fullscreen` | start in full screen |
| `--display N` | open on display N (0 is the main one) |
| `--render-scale P` | draw the 3D view at P percent of full resolution (25..100); much faster on old graphics cards |
| `--novsync` | do not wait for the display refresh |
| `--data DIR` | where worlds and settings are kept; the folder is made if it is not there |
| `--log FILE` | also write what the game prints to FILE |
| `--pack FILE` | extra content file (blocks, items, textures); may be given several times |
| `--screenshot FILE` | load the world, save one BMP frame, and exit |
| `--bench N` | load the world, time N frames, and exit |
| `--flypass N` | like `--bench`, but flying straight ahead over the land for N frames; saves four pictures (`flypass-1.bmp` to `flypass-4.bmp` in the data folder) and reports frames spent waiting for terrain. A render test |
| `--run 'CMDS'` | with `--screenshot` / `--bench`: chat commands to carry out first, separated by semicolons, for example `--run '/time night; /spawn wolf 3'` |
| `--write-icon FILE` | save the game's emblem as a PNG and exit |
| `--icon-size N` | with `--write-icon`: its size in pixels (default 256) |
| `--version` | print the version and which build this is, and exit |
| `--no-update` | do not look for a newer version this time |
| `--add-mod PATH` | copy a mod (a folder, a .zip or a pack file) into the data folder's `mods`, and exit |
| `--export FILE` | with `--world NAME`: write that world as one file, and exit |
| `--import FILE` | with `--world NAME`: add a world from such a file, and exit |
| `--pos X Y Z` | start position |
| `--look YAW PITCH` | start direction, in degrees |
| `--time T` | time of day, 0..1 (0.25 = noon) |
| `--hidden` | do not show the window (with `--screenshot` / `--bench`) |
| `--showcase` | with `--screenshot`: lay out every placeable block in front of the camera first (vehicles are not laid out; handy for checking a pack) |
| `--show LIST` | with `--screenshot`: also draw `hud`, `debug`, `pause`, `items`, `title`, `worlds`, `newworld`, `multiplayer`, `addserver`, `dead`, `map`, `atlas` (the world map), `settings:TAB`, `items:TAB`, `third` (camera behind the player), `creatures` (one of each) or `walking` (the same, mid-stride) |

`--show` takes several words joined by commas. Two of them take a value:

- `items:<tab>` opens the creative item list at that tab, named as in the
  list (`items:Eggs`); capitals do not matter.
- `settings:TAB` opens SETTINGS at that tab: `VIDEO`, `AUDIO`, `CONTROLS`,
  `GAME`, `SKIN` or `MODS`.
- `walking`, with `creatures`, shows the line-up mid-stride.

`--version` prints, for example, `voxelrust 0.1.0 (linux-x86_64, commit
5f38cb02e2e3)`. The first word in brackets is the build name (see
[BUILDING.md](BUILDING.md#build-names-and-updating)); the commit is the one
the program was built from, or `dev` for a program built by hand.

`--flypass`, `--bench`, `--run` and `--screenshot` are aids for developers
and pack makers. So is `--exp N`, which `--help` does not list: it switches
off parts of the drawing so their cost can be timed (add up 1 for the alpha
test, 2 for fog, 4 for the light texture, 16 for minimap painting).

A world created with `--world` and no `--mode` is a creative world; one
created from the menu is survival unless you change it. There is no option
for a flat world: make one from the NEW WORLD screen.

## Moving a world to another machine

A whole world can be written as one `.vxworld` file. It holds the world's
dimensions and your pets as well.

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

## Dropping items

Q (it can be rebound in Settings > CONTROLS) tosses one of the held item
ahead of you. It lies on the ground, glinting, for five minutes; walk over it
to pick it up again. In the inventory, items are moved by clicking: left
click picks a stack up or puts it down, right click picks up half or puts
down one.

## SDL2

The Windows archive has `SDL2.dll` beside the program and the Mac apps have
the SDL2 framework inside them, so nothing else need be installed. On Linux
the game uses the system's SDL2 package.
