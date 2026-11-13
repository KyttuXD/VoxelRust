# Modding

Everything in the game world is declared in text files: textures, blocks
and what they do (doors, crops, falling blocks), shapes, items, tools, food,
recipes, ores and wild plants, models, creatures and vehicles. The game's own
content is [data/base.txt](../data/base.txt) with
[data/wildlife.txt](../data/wildlife.txt) (built into the program). A **pack** is a file in the same format that is read after it and
may add new things or redefine anything by reusing its id.

The format, keyword by keyword, is in [DATA.md](DATA.md).

## Where packs are read from

In this order, each able to override the ones before:

1. The built-in data.
2. `pack.txt` in the data folder, if it exists. (The game prints
   `voxelrust: data in ...` at start-up; `--data DIR` chooses another folder.)
3. Every mod in `<data folder>/mods/`.
4. Every mod in `./mods/`, a `mods` folder beside where the game was started
   (handy while making one).
5. Every file given with `--pack FILE`, in the order given.

For each file it reads, the game prints `voxelrust: content from <path>`.

### The mods folder

To install a mod, put it in a `mods` folder. Nothing else is needed: no
option, no list to edit. A mod is either of:

- a folder with a `pack.txt` in it: `mods/<name>/pack.txt`, with its
  textures, models and included files beside it;
- a single file: `mods/<name>.txt`.

Mods in one folder are read in name order, so a later name can override an
earlier one. (A file called `mods.txt` is skipped.) The folders are looked
at again on every reload, so a mod dropped in while the game is running
appears after `/reload`, and one taken out disappears.

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
`tail` or `tail-a`, `tail-b`..., `wing-l` / `wing-r` and `ear-l` / `ear-r`.
The table is in [DATA.md](DATA.md#models).

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

To load it every time, copy `pack.txt` and `amethyst.tga` into the data
folder, or into a folder of their own under `mods/`:

```
<data folder>/mods/amethyst/pack.txt
<data folder>/mods/amethyst/amethyst.tga
```

## Checking a pack

```sh
voxelrust --pack my-pack.txt --showcase --show hud --screenshot out.bmp
```

lays one of every block out in front of the camera and writes a picture.
Problems are printed as the game starts.

In a world, the chat commands help too: `/give <item id>` for anything a
pack adds, `/spawn <creature id>` for a creature, `/time night` to see
things glow (see [PLAYING.md](PLAYING.md#chat-and-commands)). `--run` carries
commands out before a screenshot:

```sh
voxelrust --pack my-pack.txt --run '/time night; /spawn crystal_fox 3' --screenshot out.bmp
```
