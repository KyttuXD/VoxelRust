# Memory and CPU

The game aims to stay under about **300 MB** without mods and about **700 MB**
with mods (even a big pile of heavy ones), and not to burn a core when there is
nothing to do. This page says where the memory goes, what the game does about
it, and how to read the numbers.

## What takes memory

| Kind | What | Size |
| --- | --- | --- |
| chunks | the blocks of every loaded chunk (16 x 256 x 16, two bytes a block) | 128 KB each |
| meshes | vertex buffers of chunks and distant terrain | depends on terrain; the biggest part at long distances |
| textures | the block atlas, creature and model pictures | the atlas is about 1.4 MB; each model picture is its own size |
| audio | decoded long sounds | 22050 samples a second, 44 KB a second |
| models | parsed model definitions | small; the model files are not kept (see below) |
| entities | creatures, dropped items, arrows | about 1.5 KB a creature |
| other | the program, the registry, fonts | about 60 MB (an estimate) |

The view distance is what matters most: chunks are loaded in a circle, so the
number of them grows with the square of the distance (view 8: about 200
chunks, 26 MB; view 16: about 800, 100 MB; view 32: about 3200, 400 MB).

## What the game does about it

Nothing is held that can be read again.

- **Sounds.** A mod's sound files are decoded when they are first played, not
  at start-up. Short ones (up to one second: footsteps, clicks) stay decoded;
  long ones are freed again when memory is short (and decoded again when they
  next play). A sound playing right now is never freed.
- **Music.** A track a mod plays (`play_music`) is never decoded whole: its
  compressed file (up to 16 MB, a few MB for a five minute Ogg or MP3) is held
  while it plays and a block at a time is decoded into a ring of 128 KB per
  lane; both are freed when the music stops or the world closes. Counted in
  `Audio::clip_bytes`.
- **Model files.** The text of a `.bbmodel` is read when the model is first
  drawn and let go once it is parsed; the picture inside it is let go once it
  is on the graphics card. A model not drawn is never read at all.
- **Chunks.** Only the chunks that are drawn, and the ring around them that
  meshing needs, are kept (a circle, not the square around it, which saves a
  fifth at long distances). Chunks far behind are dropped.
- **Saved regions.** At most 12 region files are held in memory (the least
  recently used go; unsaved ones are written first).
- **The allocator.** On Linux the C library's memory is returned to the system
  after a world is closed, after sounds are freed, and once a minute, and the
  worker threads share four arenas instead of each having their own.
- **Workers.** One fewer than the number of cores (at least one, at most six).

## The budget

Once a second the game adds up what it holds, by kind (the estimates in the
table above), and compares the total with the limit: 300 MB, or 700 MB when
mods are installed (or the server brought its own content). When it is over,
it gives things up **in this order**, one step every few seconds (every second
when far over) until it is under again:

1. drop the decoded sounds that are not playing;
2. simplify distant terrain: coarser sampling, fewer real-block tiles, fewer
   trees (three steps);
3. shorten distant terrain, down to off (three steps, each halving it);
4. shrink the view distance, one chunk at a time, down to 3.

Your settings are not changed: the game only uses less than they say. When
memory has been under 60 per cent of the limit for 45 seconds, one step is
taken back (in the opposite order), and so on until everything is as you set
it. The game never stops or crashes because of the limit; if there is nothing
left to give up it carries on as it is.

The numbers are estimates (good to about 20 per cent), not what the operating
system reports.

## Reading the numbers

- **F3** shows two more lines: the total against the limit and what is held
  back (`detail -1 far -0 view -0`), then each kind in megabytes.
- **`--log FILE`**: every 30 seconds a line like
  `voxelrust: memory 212/300 MB (70%): chunks 90, meshes 60, textures 12, audio 0, models 3, entities 0, other 60, held back: none, trims 0, steps 0`,
  and a line each time the budget gives something up or takes it back.

## CPU

- The title screen and every other menu draw at most 60 frames a second, even
  with vsync off.
- A window that is not in front draws at most 20 frames a second, and a
  minimised or hidden one 5. (A game in progress pauses by itself when the
  window loses focus.)
- Workers wait for work without spinning; a dedicated server sleeps between
  ticks.
- Scripted runs (`--screenshot`, `--bench`) are not slowed down.

## For mod authors

You do not have to do anything. Keep long sounds short (12 seconds is the
limit) and model pictures no bigger than they need to be: a 1024 x 1024
picture is 4 MB on the graphics card, and 100 of them are 400 MB. The sound
and model files must stay where they are while the game runs, because they are
read again when needed (a mod inside a `.zip` works too).
