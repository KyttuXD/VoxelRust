#!/usr/bin/env python3
"""Writes the rail track of the Trains mod: its pictures and the block sections of its pack.

The mods live outside the game's repository (see docs/MODDING.md); this writes into
$VOXELRUST_MODS_DIR/trains (default ../VoxelRust-Mods/trains):

  textures/<id>.tga   one 16 x 16 picture per rail shape and kind of rail (flat colours, a few of them)
  pack.txt            the lines between `# BEGIN GENERATED TRACK` and `# END GENERATED TRACK`

Why so many blocks: a block's picture is mapped by where on the block it falls, and does not turn
with it, so a curve, a straight piece and a junction each need a picture and a shape of their own.
The mod's code (trains/code) picks the right one by what lies beside a rail, and the names here are
what it reads: `<kind>` is the plain north-south piece, `<kind>_<sides>` a piece joining those sides
(n, w, s, e: `rail_ew`, `rail_ne`, `junction_rail_nws`), `<kind>_up_<side>` a slope that is highest on
that side. Keep the two in step: trains/code/shape.rs has the same tables.
"""
import math
import os
import struct

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))
MOD = os.path.join(MODS_DIR, "trains")

CLEAR = (0, 0, 0, 0)
# The sleepers are the core rail's (painters/gear.rs); the ballast is a cool slate-brown with chips in pairs
# (the house grain of stone, docs/TEXTURING.md "VoxelRust house style").
SLEEPER = (158, 108, 64, 255)
SLEEPER_DARK = (116, 78, 52, 255)
BALLAST = (98, 94, 92, 255)

# kind id, name, rail colour, `tag track` percent, `tag station` seconds (0: none), shapes it may take
KINDS = [
    ("rail", "Rail", (170, 172, 180), 100, 0, "full"),
    ("speed_rail", "Speed Rail", (236, 196, 70), 220, 0, "straight"),
    ("slow_rail", "Slow Rail", (200, 80, 70), 40, 0, "straight"),
    ("station_rail", "Station Rail (6 s stop)", (80, 150, 230), 100, 6, "straight"),
    ("depot_rail", "Depot Rail (30 s stop)", (90, 200, 120), 100, 30, "straight"),
    ("junction_rail", "Junction Rail", (170, 172, 180), 100, 0, "junction"),
]
SIDES = "nwse"  # the order of the sides everywhere: north (-z), west (-x), south (+z), east (+x)
CURVES = ["nw", "sw", "se", "ne"]
TEES = ["nws", "wse", "nse", "nwe"]
SLOPE_TAG = {"n": 1, "w": 2, "s": 3, "e": 4}


def shapes(kind):
    """The shape names a kind has, the first being the one its item places."""
    if kind == "full":
        return ["ns", "ew"] + CURVES + ["up_" + s for s in SIDES]
    if kind == "straight":
        return ["ns", "ew"] + ["up_" + s for s in SIDES]
    return ["nwse"] + TEES


def block_id(kind_id, shape, first):
    return kind_id if shape == first else f"{kind_id}_{shape}"


def shade(c, pct):
    """A darker tone the house way (painters/mod.rs `scale`): red falls a little further, blue a little less, so shadows lean cool."""
    return (c[0] * pct * 95 // 10000, c[1] * pct // 100, min(255, c[2] * pct * 105 // 10000), 255)


def pale(c):
    """The sparkle: about half way to warm white (painters/mod.rs `pale`)."""
    return (c[0] + (255 - c[0]) * 55 // 100, c[1] + (255 - c[1]) * 50 // 100, c[2] + (255 - c[2]) * 44 // 100, 255)


BALLAST_CHIP = shade(BALLAST, 86)
# Chips of ballast between the sleepers, in pairs (x, y) of the north-south piece.
CHIPS = [(6, 3), (7, 3), (8, 7), (9, 7), (6, 11), (7, 11)]


def arm_ns(x, y, light):
    """The north-south piece: two rails on sleepers, ten wide."""
    if not 3 <= x <= 12:
        return CLEAR
    if x in (4, 11):
        # One sparkle on the lit rail, at the upper end of the piece.
        return pale(light) if (x, y) == (4, 2) else light
    if x in (5, 10):
        return shade(light, 70)
    if 6 <= x <= 9:
        if (x, y) in CHIPS:
            return BALLAST_CHIP
        return SLEEPER if y % 4 == 1 else SLEEPER_DARK if y % 4 == 2 else BALLAST
    return BALLAST


def arm_ew(x, y, light):
    return arm_ns(y, x, light)


def curve(x, y, light):
    """The north-east corner: an arc about the corner (16, 0)."""
    dx, dy = 16 - (x + 0.5), y + 0.5
    d = math.hypot(dx, dy)
    if not 3 <= d < 13:
        return CLEAR
    if 11 <= d < 12 or 4 <= d < 5:
        return light
    if 10 <= d < 11 or 5 <= d < 6:
        return shade(light, 70)
    if 6 <= d < 10:
        k = int(math.degrees(math.atan2(dy, dx)) // 15) % 3
        return SLEEPER if k == 0 else SLEEPER_DARK if k == 1 else BALLAST
    return BALLAST


def junction(x, y, sides, light):
    """A piece joining the given sides (three or four): the arms of each and rails meeting in the middle."""
    inside = 3 <= x <= 12
    if inside and y <= 2:
        return arm_ns(x, y, light) if "n" in sides else CLEAR
    if inside and y >= 13:
        return arm_ns(x, y, light) if "s" in sides else CLEAR
    if 3 <= y <= 12 and x <= 2:
        return arm_ew(x, y, light) if "w" in sides else CLEAR
    if 3 <= y <= 12 and x >= 13:
        return arm_ew(x, y, light) if "e" in sides else CLEAR
    if not (inside and 3 <= y <= 12):
        return CLEAR
    lines = []
    if ("n" in sides and y <= 7) or ("s" in sides and y >= 8):
        lines.append(x)
    if ("w" in sides and x <= 7) or ("e" in sides and x >= 8):
        lines.append(y)
    if any(v in (4, 11) for v in lines):
        return light
    if any(v in (5, 10) for v in lines):
        return shade(light, 70)
    return BALLAST


def picture(kind_id, light, shape):
    """The 16 x 16 pixels (rows from the top) of one shape."""
    light = tuple(light[:3]) + (255,)

    def at(x, y):
        if shape in ("ns", "up_n", "up_s"):
            return arm_ns(x, y, light)
        if shape in ("ew", "up_e", "up_w"):
            return arm_ew(x, y, light)
        if shape in CURVES:
            mx = x if "e" in shape else 15 - x
            my = y if "n" in shape else 15 - y
            return curve(mx, my, light)
        return junction(x, y, shape, light)
    return [[at(x, y) for x in range(16)] for y in range(16)]


def write_tga(path, rows):
    data = bytearray()
    for row in rows:
        for r, g, b, a in row:
            data += bytes((b, g, r, a))
    head = struct.pack("<BBBHHBHHHHBB", 0, 0, 2, 0, 0, 0, 0, 0, 16, 16, 32, 0x28)
    with open(path, "wb") as f:
        f.write(head + bytes(data))


def boxes(shape):
    """The boxes of a shape, in sixteenths (it is a 2 high bed on the floor of the block; a slope is steps)."""
    arms = {"n": "3 0 0 13 2 3", "s": "3 0 13 13 2 16", "w": "0 0 3 3 2 13", "e": "13 0 3 16 2 13"}
    mid = "3 0 3 13 2 13"
    if shape == "ns":
        return ["3 0 0 13 2 16"]
    if shape == "ew":
        return ["0 0 3 16 2 13"]
    if shape.startswith("up_"):
        high = shape[3]
        out = []
        for i in range(4):  # i = 0 is the low end; each step is 4 long and the rail rides 2 + the way along it
            lo, hi, top = 4 * i, 4 * i + 4, 4 + 4 * i
            if high == "n":
                out.append(f"3 0 {16 - hi} 13 {top} {16 - lo}")
            elif high == "s":
                out.append(f"3 0 {lo} 13 {top} {hi}")
            elif high == "w":
                out.append(f"{16 - hi} 0 3 {16 - lo} {top} 13")
            else:
                out.append(f"{lo} 0 3 {hi} {top} 13")
        return out
    # Curves and junctions: the middle square and an arm toward each joined side.
    return [mid] + [arms[s] for s in shape]


def section():
    out = []
    w = out.append
    for kid, name, light, pct, station, kind in KINDS:
        names = shapes(kind)
        first = names[0]
        for shape in names:
            bid = block_id(kid, shape, first)
            os.makedirs(os.path.join(MOD, "textures"), exist_ok=True)
            # Slopes use the straight pictures of the way they run.
            pic = shape
            tile = bid
            if shape.startswith("up_"):
                pic = "ns" if shape[3] in "ns" else "ew"
                tile = block_id(kid, pic, first)
            else:
                write_tga(os.path.join(MOD, "textures", tile + ".tga"), picture(kid, light, pic))
            w(f"block {bid}")
            w(f"  name {name}")
            w(f"  tex {tile}")
            if shape == first:
                w("  category Travel")
            else:
                w("  hidden")
                w(f"  drops {kid}")
            w("  hardness 0.4")
            w("  solid no")
            for b in boxes(shape):
                w(f"  box {b}")
            w(f"  tag track {pct}")
            if station:
                w(f"  tag station {station}")
            if shape.startswith("up_"):
                w(f"  tag slope {SLOPE_TAG[shape[3]]}")
    return out


def textures():
    """`texture` lines for every picture written."""
    out = []
    for kid, _, light, _, _, kind in KINDS:
        names = shapes(kind)
        for shape in names:
            if shape.startswith("up_"):
                continue
            tile = block_id(kid, shape, names[0])
            out.append(f"texture {tile} file textures/{tile}.tga")
    return out


def main():
    path = os.path.join(MOD, "pack.txt")
    text = open(path).read()
    for tag, lines in (("TEXTURES", textures()), ("TRACK", section())):
        # section() writes the pictures too, so it must run before the text is cut up: it already did.
        begin, end = f"# BEGIN GENERATED {tag}", f"# END GENERATED {tag}"
        a, b = text.index(begin), text.index(end)
        text = text[:a] + begin + " (tools/gen_trains.py)\n" + "\n".join(lines) + "\n" + text[b:]
    open(path, "w").write(text)
    print("wrote", path)


if __name__ == "__main__":
    main()
