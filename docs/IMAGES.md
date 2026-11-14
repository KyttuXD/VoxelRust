# Pictures: the PNG code and skins

The game has no image library (the program uses no outside crates at all), so the
picture code is its own, in `src/util/image/`:

| File | What it does |
| --- | --- |
| `mod.rs` | `decode` (PNG, JPEG, GIF, BMP, TGA by how the file starts; at most 4096 x 4096 and 24 MiB of file), `fit` (average a picture to a square) |
| `png.rs` | the PNG decoder, zlib inflate, CRC-32, Adler-32, base64 |
| `png_encode.rs` | the PNG encoder: colour type, row filters, chunks |
| `deflate.rs` | deflate compression: LZ77 with hash chains and lazy matching, dynamic Huffman codes |
| `header.rs` | a PNG's size read from its first chunk, without decoding |
| `resize.rs` | making a picture smaller (box filter) |

## The encoder

`crate::png::encode(w, h, rgba)` writes 8-bit RGBA pixels as a PNG. It used to store
the data uncompressed (a 128 x 128 skin was 65 KB); now:

1. **Colour type.** If the picture has **256 colours or fewer**, it is written
   indexed (colour type 3, one byte a pixel, a `PLTE` chunk, and a `tRNS` chunk
   if anything is see-through; see-through colours are put first so that `tRNS`
   can stop early). Else, if nothing is see-through, RGB (type 2); else RGBA (type 6).
2. **Row filters.** Each row is tried with the five PNG filters (none, sub, up,
   average, Paeth) and gets the one with the smallest sum of absolute differences
   (bytes taken as signed). A row equal to the one above is written as all zeros
   under "up" without trying the others, so flat pictures are quick.
3. **Deflate** (`deflate.rs`). Repeats are found with a hash of three bytes, chains of
   candidates in a 32 KiB window, and lazy matching (a match is only taken if the
   next position has no longer one). Blocks of up to 65 536 symbols each get their
   own dynamic Huffman codes (lengths limited to 15, the code-length code to 7);
   a block that would be bigger than the data itself is stored. How hard it looks
   depends on the size: more effort up to 8 MiB of filtered data, less above 24 MiB.

Nothing is lost: the decoder reads back exactly the pixels that were written
(tests do this for palette, alpha and sizes 1 to 4096). Too-short input is padded
with transparent black.

Speed (release build, mostly flat skins): well under a second for 1024 x 1024; a
4096 x 4096 skin takes a few seconds, which is why the game packs the skin it sends
on a worker thread (`client/skins/work.rs`) and never in the frame. Typical sizes:
a flat-coloured 64 x 64 skin is a few hundred bytes, a 1024 x 1024 painted one tens
of kilobytes, noise is the worst case and costs about what it weighs.

The decoder takes up to 4096 x 4096; the largest filtered data it unpacks is 72 MB
(a 4096 x 4096 RGBA picture is 67 MB).

## Skins

See [PLAYING.md](PLAYING.md#skins) for what a player sees. In the code:

- `content/skin_grid.rs`: which sizes are skins (a multiple of 64 from 64 to 4096,
  square, or 2:1 for the old layout), the grid scale (`W / 64`), the words for a
  refused picture. The player models declare a 64 x 64 texture and their coordinates
  are fractions of it, so a skin of any of these sizes lines up.
- `client/skinfile.rs`: turning a picked picture into a square skin (the old 2:1
  layout is mirrored at its own scale), the slim-arms test (works at any size).
- `client/skinweb.rs`: the download from a web address (1 MiB, 24 MiB unpacked).
- `net/skinxfer.rs`, `net/skinslot.rs`: the wire (below) and what the server keeps.
- `client/skins/`: `work.rs` (packing and unpacking on a worker), `budget.rs` (the
  texture memory budget and the distance tiers), `hub.rs` (keeps them in step).
- `client/render/skin_tex.rs`: the textures (nearest for 64 wide, linear with
  mipmaps for bigger).

### The wire

`Msg::Skin` (a PNG of at most 24 000 bytes) is what it always was. Protocol 14 adds
`SkinCaps`, `SkinBegin { id, size, side, slim }` and `SkinPart { id, last, data }` (see
[MULTIPLAYER.md](MULTIPLAYER.md)). A receiver checks, before keeping a byte:

- the declared size is 40 to 1 MiB, the declared side a valid skin side;
- the first piece begins with a PNG header (checksum right, not interlaced) that
  says the same side and a square;
- the pieces never add up to more than the declared size, and end exactly at it;
- one transfer at a time per sender (a client keeps up to 8 transfers and 4 MiB of
  declared bytes in all, one per other player); a transfer silent for 20 seconds is dropped;
- the server also takes one skin change per 5 seconds per player, and decodes a big
  skin fully before it passes it on (a picture whose header lies is dropped).

The server keeps the skin as sent and a 64 x 64 version (`resize::downscale_square`,
then the encoder). Clients of protocol 14 get the first, clients of protocol 13 the
second, as a plain `Skin`.

### Memory

The unpacked pixels of a skin exist only while it is being uploaded; after that the
picture is on the card only. Other players' skins are limited to 64 MB of card
memory (`client/skins/budget.rs`): the size of each is chosen by the player's distance
(48 blocks: full size, 96: 256 x 256, beyond: 64 x 64, with a margin of 6 blocks so a
skin does not flicker at a border), and when the total would be more, the skins whose
players were near longest ago are made smaller first. A skin that is made smaller or
bigger is unpacked again from its kept PNG on the worker (at most two jobs at once).
