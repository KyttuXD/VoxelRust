#!/usr/bin/env python3
"""Read the silhouettes of the OCD pack's tools (iron pickaxe, axe, ... and the
bow's pulling stages) and write them as 16x16 role pictures into
src/content/painters/tool_shapes.rs. The game's own tools are painted from
these, so they have the pack's exact shapes in whatever colours a palette gives.

  python3 tools/ocd_shapes.py "/path/to/ocd tiems/assets/game/textures/item"

Roles: . clear   o outline   h handle (lit)   H handle (shade)
       L head light   M head mid   D head dark   W highlight (white)
       S string   F fletching/feather
"""
import os, sys
from PIL import Image

SRC = sys.argv[1] if len(sys.argv) > 1 else "/home/kyttu/Downloads/ocd packs/ocd tiems/assets/game/textures/item"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "content", "painters", "tool_shapes.rs")

# our style name -> the pack's picture
NAMES = [
    ("stick", "stick"), ("pick", "iron_pickaxe"), ("axe", "iron_axe"), ("shovel", "iron_shovel"),
    ("hoe", "iron_hoe"), ("sword", "iron_sword"), ("arrow", "arrow"),
    ("bow", "bow"), ("bow_pull0", "bow_pulling_0"), ("bow_pull1", "bow_pulling_1"), ("bow_pull2", "bow_pulling_2"),
    ("crossbow", "crossbow_standby"), ("crossbow_pull0", "crossbow_pulling_0"),
    ("crossbow_pull1", "crossbow_pulling_1"), ("crossbow_pull2", "crossbow_pulling_2"),
]

def role(p, wooden):
    r, g, b, a = p
    if a < 128:
        return "."
    lum = (r + g + b) // 3
    brown = r > b + 14 and r >= g
    if brown:
        if lum < 0x40:
            return "o"
        return "h" if lum >= 0x68 else "H"
    # grey / white
    if lum < 0x60:
        return "o"
    if lum < 0xb4:
        return "D" if wooden else "M"
    if lum < 0xd0:
        return "M"
    if lum < 0xf0:
        return "L"
    return "W"

out = ["//! The silhouettes of the OCD pack's tools and bows as role pictures (made by",
       "//! tools/ocd_shapes.py from the pack; do not edit by hand). See that script for the",
       "//! meaning of the letters.", "",
       "pub fn shape(style: &str) -> Option<&'static [&'static str; 16]> {", "    Some(match style {"]
for ours, theirs in NAMES:
    im = Image.open(os.path.join(SRC, theirs + ".png")).convert("RGBA")
    px = im.load()
    rows = ["".join(role(px[x, y], ours in ("bow", "crossbow") or ours.startswith("bow_") or ours.startswith("crossbow")) for x in range(16)) for y in range(16)]
    out.append(f'        "{ours}" => &[')
    for r in rows:
        out.append(f'            "{r}",')
    out.append("        ],")
out.append("        _ => return None,")
out.append("    })")
out.append("}")
open(OUT, "w").write("\n".join(out) + "\n")
print("wrote", OUT)
for ours, theirs in NAMES[:8]:
    im = Image.open(os.path.join(SRC, theirs + ".png")).convert("RGBA"); px = im.load()
    print(ours)
    for y in range(16):
        print("  " + "".join(role(px[x, y], False) for x in range(16)))
