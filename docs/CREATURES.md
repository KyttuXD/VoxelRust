# Creatures

The game has 58 kinds of creature: farm animals, pets, wildlife, things that
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
- Many kinds come in several looks (a brown cow, a black sheep, a pony, a
  rooster); one is picked for each creature. F3 shows how many creatures are
  around you.

Creatures are not saved with the world: when you come back, new ones appear.
In multiplayer every player has their own (see
[MULTIPLAYER.md](MULTIPLAYER.md#what-is-shared-and-what-is-not)).

## Dealing with them

| Action | What happens |
| --- | --- |
| Left click | Hit it. A bare hand does 3 damage, a sword 6 to 15 (see [PLAYING.md](PLAYING.md#swords)). In survival a killed creature leaves its drop; in creative one hit removes it and gives nothing |
| Right click | Collect what it gives (milk, wool, an egg), trade, or hear what it has to say |
| Hold its favourite food | Some animals walk after you while you hold it, from up to 10 blocks away |

### Milk, wool and eggs

Right click the animal. It hands over one item, then needs time before it
has another; until then the game says `NOTHING YET - TRY AGAIN LATER`. Young
animals give nothing.

| Animal | Gives | Ready again after |
| --- | --- | --- |
| Cow | Milk | 3 minutes |
| Goat | Milk | 3 minutes 20 seconds |
| Sheep | White Wool | 4 minutes |
| Chicken | Egg | 2 minutes 30 seconds |
| Duck | Egg | 3 minutes 20 seconds |

The animal is not harmed. Milk and eggs are used in cooking (see
[PLAYING.md](PLAYING.md#the-oven-and-baking)); four String also make one
White Wool at a crafting table.

### Favourite foods

| Animal | Follows you for |
| --- | --- |
| Cow, Sheep | Wheat |
| Pig | Carrot |
| Chicken | Wheat Seeds |
| Dog | Raw Meat |
| Cat | Raw Fish |

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
| Horse | plains, savanna | herd, flees; fast; drops 2 Raw Meat. Horses cannot be ridden |
| Dog | plains, forest, savanna | follows Raw Meat; several breeds |
| Cat | plains, forest, savanna, desert | lazy, hunts small prey; follows Raw Fish |

## Wildlife

| Creature | Lives in | Notes |
| --- | --- | --- |
| Deer | forest, taiga, plains | herd, flees, prey; drops 2 Raw Meat |
| Rabbit | plains, forest, tundra, taiga, desert | flees, prey; drops 1 Raw Meat |
| Mouse | plains, forest, savanna | flees, prey |
| Elephant | savanna | herd, pack: hit one and the herd answers, 9 damage a blow; 80 health |
| Monkey | jungle | herd, flees |
| Penguin | tundra | herd |
| Turtle | swamp, desert, jungle | lazy |
| Crab | desert, swamp | territorial (1 damage) |
| Frog | swamp, jungle | hops |
| Wild Boar | forest, taiga, jungle | territorial, pack (4 damage); drops 3 Raw Meat |
| Wolf | taiga, forest, tundra, mountain | hunter, pack (4 damage) |
| Fox | forest, taiga, plains, tundra | hunter, flees from you |
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
| Lurker | anywhere | 3 damage; the commonest |
| Wraith | swamp, forest, taiga, tundra, badlands | 4 damage; floats and glows faintly |
| Bone Hound | plains, savanna, desert, mountain, badlands | 3 damage; fast, hunts in packs |

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
every player has their own villagers, so their work is not applied to the
shared world.

## Seeing them all

```sh
voxelrust --show creatures --screenshot creatures.bmp
```

lines up one of every creature and vehicle model and writes a picture. In a
world, `/spawn <id>` puts one in front of you; the ids are the `entity` names
in [data/wildlife.txt](../data/wildlife.txt), for example `/spawn wolf 3`.
