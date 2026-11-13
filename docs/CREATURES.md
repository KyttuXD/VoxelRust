# Creatures

The game has 75 kinds of creature: farm animals, pets, wildlife, things that
fly and swim, night monsters, cave dwellers and villagers. All of them are
declared in [data/wildlife.txt](../data/wildlife.txt) (written by
`tools/gen_creatures.py`), so a pack can change any of them or add more; the
format is in [DATA.md](DATA.md#creatures).

## How they appear

- About every second and a half the game tries to add one creature somewhere
  18 to 44 blocks from you, on ground that suits it. At most 22 are alive
  around you at once; one that ends up more than 96 blocks away is removed.
- What appears depends on the **biome**, on whether it is **night**, and on
  whether the spot is **open water** (only swimmers appear there, and they
  appear nowhere else).
- When you are more than 7 blocks below the surface, most new arrivals are
  **cave dwellers**, chosen by the kind of cave around you.
- **Hostile creatures only exist for survival players.** In creative mode
  none appear, and switching to creative removes the ones present.
- Herd animals arrive in twos and threes. Kinds that have young sometimes
  bring one along.
- Every kind comes in several looks, with their own colours and patterns
  (a black-and-white, brown or red cow; a pink, spotted or black pig; a
  white, black, grey or brown sheep; a pony; a rooster; villagers of every job
  in four faces); one is picked for each creature. The pictures are drawn in
  the OCD style by `tools/entity_textures.py`, and a texture pack can
  replace them (see [TEXTURING.md](TEXTURING.md#creatures-and-players)). F3
  shows how many creatures are around you.

Wild creatures and villagers are not saved with the world: when you come
back, new ones appear.
The exception is [pets](#taming-and-pets), which are saved. In multiplayer
everyone sees the same creatures; one player's game runs them for all (see
[MULTIPLAYER.md](MULTIPLAYER.md#what-is-shared-and-what-is-not)).

Creatures behind the camera are not drawn. Many kinds have a call (a cow
lows, a wolf howls, a skeleton rattles) that you hear now and then when one
is within 24 blocks; SETTINGS > AUDIO > SUBTITLES FOR SOUNDS names them on
screen.

## Dealing with them

| Action | What happens |
| --- | --- |
| Left click | Hit it. A bare hand does 3 damage, a sword 6 to 15 (see [PLAYING.md](PLAYING.md#swords)); bows hit from a distance (see [PLAYING.md](PLAYING.md#bows)). In survival a killed creature leaves its drop; in creative one hit removes it and gives nothing |
| Right click | Tame it (holding its favourite food), ride it (a tamed horse, donkey, camel or elephant, with an empty hand), tell a pet to wait or follow, collect what it gives (milk, wool, an egg), trade, or hear what it has to say |
| Hold its favourite food | Some animals walk after you while you hold it, from up to 10 blocks away |

### Milk, wool and eggs

Right click the animal. It hands over one item, then needs time before it
has another; until then the game says `NOTHING YET - TRY AGAIN LATER`. Young
animals give nothing.

| Animal | Gives | Ready again after |
| --- | --- | --- |
| Cow, Highland Cow | Milk | 3 minutes |
| Goat, Yak | Milk | 3 minutes 20 seconds |
| Sheep | White Wool | 4 minutes |
| Merino Sheep | White Wool | 2 minutes 30 seconds |
| Llama | White Wool | 5 minutes |
| Chicken | Egg | 2 minutes 30 seconds |
| Duck | Egg | 3 minutes 20 seconds |
| Goose | Egg | 3 minutes 40 seconds |

The animal is not harmed. Milk and eggs are used in cooking (see
[PLAYING.md](PLAYING.md#the-oven-and-baking)); four String also make one
White Wool at a crafting table.

### Favourite foods

| Animal | Follows you for |
| --- | --- |
| Cow, Highland Cow, Sheep, Merino Sheep, Camel | Wheat |
| Pig, Donkey, Lop Rabbit | Carrot |
| Chicken | Wheat Seeds |
| Horse, Elephant | Apple |
| Dog | Raw Meat |
| Cat | Raw Fish |

### Taming and pets

Nine kinds can be tamed. Hold the animal's favourite food and right click
a grown one: the game says `THE ... IS NOW YOURS` and, in survival, one of
the food is used up.

| Animal | Tamed with |
| --- | --- |
| Dog, Wolf | Raw Meat |
| Cat | Raw Fish |
| Fox | Berries |
| Horse, Elephant | Apple |
| Donkey, Lop Rabbit | Carrot |
| Camel | Wheat |

A pet:

- follows you, and catches up by itself if you leave it more than 40
  blocks behind;
- no longer runs from you or attacks you;
- if it can bite (dog, wolf, cat, fox), goes for hostile creatures within
  12 blocks;
- waits where it is when you right click it with anything but its food in
  hand (`... WILL WAIT HERE`), and follows again after another right click
  (`... WILL FOLLOW YOU`). For the four you can ride, an empty hand means
  riding, so hold some other item to tell it to wait or follow;
- comes with you into other dimensions;
- is saved with the world, in `<world>.pets.txt`, and is there when you
  come back. On a server pets are not saved.

### Riding

A tamed **horse, donkey, camel or elephant** can be ridden. Right click it
with an empty hand to get on.

- It faces the way you look. The movement keys (W A S D) move it, at its
  own speed: a horse is quick, an elephant is not.
- Jump makes it jump, and swim upward in water.
- The get-out key (F) gets you off.
- You also come off if the creature dies.

Dogs, wolves, cats, foxes and lop rabbits cannot be ridden. Packs can make
any creature rideable, flying ones included: on those, jump climbs and the
fly-down key (Left Shift) descends. See
[DATA.md](DATA.md#creatures) (`rideable`).

On a server, a ridden creature obeys, and a spawn egg hatches, only for
the player whose game runs the creatures (see
[MULTIPLAYER.md](MULTIPLAYER.md#how-creatures-vehicles-and-chests-are-shared)).
Riding has not been tested between two games.

### Spawn eggs

In creative mode the Eggs tab of the item list holds an egg for every
creature, drawn as the creature standing in half an eggshell. Right click
with one to set that creature down on the block you aim at. They are
ordinary items (`<id>_egg`, for example `/give wolf_egg 3`); in survival an
egg is used up when it hatches. No more hatch once 200 creatures are
about.

### Young

Young animals are smaller, have half the health of an adult, and grow up by
themselves in 4 to 15 minutes depending on the kind. They are named
`Young ...`. There is no breeding: young arrive with adults, and
`/spawn <id> 1 young` makes one (see [PLAYING.md](PLAYING.md#chat-and-commands)).

## Temperaments

Each kind has a behaviour and may have several traits on top.

| Trait | Meaning |
| --- | --- |
| flees | runs when you come within 7 blocks |
| herd | keeps near others of its kind |
| jumps | moves in hops |
| territorial | leaves you alone unless you come within about 4 blocks or hit it; then it attacks for 20 seconds |
| pack | when you hit one, every one of its kind within 18 blocks turns on you |
| hunter | chases and kills creatures marked as prey within 16 blocks |
| prey | what hunters chase |
| lazy | stands about most of the time |
| follows | comes after a player holding its favourite food |

Territorial and pack animals only attack survival players, like the hostile
kinds.

## Farm animals and pets

| Creature | Lives in | Notes |
| --- | --- | --- |
| Cow | plains, forest, savanna, mountain | herd; milk; drops 2 Raw Meat; follows Wheat |
| Pig | plains, forest, swamp | herd; drops 2 Raw Meat; follows Carrot |
| Sheep | plains, mountain, tundra | herd, prey; wool; drops 2 Raw Meat; follows Wheat |
| Goat | mountain, badlands | herd, hops, pack (hits back for 3); milk; drops 2 Raw Meat |
| Chicken | plains, forest, savanna | prey, flees; eggs; drops 1 Raw Meat; follows Wheat Seeds |
| Duck | swamp, plains, forest | herd, flees; eggs; drops 1 Raw Meat |
| Horse | plains, savanna | herd, flees; fast; drops 2 Raw Meat; follows Apple; can be ridden once tamed |
| Dog | plains, forest, savanna | follows Raw Meat; several breeds |
| Cat | plains, forest, savanna, desert | lazy, hunts small prey; follows Raw Fish |
| Turkey | forest, plains | herd, flees, prey; drops 2 Raw Meat |
| Goose | swamp, plains | herd, territorial (1 damage); eggs; drops 1 Raw Meat |
| Donkey | plains, savanna, badlands | herd; follows Carrot; can be ridden once tamed |
| Llama | mountain, badlands | herd, pack (hits back for 2); wool |
| Bison | plains | herd, pack (6 damage); 40 health; drops 4 Raw Meat |
| Yak | mountain, tundra | herd, pack (5 damage); milk; drops 3 Raw Meat |
| Camel | desert | herd; follows Wheat; can be ridden once tamed |
| Bull | plains, savanna | territorial (6 damage); drops 3 Raw Meat |
| Highland Cow | mountain, taiga, tundra | herd; milk; drops 2 Raw Meat; follows Wheat |
| Merino Sheep | plains, mountain | herd, prey; wool, ready sooner than a sheep's; drops 2 Raw Meat; follows Wheat |
| Lop Rabbit | plains, forest | flees, prey, hops; follows Carrot |

The horse, dog, cat, donkey, camel and lop rabbit can be tamed, and so can
the wolf, the fox and the elephant below; the horse, donkey, camel and
elephant can then be ridden; see [Taming and pets](#taming-and-pets).

## Wildlife

| Creature | Lives in | Notes |
| --- | --- | --- |
| Deer | forest, taiga, plains | herd, flees, prey; drops 2 Raw Meat |
| Rabbit | plains, forest, tundra, taiga, desert | flees, prey; drops 1 Raw Meat |
| Mouse | plains, forest, savanna | flees, prey |
| Elephant | savanna | herd, pack: hit one and the herd answers, 9 damage a blow; 80 health; follows Apple, tamed with Apple, and can then be ridden |
| Monkey | jungle | herd, flees |
| Penguin | tundra | herd |
| Turtle | swamp, desert, jungle | lazy |
| Crab | desert, swamp | territorial (1 damage) |
| Frog | swamp, jungle | hops |
| Wild Boar | forest, taiga, jungle | territorial, pack (4 damage); drops 3 Raw Meat |
| Wolf | taiga, forest, tundra, mountain | hunter, pack (4 damage); tamed with Raw Meat |
| Fox | forest, taiga, plains, tundra | hunter, flees from you; tamed with Berries |
| Bear | taiga, forest, mountain | territorial (6 damage); drops 3 Raw Meat |
| Polar Bear | tundra | territorial, hunter (7 damage); drops 3 Raw Meat |
| Tiger | jungle | **hostile** by day and night (7 damage); drops 2 Raw Meat |
| Alligator | swamp | territorial (6 damage); drops 2 Raw Meat |
| Snake | swamp, jungle, desert, badlands | territorial (3 damage) |
| Spider | forest, jungle, swamp | territorial (2 damage); drops 1 String |
| Worm | plains, forest, swamp, jungle | prey |
| Ant | desert, savanna, jungle, badlands, forest | herd |
| Beetle | forest, jungle, swamp, plains | |

In the air:

| Creature | Lives in | Notes |
| --- | --- | --- |
| Songbird | plains, forest, savanna, jungle, taiga | flees upward |
| Owl | forest, taiga | night only |
| Butterfly | plains, forest, jungle, savanna | |
| Bee | plains, forest | does not sting |
| Firefly | swamp, forest, jungle | night only; glows |
| Bat | caves | harmless |

In the water:

| Creature | Lives in | Notes |
| --- | --- | --- |
| Fish | seas and lakes at sea level | drops 1 Raw Fish |
| Squid | seas and lakes at sea level | drops 1 Raw Fish |

## Night monsters

These appear on the surface after dark, hunt survival players, and are gone
by morning (once you are more than 20 blocks away, or a little later if you
stay close).

| Creature | Where | Notes |
| --- | --- | --- |
| Lurker | anywhere | 3 damage; common |
| Skeleton | anywhere | an archer: shoots arrows (3 damage) from up to 20 blocks away, and hits for 2 up close; drops 2 Arrows; common |
| Zombie | anywhere | 4 damage, 18 health; slow; common |
| Wraith | swamp, forest, taiga, tundra, badlands | 4 damage; floats and glows faintly |
| Bone Hound | plains, savanna, desert, mountain, badlands | 3 damage; fast, hunts in packs |
| Werewolf | forest, taiga, mountain | 7 damage, 30 health; very fast, hunts in packs; uncommon |
| Ghoul | swamp, badlands, desert | 4 damage; quick |
| Ogre | mountain, badlands, taiga | 11 damage, 70 health; drops 4 Raw Meat; rare |
| Scarecrow | plains, savanna | 3 damage; drops 3 Wheat; uncommon |

A skeleton's arrows fly like yours: they drop, and a wall stops them. A
creature that flies and shoots (none in the base game; packs can add them,
a dragon for instance) circles overhead and shoots from above.

Hostile creatures find their way around walls and up steps. A bed lets you
skip the night (see [PLAYING.md](PLAYING.md#beds)).

## Cave dwellers

Caves come in seven kinds, each dressed with its own blocks (see
[PLAYING.md](PLAYING.md#caves)) and each with its own inhabitants. Cave
monsters are there by day as well as by night, in survival mode.

| Creature | Caves | Notes |
| --- | --- | --- |
| Bat | every kind | harmless |
| Slime | every kind | hostile, hops, 2 damage; small and big ones |
| Crawler | every kind | hostile, 4 damage; drops 2 String |
| Rock Golem | every kind | hostile, 9 damage, 60 health; drops 2 Crystal; uncommon |
| Cave Spider | webbed caves | hostile, 3 damage; drops 2 String |
| Frost Crawler | frozen caves | hostile, 4 damage |
| Magma Slime | magma caves | hostile, hops, 4 damage; glows |
| Glow Beetle | mushroom and lush caves | harmless; glows brightly |
| Blind Newt | lush and frozen caves | harmless |

A creature whose `biomes` list includes plain `cave` turns up in every kind
of cave; one that names only particular kinds stays in those.

## Villagers

Villagers appear, uncommonly, in plains, savanna, forest, taiga and desert.
Each keeps near the spot where it first stood (its home), wanders by day and
goes back there at dusk. There are no generated villages: villagers arrive
one at a time like other creatures, and what stands around them is what they
or you build.

**Trading.** Right click a villager to hear one of its offers; right click
again for the next. To accept, hold the thing it wants (enough of it) and
right click: the items are swapped at once.

| Villager | Takes -> gives |
| --- | --- |
| Farmer | 6 Wheat -> 2 Bread; 5 Carrot -> 1 Ruby; 1 Ruby -> 8 Wheat Seeds; 1 Ruby -> 6 Potato Seeds |
| Builder | 8 Oak Log -> 40 Oak Planks; 16 Cobble -> 12 Stone Bricks; 1 Ruby -> 12 Glass |
| Guard | 2 Ruby -> 1 Iron Sword; 3 Raw Meat -> 3 Cooked Meat |
| Baker | 4 Wheat -> 2 Bread; 1 Ruby -> 1 Cheese Pizza; 2 Egg -> 6 Cookie; 3 Apple -> 1 Apple Pie; 2 Milk -> 2 Cheese |
| Smith | 6 Iron Ore -> 1 Iron Pickaxe; 4 Iron Ore -> 1 Iron Hoe; 8 Coal -> 1 Ruby; 3 Ruby -> 1 Crystal Pickaxe |
| Trader | 1 Ruby -> 6 Bread; 1 Ruby -> 2 Oak Door; 2 Ruby -> 1 Red Bed; 2 Crystal -> 1 Ruby; 6 String -> 1 Ruby |
| Shepherd | 4 White Wool -> 1 Ruby; 1 Ruby -> 6 White Wool; 3 Milk -> 1 Ruby |
| Fisher | 4 Raw Fish -> 1 Ruby; 1 Ruby -> 5 Cooked Fish; 2 Ruby -> 1 Boat |

The **Wanderer** is not a villager: it has no home, walks along with you for
a while when you are near, and trades 1 Ruby for 6 Apples or 8 Berries for 1
Ruby.

**Work.** Three jobs do something in the world, by day, within a few blocks
of the villager's home:

- A **farmer** tills grass or dirt into farmland (up to a dozen squares),
  sows a crop on it, and when a plant is ripe harvests and replants it. The
  harvest is not handed to you.
- A **builder** puts up a small hut a few blocks from its home, one block at
  a time: a 5 x 5 plank floor, plank walls with a doorway and glass windows,
  and a plank roof.
- A **guard** goes after hostile creatures within 18 blocks and strikes them
  (6 damage a blow).

The other jobs (baker, smith, trader, shepherd, fisher) only trade.

Villagers' farming and building happen in singleplayer only. On a server
the villagers themselves are shared, but their work is not applied to the
shared world.

## Seeing them all

```sh
voxelrust --show creatures --screenshot creatures.bmp
```

lines up one of every creature and vehicle model and writes a picture
(`--show creatures,walking` is meant to show them mid-stride). In a
world, `/spawn <id>` puts one in front of you; the ids are the `entity` names
in [data/wildlife.txt](../data/wildlife.txt), for example `/spawn wolf 3`.
In creative mode the [spawn eggs](#spawn-eggs) do the same by hand.
