# VoxelRust

A voxel sandbox written in Rust that runs on old machines as well as new
ones. It has been run on a 2008 Mac Pro under Mac OS X 10.6.8, on Linux, and
(under Wine) as a Windows program.

It is an original game, not a clone of an existing one: its own blocks, its
own procedurally painted textures and its own terrain generator.

## What is in it

- **Worlds.** Endless invented terrain from a seed: ten biomes (plains,
  forest, desert, tundra, taiga, mountain, swamp, savanna, jungle, badlands),
  six kinds of tree, oceans, ores, and seven kinds of cave (mushroom,
  crystal, lush, frozen, magma, webbed and plain) each with its own blocks
  and creatures. Or **the real Earth**: heights from public elevation tiles;
  water, forests, sand, ice, asphalt roads with painted lines, hollow
  buildings with floors, stairs and furniture, and fields of crops from
  OpenStreetMap; and an in-game map (M) to look around and travel anywhere.
- **Survival and creative modes.** Survival has health, hunger, falling,
  drowning, mining times with cracks spreading over the block, tools, hoes
  and swords in six materials, drops, crafting by hand and at a crafting
  table, furnaces, bows, chests, and monsters at night and in caves. Creative
  has flying (press jump twice), instant building, area tools, and an item
  list with picture tabs and block icons drawn as isometric cubes.
- **Farming and cooking.** Hoes till soil; wheat, carrots, potatoes and
  tomatoes grow in four stages; an oven bakes bread, pies, stew and three
  kinds of pizza.
- **About 410 blocks.** Sixteen colours of concrete, wool, terracotta,
  glass, beds, couches and armchairs; six woods with five kinds of door,
  trapdoors and gates that open; furniture to sit on, store things in and
  cook at; food set out on plates; stone kinds; stairs, slabs, walls and
  fences that join up; lamps; chests; sand and gravel that fall; animated
  water; custom shapes.
- **Creatures.** 75 kinds: farm animals that give milk, wool and eggs and
  have young, animals you can tame, hunters and prey, birds and fish, night
  monsters (skeleton archers among them), cave dwellers, and villagers who
  trade, farm, build and stand guard. See
  [docs/CREATURES.md](docs/CREATURES.md).
- **Vehicles and railways.** Boats, a buggy, a plane and rail carts, some
  with storage. Speed, slow and station track; chains that join carts into
  trains; a locomotive that runs by itself, stops at stations and shuttles
  along its line.
- **Dimensions.** Other worlds inside each world (`sky`, `flatland`, `void`,
  `mirror`), reached with `/dim`; packs can add more.
- **Sound.** Every sound is made from a line of numbers in the data files,
  so packs can retune them or add their own. Optional subtitles name what
  you hear.
- **Weather.** Regional rain, snow and thunderstorms; optional earthquakes,
  tsunamis and tornadoes.
- **Multiplayer.** HOST A GAME from the menu runs a server inside the game
  while you play; `voxelrust --server` runs one on its own. Players join
  from the title screen. Positions, edits, chat, time, skins, creatures,
  vehicles and chest contents are shared, and the server sends its content
  packs to every player.
- **Modding.** Textures, blocks (including doors, crops, seats and falling
  blocks), shapes, items, tools, weapons, food, recipes, ores, plants,
  models, creatures, vehicles, sounds, dimensions, chat commands and small
  scripts (`when <event> do <commands>`) are declared in text files. Drop a
  mod into the
  `mods` folder and it loads; `/reload` picks up changes without
  restarting. Blockbench `.bbmodel` files are read directly. Player skins
  are 64 x 64 PNGs. The default player model is the LastRoute mannequin.
- **Long views.** Detailed chunks near you, simplified terrain beyond (tested
  out to 2 km).
- **A minimap** in the corner showing the ground you have explored (in
  Earth worlds, the whole map, dimmed where you have not been).
- **Settings** for video (including a 3D detail scale that speeds up old
  graphics cards), audio, controls (every key can be changed), game and skin.
- **Worlds as files.** Export a world to one `.vxworld` file and import it on
  another machine.

## Documentation

| | |
| --- | --- |
| [docs/PLAYING.md](docs/PLAYING.md) | controls, game modes, crafting, farming, baking, furnaces, furniture, bows, caves, vehicles and railways, dimensions, chat commands, settings, command-line options |
| [docs/CREATURES.md](docs/CREATURES.md) | wildlife, monsters and villagers: what lives where, milk, wool and eggs, taming, trading |
| [docs/MULTIPLAYER.md](docs/MULTIPLAYER.md) | hosting and joining, what is shared |
| [docs/EARTH.md](docs/EARTH.md) | real-world terrain, the map, data sources |
| [docs/MODDING.md](docs/MODDING.md) | packs, the mods folder, scripts and commands, models, skins |
| [docs/DATA.md](docs/DATA.md) | the content file format |
| [docs/BUILDING.md](docs/BUILDING.md) | building for each platform, build names and updating, tests, CI, icons |
| [docs/PORTING.md](docs/PORTING.md) | how it stays portable, old Macs, PowerPC, measurements |

## How it stays portable

The program is `no_std` Rust with **no dependencies**: about 23,000 lines,
and a Linux binary of about 2.5 MB. It needs the C library and SDL2 (opened at run
time); graphics are fixed-function OpenGL 1.5 with no shaders. JSON, PNG,
sockets, sound synthesis and the map projection are all written here. Why,
and how each platform is built: [docs/PORTING.md](docs/PORTING.md).

## Build and run

Rust (1.77 is the oldest version tested), and SDL2 where the system can find
it (`libsdl2-2.0-0` on Debian/Ubuntu, `brew install sdl2` on macOS,
`SDL2.dll` beside the program on Windows).

```sh
cargo run --release
```

```sh
cargo test --lib
```

Cross-builds from any machine, with no Apple or Microsoft SDK:

```sh
scripts/build-macos-legacy.sh
```

```sh
scripts/build-windows.sh
```

Details: [docs/BUILDING.md](docs/BUILDING.md). `voxelrust --help` lists the
command-line options.

## Builds and updating

Each build has a name that says what machine it is for: `linux-x86_64`,
`windows-x86_64`, `macos-x86_64` (macOS 10.12 and later) or
`macos-legacy-x86_64` (Mac OS X 10.6 and later). `voxelrust --version` prints
it, and so does the F3 screen. The nightly files are named after it and
listed in `BUILDS.txt`. The two Mac apps declare their minimum system, so
macOS refuses to start the 10.12 one on an older system.

The game does not update itself. To update, replace the program with a newer
file of the same build name; worlds and settings live in the data folder and
are not touched.

## What has actually been run

| Platform | State |
| --- | --- |
| Linux x86-64 | Built, unit tests pass, run and benchmarked |
| Mac OS X 10.6.8 (2008 Mac Pro, Radeon HD 2600) | **Run on the real machine**: the game, a benchmark, and hosting a server that a Linux client joined |
| Windows x64 | Run under Wine 10 with the official `SDL2.dll`: the game, and hosting a server. Not run on real Windows |
| macOS 10.12+ (Intel) | Cross-built (`dist/macos/`); **not run on any modern Mac**. The 10.6 build is expected to work there too |
| Apple Silicon | Not built |
| PowerPC (Linux) | Compiles (`cargo check`); never run |
| Mac OS X 10.5 and older | Not supported (see docs/PORTING.md) |

## Known limits

- The world is 128 blocks high.
- There is no crafting grid. Recipes are lists you click to make; the lists
  are the recipe book.
- Doors do not animate: a door switches at once between its shut shape and
  its open shape.
- Rivers do not flow. Water is animated but stays where it is.
- There are no moving ships built from blocks: vehicles are items you place.
- Railways are simple automation (timed stops and a locomotive that shuttles
  along its line), not a scheduling system: no timetables, routes or working
  signals.
- Pack scripts are a small event-and-command system, not a programming
  language, and Rust code cannot be loaded as a mod.
- In multiplayer, creatures, vehicles and chest contents are shared, but one
  player's game runs the creatures and unridden vehicles for everyone, and
  inventories, health and hunger stay per player and are not kept by the
  server. Villagers' farming and building is not applied to a shared world.
  Dimensions are not available on a server. The sharing has only been tested
  between two copies of the game on one machine.
- Creatures and villagers are not saved with the world: new ones appear
  when you come back. (Pets, vehicles and chests are saved.)
- There are no generated villages, and no breeding: villagers and young
  animals simply turn up.
- Servers have no accounts, passwords or encryption, and **the project hosts
  no public server**. You play with people you give an address to.
- There is no in-game updater.
- The Windows build has only been tested under Wine, not on real Windows.
- Modern macOS is untested: the 10.12 build has never been run.
- PowerPC is only compile-checked (`cargo check`); it has never been run.
- The repository is on GitHub (private, `KyttuXD/VoxelRust`), but its
  workflows (CI and nightly builds) have not been able to run: the account's
  Actions budget blocks them on private repositories. So there are no
  nightly files yet.
- The website is not deployed.
- In Earth worlds, building heights are guesses from the size of the
  footprint: the map tiles carry no heights.

## Multiplayer in one minute

On the title screen choose MULTIPLAYER, then HOST A GAME: a server runs
inside your game and the screen shows the address to give your friends. It
ends when you leave. Or, from the command line:

```sh
voxelrust --host myworld
```

For a server on its own, with no window:

```sh
voxelrust --server --world myworld
```

Others choose MULTIPLAYER on the title screen and type the host's address, or
run `voxelrust --join host`. There is no public server: someone has to host. The port is 25590 (TCP); forward it on your
router to let people outside your network in.
[docs/MULTIPLAYER.md](docs/MULTIPLAYER.md) has the rest.

## Earth worlds

Choose TERRAIN: EARTH when creating a world, pick a starting place and a
scale. This needs the internet and the system's `curl`.
[docs/EARTH.md](docs/EARTH.md) has the rest.

Map data (c) OpenStreetMap contributors. Elevation: Mapzen / AWS Terrain
Tiles.

## Controls

| | |
| --- | --- |
| W A S D, mouse | move, look |
| Space, Ctrl | jump, sprint |
| Space twice | fly, in creative (Space / Shift go up / down) |
| F | get out of a vehicle, or up from a seat |
| Left / right / middle click | break or hit / place, use, open, sit, shoot, trade, tame / pick |
| 1-9, mouse wheel | choose a hotbar slot |
| E | inventory |
| M | world map (Earth worlds); bigger minimap (other worlds) |
| T | chat and commands (`/help` lists them) |
| Esc | menu |
| F1, F2, F3, F5, F11 | hide the HUD, screenshot, details, third-person view, full screen |

## Licence

MIT; see [LICENSE](LICENSE).
