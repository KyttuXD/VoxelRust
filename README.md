# VoxelRust

A voxel sandbox written in Rust that runs on old machines as well as new
ones. It has been run on a 2008 Mac Pro under Mac OS X 10.6.8, on a 2015
MacBook Air under macOS 12, on Linux, and (under Wine) as a Windows program.

It is an original game, not a clone of an existing one: its own blocks, its
own procedurally painted textures and its own terrain generator.

## What is in it

- **Worlds.** Endless invented terrain from a seed: ten biomes (plains,
  forest, desert, tundra, taiga, mountain, swamp, savanna, jungle, badlands),
  six kinds of tree, oceans, ores, and seven kinds of cave (mushroom,
  crystal, lush, frozen, magma, webbed and plain) each with its own blocks
  and creatures. Or a **flat** world of level grassland to build on. Or
  **the real Earth** (a mod that comes with the game): heights from public
  elevation tiles;
  water, forests, sand, ice, asphalt roads with painted lines, hollow
  buildings with floors, stairs and furniture, and fields of crops from
  OpenStreetMap; and an in-game map (M) to look around, with waypoints (N).
  The world is 256 blocks high.
- **Survival and creative modes.** Survival has health, hunger, falling,
  drowning, mining times with cracks spreading over the block, tools, hoes
  and swords in six materials, drops, crafting in a 2 x 2 and a 3 x 3 grid with a recipe book
  at a crafting table, furnaces, bows, chests, and monsters at night and in caves. Creative
  has flying (press jump twice), instant building, area tools, and an item
  list with picture tabs and block icons drawn as isometric cubes, including
  a spawn egg for every creature.
- **Farming and cooking.** Hoes till soil; wheat, carrots, potatoes and
  tomatoes grow in four stages; an oven bakes bread, pies, stew and three
  kinds of pizza.
- **About 490 blocks.** Sixteen colours of concrete, wool, terracotta,
  glass, beds, couches and armchairs; six woods with five kinds of door,
  trapdoors and gates that open; furniture to sit on, store things in and
  cook at; food set out on plates; stone kinds; stairs, slabs, walls and
  fences that join up; lamps; chests; sand and gravel that fall; animated
  water; custom shapes, and props (blocks drawn as models).
- **Creatures.** 75 kinds: farm animals that give milk, wool and eggs and
  have young, animals you can tame, hunters and prey, birds and fish, night
  monsters (skeleton archers among them), cave dwellers, and villagers who
  trade, farm, build and stand guard. Horses, donkeys, camels and elephants
  can be tamed and ridden. See [docs/CREATURES.md](docs/CREATURES.md).
- **Vehicles and railways.** Boats, a plane, rail carts (some with
  storage), a buggy, a truck and a kart, from the Boats, Planes, Trains and
  Land Vehicles mods. Speed, slow
  and station track; chains that join carts into trains; a locomotive that
  runs by itself, stops at stations and shuttles along its line.
- **Dimensions.** Other worlds inside each world (`sky`, `flatland`, `void`,
  `mirror`), reached with `/dim`; packs can add more.
- **Sound.** Every sound is made from a line of numbers in the data files,
  so packs can retune them or add their own. Optional subtitles name what
  you hear.
- **Weather.** Regional rain, snow and thunderstorms; optional earthquakes,
  tsunamis and tornadoes (the Natural Disasters mod).
- **Multiplayer.** HOST A GAME from the menu runs a server inside the game
  while you play; `voxelrust --server` runs one on its own. Players join
  from the title screen. Positions, edits, chat, time, skins, creatures,
  vehicles and chest contents are shared, and the server sends its content
  packs to every player.
- **Modding.** Textures, blocks (including doors, crops, seats and falling
  blocks), shapes, items, tools, weapons, food, recipes, ores, plants,
  models, creatures, vehicles, sounds, dimensions, chat commands and small
  scripts (`when <event> do <commands>`, with variables and conditions) are
  declared in text files. Drop a mod folder with a `pack.txt` into the
  `mods` folder and it loads; `/reload` picks up changes to packs without
  restarting. Eight mods come with the game, each a folder of its own in
  `mods/` (see [The mods that come with the game](#the-mods-that-come-with-the-game));
  SETTINGS > MODS switches any mod on or off. Blockbench `.bbmodel` files
  are read directly. Player skins are 64 x 64 PNGs. The default player model is the LastRoute mannequin.
- **Long views.** Detailed chunks near you, simplified terrain beyond (tested
  out to 2 km).
- **A minimap** in the corner showing the ground you have explored (in
  Earth worlds, the whole map, dimmed where you have not been).
- **Settings** for video (including a 3D detail scale that speeds up old
  graphics cards), audio, controls (every key can be changed), game, skin and
  mods.
- **Worlds as files.** Export a world to one `.vxworld` file and import it on
  another machine.
- **Updates.** The game looks for a newer nightly build when it starts and
  can replace its own program (see [Builds and updating](#builds-and-updating)).

## The mods that come with the game

Each is a folder in `mods/`, and each is named in `mods/SHIPPED`. SETTINGS >
MODS switches it off.

| Folder | What it adds |
| --- | --- |
| `disasters` | Natural Disasters: earthquakes, tsunamis and tornadoes. Pack only. Its own switch is in SETTINGS > GAME. |
| `earth` | Real-world Earth: the terrain, the map (M) and the EARTH world choice. Has Rust code. |
| `vehicles` | The machinery for boats, carts, cars and planes. Has Rust code. Adds no vehicle of its own. |
| `boats` | Boats. Needs `vehicles`. |
| `trains` | Rail track, carts, a locomotive and stations. Needs `vehicles`. |
| `planes` | A small plane. Needs `vehicles`. |
| `landvehicles` | A buggy, a truck and a kart. Needs `vehicles`. |
| `holidays` | Decorations and treats for the feast days, made at a crafting table or in an oven, and four big props. |

## Documentation

| | |
| --- | --- |
| [docs/PLAYING.md](docs/PLAYING.md) | controls, game modes, crafting, farming, baking, furnaces, furniture, bows, caves, vehicles and railways, dimensions, chat commands, settings, command-line options |
| [docs/CREATURES.md](docs/CREATURES.md) | wildlife, monsters and villagers: what lives where, milk, wool and eggs, taming, trading |
| [docs/MULTIPLAYER.md](docs/MULTIPLAYER.md) | hosting and joining, what is shared |
| [docs/EARTH.md](docs/EARTH.md) | real-world terrain, the map, data sources |
| [docs/MODDING.md](docs/MODDING.md) | packs, the mods folder, the mods that come with the game, scripts and commands, models, skins |
| [docs/DATA.md](docs/DATA.md) | the content file format |
| [docs/BUILDING.md](docs/BUILDING.md) | building for each platform, packaging a release, build names and updating, tests, CI, icons |
| [docs/PORTING.md](docs/PORTING.md) | how it stays portable, old Macs, PowerPC, measurements |
| [docs/WEB.md](docs/WEB.md) | the browser build |

## How it stays portable

The program is `no_std` Rust with **no dependencies**: about 25,000 lines,
and a Linux binary of about 2.6 MB. It needs the C library and SDL2 (opened at run
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

Everything at once, laid out as a release in `dist/nightly/`:

```sh
scripts/package-release.sh
```

Details: [docs/BUILDING.md](docs/BUILDING.md). `voxelrust --help` lists the
command-line options.

The game reads the mods that come with it from a `mods` folder beside
the program (inside the app on a Mac) or from `./mods`, so start a program
built by hand from the top of the source tree, as `cargo run` does.

## Repository layout

| Path | What is in it |
| --- | --- |
| `src/` | The game. `src/native.rs` holds the table of hooks (`struct Native`) that mod code fills in. |
| `mods/<name>/` | A mod: a `pack.txt`, and a `code/` folder if it has Rust. `build.rs` compiles each `code/mod.rs` into the program, so changing mod code needs a rebuild. The code runs only while a loaded pack says `feature <name>`. |
| `mods/SHIPPED` | The mods that come with the game, one folder name a line. The release scripts and `mods/tests.rs` use it. |
| `data/` | The base content (`base.txt`, `creatures.txt`, `wildlife.txt`) and its models. |
| `scripts/` | Build, cross-build and release scripts. |
| `tools/` | Python generators for the data, icons and models. |
| `web/` | The browser build. |

## Builds and updating

Each build has a name that says what machine it is for: `linux-x86_64`,
`windows-x86_64`, `macos-x86_64` (macOS 10.12 and later) or
`macos-legacy-x86_64` (Mac OS X 10.6 to 10.11). `voxelrust --version` prints
it with the commit the program was built from, and the F3 screen shows it.
The two Mac apps declare their minimum system, so macOS refuses to start the
10.12 one on an older system.

The files are on the `nightly` release:
<https://github.com/KyttuXD/VoxelRust/releases/tag/nightly>. Each archive
holds the program (an app on the Mac), the mods that come with the
game, and the docs; `BUILDS.txt` says which file is for which machine. They
are built by `scripts/package-release.sh` and uploaded by hand.

**The game updates itself.** When it starts it reads `nightly.txt` from that
release; if the release was built from a newer commit, it downloads the
program listed for its own build name, checks its SHA-256, and puts it in
place of the running program. The new version is in use from the next start,
and the title screen says what happened. `--no-update` skips the check for
one run; UPDATE THE GAME AUTOMATICALLY in SETTINGS > MODS turns the
replacing off (the game then only says a newer version is out). Limits:

- The download is done by the system's `curl`. Mac OS X 10.6's own `curl`
  cannot make modern HTTPS connections, so **the old-Mac build cannot update
  itself**: replace the app by hand.
- Only the program is replaced. The `mods` folder that came with it, the
  docs and SDL2 are not.
- A program built by hand without a commit stamp is never replaced, and
  there is no check in scripted runs (`--screenshot`, `--bench`), for
  `--server`, or in the browser.

Worlds and settings live in the data folder and are not touched. Details:
[docs/BUILDING.md](docs/BUILDING.md#build-names-and-updating).

## What has actually been run

| Platform | State |
| --- | --- |
| Linux x86-64 | Built, unit tests pass, run and benchmarked |
| Mac OS X 10.6.8 (2008 Mac Pro, Radeon HD 2600) | **Run on the real machine** (the legacy build): the game, a benchmark (about 256 FPS in a window), and hosting a server that a Linux client joined |
| macOS 12.7.6 (2015 MacBook Air, Intel HD 6000) | **Run on the real machine**: the 10.12+ build as an app with the SDL2 2.0.4 framework inside it. 246 FPS in a 1280 x 720 window at default settings; an Earth world rendered; started plainly, it stays open |
| Windows x64 | Run under Wine 10 with the official `SDL2.dll`: the game, and hosting a server. Not run on real Windows |
| Apple silicon | No build of its own, and no such Mac could be reached for testing: untested |
| PowerPC (Linux) | Compiles (`cargo check`); never run |
| Mac OS X 10.5 and older | Not supported (see docs/PORTING.md) |

## Known limits

- There are no vertical chunk sections. The world is 256 blocks high and
  each chunk is one column of that height, so there is no building above
  256 or digging below 0. Invented terrain tops out at height 114; the rest
  is room to build, and for mountains in Earth worlds.
- The crafting grid is 2 x 2 in the inventory and 3 x 3 at a table, with
  shaped and shapeless recipes, but there is no recipe-book unlocking and no
  tags beyond the `group` lines of the data.
- Doors do not animate: a door switches at once between its shut shape and
  its open shape.
- Rivers do not flow. Water is animated but stays where it is.
- There are no moving ships built from blocks: vehicles are items you place.
- Railways are simple automation (timed stops and a locomotive that shuttles
  along its line), not a scheduling system: no timetables, routes or working
  signals.
- Pack scripts are an event-and-command language with variables and
  conditions, not a general programming language, and a mod's Rust is
  compiled in at build time, so changing it needs a rebuild.
- In multiplayer, inventories, health and hunger stay per player and are not
  kept by the server. Creatures, vehicles and chest contents are shared, but
  one player's game runs the creatures and unridden vehicles for everyone.
  Villagers' farming and building is not applied to a shared world.
  Dimensions are not available on a server. The sharing has only been tested
  between two copies of the game on one machine.
- Villagers and wild creatures are not saved with the world: new ones appear
  when you come back. (Pets, vehicles and chests are saved.)
- There are no generated villages, and no breeding: villagers and young
  animals simply turn up.
- Servers have no accounts, passwords or encryption. The project runs one
  public server; everyone else can host their own (see
  [docs/MULTIPLAYER.md](docs/MULTIPLAYER.md)).
- The updater never works on Mac OS X 10.6 (see
  [Builds and updating](#builds-and-updating)). It replaces the program only,
  not the mods that came with it.
- The Windows build has only been tested under Wine, not on real Windows.
- Modern macOS has been run on one Intel Mac (macOS 12.7.6). Apple silicon
  is untested.
- PowerPC is only compile-checked (`cargo check`); it has never been run.
- The repository is on GitHub (`KyttuXD/VoxelRust`). Its workflows (CI and
  nightly builds) cannot run: the account's Actions budget blocks them. The
  nightly files are built on the author's machine and uploaded by hand.
- The browser build has no folders to read mods from, so none of the mods that
  come with the game are in it.
- The browser build ([docs/WEB.md](docs/WEB.md)) has been built and run
  locally in Chromium, but the website is not deployed: the server meant for
  it (`voxelrust.games.kyttuslab.org`) could not be reached over SSH without
  credentials.
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
run `voxelrust --join host`. The project runs one public server (the PUBLIC SERVER button on the multiplayer screen); everyone else can host. The port is 25590 (TCP); forward it on your
router to let people outside your network in.
[docs/MULTIPLAYER.md](docs/MULTIPLAYER.md) has the rest.

## Earth worlds

Earth is a mod that comes with the game and is on unless you switch it off
in SETTINGS > MODS. Choose EARTH on the TERRAIN button when creating a world, pick a
starting place and a scale. This needs the internet and the system's `curl`.
[docs/EARTH.md](docs/EARTH.md) has the rest.

Map data (c) OpenStreetMap contributors. Elevation: Mapzen / AWS Terrain
Tiles.

## Controls

| | |
| --- | --- |
| W A S D, mouse | move, look |
| Space, Ctrl | jump, sprint |
| Space twice | fly, in creative (Space / Shift go up / down) |
| F | get out of a vehicle, off a creature, or up from a seat |
| Left / right / middle click | break or hit / place, use, open, sit, shoot, trade, tame, ride / pick |
| 1-9, mouse wheel | choose a hotbar slot |
| E | inventory |
| M | world map (Earth worlds: the Earth map) |
| N | world map with waypoints |
| T | chat and commands (`/help` lists them) |
| Esc | menu |
| F1, F2, F3, F5, F11 | hide the HUD, screenshot, details, third-person view, full screen |

## Licence

MIT; see [LICENSE](LICENSE).
