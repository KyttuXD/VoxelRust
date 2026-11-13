# Multiplayer

There are two ways to host, and both use the game program itself:

- **HOST A GAME** in the MULTIPLAYER menu (or `--host NAME`): a server runs
  inside the game while you play in it.
- **`--server`**: a server on its own, with no window. It needs no graphics
  card, so any machine that can start the game can host, including a 2008
  Mac on Mac OS X 10.6.8.

The project runs no public server and keeps no list of servers. To play
together, one of you hosts and gives the others an address.

## Hosting from inside the game

On the title screen choose MULTIPLAYER, type a name for the shared world
under `OR HOST YOUR OWN` (letters, digits, `-` and `_`; `shared` if left
empty) and click **HOST A GAME**. From the command line:

```sh
voxelrust --host myworld
```

- The game starts a server on port 25590 for up to 16 players and joins it
  itself. The chat area shows what others type to join, for example
  `HOSTING. OTHERS JOIN WITH  192.168.1.20:25590`. The MULTIPLAYER screen
  also shows `YOUR ADDRESS ON THIS NETWORK`.
- That address works for people on the same network. For anyone outside it,
  forward the port on your router (see
  [below](#hosting-for-people-outside-your-network)).
- **The game ends when the host leaves.** Quitting to the title screen or
  closing the game stops the server; the edits are saved first.
- The shared world is a server world: it uses the same two files as
  `--server` (below), so the same world can be hosted either way, one at a
  time. It is separate from your singleplayer worlds.
- If the port is already in use, hosting fails with a message.

## Running a server on its own

```sh
voxelrust --server --world myworld
```

| Option | Meaning |
| --- | --- |
| `--world NAME` | name of the server's world (default `server`) |
| `--seed N` | seed, the first time the world is made |
| `--earth LAT LON`, `--scale M V` | make it an Earth world (see [EARTH.md](EARTH.md)), the first time |
| `--port N` | TCP port to listen on (default **25590**) |
| `--players N` | most players at once (default 16, at most 200) |
| `--pack FILE` | content packs every player will use (see below) |
| `--data DIR` | where the server keeps its files |
| `--log FILE` | also write the server's output to a file |

The server prints who joins and leaves, chat, and relay requests. Stop it
with Ctrl+C.

### Files it writes

In the data folder:

- `<name>.server.txt`: the world's seed and kind. Once this exists, `--seed`
  and `--earth` are ignored for that world.
- `<name>.server-edits.txt`: every block players have changed, one per line,
  by block name. It is written about every 20 seconds when something has
  changed. A `--server` stopped with Ctrl+C does not save on the way out, so
  the last few seconds of building can be lost; a game hosted from the menu
  saves when the host leaves.

Terrain is never stored or sent: every player's game generates it from the
seed and then applies the edits. A server world is not a singleplayer world;
the two kinds of save are separate.

## Joining

- From the title screen: MULTIPLAYER, type the address (`host` or
  `host:port`) and your name, JOIN.
- From the command line: `voxelrust --join host` or `voxelrust --join host:port`.

Your name comes from SETTINGS > GAME (letters, digits, `-` and `_`, 16 at
most). If someone on the server already has it, a number is added.

The server refuses a player whose game speaks a different version of the
network protocol (currently version 4; it changed when creatures, vehicles
and chest contents became shared), and anyone beyond the player limit.
Everyone should run the same version of the game.

## What is shared, and what is not

Shared between everyone on a server:

- **Players**: position, the direction they look, name tags, and skins.
- **Block edits**: every block placed or broken, including everything built
  before you joined.
- **Chat**.
- **Time of day**, and with it the weather (which is worked out from the
  seed, place and time, so everyone sees the same sky).
- **Content**: the server's packs (next section).

- **Creatures.** Everyone sees the same animals, monsters and villagers.
- **Vehicles**, including trains and what they carry.
- **Chest contents.**

### How creatures, vehicles and chests are shared

The server does not run creatures or vehicles itself: it only passes
messages on. Instead **one player's game runs them for everybody**: the
player with the lowest player number of those present (normally whoever
joined first). When that player
leaves, the next lowest takes over.

- That game moves the creatures and the vehicles nobody is riding, and
  tells the others where everything is five times a second. The others
  only animate what they are told.
- When another player hits a creature, their game sends the hit to that
  player, whose game decides what becomes of it. Arrows work the same way.
- When another player sets down or picks up a vehicle, their game asks
  that player to do it. A vehicle you are riding is steered by your own
  game, which reports where it is.
- A hostile creature's bite is judged in the game of the player being
  bitten.
- Chest contents are sent when a player changes what is in a chest, and
  **the server keeps the latest contents of each chest** and sends them to
  players who join later. It keeps them in memory, not in a file: they last
  as long as the server runs.
- `/spawn` works only for the player whose game runs the creatures.

This has been tested only between two copies of the game on one machine. It
has not been tried over a real network, with slow connections, or with
many players.

### Not shared

- **Inventories, health and hunger.** These are per player and are not kept
  by the server. You join in creative mode with the creative starting items,
  and nothing you carry is saved when you leave.
- **Villagers' work.** Villagers are shared like other creatures, but their
  farming and building is not applied to a shared world.
- **Pets** are not saved on a server.
- **Dimensions.** `/dim` does not work on a server.

Natural disasters do not happen on servers. Sleeping in a bed sets where you
come back to but does not skip the night. The commands `/time`, `/mode`,
`/weather`, `/set`, `/dim`, `/shoot`, `/aim`, `/strike` and `/reload` do not
work, and `/spawn` only for one player (see
[PLAYING.md](PLAYING.md#chat-and-commands)). Pack scripts run in each
player's own game.

## Server-hosted packs

A server started with packs (`pack.txt` in its data folder, anything in its
`mods` folders, or `--pack FILE`; see [MODDING.md](MODDING.md)) sends them to
every player on joining: the pack files themselves plus every texture, model
and included file they refer to. A game hosted from the menu sends the
host's packs the same way. A unit test checks that a pack reaches a joining
player whole. Players then use the
server's blocks, items, recipes, creatures and vehicles instead of their own,
and see `USING THE SERVER'S CONTENT`. Nothing is written to the player's
disk; when they go back to singleplayer their own packs are in use again.

So players do not need to install anything to join a modded server. While on
a server, `/reload` and RELOAD CONTENT PACKS NOW do nothing: the server
decides the content. To change a server's packs, restart it.

## The map-data relay

Earth worlds download map tiles with the system's `curl` over HTTPS. Machines
whose `curl` is too old for today's HTTPS (Mac OS X 10.6, for example) can
ask a VoxelRust server to fetch for them instead:

1. Run `voxelrust --server` on a machine with working internet and a modern
   `curl` (any server will do, including a game hosted from the menu; it
   need not be the one you play on).
2. On the old machine, set SETTINGS > GAME > MAP DATA RELAY to that server's
   `host:port`, for example `192.168.1.20:25590`, then restart the game (the
   relay address is read at start-up).

The old machine then gets elevation tiles, map tiles and place names through
the server. Tiles are cached on both machines.

## Hosting for people outside your network

- Forward TCP port 25590 (or the one you chose with `--port`) on your router
  to the machine running the server. The game does not do this for you.
- Give your players your public address.
- There is no server list and no public server run by the project.

Be clear about what the server does **not** have:

- **No accounts and no passwords.** Anyone who can reach the port can join
  under any free name, build and break anything, and use the relay.
- **No encryption.** Positions, chat, skins, edits and chest contents travel
  as plain data.
- **No moderation tools**: no kick, ban or protected areas.

Host for people you trust, keep backups of the two server files, and close
the port when you are done.
