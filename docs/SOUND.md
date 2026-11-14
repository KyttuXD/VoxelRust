# Sound

How the game makes and mixes sound, and why it does not loop. The sound
lines in the data files (what each sound is made of) are in
[DATA.md](DATA.md#sounds); the volume settings are in
[PLAYING.md](PLAYING.md) (SETTINGS > AUDIO).

There are no sound files in the game. Every effect is made while it plays
from noise and a gliding tone (a pack may add recordings), and mixed in
SDL's audio callback: one channel, 22050 Hz, 16 voices.

| Part | Where |
| --- | --- |
| Mixer, `Audio`, variation on each play | `src/client/audio.rs` |
| Ambient beds, single sounds, thunder, carrying out the plan | `src/client/audio/ambient.rs` |
| Grains: overlapping noise with equal-power crossfades | `src/util/audio/grains.rs` |
| Beds: grains plus filters, one per kind of ambient sound | `src/util/audio/bed.rs` |
| The plan: levels, muffling, intervals, thunder | `src/util/audio/ambience.rs` |
| No-repeat chooser, pitch and volume jitter, rate limits | `src/util/audio/variation.rs` |
| Soft limiter | `src/util/audio/limiter.rs` |
| What is around the player (roof, water, lava, fire) | `src/game/ambience_probe.rs` |

## Continuous sounds are beds, not loops

Rain, wind, running water and waves, lava, fire and the air of a cave are
**beds**: six of them live in the mixer and each plays all the time while
it is wanted. A bed is never a clip, so there is no seam and nothing that
comes round again.

- A bed is **granular noise**. Its noise is cut into grains 0.25 to 2.2
  seconds long (the recipe in `bed.rs` sets the lengths). Each grain has its
  own noise, so its own random start; its own pitch within 4 percent (the
  noise is read a little faster or slower); its own volume within 2 dB; and
  its own place between left and right (the mixer has one channel, so the
  place only changes how loud the grain is, between 0.71 and 1; the stereo
  path, `Granular::next_stereo`, is there for when it has two).
- The next grain starts while the one before it fades out, over the same
  number of samples. The weights are equal-power: `sin` going up and `cos`
  coming down, so their squares add to exactly 1 at every sample. Loudness
  never dips or jumps where grains meet. A fade is at most a quarter of its
  grain and a third of the shortest grain, so a grain has finished fading
  in before it starts to fade out.
- The grains go through two one-pole low-passes in a row (the soft top: for
  rain it starts at 6 kHz, so there is no hiss) and a second low-pass for
  body, mixed in beside it (a gentle low shelf: 350 Hz for rain).
- The mixer allocates nothing: a bed is a few dozen numbers.

### What moves

`util/audio/ambience.rs` makes the plan, once a frame:

- **Fades.** A bed's level moves towards what is wanted at 1/2.5 per second:
  the weather changes over a couple of seconds, never in a step.
- **Breathing.** Each bed's volume, and how bright it is, ride slow random
  walks (a drift per square-root second with a pull to the middle, kept
  inside a range: 0.85 to 1.12 for rain, 0.6 to 1.3 for waves, which is
  the swell). The wind also gets gusts: a raised-cosine swell of 2 to 5.5
  seconds every 4 to 12 seconds that makes it louder and brighter.
- **Single sounds on top**, at random intervals (a quarter to one and three
  quarters of the mean gap, so never on a beat): rain drops (0.4 a second
  in a light rain, about 9 in a downpour), drips off a roof when it rains
  and you are under one, bubbles in water, pops and bubbles in lava,
  crackles in fire (and now and then a bigger pop), drips with an echo in a
  cave. They are quiet, soft (no tone above 4 kHz) and they never take a
  voice that is playing something else.
- **Muffling.** Under a roof, a bed's low-pass drops by 82 percent and rain
  is 45 percent quieter; deep in rock it is dull and the wind is gone; with
  the head under water everything is a 380 Hz wash and you hear the water
  bed. The surroundings arrive every 0.4 seconds, so the plan blends them
  (0.7 seconds) and a step into a house is a short muffling, not a click.
  If the game stops reporting (a menu is open) the ambience fades away.

### What the game tells it

`game/ambience_probe.rs` looks at the blocks every 0.4 seconds
(`Session::weather_update`) and builds an `Env`:

| Field | How |
| --- | --- |
| `shelter` | Five columns (the player's and four 2 blocks out) are searched 48 blocks up; solid blocks count 1, leaves 0.35, water 0.6. |
| `cave` | The same columns: how many solid blocks are above (3 is nothing, 10 or more is deep). A house roof is thin; a cave is not. |
| `underwater` | The block at the head is water. |
| `wind` | Open sky and height above sea level. |
| `water`, `lava` | Blocks of that liquid on a 5 by 5 grid within 8 blocks, at the feet and just below (water) or two below (lava). |
| `fire` | A block named `fire` or tagged `tag fire` (the campfire) within 4 blocks, nearer counts more. |
| `rain`, `snow`, `storm` | The weather where the player is. Snow plays no rain bed, only wind. |

## Thunder

A strike is some 25 to 265 blocks away. The flash is at once; the sound
comes `distance / 80` seconds later (up to 4). There are four takes (different
length, pitch, and dullness) and one is never followed by itself. A near
strike has a short crack in front of the rumble; far ones are only a rumble,
longer, quieter (down to 30 percent) and duller (low-pass about a third). A
softer roll follows 0.7 to 1.6 seconds later. Under a roof it is 40 percent
quieter and duller. A `/strike` or a script playing `thunder` goes the same
way (as if 30 blocks off). Thunder follows EFFECTS, not AMBIENCE.

## Sounds that repeat

Every play of a sound goes through `Audio::play_named`:

1. **A different take.** A made sound has four takes (its pitch and
   brightness moved by 0 to 6 percent); a sound with `<id>_v2.wav`,
   `<id>_v3.wav` files has those. A take is never followed by itself
   (`NoRepeat`).
2. **Jitter.** Pitch within plus or minus 6 percent and volume within plus
   or minus 1.5 dB, at random, for every sound including a mod's recordings.
3. **Limits** per sound (`RateLimiter`): a minimum gap between plays and a
   most at once. Footsteps: 0.14 seconds, 2 at once. Interface: 0.03 seconds,
   3. Others: a fifth of their length between 0.05 and 0.35 seconds, 3 at
   once. A play that would break the limit is dropped, not queued.
4. **Gentle highs.** The noise low-pass of a made sound is held to 0.6
   (above that is a hiss), tones above 1.8 kHz are let down in proportion,
   and the data lines of the sounds that were bright (`click`, `pop`,
   `glass`, `bow_load`, `hiss`, `chirp`, `rattle`, `chime`, `sizzle`) are duller
   and a little quieter.
5. **Distance and rooms.** A volume below 1 means the sound is far. It falls
   off along an eased curve instead of a line, goes duller (the low-pass of
   a made sound, a one-pole filter on a recording), and a far sound is
   quieter again under a roof. Sounds at full volume are not touched.
6. **Soft limiter.** The sum of voices and beds is summed wide (32 bits) and
   bent by `soft_limit`: untouched up to 18000, then a smooth curve that
   approaches 32000 and never reaches full scale. Many loud sounds at once
   are a louder, denser sound, not a crackle.

## Volumes

MASTER, EFFECTS, FOOTSTEPS and INTERFACE work as before (each sound names
its bus in its data line). AMBIENCE VOLUME (`ambience_volume`, 0 to 10, 6 to
start with) scales the beds and their single sounds, and is multiplied by
the master volume. Thunder follows EFFECTS.

## Memory

No sample bank is made. A bed is grains of noise generated sample by sample;
single sounds are the made sounds already in the game. The mixer has 16 voices,
6 beds (about 200 bytes each) and allocates nothing in the audio callback.

## Tests

The pure parts are tested in their files: the crossfade weights (squares
add to 1), grains overlapping with total power 1 over 200 thousand samples,
no repeat, the rate limiter, the random walks staying in range, the
limiter never reaching full scale, rain drop density rising with intensity,
fades taking a couple of seconds, muffling under a roof and under water,
thunder being later, quieter and duller with distance, and the setting
being saved. The sound itself has not been listened to by the person who
wrote this: numbers such as the scale of each bed (`bed.rs`) and the single
sounds' loudness (`ambience.rs`) are by calculation and are the first
things to tune by ear.
