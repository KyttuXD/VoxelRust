# Texturing blocks and items (OCD style)

VoxelRust's pictures are drawn in the **OCD style**: flat, bold and simple,
like the OCD texture pack. The game's older look (noisy, shaded, painted by
code with `builtin` and `paint`) is not wanted any more. New blocks and items
are drawn by hand in the OCD style, and the older pictures are replaced by
pack pictures wherever one exists (see [Using the pack](#using-the-pack)).

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

## Checklist

- [ ] 16 x 16 pixels, RGBA.
- [ ] Two to four colours (blocks), three to eight (items).
- [ ] No noise, dithering or gradients.
- [ ] Frame round blocks, outline round items.
- [ ] Tools upright, handle down.
- [ ] Greys for things the biome tints (grass, leaves, water).
- [ ] Named like the pack names it, or added to `pack_names`.
