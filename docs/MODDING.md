# Modding

Everything in the game world is declared in text files: textures, blocks
and what they do (doors, crops, seats, falling blocks), shapes, items, tools,
weapons, food, recipes, ores and wild plants, models, creatures, vehicles,
sounds, dimensions, chat commands and small scripts. The game's own
content is [data/base.txt](../data/base.txt) with
[data/wildlife.txt](../data/wildlife.txt) (built into the program). A **pack** is a file in the same format that is read after it and
may add new things or redefine anything by reusing its id.

The format, keyword by keyword, is in [DATA.md](DATA.md).

## Where packs are read from

In this order, each able to override the ones before:

1. The built-in data.
2. The mods that come with the game (see below), unless switched off.
3. `pack.txt` in the data folder, if it exists. (The game prints
   `voxelrust: data in ...` at start-up; `--data DIR` chooses another folder.)
4. Every mod in `<data folder>/mods/`, unless switched off.
5. Every mod in `./mods/`, a `mods` folder beside where the game was started
   (handy while making one), unless switched off.
6. Every file given with `--pack FILE`, in the order given.

For each file it reads from disk, the game prints `voxelrust: content from
<path>`.

### Mods that come with the game

Three mods are embedded in the program, like the base data. Their text is in
the source tree under `data/packs/`:

| Mod | File | What it holds |
| --- | --- | --- |
| Natural Disasters | `data/packs/disasters/pack.txt` | `feature disasters`, which lets earthquakes, tsunamis and tornadoes happen, and the sounds `quake` and `twister` |
| Earth | `data/packs/earth/pack.txt` | `feature earth`, which offers real-world terrain (see [EARTH.md](EARTH.md)) |
| Land Vehicles | `data/packs/landvehicles/pack.txt` | the buggy, the truck and the kart, their icons and their recipes |

Boats, rail carts, the locomotive and the plane are in the base data, not in
a mod.

The first two show what `feature <name>` is for: the code for disasters and
for Earth worlds is part of the program, and the pack is what switches it
on. With the Disasters mod off, disasters cannot happen and their setting
is not shown; with the Earth mod off, the EARTH terrain choice is not
offered and `--earth` is refused. The program knows only these two feature
names; a pack cannot add new program behaviour by inventing another. See
[DATA.md](DATA.md#features).

### Switching mods on and off

SETTINGS > MODS lists every mod, the three above first and then those found
in the mods folders, each with a switch. A change takes effect at once (the
content is reloaded). The names switched off are kept in `settings.txt` as
`mods_off`, a list separated by commas, for example
`mods_off=disasters,landvehicles`. A mod goes by the name of its folder, or
of its file without `.txt`.

The data folder's own `pack.txt` and files given with `--pack` are not mods
and have no switch.

### The mods folder

To install a mod, put it in a `mods` folder. Nothing else is needed: no
option, no list to edit. A mod is either of:

- a folder with a `pack.txt` in it: `mods/<name>/pack.txt`, with its
  textures, models and included files beside it;
- a single file: `mods/<name>.txt`.

Mods in one folder are read in name order, so a later name can override an
earlier one. (A file called `mods.txt` is skipped.) The folders are looked
at again on every reload, so a mod dropped in while the game is running
appears after `/reload`, and one taken out disappears. A mod can be switched
off without removing it: see
[Switching mods on and off](#switching-mods-on-and-off).

In the source tree, `mods/` is ignored by git, so a mod under development
there is not committed by accident.

A pack can pull in more files with `include`, and refers to textures (`.tga`)
and models (`.bbmodel`) by paths relative to the pack file itself.

Mistakes never stop the game: the bad line is skipped and a message such as
`pack.txt:19: unknown block property (`bogus`)` is printed (use `--log FILE`
to keep them). A `--pack` file that cannot be read at all is an error at
start-up.

## Reloading without restarting

After editing a pack, or adding or removing a mod, either:

- SETTINGS > GAME > **RELOAD CONTENT PACKS NOW**, or
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

Leave `what` out and the line runs for every block, item, creature or
dimension (`when break do /sound chime`). `use` and `useitem` need a `what`.
A block with a `use` script does that instead of opening and shutting
(`toggle`); a block that already opens a screen (`use chest` and so on), a
seat, and food set out keep their own behaviour and the script is not run.
Eating (in survival) and shooting come before `useitem`, so it suits
items that are neither food nor weapons.

```
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
| `not CONDITION` | the condition is false |

There is no `and` or `or`. For "both", put one `/if` inside another:
`/if night then /if survival then /say BEWARE`.

Each branch is **one** command. A `;` ends the `/if` along with the command,
so in `/if night then /heal 2; /say HELLO` the `/say` always runs. To do
several things in a branch, give them a name with `command` and call that.

### Timing and repeating

```
/after SECONDS COMMAND     carry it out later (0 to 3600 seconds)
/repeat N COMMAND          carry it out N times (64 at most)
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
| `/blast [R]` | clears a ball of blocks of radius R (1 to 5; 2 if left out) around the block the player points at; unbreakable blocks stay. Not on a server |

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
conditions cannot be joined with `and` or `or`; and a script can ask only
what the conditions above offer (it cannot look at a block or a creature in
the world).

Compiled Rust mods are not supported: the game has no plug-in interface,
and a pack is only ever data. (The mods that come with the game are no
exception; their `feature` lines switch on code that is already in the
program.)

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

A server (or a game hosted from the menu) sends its packs and mods, and
every texture, model and include they use, to each player on joining. Players need to install nothing, and get their own
packs back when they return to singleplayer. See
[MULTIPLAYER.md](MULTIPLAYER.md#server-hosted-packs).

Keep server packs modest: the whole bundle is sent to every player each time
they join (16 MB at most is accepted).

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

To look at every creature and vehicle model at once:

```sh
voxelrust --pack my-pack.txt --show creatures --screenshot out.bmp
```

## Player skins

SETTINGS > SKIN shows a turning preview of your character.

- **LOOK** steps through the built-in looks: `default`, `clay`, `moss`,
  `slate`, `charcoal`, `snow`, `rose`, `gold` (tints of the default body) and
  `custom`.
- **Your own skin**: a **64 x 64 PNG** in the usual player-skin layout. Type
  the path of the file and click USE THIS FILE. The game copies it into the
  data folder as `skin.png` and selects `custom`. You can also put `skin.png`
  there yourself.

If the file is not a 64 x 64 PNG the game says so and keeps the current look.

In multiplayer your skin is sent to the server and shown to the other
players, as long as its PNG file is no larger than 24,000 bytes; a larger one
is still used on your own screen.

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
- recipes by hand, at a crafting table and in an oven (`recipe`, `craft`,
  `bake`);
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

A bigger pack, "Spells & Magic", lives in `mods/spellsNmagic/` on the
author's machine. That folder is ignored by git, so it is **not in the
repository**. If you have the folder, its `pack.txt` is a worked example of
the newer features together: its own sounds, wands and potions driven by
`when` scripts, chat commands, a dimension reached through portal blocks,
and creatures (a dragon that shoots, a unicorn to tame) with their own
Blockbench models.

### A larger example still: Titanlands

"Titanlands", in `mods/titanlands/`, is an original big-adventure pack and
is likewise **not in the repository**. Its `pack.txt` is written by a
script, `mods/titanlands/gen.py`, from tables, which is a practical way to
make a pack of this size. It has nine ores, each with an ingot, tools, a
sword, a block and bricks; special gear driven by `useitem` scripts; and
four dimensions reached through gate blocks (`when use gate_... do /dim
...; /title ...`). The script also describes giant creatures and dragons to
tame and ride (`tamable`, `rideable`), and writes each one into the pack
only when its model file is present in `mods/titanlands/models/`. It uses
the newer scripting too: `/title`, `/blast`, `/if chance`, and the `ride`
and `night` events.

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
