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
colour, and `texture <id> paint <style> palette <name>` one in several colours
(see [Palettes and gradients](#palettes-and-gradients)). The dark, light and pale tones are made from the colour by fixed
ratios (frame 73%, light a fifth of the way to white, pale halfway), so a
metal, wood or stone recolours consistently.

| File | Builtin names | Paint styles |
| --- | --- | --- |
| `blocks.rs` | `dirt sand gravel clay snow snow_side sandstone lamp` | `concrete wool terracotta farmland magma chest chest_front chest_side chest_top workbench` |
| `stone.rs` | `stone deepstone cobble brick stone_bricks` | `rock polished bricks tiles` |
| `wood.rs` | `log_side pine_side log_top planks` | `planks bark rings door` |
| `ores.rs` | `coal_ore iron_ore copper_ore crystal_ore ruby_ore` | `ore ore_vein cluster` |
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
- **Chest faces**: the chest block is 14 x 14 and 10 tall, so `chest_front`,
  `chest_side` and `chest_top` draw their own dark outline inside the picture
  (the pixels outside it are clear and never seen). `chest_front` has the lid line and
  the latch across it (`accent`), `chest_side` the lid line only, `chest_top` the
  lid's inset trim. The block takes the front through `front` (see DATA.md).
- **Ores** (`ore`) are stone with four 2 x 2 blobs of the ore's colour, each with a
  lighter pixel on top (the shipped ruby ore uses a palette: base, a dark
  pixel at the bottom right, a light one on top); `ore_vein` crosses the stone with
  two diagonal bands. Both take a `host` stone and several gem palettes. `cobble` is four stones with a light corner on each.
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

### Palettes and gradients

One colour is often not enough: a ruby ore wants two reds and a highlight, a
fire axe a head that goes from red to yellow, a sword a fuller in a second
colour. A **palette** is up to **six colours with roles**, optionally spread
over the picture in flat bands, and may carry more palettes (the gems of an
ore, the handle of a tool). The old `paint <style> r g b` line still works and
draws exactly what it always drew (it is a palette of one colour: dark and
light are made from it by the ratios in
[the quick reference](#quick-reference-ratios-measured-from-the-pack)).

**Roles.** Colours listed in a palette take these roles in order, or are given
by name (`accent=#ffffff`):

| # | Role | What the painters use it for |
| --- | --- | --- |
| 1 | `base` | the main fill: a block's face, a tool's metal, an ore's blobs, a gem's body |
| 2 | `dark` | the shaded tone: a block's frame, the shaded edge of a head or gem, an ore blob's bottom right pixel |
| 3 | `light` | the lit tone: a head's lit edge, a gem's lit side, an ore blob's top left pixel |
| 4 | `accent` | a second colour on top: a sword's fuller, a chest's latch, a door's handle, mortar of bricks |
| 5 | `highlight` | the single brightest pixel (an ore blob's top left pixel wins over `light`) |
| 6 | `outline` | the item outline instead of near-black `#1a1a1a` |

A role that is not given is made from the base the old way (dark 73%, light a
fifth of the way to white), so a one-colour palette and a six-colour one draw
the same shapes. Colours are `#rrggbb` (six digits) or three numbers
`r g b`.

**Declaring and using** (in `data/base.txt` or any pack; declare a palette
before the `texture` lines that use it; a later declaration of the same name
replaces the earlier one):

```
palette <name> <colours...> [gradient vertical|horizontal|diagonal|radial]

texture <id> paint <style> palette <name>            a declared palette
texture <id> paint <style> palette <name>+<name2>    ... with a second palette (see below)
texture <id> paint <style> colors #aa... #bb... #cc...   colours written in place (`colors` may be left out)
texture <id> paint <style> palette <name> gradient radial   a gradient word on the line wins
texture <id> paint <style> palette <name> host deepstone    ores: the stone they sit in
texture <id> paint <style> r g b                     the old way
```

In a `texture` or `palette` line `#rrggbb` is a colour, not a comment (a `#`
followed by anything else still starts a comment, so write `# note` with a
space).

**Gradients** are flat bands, never blends: with *n* colours the area is split
in *n* equal bands (the OCD style has no smooth shading, and a gradient
picture keeps to eight colours or fewer). The first colour is at the top
(`vertical`), the left (`horizontal`), the top left (`diagonal`) or the middle
(`radial`); the last at the bottom, the right, the bottom right or the rim.
Over what area? Over the **head** of a tool (`pick`: rows 2-4, `axe`, `shovel`,
`hoe`: their blades, `sword`: the blade) and over the whole 16 x 16 for
everything else. With a gradient the bands do the shading: dark and light
tones are not made from each band (give `dark=` / `light=` by name if you
want them, they are then the same everywhere).

```
palette fire  #d83a1c #f2a21c gradient vertical      # red at the top of the head, yellow below
palette frost #ffffff #8cc8f4 #3a78d0 gradient diagonal
palette oak   #a07a4a #745634                        # wood: lit column, shaded column
palette ruby  #e2406e #a82454 #ff9cb8                # base, dark, light

texture fire_axe  paint axe   palette fire+oak       # head: fire, handle: oak
texture frost_pick paint pick palette frost
texture jade_sword paint sword colors #3aa66a #1e6a3e #8fe0b0 accent=#e8f8ee
texture ruby_ore  paint ore   palette ruby           # stone + four ruby blobs
texture ruby_ore_deep paint ore palette ruby host deepstone
texture twin_ore  paint ore palette ruby+emerald     # blobs take the gems in turn (emerald declared like ruby)
texture jade_vein paint ore_vein palette jade host stone  # banded vein
texture gift_red paint chest colors #d63430 accent=#f2e6a0
```

**What each painter does with a palette**

| Style | Palette use |
| --- | --- |
| `ore` | host stone (`host`, default stone) with a frame, and four 2 x 2 blobs; blob *i* takes palette *i* of `palette+more+more...` in turn; each blob: base, top left `highlight`/`light`, bottom right `dark` (only if the palette gives one) |
| `ore_vein` | the same stone crossed by two diagonal bands (upper left in the first palette, lower right in the second, or the first again); each band is a `light` row beside a `base` row |
| `pick axe shovel hoe sword` | head: `base`, `dark`, `light`, or a gradient across the head; **handle: the second palette** (`fire+oak`: its base is the lit column, its dark the shaded column); sword `accent` is a **fuller**, a line down the blade (column 7, rows 3-8); `outline` replaces the near-black |
| `gem ingot meat fruit` and the other items | `base`, `dark`, `light`, `outline` |
| `concrete wool terracotta rock polished tiles farmland workbench door chest` | `base` fill, `dark` replaces the frame tone, `light` the shine of `polished`; `accent`: the chest's latch and the door's handle |
| `bricks` | `base` and `light` bricks, `accent` mortar |
| `planks bark rings` | `base`, and `dark` for the gaps, the bark's dark base, the rim of rings |
| `leaves cluster` | `base`, `dark`, `light` |
| `bow crossbow` (`art/bow.rs`) | the base only (the colour at that pixel if there is a gradient) |

A pack can check a palette with `voxelrust --export-textures DIR` and look at
the PNGs. The pure parts are in `src/content/palette.rs` (`Palette`,
`Palette::at_in` / `sample` for the colour at a pixel, `light_of`, `dark_of`,
`shade`, `parse_spec`, `declare`), tested without a game.

## The OCD pack, measured

The rules below were read off the pack itself: the pictures in its
`textures/block`, `textures/entity` and `textures/gui` folders (1383 block
pictures, 1198 of them 16 x 16) and the 307 16 x 16 item pictures of the items
folder, pixel by pixel. Coordinates are `x, y` from the top left of a 16 x 16
picture. "Base" is the main fill of a picture. Numbers are medians unless
stated; the exact hex values are the pack's, kept as examples, not to be
copied (the pack stays outside the repository).

**What the pack does not have:** no creature (mob) pictures. Its `entity`
folder holds the shulker, armour stand, chests, beds, boats, signs, banners,
bell, minecart, conduit and cushions, and those are what the creature rules
below are read from.

### Quick reference: ratios measured from the pack

"x 0.73" means every channel of the base multiplied by 0.73 (what the painters'
`dark` does); "to white 0.26" means 26% of the way from the colour to white
(`lift` is 22%, `pale` 50%).

| What | Measured | Painters use | Notes |
| --- | --- | --- | --- |
| Block frame (stone, dirt, terracotta, sand, ore host) | x 0.73 (stone `#7e7e7e` / `#5c5c5c`, dirt `#845e42` / `#604530`, terracotta `#975d43` / `#6f4431`) | `DARK` 73 | 463 of the 1198 16 x 16 blocks have a one-colour frame all round; the rest are tiles that join up |
| Concrete frame | x 0.89 (0.87-0.92) | 92 | `white_concrete` `#cfd5d6` / `#bec3c4` |
| Wool | no frame: the darker tone x 0.85 in a pattern inside (`#fafafa` / `#d3d9da`) | frame 85 | |
| Sandstone, snow | x 0.87 (`#ede7b8` / `#ccc28a`), x 0.9 (`#ffffff` / `#e4e1fa`) | 82-90 | |
| Planks: gap or edge / slat | x 0.73 (oak `#81663f` / `#ac8c57`, spruce `#4c3922` / `#684e2f`, birch `#98875a` / `#c4b37b`, dark oak x 0.67) | 77 | the slat is x 1.32 of the gap |
| Log side: dark / bar | x 0.63-0.73 (oak `#4e3e26` / `#7d633c`) | 66 | |
| Log top: rim / rings | rim x 0.44 of the light ring, middle ring x 0.84 (`#4e3e26`, `#927a47`, `#af8e58`) | rim 55, ring 86 | |
| Three-tone stones (andesite, granite, deepslate) | dark x 0.82, base 1, light x 1.10-1.13 (andesite `#646463` `#7a7a72` `#898988`) | | cobblestone: `#5c5c5c` joint, `#6b6b6b`, `#7e7e7e`, corner `#979797` (to white 0.2) |
| Stained glass, ice | glass: frame alpha 163, fill alpha 102 (of 255); ice: frame 255, fill 159; slime: 237 / 188 | 235 / 105 | plain glass: frame only, `#fafafa`, fill clear |
| Item outline | x 0.33 of the base for food, x 0.30 gems and ingots, x 0.17 armour, x 0.36-0.57 metal tools (iron 0.36, diamond 0.34, gold 0.55, stone 0.57); luminance median 57 of 255 | near-black `#1a1a1a` | **an outline is the item's own darkest tone**, not black; wood handles `#281e0b`, metal heads `#444444` (iron) |
| Item shade (lower or right edge) | x 0.54 for food, 0.34 gems, 0.21 armour | `dark` 73 | for a new item write `dark=` by hand at about x 0.55 |
| Item light (top left) | to white 0.26 food, 0.37 iron tool, 0.54 armour; highlight pixels go all the way to white (`#ffffff`) | `lift` 22 | |
| Entity faces (shulker, bed) | frame x 0.73 (shulker red `#9e2622` / frame `#7b1919`), bed `#ac2c24` / `#8f2020` x 0.83; the shaded faces of the shulker x 0.55 and x 0.45 (`#691112`, `#5b0e0e`) | `FRAME` 0.78 | |
| Colours per block picture | median 4 (p10 2, p90 7, max 18; a plain block 2, planks and logs 2-3) | 2-8 | |
| Colours per item picture | median 6 (p10 3, p90 12, the rest are maps and clocks); food 5, metal tools 7, armour 5, dyes 3, gems and ingots 7 | 3-8 | |

A palette gets the same effect by writing the colours: for a new metal
`palette tin #b8bcc4 #70747c #dce0e8 outline=#3c3e44` (base, dark x 0.62, light
a third of the way to white, outline x 0.33). To compute them with Python:
`dark = [round(c * 0.55) for c in base]`,
`light = [round(c + (255 - c) * 0.3) for c in base]`.

### Standing up: tools, weapons, bows, armour

**Every tool, weapon and bow is drawn upright.** Grip at the bottom, head on
top, nothing diagonal: the game finds the long axis itself, so an upright
picture is held right in the hand and in the hotbar. (The pack itself draws its
crossbow lying on its side; ours stands up.)

Common to all of them, as the pack draws them:

- **The handle** is one pixel wide plus a one-pixel outline on its **right**
  only: its column (x 7 for the pickaxe, sword, shovel and arrow, x 8 for the
  axe, x 9 for the hoe) alternates lit `#896727` and dark `#684e1e` every row,
  the column to its right is the outline `#281e0b`. It runs the full height of
  the picture, from the bottom up into the head. (The painters in `tools.rs` draw a two-column
  handle, 7 lit and 8 shaded; same idea.)
- **The head** has a 1 px outline in the metal's darkest tone, a base fill, a
  lighter column or row on the lit side (top left) and **one column of
  pure highlight** (`#ffffff` or the lightest tone) next to it.
- Metals share one drawing and differ only in the colours: outline `#444444`,
  base `#c1c1c1`, light `#d8d8d8`, highlight `#ffffff` for iron; the same
  shapes in gold (base `#e9b115`, light `#eaee57`, highlight `#fdff76`,
  outline `#825d16`), diamond (`#27b29a`, `#2bc7ac`, `#33ebcb`, `#0e3f36`),
  stone (`#7f7f7f`, `#898989`, `#9a9a9a`, `#494949`) and wood (`#6b511f`
  head, outline `#372910`).
- 6 to 8 colours, never more: outline, base, light, highlight, the handle's
  two browns and its outline, and for swords the guard tone.

| Tool | Layout (16 x 16; the rows up to the arrow are read from the pack, the rest are our layouts in the same style) | Pixels |
| --- | --- | --- |
| Pickaxe | outline bar `x 2-13, y 1`; base bar `y 2`; the two ends bend down `x 2-4` and `x 11-13` to `y 6`; the light tone down `x 3` and `x 12` on rows 3-4, the highlight at `(3, 5)` and `(12, 5)`; handle `x 7-8`, `y 0-15` | head 12 x 6, about 36 outline + 10 base + 4 light + 2 highlight |
| Axe | blade `x 4-11, y 1-7` (8 x 7) with the handle running through it at `x 8-9`; highlight column `x 5`, light `x 6`, base `x 7-10`; handle `y 0-15` | blade 26 outline, 12 base, 5 light, 5 highlight |
| Shovel | blade `x 5-10, y 0-6` (6 x 7): outline all round, highlight columns `x 6` and `x 9`, base `x 7-8`; handle `x 7-8`, `y 6-15`, its foot outlined | blade 20 outline, 10 base, 10 highlight |
| Hoe | head bar `x 3-11, y 2-5`: outline `y 2`, light row `y 3`, a hook down at `x 3-5`, `y 4-5`; handle `x 9-10`, `y 0-15` | head 22 outline, 7 light/base |
| Sword | blade `x 6-9, y 0-8` (4 wide, 9 tall): outline `x 6` and `x 9` and the tip row `y 0`, highlight column `x 7`, base column `x 8`; guard `x 3-12, y 9-11` (outline row, fill row, outline row; left half light, right half dark); grip `x 7-8, y 12-14`; pommel `x 7-8, y 15` | blade 24 outline, 9 highlight, 9 base; guard 22 outline |
| Stick | handle only, `x 7-8`, `y 0-15` | 3 colours |
| Bow | stave `x 4-10, y 0-15` curving: thick at the top and bottom (3 rows), thin round the grip (`x 4-6, y 6-9` wrapped in grey); string straight down `x 10, y 3-12` in `#444444`; three pulled-back pictures: the string and a nocked arrow move left | 7-11 colours |
| Crossbow | the pack draws it lying on its side (limbs `x 2-9` vertical, bolt `y 6-9` across the whole width, 9 colours). Ours stands: stock down `x 7-8, y 4-14`, arms across the top `y 2-4`, a taut string `y 5`, the bolt up the stock | 8-9 colours |
| Arrow | tip `y 0-3` (grey, 2-4 px wide), shaft `x 7-8, y 4-12`, three rows of feathers `y 13-15` in light grey | 7 colours |
| Fishing rod | a straight rod `x 7-8` from the bottom to the top, a thin line `x 12` from the tip down to a 2 x 2 bobber (white over red) | 6-8 colours |
| Trident | shaft `x 7-8, y 4-15`, three prongs `x 3-4`, `x 7-8`, `x 11-12` rising to `y 0`, a crossbar `y 4` | 6 colours |
| Shield | a 12 x 12 board `x 2-13, y 2-13`: outline all round, base fill, a vertical stripe of the second colour down the middle; the pack only has the empty-slot outline (`#192022`, 44 px) | 4-5 colours |
| Shears, flint and steel, brush | two blades crossing at the pivot, drawn upright: grip at the bottom, blades `x 6-9` on top | 6-7 colours |

**Armour pieces** (the pack's icons, 5 colours: outline `#191919`, edge
`#2d2d2d`, base `#aeaeae`, shade `#6b6b6b`, light `#d8d8d8` for iron; the same
shapes in every material):

| Piece | Box | Layout |
| --- | --- | --- |
| Helmet | `x 3-12, y 3-11` (10 x 9) | top and side outline, a light row under the top, base below, a 4 x 3 face opening `x 6-9, y 8-10` in the darkest tone |
| Chestplate | `x 1-14, y 2-14` | shoulders `x 1-5` and `x 10-14` rows 2-6 (outline, light fill), neck notch between them, torso `x 3-12` base, a shade row at the hem |
| Leggings | `x 3-12, y 2-14` | belt rows 2-5 (outline row, then three light rows), then two legs `x 3-6` and `x 9-12` from row 6 with a gap of 2 px (`x 7-8`) between them, a shade row at the hem |
| Boots | `x 1-14, y 3-12` | two boots `x 3-6` and `x 9-12` (a shaft 4 px wide, light on top), and from row 9 a foot 6 px wide pointing outward (`x 1-6` and `x 9-14`), shade row at the bottom |

### Food and potions

- **Shape first, then three tones.** An apple is a rounded block `x 2-12, y 4-13`
  (11 x 10): outline in x 0.33 of the red, base fill, a **highlight of 3 px at
  the top left** (`y 5`), one lighter row under it, a **shade row along the
  bottom** (`y 12`, x 0.6 of the base) and a 3 px brown stem on top (`x 7, y 1-3`).
- **The outline has two tones:** the lit side (top, left) a little lighter than
  the shaded side (bottom, right). Bread: top and left `#654b17`, bottom and
  right `#3f2e0e`; cooked beef: `#4e2719` and `#2a160d`.
- **Banding.** Breads, steaks and loaves are drawn in stripes of the three tones
  (cooked beef: rows of `#8e523c`, `#673728`, `#522f1f`, repeating every three
  rows). The stripes are the only texture. 4 to 9 colours, 5 typical.
- **Crops as items** (carrot, wheat, beetroot): the plant's part upright: the
  carrot is `x 5-10, y 1-14` with a green top (rows 1-4, then a 2 px stem)
  over an orange block 6 px wide with a darker outline (`#913101` on `#ff8e09`).
- **Potions and bottles** share one bottle: neck `x 7-9, y 2-4` (the cork, 3
  tones), a glass shoulder `y 5-7` (`#d4e5f7` outline, `#b3cfec` walls, light
  outline on top), a body `x 4-12, y 8-14` with 1 px glass walls and a
  **see-through middle** in the potion's own colour. The pack draws the bottle
  and a tint overlay (`potion_overlay`) separately: do the same, so one bottle
  serves every potion; a splash potion is the same bottle with the neck at the
  top right.
- Sweets and pies: a 10 x 10 body, outline x 0.33, a lit top edge, a darker
  lower edge, the pattern (chips, a lattice) in the shade colour, no more than 5 colours.

### Blocks that need more than one picture

- **Doors: two pictures**, `_top` and `_bottom`, 16 x 16 each. Bottom: planks
  in vertical slats (columns 1-2, 5-6, 9-10, 13-14 light `#ac8c57`, the rest base
  `#81663f`), the lowest row `y 15` darkened to x 0.8 (`#695333`), a few grey hinge
  pixels (`#434343`) on the left edge and a 2 x 2 knob at `x 13-14, y 0-1`.
  Top: the same slats with a window frame `x 4-11, y 4-11` of lighter wood
  (`#c4b37b`) round a see-through middle `x 5-10, y 6-9`, and the top row `y 0` in base. Our
  `door` style draws a closed bottom half; a top half is the same picture with
  the panels cut by the window.
- **Trapdoors** are two tones (`#81663f`, `#ac8c57`): a dark 1 px outline, a light
  2 px border, a dark inner frame `x 3-12, y 3-12` crossed by a bar at rows 7-8 and
  columns 7-8: four recesses.
- **Signs:** the icon is a board `x 2-14, y 3-10` of two plank tones in
  alternating rows (outline rows at 3 and 9) with a darker bottom edge (`#755a33`, row 10) and a 3-pixel post
  `x 7-9, y 11-15` in three tones (lit, base, shaded). The entity picture is 64 x 32
  with three colours (`#81663f` outline, `#ac8c57` face, `#4e3e26` post).
- **Beds, chests, shulkers** are entity boxes: each face is flat with a
  1 px frame and the sides get darker tones of the colour (bed red `#ac2c24`,
  its shade `#8f2020`; shulker: see the creature rules), 5-9 colours in all.

### Redstone-like and mechanical parts

- **Rails** (16 x 16, 5 colours): two rails 3 px wide at `x 1-5` and `x 10-14`
  (outline `#40321c` on the outside edges, `#686868` shade, `#a4a4a4` light centre column),
  sleepers across between them `x 6-9` in rows of three (`y % 4 == 0, 2` base
  `#584527`, `y % 4 == 1` light `#795f35`) and one clear row (`y % 4 == 3`)
  where only the rails are drawn. A powered rail adds a gold stripe; no frame,
  see-through ground.
- **Wires, dust** are drawn **grey** and tinted by the game: a line `x 6-9`
  (`#6b6b6b` outer, `#929292` inner), a dot (`x 5-10, y 5-10` with a lighter
  middle `#a8a8a8`). Power is a tint, not a different picture.
- **Repeaters, comparators, levers, buttons** are plates: a stone base
  `x 2-13, y 9-12` (`#a8a8a8` face) with a `#68676a` rim row at `y 13`, two
  torches (`x 3-4` and `x 11-12`: a 2 x 2 top in the signal colour `#600000` on
  a 2 px wooden stem `#886c41` / `#634f2f`, rows 5-8) and a slider row between
  them in `#600000` / `#460000`. When powered only the signal colour changes.
- **Torches and lanterns:** a 2 x 5 flame/handle (the pack draws them at 32 x
  32 with four colours, we draw 16 x 16 with the same four: outline, wood,
  flame, flame core). Lit and unlit are two pictures.
- Machines (furnace, hopper...) are block faces: the frame, a darker opening,
  a lit opening in the working picture.

### Plants: crops by stage, saplings, flowers

- **Crops by growth stage:** eight pictures for wheat, four each for carrots,
  potatoes and beetroots. A row of stalks: **3 px wide stalks at `x 0-2, 4-6,
  8-10, 12-14`** with a clear column between them, drawn with a 1 px
  outline and a lighter 1 px core. They **grow by two rows per stage** from the
  bottom: stage 0 is 3 rows (`y 13-15`), stage 3 is 9 rows (`y 7-15`), the last
  is all 16 rows. The green gets lighter and yellower each stage
  (`#17a509` / `#1fd00d` at stage 0, `#46a51c` / `#60d026` at stage 3) and the
  ripe stage is gold with a 4-row head (`#c7be61`, `#e9e385`, head `#f5f29b`).
  Root crops show the green top and, in the last stage, the vegetable's own
  colour at the foot (`y 14-15`). Our `crop1`-`crop4` styles follow this, with the
  crop's colour on the last.
- **Saplings** are blocky, not sprigs: a 4 x 4 leaf cube on top (`x 6-9, y 4-7`:
  outline `#1f5b13`, 2 x 2 fill `#2a7d1a`) over a 4-px trunk with a 2 x 2 light core
  (`x 6-9, y 8-15`, outline `#4e3e26`, two 2 x 2 cores `#af8e58` and `#7d633c`), no ground. 5 colours.
- **Flowers are cross shapes:** drawn once on a clear 16 x 16 and the game
  crosses two copies diagonally. A head of 7 x 7 pixels round (a ring) in three
  tones of the petal colour (dandelion: ring `#a7b20d`, petals `#e2e912`,
  centre `#c5ce10`), at `x 4-10, y 5-11`; a stem 3 px wide (`x 6-8, y 12-15`: outline, core, outline), two leaves
  one pixel out at `y 13` and joined at `y 14` (`x 4-10`), in three greens
  (`#326228` outline, `#366f2a`, `#3b7d2e`). Poppy: the same with a four-lobed
  head in red. 6 colours.
- **Grass and ferns** are 2 colours (a stalk's outline and fill), grey and tinted by the game.
- **Tall plants** (rose bush, sunflower, large fern) are two pictures, `_bottom` and
  `_top`, joined at the edge: the stem is in the same column in both.

### Liquids and animation strips

- An animated picture is **one tall strip of square frames**, top frame first.
  Ores are `16 x 112` (7 frames; frame 0 is the plain ore and is held
  100 ticks, the other six are the glint of the ore, not examined pixel by pixel);
  lava is `16 x 320` (20 frames), water `8 x 256` (32 frames of 8 x 8 at
  **alpha 180 of 255**). The pack's `.mcmeta` files name the frame times.
- Water: 10 blues in a soft ripple (ours: two blues at alpha 168 with a
  one-pixel ripple travelling down over eight frames). The pack's lava is
  smooth (hundreds of colours); ours stays flat: two oranges and a yellow,
  the same ripple, opaque.

### Tint rules

- The pictures of **leaves and redstone dust are grey** in the pack (`#6b6b6b` /
  `#929292`, x 0.73 of each other: leaves are 7 x 6-pixel patches with a dark outline
  and 2-pixel gaps; dust is a line `x 6-9` or a dot), and the game multiplies
  them by the biome colour or the signal colour. Grass, ferns and water follow
  the same convention.
- Things with a fixed colour are **not** grey: logs, flowers, crops (they are tinted
  only when a pack asks), gems, ores, stone.
- A grey picture should be **mid-grey** (`#7e7e7e`-ish, never white or black), so the
  tint decides the colour. Our painted leaves and grass are drawn in their
  green instead (the game only tints a *pack's* grey pictures), see
  [The painters](#the-painters).
- **Leaves:** 2 tones, 4 x 4 or 7 x 6 blocks with a 1 px outline in the dark
  tone, pure holes (alpha 0 or 255), and an `_opaque` copy of each (`acacia_leaves_opaque`)
  without holes for fast graphics.

### Particles and the sky

- A **particle** is a small picture of the thing that makes it: a
  16 x 16 `<block>_particle` in 2 or 3 colours of the block with its frame
  (bamboo fence: `#907e3a` frame, `#c8b14d` ring, `#e3cc6a` fill). Only a few
  pixels of it are seen at a time, so **no detail finer than 2 px**, **opaque**,
  and nothing that is not in the block itself.
- The blob shadow (`shadow.png`, 64 x 64) is **one colour**, black, in two alpha levels
  (191 on the soft ring, 255 inside).
- **Sun and moon** (`environment/celestial`) are 32 x 32 pictures of 7-10 flat
  colours drawn on an opaque black field, not discs with soft edges.

### Interface (GUI)

- **Buttons** 200 x 20: 1 px **black** frame (white when highlighted), a flat
  `#6f6f6f` fill, a darker `#565656` bottom and right edge, a lighter top and
  left edge (bevel tones), 11 colours in all; disabled: one `#2c2c2c` fill with the
  black frame (2 colours).
- **HUD icons** (hearts, food, armour, air) are **9 x 9**: a **1 px black
  outline all round** (24-26 of about 41 pixels), then 2-4 colours (armour `#b8b9c4` and
  `#e6e7f2`; air `#0094ff` and `#56b8ff`). Empty = the same shape in dark grey `#3d3d3d`;
  half = the left half full. Variants (poison, hunger) change only the fill colour.
- **Crosshair:** a 15 x 15 white plus (13 px). **XP bar:** 182 x 5, a dark
  `#28342e` background in a black frame and a progress bar. The pack draws a smooth green
  gradient there (56 colours); ours uses flat bands of 2-3 greens.
- **Hotbar and containers** (our guideline, same rules as blocks): a 1 px dark
  frame round a flat panel, slots one tone darker, the selection a white 1 px frame.

### OCD-styled creatures

The pack has no mob pictures, so these rules come from its entity boxes
(shulker, armour stand, bed, chest, minecart) and from what
`tools/entity_palettes.py` and `tools/entity_textures.py` already do.

- **Silhouette first.** One simple box for each part, `head`, `body`, four
  `leg`s, `tail`, `ear`s; no part is thinner than 2 px. The creature has to be
  recognised from its outline at 16 px high on the hotbar's spawn egg.
- **Flat fill and a frame on every face.** Each box face is one colour with a 1
  px darker frame (x 0.78, the `FRAME` constant of the script) on faces of 3 x 3
  or more. Limbs are therefore **outlined**: a leg is a column of the leg colour
  with a frame, a knee is never drawn. Faces smaller than 3 x 3 get no frame.
- **Light and shade by face, not by pixel.** The shulker's red is a lit
  `#9e2622` with its face frames `#7b1919` (x 0.73), a darker side `#691112`
  (x 0.55) and the darkest `#5b0e0e` (x 0.45). Belly and muzzle are
  lighter by 22-30% of the way to white, ears and tail tips darker (x 0.86), legs x 0.88,
  mane x 0.74.
- **Colour budget: 3 to 8 colours** on a creature, and **two patterns at most**
  (spots, stripes, socks, blaze, band): a light and a dark colour for each. A
  bed has 6, a shulker 9, a chest 5.
- **Eyes.** Two pixels on the front of the head, 1 px each (**2 x 2 on a head
  9 px or wider**), very dark `#1c1a20`, at 36% of the height of the face and 20% in
  from each side; "wide" eyes (14%) for prey animals, none for faceless things;
  **side eyes** (one each side, near the front) for birds and fish. Night
  creatures and cats get yellow eyes, the sinister red. A **human** face has the
  white outside the dark pupil (`#fafafa` then eye colour) and a 2-px mouth at
  78% of the height in x 0.62 of the head colour.
- **Nose holes** on a muzzle: two pixels at 25% in from each side, one row
  down, in x 0.45 of the snout colour. A beak has two dark nostrils (x 0.6).
- **Variants** differ by **one change of palette**, not a new drawing: the same
  boxes with another `main` (and what follows from it), or a different pattern
  colour. A cow's `brown` and `red` are the base look with another main; a
  longhorn adds `horn`. Everything not named follows the main colour by the
  ratios above. Never make a variant by editing pixels.
- **Babies** are **the same picture** on the same model, scaled by the `baby`
  size (0.2-0.9 of an adult, see [DATA.md](DATA.md#creatures)). So the picture has to
  hold up at half size: **keep the eyes 1 px and the frame 1 px** (a bigger eye
  or a wider frame reads as a different, older animal when shrunk), keep
  patterns to big shapes, and never draw an adult-only detail (horns, a mane,
  a comb) in a colour that the base picture does not also give the young. A baby
  needs no picture of its own.
- **Undead and monsters** use the same rules with desaturated main colours, a
  `dark` colour for the cavities, bones `#e8deb8`, and eyes in a bright colour
  for contrast.
- A **player skin** is the exception: 64 x 64, the usual layout, painted by the
  same script (see [Player skins](#player-skins)).

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
scaled. The settings (`settings.txt`) are `pack_hue` (degrees, default 0),
`pack_saturation` (percent, default 100) and `pack_brightness` (percent,
default 100): the pack's own colours unless you change them. A recoloured
pack (such as `ocd recolour`, listed last so its pictures win) is the better way to change the look. The change is made to every picture taken from the
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

These rules are written out, with the pack's measured numbers, in
[OCD-styled creatures](#ocd-styled-creatures): silhouette, frames on every
face, eyes, variants, babies.

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
- [ ] No noise or dithering. A gradient (see [Palettes](#palettes-and-gradients)) is flat bands of
  two to three colours, never a smooth blend.
- [ ] Frame round blocks, outline round items.
- [ ] Tools upright, handle down.
- [ ] Greys for things the biome tints (grass, leaves, water).
- [ ] Named like the pack names it, or added to `pack_names`.
- [ ] Creatures: the palette is in `tools/entity_palettes.py`, the pictures are
  made by `tools/entity_textures.py`, nothing is painted by hand.


## The tools' shapes come from the pack

The game's own tools, weapons and bows are painted from the pack's silhouettes,
not from shapes of ours: `tools/ocd_shapes.py` reads the OCD pack's iron pickaxe,
axe, shovel, hoe, sword, stick, arrow, bow (at rest and drawn) and crossbow and
writes them as role pictures (outline, handle lit/shade, head light/mid/dark,
highlight) into `src/content/painters/tool_shapes.rs`. The painters colour those
roles from a palette, so every metal, wood and gem tool has the pack's exact shape
(the hooked pickaxe, the bracket bow) in its own colours, and a palette with
several colours or a gradient gives a multi-coloured head. Run
`python3 tools/ocd_shapes.py "<pack>/assets/game/textures/item"` to regenerate.
