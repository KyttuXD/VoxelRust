# Earth worlds

An Earth world is real terrain: you start at a real place and can walk, sail
or travel by map to anywhere between 85 degrees north and south.

## The Earth mod

Earth is a mod that comes with the game. It lives in `mods/earth/`:

- `pack.txt` holds the data. Its `feature earth` line switches the mod on,
  and its `map` lines give the sources, the colours, the snow line and the
  starting places.
- `code/` holds the Rust code, `mod.rs` and `map.rs`. It is compiled into
  the program at build time, and it runs only while a loaded pack says
  `feature earth`.

The game itself has no Earth code. Changing the code needs a rebuild.
Changing the `map` lines does not, because they are read from the pack.

The mod ships beside the program in a `mods` folder (inside the app on a
Mac). It is on unless you turn it off under SETTINGS > MODS, where it is
listed as EARTH, or the folder is missing.

With the mod off, the NEW WORLD screen does not offer EARTH as a terrain,
and `--earth` stops with the message ``that option needs the mod `earth`,
which is switched off (Settings > MODS)``. An Earth world made earlier still
opens, but its ground is made up from the seed, as in any other world, until
the mod is on again.

The mod's code adds these to the game:

- the EARTH choice on the NEW WORLD screen, and the `--earth` and `--scale`
  options (listed under "From the mod `earth`" in `--help`),
- the map on M, and place notices,
- two rows on SETTINGS > GAME: PLACE NOTICES and MAP DATA RELAY,
- answers to other players' requests for map data.

The `map` lines in `pack.txt` are:

| Line | Meaning |
| --- | --- |
| `map heights URL terrarium 14` | Elevation tiles. `{z}`, `{x}` and `{y}` are filled in. 14 is the deepest zoom. |
| `map cover URL` | Map tiles. Read for land cover, roads and buildings, and shown on the map. |
| `map names URL` | Place-name lookup. `{lat}`, `{lon}` and `{zoom}` are filled in. |
| `map colour R G B KIND` | What a colour on the map tiles means. Kinds: land, water, forest, grass, sand, ice, rock, urban, building, road, farm. |
| `map snowline TOP DROP` | Permanent snow: metres up at the equator, and metres lower per degree north or south. |
| `map place LAT LON NAME` | A starting place on the NEW WORLD screen. |

A line that is wrong is left out, and the game logs a warning.

## Making one

On the NEW WORLD screen set TERRAIN to EARTH (the button steps through
NORMAL, FLAT and EARTH), then choose:

- **START AT**: one of ten places (Mount Rainier, Grand Canyon, Mount Fuji,
  Matterhorn, Manhattan, London, Sydney Harbour, Mount Everest, Nile Delta,
  Hawaii). You can go anywhere else afterwards with the map.
- **SCALE** `1:N`: one block stands for N metres across. Choices: 1:1 (life
  size), 1:2, 1:5, 1:10, 1:30, 1:100, 1:300, 1:1000.
- **HEIGHT** `1:N`: one block stands for N metres of altitude. Choices: 1:1,
  1:2, 1:5, 1:10, 1:25, 1:50, 1:75, 1:150, 1:300.
- **ROADS AND BUILDINGS**: on or off.

The defaults are 1:30 across and 1:75 up. The world is 256 blocks high, sea
level is at height 48, and land can rise to height 242, which leaves 193
blocks above the sea: about 14,400 metres at 1:75, 9,600 at 1:50 (enough for
Mount Everest) and 4,800 at 1:25. Anything taller is cut off flat at that
height. With blocks less than 20 metres tall the world cannot hold real
altitudes, so heights are measured from near the starting place's own
altitude instead of from sea level.

There are no vertical chunk sections: 256 blocks is all the height there
is, at any scale.

From the command line, for any latitude and longitude:

```sh
voxelrust --world alps --earth 45.9763 7.6586 --scale 30 75
```

`--scale` takes metres per block across and up. Worlds made this way have
roads and buildings on.

## What is generated

- **Heights** from elevation tiles.
- **Land cover** from the colours of the standard OpenStreetMap map: water
  (seas, lakes, rivers), forest, grass, sand, ice and bare rock. Together
  with latitude and altitude this picks the biome, and so the trees, ground
  and creatures: taiga in the far north and high up, jungle and savanna in
  the tropics, snow above the snow line.
- **Roads** (if ROADS AND BUILDINGS is on): roads become asphalt. At scales
  of 6 metres per block or finer (1:1, 1:2, 1:5) they are also painted: roads three or
  more blocks wide get a dashed yellow line down the middle, and roads five
  or more wide get solid white edges as well. Yards, squares and other built-up ground are gravel.
- **Buildings** (if on): where the map shows a building, one stands, in one
  of four wall materials, with rows of windows, a doorway here and there at
  street level and a flat roof. Buildings are hollow: there is a floor every
  4 blocks, flights of stairs between floors, a lamp in the ceiling now and
  then, and some furniture (tables, chests, crafting tables, beds).
  **The map tiles carry no building heights**, so the game guesses: the
  broader a building's footprint, the more storeys it gets (from one to
  twenty, about three metres each, varied a little from lot to lot). Shapes
  follow the map; heights, materials and everything inside are invented.
- **Farmland**: areas the map marks as farmland, orchards or vineyards
  become fields about 16 blocks square with paths between them. Most are
  tilled and planted in rows with one crop each (wheat, carrot, potato or
  tomato, already ripe); some lie fallow and some are pasture with tall
  grass.

The elevation data stops at a certain detail; worlds finer than that
(roughly 1:10 and below, depending on latitude) are smoothed between its
points.

## The map (M)

Press M in an Earth world. (In other worlds M enlarges the minimap instead;
see [PLAYING.md](PLAYING.md#the-minimap).) Drag to move, use the wheel to zoom, **right click
to travel** to the spot under the mouse, M or Esc to close. The top line
shows the latitude, longitude and block position under the mouse; the red
marker is you. Travelling sets you down above the ground at that spot.

The minimap in the corner works differently in Earth worlds too. The land
is known from the start, so the minimap shows all of it: ground you have
been near at full brightness, the rest dimmed. (In another dimension of an
Earth world it goes back to showing only what you have explored.)

## Place notices

As you move, the game can tell you where you are (`YOU ARE IN ...`,
`ENTERING ...`). SETTINGS > GAME > PLACE NOTICES chooses how much detail:
OFF, COUNTRIES (the default), REGIONS or TOWNS. The game asks at most once
every twelve seconds.

## Internet, cache and the relay

Earth worlds need an internet connection and the `curl` program (present on
Linux, macOS and Windows 10/11). Tiles are downloaded as you go and kept in
the data folder as `tile-*.png`, so places you have visited work offline
later. If a tile cannot be fetched, that piece of terrain comes out as sea;
opening the map makes the game try failed tiles again.

Machines whose `curl` cannot make modern HTTPS connections (Mac OS X 10.6)
can fetch through any machine running `voxelrust --server`: set SETTINGS >
GAME > MAP DATA RELAY to its `host:port` and restart the game. See
[MULTIPLAYER.md](MULTIPLAYER.md#the-map-data-relay).

## Data sources and attribution

The mod fetches from three addresses. Each is set by a `map` line in
`mods/earth/pack.txt`:

| Data | Source | Pack line | Address |
| --- | --- | --- | --- |
| Elevation | Mapzen "terrarium" tiles, hosted as AWS Terrain Tiles | `map heights` | `https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png` |
| Land cover, roads, buildings, the map screen | OpenStreetMap standard map tiles | `map cover` | `https://tile.openstreetmap.org/{z}/{x}/{y}.png` |
| Place names | OpenStreetMap Nominatim reverse geocoding | `map names` | `https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat={lat}&lon={lon}&zoom={zoom}&accept-language=en` |

An address must start with `https://`, be under 300 characters, and use only
letters, digits and `/ : . _ - ? = & { } , + ~`. A line with any other
address is left out. The addresses go into a `curl` command, so the rule
keeps shell characters out. For place names, `{zoom}` is 3 for countries, 5
for regions and 10 for towns.

A client builds every address from its own packs. When it joins a server,
the server's packs replace the blocks and items, but its `map` lines stay
the client's own. A server cannot change the addresses a client fetches
from. A MAP DATA RELAY is a server that fetches for the client: the client
sends only tile or place numbers, and the relay answers using the addresses
in its own packs.

Required attribution, shown at the bottom of the map screen and to be kept
with any pictures or videos of Earth worlds that you publish:

> Map data (c) OpenStreetMap contributors. Elevation: Mapzen / AWS Terrain
> Tiles.

The OpenStreetMap tile and Nominatim servers are free, shared services run on
donations. The game identifies itself as `VoxelRust/<version>`, caches every
tile and limits place-name questions; please do not script bulk downloads
through it.

## Compared with Tellus

[Tellus](https://github.com/Yucareux/Tellus) is a Minecraft Java mod that
also builds real-world terrain. The two work differently, and this is what
the Earth mod does and does not do, as Tellus's own README describes its
features:

| Feature | Tellus | VoxelRust Earth mod |
| --- | --- | --- |
| Terrain from elevation data | Yes | Yes (Terrarium tiles on AWS) |
| Land cover from map data | Yes | Yes (OpenStreetMap tiles, colour-classified) |
| Biomes by climate | Yes (Köppen-style zones) | No: biomes come from latitude and the map's cover |
| Roads and buildings from OSM | Yes | Yes, with heights guessed from footprint |
| Building interiors, stairs, street lights | Yes | No |
| Rivers and water from the map | Yes | Water from the map's colour, no river flow |
| Map teleport UI | Yes | Yes: press M and right click a spot |
| Terrain preview before making a world | Yes | No |
| Real-time time and weather | Yes (optional) | No |
| Distant Horizons integration | Yes | No: distant terrain is the game's own |
| Multiplayer server support | Installed on the server | Shared worlds: the server keeps the kind, packs define the Earth mod |

Not compared against Tellus's code, only its README. Anything Tellus does
that is not in the table is not in this game.
