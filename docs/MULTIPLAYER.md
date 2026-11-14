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
| `--no-update` | do not keep the server up to date (see below) |
| `update_check_seconds=N` (in `settings.txt` in `--data`) | how often the server looks for a new release: 90 by default, kept between 30 and 600 |
| `pvp=on\|off` and `spawn_protection_seconds=N` (in `settings.txt` in `--data`) | whether players may hurt each other (on by default; `/pvp` overrides it once used) and the seconds of protection after joining or respawning (5 by default, 0 to 60); see [Player versus player](#player-versus-player) |
| `--update-mirror URL` | fetch updates from this web folder over plain HTTP, tried before the release (or `update_mirror=URL` in `settings.txt` in `--data`) |

The server prints who joins and leaves, chat, and relay requests. Stop it
with Ctrl+C.

A server keeps itself up to date with the same updater as the game (see
[BUILDING.md](BUILDING.md#how-the-game-updates-itself)): every
`update_check_seconds` (a setting in `settings.txt` in `--data`: default 90,
30 to 600) it looks at the signed release list, and installs a newer program.
It then warns the players in chat (RESTARTING FOR AN UPDATE IN 30 SECONDS),
saves, and restarts into it (at once if nobody is connected); `run.sh` of the
server package puts the old program back if the new one stops within 20
seconds. How it is set up, and how releases are published, is in
[DEPLOY.md](DEPLOY.md). What it reads is signed: the list of programs
(`nightly.txt`) must carry a valid `nightly.txt.sig` made with the project's
key, and each program must match its listed SHA-256, or nothing is installed
(the server logs `update: the release list's signature is not valid`). A
server that cannot reach GitHub (or sits behind a plain-HTTP-only path) can
take the same files from a mirror with `--update-mirror`; the mirror needs no
trust, because everything it serves is checked.

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
  Without a port the game uses 25590. An IPv6 address goes in brackets:
  `voxelrust --join [::1]:25590` (or just `[::1]`). A port is a number from 1
  to 65535; `host:0` or `host:+80` is refused.

Your name is the **YOUR NAME** box on the MULTIPLAYER screen, the same name
as in SETTINGS > GAME. It takes letters, digits, `-` and `_`, 16 at most; an
empty name becomes `Player`. If someone on the server already has it, a
number is added (`Bob` becomes `Bob2`).

The server refuses a player whose game speaks a different version of the
network protocol (currently version 14; a server also lets in version 13 games, see below and [ACCOUNTS.md](ACCOUNTS.md)), and anyone beyond the player limit.
Only the protocol version decides: the build name (`linux-server-x86_64` for
the public server, `linux-x86_64`, `windows-x86_64`, `macos-...` for games) and
the release number do not, so a game and a server of different builds or
nearby releases play together. A refused player is told both protocol versions
and which side must update ("your game is too old: update it" or "this server
is too old for your game: its owner must update it"). After joining, the
server says which program it is (its release number, build name and commit),
and the game adds a line when the two releases differ. `/version` shows both,
in the game (your own and the server's) and as a server command anyone may use.

## What is shared, and what is not

Shared between everyone on a server:

- **Players**: position, the direction they look, name tags, and skins; what
  their body is doing (a pose byte with every position: standing, crouching,
  sitting, lying, swimming, climbing, falling, riding, and whether they are
  drawing a bow, eating or striking), and the **emote** they are playing
  (`Msg::Emote`, the number of the emote in the server's pack; 0 stops it).
- **Block edits**: every block placed or broken, including everything built
  before you joined.
- **Chat**, and the last ten lines of it for anyone who joins.
- **Health, hunger, armour and ping** of every player (next section).
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
player who came first of those present. The server names them to everybody
when someone joins or leaves; when that player leaves, the next to have come
takes over (the game that takes over carries on with the creatures it had
been shown). A server that names nobody (an older one) is taken to mean the
lowest player number. Vehicles are run by the code of the
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

### Health, armour, the player list and what the server remembers

Every player's game tells the server how it is: health and most health (up
to 100, in half hearts), hunger, and the item in each of the four armour
slots, at most four times a second and every two seconds even when nothing
changed. The server checks the numbers (a most health from 1 to 100, health
not above it, hunger 0 to 20; anything else is ignored), adds the player's
ping and passes it on. Games keep the latest of each player and show it:

- **Hearts under the name tag** of players within 24 blocks: ten small hearts
  to a row, more rows below for more than 20 health.
- **The armour** other players wear is drawn on their models.
- **The player list**: hold **Tab** in the game (a shared game only) for a list
  of the players with a health bar, health and ping in milliseconds. The
  server asks every five seconds and the game answers at once; a game that
  does not know this is not asked and shows no ping.

Health belongs to each game, as before: the server does not heal or hurt
anyone and does not check how fast health rises. Two things are the
server's: **a death** (the dying game sends "I died" with its cause; the
server says `NAME died` / `NAME fell from a high place` / ... in chat once,
until that player is well again), and **memory**: the server writes the last
health, most health and hunger of each player (by account, or by name for a
guest) next to the place they left, in the `.server-spots.txt` file
(`name x y z yaw pitch health max hunger`; older lines with no health still
load). When the player joins again their game is given that health and
hunger. A player who left dead comes back at the world's spawn at full
health instead, like after a respawn.

All of this is one state channel (7, `channel::VITALS` in `src/net/proto.rs`;
the code is in `src/net/sync/`), so the protocol version did not change: an
older game ignores the messages, and an older server just passes them on.

### Joining, leaving and joining again

What a server tells a player who joins, in this order: the world (seed, time of day), the
program's version, the content pack, the mod list, the last ten lines of chat,
who runs the creatures, then each player here with their skin, position and
pose, status effects, health and armour, and the emote they are playing, then
every block change, the kept chest contents, and, for a player who has been
here before, where they were and how they were. Time of day is sent again every five seconds
and the weather follows from the seed and the time, so everyone has the same
sky. The skin is sent again on every join, and the skins of players who
left are forgotten.

- **Coming back before the old connection has timed out.** A player who
  joins again under the same account replaces their old connection. A guest
  (no account) who joins again from the same address under the same name,
  when their old connection has said nothing for 2.5 seconds, replaces it
  too: they keep their name (it does not become `Bob2`) and their place on a
  full server. From another address, or while the old connection is still
  talking, the name is taken as before and the newcomer gets a number added.
  The server looks again at the player limit once the old connection has
  made way.
- **A player who says nothing for 60 seconds is dropped** (a game says
  something at least every two seconds, also while it loads the world).
- **Two players changing the same block within a moment**: the one who came
  last wins on the server, and both are told, so that nobody is left with the
  other's block.

### Not shared

- **Inventories.** These are per player and are not kept
  by the server. You join with the starting items, and nothing you carry is saved when you leave.
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

## Player versus player

Players can hit each other. The server decides every hit; nothing about it
changes the protocol (the messages are state messages on channels 7 to 10,
which games from before it ignore).

**How a hit goes.** The attacker's game finds the player under the crosshair
(a ray against the other players' boxes, 0.6 wide and 1.8 tall, within the
reach of the item in hand, at most 4.5 blocks; a block in front stops it) and
swings at once, with the hit sound. It sends the server "I hit player N" with
the item in hand and the blow it worked out: the same blow as against a
creature (the item's damage, 1 for a bare hand, then strength or weakness and
the `damage` attribute), at most one hit every 0.35 seconds. The server then
checks:

- the switch is on (`pvp=on`, the default; `/pvp off` turns it off for everyone
  and takes `pvp` out of every player's permissions) and both players have the
  `pvp` permission (`/perm NAME -pvp`; allowed by default; a player without it
  can neither hit nor be hit);
- both are in survival mode (creative players are immune and never hit
  anyone) and alive;
- the target is another player who has joined, whose game takes part (see
  below);
- the target has no **spawn protection**: nobody can be hurt in the first 5
  seconds after joining or respawning (`spawn_protection_seconds` in
  `settings.txt`, 0 to 60);
- from the attacker's eyes to the target's body is at most **5.2 blocks**
  (reach plus room for lag), by the last positions each sent;
- the attacker's last hit that counted was at least 0.35 seconds ago (0.04 is
  forgiven, for messages that come in a bunch).

A hit that fails any of these does nothing, silently. A hit that passes is
cut to what the weapon allows (at most four times its damage plus 4, never
over 40: a changed game cannot claim more), and the **target's armour** is
taken off it with the game's own armour maths (`Armour and wear` in
PLAYING.md). The target's game tells the server its defense points and toughness
(clamped to 20 and 8 here) and whether it is alive in a gear message whenever
they change, at most four times a second and at least every 10 seconds, so that
armour cannot be claimed for more than the most a player can wear.

The server then tells the target "you were hit for N by NAME" (and which way
to be thrown, away from the attacker) and tells everyone within 64 blocks, the
attacker included, who was hit (the red flash of the model for 0.35 seconds
and the hurt sound). The target's game takes the damage from its own health (a
player owns their health: the server never changes it), with the red screen
vignette and the hearts as for any blow, throws the player back, and wears
the armour. Friendly fire is on: there are no teams.

**Deaths.** When a player's game says it has died and it was hit by another
player in the last 15 seconds, the server tells everyone "A was slain by B"
and counts a kill for B (`/kills`). Other deaths say nothing here.

**Older games** (protocol 13, or 14 games from before fighting) never send
the gear message, so they are not fighting players: they can hit no one
and take **no damage** from anyone. A server that only has such games never sees
a fight.

**The swing.** Clicking the attack button with nothing in reach swings the arm
too (a quarter of a second between swings; it never delays mining). Other
players see your arm swing, 0.3 seconds, whenever the pose byte of your
position messages has its swing flag: right arm back, then forward and up,
the forearm following.

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

Mods are ordinary packs here: the ones found and switched on where the
server runs (in `<server data folder>/mods`) are sent with the rest, so players
on a server whose Land Vehicles mod is off have no buggy, truck or kart there,
whatever their own settings say.

## Mods from the server

No mods come with the game, so a server can also hand its mods to players as
whole folders (textures, models, sounds, scripts and all), which is what makes
a modded server joinable without installing anything first.

**What the server offers.** Every mod in the server's mods folder
(`<server data folder>/mods`, or `--mods-dir DIR`) that is switched off
nowhere, has no native code (`code/mod.rs`) and does not say `client no` in
its pack.txt. `--serve-mods a,b,c` offers exactly the named mods instead
(`--serve-mods all` goes back to the default). The same options apply to a
game hosted from the menu. The server logs what it offers and what it left
out and why.

**What happens when you join.** After the pack the server announces its
mods: name, version (the pack's `version` line) and a checksum of each. The
client keeps what it already has from this visit if name, version and
checksum match, and asks for the rest; the server then sends each asked mod
in pieces, a few per tick, so play is not stalled. The loading screen shows
`DOWNLOADING MODS 3/12`. Each mod is checked against its checksum, and
every file in it is checked before anything is written (see below).

**Where it goes.** `<data folder>/tmp/mods/<host>_<port>/<mod>/...`, next to a
`manifest.txt` that says which mods arrived whole. The mods are loaded for
that visit only, never in a world of your own, and SETTINGS > MODS lists them
as FROM SERVER, read only. The folder is emptied when you leave the server,
when the game quits, and (for what a crash left) when the game starts. If you
join a server again before that, unchanged mods are not downloaded twice.
Two copies of the game started with the same data folder share this folder: one
starting up clears the other's downloaded mods.

**What is refused.** The client accepts only files of these types: `.txt`
`.json` `.bbmodel` `.png` `.jpg` `.gif` `.bmp` `.tga` `.wav` `.ogg` `.mp3`
`.flac` `.rs` `.md`. A path may not be absolute, hold `..`, a drive letter,
a backslash or other odd characters, start a name with a dot, be longer than
160 characters or nested more than 8 deep. A mod is refused whole if any file
breaks that, if a name repeats (ignoring case), or if it breaks the limits:
2000 files, 4 MB a file, 16 MB a mod, 48 MB and 64 mods from one server.
Mods that contain native (compiled) code cannot be downloaded, since the
player's program has only the code it was built with: the server skips them
with a log line, and the player needs a program built the same way
(`--features native-mods`, see [BUILDING.md](BUILDING.md#mod-code)) to use
the code of such a mod.

**Scripts.** `.rs` files in a downloaded mod are handed to the script engine
(`content::mod_runtime`, see [MODDING.md](MODDING.md#scripts-that-mods-bring));
without an engine they are kept and not run.

(Version 9 added poses and emotes.) The protocol: version 14 adds big (HD) skins in pieces: `SkinCaps` (server to client, after `Info`: the largest side, 4096, and the most bytes, 1 MiB, that the server takes; a client that never gets it sends 64 x 64 skins only), `SkinBegin` (a skin of `size` bytes of PNG and `side` pixels follows) and `SkinPart` (up to 40 000 bytes, `last` ends it); see `net/skinxfer.rs` and `net/skinslot.rs`. A server of version 14 still lets in clients of version 13 and never sends them a message they do not know: it keeps the skin as sent and a 64 x 64 version made from it, and sends version 13 clients the latter as a plain `Skin` (at most 24 000 bytes, as before). Version 14 also adds accounts (see [ACCOUNTS.md](ACCOUNTS.md)): after `Hello` a server may send `AuthChallenge` (kind 44: what it is for, flags, how many rounds the PIN is hashed, tries left, the account's salt, a salt for a new key, a fresh 16-byte nonce) and wait up to 15 seconds for `AuthLogin` (40: the HMAC-SHA-256 proof) or, for a new account, `AuthRegister` (41: the key); a player already in the game asks with `AuthAsk` (43) and answers a `/pin` challenge with `AuthChange` (42); the server's `AuthResult` (45) says how it went and `Account` (46: its mode and whether you are logged in) follows `Welcome`. A server lets a version 13 client in as a guest (a name without an account) and sends it none of these. Version 13 lets the creature snapshot carry, after a creature's ten fields, its own size, health limit, tilt, bone overrides and a state text of up to 64 bytes (see `game/creature_wire.rs`; a creature with none of these has the old line, and in between full sets a creature whose extras did not change says `|=`), and adds the mods' message channel: `State` on channel 5, key `mod\x1fchannel`, data a receiver number (two bytes, `0xFFFF` for everyone else) and up to 512 bytes of text; the server drops messages with a bad key or text and from a player who sends more than 8 a second (never kept, never relayed to the sender), and passes a message for one player to that player only (`net/modmsg.rs`). Version 12 adds `Info` (server to client, right after `Welcome`: build name, release number and commit, for showing only) and fixes `State` keys, which lost the separator of the script channel's `mod\x1fkey`; the biggest `State` is now 59 000 bytes (it was over the message limit). Version 11 adds `Effects`: the status effects on a player (id, level and whole seconds left for each, 16 at most), sent by that player's game whenever the set changes, relayed with their id, and told to players who join later. It is for show (the name tag) and for mods; each game keeps its own effects. Version 10 adds `ModList` (server to client, after the pack),
`ModsWant` (client to server) and `ModPart` (server to client, one mod's
bundle in pieces of up to 40,000 bytes). A mod's bundle is the
`save::bundle` format with the paths inside the mod as names.

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
| `/version` | anyone | the release, build name and protocol version of the game and of the server |
| `/help` | anyone | the server's commands |
| `/op NAME`, `/deop NAME` | operators | make or unmake an operator (the last one cannot be removed) |
| `/kick NAME` | operators | remove a player |
| `/ban NAME`, `/pardon NAME` | operators | keep a name out, or let it back |
| `/say TEXT` | operators | a message from the server to everyone |
| `/time 0..1` | operators | set the time of day (0.25 is noon) |
| `/perm NAME [+build -build +gamemode -gamemode +commands -commands +voice -voice +pvp -pvp]` | operators | show or change what one player may do; `/default [...]` sets it for everyone without an entry of their own |
| `/mute NAME`, `/unmute NAME` | operators | take away or give back a player's voice chat (kept in the permissions file) |
| `/voicerange BLOCKS` | operators | how far a voice carries (2 to 128, 24 by default); with no number, shows it |
| `/pvp on`, `/pvp off` | operators | allow or stop fighting between players (kept in the permissions file); `/pvp` alone, for anyone, shows it |
| `/kills [NAME]` | anyone | the kills of a player since the server started (not kept) |

Operators and bans are kept in `<world>.server-ops.txt` beside the world.
A name is free for anyone unless it has an account: a name can be protected by
a 4 or 6 digit PIN (`/register PIN PIN`), and a server can name administrators
and ask every player for an account. That stops casual impersonation and no
more: read [ACCOUNTS.md](ACCOUNTS.md) before trusting it with anything.

| Command | Who | What it does |
|---|---|---|
| `/register PIN PIN`, `/pin OLD NEW`, `/forgetpin` | you, in the game | make an account for your name, change its PIN, make the game forget a saved PIN (typed in the game, never sent as chat) |
| `/accounts` | administrators | the accounts (names only) and who is locked out |
| `/resetpin NAME [PIN]` | administrators | set a new PIN for an account (a random six digits if none is given; the answer shows it to you) |

Administrators are the names in the server's `admins=` setting; they have every
permission, use every command above, and are starred in `/list`.


## Limits a server keeps (for hosts)

A server does not trust what it is sent. Per connection, messages over their
allowance are dropped (and a connection that keeps sending far too much is
cut): positions, emotes and effects 100 a second, block changes 1500 a second
(6000 at once, for explosions), chat 2 a second (8 at once), shared state 300 a
second, skins one every two seconds (and in fact a change of skin only once in five seconds), and the pieces of a big
skin 30 a second. Voice has its own limit. Also:

- a skin is checked before it is kept: its PNG header must say a square of a multiple of 64 up to 4096, the picture
  must decode at that size, and at most 1 MiB of PNG is taken, one transfer at a time per player (a transfer that stops
  for 20 seconds is dropped);

- at most 24 connections that have not said hello yet, and nobody stays
  connected without a word for a minute;
- the server keeps at most 600 000 changed blocks and 8 MB of kept state (chests);
  a chest's contents are only taken from players who may build, and the
  creature snapshots only from the player who runs the creatures;
- player numbers are never reused while in use, and names that differ only in
  capital letters count as the same name; a ban is checked against the name as
  the server knows it;
- a server's list of operators, bans and permissions, and its saved changes,
  are written whole or not at all, and tried again if the disk is full;
- mod files from a server may not be named like a Windows device (`con`,
  `nul`, `com1`...), and a downloaded mod may not be called `mods` or `update`.
