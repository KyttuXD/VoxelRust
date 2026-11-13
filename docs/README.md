# VoxelRust documentation

| Document | What it covers |
| --- | --- |
| [PLAYING.md](PLAYING.md) | Controls, screens, world types, the minimap, survival and creative, crafting, farming, baking, furnaces, furniture, bows, doors and beds, caves, vehicles and railways, dimensions, weather, chat colours and commands, settings, command-line options, moving worlds between machines |
| [CREATURES.md](CREATURES.md) | Wildlife, monsters and villagers: what lives where, temperaments, milk, wool and eggs, taming, pets and riding, spawn eggs, young, trading, cave dwellers |
| [MULTIPLAYER.md](MULTIPLAYER.md) | Hosting from inside the game or with a server, joining, what is and is not shared, server-hosted packs, the map-data relay |
| [EARTH.md](EARTH.md) | Real-world terrain: data sources and attribution, scales, roads, buildings and fields, the map screen |
| [MODDING.md](MODDING.md) | Content packs, where mods are looked for, the mods that come with the game and switching mods on and off, reloading, scripts and pack commands (events, variables, conditions), Blockbench models, player skins |
| [DATA.md](DATA.md) | The content file format: every keyword, including sounds, dimensions, features, commands and scripts |
| [BUILDING.md](BUILDING.md) | Building for each platform, packaging a release, build names and how the game updates itself, tests, the GitHub workflows, icons |
| [PORTING.md](PORTING.md) | Why the game is built the way it is, old Macs, PowerPC, the machines it has been run on, measurements |
| [WEB.md](WEB.md) | The browser version: building, serving and publishing it, what works there and what does not, how it is put together |

## Mods that come with the game

`mods/SHIPPED` lists them, one folder name a line: disasters, earth,
vehicles, boats, trains, planes, landvehicles and holidays. The release
scripts copy these beside the program (inside the app on macOS), and the
shipped-mod tests are in `mods/tests.rs`. [MODDING.md](MODDING.md) explains how mods are read and
switched on and off.

## Where the code is

| Where | What |
| --- | --- |
| `src/` | The game. It has no Earth, vehicle or disaster code. |
| `src/native.rs` | `struct Native`: the hooks that mod code fills, from start-up to content, options and screens. |
| `mods/<name>/code/` | A mod's Rust: `mod.rs` and any files beside it. Today there is `mods/earth/code` and `mods/vehicles/code`. |
| `mods/<name>/pack.txt` | The pack for that mod. Its `feature <name>` line switches the code on. |

`build.rs` finds every `mods/<name>/code/mod.rs` and compiles it into the
program at build time. Most of that code runs only while a loaded pack
says `feature <name>`. Its command-line options and its reading of a saved
world's kind are asked whether or not it is switched on. It cannot be
loaded at run time, so a change to it needs a rebuild. Pack files are read again on `/reload`.

Vehicles are `vehicle` sections in `mods/boats`, `mods/trains`,
`mods/planes` and `mods/landvehicles`. They need the `vehicles` mod switched
on. Without it, the loader warns that the section needs the mod.

The project overview is in [../README.md](../README.md).
