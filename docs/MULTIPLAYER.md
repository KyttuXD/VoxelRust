# Multiplayer

There are two ways to host, and both use the game program itself:

- **HOST A GAME** in the MULTIPLAYER menu (or `--host NAME`): a server runs
  inside the game while you play in it.
- **`--server`**: a server on its own, with no window. It needs no graphics
  card, so any machine that can start the game can host, including a 2008
  Mac on Mac OS X 10.6.8.

The project runs one public server, 195.62.47.178 (port 25590), and has a button for it on the multiplayer screen. It keeps no list of other servers. To play
together, one of you hosts and gives the others an address.

## Hosting from inside the game

On the title screen choose MULTIPLAYER, type a name for the shared world
in the **SHARED WORLD NAME** box (letters, digits, `-` and `_`; `shared` if
left empty) and click **HOST A GAME**. From the command line:

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

- From the title screen: MULTIPLAYER, then click a saved server to join it.
  To add one, use ADD SERVER (see [Saved servers](#saved-servers)).
- From the command line: `voxelrust --join host` or `voxelrust --join host:port`.
  Without a port the game uses 25590.

Your name is the **YOUR NAME** box on the MULTIPLAYER screen, the same name
as in SETTINGS > GAME. It takes letters, digits, `-` and `_`, 16 at most; an
empty name becomes `Player`. If someone on the server already has it, a
number is added (`Bob` becomes `Bob2`).

The server refuses a player whose game speaks a different version of the
network protocol (currently version 9), and anyone beyond the player limit.
Everyone should run the same version of the game.

## What is shared, and what is not

Shared between everyone on a server:

- **Players**: position, the direction they look, name tags, and skins; what
  their body is doing (a pose byte with every position: standing, crouching,
  sitting, lying, swimming, climbing, falling, riding, and whether they are
  drawing a bow, eating or striking), and the **emote** they are playing
  (`Msg::Emote`, the number of the emote in the server's pack; 0 stops it).
- **Block edits**: every block placed or broken, including everything built
  before you joined.
- **Chat**.
- **Time of day**, and with it the weather (which is worked out from the
  seed, place and time, so everyone sees the same sky).
- **Content**: the server's packs (next section).
- **Creatures.** Everyone sees the same animals, monsters and villagers.
- **Vehicles**, when the Vehicles mod is on: boats, trains, planes and land
  vehicles, with what is in their holds.
- **Chest contents.**

### How creatures, vehicles and chests are shared

The server does not run creatures or vehicles itself: it only passes
messages on. Instead **one player's game runs them for everybody**: the
player with the lowest player number of those present (normally whoever
joined first). When that player
leaves, the next lowest takes over. Vehicles are run by the code of the
Vehicles mod, so they are shared only while that mod is on.

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
- `/spawn` and spawn eggs work only for the player whose game runs the
  creatures. A creature being ridden is moved by that game too, so riding
  one works for that player only.

This has been tested only between two copies of the game on one machine. It
has not been tried over a real network, with slow connections, or with
many players.

### Messages from mod code

A mod's own code can send messages to the same mod's code on the other
players' machines. Each mod whose code is built into the program has its own
channel, from 32 up (`channel::NATIVE` in `src/net/proto.rs`). The server
passes these on like everything else and keeps a message only when the mod
asks it to. A game acts on them only while that mod is on; the others
ignore them.

### Not shared

- **Inventories, health and hunger.** These are per player and are not kept
  by the server. You join in creative mode with the creative starting items,
  and nothing you carry is saved when you leave.
- **Villagers' work.** Villagers are shared like other creatures, but their
  farming and building is not applied to a shared world.
- **Pets** are not saved on a server.
- **Dimensions.** `/dim` does not work on a server.

World variables set by scripts (`/var`) are per player and are not saved on
a server.

Natural disasters do not happen on servers. Sleeping in a bed sets where you
come back to but does not skip the night. The commands `/time`, `/mode`,
`/weather`, `/set`, `/dim`, `/shoot`, `/aim`, `/strike`, `/blast` and `/reload` do not
work, and `/spawn` only for one player (see
[PLAYING.md](PLAYING.md#chat-and-commands)). Pack scripts run in each
player's own game.

## Voice chat

Proximity voice chat: players near each other hear each other, louder the
closer they are, like in real life. Everybody starts **muted** (the
microphone is off) but **can hear**; see
[PLAYING.md](PLAYING.md#voice-chat) for the player's side.

The server does the thinking so that the clients do almost nothing:

- A talker's game packs speech into 20 ms frames of IMA ADPCM (4 bits a
  sample, 16 kHz, mono; 163 bytes a frame, about 8 kB a second while
  talking) and sends them over the same connection as everything else.
  Nothing is sent while nobody is talking.
- The server knows where every player is (from their position messages). For
  each frame it picks the players who are within the voice range (24 blocks by
  default, `/voicerange`), have **hear other players** on and are not the
  talker, works out the loudness from the distance (full volume within a fifth of the range, then falling smoothly
  to nothing at the range) and forwards the frame, untouched, with that
  loudness as one byte. A client only decodes, multiplies by that byte and its
  own voice volume, and plays.
- It drops frames from players who may not talk: `voice` is a permission
  (`/perm NAME -voice`; allowed by default, so `/default -voice` makes a
  server where operators choose who may talk), muted players
  (`/mute`), players whose microphone switch is off, and anyone sending more
  than about 56 frames a second (a burst of 12 is allowed). Frames of the
  wrong shape are dropped too.
- Messages: `Voice` (id, sequence number, loudness, frame) in both
  directions and `VoiceState` (client to server: two flags, "I want to hear"
  and "my microphone is on", sent at joining and whenever they change). A
  client that never sends `VoiceState` is neither sent nor heard.
- Voice does not know about dimensions (`/dim` does not work on a server
  anyway), and walls do not muffle it.
- **Privacy.** The server forwards each frame and forgets it at once. Nothing
  is recorded, logged or stored, by the server or by the game, and the log
  shows no voice traffic. The microphone is not opened by a client unless its
  player has turned it on, the server allows it, and push to talk is held (or
  voice activation is on).

Every server of this version relays voice, but with everyone at their
default (microphone off) it costs nothing beyond a three-byte message per
player when they join.

The permissions and mutes are kept with the operators in the server's
permissions file (`voicerange N` and `mute NAME` lines beside the `perm`
lines). Files written before voice chat existed still allow it.

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
a server, `/reload` and RELOAD MODS NOW do nothing: the server
decides the content. To change a server's packs, restart it.

The mods that come with the game (Natural Disasters, Earth, Vehicles, Boats,
Trains, Planes, Land Vehicles and Holidays) are ordinary packs here: the ones
found and switched on where the server runs are sent with the rest, so players on a server whose Land Vehicles mod is off have
no buggy, truck or kart there, whatever their own settings say.

## The map-data relay

Earth worlds download map tiles with the system's `curl` over HTTPS. Machines
whose `curl` is too old for today's HTTPS (Mac OS X 10.6, for example) fetch
through a VoxelRust server instead. This is automatic: when curl fails its
secure connection and no relay is set, the game uses the project's public
server, 195.62.47.178:25590. The game logs this once.

To use another server (for example one on your own network), run
`voxelrust --server` on a machine with working internet and a modern `curl`
(any server will do, including a game hosted from the menu), then set SETTINGS
> GAME > MAP DATA RELAY to its `host:port`, for example `192.168.1.20:25590`.
A change applies at once; blank the field to go back to automatic.

The game then gets elevation tiles, map tiles and place names through the
server. Tiles are cached on both machines. Every map request reaches the
relay's operator, so use a server you trust.

## Hosting for people outside your network

- Forward TCP port 25590 (or the one you chose with `--port`) on your router
  to the machine running the server. The game does not do this for you.
- Give your players your public address.
- There is no server list. The project runs one public server, which you reach from the multiplayer screen.

Be clear about what the server does **not** have:

- **No accounts and no passwords.** Anyone who can reach the port can join
  under any free name, build and break anything, and use the relay.
- **No encryption.** Positions, chat, skins, edits and chest contents travel
  as plain data.
- **Little moderation**: operators can `/kick` and `/ban` by name, but there
  are no protected areas.

Host for people you trust, keep backups of the two server files, and close
the port when you are done.

## Saved servers

The MULTIPLAYER screen lists the servers you have added, up to 64. Six show
at a time; scroll with the mouse wheel or the bar at the side. Each row shows
a server's name and address:

- Click a row to join that server.
- **EDIT** opens the server on the ADD SERVER screen, to change its name or
  address.
- **X** removes the server from the list.

**ADD SERVER** opens a screen with SERVER NAME and SERVER ADDRESS boxes. The
address is `host` or `host:port`, and spaces in it are ignored. **SAVE** keeps
the server and returns to the list; **SAVE AND JOIN** keeps it and joins;
**CANCEL** goes back without saving. Tab moves between the two boxes, Enter in
the address box saves, and CTRL+V pastes an address. A server saved without a
name is called `server` and a number. Saving from EDIT replaces that entry;
saving from ADD SERVER adds a new entry, even if the name is already listed.

The list is `servers.txt` in the data folder, one `name address` pair per
line.

## Operators and server commands

A server has operators: players who may use its commands. The first player
ever to join a world's server becomes one (the host, when hosting from the
menu); operators make others. Commands are typed in chat like any other;
what the game itself does not know is passed to the server.

| Command | Who | What it does |
|---|---|---|
| `/list` | anyone | who is online (operators are starred) |
| `/help` | anyone | the server's commands |
| `/op NAME`, `/deop NAME` | operators | make or unmake an operator (the last one cannot be removed) |
| `/kick NAME` | operators | remove a player |
| `/ban NAME`, `/pardon NAME` | operators | keep a name out, or let it back |
| `/say TEXT` | operators | a message from the server to everyone |
| `/time 0..1` | operators | set the time of day (0.25 is noon) |
| `/perm NAME [+build -build +gamemode -gamemode +commands -commands +voice -voice]` | operators | show or change what one player may do; `/default [...]` sets it for everyone without an entry of their own |
| `/mute NAME`, `/unmute NAME` | operators | take away or give back a player's voice chat (kept in the permissions file) |
| `/voicerange BLOCKS` | operators | how far a voice carries (2 to 128, 24 by default); with no number, shows it |

Operators and bans are kept in `<world>.server-ops.txt` beside the world.
Names are not protected by passwords: anyone can join under any free name,
so treat this as housekeeping among friends, not security.
