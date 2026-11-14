#!/usr/bin/env python3
"""Generates the big prop models of the Holidays mod (mods/holidays/models).

    python3 tools/gen_props.py

A big prop is one block that stands several blocks wide and tall (`prop` with
`fills W H` in the pack). Its model is plain boxes, eight texels to a block,
feet on y = 0, centred on x = z = 0. Every face is painted from a colour
function of the texel's position, so patterns run round corners. Output is
deterministic.
"""
import base64
import io
import json
import os
import uuid

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))  # the mods live outside the repository
OUT = os.path.join(MODS_DIR, "holidays", "models")
FACES = ("north", "south", "east", "west", "up", "down")


def hsh(*a):
    h = 2166136261
    for v in a:
        h ^= int(v) & 0xFFFFFFFF
        h = (h * 16777619) & 0xFFFFFFFF
        h ^= h >> 13
        h = (h * 0x5BD1E995) & 0xFFFFFFFF
        h ^= h >> 15
    return h / 4294967296.0


def shade(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c)


class Model:
    def __init__(self, name):
        self.name = name
        self.boxes = []

    def box(self, lo, hi, paint):
        """`paint(x, y, z, face)` gives the colour of the texel at that point."""
        self.boxes.append((tuple(map(float, lo)), tuple(map(float, hi)), paint))

    def write(self):
        # Lay the faces out on shelves, tallest first.
        faces = []
        for bi, (lo, hi, _) in enumerate(self.boxes):
            dx, dy, dz = (int(round(hi[i] - lo[i])) for i in range(3))
            for f in FACES:
                w, h = {"north": (dx, dy), "south": (dx, dy), "east": (dz, dy), "west": (dz, dy), "up": (dx, dz), "down": (dx, dz)}[f]
                faces.append((max(h, 1), max(w, 1), bi, f))
        faces.sort(reverse=True)
        size = 64
        while True:
            x = y = 1
            row = 0
            place = {}
            ok = True
            for h, w, bi, f in faces:
                if x + w + 1 > size:
                    x, y, row = 1, y + row + 1, 0
                if y + h + 1 > size or w + 2 > size:
                    ok = False
                    break
                place[(bi, f)] = (x, y, w, h)
                x += w + 1
                row = max(row, h)
            if ok:
                break
            size *= 2
        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        px = img.load()
        elements, ids = [], []
        for bi, (lo, hi, paint) in enumerate(self.boxes):
            uid = str(uuid.uuid5(uuid.NAMESPACE_OID, f"{self.name}/{bi}"))
            ids.append(uid)
            fs = {}
            for f in FACES:
                x0, y0, w, h = place[(bi, f)]
                for v in range(h):
                    for u in range(w):
                        # The middle of this texel, in the model's space.
                        a, b = u + 0.5, v + 0.5
                        if f == "north":
                            p = (hi[0] - a, hi[1] - b, lo[2])
                        elif f == "south":
                            p = (lo[0] + a, hi[1] - b, hi[2])
                        elif f == "east":
                            p = (hi[0], hi[1] - b, hi[2] - a)
                        elif f == "west":
                            p = (lo[0], hi[1] - b, lo[2] + a)
                        elif f == "up":
                            p = (lo[0] + a, hi[1], lo[2] + b)
                        else:
                            p = (lo[0] + a, lo[1], hi[2] - b)
                        c = paint(p[0], p[1], p[2], f)
                        px[x0 + u, y0 + v] = (c[0], c[1], c[2], 255)
                fs[f] = {"uv": [x0, y0, x0 + w, y0 + h], "texture": 0}
            elements.append({"name": f"box{bi}", "type": "cube", "uuid": uid, "from": list(lo), "to": list(hi),
                             "origin": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "faces": fs})
        buf = io.BytesIO()
        img.save(buf, "PNG")
        doc = {
            "meta": {"format_version": "4.10", "model_format": "free", "box_uv": False},
            "name": self.name,
            "resolution": {"width": size, "height": size},
            "elements": elements,
            "outliner": [{"name": "Body", "origin": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "children": ids}],
            "textures": [{"name": self.name + ".png", "source": "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()}],
        }
        os.makedirs(OUT, exist_ok=True)
        path = os.path.join(OUT, self.name + ".bbmodel")
        with open(path, "w") as fh:
            json.dump(doc, fh, separators=(",", ":"))
        print(f"{self.name}: {len(self.boxes)} boxes, texture {size}x{size}")


def speckle(base, seed, amount=0.14):
    def paint(x, y, z, f):
        return shade(base, 1.0 - amount + 2 * amount * hsh(seed, int(x * 2), int(y * 2), int(z * 2)))
    return paint


BAUBLES = [(214, 48, 52), (240, 196, 60), (70, 120, 220), (236, 236, 240), (200, 80, 190)]


def needles(seed):
    """Fir needles with baubles and a few lights scattered on the sides."""
    def paint(x, y, z, f):
        g = shade((34, 104, 52), 0.8 + 0.4 * hsh(seed, int(x), int(y), int(z)))
        if f in ("up", "down"):
            return shade(g, 0.9) if f == "down" else g
        cell = (int(x // 3), int(y // 3), int(z // 3))
        r = hsh(seed + 7, *cell)
        if r > 0.82:
            return BAUBLES[int(hsh(seed + 9, *cell) * len(BAUBLES))]
        if r < 0.06:
            return (255, 244, 170)
        return g
    return paint


def grand_tree():
    m = Model("grand_tree")
    # A 3 x 3 footprint, seven blocks tall: 24 across, 56 high.
    m.box((-2, 0, -2), (2, 10, 2), speckle((104, 70, 40), 3))
    tiers = [(12, 8, 10), (10, 16, 9), (8, 24, 9), (6, 32, 8), (4, 39, 7), (2, 45, 6)]
    for i, (half, y, h) in enumerate(tiers):
        m.box((-half, y, -half), (half, y + h, half), needles(20 + i))
    # The star: two crossed plates.
    gold = speckle((250, 214, 70), 5, 0.08)
    m.box((-3, 50, -0.5), (3, 56, 0.5), gold)
    m.box((-0.5, 50, -3), (0.5, 56, 3), gold)
    # Presents round the foot.
    for k, (x, z, s, c) in enumerate([(-9, -9, 4, (200, 50, 60)), (7, -8, 3, (60, 110, 210)), (-8, 7, 3, (240, 200, 70)), (6, 6, 5, (120, 180, 90))]):
        def wrap(px, py, pz, f, c=c, x=x, z=z, s=s):
            return (245, 245, 245) if abs(px - (x + s / 2)) < 0.6 or abs(pz - (z + s / 2)) < 0.6 else c
        m.box((x, 0, z), (x + s, s, z + s), wrap)
    m.write()


def giant_snowman():
    m = Model("giant_snowman")
    snow = speckle((240, 244, 250), 11, 0.05)
    m.box((-5, 0, -5), (5, 9, 5), snow)

    def chest(x, y, z, f):
        # Coal buttons down the front.
        if f == "north" and abs(x) < 0.6 and int(y) in (11, 13, 15):
            return (30, 30, 34)
        return snow(x, y, z, f)

    def head(x, y, z, f):
        if f == "north":
            if int(y) == 21 and int(abs(x)) == 1:
                return (24, 24, 28)
            if int(y) == 19 and abs(x) < 2.5 and int(abs(x)) != 0:
                return (40, 40, 44)
        return snow(x, y, z, f)

    m.box((-4, 9, -4), (4, 17, 4), chest)
    m.box((-3, 17, -3), (3, 23, 3), head)
    m.box((-0.5, 19.5, -6), (0.5, 20.5, -3), lambda x, y, z, f: (240, 130, 40))  # carrot
    m.box((-3.5, 16.5, -3.5), (3.5, 17.5, 3.5), lambda x, y, z, f: (200, 40, 50) if int(x + z) % 2 else (230, 230, 230))  # scarf
    hat = lambda x, y, z, f: (190, 40, 50) if 23.9 < y < 25 and f not in ("up", "down") else (28, 28, 34)
    m.box((-4, 23, -4), (4, 24, 4), lambda x, y, z, f: (28, 28, 34))
    m.box((-2.5, 24, -2.5), (2.5, 28, 2.5), hat)
    stick = speckle((96, 66, 40), 13)
    m.box((-9, 13, -0.5), (-4, 14, 0.5), stick)
    m.box((4, 13, -0.5), (9, 14, 0.5), stick)
    m.write()


def great_pumpkin():
    m = Model("great_pumpkin")

    def skin(x, y, z, f):
        # Ribs run up the sides.
        rib = (int(x + 40) % 4 == 0) if f in ("north", "south", "up", "down") else (int(z + 40) % 4 == 0)
        c = shade((226, 120, 30), 0.82 if rib else 0.94 + 0.12 * hsh(31, int(x), int(y), int(z)))
        if f == "north":
            glow = (255, 214, 90)
            if 11 <= y < 15 and 3 <= abs(x) < 7 and (y - 11) <= (7 - abs(x)) + 1:
                return glow
            if 4 <= y < 8 and abs(x) < 8 and not (int(x + 40) % 4 == 0 and y >= 6):
                return glow
        return c

    m.box((-11, 0, -11), (11, 3, 11), skin)
    m.box((-12, 3, -12), (12, 17, 12), skin)
    m.box((-10, 17, -10), (10, 20, 10), skin)
    m.box((-1.5, 20, -1.5), (1.5, 24, 1.5), speckle((70, 110, 40), 33))
    m.write()


def maypole():
    m = Model("maypole")
    ribbons = [(230, 70, 90), (250, 210, 80), (90, 160, 230), (120, 200, 110)]

    def pole(x, y, z, f):
        # Ribbons wound round a pale pole.
        k = int((y + (x + z) * 1.5) // 3) % 8
        return ribbons[k // 2] if k % 2 == 0 else (236, 230, 214)

    m.box((-1, 0, -1), (1, 44, 1), pole)
    m.box((-4, 44, -4), (4, 46, 4), speckle((120, 190, 100), 41))
    flowers = lambda x, y, z, f: ribbons[int(hsh(43, int(x), int(z)) * 4)] if hsh(44, int(x), int(y), int(z)) > 0.5 else (90, 170, 80)
    m.box((-3, 46, -3), (3, 48, 3), flowers)
    # Streamers hanging from the crown.
    for i, (dx, dz) in enumerate([(-4, 0), (4, 0), (0, -4), (0, 4)]):
        c = ribbons[i]
        m.box((dx - 0.5, 30, dz - 0.5), (dx + 0.5, 44, dz + 0.5), lambda x, y, z, f, c=c: c)
    m.write()


if __name__ == "__main__":
    grand_tree()
    giant_snowman()
    great_pumpkin()
    maypole()
