# Texturing blocks, items, creatures and players (OCD style)

VoxelRust's pictures are drawn in the **OCD style**: flat, bold and simple,
like the OCD texture pack. The game's older look (noisy, shaded, painted by
code with `builtin` and `paint`) is gone: the painters now draw in the OCD
style too (see [The painters](#the-painters)), so the game looks the same with
and without the pack. New blocks and items are drawn by hand in the OCD style,
and the painted pictures are replaced by pack pictures wherever one exists
(see [Using the pack](#using-the-pack)).

## The style in short

- **16 x 16 pixels**, no anti-aliasing, no dithering, no noise.
- **Few colours.** A block is two to four colours, an item three to eight.
  `stone` is two (a `#7e7e7e` fill and a `#5c5c5c` frame); `oak_planks` is two
  (`#81663f` and the lighter `#ac8c57` for the slats); `bricks` is three.
- **Flat fills.** One colour covers a whole area. Light and shade are done by
  a second, darker or lighter colour in a shape, never by scattered pixels.
- **A frame.** Most blocks have a one-pixel border all the way round in a
  colour about 25-30% darker than the fill (`dirt`: `#845e42` inside,
  `#604530` round). Blocks that must join up (large floors, walls) may leave
  the frame off on the sides that touch.
- **Simple, readable shapes.** A pattern is a few big pieces: slats for
  planks, a four-block grid for cobblestone, offset bricks, four blobs in
  stone for an ore. If it does not read at 16 x 16 in the hotbar, simplify it.
- **Saturated, slightly warm midtones.** Pick a hue, then make the dark colour
  by lowering brightness and keeping (or raising) saturation, not by adding grey.
- **Pure transparency.** A pixel is fully see-through or fully solid, except
  glass and leaves, whose cut-outs and see-through fills use one alpha value
  (160 of 255 for the fill of glass and leaves) all over.

## Blocks

| Kind | How to draw it |
| --- | --- |
| Plain stone, dirt, sand, wool, concrete | Fill + one-pixel frame in the darker colour. |
| Planks | A base colour with lighter horizontal slats, four to a block, with a gap of base colour between. |
| Logs (side) | Dark base with lighter vertical bars; the top (`_top`) is rings: lighter inside a frame. |
| Bricks | Mortar colour (light), brick colour with a lighter second tone, offset rows. |
| Ores | The stone fill and frame, and four small blobs of the ore's colour, each with a lighter pixel on top. |
| Glass | A white frame (`#fafafa`) round a see-through fill. Stained glass: the colour for the frame and the same colour at alpha 160 for the fill. |
| Leaves | Two greys (or greens) in a loose check, some pixels see-through at alpha 160. Drawn **grey**: the game tints leaves, grass and water by the biome, as the pack does. |
| Grass | The top is a flat grey-green fill (tinted in the game); the side is a green band over the dirt tone with a straight edge. |
| Crops, flowers | Cross-shaped plants are drawn on a see-through background, one flat colour per part, a dark stem. |

Light comes from the top left. A block's top is its lightest picture, its
sides one step darker, its bottom the darkest; draw the top and bottom as
separate pictures only when they differ.

## Items

- **Draw upright.** A tool or weapon is drawn with its handle straight down
  the middle and its head or blade at the top. (The game finds the long axis
  of a picture itself and holds the handle in the fist, so diagonal pictures
  also work, but upright is the style.)
- **Dark outline.** One pixel of near-black (`#1a1a1a` or the darkest tone of
  the item) all round the shape. The outline is part of the picture, not a glow.
- **Flat fills inside it:** one light colour for the lit face, one mid colour,
  one dark colour for the shaded edge. Handles are a dark brown with a lighter
  stripe; metal heads are grey with one white highlight pixel column.
- **Metals share one drawing.** A pickaxe, axe, shovel, hoe and sword of each
  metal are the same shapes with the metal's colours, so draw the iron ones once
  and recolour. The game does that for any tool the pack has no picture of: it
  takes the pack's iron tool and swaps the grey metal for the colour of the
  material (see Ruby below).
- **Food and gems:** a bright main colour with a darker lower edge and one
  lighter highlight; keep the outline.
- **Bows and crossbows** are drawn upright too, with the grip in the middle.
  Each also has three pictures of it drawn back, `<bow>_pulling_0`, `_1` and
  `_2` (the string pulled further and an arrow nocked; the OCD pack's
  `bow_pulling_*` and `crossbow_pulling_*` are used for them). The base game
  paints its own in `src/content/art/bow.rs` (styles `bow_pull0..2`,
  `crossbow_pull0..2`). The held bow and its hotbar picture switch between them.

## Hearts and effect icons

The HUD's hearts and the icons of status effects are textures too, painted by
`src/content/art/hud.rs` in this style (flat fills, a one-pixel dark outline,
no grain): a flat red heart, nine pixels wide and eight tall in the middle of
a 16 x 16 tile, and 16 x 16 badges with a light picture on a flat colour.

| Texture | Picture |
| --- | --- |
| `heart_empty`, `heart_full`, `heart_half` | the red heart; the half heart is lit on its left and empty on its right |
| `heart_poison_*`, `heart_wither_*`, `heart_frozen_*` | the same in green, dark grey and blue, `_full` and `_half` |
| `heart_absorb_full`, `heart_absorb_half` | the yellow hearts of absorption |
| `heart_hardcore_full`, `heart_hardcore_half` | a deep red heart (not used by any mode yet) |
| `effect_<id>` | the icon of an effect: poison, burning, regeneration, slowness, speed, hunger, weakness, strength, wither, absorption, frozen, saturation, night_vision, jump_boost |
| `effect_flame` | the flame beside the hearts of someone who is burning |

A texture pack replaces any of them by a picture of the same name; an effect
a pack adds uses `effect_<its id>` or the texture its `icon` line names.

## The painters

The pictures the game draws itself live in `src/content/painters/`, one file
for each subject, and follow the rules above with no random numbers at all:
the same pixel is always the same colour. `texture <id> builtin <name>` picks
a fixed picture; `texture <id> paint <style> r g b` picks a pattern in your
colour. The dark, light and pale tones are made from the colour by fixed
ratios (frame 73%, light a fifth of the way to white, pale halfway), so a
metal, wood or stone recolours consistently.

| File | Builtin names | Paint styles |
| --- | --- | --- |
| `blocks.rs` | `dirt sand gravel clay snow snow_side sandstone lamp` | `concrete wool terracotta farmland magma chest workbench` |
| `stone.rs` | `stone deepstone cobble brick stone_bricks` | `rock polished bricks tiles` |
| `wood.rs` | `log_side pine_side log_top planks` | `planks bark rings door` |
| `ores.rs` | `coal_ore iron_ore copper_ore crystal_ore ruby_ore` | `cluster` |
| `plants.rs` | `grass_top grass_side leaves pine_leaves flower_red flower_yellow tall_grass cactus_side cactus_top` | `leaves blossom crop1 crop2 crop3 crop4 mushroom vine` |
| `glass.rs` | `glass ice` | `glass glassdoor icicle web` |
| `liquids.rs` | `water lava` | |
| `sky.rs` | `sun moon` | |
| `tools.rs` | `wand` | `stick pick axe shovel hoe sword bow crossbow arrow` |
| `food.rs` | | `meat fruit fishfood loaf pizza wedge root tuber jar egg sack cookie pie bowl seeds sheaf` |
| `gear.rs` | | `gem ingot rail machine hull wings` |

`painters/mod.rs` holds the registry of names (`BUILTINS`, `STYLES`) and the
helpers: `framed` (fill and frame), `outlined` (an item's shape with its dark
outline), `dark`, `lift`, `pale`, `rect`, `disc`. To add a picture, write it in
the right file, add its name to the lists, and run the tests: they check that
every name is painted, that every picture has at most eight colours, and that
block faces (`FRAMED_BUILTINS`, `FRAMED_STYLES`) have the frame.

What each one looks like:

- **Block faces** are a fill with a frame (`dirt`, `sand`, `concrete` 92%,
  `wool` 85%, `terracotta` 73%). `rock` adds two flatter patches, `polished`
  a three-pixel shine, `farmland` furrows every fourth row, `magma` a dark
  crust with four glowing blobs, `chest` a lid line and a latch, `workbench`
  a grid, `door` two panels and a handle.
- **Ores** are stone with four 2 x 2 blobs of the ore's colour, each with a
  lighter pixel on top. `cobble` is four stones with a light corner on each.
- **Planks** are lighter slats in the colour, four rows, with the darker base
  tone as gap and a staggered joint. **Bark** is a dark base with lighter
  vertical bars; **rings** the cut end: a dark rim, then two tones in turn.
- **Glass** is a white frame (alpha 235 for stained, solid for plain) round a
  see-through fill (stained: your colour at alpha 105). **Leaves** are two
  greens in a 2 x 2 check with some see-through cells (alpha 0, never half).
  **Ice** stays solid; **water** is two blues at alpha 168 with a one-pixel
  ripple that travels down the tile over eight frames.
- **Grass and leaves are green here**, not grey: the game only tints the
  grey pictures of a *pack* (see below), so the painted ones carry the green
  they should have. A picture of your own in grey needs no change.
- **Tools** are upright with a dark outline: head on top, a two-pixel wooden
  handle in columns 7 and 8; the colour you give is the metal (lit, mid and
  shaded tone). The sword has a guard, grip and pommel; the bow's string is
  on the right.
- **Food and gems** have a main colour, a lighter highlight at the top left,
  a darker lower edge and the outline. **Rails** are left without an outline.
- **Plants** (flowers, tall grass, crops, mushrooms, vines, icicles, cobwebs,
  crystal clusters) have no outline and no frame, one flat colour for each
  part. Cobwebs and icicles keep their one alpha (220, 215).
- **Plain colours** (`texture <id> flat r g b`) are solid; with `noise` given
  (the number no longer does anything) they get the frame.

## Using the pack

Put the pack's folders in a list and the game uses them in place of its own
pictures:

```
voxelrust --texture-pack "path/to/ocd" --texture-pack "path/to/ocd items"
```

or, in the data folder's `settings.txt`:

```
texture_pack=/path/to/ocd|/path/to/ocd items
```

The folders hold `assets/<name>/textures/block/*.png` and `.../item/*.png`
(or just PNG files). A texture is taken from the pack when the pack has a
picture called the same, or one the game knows it as: `grass_top` is
`grass_block_top`, `pine_side` is `spruce_log`, `wood_pick` is
`wooden_pickaxe`, `crystal_axe` is `diamond_axe`, `red_glass` is
`red_stained_glass`, `wheat_3` is `wheat_stage4`. The table is
`pack_names` in `src/content/textures.rs`; add a line to it when you add a
texture the pack calls something else.

**Colours.** The pack's colours are changed a little so the game does not look
exactly like it: the hue is turned, and saturation and brightness are
scaled. The settings (`settings.txt`) are `pack_hue` (degrees, default 6),
`pack_saturation` (percent, default 108) and `pack_brightness` (percent,
default 97); `pack_hue=0`, `pack_saturation=100`, `pack_brightness=100` gives
the pack's own colours. The change is made to every picture taken from the
pack, blocks and items alike, and to the tools made from its iron ones.

**Ruby, tin, silver and the other tools the pack lacks.** The game's own ruby
tools stay: `ruby_pick`, `ruby_axe`, `ruby_shovel`, `ruby_hoe` and `ruby_sword`
are still there, drawn in the pack's style from its iron tools in the ruby
colour. So are the tin, silver, copper, emberite, frostite and other metal
tools. The ruby gem and ore are not matched to anything in the pack, and keep
the game's own pictures until someone draws OCD ones.

**Your own pictures win.** A file in the data folder's `textures/` folder
(`<id>.png`, also `.jpg`, `.gif`, `.bmp` or `.tga`, any size, fitted to 16 x
16) is used before the pack's. To start from the current pictures run
`voxelrust --export-textures DIR`, which writes every texture as a PNG.

## Making a new block or item

1. Draw it in an image editor at 16 x 16 in the style above. Name the file
   after the texture id (`ruby_block.png`).
2. In your pack: `texture ruby_block file textures/ruby_block.png`, then
   `block ruby_block` with `tex ruby_block` (see [DATA.md](DATA.md)). The picture
   can be a PNG, JPEG, GIF, BMP or TGA.
3. Look at it in the hotbar and held in the hand, and next to the pack's
   stone, planks and dirt: if it is busier than they are, remove detail.
4. Check the checklist: 16 x 16, one-pixel frame (blocks) or outline (items),
   two to eight flat colours, no dithering, upright if it is a tool.

## Creatures and players

Every creature, villager and player is drawn in the same style: flat colours,
a one-pixel darker frame round each face, no noise, no dithering, three to
eight colours a picture. Nobody paints them in an editor. They are **made by a
script from a table of colours**, so a change of palette repaints everything.

### The workflow

1. The colours are in `tools/entity_palettes.py`. There is one `sp(model, base,
   looks)` entry per model: `base` gives the colours of the parts (`main`,
   `head`, `belly`, `snout`, `ear`, `leg`, `hoof`, `tail`, `mane`, `horn`,
   `eye`...) and a few patterns (`spots`, `stripes`, `socks`, `blaze`, `belly`,
   `back`, `band`...); every entry in `looks` is another look made from the
   base by changing some of the colours (`cow` has `brown` and `red`, which
   become `cow_brown` and `cow_red`). The header of the file lists every key.
2. Run

   ```
   python3 tools/entity_textures.py                  # writes data/textures/entity/*.png
   python3 tools/entity_textures.py --only cow,pig   # just these
   python3 tools/entity_textures.py --sheet all.png  # a contact sheet to look at
   python3 tools/entity_textures.py --template cow   # cow_layout.png: where each part goes
   ```

   It needs Python 3 and Pillow. The output is the same every time, so a
   change shows in `git diff` as the pictures that changed.
3. Run `python3 tools/gen_creatures.py` if you added a look or a creature. It
   writes `data/wildlife.txt`, where each look is one line,
   `variant Brown tex cow_brown` (the `L(...)` helper in the script), and
   checks that every picture it names exists.
4. Look at it in the game (creatures show on the inventory's spawn eggs, and
   in the world; the skin tab of SETTINGS shows the player).

How the script paints. It reads each model (`data/models/<model>.bbmodel`), which
says where on the picture every face of every box is, and for each face: finds
the part from the box's name (`head`, `ear-l`, `shin-f-r`, `sleeve-l`...),
fills it with the part's colour, lets the patterns recolour pixels (a pattern
is worked out from the 3D place of the pixel, so a spot goes on across an
edge), draws the frame on faces of 3 x 3 pixels or more, and puts the eyes and
nose holes where they go. Parts the palette does not name take the colour of
a related part (`tail_tip` is the tail, `hoof` the leg, anything else the
body). The bottom third of every lower leg is the `hoof` colour, when the
palette has one (hooves, paws, boots).

**What looks right.** Light colours for belly and muzzle, dark ones for ears,
hooves and the end of the tail, a light/dark pair in each pattern, and eyes of
a very dark colour (yellow for night creatures and cats, red for the sinister).
Do not add more patterns than two: the pictures are small and the frame
already makes them busy.

### The looks

Each creature has a base look (the picture named after its model) and two to
nine more, as `variant` lines of its data (see [DATA.md](DATA.md#creatures)).
One is picked for each creature that appears. The base game's creatures:

| Creature | Looks |
| --- | --- |
| Cow | black and white, brown, red, longhorn, dark longhorn |
| Pig | pink, spotted, black |
| Sheep, Merino | white, black, grey, brown (the sheep also a ram) |
| Chicken | white, brown, black, rooster |
| Horse | chestnut, bay, black, palomino, grey, pony, chestnut pony |
| Dog | golden, brown, grey, tan, shepherd, terrier, hound (each in two colours) |
| Cat | ginger, black, cream, grey, fluffy, lynx |
| Wolf, fox, bear | the usual colours, also arctic or black |
| Songbird | brown, blue, red, yellow, green |
| Butterfly | orange, blue, white, violet, lime |
| Zombie, skeleton, ghoul | three each (pale, drowned, mossy, charred...) |
| Villagers | each job has its own outfit, worn by four faces (fair, tan, dark, ruddy) |

The rest (goat, duck, goose, turkey, donkey, llama, camel, bison, yak, rabbits,
deer, elephant, monkey, penguin, turtle, crab, frog, tiger, alligator, snake,
spiders, owl, bee, firefly, bat, worm, ant, beetles, fish, squid, wraith,
werewolf, ogre, scarecrow, slimes, crawlers, golem...) have two to nine
looks each: see the table in `tools/entity_palettes.py`, which is the
complete list.

### Player skins

Players are painted the same way, on the usual 64 x 64 skin layout (so a skin
from anywhere works in the game). The eight built-in skins are `skin_explorer`,
`skin_miner`, `skin_farmer`, `skin_sailor`, `skin_knight`, `skin_mage`,
`skin_ranger` and `skin_baker`: each has its own skin tone, hair, shirt,
trousers and a hat, apron, belt or beard. A player whose LOOK is `default` wears the
one their name picks (a hash of the lower-case name modulo eight; see
`default_skin_for` in `src/content/pack_entities.rs`), and so do other
players in multiplayer until their own skin arrives. The model's overlay
layers (hat, jacket, sleeves, trousers) are only painted when the palette has
a colour for them (`o_hat`, `o_jacket`...), and a pointed hat or helmet covers only the
top of the head (`keep_top`). The first-person hand uses the same skin.

### Using a pack's creature pictures

The pack folders you give `--texture-pack` (see [Using the pack](#using-the-pack))
are also searched for creature pictures, in the pack's
`assets/<namespace>/textures/entity/` folder (or `textures/entity/`, `entity/`,
or the folder itself) and in the folders inside it, down to three deep. A
picture is found by its file name, or by its path inside the entity folder.
The name table (`pack_names` and `ALIASES` in `src/content/pack_entities.rs`):

| Our picture | The pack may call it |
| --- | --- |
| `cow`, `pig`, `horse`, `skeleton`... (the model's name) | the same, `cow/cow` |
| `cow_brown`, `pig_spotted`... | the same, or `cow/cow_brown` (the name's front half as a folder) |
| `villager_farmer_tan` | `villager_farmer_tan`, `villager_farmer/villager_farmer_tan` |
| `player`, `skin_explorer` | `player/wide/steve`, `steve` |
| `player_slim` | `player/slim/alex`, `alex` |
| `skin_miner`, `skin_farmer`, `skin_sailor`, `skin_knight`, `skin_mage`, `skin_ranger`, `skin_baker` | `kai`, `sunny`, `makena`, `ari`, `noor`, `efe`, `zuri` (in `player/wide/`, or anywhere) |

A pack's picture wins over the game's. For the model's own picture (a picture
called the same as the model) it must have the shape of the model's texture
(`128 x 64`, or any size with that proportion), or it is ignored; a variant's
picture is used whatever its shape, so make it the same shape. **The picture
must follow our layout**: the models decide where on the picture each part is,
and a picture drawn for another game's models will look scrambled. Run
`python3 tools/entity_textures.py --template cow` for a map of one model's
layout (each part in its own colour); the player skins use the usual layout.
The pack's colour settings (hue, saturation, brightness) do not change these
pictures.

### Making a new creature's pictures

1. Write the model (see [DATA.md](DATA.md)): name the boxes like the
   other models do (`head`, `ear-l`, `leg-f-l`, `tail`...) so the parts are found.
2. In `tools/entity_palettes.py` add `sp("mymodel", dict(main="#a05030", ...), {...looks})`.
3. `python3 tools/entity_textures.py --only mymodel`, look at the picture,
   and name the looks with `variant <name> tex mymodel_<look>` in the data.
   Mods that ship their own model texture inside the `.bbmodel` need none
   of this: the picture inside the model is used when no `tex` is named.

## Checklist

- [ ] 16 x 16 pixels, RGBA.
- [ ] Two to four colours (blocks), three to eight (items).
- [ ] No noise, dithering or gradients.
- [ ] Frame round blocks, outline round items.
- [ ] Tools upright, handle down.
- [ ] Greys for things the biome tints (grass, leaves, water).
- [ ] Named like the pack names it, or added to `pack_names`.
- [ ] Creatures: the palette is in `tools/entity_palettes.py`, the pictures are
  made by `tools/entity_textures.py`, nothing is painted by hand.
