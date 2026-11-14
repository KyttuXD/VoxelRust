# VoxelRust documentation

| Document | What it covers |
| --- | --- |
| [PLAYING.md](PLAYING.md) | Controls, screens, world types, the minimap, survival and creative, status effects, crafting, farming, baking, furnaces, furniture, bows, doors and beds, caves, vehicles and railways, dimensions, weather, chat colours and commands, settings, command-line options, moving worlds between machines |
| [CREATURES.md](CREATURES.md) | Wildlife, monsters and villagers: what lives where, temperaments, milk, wool and eggs, taming, pets and riding, spawn eggs, young, trading, cave dwellers |
| [IMAGES.md](IMAGES.md) | The picture code: the PNG encoder (palette, row filters, deflate) and decoder, header checks, shrinking, and how skins use them |
| [MULTIPLAYER.md](MULTIPLAYER.md) | Hosting from inside the game or with a server, joining, what is and is not shared, server-hosted packs, the map-data relay |
| [EARTH.md](EARTH.md) | Real-world terrain: data sources and attribution, scales, roads, buildings and fields, the map screen |
| [MODDING.md](MODDING.md) | Content packs, where mods are looked for, where mods are looked for (no mod comes with the game), mods from a server and switching mods on and off, reloading, scripts and pack commands (events, variables, conditions), Blockbench models, player skins |
| [SCRIPT-COMPAT.md](SCRIPT-COMPAT.md) | Which mods run as scripts with no compiled code, what each one needs, the matrix of mod by capability (`tools/script_compat.py`), what stays native (Earth) and what the two protected mods would still lack |
| [SOUND.md](SOUND.md) | How the game sounds: layered grain beds for rain, wind, water, lava, fire and caves, thunder with distance, the variation and limits on repeated sounds, the soft limiter, the AMBIENCE VOLUME setting |
| [SCRIPTING.md](SCRIPTING.md) | Mod scripts: plain `.rs` files, a small subset of Rust, that the game runs while it plays (no compiling): the language, the library, the hooks, the host API (`give`, `spawn`, `set_block`...), limits and errors, `--script-check`, worked examples |
| [SHADERS.md](SHADERS.md) | Post-processing shaders from mods: the `shader` line, writing GLSL ES 1.00 fragment shaders, the uniforms (`u_time`, `u_sun`, `u_daylight`, `u_underwater`...), numbers from scripts, what happens on an error, how the passes are drawn |
| [DATA.md](DATA.md) | The content file format: every keyword, including sounds, dimensions, features, commands and scripts |
| [BUILDING.md](BUILDING.md) | Building for each platform, packaging a release, build names and how the game updates itself, tests, the GitHub workflows, icons |
| [PORTING.md](PORTING.md) | Why the game is built the way it is, old Macs, PowerPC, the machines it has been run on, measurements |
| [DEPLOY.md](DEPLOY.md) | Publishing a release and the website, the public server's quick self-update (check interval, restart warning, rollback), the website's pull-based updater, the deploy settings |
| [WEB.md](WEB.md) | The browser version: building, serving and publishing it, what works there and what does not, how it is put together |

## Mods

No mod comes with the game. The author's mods (disasters, earth, vehicles,
boats, trains, planes, landvehicles, holidays...) are in the `VoxelRust-Mods`
folder, outside this repository; players install mods in `<data folder>/mods`
or get them from the server they join ([MULTIPLAYER.md](MULTIPLAYER.md#mods-from-the-server)).
Only a mod with Rust code needs the program built with it: the opt-in
`native-mods` feature (`VOXELRUST_MODS_DIR` names the folder, see
[BUILDING.md](BUILDING.md#mod-code)). [MODDING.md](MODDING.md) explains how mods are read and
switched on and off.

## Where the code is

| Where | What |
| --- | --- |
| `src/` | The game. It has no Earth, vehicle or disaster code. |
| `src/native.rs` | `struct Native`: the hooks that mod code fills, from start-up to content, options and screens. |
| `src/script/` | The script engine: lexer, parser, interpreter, library, host API and the bridge from the game's hooks (see [SCRIPTING.md](SCRIPTING.md)). |
| `<mods>/<name>/scripts/` | A mod's scripts: plain `.rs` files read at run time, no rebuild. |
| `<mods>/<name>/code/` | (in `VoxelRust-Mods`) A mod's Rust: `mod.rs` and any files beside it. Today there is `mods/earth/code` and `mods/vehicles/code`. |
| `mods/<name>/pack.txt` | The pack for that mod. Its `feature <name>` line switches the code on. |

`build.rs` finds every `<mods>/<name>/code/mod.rs` and compiles it into the
program at build time. Most of that code runs only while a loaded pack
says `feature <name>`. Its command-line options and its reading of a saved
world's kind are asked whether or not it is switched on. It cannot be
loaded at run time, so a change to it needs a rebuild. Pack files are read again on `/reload`.

Vehicles are `vehicle` sections in `mods/boats`, `mods/trains`,
`mods/planes` and `mods/landvehicles`. They need the `vehicles` mod switched
on. Without it, the loader warns that the section needs the mod.

The project overview is in [../README.md](../README.md).
