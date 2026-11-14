# Modding

Everything in the game world is declared in text files: textures, blocks
and what they do (doors, crops, seats, falling blocks), shapes, items, tools,
weapons, food, recipes, ores and wild plants, models, creatures, vehicles,
sounds, music, dimensions, chat commands and small scripts. The game's own
content is the small core, [data/base.txt](../data/base.txt) (built into the program: see
[DATA.md](DATA.md) for what is in it and what is a mod). A **pack** is a file in the same format that is read after it and
may add new things or redefine anything by reusing its id. A mod can also bring
a little Rust of its own, built into the program when it is compiled (see
[Code that mods bring](#code-that-mods-bring)).

The format, keyword by keyword, is in [DATA.md](DATA.md).

## Where packs are read from

In this order, each able to override the ones before:

1. The built-in data.
2. `pack.txt` in the data folder, if it exists. (The game prints
   `voxelrust: data in ...` at start-up; `--data DIR` chooses another folder.)
3. Every mod in the mods folder, unless switched off:
   1. `<data folder>/mods/`: the place for mods you install. The game makes
      it when it starts and puts a `README.txt` in it that says how.
      `--mods-dir DIR` uses the folder `DIR` instead (and then the next one
      is not read).
   2. `./mods/`, a `mods` folder where the game was started (handy while
      making one. The mods are not in the source tree: to run from it, link or copy
      the `VoxelRust-Mods` folder there as `mods`, e.g. `ln -s ../VoxelRust-Mods mods`).
4. Every file given with `--pack FILE`, in the order given.

No mod comes with the game, none sits beside the program and none is built
into it. On a server, the mods the server sends (see below) are used instead
of these.

A mod goes by the name of its folder (or of its file without `.txt`). **A
mod found twice is loaded once**: the first one found, in the order above,
is used and later ones of the same name are left out.

For each file it reads from disk, the game prints `voxelrust: content from
<path>`.

### The author's mods

The game ships no mods. The author keeps the mods below (twenty-two: the eight below, the
creature mods and the mods that hold the content the game used to carry itself;
see [DATA.md](DATA.md): the game's core is small) in a separate repository
(`VoxelRust-Mods`); each is an ordinary folder with a `pack.txt` (and a
`README.txt` saying what it adds and what it needs) that is installed like any
other mod (see above), and can be read, changed, switched off or deleted. Earth and
Vehicles also have Rust code of their own, which works only in a program built
with the `native-mods` feature (see [Code that mods bring](#code-that-mods-bring)).

| Mod | Folder | What it holds |
| --- | --- | --- |
| Natural Disasters | `mods/disasters` | earthquakes, tsunamis and tornadoes, as `episode` lines (see [Episodes](#episodes)), and the `disasters` switch in SETTINGS > GAME, off at first. Pack lines only |
| Earth | `mods/earth` | `feature earth`, which offers real-world terrain (see [EARTH.md](EARTH.md)) |
| Vehicles | `mods/vehicles` | `feature vehicles`: how boats float, carts follow track, cars drive and planes fly, getting in and out, and vehicles kept in step between players. It adds no vehicle of its own |
| Boats | `mods/boats` | the boat and the cargo boat |
| Trains | `mods/trains` | rail track (`tag track`), station furniture (`tag station`), the carts and the locomotive |
| Planes | `mods/planes` | the plane |
| Land Vehicles | `mods/landvehicles` | the buggy, the truck and the kart, their icons and their recipes, and their model, `models/car.bbmodel` |
| Holidays | `mods/holidays` | trees, presents, lights and treats for the year's feasts, four of them big props |
| Colours | `mods/colours` | sixteen colours of concrete, wool, terracotta, glass, boards, chests, beds and couches |
| Woods | `mods/woods` | birch, maple, palm, blossom, acacia and dark oak, and every door, gate, trapdoor, stair, slab and fence of the woods |
| Stonework | `mods/stonework` | marble, slate, granite, basalt, tiles, and stairs, slabs and walls of them and of brick and sandstone |
| Furniture | `mods/furniture` | chairs, shelves, kitchen and bathroom fittings, tables, lanterns and glass panes |
| Farming | `mods/farming` | farmland, crops, seeds and hoes |
| Cooking | `mods/cooking` | ovens, kilns, smokers, campfires, food from flour to cake, and dishes to set out |
| Caves | `mods/caves` | what grows and glows underground: moss, glowcaps, crystals, icicles, magma, vines, cobwebs |
| Explosives | `mods/explosives` | TNT, gunpowder and flint and steel |
| More Tools | `mods/tools-extra` | copper, crystal and ruby tools and swords, the longbow and the crossbow |
| Wildlife | `mods/wildlife` | every animal (`animals.txt`) and 32 newer species (`pack.txt`) |
| Monsters | `mods/monsters` | what hunts at night and in the caves |
| Villagers | `mods/villagers` | the eight trades |
| The Wanderer | `mods/wanderer` | the Wanderer |
| Shaders | `mods/shaders` | four post-processing shaders (soft bloom, warm sunset, underwater ripple, retro dither), chosen in SETTINGS > VIDEO (see [SHADERS.md](SHADERS.md)). Pack lines and `.glsl` files only |

Where they are: in the separate folder `VoxelRust-Mods`, beside the game's
repository (with `SHIPPED` and `tests.rs`, which only the `native-mods` build
uses; see [BUILDING.md](BUILDING.md#mod-code)). Copy the folders you want into
`<data folder>/mods/`. Without them the game still runs, as a bare world of the
core's blocks: no Earth terrain, disasters, vehicles, boats, trains, planes, land
vehicles or holidays, and no creatures, colours, furniture, crops or cooking. The
browser version has no folders and so has none of them.

A mod may use what another mod brings (the villagers trade bread of the cooking mod; the colours
mod's boards need the woods'). Anything it names that is missing is left out with a warning in the
log, and the rest still works. Each README says what its mod needs.

Vehicles are not in the base game (`data/base.txt` has no vehicles and no
rails). Boats, trains, planes and land vehicles are mods that write `vehicle`
sections, and each needs the Vehicles mod switched on. Without it the loader
warns that the section needs the mod `vehicles`, which is switched off or
missing. With Vehicles on, `/vehicle ID` sets a vehicle down 2.5 blocks ahead,
`/board` gets into the nearest one within 6 blocks, and `/vehicles` says how
many there are.

`feature <name>` switches on the code of the mod with that name, and nothing
else. Earth and Vehicles have code, built into the program (see
[Code that mods bring](#code-that-mods-bring)); their `feature` lines are what
switch it on. With the Earth mod off, the EARTH terrain choice is not offered
and `--earth` is refused. With the Vehicles mod off, the vehicle sections are
reported, and the vehicle commands and conditions are not there. With the
Disasters mod off, disasters cannot happen and their setting is not shown.
The Disasters mod has no code: its episodes are pack lines that the program
runs. A pack cannot switch on anything by inventing a feature name; a name
with no code behind it does nothing. See [DATA.md](DATA.md#features).

### Switching mods on and off

SETTINGS > MODS lists every mod found in the mods folder, each with its
name, its version (the `version` line of its pack.txt, if it has one) and a
switch. While you are on a server, the mods the server sent are listed after
them, marked FROM SERVER; those have no switch (they are the server's, for
this visit). A change takes effect at once (the content is reloaded). The names switched off are kept in `settings.txt` as
`mods_off`, a list separated by commas, for example
`mods_off=disasters,landvehicles`. A mod goes by the name of its folder, or
of its file without `.txt`.

The data folder's own `pack.txt` and files given with `--pack` are not mods
and have no switch.

A pack can say two things about its mod, as top-level lines: `version 1.2`
(shown in the list and announced to players) and `client no` (a mod only a
server needs: it is not offered for download unless named in `--serve-mods`).
The game reads nothing else into them.

A pack's `setting` switches (see [Episodes](#episodes)) are saved in
`settings.txt` as `flag_<id>=0` or `flag_<id>=1`. Settings keys the game does
not know are kept there too, for mod code. A world made with a mod's terrain
keeps its kind, so it has that terrain again once the mod is on; while the
mod is off, the terrain is not used.

### The mods folder

To install a mod, put it in a `mods` folder. Nothing else is needed: no
option, no list to edit. A mod is either of:

- a folder with a `pack.txt` in it: `mods/<name>/pack.txt`, with its
  textures, models and included files beside it;
- a single file: `mods/<name>.txt`.

Mods in one folder are read in name order, so a later name can override an
earlier one. (A mod may redefine a block of the core: the farming mod redefines dirt,
the flowers and tall grass, to drop seeds.) (A file called `mods.txt` is skipped.) The folders are looked
at again on every reload, so a mod dropped in while the game is running
appears after `/reload`, and one taken out disappears. A mod can be switched
off without removing it: see
[Switching mods on and off](#switching-mods-on-and-off).

The mods are kept outside the game's repository, in the `VoxelRust-Mods`
folder (its own git repository, beside this one; `VOXELRUST_MODS_DIR` names
another place). A `mods/` folder at the top of the game's source tree is
ignored by git, so a mod under development there is not committed by accident.

A pack can pull in more files with `include`, and refers to textures (`.tga`)
and models (`.bbmodel`) by paths relative to the pack file itself.

A pack can also declare **palettes**, `palette <name> <colours...> [gradient
<dir>]`, and paint textures from them: `texture ruby_ore paint ore palette
ruby`. Colours are `#rrggbb` or `r g b`; a palette has up to six colours with
roles (base, dark, light, accent, highlight, outline) and may spread them over
the picture in flat bands. Palettes of the base game and of mods loaded
earlier can be used by later packs; see
[TEXTURING.md](TEXTURING.md#palettes-and-gradients).

Mistakes never stop the game: the bad line is skipped and a message such as
`pack.txt:19: unknown block property (`bogus`)` is printed (use `--log FILE`
to keep them). A `--pack` file that cannot be read at all is an error at
start-up.

## Reloading without restarting

After editing a pack, or adding or removing a mod, either:

- SETTINGS > GAME > **RELOAD MODS NOW**, or
- type `/reload` in chat (T).

The files are read again and the open world switches to the new content on
the spot. A note says how many blocks and items were loaded, or how many
problems were found (details are in the log). Creatures present at that
moment are removed and new ones appear in time.

This does not work while connected to a server: there the server decides the
content.

## Scripts and commands

A pack can make things happen with two kinds of line. Both are lists of
chat commands separated by `;`, the same commands a player can type (see
[PLAYING.md](PLAYING.md#chat-and-commands)).

```
when <event> [what] do <commands separated by ;>
command <name> <commands separated by ;>
```

### Events

**`when`** runs its commands each time the event happens to the player:

| Event | Happens when | `what` |
| --- | --- | --- |
| `start` | a world is opened | none |
| `every <seconds>` | that many seconds have passed, over and over (half a second at least) | the number of seconds |
| `break <block>` | you break that block | block id |
| `place <block>` | you place that block | block id |
| `use <block>` | you right click that block | block id |
| `useitem <item>` | you right click with that item in hand | item id |
| `eat <item>` | you eat that food from your hand, in survival | item id |
| `hit <entity>` | you hit that creature with a left click (also on the blow that kills it, before `kill`) | entity id |
| `kill <entity>` | you kill that creature, by hand or with a shot | entity id |
| `tame <entity>` | you tame that creature | entity id |
| `ride <entity>` | you get on that creature | entity id |
| `dimension <name>` | you arrive in that dimension | dimension name |
| `night` | night falls | none |
| `day` | day breaks | none |
| `death` | you die | none |
| `wake` | you sleep through the night and wake in the morning | none |
| `effectstart <effect>` | a status effect starts on you, from a source, a mod or `/effect give` (not when it is only made stronger or longer) | effect id |
| `effectend <effect>` | it runs out, is put out by water, or is cleared (and for each one when you respawn after dying) | effect id |
| `episode <id>` | an episode of a pack begins (see [Episodes](#episodes)) | episode id |
| `episode_end <id>` | that episode ends | episode id |
| `tntprimed <block>` | a block of TNT is lit (by fire, lava, an explosion or the flint and steel) | block id |
| `explode` | an explosion happens, before it breaks anything (TNT, an exploding creature, `/blast`, a mod's code) | what set it off: `tnt`, `command`, a creature's id, or `mod<N>` |
| `blockexploded <block>` | an explosion has broken blocks: once for each kind of block it took, after it is over | block id |
| *any name* | a pack raises it with `/fire <event> [what]` | whatever it sends |

The newer events, which fire in the same way:

| Event | Happens when | `what` |
| --- | --- | --- |
| `entityspawn` | a creature appears (by itself, from an egg, from breeding) | entity id |
| `entitydeath` | a creature dies, whatever the cause | entity id |
| `entityhurt` | you strike a creature (it may die of it) | entity id |
| `playerhurt` | you take damage in survival, after armour and mods | the cause: `fall`, `drown`, `hunger`, `block`, `arrow`, `explosion`, `effect`, `command`, `other`, or the id of the creature that bit |
| `respawn` | you come back after dying | none |
| `chat` | you say something (a line not starting with `/`) | its first word |
| `command` | you type a `/command` | its name without the `/` |
| `itempickup` | you pick an item up off the ground | item id |
| `itemdrop` | you drop an item | item id |
| `craft` | you craft something at a table or oven | the output item id |
| `sleep` | you lie down to sleep | none |
| `wake` | you wake | none |
| `weather` | the weather where you are changes | `clear`, `rain`, `snow` or `storm` |
| `join` | another player joins a shared game | their name |
| `leave` | another player leaves a shared game | their name |

Some events have a second name, and a pack may write either:
`blockbreak` = `break`, `blockplace` = `place`, `playerdeath` = `death`,
`nightfall` = `night`, `daybreak` = `day`, `itemuse` = `useitem`.
Creature events are raised by whoever runs the world: in a shared game, the
host (the other players hear `entityhurt` for their own blows only). A mod's
native code can also hear and cancel many of these before they happen (see
[Hooks reference](#hooks-reference)).

A pack hooks into another by raising an event with `/fire`, and the other
answers with its own `when`. `/fire` can be used within a `when` too, but a
chain of events is cut off after four levels, so two packs cannot loop.

Leave `what` out and the line runs for every block, item, creature or
dimension (`when break do /sound chime`). `use` and `useitem` need a `what`.
A block with a `use` script does that instead of opening and shutting
(`toggle`); a block that already opens a screen (`use chest` and so on), a
seat, and food set out keep their own behaviour and the script is not run.
Eating (in survival) and shooting come before `useitem`, so it suits
items that are neither food nor weapons.

```
when useitem tonic do /effect give regeneration 30 2; /effect give speed 30; /take tonic
when effectstart wither do /say &8THE AIR TURNS COLD
when kill spider do /if effect poison then /heal 6
when use arcane_portal do /sound portal; /dim arcane; /say &dYOU STEP THROUGH
when useitem fire_wand do /sound spell; /shoot 10 38 hot_coals
when eat healing_potion do /heal 14
when kill dragon do /give dragon_scale 3
when every 300 do /say &7FIVE MINUTES HAVE PASSED
when night do /title NIGHT FALLS
```

### Pack commands

**`command`** adds a chat command. In its list, `$1` to `$9` stand for the
words typed after the name and `$*` for all of them:

```
command heal /heal 20; /feed 20; /sound sparkle
command summon /spawn $1 $2
```

`/summon wolf 3` then runs `/spawn wolf 3`. The player sees the reply of the
last command in the list. A pack command may call other pack commands, six
deep at most, and one with the name of a built-in command replaces it
(except `/help` and `/reload`).

The commands meant for scripts are `/say`, `/title`, `/sound`, `/heal`,
`/feed`, `/hurt`, `/take`, `/launch`, `/shoot`, `/aim`, `/strike`, `/blast`
and `/dim`, beside `/give`, `/tp`, `/set`, `/spawn`, `/time`, `/mode` and
`/weather`, and the scripting commands below. Text is lowered before it is
run, so ids match; `/say` and `/title` show their text in capitals, and
`/say` takes `&0` to `&f` colour codes.

### Variables

```
/var NAME = N         set it
/var NAME + N         add
/var NAME - N         subtract
/var NAME random N    a random whole number from 0 to N-1
```

A variable holds one whole number. One that has never been set counts as 0.
A world can have 256 of them. They belong to the world and are saved with
it, in `<world>.vars.txt` in the data folder (not on a server, where nothing
of the kind is kept).

Anywhere in a command, `$NAME` is replaced by the variable's value before
the command runs, and `$x`, `$y` and `$z` by the block the player stands in:

```
command where /say &eYOU ARE AT $x $y $z
when death do /var deaths + 1; /say &7YOU HAVE DIED $deaths TIMES
```

Two things to know: `$NAME` is only replaced once the variable has been set
(before that the text stays as written), and because `$x`, `$y` and `$z`
are filled in first, a variable whose name begins with x, y or z cannot be
written with `$`.

### Conditions

```
/if CONDITION then COMMAND
/if CONDITION then COMMAND else COMMAND
```

| Condition | True when |
| --- | --- |
| `NAME OP NUMBER` | the variable compares that way. OP is `=` (or `==`), `!=`, `<`, `>`, `<=` or `>=`, with spaces round it. Instead of a variable, NAME may be `health`, `hunger` (both 0 to 20) or `y` (the height you stand at) |
| `chance N` | N times in a hundred, at random (50 if N is left out) |
| `holding ITEM` | that item is in your hand |
| `has ITEM [N]` | you carry at least N of it (1 if left out) |
| `dim NAME` | you are in that dimension; `dim home` is the world itself |
| `night`, `day` | it is night, or day |
| `creative`, `survival` | the game mode |
| `riding` | you are on a creature or in a vehicle |
| `hurt` | your health is below full |
| `hungry` | your hunger is below full |
| `effect ID [LEVEL]` | that status effect is active on you, at that level or more (1 if left out) |
| `driving` | you are in a vehicle (the Vehicles mod is on); `riding` is true then too |
| `setting ID` | that switch is on in SETTINGS > GAME (a pack's `setting` line) |
| `storm` | a storm is on |
| `rain` | it is raining, not snowing |
| `snow` | it is snowing |
| `coast` | you are near the sea |
| `lowland` | the ground is less than 24 blocks above sea level |
| `biome NAME [NAME...]` | you are in one of those biomes, by name, such as `plains` or `savanna` |
| `not CONDITION` | the condition is false |

There is no `and` or `or`. For "both", put one `/if` inside another:
`/if night then /if survival then /say BEWARE`.

The one place `and` is read is an episode's `if` line (see [Episodes](#episodes)).

Each branch is **one** command. A `;` ends the `/if` along with the command,
so in `/if night then /heal 2; /say HELLO` the `/say` always runs. To do
several things in a branch, give them a name with `command` and call that.

### Timing and repeating

```
/after SECONDS COMMAND     carry it out later (0 to 3600 seconds)
/repeat N COMMAND          carry it out N times (64 at most; fewer when nested)
```

At most 64 commands can be waiting at once. Waiting commands are not saved:
leaving the world drops them. `$NAME` in a delayed command is filled in when
`/after` is run, not when the time is up.

`/if`, `/repeat`, pack commands and `$` replacement count toward the same
limit of six levels of nesting.

### Other commands for scripts

| Command | Effect |
| --- | --- |
| `/take ITEM [N]` | removes N of the item (1 if left out) from the player, if they have it. Pair it with `/if has ITEM N` to charge a price |
| `/title TEXT` | large letters in the middle of the screen for two seconds |
| `/blast [R]` | an explosion of radius R (1 to 5; 2 if left out) round the block the player points at: it clears a rough ball of blocks (unbreakable blocks, liquids and `tag blastproof` blocks stay), hurts and throws what is near, and fires the explosion events below. Not on a server |

### Explosions

TNT and every other explosion (an exploding creature, `/blast`, a mod's
code) go through one function, `explosion::explode` in
[src/game/explosion.rs](../src/game/explosion.rs). An explosion of power P
clears a rough ball of blocks of radius 2P (TNT has power 2.5: a ball about
10 blocks across). Water, lava, unbreakable blocks and blocks marked
`tag blastproof` stay. In survival about a third of what is broken drops as
items (at most 24 a blast) and a broken chest scatters what it held. TNT in
the ball is lit with a fuse of half a second to a second and a half, so a
stack goes off in a chain. A blast that goes off in water breaks no blocks
and hurts about 0.6 as much (`fluid_rules::UNDERWATER_HURT`). Creatures and the player within three times P
blocks are hurt and thrown the less the farther they are (TNT takes 20
health point blank, about 4 at four blocks).

A block is TNT when it has `tag tnt <power>` (see [DATA.md](DATA.md#blocks)).
It is lit by lava or a `tag fire` block (the campfire) beside it, by an
explosion, or by right clicking it with the `flint_and_steel` item. A lit
block leaves the world and burns a four second fuse as a block that flashes
white, then explodes. Lit blocks are not saved with the world.

Scripts hear of it with three events (see the table above): `tntprimed`,
`explode` and `blockexploded`. `blockexploded` is raised once for each kind
of block an explosion broke, not once for every block, and only when some
pack has a `when blockexploded` line.

```
when tntprimed do /say &cFUSE LIT
when explode creeper do /title SSSSS...BOOM
when blockexploded diamond_ore do /give ruby 1
```

Mod code has three hooks for explosions (see
[Code that mods bring](#code-that-mods-bring)):

| Hook | Called | May |
| --- | --- | --- |
| `before_explode(g, &mut ExplosionInfo) -> bool` | before an explosion breaks anything | change `at` and `power`, or return false to cancel it |
| `after_explode(g, &ExplosionInfo, destroyed)` | when it is over; `destroyed` is how many blocks went | |
| `on_prime(g, cell) -> bool` | before a block of TNT is lit | return false to keep it from being lit |

`ExplosionInfo` is `{ at: [f64; 3], power: f32, source: ExplosionSource }`
and `ExplosionSource` is `Tnt`, `Creature(entity index)`, `Command` or
`Mod(number)`. Mod code can set off its own explosion with
`crate::explosion::explode(g, at, power, ExplosionSource::Mod(n))`; the power
is at most 8.

### Episodes

A pack can give the world events of its own. An `episode` line says how one
may start, how long it lasts and what it does while it lasts:

```
episode <id> [every S] [lasts S] [moves SPEED] [funnel] [then ID] [if COND [and COND]...] do <commands>
setting <id> <on|off> <LABEL>
```

- `every S`: it may start on its own, about once every S seconds on average,
  while its `if` conditions hold. Without `every` it starts only by
  `/episode`, or by `then` from another episode.
- `lasts S`: how long it runs, from 1 to 600 seconds (10 if left out).
- `moves SPEED`: it travels that many blocks a second. One that moves starts
  30 blocks away and passes close by.
- `funnel`: it is drawn as a whirling funnel where it is.
- `then ID`: when it ends, episode ID starts, if its conditions hold.
- `if`: every condition must hold for it to start on its own. Here, and only
  here, conditions are joined with `and`.
- `do`: its commands run a few times a second while it lasts.

`setting <id> <on|off> <LABEL>` adds a switch to SETTINGS > GAME, with its
label in capitals. The condition `setting <id>` is true while the switch is
on. The Disasters mod has one, `disasters`, which is off at first. Its three
episodes are an earthquake, a tsunami and a tornado. The tornado is the one
with the most conditions:

```
episode tornado every 120 lasts 26 moves 3.5 funnel if setting disasters and storm and lowland and biome plains savanna do /pull 14 2.2; /strip 3 40; /sound twister
```

The commands an episode can run:

| Command | Effect |
| --- | --- |
| `/episode ID` | begins episode ID now; `/episode stop` ends the one running |
| `/shake S` | rocks the view; S from 0 to 1 (0.35 if left out). It dies away by itself |
| `/pull REACH STRENGTH` | for a moment, drags the player towards the episode and lifts them when close |
| `/strip R N` | tears up to N plants and leaves within R blocks of the episode |
| `/flood PEAK` | while an episode runs, the sea rises up to PEAK blocks (24 at most) over its level near the player, then drains away |

Episodes do not run in a shared game (multiplayer). `/episode`, `/strip` and
`/flood` are refused on a server.

### Worked examples

**A bounty.** Count kills in a variable and pay out on every tenth:

```
when kill zombie do /var slain + 1; /if slain >= 10 then /bounty
command bounty /var slain = 0; /give ruby 1; /title TEN ZOMBIES DOWN
```

**A lucky block.** Roll a number and act on it (`lucky_block` being a block
the pack declares):

```
when break lucky_block do /var roll random 3; /if roll = 0 then /give ruby 2; /if roll = 1 then /spawn chicken 3; /if roll = 2 then /launch 12
```

**A toll gate.** Charge an item to pass, and say something a little later:

```
when use toll_gate do /if has ruby 1 then /pay_toll else /say &cTHE TOLL IS ONE RUBY
command pay_toll /take ruby 1; /tp ~ ~ ~-3; /title SAFE TRAVELS; /after 5 /say &7THE GATE SHUTS BEHIND YOU
```

**Night and day.**

```
when night do /if survival then /say &cMONSTERS ARE OUT
when day do /if hurt then /heal 4
when ride horse do /title GIDDY UP
```

### What this is, and is not

It is an event-and-command language: a fixed list of events, each tied to a
list of commands, with numbered variables, simple arithmetic (set, add,
subtract, random), conditions, delays and counted repeats. That is enough
for counters, quests, random outcomes, prices and timed effects.

It is not a general programming language. There are no text variables,
lists, functions with results, or loops that run until something is true;
conditions in `/if` cannot be joined with `and` or `or`; and a script can ask
only what the conditions above offer, plus what a mod's code adds (it cannot
look at a block or a creature in the world).

A pack is only ever data. Code that a mod brings is compiled into the program
when it is built, not loaded from a pack (see
[Code that mods bring](#code-that-mods-bring)).

On a server the scripts of the server's packs run in each player's own
game, and the commands that are refused on a server (`/time`, `/set`,
`/dim`, `/shoot`, `/aim`, `/strike`, `/blast` and so on) are refused there
too. Variables are per player there and are not saved.

## Worlds survive content changes

Saves record blocks, items, chest contents and vehicles **by name**, not by
number. Adding, removing or reordering content never scrambles a world:

- New blocks simply become available.
- A block whose id no longer exists turns into air when its chunk is next
  loaded; items and vehicles that no longer exist disappear from inventories
  and the world.
- Rotated and flipped states are saved as `id@n`; if a block loses its
  `rotate` or `flip`, old placements fall back to its first state.

Renaming an id counts as removing it and adding a new one.

## Packs on a server

A server (or a game hosted from the menu) sends its packs, and every texture,
model and include they use, to each player on joining, and offers its mods
(whole folders, scripts included) for download. Players need to install
nothing, and get their own packs back when they return to singleplayer.
Downloaded mods are kept in `<data folder>/tmp/mods/<server>/` for that visit
only. See
[MULTIPLAYER.md](MULTIPLAYER.md#server-hosted-packs).

Keep server packs modest: the whole bundle is sent to every player each time
they join (16 MB at most is accepted).

## HD pictures

A mod's `texture <id> file <picture>` line (PNG, JPEG, GIF, BMP or TGA, the path
relative to the `pack.txt`) may name a picture bigger than 16 x 16: a **square, a
multiple of 16 pixels on a side, from 16 to 512** (32, 48, 64, 128, 256, 512...). Blocks
and items use it like any texture (`tex`, `icon`); nothing else in the pack changes. The game
keeps the full picture for a separate HD atlas and the tile's 16 x 16 shrunk version in the
normal one (see [TEXTURING.md](TEXTURING.md#hd-textures) for how it is drawn and what the player's
TEXTURE SIZE setting does).

- A picture of another shape (24 x 24, 32 x 16) is fitted to 16 x 16 and the log says so; one over
  512 on a side, or a file over 2 MiB that is an HD tile, is refused (the log names it and the
  texture is not made).
- Creature and model pictures (`picture <name> file`, a model's own texture) may also be any
  size up to 512 x 512; a model's faces are fractions of its declared texture size, so a picture
  twice as big needs no change to the model.
- A server sends such a mod like any other (a mod up to 64 MB, 96 MB for all of a server's mods,
  a file up to 4 MiB). The receiving player's game checks the declared size of a picture before it
  decodes it and shrinks what the player's TEXTURE SIZE or graphics card cannot hold. A client of an
  older version does not take a mod bigger than its limit.
- The memory the pictures cost raises the game's memory limit (see [MEMORY.md](MEMORY.md)); a
  hundred 512 x 512 tiles are about 140 MB of the HD atlas.
- The `hd-test` mod of the mods repository has a gradient (64), a checker (128), a test card (512)
  and a sign item (128) to try it with. Do not replace the game's own pictures with HD ones: the
  game's textures stay in the house style (see [TEXTURING.md](TEXTURING.md)).

## Blockbench models

Creature, vehicle and player models are [Blockbench](https://www.blockbench.net)
files (`.bbmodel`, generic "free" model format) read directly: cubes, the
bone tree, per-face UVs and the embedded texture. No export or conversion
step is needed.

```
model fox file fox.bbmodel

entity fox
  name Fox
  model fox
  height 0.7
  behavior wander
  biomes forest taiga
```

Conventions: y is up, the front of the model faces -z, feet stand on y = 0.
The model is scaled to the entity's or vehicle's `height`. Bones are animated
by name: `Head`, `Body`, legs (`Leg-FL`..., or `Leg-L1`... for many legs),
arms, `tail` or `tail-a`, `tail-b`..., `wing-l` / `wing-r` and `ear-l` /
`ear-r`. A leg may have a lower half that bends at the knee: a child bone
named `Shin-` plus the same ending as its leg (`Shin-FL` under `Leg-FL`).
An arm may likewise have a `Forearm-` child (`Forearm-L` under `Arm-L`) that
bends at the elbow. The table is in [DATA.md](DATA.md#models).

The game's own models are made by `tools/gen_models.py`, and every one of
them can be used from a pack with `model <id> builtin <name>` or, inside an
entity, simply `model <name>` for those the base data already declares.

Redefining the model `player` replaces every player's body.

A model of a person (it has arms and legs: `Arm-L`/`L-Arm`, `Leg-L`/`L-Thigh`)
also takes the postures: sitting, lying, crouching, swimming, climbing and
the walking bends of the elbows and knees. A `Forearm-` or `Calf-`/`Shin-`
child bone is bent for those, and the torso is one bone called `Body`, or
three called `Pelvis`, `Belly` and `Chest`. Bones that are not named like
those are left to their own animation. Emotes turn any bone by name (see
below).

The bends of the built-in body are plain bones, so a mod body can use them by
naming its bones alike (case does not matter, a side is `L-`/`R-` in front or
`-L`/`-R` behind): the arm is `L-Arm` (upper arm, pivot at the shoulder), its
child `L-Forearm` (pivot at the elbow) and its child `L-Hand` (the wrist); the leg is
`L-Thigh` (the hip), `L-Calf` (the knee; `Shin-L` does as well) and `L-Foot` (the ankle).
The built-in player model (`data/models/player.bbmodel`, `player_slim.bbmodel`)
has them, each limb in three cubes cut along the skin's own regions, so a skin maps the
same as on a one-box limb. A limb without the lower bone is drawn as one straight box, as
before. What the bends do: walking swings the arms against the legs, the elbow folds
10 to 35 degrees on the forward swing and the knee up to 50; sprinting adds a longer
swing, elbows about 90 degrees, high knees and a forward lean of 8 degrees; crouching bends the knees and lowers the body;
swimming strokes with the elbows and kicks with small knee bends; climbing
reaches hand over hand; sitting bends the knees 90 degrees; falling throws the arms out;
standing still breathes with a little arm sway. Other players' gait comes from how fast
their position changes, and villagers or other humanoids that use the player model
move the same way. For pictures: `--show bends:NAME` (with `--screenshot`; names `idle walk
sprint crouchwalk swim climb sit sleep fall ride`, optionally `@PHASE` in radians) shows
the player's own body from outside in that gait.

### Shaders

A mod adds post-processing shaders (a golden evening, bloom, an underwater wobble,
the colours of an old console) with `shader <id> <file> [<file2> [<file3>]]` lines
in its `pack.txt` and plain GLSL ES 1.00 files, usually in a `shaders/` folder of the mod.
The player picks one in SETTINGS > VIDEO > DETAIL AND DISTANCE... > SHADER, scripts can
feed them numbers with `set_shader_uniform`, and one that does not compile is logged and
switched off. The whole format, the uniforms (`u_time`, `u_resolution`, `u_sun`,
`u_daylight`, `u_underwater`, `u_rain`, `u_fov`), the limits and how it is drawn are in
[SHADERS.md](SHADERS.md); the line is in [DATA.md](DATA.md#shaders).

### Emotes

A pack or mod adds emotes to the wheel (B) with `emote` lines and `key` lines
of bone turns over time. The format is in [DATA.md](DATA.md#emotes); the
names of the bones are those of the model (the player's are listed there).
For example, a pack file `emotes.txt` beside `pack.txt`, included with
`include emotes.txt`:

```
emote clap Clap 2 loop
  key 0.0 L-Arm 55 0 -12
  key 0.0 R-Arm 55 0 12
  key 0.0 L-Forearm 60 0 0
  key 0.0 R-Forearm 60 0 0
  key 0.15 L-Arm 55 0 -2
  key 0.15 R-Arm 55 0 2
  key 0.3 L-Arm 55 0 -12
  key 0.3 R-Arm 55 0 12
```

The same emote list is read by every player of a server from the server's
pack, so the emote numbers on the wire (`Msg::Emote`) match.
Emotes are not hooks of the mods' code, but code can start one for the
player by `Session::play_emote(index)` (the index is the place of the emote
in `block::reg().emotes.list`).

To look at every creature model at once:

```sh
voxelrust --pack my-pack.txt --show creatures --screenshot out.bmp
```

### Props: blocks drawn as models

A model can also stand in for a block. Give a block `prop <model> [height] [lift]`
and it is drawn as that model, scaled to that many blocks high (1 if left
out) and raised by `lift` blocks (0 if left out), instead of as boxes:

```
model stone_lantern file models/lantern.bbmodel

block stone_lantern
  name Stone Lantern
  tex stone
  prop stone_lantern 1.1
  rotate
  box 4 0 4 12 16 12
  light 12
craft stone_lantern 1 stone 3 lamp 1
```

(`craft` is a shapeless recipe: the four items go anywhere in the crafting
grid. A pattern bigger than 3 x 3, up to 5 x 5 or 7 x 7, is made at a
crafting bench or a workshop: a block with `use craft 5` or `use craft 7`
opens those grids, and a pack may declare its own benches; see
[Recipes](DATA.md#recipes). A `shaped` line fixes the pattern, as in `shaped stone_lantern 1 ".S."
"SLS" ".S." with S stone L lamp`; see [Recipes](DATA.md#recipes).)

- Give the block at least one `box` line. The boxes are not drawn; they are
  the solid shape, used for collision and for aiming (with none, the block is
  solid as a whole block). For something to walk
  through (a bush, a hanging lamp), give a small box and `solid no`.
- With `rotate`, the model turns to face the way the block was placed.
- The item shows as the model in the inventory and the creative list.
- Everything else is an ordinary block: `seat`, `use chest`, `light`,
  `sound`, `eat`, recipes, and `plant` lines to scatter it in the world.
- The model stands still (bones are not animated), and props are drawn only
  near the player, out to about 44 blocks.

The keyword is described in [DATA.md](DATA.md#props).

#### Big props

A prop can take up more than one block. `fills <wide> <tall>` gives its room:
up to 7 blocks wide and 24 tall, standing on the block's floor and centred on
the block. The rest of that room is hidden solid blocks, the `part` blocks
(`prop_part` in the base data). Placing needs all of the room free, and
breaking or aiming at any part acts on the whole prop.

```
block grand_tree
  name Grand Festive Tree
  box 0 0 0 16 16 16
  prop grand_tree 7
  fills 3 7
```

The base game has `golem_statue` (one wide, three tall). `mods/holidays` has
four big props (a grand tree, a giant snowman, a great pumpkin and a maypole),
with their models made by `tools/gen_props.py`.

## Player skins

SETTINGS > SKIN shows a turning preview of your character.

- **LOOK** steps through the built-in skins: `default`, `explorer`, `miner`,
  `farmer`, `sailor`, `knight`, `mage`, `ranger`, `baker` and `custom`.
  `default` is the skin your **player name** picks: the same name always
  wears the same one of the eight (any capitals count as small letters), and
  so do players in multiplayer whose own skin has not arrived. Change your
  name and the skin changes with it (`default` only). The skins are drawn by
  `tools/entity_textures.py` in the OCD style (see
  [TEXTURING.md](TEXTURING.md#creatures-and-players)); a texture pack can
  replace them.
- **Your own skin**: a square picture in the usual player-skin layout, **64 x
  64** or an HD one: any side that is a multiple of 64, up to **4096 x 4096**
  (128, 192, 256, 512, 1024...; see [IMAGES.md](IMAGES.md) and
  [PLAYING.md](PLAYING.md#skins)). The older 2:1 layout (**64 x 32**, 128 x 64...)
  works too; the game mirrors the arm and leg to make the left ones, at the
  picture's own scale. Click CHOOSE A SKIN FILE... to open the file
  browser (below), pick the picture, and click SELECT (or double click it).
  PNG, JPEG, GIF, BMP and TGA are read; the game saves the skin in the data
  folder as `skin.png` and selects `custom`. You can also put `skin.png`
  there yourself.

If the picture is the wrong size (not square, not a multiple of 64, over
4096) or cannot be read, the game says so on the SKIN tab and keeps the
current look. A skin that is W pixels wide is the 64 x 64 layout with every cell
W / 64 pixels across, so an HD skin lines up on the same models: the player
models declare a 64 x 64 texture and take their coordinates as fractions of
it. A texture pack or a mod may bring HD skins too (`skin_<name>` pictures of
any skin size).

### The file browser

A screen of its own, built into the game, so it works the same on every
system. It shows the folder you are in (path bar on top), its sub-folders
(`>` in front) and the files that fit (pictures here), with a small preview
of the picture under the cursor, its size, and whether the size suits.

- Click a folder (or press Enter / Right) to go in; UP, Backspace or Left
  goes out. Double click a file, or SELECT, to choose it. CANCEL or Escape
  goes back without choosing.
- Shortcut buttons jump to HOME, DOWNLOADS, DESKTOP, PICTURES (those that
  exist), the DATA folder and the top of the disk.
- Up, Down, Page Up, Page Down, Home and End move the selection; typing a
  letter jumps to the next name starting with it. The mouse wheel and the
  scroll bar scroll the list.
- Hidden names (starting with a dot) are not listed. The folder you were in
  is remembered (`picker_dir` in the settings file) and opened next time.
- In a browser page there are no folders to look in, so the list is empty.

For testing, `--show picker:<folder>` opens it at a folder (with `pickfile`
the first file is selected, with `pickgo` as well it is chosen) in a
scripted run.

In multiplayer your skin is sent to the server and shown to the other
players (see [Skins in multiplayer](PLAYING.md#skins)): the game packs it into
a small PNG first (at most 1 MiB; an HD skin that would not fit is sent at a
smaller size), and older games and servers get a 64 x 64 version.

## A worked example

[examples/pack.txt](../examples/pack.txt) is a small pack to copy from:

```sh
voxelrust --pack examples/pack.txt
```

It shows:

- a texture from an image file (`amethyst.tga`) and two made from plain
  colours with grain;
- a glowing block, stairs and a slab made from those textures;
- a custom shape built from boxes (a bench with a back rest) that is placed
  facing the player (`rotate`);
- a thin glowing panel;
- redefining a base block (`lamp`) by reusing its id;
- an item that does more than place one block (a brush);
- a hatch that opens and shuts (`toggle`);
- rubble that falls (`falls`), hot coals that burn (`hurts`) and tar that
  slows (`slow`);
- a crop in three stages (`on`, `grows`, `also`) with its seeds and fruit;
- a wand that turns one block into another and hits hard (`turns`,
  `damage`);
- recipes for the crafting grid, shaped or shapeless, and in an oven or a
  furnace (`shaped`, `craft`, `group`, `bake`, `smelt`);
- an ore in the stone and a wild plant (`ore`, `plant`);
- a creature with traits, variants, young, a glow, something it gives and a
  food it likes;
- a villager with a made-up job who trades.

It does not use sounds, scripts, commands or dimensions; for those see the
examples in [DATA.md](DATA.md) and the packs below.

To load it every time, copy `pack.txt` and `amethyst.tga` into the data
folder, or into a folder of their own under `mods/`:

```
<data folder>/mods/amethyst/pack.txt
<data folder>/mods/amethyst/amethyst.tga
```

### A larger example: Spells & Magic

A bigger pack, "Spells & Magic", lives in `spellsNmagic/` of the author's
mods folder, which is **not in the repository**. If you have the folder, its `pack.txt` is a worked example of
the newer features together: its own sounds, wands and potions driven by
`when` scripts, chat commands, a dimension reached through portal blocks,
and creatures (a dragon that shoots, a unicorn to tame) with their own
Blockbench models.

### A larger example still: Titanlands

"Titanlands", in `titanlands/` of the mods folder, is likewise **not in the
repository**. It is an original pack in the big-adventure style: its names,
content and models were made for VoxelRust, and it is **not a port of any
existing mod**. Its `pack.txt` is written by a script,
`titanlands/gen.py`, from tables, which is a practical way to make a
pack of this size. It has 68 creatures, about 130 blocks and 173 items:
hatch stones, a capture net and cages, three more metal tiers, crops and
fruit trees, ants with nests, dinosaurs, robots, worms, sea creatures, birds,
bosses called with lures, and gadgets. Its dimensions include Crystalis and
Undergrowth, reached through gate blocks (`when use gate_... do /dim ...;
/title ...`). Creatures have their own models in `mods/titanlands/models/`;
the script writes a creature into the pack only when its model file is
there. It uses the newer scripting too: `/title`,
`/blast`, `/if chance`, and the `ride` and `night` events.

### A pack of props: Hearth & Garden

"Hearth & Garden", in `mods/hearthgarden/`, is a decoration pack and is
also **not in the repository**. Its `pack.txt` is written by
`mods/hearthgarden/gen.py` from one table of 64 furniture and garden
pieces, every one a prop: a block drawn
as a model (see [Props](#props-blocks-drawn-as-models)). Some can be sat on
(`seat`: the garden bench, the wing chair, the hay bale), some store things
(`use chest`: the crate, the barrel, the mailbox), some give light or make a
sound, and each has a recipe. `plant` lines scatter boulders, bushes, ferns
and a few others in the world. Like Titanlands' script, it writes a piece
into the pack only when its model file is present in
`mods/hearthgarden/models/`, so without the models the pack is empty.

## Checking a pack

```sh
voxelrust --pack my-pack.txt --showcase --show hud --screenshot out.bmp
```

lays one of every block out in front of the camera and writes a picture.
Problems are printed as the game starts.

In a world, the chat commands help too: `/give <item id>` for anything a
pack adds, `/spawn <creature id>` for a creature (or its spawn egg,
`<creature id>_egg`, in the Eggs tab), `/time night` to see
things glow (see [PLAYING.md](PLAYING.md#chat-and-commands)). `--run` carries
commands out before a screenshot:

```sh
voxelrust --pack my-pack.txt --run '/time night; /spawn crystal_fox 3' --screenshot out.bmp
```

## Adding a mod from elsewhere

Settings > MODS has a box for a path: a folder, a `.zip`, or a lone pack
file. ADD copies it into the data folder's `mods` (unpacking a zip with the
system's `unzip` or `tar`) and reloads. From a terminal:
`voxelrust --add-mod PATH`. A mod that sits one folder down inside its
archive is found too. A mod with code in it (see
[Code that mods bring](#code-that-mods-bring)) is copied the same way, but
its code is used only by a program built with it.

## The food chain

`hunts <ids...>` on a creature names what it preys on; it chases and kills
those and nothing else (a `hunter` with no list goes for anything marked
`prey`). In the base data wolves hunt sheep, rabbits, deer, goats and
poultry; foxes hunt rabbits, poultry and mice; cats hunt mice; alligators
hunt ducks, geese, frogs and turtles; snakes, spiders and frogs have their
own small quarry.

## Holidays

`mods/holidays` comes with the game: trees, presents, lights and treats for
winter, the spooky season, the harvest feast, spring and the new year. All
of it is made by recipe, at a crafting table or in an oven. Four of its
pieces are big props (see [Big props](#big-props)).

## Code that mods bring

A pack's text can only say so much. A mod that needs more carries Rust of its
own in `mods/<name>/code/`, with `mod.rs` as its entry point (Earth also has
`map.rs`, Vehicles `physics.rs`). The build script, [build.rs](../build.rs),
finds every folder of the source tree's `mods` that has a `code/mod.rs` (its
name letters, digits and `_`, starting with a letter) and compiles it into
the program as a module. The game itself names none of them.

Each mod's code offers one table of hooks, `struct Native` in
[src/native.rs](../src/native.rs). The game calls a hook at a fixed point, and
a hook left out is not called. The hooks cover start-up, the content once it
is loaded (a mod checks the lines and sections it claimed), new-world terrain
and the land of a dimension of its own (`dimension_terrain`),
a world being opened, saved or reloaded, each frame, clicks, chat commands,
`/if` conditions, the player killing a creature (`killed`) and being hurt
(`hurt`, which may lessen or cancel the blow and says which creature bit),
explosions (`before_explode`, `after_explode`) and TNT being lit (`on_prime`),
status effects (`effect_apply` may change the seconds and level of an effect
about to be given to the player, or refuse it; `effect_tick` is called every
frame for each effect that is running; `effect_start` and `effect_end` when one
starts or ends),
the events of the [Hooks reference](#hooks-reference) (breaking and placing,
creatures appearing and dying, chat, crafting, using items, the HUD),
messages and state shared between players, drawing, the map,
rows in SETTINGS > GAME, and answers to a server. A mod claims pack lines and
indented sections by name; the game keeps those word for word for the code to
read, and each section also becomes an item.

The effect hooks, in full (see [DATA.md](DATA.md#effects) for what effects are):

```rust
effect_apply: Some(|g, id, secs, level| { /* id: &str, secs: &mut f32, level: &mut i32 */ true }),
effect_tick: Some(|g, id, secs_left, level, dt| { /* every frame, for each running effect */ }),
effect_start: Some(|g, id, level| { /* it was not active before */ }),
effect_end: Some(|g, id| { /* ran out, was put out, or was cleared */ }),
```

`effect_start` is called when an effect starts (not when it is only made longer
or stronger), `effect_end` when it ends, however it ends (death included).
An effect needs no code when the `effect` lines of [DATA.md](DATA.md#effects)
say what it does; the hooks are for what they cannot say. A script has the same
three as `on_effect_start`, `on_effect_tick` (once a second) and `on_effect_end`,
and `apply_effect`, `remove_effect`, `has_effect` and `effect_level` to work with
effects: see [SCRIPTING.md](SCRIPTING.md#effects).

`effect_apply` is asked before an effect is given to the player (by a source,
`/effect give` or `game::effects::apply::give`); it may change `secs` and
`level`, and returns `false` to refuse. It is not asked when the effect would
change nothing (the player already has a stronger one), nor each frame for
blocks that keep an effect going. A mod gives, asks about and clears effects
with `crate::game::effects::apply::{give, clear, clear_all}`, and reads them
from `Session::fx` (`fx.has(id)`, `fx.level(id)`). Damage an effect deals goes
through the `hurt` hook like any other, with no attacker.

The code runs only while a loaded pack says `feature <name>` for that mod, so
switching the mod off in SETTINGS > MODS switches its code off too.

The code is built in, not loaded, and only when asked for: a plain build
compiles in no mod code; `cargo build --features native-mods` compiles in the
code of the mods in `VOXELRUST_MODS_DIR` (default `../VoxelRust-Mods`). To
change it, change the source and rebuild the program. A mod folder with code
copied into the mods folder adds its pack but not its code, since the program
has only the code it was built with; a `feature` line for code that is not
there does nothing. A server's packs can switch on only the code that a
player's build has, because the packs are sent to players and the code is not:
a mod with a `code/mod.rs` is never offered for download.

`mods/SHIPPED` names the mods whose files the `native-mods` test build reads,
and `mods/tests.rs` holds the tests for those mods; nothing is shipped.

Most of what code does can also be done by a [script](SCRIPTING.md), which needs
no build and can come down from a server: Trains, Vehicles and The Man From The
Fog all run as scripts by default (`scripts/*.rs`), with their `code/` kept as the
same logic for the `native-mods` build, which runs the code **instead of** the
scripts (never both). Only what scripts cannot reach still needs code: new
terrain and dimensions, fetching from the network (Earth's map tiles) and screens
of the mod's own. [SCRIPT-COMPAT.md](SCRIPT-COMPAT.md) has the table.

#### Lines and sections for scripts

A pack with scripts keeps its own lines and sections the same way code does,
by claiming them (it has no code to say so) with top-level lines in its `pack.txt`:

```
claims line manfromthefog      # lines "manfromthefog <key> <value>" are kept for scripts
claims section vehicle         # sections "vehicle <id>" with their indented lines are kept
```

A claim holds for every pack read in the same loading, in whatever order, so the
Boats pack can write `vehicle` sections for the script of the Vehicles pack. The
words the game reads itself cannot be claimed. A claimed section also becomes an
item with the section's id, as for code. Scripts read them with `pack_lines`,
`pack_sections`, `pack_prop` and `pack_props` (see [SCRIPTING.md](SCRIPTING.md#more-host-calls)).

### Scripts that mods bring

A mod may carry scripts, `.rs` files in its folder (for the runtime script
engine in `src/script/`, which is separate from the pack text). Unlike
compiled code they can be downloaded from a server. The game hands every
downloaded mod that has scripts to a script host through
`content::mod_runtime` (`ScriptHost::load_mod(name, version, files)` when the
mod arrives and `unload_all()` when the player leaves the server); the host
registers itself with `mod_runtime::set_host`. With no host, scripts are
downloaded and kept but not run.

## Hooks reference

Two layers react to what happens in the game. **Scripts** are `when` lines in
a pack's text (see [Events](#events)): they run after the thing happened and
cannot stop it. **Native hooks** are optional functions in a mod's Rust code
(the `on_...` fields of `struct Native` in [src/native.rs](../src/native.rs)):
they run before the thing happens where it makes sense, and may cancel or
change it. The game raises both from the same places, and every script event is
also passed to the mod's `on_event` hook.

| Event (script) | Fires when | `what` | Native hook that can cancel it | Native signature |
| --- | --- | --- | --- | --- |
| `break` (`blockbreak`) | the player breaks a block | block id | `on_block_break`: `false` keeps the block | `fn(&mut Session, cell: [i32; 3], Block) -> bool` |
| `place` (`blockplace`) | the player places a block | block id | `on_block_place`: `false` stops it | `fn(&mut Session, cell: [i32; 3], Block) -> bool` |
| `entityspawn` | a creature appears | entity id | `on_entity_spawn`: `false` removes it again | `fn(&mut Session, kind: usize) -> bool` |
| `entitydeath` | a creature dies | entity id | no, `on_entity_death` after | `fn(&mut Session, kind: usize, at: [f64; 3])` |
| (world generation) | a chunk was just generated (not loaded from a save) | | `chunk_generated`: place structures in it (4096 writes a call, a couple of chunks a frame, only for whoever runs the world) | `fn(&mut Session, cx: i32, cz: i32, seed: u32)` |
| (any placement) | a block was placed: `player`, `script`, `remote`, `fall`, `fluid` or `other` (not air, a frame later, two hooks deep at most) | | `on_block_placed` | `fn(&mut Session, cell: [i32; 3], Block, cause: &str)` |
| (creatures) | a creature died, with who killed it (a player's name, a creature's id, `lava`, `script`, `other`) | | `on_creature_death` | `fn(&mut Session, id: u32, kind: usize, at: [f64; 3], killer: &str)` |
| (creatures) | a creature left the world: `killed`, `despawned`, `unloaded`, `scripted` | | `on_creature_removed` | `fn(&mut Session, id: u32, kind: usize, reason: &str)` |
| (creatures) | the player right-clicked a creature | | `on_creature_interact`: `false` cancels the game's handling | `fn(&mut Session, id: u32, kind: usize, item: Option<usize>) -> bool` |
| (creatures) | a night creature would burn in daylight | | `on_creature_sunburn`: `false` lets it live (asked again after 20 seconds) | `fn(&mut Session, id: u32, kind: usize) -> bool` |
| `playerhurt` | the player takes damage | cause | `on_player_hurt`: may change `amount`; `false` or 0 cancels | `fn(&mut Session, amount: &mut i32, cause: &str) -> bool` |
| `chat`, `command` | the player types a line | first word / command name | `on_chat`: `false` swallows the line (commands too) | `fn(&mut Session, line: &str) -> bool` |
| `useitem` (`itemuse`) | the player right clicks with an item that has a script | item id | `on_item_use`: asked for any held item, once per press; `false` stops the use | `fn(&mut Session, item: usize) -> bool` |
| `craft` | the player crafts | output item id | no, `on_craft` after | `fn(&mut Session, out: usize)` |
| (armour) | a worn piece was put on, taken off, set by code or broke | (none) | no, `on_armor_change` after | `fn(&mut Session, slot: usize)` (0 head .. 3 feet) |
| `itempickup` | the player is about to pick up a dropped item | item id | `on_item_pickup`: `false` leaves it lying (offered again a second later) | `fn(&mut Session, item: usize, count: u16) -> bool` |
| `every N` | every N seconds | seconds | no, `on_tick` runs every frame | `fn(&mut Session, dt: f32)` |
| (every event) | any event above or `/fire` | the event's | `on_event` hears it, cannot cancel | `fn(&mut Session, name: &str, what: &str)` |
| `start` | a world is opened | none | `on_world_load` once the world is open | `fn(&mut Session)` |
| (saving) | the world was saved | | `on_world_save` after the files are written | `fn(&mut Session)` |
| (each frame) | the HUD is drawn in play | | `on_draw_hud` | `fn(&Session, &mut dyn HudSink)` |
| (a key binding) | a mod's key binding goes down or up | | `on_key` (see [Controls, HUD and screens](#controls-hud-and-screens)) | `fn(&mut Session, id: &str, down: bool)` |
| (each frame) | a frame of play | | `on_hud`: draw through the checked API | `fn(&Session, &mut ModApi)` |
| (a mod's screen) | the player used a screen the mod opened | | `on_ui` | `fn(&mut Session, &mut ModApi, &UiEvent)` |
| `respawn`, `sleep`, `wake`, `weather`, `join`, `leave`, `itemdrop`, `eat`, `hit`, `kill`, `tame`, `ride`, `dimension`, `night`, `day`, `death`, `episode`, `episode_end` | see [Events](#events) | see there | no hook of their own: use `on_event` | |
| `explode`, `blockexploded`, `tntprimed` | an explosion happens, breaks blocks, or TNT is lit | see [Explosions](#explosions) | `before_explode`: `false` cancels; `after_explode` and `on_prime` after | `fn(&mut Session, &mut ExplosionInfo) -> bool` |
| `effectstart`, `effectend`, `wake` | a status effect starts or ends on the player; the player wakes | effect id | `effect_apply`: `false` refuses it; `effect_tick` every frame per running effect; `effect_start`, `effect_end` when it starts and ends (scripts: `on_effect_start`, `on_effect_tick`, `on_effect_end`) | `fn(&mut Session, id: &str, secs: &mut f32, level: &mut i32) -> bool` |

Older hooks (`update`, `interact`, `killed`, `hit`, `hurt`, `open`, `save` and so
on) are described in `src/native.rs` and work as before. `open` and `save` also
run for each dimension; `on_world_load` and `on_world_save` run once per world.

How cancelling works: the mods whose code is switched on are asked in the
order of the mods list; the first to return `false` cancels, and the ones
after it are not asked. A hook that is `None` costs nothing; and when no mod
has any of these hooks, each call site costs one test of a flag. Hooks that
cannot cancel are told after the fact. A hook may call anything on `Session`;
events it raises with `fire` are not passed back to its own `on_event`. A
cancelled break or place leaves the world as it was (digging starts over);
the script event is then not fired.

`on_draw_hud` (the older, unchecked way) gets a `HudSink` with `size()`, `scale()`, `rect(x, y, w, h,
colour)`, `text(x, y, scale, text, colour)` and `text_width(text, scale)`.
Positions are screen pixels; multiply sizes by `scale()`.

A mod's code states which interface it was written for with
`api: API_VERSION` in its `Native`.

### Player bodies

`body` picks the model a player is drawn with: `fn(&Session, player: u16) ->
Option<String>`. `player` is the player's number in a shared game (0 when
playing alone, for the local player); the hook is asked for the local player
and for every remote player drawn. Return the model id (`player`,
`player_slim`, or a model the mod brings), or `None` to keep the game's own
choice from the arms setting. It is generic: nothing in the game knows about
any particular mod's bodies. A mod that does not set `body` (or returns
`None`) gets exactly the game's behaviour. Hands: a body other than `player`
and `player_slim` has no hand of its own, so it uses the hand for the arms.

The game does not send the choice between players. A mod keeps its own state
in its `Session` ext and sends it from its `share` hook with `Session::send_ext`
on the mod channel, keep set, under a key that names the player, for example
`body <own player number>` (the server keeps one entry per key and replays
them to players who join later, so one shared key would overwrite every
player's choice). It reads the others with its `message` hook, which gets the
text and no sender, so the text carries the player number too. The server does
not forget a player who leaves: the mod should ignore entries of players who
are no longer in the game.
Skins for a mod's body come from its `picture skin_<name>` lines, the same as
the built-in skins.

### Stability

`native::API_VERSION` (now 1) is the version of the hook interface. New hooks
and new events do not change it: a mod written for version 1 keeps working
when the game gains more. It goes up when a hook is removed, renamed or its
signature or meaning changes, and the notes for that release say what to do.
Code that says `api` higher than the game's is switched off (not called),
rather than called with the wrong idea of a hook. The interface is young:
until version 2, treat the signatures of the newer `on_...` hooks as likely to
grow by an argument, and prefer `..Native::NONE` so that new fields do not
break your code. Event names in scripts are kept for good: an event is never
renamed without leaving the old name as an alias.

### Armour, wear, item data and attributes from native code

A mod's code reaches the same things the script calls do, directly on the
`Session`: `g.inv.worn` (the four worn slots), `g.set_worn(slot, stack)`,
`g.put_on_armor(pack_slot)`, `g.take_off_armor(slot)`, `g.armor_points()`
(defense and toughness), `g.wear_held(uses, audio)`, `g.slot_mut(n)` (a stack
by the slot numbers scripts use), `g.attrs` (`game::gear::attributes`: base and
modifiers of `max_health`, `walk_speed`, `jump_height`, `armor`, `damage`) and
`g.max_health()`. A `Stack` has `wear` (uses spent) and `data` (a handle to its
item data map, see `game::gear::item_data`: `with_key`, `get`, `without_key`);
`Stack::new(item, count)` makes a plain one and `s.with_count(n)` changes only
the count. Stacks merge only with `same_kind`. The hook `on_armor_change` of
`Native` is called after a piece changes. Items have `x.armor`, `x.durability`
and `x.tags` in the registry. See [SCRIPTING.md](SCRIPTING.md#armour-wear-item-data-and-tags).

### Example

[examples/hooks](../examples/hooks) is a small mod that protects a block with
`on_block_break`, softens falls with `on_player_hurt` and draws the player's
health with `on_draw_hud`, plus a few `when` scripts for the new events.
Copy it to `mods/hooksdemo/` and rebuild.

### Effects for mods (items, blasts, blocks, particles, quads, skies)

What scripts do with `drop_item`, `explode` with flags, `spawn_falling_block`,
`spawn_particles`, `quad_*` and `set_dimension_sky` (see
[SCRIPTING.md](SCRIPTING.md#effects-items-explosions-blocks-particles-quads-skies)) is plain
functions of the game that native code calls too, with the same limits:

| What | Function (in `src/game/fx/`) |
| --- | --- |
| a dropped item with a velocity | `drops::spawn(g, pos, vel, item, count) -> bool` |
| an explosion with flags; answers the blocks destroyed | `explosion::explode_with(g, at, power, source, blast::Flags) -> usize` (`blast::run` for the limits scripts have) |
| a flying block | `falling::launch(&mut fx::state(g).falling, block, pos, vel) -> bool` |
| particles | `particles::spawn(g, &particles::Request { .. }) -> u32` (they are `Session::primed.sparks`) |
| translucent quads | `fx::state(g).quads.create(quads::FxQuad { .. })`, `move_to`, `tint`, `remove`, `remove_mod` (name your quads by your mod's name; call `remove_mod` on world close) |
| a dimension's sky | `sky::set(g, dim, key, value)`; read with `sky::current(g)` |
| the pickup hook | `Native::on_item_pickup: fn(&mut Session, item: usize, count: u16) -> bool` (`false` leaves the item lying) |

The pack's own sky properties for a dimension are in [DATA.md](DATA.md#the-sky-of-a-dimension).

### Controls, HUD and screens

A mod can put its own controls, HUD and screens in the game, from code
(`Native`) or from a script ([SCRIPTING.md](SCRIPTING.md#controls-hud-and-screens),
which has the same calls under other names). Both go through one checked
handle, `ModApi` ([src/client/modui/api.rs](../src/client/modui/api.rs)):
every string is cut, every count has a cap, NaN and huge numbers are brought
into range, and a call over a cap just returns `false`. Nothing blocks the frame.

| `Native` field | What it is |
| --- | --- |
| `bindings: &'static [KeyDecl]` | key bindings: `KeyDecl { id, label, group, key }` (`key` is an SDL scancode; `binds::key_scancode("g")` is 10). They are listed in SETTINGS > CONTROLS > KEY BINDINGS under a heading with the mod's name (and the group), can be rebound, and are saved as `modkey_<mod>.<id>` in `settings.txt`. |
| `on_key: fn(&mut Session, id: &str, down: bool)` | a binding of this mod went down or up while playing (not while typing or in a menu; a release is always passed on). |
| `on_hud: fn(&Session, &mut ModApi)` | once a frame while playing: draw with `api.hud_text`, `hud_rect`, `hud_outline`, `hud_item`, `hud_block`, `hud_bar` (anchor names `top-left`... `bottom-right`, distances in interface pixels, a colour as `0xRRGGBB` or `0xAARRGGBB`), keep lines in the slots with `hud_set` (`above_hotbar`, `beside_hearts`, `top_left_list`, `under_crosshair`), show `toast` and `action_bar`. At most 512 drawing commands a frame in all (256 for one mod); the rest return `false`. |
| `on_ui: fn(&mut Session, &mut ModApi, &UiEvent)` | the player used a screen this mod opened with `api.ui_open(name, title)` and filled with `ui_text`, `ui_button`, `ui_toggle`, `ui_stepper`, `ui_input`, `ui_list`: a click, a change, or the screen closing (`UiEvent`, `UiKind`). |
| `switches: &'static [SwitchDecl]` | rows for SETTINGS > GAME > MOD SWITCHES AND RELOAD: `SwitchDecl { id, label, default }`. Read one with `modui::keys::switch_on(g, NAME, id)`. |

Reading the player for a HUD needs no new call: the hooks have the `Session`
(`g.health`, `g.hunger`, `g.air`, `g.player.pos`, `g.held()`, `g.time`), and
`modui::info::read(g, fps)` collects them (with the biome and a compass
heading) into a `PlayerInfo`. Draw with the `ModApi` and not with the older
`on_draw_hud` when you want the scaling, clipping and caps done for you.

The list of key bindings **scrolls** (mouse wheel and a bar) when it is longer
than the page: the game's own bindings come first, then each mod's under its
name. Two bindings on one key are outlined in red; both fire. The same goes for
the mod switches page. Do not call `with_ui` (or anything that calls a script)
from inside these hooks: use the `ModApi` you are given. The example in
[examples/hooks](../examples/hooks) has a binding that shows or hides a line
beside the hearts.

### Giant creatures, model control and messages

Native code reaches the same limits and the same shared-game sending as scripts
(`set_creature_size` and friends in [SCRIPTING.md](SCRIPTING.md#giants-model-control-and-state)); it
calls them on `g.mobs` (a `Mobs`, from `game/creature_api.rs`), naming a
creature by its stable `id`:

| Call | What it does |
| --- | --- |
| `set_size(id, width, height) -> bool` | the creature's own size, kept to 0.1..32 wide and 0.1..64 tall (blocks); it collides and is drawn at it |
| `set_health(id, hp, max_hp) -> bool` | health and its limit (the limit 1..1,000,000; the health 0..limit) |
| `set_bone(id, bone_name, [rx, ry, rz], [ox, oy, oz], scale) -> BoneResult` | one bone of the model, on top of the animation (degrees about its pivot, model units, uniform scale); `UnknownBone` is logged once and ignored; 64 bones a creature, 256 creatures |
| `clear_bones(id) -> bool` | |
| `set_orientation(id, pitch, roll) -> bool` | tilt the whole model about its centre, degrees |
| `set_state(id, text) -> bool`, `state_of(id) -> &str` | a text of up to 64 bytes on a creature |
| `bone_names.names(model)` and `creature_api::model_bone_names(&mut mobs, id)` | the bone names of a model |

A creature past the old size limits (6 tall, 3 wide) is "big": it is found by walking straight
instead of a path search, steps up a quarter of its width, tests its collisions only through the
slab it just moved into, and is kept alive and drawn from `creature_size::keep_extra` blocks
further off. These are plain changes to the host's own state: in a shared game only the player who
runs the creatures should call `set_size`, `set_health` and `set_state`, which the snapshot then
sends to the others (bone overrides, tilt and state travel with it). The renderer reads a
creature's overrides with `Mob::bone_view()` and draws with `Renderer::draw_model_with`.

Messages: `g.send_mod_message(mod_name, channel, to, text) -> bool` (`to` is a player name, or `None`
for everyone else) sends to the same mod's scripts and native code on the other machines; they
arrive in the script's `on_message(channel, from, text)` and in the `Native` field
`on_message: Option<fn(&mut Session, channel: &str, from: &str, text: &str)>`. At most 8 a second a
player, 512 bytes of text without control characters, 64 bytes of mod name and channel together; the
server drops what breaks these (see `net/modmsg.rs`). Existing hooks are unchanged and `Native::NONE`
has `on_message: None`.

### Adding an event or a hook

A new event is one row in `REGISTRY` in
[src/game/events.rs](../src/game/events.rs), one `Session::fire` call at the
right place, and one row in the tables above. A new cancelling hook is a
field in `Native` (and in `Native::NONE` and `hooks::has_any`) and a small
dispatcher in [src/game/hooks.rs](../src/game/hooks.rs) built on `hooks::allow`.
If scripts should hear it too, the dispatcher also calls a function in
[src/script/bridge.rs](../src/script/bridge.rs), and the hook gets a row in
`script::HOOKS` and in [SCRIPTING.md](SCRIPTING.md) (a test checks the docs list
every hook).

## Scripts: code without compiling

A mod that needs logic but not speed does not have to be built into the
program: it can carry plain `.rs` files in `scripts/` (a small subset of Rust)
that the game reads and runs. A script defines functions named after the
hooks it wants (`on_block_break`, `on_tick`, `on_entity_death`, `on_command`...)
and may call a fixed set of host functions (`give`, `say`, `spawn`,
`set_block`, `explode`, `effect`, `run_command`...). The scripts are one more
listener beside the native hooks above: the same dispatchers in
[src/game/hooks.rs](../src/game/hooks.rs) call them after the native code, a
script may cancel what a cancelling hook may cancel, and with no scripts loaded
a call site costs the same flag test. Scripts run in a sandbox (no files, no
network, a budget of steps and memory for every call; an error switches the
script off and is logged with `file:line`).
The language, the library, every hook and every host function, and
worked examples are in [SCRIPTING.md](SCRIPTING.md); `voxelrust --script-check
FILE` checks a script without starting the game.

### Quality-of-life script mods

Nine small mods in the mods repository are plain script mods (one `.rs`
file each, nothing to build). Each has a README.txt; their commands are:

| Mod | What it does |
| --- | --- |
| `waypoints` | `/wp add NAME`, `/wp del NAME`, `/wp list`, `/wp NAME`: named places with distance, compass direction and where to turn |
| `death_marker` | remembers where you died; `/death` shows the way back, `/death hud` keeps it on the screen |
| `light_warning` | a notice when the light at your feet is low (`/lightwarn N`, `off`, `on`) |
| `sort_inventory` | `/sort` merges stacks and orders the pack, `/sort all` includes the hotbar |
| `quick_stack` | `/stack` puts what the nearest chest already holds into it, `/stack all` for every chest within 6 blocks |
| `coordinates_hud` | XYZ, facing and biome on the screen (`/coords`, `off`, `on`, `long`) |
| `clock_hud` | the time of day and the minutes to night on the screen (`/clock`, `off`, `on`, `12`, `24`) |
| `tree_felling` | breaking a log with an axe brings down the tree, at most 64 logs; sneak to chop one (`/treefelling off`, `on`) |
| `veinminer` | sneak and break an ore with a pickaxe to mine the whole vein, at most 40 blocks (`/veinminer off`, `on`) |

They use the host calls `heading`, `biome`, `block_light`, `block_drop`,
`survival`, `sneaking`, `hud`, `slot_item`, `slot_count`, `move_slot` and
`stack_to_chest` that were added for them (see [SCRIPTING.md](SCRIPTING.md)).
The game's tests (`src/script/tests_qol.rs`) read and run every one of these
scripts against a mock host when the mods folder is there.

## What mods cannot replace

A mod's code is called only at the hooks in `src/native.rs`. Anything those
hooks do not reach stays the game's own. A pack alone cannot add a kind of
terrain, a vehicle's physics or an effect that does more than the `effect` lines
of [DATA.md](DATA.md#effects) cover (damage, healing, speed, jump, melee
damage, hunger, absorption, night vision): those need code, and
code needs a rebuild. What a pack can do is run the episodes and scripts
described above.

### Sound and music from native code

A mod's code plays sounds and music through `crate::native::sound`
([src/game/mod_sound.rs](../src/game/mod_sound.rs)), with the session it is
called with and its own name: `sound::play(g, "<mod>", "<sound>", volume, pitch, pan,
at, looping)` returns a handle (0: none) for `sound::stop`, `fade`, `set` and
`is_playing`; `sound::play_music`, `stop_music`, `set_music_volume` and
`music_playing` drive the world's one music channel. The sounds are found, limited
and mixed exactly as a script's are (see [SCRIPTING.md](SCRIPTING.md#sound-and-music)
and [SOUND.md](SOUND.md)); calls queue and the game carries them out once a frame.
`audio.play_named` still plays a sound of the data files at once, with the game's own
variation, from the hooks that are handed the `Audio`.

## Memory

Long sounds and model files are read again from their files when they are
needed, and let go when memory is short, so a mod can bring many of them: see
[MEMORY.md](MEMORY.md). The files must stay in place while the game runs.
