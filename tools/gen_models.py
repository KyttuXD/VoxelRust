#!/usr/bin/env python3
"""Generates the creature and vehicle models in data/models/ and data/creatures.txt.

    python3 tools/gen_models.py                 # write the models, validate
    python3 tools/gen_models.py --creatures     # also rewrite creatures.txt (it has hand edits)
    python3 tools/gen_models.py --sheet out.png # also render a contact sheet
    python3 tools/gen_models.py --only wolf,fox # just these models

Each model is a set of boxes hung on named bones (the game animates bones by
name: `Head`, `Leg-FL`, `L-Leg`, ...). Every box face gets its own area in the
model's texture. The base colour of a texel comes from a "shader" evaluated at
the texel's 3D position (so stripes, patches and belly shading run coherently
across faces), then per-face "decals" paint the eyes, noses and so on.

Conventions: y is up, the front of a model faces -z, its left side is -x,
feet stand on y = 0, one model unit is one texel. Output is deterministic.
"""
import base64
import io
import json
import math
import os
import sys
import uuid

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(ROOT, "data", "models")
CREATURES = os.path.join(ROOT, "data", "creatures.txt")
FACES = ("north", "south", "east", "west", "up", "down")
# Brightness the game gives each face (src/model.rs), for the preview.
FACE_SHADE = {"east": 200, "west": 200, "up": 255, "down": 140, "south": 228, "north": 228}
MAX_TEX = 128

WHITE = (250, 250, 250)
BLACK = (16, 14, 16)


# ---------------------------------------------------------------- helpers

def hsh(*a):
    """Deterministic hash of some integers -> 0..1."""
    h = 2166136261
    for v in a:
        h ^= int(v) & 0xFFFFFFFF
        h = (h * 16777619) & 0xFFFFFFFF
        h ^= h >> 13
        h = (h * 0x5BD1E995) & 0xFFFFFFFF
        h ^= h >> 15
    return h / 4294967296.0


def vnoise(seed, x, y, z):
    """Smooth 3D value noise, 0..1."""
    xi, yi, zi = math.floor(x), math.floor(y), math.floor(z)
    fx, fy, fz = x - xi, y - yi, z - zi
    fx, fy, fz = (t * t * (3 - 2 * t) for t in (fx, fy, fz))
    v = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (fx if dx else 1 - fx) * (fy if dy else 1 - fy) * (fz if dz else 1 - fz)
                v += w * hsh(seed, xi + dx, yi + dy, zi + dz)
    return v


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def mul(c, f):
    return tuple(v * f for v in c[:3])


def clamp8(v):
    return max(0, min(255, int(round(v))))


class P:
    """What a shader gets to know about one texel."""
    __slots__ = ("x", "y", "z", "lx", "ly", "lz", "face", "i", "j", "w", "h", "part")

    def side(self):
        return self.face in ("east", "west", "north", "south")


class Canvas:
    """One face of one box, for decals. (0,0) is the top-left seen from outside."""

    def __init__(self, face, w, h, px, part):
        self.face, self.w, self.h, self.p, self.part = face, w, h, px, part

    def px(self, x, y, c):
        x, y = int(x), int(y)
        if 0 <= x < self.w and 0 <= y < self.h:
            self.p[y][x] = (clamp8(c[0]), clamp8(c[1]), clamp8(c[2]), c[3] if len(c) > 3 else 255)

    def rect(self, x, y, w, h, c):
        for yy in range(int(y), int(y + h)):
            for xx in range(int(x), int(x + w)):
                self.px(xx, yy, c)

    def sym(self, x, y, c):
        self.px(x, y, c)
        self.px(self.w - 1 - x, y, c)

    def symrect(self, x, y, w, h, c):
        self.rect(x, y, w, h, c)
        self.rect(self.w - x - w, y, w, h, c)

    def fx(self, k):
        """On a side (east/west) face: the column k texels back from the front."""
        return self.w - 1 - k if self.face == "east" else k

    def eyes(self, x, y, pupil=BLACK, white=WHITE):
        """A pair of eyes on a front face: white outside, pupil inside."""
        self.sym(x, y, white)
        self.sym(x + 1, y, pupil)

    def side_eye(self, k, y, pupil=BLACK, white=None):
        """An eye on an east/west face, k texels back from the front."""
        if self.face in ("east", "west"):
            self.px(self.fx(k), y, pupil)
            if white:
                self.px(self.fx(k + 1), y, white)


def face_frame(f, lo, hi):
    """Top-left, top-right and bottom-left corners of a face, as src/model.rs maps them."""
    if f == "east":
        return (hi[0], hi[1], hi[2]), (hi[0], hi[1], lo[2]), (hi[0], lo[1], hi[2])
    if f == "west":
        return (lo[0], hi[1], lo[2]), (lo[0], hi[1], hi[2]), (lo[0], lo[1], lo[2])
    if f == "up":
        return (lo[0], hi[1], lo[2]), (hi[0], hi[1], lo[2]), (lo[0], hi[1], hi[2])
    if f == "down":
        return (lo[0], lo[1], hi[2]), (hi[0], lo[1], hi[2]), (lo[0], lo[1], lo[2])
    if f == "south":
        return (lo[0], hi[1], hi[2]), (hi[0], hi[1], hi[2]), (lo[0], lo[1], hi[2])
    return (hi[0], hi[1], lo[2]), (lo[0], hi[1], lo[2]), (hi[0], lo[1], lo[2])  # north


def face_dims(f, lo, hi):
    d = [hi[a] - lo[a] for a in range(3)]
    if f in ("east", "west"):
        w, h = d[2], d[1]
    elif f in ("up", "down"):
        w, h = d[0], d[2]
    else:
        w, h = d[0], d[1]
    return max(1, int(round(w))), max(1, int(round(h)))


class Part:
    pass


class Model:
    def __init__(self, mid, seed, noise=7):
        self.id, self.seed, self.noise = mid, seed, noise
        self.parts = []
        self.bones = {"Body": ((0, 0, 0), None)}

    def bone(self, name, pivot, parent="Body"):
        self.bones[name] = (tuple(pivot), parent)

    def box(self, name, bone, lo, hi, shader, decal=None, rot=None, origin=None, skip=(), noise=None):
        assert all(lo[a] < hi[a] for a in range(3)), (self.id, name, lo, hi)
        assert bone in self.bones, (self.id, bone)
        p = Part()
        p.name, p.bone, p.lo, p.hi = name, bone, tuple(map(float, lo)), tuple(map(float, hi))
        p.shader = shader if callable(shader) else (lambda _p, c=shader: c)
        p.decal, p.skip = decal, tuple(skip)
        p.rot = tuple(rot) if rot else (0, 0, 0)
        p.origin = tuple(origin) if origin else tuple((lo[a] + hi[a]) / 2 for a in range(3))
        p.noise = self.noise if noise is None else noise
        self.parts.append(p)
        return p

    def pair(self, name, bones, lo, hi, shader, decal=None, rot=None, origin=None, skip=(), noise=None):
        """A box on the left (-x) side and its mirror image on the right."""
        bl, br = bones if isinstance(bones, tuple) else (bones, bones)
        self.box(name + "-l", bl, lo, hi, shader, decal, rot, origin, skip, noise)
        mlo, mhi = (-hi[0], lo[1], lo[2]), (-lo[0], hi[1], hi[2])
        mrot = (rot[0], -rot[1], -rot[2]) if rot else None
        morg = (-origin[0], origin[1], origin[2]) if origin else None
        self.box(name + "-r", br, mlo, mhi, shader, decal, mrot, morg, skip, noise)

    # -- texture

    def paint(self):
        for pi, part in enumerate(self.parts):
            part.tex = {}
            lo, hi = part.lo, part.hi
            for fi, f in enumerate(FACES):
                if f in part.skip:
                    continue
                w, h = face_dims(f, lo, hi)
                tl, tr, bl = face_frame(f, lo, hi)
                px = []
                for j in range(h):
                    row = []
                    for i in range(w):
                        u, v = (i + 0.5) / w, (j + 0.5) / h
                        p = P()
                        p.x, p.y, p.z = (tl[a] + (tr[a] - tl[a]) * u + (bl[a] - tl[a]) * v for a in range(3))
                        p.lx = (p.x - lo[0]) / (hi[0] - lo[0])
                        p.ly = (p.y - lo[1]) / (hi[1] - lo[1])
                        p.lz = (p.z - lo[2]) / (hi[2] - lo[2])
                        p.face, p.i, p.j, p.w, p.h, p.part = f, i, j, w, h, part.name
                        c = part.shader(p)
                        a = c[3] if len(c) > 3 else 255
                        # Soft mottling, a touch of shade towards the bottom, texel grain.
                        k = 1.0
                        if part.noise:
                            k = 0.95 + 0.10 * vnoise(self.seed + 7, p.x / 2.7, p.y / 2.7, p.z / 2.7)
                            if f not in ("up", "down"):
                                k *= 0.90 + 0.10 * p.ly
                            n = (hsh(self.seed, pi, fi, i, j) - 0.5) * 2 * part.noise
                        else:
                            n = 0
                        row.append((clamp8(c[0] * k + n), clamp8(c[1] * k + n), clamp8(c[2] * k + n), a))
                    px.append(row)
                if part.decal:
                    part.decal(Canvas(f, w, h, px, part.name))
                part.tex[f] = px

    def pack(self):
        """Shelf-packs every face (with a 1 px replicated gutter) into the smallest texture."""
        rects = []
        for pi, part in enumerate(self.parts):
            for f in FACES:
                if f in part.tex:
                    px = part.tex[f]
                    rects.append((len(px), len(px[0]), pi, f))
        rects.sort(key=lambda r: (-r[0], -r[1], r[2], r[3]))
        for tw, th in ((32, 32), (64, 32), (64, 64), (128, 64), (128, 128)):
            pos, x, y, shelf = {}, 0, 0, 0
            ok = True
            for h, w, pi, f in rects:
                if x + w + 2 > tw:
                    x, y, shelf = 0, y + shelf, 0
                if y + h + 2 > th or w + 2 > tw:
                    ok = False
                    break
                pos[(pi, f)] = (x + 1, y + 1)
                x += w + 2
                shelf = max(shelf, h + 2)
            if ok:
                break
        else:
            raise SystemExit("%s: texture does not fit in %dx%d" % (self.id, MAX_TEX, MAX_TEX))
        img = Image.new("RGBA", (tw, th), (0, 0, 0, 0))
        out = img.load()
        for (pi, f), (x0, y0) in pos.items():
            px = self.parts[pi].tex[f]
            h, w = len(px), len(px[0])
            for j in range(-1, h + 1):
                for i in range(-1, w + 1):
                    out[x0 + i, y0 + j] = px[min(max(j, 0), h - 1)][min(max(i, 0), w - 1)]
            self.parts[pi].uv = getattr(self.parts[pi], "uv", {})
            self.parts[pi].uv[f] = [x0, y0, x0 + w, y0 + h]
        self.image = img

    # -- output

    def to_json(self):
        self.paint()
        for p in self.parts:
            p.uv = {}
        self.pack()
        ns = uuid.UUID("6f1d2c1e-8f0a-4a57-9c7e-2b1a7d3c9e10")
        elements, by_bone = [], {}
        for i, p in enumerate(self.parts):
            p.uuid = str(uuid.uuid5(ns, "%s/%d/%s" % (self.id, i, p.name)))
            elements.append({
                "name": p.name, "type": "cube", "uuid": p.uuid,
                "from": list(p.lo), "to": list(p.hi),
                "origin": list(map(float, p.origin)), "rotation": list(map(float, p.rot)),
                "faces": {f: {"uv": p.uv[f], "texture": 0} for f in FACES if f in p.uv},
            })
            by_bone.setdefault(p.bone, []).append(p.uuid)

        def node(name):
            pivot, _ = self.bones[name]
            kids = list(by_bone.get(name, []))
            kids += [node(b) for b, (_, par) in self.bones.items() if par == name]
            return {"name": name, "origin": list(map(float, pivot)), "rotation": [0.0, 0.0, 0.0], "children": kids}

        buf = io.BytesIO()
        self.image.save(buf, "PNG", optimize=False)
        return {
            "meta": {"format_version": "4.10", "model_format": "free", "box_uv": False},
            "name": self.id,
            "resolution": {"width": self.image.width, "height": self.image.height},
            "elements": elements,
            "outliner": [node(b) for b, (_, par) in self.bones.items() if par is None],
            "textures": [{"name": self.id + ".png",
                          "source": "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode("ascii")}],
        }


def four_legs(m, x0, x1, zf, zb, depth, top, shader, decal=None, sink=1.0):
    """Four legs x0..x1 wide (left side, x0 < x1 <= 0), front ones starting at zf, back ones at zb."""
    for tag, z in (("F", zf), ("B", zb)):
        cx, cz = (x0 + x1) / 2, z + depth / 2
        m.bone("Leg-%sL" % tag, (cx, top, cz))
        m.bone("Leg-%sR" % tag, (-cx, top, cz))
        m.pair("leg-" + tag.lower(), ("Leg-%sL" % tag, "Leg-%sR" % tag), (x0, 0, z), (x1, top + sink, z + depth), shader, decal)


def wheel(m, name, bone, x0, x1, cy, cz, r, tire=(38, 38, 42), hub=(150, 150, 155), cut=None):
    """A plus-shaped (corner-cut) wheel in the y/z plane."""
    c = cut if cut is not None else max(1, int(round(r * 0.34)))

    def sh(p):
        d = math.hypot(p.y - cy, p.z - cz)
        if p.face in ("east", "west"):
            if d < r * 0.22:
                return mul(hub, 0.6)
            if d < r * 0.58:
                return hub
            return tire
        return tire if (p.i + p.j) % 2 else mul(tire, 1.35)

    m.box(name + "-a", bone, (x0, cy - r, cz - r + c), (x1, cy + r, cz + r - c), sh)
    m.box(name + "-b", bone, (x0 + 0.1, cy - r + c, cz - r), (x1 - 0.1, cy + r - c, cz + r), sh)


# ---------------------------------------------------------------- animals

def cow():
    m = Model("cow", 101)
    W, K, PK, HF, HN = (240, 238, 230), (46, 42, 42), (232, 170, 166), (64, 54, 48), (228, 218, 190)

    def patch(p):
        return vnoise(101, p.x / 6.0 + 3, p.y / 6.0, p.z / 6.0) > 0.52

    def body(p):
        return K if patch(p) else W

    def leg(p):
        if p.y < 2:
            return HF
        return K if p.y > 6 and patch(p) else W

    def head(p):
        if p.face in ("north", "up") and abs(p.x) < 1.2:
            return W
        if p.face == "down":
            return W
        return K

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 3)
            c.rect(3, 0, 2, 2, (240, 238, 230))
        c.side_eye(1, 3, BLACK, WHITE)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (150, 90, 95))
            c.rect(1, 3, c.w - 2, 1, (170, 105, 108))

    def tail(p):
        return K if p.ly < 0.25 else W

    m.box("body", "Body", (-6, 10, -9), (6, 21, 10), body)
    m.box("udder", "Body", (-3, 8, 2), (3, 10.5, 7), PK,
          lambda c: c.face == "down" and [c.sym(1, 1, (200, 130, 130)), c.sym(1, 3, (200, 130, 130))])
    m.box("tail", "Body", (-0.5, 9, 9.5), (0.5, 20, 10.5), tail, rot=(-8, 0, 0), origin=(0, 20, 10))
    m.bone("Head", (0, 18, -9))
    m.box("head", "Head", (-4, 14, -16), (4, 22, -8.5), head, head_d)
    m.box("muzzle", "Head", (-3, 14, -18), (3, 18, -15.5), PK, muzzle_d)
    m.pair("horn", "Head", (-5.5, 21, -13.5), (-4, 24.5, -12), HN)
    m.pair("ear", "Head", (-7, 18, -11.5), (-3.5, 20, -10.5), lambda p: PK if p.face == "north" and 0.2 < p.lx < 0.8 else K)
    four_legs(m, -6, -3, -8, 6, 3, 10, leg)
    return m


def pig():
    m = Model("pig", 102)
    PK, DK, HF = (240, 176, 170), (212, 130, 128), (120, 84, 80)

    def skin(p):
        c = PK
        if vnoise(102, p.x / 4.0, p.y / 4.0, p.z / 4.0) > 0.68:
            c = mix(PK, DK, 0.45)
        if p.face == "down":
            c = mix(c, (250, 205, 200), 0.6)
        return c

    def leg(p):
        return HF if p.y < 1.5 else skin(p)

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 3)
            c.rect(2, 7, 6, 1, (190, 110, 110))
        c.side_eye(1, 3)

    def snout_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, c.h, (226, 142, 140))
            c.sym(1, 1, (140, 70, 78))

    m.box("body", "Body", (-5, 5, -8), (5, 14, 9), skin)
    m.box("tail-a", "Body", (-0.5, 10, 8.5), (0.5, 13, 9.8), DK)
    m.box("tail-b", "Body", (0.5, 12, 8.8), (1.8, 13.2, 10.2), DK)
    m.box("tail-c", "Body", (0.9, 10.6, 9.6), (2.0, 12, 10.8), DK)
    m.bone("Head", (0, 10, -8))
    m.box("head", "Head", (-4.5, 6, -15), (4.5, 15, -7.5), skin, head_d)
    m.box("snout", "Head", (-2, 7, -17), (2, 10, -14.5), DK, snout_d)
    m.pair("ear", "Head", (-5, 12.5, -15.5), (-2, 15.5, -14.2), DK, rot=(-25, 0, 12), origin=(-3.5, 15.5, -14.5))
    four_legs(m, -5, -2, -7.5, 5.5, 3, 5, leg)
    return m


def chicken():
    m = Model("chicken", 103)
    W, RED, YL, OR = (244, 242, 236), (200, 40, 36), (240, 190, 60), (226, 150, 50)

    def feathers(p):
        c = W
        if p.side() and (int(p.z + 50) + int(p.y + 50) * 2) % 4 == 0:
            c = (222, 220, 212)
        return c

    def wing(p):
        c = (228, 226, 218)
        if (p.i + p.j * 2) % 4 == 0 or p.j == p.h - 1:
            c = (200, 198, 192)
        return c

    def head_d(c):
        if c.face == "north":
            c.sym(0, 2, BLACK)
        c.side_eye(1, 2)

    def tail(p):
        return (214, 212, 204) if p.i % 2 == 0 else W

    m.box("body", "Body", (-4, 4, -5), (4, 12, 5), feathers)
    m.pair("wing", "Body", (-5, 6, -4), (-3.8, 11, 3), wing)
    m.box("tail", "Body", (-3, 10, 3.5), (3, 16, 5.5), tail, rot=(-28, 0, 0), origin=(0, 10, 4.5))
    m.bone("Head", (0, 12, -4))
    m.box("head", "Head", (-2.5, 10, -8), (2.5, 18, -3.5), feathers, head_d)
    m.box("beak", "Head", (-1.5, 13, -10.5), (1.5, 15, -7.5), YL,
          lambda c: c.face in ("east", "west", "north") and c.rect(0, 1, c.w, 1, OR))
    m.box("wattle", "Head", (-1, 10.5, -9), (1, 13.2, -7.8), RED)
    m.box("comb", "Head", (-0.5, 17.5, -8), (0.5, 20, -4), lambda p: RED + ((0,) if p.face in ("east", "west") and p.j == 0 and p.i % 2 else (255,)),
          skip=("up",))
    for tag, sx in (("L", -1), ("R", 1)):
        x = 2.2 * sx
        m.bone(tag + "-Leg", (x, 4.2, 0.5))
        m.box("leg-" + tag.lower(), tag + "-Leg", (x - 0.5, 0.5, 0), (x + 0.5, 5, 1), OR)
        m.box("foot-" + tag.lower(), tag + "-Leg", (x - 1.5, 0, -2.5), (x + 1.5, 0.8, 1.2),
              lambda p: OR + ((0,) if p.face in ("up", "down") and p.i == 1 and (p.j < 1 if p.face == "up" else p.j > p.h - 2) else (255,)))
    return m


def cat():
    m = Model("cat", 104)
    G, D, W, PK = (196, 192, 186), (112, 108, 104), (244, 242, 238), (232, 150, 150)

    def stripe(p, c0, period=3):
        wob = 2.2 * vnoise(104, p.x / 3.0, p.y / 3.0, p.z / 7.0)
        return int(math.floor(p.z + wob + 100)) % period == 0

    def body(p):
        if p.face == "down" or p.y < 7.2:
            return W
        c = D if stripe(p, G) and p.y > 8 else G
        if p.face == "up" and abs(p.x) < 0.8:
            c = D
        return c

    def leg(p):
        if p.y < 2:
            return W
        return D if int(p.y) % 3 == 1 else G

    def head(p):
        if p.face == "down":
            return W
        if p.face == "up":
            return D if int(p.x + 10) % 2 == 0 and abs(p.x) < 2.5 else G
        if p.face in ("east", "west") and int(p.y) % 2 == 0 and p.lz > 0.4:
            return D
        return G

    def head_d(c):
        if c.face == "north":
            # forehead "M", eyes, white lower face
            c.px(2, 0, D), c.px(3, 0, D), c.px(4, 0, D)
            c.sym(1, 2, (120, 190, 80))
            c.sym(1, 1, D)
            c.rect(2, 3, 3, 2, W)
            c.rect(1, 4, 5, 1, W)

    def muzzle_d(c):
        if c.face == "north":
            c.px(1, 0, PK)
            c.px(1, 1, (150, 120, 120))

    def tail(p):
        return D if int(p.lz * 6) % 2 == 1 else G

    m.box("body", "Body", (-2.5, 6, -7), (2.5, 11.5, 7), body)
    m.box("tail-a", "Body", (-1, 9.2, 6.5), (1, 11.2, 12), tail, rot=(-38, 0, 0), origin=(0, 10.2, 6.5))
    m.box("tail-b", "Body", (-0.9, 12.6, 10.2), (0.9, 14.4, 15), lambda p: D if p.lz > 0.55 else G, rot=(-72, 0, 0), origin=(0, 13.5, 10.2))
    m.bone("Head", (0, 11, -7))
    m.box("head", "Head", (-3.5, 8.5, -12), (3.5, 14.5, -6.5), head, head_d)
    m.box("muzzle", "Head", (-1.5, 9, -13), (1.5, 11, -11.5), W, muzzle_d)
    m.pair("ear", "Head", (-3.5, 14, -10.5), (-1.5, 16.5, -9.2),
           lambda p: PK if p.face == "north" and p.j > 0 and abs(p.x) < 2.5 else D if p.j == 0 else G)
    four_legs(m, -2.5, -0.7, -6.5, 4.5, 2, 6, leg)
    return m


def dog():
    m = Model("dog", 105)
    T, S, W, NB = (226, 196, 150), (120, 88, 60), (246, 240, 228), (30, 26, 26)

    def body(p):
        if p.face == "down" or p.y < 8.2:
            return W
        # darker saddle over the back
        if p.y > 11 + 1.5 * vnoise(105, p.x / 3.0, 0, p.z / 3.0) and -4 < p.z < 7:
            return S
        return T

    def chest(p):
        if p.face == "down" or (p.face == "north" and abs(p.x) < 2.5) or p.y < 8:
            return W
        return T

    def leg(p):
        return W if p.y < 2.5 else T

    def head(p):
        if p.face == "north" and abs(p.x) < 1 or p.face == "up" and abs(p.x) < 1 and p.lz < 0.6:
            return W
        if p.face == "down":
            return W
        return T

    def head_d(c):
        if c.face == "north":
            c.sym(1, 1, (60, 40, 30))
            c.sym(1, 0, S)

    def snout_d(c):
        if c.face == "north":
            c.rect(1, 0, c.w - 2, 1, NB)
            c.rect(0, c.h - 1, c.w, 1, (90, 50, 50))
            c.px(c.w // 2, c.h - 1, (220, 110, 120))
        elif c.face == "up":
            c.rect(1, 0, c.w - 2, 1, NB)
        elif c.face in ("east", "west"):
            c.rect(0, c.h - 1, c.w, 1, (90, 60, 55))

    def tail(p):
        return W if p.lz > 0.75 else S if p.face == "up" else T

    m.box("body", "Body", (-3, 7, -3), (3, 14, 8), body)
    m.box("chest", "Body", (-3.5, 6.5, -8), (3.5, 14.8, -2.5), chest)
    m.box("tail", "Body", (-1, 12, 7.5), (1, 14, 14), tail, rot=(-40, 0, 0), origin=(0, 13, 7.5))
    m.bone("Head", (0, 14, -8))
    m.box("head", "Head", (-3, 12.5, -13), (3, 18.5, -7.5), head, head_d)
    m.box("snout", "Head", (-1.5, 12.5, -17), (1.5, 15.5, -12.5), lambda p: W if p.face != "up" else T, snout_d)
    m.pair("ear", "Head", (-4, 14, -11.5), (-2.8, 18.8, -8.5), S)
    four_legs(m, -3.3, -1.3, -7.5, 5.5, 2, 7, leg)
    return m


def tiger():
    m = Model("tiger", 106)
    O, K, W, PK = (232, 140, 44), (30, 24, 22), (246, 240, 226), (226, 140, 140)

    def stripe(p):
        wob = 2.6 * vnoise(106, p.x / 3.0, p.y / 3.5, p.z / 9.0)
        return int(math.floor(p.z + wob + 100)) % 4 == 0

    def body(p):
        if p.face == "down" or p.y < 10.5:
            return K if stripe(p) and p.face != "down" and p.y > 9.6 else W
        if p.face == "up" and abs(p.x) < 1 and not stripe(p):
            return mul(O, 0.9)
        return K if stripe(p) else O

    def leg(p):
        if p.y < 1.5:
            return W
        inner = (p.face == "east" and p.x < 0) or (p.face == "west" and p.x > 0)
        base = W if inner else O
        return K if int(p.y + 0.5) % 3 == 0 and p.y > 2 else base

    def head(p):
        if p.face == "down":
            return W
        if p.face == "up":
            return K if int(p.z + 100) % 2 == 0 and abs(p.x) > 0.8 and abs(p.x) < 3.2 else O
        if p.face in ("east", "west"):
            if p.y < 15.5:
                return K if int(p.z + 100) % 3 == 0 and p.y > 14 else W
            return K if int(p.y) % 2 == 0 and p.lz > 0.35 else O
        return O

    def head_d(c):
        if c.face == "north":
            c.rect(0, 5, 8, 3, W)
            c.symrect(0, 4, 2, 1, W)
            c.eyes(1, 2, BLACK, (240, 220, 90))
            c.sym(1, 1, W)
            c.sym(2, 0, K), c.px(3, 1, K), c.px(4, 1, K), c.sym(0, 3, K), c.sym(0, 6, K)

    def muzzle_d(c):
        if c.face == "north":
            c.rect(1, 0, 3, 1, PK)
            c.px(2, 1, PK)
            c.rect(1, 3, 3, 1, (120, 60, 60))
            c.sym(0, 2, (150, 150, 150))
        elif c.face == "up":
            c.rect(1, 0, 3, c.h, O)

    def tail(p):
        return K if int(p.lz * 7) % 2 == 1 else O

    m.box("body", "Body", (-4, 9, -11), (4, 18, 11), body)
    m.box("tail-a", "Body", (-1, 14, 10.5), (1, 16, 18), tail, rot=(28, 0, 0), origin=(0, 15, 10.5))
    m.box("tail-b", "Body", (-0.9, 10.2, 16.6), (0.9, 12, 23), lambda p: K if p.lz > 0.6 or int(p.lz * 6) % 2 else O, rot=(-22, 0, 0), origin=(0, 11.1, 16.6))
    m.bone("Head", (0, 16, -11))
    m.box("head", "Head", (-4, 13, -18), (4, 21, -10.5), head, head_d)
    m.box("muzzle", "Head", (-2.5, 13, -20), (2.5, 17, -17.5), W, muzzle_d)
    m.pair("ear", "Head", (-4, 20.5, -15), (-2, 23, -13.8), lambda p: W if p.face == "south" and p.j == 1 else K if p.j == 0 else O)
    m.pair("cheek", "Head", (-5, 13, -16), (-3.8, 17.5, -12), lambda p: K if p.j % 2 == 0 and p.face in ("east", "west") and p.i % 2 == 0 else W)
    four_legs(m, -4.2, -1.2, -10.5, 7.5, 3, 9, leg)
    return m


def elephant():
    m = Model("elephant", 107)
    G, D, L, IV = (150, 146, 148), (112, 108, 112), (176, 172, 172), (240, 234, 212)

    def wrinkle(p, f=1.0):
        return abs(vnoise(107, p.x / 7.0, p.y * 0.55 * f, p.z / 7.0) - 0.5) < 0.035

    def skin(p):
        c = G
        if wrinkle(p):
            c = D
        if p.face == "down":
            c = mix(c, D, 0.5)
        return c

    def leg(p):
        if p.y < 2 and p.face in ("north", "east", "west") and p.i % 2 == 1 and not (p.face != "north" and p.lz > 0.6):
            return (222, 214, 196)
        return D if int(p.y) % 4 == 3 and hsh(107, p.i, int(p.y)) > 0.3 else G

    def trunk(p):
        return D if int(p.y + 0.5) % 2 == 0 and p.face != "south" else G

    def ear(p):
        c = mix(G, (176, 150, 150), 0.5) if p.face == "north" else G
        if p.face in ("north", "south"):
            edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j)
            if edge == 0:
                c = D
            outer = p.lx < 0.5 if p.x < 0 else p.lx > 0.5
            if outer and p.j >= p.h - 2 and edge == 0 and p.j == p.h - 1 and (p.i < 2 or p.i > p.w - 3):
                return c + (0,)
        return c

    def head_d(c):
        if c.face == "north":
            c.sym(1, 5, BLACK)
            c.sym(1, 4, D), c.sym(2, 4, D), c.sym(0, 6, D)
            c.rect(3, 1, 6, 1, L), c.rect(4, 0, 4, 1, L)
        c.side_eye(2, 5, BLACK, None)

    m.box("body", "Body", (-8, 12, -12), (8, 30, 12), skin)
    m.box("tail", "Body", (-0.6, 13, 11.5), (0.6, 26, 12.7), lambda p: (60, 56, 56) if p.ly < 0.2 else G, rot=(-10, 0, 0), origin=(0, 26, 12))
    m.bone("Head", (0, 24, -12))
    m.box("head", "Head", (-6, 17, -21), (6, 31, -11.5), skin, head_d)
    m.box("brow", "Head", (-5, 30.5, -20), (5, 32.5, -13), skin)
    m.pair("ear", "Head", (-13, 16, -14.5), (-5.5, 30, -13.3), ear, rot=(0, 22, 0), origin=(-5.5, 23, -14))
    m.box("trunk-a", "Head", (-2.5, 12, -24.5), (2.5, 23, -20.5), trunk)
    m.box("trunk-b", "Head", (-2, 5, -25.5), (2, 13, -22), trunk)
    m.box("trunk-c", "Head", (-1.5, 1, -27.5), (1.5, 6, -24.5), trunk,
          lambda c: c.face == "down" and c.rect(1, 1, 1, 1, (90, 70, 74)))
    m.pair("tusk", "Head", (-5, 14.5, -27), (-3.4, 16.3, -20.5), lambda p: mix(IV, (214, 200, 160), p.lz), rot=(14, 0, 0), origin=(-4.2, 15.5, -20.5))
    four_legs(m, -8, -2.5, -11.5, 6, 5.5, 12, leg)
    return m


def bear():
    m = Model("bear", 108)
    B, D, TN, NB = (124, 84, 52), (92, 60, 38), (206, 170, 120), (28, 22, 22)

    def fur(p):
        c = B
        if (int(p.z + 100) + 2 * int(p.y + 100) + int(p.x + 100)) % 5 == 0:
            c = mix(B, D, 0.55)
        if p.face == "down":
            c = D
        return c

    def leg(p):
        if p.y < 1.2 and p.face == "north" and p.i % 2 == 0:
            return (40, 34, 34)
        return D if p.y < 5 else fur(p)

    def head_d(c):
        if c.face == "north":
            c.sym(2, 3, BLACK)
            c.sym(2, 2, D), c.sym(1, 2, D)
        c.side_eye(1, 3)

    def muzzle_d(c):
        if c.face == "north":
            c.rect(1, 0, 3, 2, NB)
            c.px(2, 2, NB)
            c.rect(1, 3, 3, 1, (120, 70, 60))
        elif c.face == "up":
            c.rect(1, 0, 3, 1, NB)

    m.box("body", "Body", (-6, 8, -10), (6, 20, 10), fur)
    m.box("hump", "Body", (-5, 19.5, -9.5), (5, 22, -2), fur)
    m.box("rump", "Body", (-5, 9, 9.5), (5, 19, 11.5), fur)
    m.box("tail", "Body", (-1.5, 14, 11), (1.5, 17, 13), D)
    m.bone("Head", (0, 17, -10))
    m.box("head", "Head", (-4.5, 13, -17), (4.5, 22, -9.5), fur, head_d)
    m.box("muzzle", "Head", (-2.5, 13.5, -20), (2.5, 17.5, -16.5), TN, muzzle_d)
    m.pair("ear", "Head", (-5.2, 21, -14.5), (-2.8, 23.5, -13), lambda p: TN if p.face == "north" and p.i == 1 and p.j == 1 else D)
    four_legs(m, -6.3, -2, -9.5, 5, 4.5, 8, leg)
    return m


def gator():
    m = Model("gator", 109)
    G, D, Y, TH = (86, 112, 62), (52, 72, 42), (214, 206, 150), (244, 242, 230)

    def hide(p):
        if p.face == "down":
            return Y if (p.i + p.j) % 2 else mul(Y, 0.92)
        if p.face == "up":
            cell = (int(math.floor(p.x / 2.0)) + int(math.floor(p.z / 2.0))) % 2
            edge = int(math.floor(p.x + 100)) % 2 == 0 and int(math.floor(p.z + 100)) % 2 == 0
            return D if edge else (G if cell else mul(G, 0.88))
        if p.ly < 0.3:
            return mix(Y, G, 0.3)
        return D if vnoise(109, p.x / 2.0, p.y / 2.0, p.z / 2.5) > 0.6 else G

    def snout_d(c):
        if c.face in ("east", "west"):
            for i in range(0, c.w, 2):
                c.px(i, c.h - 1, TH)
        elif c.face == "north":
            c.sym(1, c.h - 1, TH), c.sym(3, c.h - 1, TH)
            c.sym(2, 0, BLACK)
        elif c.face == "up":
            c.sym(2, 0, (30, 40, 26))

    def jaw_d(c):
        if c.face in ("east", "west"):
            for i in range(1, c.w, 2):
                c.px(i, 0, TH)

    def eye_d(c):
        if c.face in ("east", "west"):
            c.rect(0, 0, c.w, c.h, (226, 200, 60))
            c.px(1, 0, BLACK), c.px(1, 1, BLACK)
        elif c.face == "north":
            c.rect(0, 0, c.w, c.h, (226, 200, 60))
            c.px(0, 0, BLACK) if c.part.endswith("-r") else c.px(c.w - 1, 0, BLACK)

    def ridge(p):
        a = 255 if (int(math.floor(p.z + 100)) % 2 == 0) else 0
        return D + (a,)

    def tail(p):
        if p.face == "down":
            return Y
        if p.face == "up" or p.ly > 0.4:
            return D if int(math.floor(p.z + 100)) % 3 == 0 else G
        return mix(Y, G, 0.3)

    def leg(p):
        if p.y < 1 and p.face in ("north", "up") and p.i % 2 == 0:
            return (30, 34, 26)
        return D if (p.i + p.j) % 3 == 0 else G

    m.box("body", "Body", (-6, 2, -10), (6, 7.5, 10), hide)
    m.pair("ridge", "Body", (-3.5, 7.3, -9), (-2.5, 8.5, 10), ridge, skip=("north", "south", "down"))
    m.box("tail-a", "Body", (-4.5, 2.2, 9.5), (4.5, 7, 18), tail)
    m.box("tail-b", "Body", (-3, 2.4, 17.5), (3, 6, 26), tail)
    m.box("tail-c", "Body", (-1.5, 2.6, 25.5), (1.5, 5, 34), tail)
    m.box("tail-fin", "Body", (-0.5, 4.8, 10), (0.5, 7.9, 33), lambda p: D + ((255 if (int(math.floor(p.z + 100)) % 2 == 0 and p.ly < 0.4 + 0.6 * (1 - p.lz)) or p.ly < 0.15 else 0),),
          skip=("north", "south", "down", "up"))
    m.bone("Head", (0, 4.5, -10))
    m.box("head", "Head", (-5, 2.2, -17), (5, 7.2, -9.5), hide)
    m.box("snout", "Head", (-4, 3.6, -28), (4, 6.2, -16.5), hide, snout_d)
    m.box("jaw", "Head", (-3.5, 2.2, -27), (3.5, 3.7, -16.5), lambda p: Y if p.face == "down" else mix(Y, G, 0.45), jaw_d)
    m.pair("eye", "Head", (-4.6, 6.8, -15.5), (-2.4, 8.8, -12.5), G, eye_d)
    for tag, z in (("F", -8.5), ("B", 5.5)):
        m.bone("Leg-%sL" % tag, (-6, 3.5, z + 1.5))
        m.bone("Leg-%sR" % tag, (6, 3.5, z + 1.5))
        m.pair("leg-" + tag.lower(), ("Leg-%sL" % tag, "Leg-%sR" % tag), (-9, 0, z), (-5.5, 4, z + 3.2), leg)
    return m


def snake():
    m = Model("snake", 110)
    G, D, Y = (104, 150, 62), (48, 78, 40), (226, 214, 140)

    def scales(off):
        def sh(p):
            if p.face == "down":
                return Y if p.j % 2 else mul(Y, 0.9)
            if p.side() and p.ly < 0.3:
                return Y
            t = off + p.lz * 4.5
            k = int(math.floor(t + 100)) % 5
            if k == 0 or (k in (1, 4) and p.face == "up" and 0.3 < p.lx < 0.7):
                return D
            return mix(G, (150, 180, 70), 0.5) if k == 2 and p.face == "up" else G
        return sh

    z0 = -4.0
    pts = [(3.2 * math.sin(i * 1.15 + 0.4), z0 + i * 4.2) for i in range(8)]
    pts[0] = (0.0, z0)
    sizes = [3, 3, 3, 3, 2.6, 2, 1.4]
    for i in range(7):
        (xa, za), (xb, zb) = pts[i], pts[i + 1]
        cx, cz, s = (xa + xb) / 2, (za + zb) / 2, sizes[i]
        ln = math.hypot(xb - xa, zb - za) + s * 0.7
        ang = math.degrees(math.atan2(xb - xa, zb - za))
        top = s - 0.06 * (i % 2)
        m.box("seg-%d" % i, "Body", (cx - s / 2, 0.03 * (i % 2), cz - ln / 2), (cx + s / 2, top, cz + ln / 2),
              scales(i * 4.2), rot=(0, ang, 0), origin=(cx, s / 2, cz))
    m.box("neck", "Body", (-1.45, 0.4, z0 - 7.5), (1.45, 3.2, z0 + 1), scales(-6), rot=(48, 0, 0), origin=(0, 1.6, z0))

    def head(p):
        if p.face == "down" or (p.side() and p.ly < 0.34):
            return Y
        if p.face == "up" and abs(p.x) < 1 and 0.3 < p.lz < 0.8:
            return D
        return G

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 0, (240, 200, 40))
            c.px(c.fx(2), 1, BLACK)
        elif c.face == "north":
            c.sym(0, 0, (240, 200, 40))
            c.sym(1, 2, (40, 50, 30))
        elif c.face == "up":
            c.sym(0, 2, (240, 200, 40))

    m.bone("Head", (0, 6.5, z0 - 5))
    m.box("head", "Head", (-2, 5.2, z0 - 11.5), (2, 8.4, z0 - 4.6), head, head_d)
    RD = (214, 40, 50)

    def tongue(p):
        fork = p.lz < 0.5
        on = (abs(p.lx - 0.5) > 0.2) if fork else (abs(p.lx - 0.5) < 0.2)
        return RD + ((255 if on else 0),)

    m.box("tongue", "Head", (-1.5, 5.9, z0 - 15), (1.5, 6.2, z0 - 11), tongue, skip=("east", "west", "south", "north"), noise=0)
    return m


def fish():
    m = Model("fish", 111)
    S, BK, BL, FN = (206, 214, 220), (120, 140, 156), (246, 246, 240), (236, 150, 80)

    def scales(p):
        if p.face == "up":
            return BK
        if p.face == "down":
            return BL
        c = mix(BL, S, min(1, p.ly * 2.2)) if p.ly < 0.45 else mix(S, BK, (p.ly - 0.6) * 2.5)
        if (p.i + (p.j // 2) % 2 * 1) % 2 == 0 and p.j % 2 == 0:
            c = mul(c, 0.84)
        if abs(p.ly - 0.52) < 0.07:
            c = mix(c, (240, 130, 110), 0.45)
        return c

    def head(p):
        if p.face == "up":
            return BK
        if p.face == "down":
            return BL
        return mix(BL, BK, p.ly * 0.9)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(1), 1, WHITE), c.px(c.fx(2), 1, WHITE), c.px(c.fx(1), 2, WHITE)
            c.px(c.fx(2), 2, BLACK)
            for j in range(1, c.h - 1):
                c.px(c.fx(c.w - 1), j, (130, 110, 120))
            c.px(c.fx(0), c.h - 2, (120, 70, 70))
        elif c.face == "north":
            c.rect(0, c.h - 2, c.w, 1, (120, 70, 70))

    def tailfin(p):
        half = 0.22 + 1.2 * p.lz
        d = abs(p.ly - 0.5)
        notch = p.lz > 0.55 and d < (p.lz - 0.5) * 0.45
        a = 255 if d < half and not notch else 0
        c = FN if int(p.ly * p.h) % 2 else mul(FN, 0.8)
        return c + (a,)

    def dorsal(p):
        a = 255 if p.ly < 0.45 + p.lz * 1.2 else 0
        return (FN if p.i % 2 else mul(FN, 0.8)) + (a,)

    def belly(p):
        a = 255 if p.ly > 0.75 - p.lz * 1.0 else 0
        return (FN if p.i % 2 else mul(FN, 0.8)) + (a,)

    m.box("body", "Body", (-2, 2, -6), (2, 10, 5), scales)
    m.box("head", "Body", (-1.6, 3, -10), (1.6, 9, -5.5), head, head_d)
    m.box("peduncle", "Body", (-1.1, 4, 4.5), (1.1, 8, 8.5), scales)
    m.box("tail-fin", "Body", (-0.4, 1, 8), (0.4, 11, 13), tailfin, skip=("north", "south", "up", "down"))
    m.box("dorsal", "Body", (-0.4, 9.8, -4), (0.4, 12.5, 3.5), dorsal, skip=("north", "south", "up", "down"))
    m.box("belly-fin", "Body", (-0.4, 0, -1), (0.4, 2.2, 3), belly, skip=("north", "south", "up", "down"))
    m.pair("pectoral", "Body", (-3.4, 3.2, -4.5), (-2, 4, -1.5), lambda p: FN if p.j % 2 else mul(FN, 0.85), rot=(0, 0, 28), origin=(-2, 3.8, -3))
    return m


def squid():
    m = Model("squid", 112)
    B, D, L, SP = (64, 78, 132), (40, 48, 92), (150, 160, 200), (110, 128, 190)

    def mantle(p):
        if p.face == "down":
            return D
        c = mix(B, D, 1 - p.ly) if p.side() else B
        if vnoise(112, p.x / 1.6 + 9, p.y / 1.6, p.z / 1.6) > 0.7:
            c = SP
        if p.face == "south":
            c = mix(c, L, 0.25)
        return c

    def head(p):
        return mix(B, L, 0.25) if p.face != "down" else D

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 0, 3, 3, WHITE)
            c.symrect(1, 1, 2, 2, BLACK)
        elif c.face in ("east", "west"):
            c.rect(c.fx(1) - (1 if c.face == "east" else 0), 0, 2, 3, WHITE)
            c.rect(c.fx(1) - (1 if c.face == "east" else 0), 1, 2, 2, BLACK)

    def fin(p):
        outer = (1 - p.lx) if p.x < 0 else p.lx
        a = 255 if abs(p.ly - 0.5) < 0.5 - outer * 0.42 else 0
        return mix(B, L, 0.35) + (a,)

    def arm(p):
        if p.face == "north" or p.face == "south":
            return L if p.j % 2 else mix(B, L, 0.5)
        return mix(B, D, 1 - p.ly * 0.8)

    m.box("mantle", "Body", (-5, 10, -5), (5, 25, 5), mantle)
    m.box("cap", "Body", (-3.5, 24.5, -3.5), (3.5, 27, 3.5), mantle)
    m.pair("fin", "Body", (-9, 17, -0.5), (-4.8, 26, 0.5), fin, skip=("up", "down", "east", "west"))
    m.box("head", "Body", (-4, 6.5, -4), (4, 10.5, 4), head, head_d)
    spots = [(-3, -3, "FL"), (0, -3.3, "FR"), (3, -3, "FR"), (-3.3, 0, "BL"), (3.3, 0, "BR"), (-3, 3, "BL"), (0, 3.3, "BR"), (3, 3, "BR")]
    names = ["FL", "FR", "FR", "BL", "FL", "BL", "BR", "BR"]
    for k, (x, z, _) in enumerate(spots):
        b = "Leg-%d-%s" % (k + 1, names[k])
        m.bone(b, (x, 6.5, z))
        ln = 0.0 if k % 2 == 0 else 1.5
        m.box("tentacle-%d" % (k + 1), b, (x - 1, ln, z - 1), (x + 1, 7, z + 1), arm)
    return m


def worm():
    m = Model("worm", 113, noise=5)
    PK, DK, BAND = (226, 150, 140), (190, 110, 108), (240, 190, 170)
    ys = [0, 1.2, 2.4, 3, 2.4, 1.2, 0, 0]
    for i, y in enumerate(ys):
        z = -8 + i * 2.1
        s = 3.0 if i else 3.3

        def sh(p, i=i):
            c = BAND if i in (2,) else PK
            if p.face in ("up", "east", "west") and p.lz > 0.75 or (p.face in ("up", "down") and p.j == 0):
                c = DK
            if p.face == "down":
                c = mix(c, (250, 215, 200), 0.5)
            return c

        def dec(c, i=i):
            if i == 0 and c.face == "north":
                c.sym(0, 0, BLACK)
                c.px(1, 2, (150, 70, 80))

        ext = 0.04 * (i % 2)
        m.box("seg-%d" % i, "Body", (-s / 2 + ext, y, z), (s / 2 - ext, y + s, z + 2.6), sh, dec)
    m.box("tail-tip", "Body", (-1, 0, 8.8), (1, 2, 10.6), DK)
    return m


def ant():
    m = Model("ant", 114, noise=5)
    K, R, HL = (44, 30, 28), (120, 52, 34), (96, 70, 64)

    def shell(p):
        c = K
        if p.face == "up" and 0.2 < p.lx < 0.8 and 0.15 < p.lz < 0.55:
            c = HL
        return c

    def abdomen(p):
        c = R if p.face != "down" else mul(R, 0.7)
        if p.face in ("up", "east", "west") and int(p.lz * 7) % 2 == 1:
            c = mul(R, 0.72)
        if p.face == "up" and 0.25 < p.lx < 0.75 and p.lz < 0.3:
            c = mix(R, (200, 120, 90), 0.6)
        return c

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, (190, 190, 200))
            c.sym(0, 2, (90, 90, 110))
        c.side_eye(1, 1, (190, 190, 200))
        c.side_eye(1, 2, (90, 90, 110))

    m.box("thorax", "Body", (-1.5, 3, -4), (1.5, 6, 1.5), shell)
    m.box("waist", "Body", (-0.8, 3.6, 1), (0.8, 5.2, 2.5), K)
    m.box("abdomen", "Body", (-2.5, 2.5, 2), (2.5, 7.5, 9), abdomen)
    m.box("sting", "Body", (-1.2, 3.6, 8.5), (1.2, 6.2, 10), mul(R, 0.7))
    m.bone("Head", (0, 5, -4))
    m.box("head", "Head", (-2, 3, -8), (2, 7, -3.6), shell, head_d)
    m.pair("antenna", "Head", (-1.9, 6.6, -7.2), (-1.1, 10, -6.4), K, rot=(-32, 0, 14), origin=(-1.5, 6.8, -6.8))
    m.pair("mandible", "Head", (-1.8, 3.2, -9.6), (-0.6, 4.2, -7.6), R)
    for tag, z, yaw in (("FL", -3.4, -32), ("M-L", -1.4, 0), ("BL", 0.6, 32)):
        rt = tag[:-1] + "R"
        m.bone("Leg-" + tag, (-1.4, 4.3, z + 0.5))
        m.bone("Leg-" + rt, (1.4, 4.3, z + 0.5))
        org = (-1.4, 4.3, z + 0.5)
        m.pair("thigh-" + tag[:-1].lower().strip("-"), ("Leg-" + tag, "Leg-" + rt), (-5.8, 3.9, z), (-1.3, 4.8, z + 0.9), R, rot=(0, yaw, 0), origin=org)
        m.pair("shin-" + tag[:-1].lower().strip("-"), ("Leg-" + tag, "Leg-" + rt), (-6.2, 0, z + 0.02), (-5.3, 4.6, z + 0.88), HL, rot=(0, yaw, 0), origin=org)
    return m


def sheep():
    m = Model("sheep", 115, noise=9)
    W, SH, F, HF = (244, 242, 234), (206, 204, 196), (214, 190, 160), (70, 60, 54)

    def wool(p):
        n = vnoise(115, p.x / 1.7, p.y / 1.7, p.z / 1.7)
        c = SH if n > 0.6 else W
        if (int(p.x + 100) + int(p.y + 100) + int(p.z + 100)) % 5 == 0 and n > 0.4:
            c = mul(SH, 0.93)
        return c

    def leg(p):
        if p.y < 1.5:
            return HF
        return wool(p) if p.y > 6 else F

    def face(p):
        return F if p.face != "down" else mul(F, 0.9)

    def head_d(c):
        if c.face == "north":
            c.eyes(0, 3)
            c.rect(2, 5, 2, 1, (150, 110, 100))
            c.px(2, 6, (130, 96, 88)), c.px(3, 6, (130, 96, 88))
            c.rect(0, 0, c.w, 1, W)
        c.side_eye(1, 3)

    m.box("body", "Body", (-6, 9, -9), (6, 21, 9), wool)
    m.box("tail", "Body", (-1.5, 13, 8.5), (1.5, 18, 10.5), wool)
    m.bone("Head", (0, 17, -9))
    m.box("head", "Head", (-3, 13, -14.5), (3, 20, -8.5), face, head_d)
    m.box("cap", "Head", (-3.6, 18, -13.2), (3.6, 21.5, -8), wool)
    m.pair("ear", "Head", (-5.6, 16, -12), (-2.8, 17.6, -10.6), lambda p: (220, 160, 150) if p.face == "north" and 0 < p.i < p.w - 1 else F, rot=(0, 0, 14), origin=(-3, 17, -11))
    four_legs(m, -5.5, -2.5, -8, 5, 3, 9, leg)
    return m


def horse():
    m = Model("horse", 116)
    C, D, MN, HF, W = (226, 212, 196), (196, 180, 162), (86, 76, 70), (52, 46, 44), (250, 248, 242)

    def coat(p):
        c = C
        if p.face == "down":
            c = D
        elif vnoise(116, p.x / 5.0, p.y / 5.0, p.z / 5.0) > 0.62:
            c = D
        return c

    def leg(p):
        if p.y < 2:
            return HF
        if p.y < 5:
            return mix(C, MN, 0.6)
        return coat(p)

    def head(p):
        if p.face in ("north", "up") and abs(p.x) < 1:
            return W
        return coat(p)

    def head_d(c):
        c.side_eye(2, 2, BLACK, WHITE)
        if c.face == "north":
            c.sym(0, 2, BLACK)

    def muzzle(p):
        if p.face in ("north", "up") and abs(p.x) < 1 and p.face == "up":
            return W
        return (150, 132, 124)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(0, 1, (60, 44, 44))
            c.rect(0, c.h - 1, c.w, 1, (96, 76, 72))
        elif c.face in ("east", "west"):
            c.rect(c.fx(2) if c.face == "west" else c.fx(0) - 0, c.h - 2, 1, 1, (96, 76, 72))

    def hair(p):
        return MN if (p.i + p.j // 2) % 3 else mul(MN, 0.7)

    NP, NR = (0, 21, -9.5), (-28, 0, 0)
    m.box("body", "Body", (-5, 13, -11), (5, 23.5, 11), coat)
    m.box("tail", "Body", (-1.5, 9, 10.5), (1.5, 22.5, 12.8), hair, rot=(-14, 0, 0), origin=(0, 22.5, 11))
    m.bone("Head", NP)
    m.box("neck", "Head", (-3, 18, -15.5), (3, 32, -8.5), coat, rot=NR, origin=NP)
    m.box("mane", "Head", (-1, 21, -8.9), (1, 33.5, -7), hair, rot=NR, origin=NP)
    m.box("head", "Head", (-3, 26.5, -23.5), (3, 33.5, -14), head, head_d)
    m.box("muzzle", "Head", (-2.2, 26.5, -27), (2.2, 31, -22.5), muzzle, muzzle_d)
    m.box("forelock", "Head", (-1.5, 32.5, -18.5), (1.5, 34.3, -13.5), hair)
    m.pair("ear", "Head", (-2.8, 33, -16.2), (-1.4, 36, -15), lambda p: (200, 150, 140) if p.face == "north" and p.j > 0 else C)
    four_legs(m, -5, -2, -10.5, 7.5, 3, 13, leg)
    return m


def frog():
    m = Model("frog", 117)
    G, D, Y, EY = (96, 164, 66), (54, 108, 46), (230, 226, 160), (240, 200, 50)

    def skin(p):
        if p.face == "down" or (p.side() and p.ly < 0.3):
            return Y
        return D if vnoise(117, p.x / 1.8, p.y / 1.8, p.z / 1.8) > 0.66 else G

    def head_d(c):
        if c.face == "north":
            c.rect(0, 3, c.w, 1, (40, 70, 36))
            c.rect(0, 4, c.w, c.h - 4, Y)
            c.sym(3, 2, (40, 70, 36))
        elif c.face in ("east", "west"):
            c.rect(0 if c.face == "west" else 2, 3, c.w - 2, 1, (40, 70, 36))

    def eye_d(c):
        if c.face in ("north", "east", "west"):
            c.rect(0, 0, c.w, c.h, EY)
            c.rect(c.w // 2 - (0 if c.w % 2 else 1), 0, 1 if c.w % 2 else 2, c.h, BLACK)
            c.rect(0, c.h - 1, c.w, 1, D) if c.h > 2 else None

    def leg(p):
        if p.face == "down":
            return Y
        return D if (p.i + p.j) % 3 == 0 else G

    def toes(p):
        a = 0 if p.face in ("up", "down") and p.i % 2 == 1 and (p.j < 2 if p.face == "up" else p.j > p.h - 3) else 255
        return mix(G, Y, 0.35) + (a,)

    m.box("body", "Body", (-4, 1.5, -4), (4, 7, 6), skin, rot=(-12, 0, 0), origin=(0, 2, 6))
    m.bone("Head", (0, 6, -4))
    m.box("head", "Head", (-4.2, 3.5, -9), (4.2, 9.5, -3.5), skin, head_d)
    m.pair("eye", "Head", (-4.4, 8.8, -7.5), (-1.8, 11.5, -4.8), G, eye_d)
    m.bone("Leg-FL", (-3.8, 4.5, -3.5))
    m.bone("Leg-FR", (3.8, 4.5, -3.5))
    m.pair("arm", ("Leg-FL", "Leg-FR"), (-5.3, 0.6, -4.5), (-3.7, 5.5, -2.8), leg)
    m.pair("hand", ("Leg-FL", "Leg-FR"), (-6.2, 0, -7), (-3.4, 0.8, -3.5), toes)
    m.bone("Leg-BL", (-4, 4, 4))
    m.bone("Leg-BR", (4, 4, 4))
    m.pair("thigh", ("Leg-BL", "Leg-BR"), (-6.8, 0.8, 0.5), (-3.8, 5.2, 7), leg, rot=(0, -10, 0), origin=(-4, 3, 6))
    m.pair("foot", ("Leg-BL", "Leg-BR"), (-7.8, 0, -3), (-4.6, 0.9, 5), toes, rot=(0, -10, 0), origin=(-4, 0.5, 6))
    return m


def duck():
    m = Model("duck", 118)
    GR, WH, BR, GY, DG, OR, YB = (40, 110, 70), (244, 242, 236), (120, 76, 52), (196, 192, 186), (112, 108, 104), (236, 140, 40), (232, 200, 70)

    def body(p):
        if p.face == "down":
            return WH
        if p.lz < 0.3:
            return BR
        if p.lz > 0.85:
            return (40, 40, 44)
        if p.face == "up":
            return DG if abs(p.x) < 1.5 else GY
        return GY if p.ly > 0.25 else WH

    def wing(p):
        if abs(p.lz - 0.7) < 0.1 and 0.25 < p.ly < 0.7:
            return (50, 80, 190)
        return DG if (p.i + 2 * p.j) % 4 == 0 or p.j == p.h - 1 else mul(GY, 0.9)

    def neck(p):
        return WH if p.ly < 0.45 else GR

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK)
        if c.face in ("east", "west"):
            c.px(c.fx(2), 0, mul(GR, 1.5))

    m.box("body", "Body", (-4, 4.5, -6), (4, 11.5, 6), body)
    m.pair("wing", "Body", (-4.9, 6.5, -4.5), (-3.8, 11, 5), wing)
    m.box("tail", "Body", (-2.5, 9, 5.5), (2.5, 11, 9.5), lambda p: WH if p.lz > 0.6 else (40, 40, 44), rot=(-26, 0, 0), origin=(0, 10, 5.5))
    m.bone("Head", (0, 11, -5))
    m.box("neck", "Head", (-1.5, 10.5, -7.5), (1.5, 15.5, -4.5), neck)
    m.box("head", "Head", (-2.5, 14.5, -9.5), (2.5, 19.5, -4), GR, head_d)
    m.box("bill", "Head", (-2, 14.8, -13.5), (2, 16.4, -9), YB,
          lambda c: (c.face == "up" and [c.sym(1, 3, (120, 90, 30))]) or (c.face == "north" and c.rect(0, 1, c.w, 1, (190, 150, 40))))
    for tag, sx in (("L", -1), ("R", 1)):
        x = 2.2 * sx
        m.bone(tag + "-Leg", (x, 4.5, 1.5))
        m.box("leg-" + tag.lower(), tag + "-Leg", (x - 0.6, 0.5, 1), (x + 0.6, 5.5, 2.2), OR)
        m.box("foot-" + tag.lower(), tag + "-Leg", (x - 1.6, 0, -2.2), (x + 1.6, 0.8, 2.4),
              lambda p: OR + ((0,) if p.face in ("up", "down") and (p.i in (0, p.w - 1)) and (p.j > 2 if p.face == "up" else p.j < p.h - 3) else (255,)))
    return m


# ---------------------------------------------------------------- more animals
#
# These hang tails, ears, wings, necks and jaws on bones of their own (`tail`
# or `tail-a`/`tail-b`/..., `ear-l`, `wing-l`, `neck`, `jaw`) with the pivot at
# the joint, and number the legs of many-legged ones `Leg-L1`, `Leg-R1`, ...

def fur(seed, base, dark, belly=None, belly_y=-1e9, scale=3.0, thr=0.62, amt=0.5):
    """A mottled coat, optionally paler underneath and below belly_y."""
    def sh(p):
        if belly is not None and (p.face == "down" or p.y < belly_y):
            return belly
        if vnoise(seed, p.x / scale, p.y / scale, p.z / scale) > thr:
            return mix(base, dark, amt)
        return base
    return sh


def rotp(pt, rot, org):
    """Where a point ends up after a box rotation about org (for child pivots)."""
    r = rot_matrix(rot)
    q = [pt[k] - org[k] for k in range(3)]
    return tuple(round(sum(r[i][k] * q[k] for k in range(3)) + org[i], 2) for i in range(3))


def pointed(outer, inner=None, tip=None):
    """Shader for an upright ear whose outer top corner is cut away."""
    def sh(p):
        out = (p.lx < 0.5) == (p.x < 0)
        if p.face in ("north", "south") and p.j == 0 and out:
            return outer + (0,)
        if p.face == "up" and out:
            return outer + (0,)
        if p.face == ("west" if p.x < 0 else "east") and p.j == 0:
            return outer + (0,)
        if tip is not None and (p.face == "up" or p.j == 0):
            return tip
        if inner is not None and p.face == "north" and p.j > 0 and (not out or p.w > 2 and 0 < p.i < p.w - 1):
            return inner
        return outer
    return sh


def ears(m, lo, hi, shader, decal=None, rot=None, pivot=None, parent="Head"):
    """A pair of ears on the bones ear-l / ear-r (pivot: middle of the base unless given)."""
    piv = pivot or ((lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2)
    m.bone("ear-l", piv, parent)
    m.bone("ear-r", (-piv[0], piv[1], piv[2]), parent)
    m.pair("ear", ("ear-l", "ear-r"), lo, hi, shader, decal, rot, piv if rot else None)


def wings(m, pivot, lo, hi, shader, rot=None, skip=(), name="wing", parent="Body"):
    """A pair of wing boxes on the bones wing-l / wing-r, which pivot at the shoulder."""
    m.bone("wing-l", pivot, parent)
    m.bone("wing-r", (-pivot[0], pivot[1], pivot[2]), parent)
    m.pair(name, ("wing-l", "wing-r"), lo, hi, shader, None, rot, pivot if rot else None, skip)


def side_legs(m, spec, x_in, x_out, top, th, thigh, shin, knee=0.0):
    """Insect legs: a thigh out to x_out and a shin down to the ground, per (z, yaw) on each side."""
    for k, (z, yaw) in enumerate(spec, 1):
        bl, br = "Leg-L%d" % k, "Leg-R%d" % k
        piv = (x_in, top - th / 2, z + th / 2)
        m.bone(bl, piv)
        m.bone(br, (-x_in, piv[1], piv[2]))
        m.pair("thigh-%d" % k, (bl, br), (x_out, top - th, z), (x_in + 0.3, top, z + th), thigh, rot=(0, yaw, 0), origin=piv)
        m.pair("shin-%d" % k, (bl, br), (x_out - 0.5, 0, z + 0.02), (x_out + 0.45, top + knee - 0.12, z + th - 0.02), shin,
               rot=(0, yaw, 0), origin=piv)


def nose_d(nb, mouth=(96, 56, 56)):
    """Decal for a snout: nose pad on the front top edge, mouth line along the bottom."""
    def d(c):
        x, w = (1, c.w - 2) if c.w > 2 else (0, c.w)
        if c.face == "north":
            c.rect(x, 0, w, 1, nb)
            c.rect(0, c.h - 1, c.w, 1, mouth)
        elif c.face == "up":
            c.rect(x, 0, w, 1, nb)
        elif c.face in ("east", "west"):
            for k in range(max(1, c.w - 1)):
                c.px(c.fx(k), c.h - 1, mouth)
    return d


def wolf():
    m = Model("wolf", 301)
    G, D, W, NB, EY = (198, 198, 204), (134, 136, 146), (242, 240, 236), (34, 30, 32), (236, 180, 50)

    def coat(p):
        if p.face == "down" or p.y < 11.3:
            return W
        if p.face == "up" or p.y > 15.2 + 1.2 * vnoise(301, p.x / 3.0, 0, p.z / 3.0):
            return D if (int(p.z + 100) + int(p.x + 100)) % 3 else mix(D, G, 0.5)
        return mix(G, D, 0.45) if vnoise(301, p.x / 2.0, p.y / 2.0, p.z / 2.0) > 0.62 else G

    def chest(p):
        if p.face == "north" and abs(p.x) < 2.5 and p.y < 15.5:
            return W
        return coat(p)

    def leg(p):
        if p.y < 2:
            return W
        return mix(G, D, 0.5) if p.y > 7 and (p.i + p.j) % 3 == 0 else G

    def head(p):
        if p.face == "down":
            return W
        if p.face == "up":
            return D if abs(p.x) < 2 else G
        if p.face in ("east", "west") and p.y < 16.4 and p.lz < 0.6:
            return W
        return G

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 3, 2, 3, W)
            c.rect(2, 4, 2, 2, W)
            c.rect(2, 0, 2, 2, D)
            c.sym(1, 2, EY)
            c.sym(1, 1, D), c.sym(0, 1, D)
        c.side_eye(1, 2, EY)

    def tail(p):
        if p.lz > 0.82:
            return (60, 58, 66)
        return D if p.face == "up" else W if p.face == "down" else G

    m.box("body", "Body", (-3, 10, -3), (3, 16.5, 9), coat)
    m.box("chest", "Body", (-3.5, 9.5, -9), (3.5, 17.5, -2.5), chest)
    m.bone("tail", (0, 15.2, 9))
    m.box("tail", "tail", (-1.2, 13.6, 8.5), (1.2, 16, 16.5), tail, rot=(32, 0, 0), origin=(0, 15.2, 9))
    m.box("tail-brush", "tail", (-1.9, 13, 10.5), (1.9, 16.4, 15), tail, rot=(32, 0, 0), origin=(0, 15.2, 9))
    m.bone("Head", (0, 16, -9))
    m.box("head", "Head", (-3, 14, -14.5), (3, 20, -8.5), head, head_d)
    m.box("snout", "Head", (-1.5, 14, -19.5), (1.5, 16.8, -14), lambda p: G if p.face == "up" else W, nose_d(NB))
    m.pair("ruff", "Head", (-4, 13.6, -11.5), (-2.8, 17, -8.8), lambda p: W if p.ly < 0.6 else G)
    ears(m, (-3, 19.5, -11.5), (-1.2, 23, -10.3), pointed(G, (226, 170, 168), D))
    four_legs(m, -3.3, -1.3, -8, 6.5, 2, 10, leg)
    return m


def fox():
    m = Model("fox", 302)
    O, DO, W, K, NB = (228, 122, 52), (188, 90, 40), (246, 242, 234), (44, 34, 34), (30, 26, 26)
    coat = fur(302, O, DO, W, 7.3, 2.5)

    def chest(p):
        if p.face == "north" and abs(p.x) < 2 and p.y < 10.5:
            return W
        return coat(p)

    def head(p):
        if p.face == "down" or (p.face in ("east", "west") and p.y < 11.4 and p.lz < 0.7):
            return W
        return O

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 3, 2, 3, W)
            c.rect(2, 5, 2, 1, W)
            c.sym(1, 2, BLACK)
            c.sym(0, 2, DO)
        c.side_eye(1, 2)

    def tail_a(p):
        return W if p.face == "down" else DO if p.face == "up" and abs(p.x) < 0.8 else O

    def tail_b(p):
        if p.lz > 0.6:
            return W
        return W if p.face == "down" else DO if p.lz > 0.48 else O

    m.box("body", "Body", (-2.5, 6, -3), (2.5, 11, 7), coat)
    m.box("chest", "Body", (-3, 5.5, -7.5), (3, 11.8, -2.5), chest)
    TP, TR = (0, 9.6, 7), (14, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-1.4, 8.2, 6.5), (1.4, 11, 12.5), tail_a, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 9.6, 12.2), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-2.1, 7.4, 11.8), (2.1, 11.6, 18.5), tail_b, rot=TR, origin=TP)
    m.bone("Head", (0, 11, -7.5))
    m.box("head", "Head", (-3, 9, -12.5), (3, 14.5, -7), head, head_d)
    m.box("snout", "Head", (-1.2, 9, -16.5), (1.2, 11.3, -12), lambda p: O if p.face == "up" else W, nose_d(NB))
    ears(m, (-3.2, 14, -10.5), (-1, 18.5, -9.3), pointed(O, W, K))
    four_legs(m, -2.8, -1.2, -7, 5, 1.8, 6, lambda p: K if p.y < 4.6 else O)
    return m


def rabbit():
    m = Model("rabbit", 303)
    B, D, W, PK = (182, 154, 124), (142, 116, 92), (246, 242, 234), (236, 168, 166)
    coat = fur(303, B, D, W, 3.2, 2.2)

    def head_d(c):
        if c.face == "north":
            c.rect(1, 3, 3, 2, W)
            c.px(2, 2, PK)
            c.px(2, 3, (150, 100, 100))
            c.px(2, 4, WHITE)
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK, None)

    def ear(p):
        if p.face == "north" and 0 < p.j < p.h - 1:
            return PK
        return D if p.j == 0 else B

    m.box("body", "Body", (-3, 2, -4), (3, 8, 2), coat)
    m.box("rump", "Body", (-3.5, 1.6, 0), (3.5, 9, 4.8), coat)
    m.bone("tail", (0, 5.5, 4.8))
    m.box("tail", "tail", (-1.5, 4, 4.3), (1.5, 7, 6.3), W)
    m.bone("Head", (0, 7.5, -4))
    m.box("head", "Head", (-2.5, 6, -8.5), (2.5, 11, -3.5), fur(303, B, D, W, 6.8, 2.2), head_d)
    ears(m, (-2.3, 10.5, -5.8), (-0.7, 17.5, -4.8), ear, rot=(10, 0, 8))
    m.bone("Leg-FL", (-1.7, 3, -3.2))
    m.bone("Leg-FR", (1.7, 3, -3.2))
    m.pair("leg-f", ("Leg-FL", "Leg-FR"), (-2.4, 0, -4), (-1, 3.5, -2.5), lambda p: W if p.y < 1.2 else B)
    m.bone("Leg-BL", (-3.3, 4.5, 2.5))
    m.bone("Leg-BR", (3.3, 4.5, 2.5))
    m.pair("haunch", ("Leg-BL", "Leg-BR"), (-4.4, 1, 0.2), (-2.6, 6.6, 4.6), coat)
    m.pair("foot", ("Leg-BL", "Leg-BR"), (-4.4, 0, -2.6), (-2.6, 1.3, 3.6), lambda p: W if p.lz < 0.35 or p.face == "down" else B)
    return m


def deer():
    m = Model("deer", 304)
    T, D, W, HF, AN, NB = (190, 134, 86), (140, 94, 60), (244, 238, 226), (52, 42, 38), (218, 200, 164), (36, 30, 30)

    def coat(p):
        if p.face == "down" or p.y < 15.5:
            return mix(W, T, 0.25)
        if p.face == "south" and abs(p.x) < 3 and p.y < 20.5:
            return W
        if p.face == "up" and abs(p.x) < 1:
            return D
        return mix(T, D, 0.5) if vnoise(304, p.x / 3.0, p.y / 3.0, p.z / 3.0) > 0.64 else T

    def neck(p):
        return W if p.face == "north" and p.ly < 0.45 else mix(T, D, 0.5) if p.face == "south" else T

    def leg(p):
        if p.y < 1.5:
            return HF
        if p.y < 6:
            return mix(T, D, 0.6)
        return T

    def head(p):
        if p.face == "down":
            return W
        return D if p.face == "up" and abs(p.x) < 1 else T

    def head_d(c):
        if c.face == "north":
            c.sym(0, 2, BLACK)
            c.sym(0, 1, W)
        c.side_eye(2, 2, BLACK, WHITE)

    def muzzle_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, 1, NB)
            c.rect(0, c.h - 1, c.w, 1, W)
        elif c.face == "up":
            c.rect(0, 0, c.w, 1, NB)
        elif c.face in ("east", "west"):
            c.rect(0, c.h - 1, c.w, 1, W)

    def antler(p):
        return mix(AN, (150, 120, 90), 0.6) if p.y < 32 else AN

    NP, NR = (0, 20, -8.5), (-22, 0, 0)
    m.box("body", "Body", (-4, 14, -9), (4, 22, 9), coat)
    m.bone("tail", (0, 21, 9))
    m.box("tail", "tail", (-1.5, 17.5, 8.6), (1.5, 21.5, 10.2), lambda p: T if p.face == "up" or (p.face == "south" and p.j == 0) else W)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2, 17.5, -12), (2, 29, -7.5), neck, rot=NR, origin=NP)
    m.bone("Head", (0, 27.5, -13), "neck")
    m.box("head", "Head", (-2.5, 25.5, -18), (2.5, 31, -11.5), head, head_d)
    m.box("muzzle", "Head", (-1.6, 25.5, -21.5), (1.6, 28.5, -17.5), lambda p: T if p.face == "up" else mix(T, W, 0.35), muzzle_d)
    ears(m, (-5.4, 28.8, -13.4), (-2.3, 30.6, -12.4), lambda p: W if p.face == "north" and 0 < p.i < p.w - 1 else T,
         rot=(0, 0, -22), pivot=(-2.5, 29.7, -12.9))
    AO, AR = (-2.3, 30.5, -13.3), (14, 0, 10)
    for name, lo, hi in (("beam", (-2.8, 30.5, -13.8), (-1.8, 38, -12.8)),
                         ("brow", (-2.75, 32.5, -16.6), (-1.85, 33.4, -13.6)),
                         ("brow-tip", (-2.7, 33, -16.55), (-1.9, 35.2, -15.7)),
                         ("fork", (-5.6, 35.4, -13.75), (-2.7, 36.3, -12.85)),
                         ("fork-tip", (-5.55, 36, -13.7), (-4.7, 39.6, -12.9))):
        m.pair("antler-" + name, "Head", lo, hi, antler, rot=AR, origin=AO)
    four_legs(m, -4, -2, -8.5, 6.5, 2, 14, leg)
    return m


def goat():
    m = Model("goat", 305)
    W, G, HN, HF, PK = (240, 234, 222), (198, 188, 172), (128, 110, 92), (64, 54, 48), (226, 170, 160)

    def coat(p):
        c = W
        if p.side() and (int(p.x + p.z + 100) * 2 + p.j // 2) % 5 == 0:
            c = G
        if p.face == "up" and abs(p.x) < 1:
            c = G
        if p.side() and p.j == p.h - 1 and (p.i % 2):
            c = mul(G, 0.9)
        return c

    def leg(p):
        return HF if p.y < 1.5 else G if p.y < 4 and p.i % 2 else W

    def head_d(c):
        if c.face == "north":
            c.sym(0, 2, (232, 196, 80))
            c.rect(2, 0, 1, 3, G)
        c.side_eye(2, 2, BLACK, (232, 196, 80))

    def muzzle_d(c):
        if c.face == "north":
            c.rect(1, 0, c.w - 2, 1, PK)
            c.rect(0, c.h - 1, c.w, 1, (150, 120, 110))

    def beard(p):
        thin = p.ly < 0.4 and p.face in ("north", "south") and p.i != p.w // 2 and p.w > 1
        return (G if p.j % 2 else W) + ((0,) if thin else (255,))

    def horn(p):
        return HN if (int(p.y * 1.2) + int(p.z * 1.2 + 50)) % 2 else mul(HN, 1.25)

    NP, NR = (0, 16, -7.5), (-18, 0, 0)
    m.box("body", "Body", (-4, 9, -8), (4, 17.5, 8), coat)
    m.bone("tail", (0, 17, 8))
    m.box("tail", "tail", (-1, 16, 7.5), (1, 19.5, 9), coat, rot=(-22, 0, 0), origin=(0, 17, 8))
    m.bone("neck", NP)
    m.box("neck", "neck", (-2, 13.5, -11), (2, 21, -7), coat, rot=NR, origin=NP)
    m.bone("Head", (0, 20, -11), "neck")
    m.box("head", "Head", (-2.5, 18.5, -16), (2.5, 24, -10), lambda p: W, head_d)
    m.box("muzzle", "Head", (-1.8, 18.5, -18.5), (1.8, 21.3, -15.5), lambda p: mix(W, G, 0.4), muzzle_d)
    m.box("beard", "Head", (-1, 14.8, -17.2), (1, 18.7, -15.8), beard, skip=("up",))
    HO, HR = (-1.8, 23.5, -11.9), (32, 0, 8)
    m.pair("horn-a", "Head", (-2.4, 23.5, -12.5), (-1.2, 28.6, -11.3), horn, rot=HR, origin=HO)
    m.pair("horn-b", "Head", (-2.35, 27.4, -11.4), (-1.25, 28.55, -8.6), horn, rot=HR, origin=HO)
    ears(m, (-5.3, 20.6, -12.5), (-2.4, 22, -11.3), lambda p: PK if p.face == "north" and 0 < p.i < p.w - 1 else G,
         rot=(0, 0, 24), pivot=(-2.5, 21.3, -11.9))
    four_legs(m, -4, -2, -7.5, 5.5, 2, 9, leg)
    return m


def bird():
    m = Model("bird", 306, noise=5)
    BR, RD, W, DK, BK, LG = (126, 100, 76), (228, 112, 52), (242, 238, 228), (84, 66, 52), (66, 54, 46), (160, 116, 92)

    def body(p):
        if p.face == "down":
            return W
        if p.face == "up" or p.face == "south":
            return BR if (p.i + p.j) % 3 else DK
        if p.face == "north":
            return RD
        if p.ly < 0.3:
            return W
        return RD if p.lz < 0.45 and p.ly < 0.8 else BR

    def head(p):
        if p.face == "north":
            return RD if p.ly < 0.6 else BR
        if p.face in ("east", "west") and p.ly < 0.5 and p.lz < 0.55:
            return RD
        return W if p.face == "down" else BR

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1)

    def wing(p):
        return DK if (p.i + 2 * p.j) % 4 == 0 or p.j == p.h - 1 else BR

    m.box("body", "Body", (-2, 2.6, -3), (2, 6.8, 3), body)
    m.bone("tail", (0, 6, 3))
    m.box("tail", "tail", (-1.2, 5.4, 2.6), (1.2, 6.2, 7), lambda p: DK if p.i % 2 == 0 else BR, rot=(-12, 0, 0), origin=(0, 6, 3))
    wings(m, (-2, 6.2, -1.5), (-2.7, 3.4, -2.5), (-1.9, 6.4, 4), wing)
    m.bone("Head", (0, 6.5, -2.5))
    m.box("head", "Head", (-1.8, 6, -5.5), (1.8, 9.5, -2), head, head_d)
    m.box("beak", "Head", (-0.5, 6.9, -7), (0.5, 7.7, -5.3), BK)
    for tag, sx in (("L", -1), ("R", 1)):
        x = 1.0 * sx
        m.bone("Leg-" + tag, (x, 2.8, 0.3))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (x - 0.3, 0, 0), (x + 0.3, 3, 0.6), LG)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (x - 0.7, 0, -1.2), (x + 0.7, 0.4, 0.9), LG)
    return m


def bat():
    m = Model("bat", 307, noise=5)
    F, D, M, PK = (96, 78, 72), (58, 48, 48), (74, 60, 68), (196, 134, 134)

    def body(p):
        return mix(F, (150, 128, 112), 0.5) if p.face == "down" else D if vnoise(307, p.x, p.y, p.z / 2.0) > 0.62 else F

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, WHITE)
            c.sym(1, 1, BLACK)
            c.rect(1, 2, 2, 1, PK)
            c.sym(1, 3, WHITE)
        c.side_eye(1, 1)

    def wing(p):
        u = abs(p.x)
        if p.face in ("up", "down"):
            t = ((u - 1.4) % 3.0) / 3.0
            if p.lz > 1 - 0.34 * math.sin(math.pi * t) - 0.02:
                return M + (0,)
        if p.lz < 0.16 or p.face == "north":
            return F
        if int(u - 1.4) % 3 == 0:
            return D
        return M if p.face == "up" else mul(M, 1.15)

    m.box("body", "Body", (-1.5, 0.8, -2), (1.5, 3.8, 3.5), body)
    WP, WR = (-1.5, 3, 0.5), (0, 0, -10)
    wings(m, WP, (-7.5, 2.6, -2.5), (-1.4, 3.2, 4), wing, rot=WR, skip=("south",), name="wing-in")
    wings(m, WP, (-13.4, 2.62, -2), (-7.4, 3.18, 3.5), wing, rot=WR, skip=("south", "east", "west"), name="wing-out")
    m.bone("Head", (0, 2.5, -2))
    m.box("head", "Head", (-2, 0.9, -5), (2, 4.6, -1.7), body, head_d)
    ears(m, (-2, 4.3, -4), (-0.4, 7.3, -3.2), pointed(F, PK))
    for tag, sx in (("L", -1), ("R", 1)):
        m.bone("Leg-" + tag, (0.85 * sx, 1.2, 3))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (0.85 * sx - 0.35, 0, 2.6), (0.85 * sx + 0.35, 1.3, 4.6), D)
    return m


def butterfly():
    m = Model("butterfly", 308, noise=4)
    O, K, Y, BD = (242, 142, 40), (30, 24, 26), (250, 214, 110), (60, 46, 40)

    def in_fore(u, v):
        if u < 0 or u > 0.97 or v < 0 or (u > 0.84 and v < 0.2):
            return False
        return v < (0.56 + 0.44 * u / 0.72 if u < 0.72 else 1.0 - (u - 0.72) * 2.6)

    def in_hind(u, v):
        return u >= 0 and v >= 0 and (((u - 0.45) / 0.56) ** 2 + ((v - 0.42) / 0.6) ** 2 < 1 or (u < 0.35 and v < 0.62))

    def wing(inside, x0, span, fore):
        def sh(p):
            u, v = (abs(p.x) - x0) / span, p.lz
            if not inside(u, v):
                return O + (0,)
            eu, ev = 1.05 / p.w, 1.05 / p.h
            edge = not inside(u + eu, v) or not inside(u, v + ev) or (fore and v < ev)
            ii, jj = int(u * p.w), int(v * p.h)
            if edge:
                return WHITE if (ii + jj) % 2 == 0 and u > 0.3 else K
            if (jj % 2 == 1 and ii % 3 == 2) or (not fore and jj == 0):
                return K
            return mix(O, Y, 0.7 * (1 - u)) if u < 0.5 else O
        return sh

    WP, WR, SK = (-0.5, 1.2, 0), (0, 0, -38), ("north", "south", "east", "west")
    m.box("body", "Body", (-0.6, 0, -3.5), (0.6, 1.3, 3.5), lambda p: K if int(p.z + 100) % 2 else BD)
    wings(m, WP, (-9, 1.0, -6), (-0.5, 1.4, 0.5), wing(in_fore, 0.5, 8.5, True), rot=WR, skip=SK, name="wing-fore")
    wings(m, WP, (-7.5, 1.02, 0), (-0.5, 1.38, 5.5), wing(in_hind, 0.5, 7.0, False), rot=WR, skip=SK, name="wing-hind")
    m.bone("Head", (0, 0.8, -3.5))
    m.box("head", "Head", (-0.8, 0.1, -4.9), (0.8, 1.5, -3.4), K, lambda c: c.face == "north" and c.sym(0, 0, (150, 150, 170)))
    m.pair("antenna", "Head", (-0.9, 1.3, -7), (-0.4, 1.7, -4.6), lambda p: Y if p.lz < 0.2 else K, rot=(28, 22, 0), origin=(-0.65, 1.5, -4.6))
    return m


def bee():
    m = Model("bee", 309, noise=6)
    Y, K, WG, VN = (246, 198, 48), (42, 34, 30), (228, 238, 246), (176, 196, 216)

    def thorax(p):
        if p.face == "up" and abs(p.lx - 0.5) < 0.28 and abs(p.lz - 0.5) < 0.3:
            return K
        return mix(Y, (200, 150, 30), 0.6) if hsh(309, p.i, p.j, len(p.face)) > 0.6 else Y

    def abdomen(p):
        if p.face == "south":
            return K
        c = K if int(math.floor((p.z + 0.2) / 1.3)) % 2 == 1 else Y
        return mul(c, 0.82) if p.face == "down" else c

    def head_d(c):
        if c.face == "north":
            c.rect(1, 1, 2, 3, Y)
            c.symrect(0, 0, 1, 3, (70, 74, 110))
            c.sym(0, 0, (170, 176, 210))
            c.rect(1, 3, 2, 1, (120, 80, 30))
        elif c.face in ("east", "west"):
            c.rect(c.fx(1) - (1 if c.face == "east" else 0), 0, 2, 3, (70, 74, 110))
            c.px(c.fx(0), 0, (170, 176, 210))

    def wing(p):
        u, v = (abs(p.x) - 0.6) / 5.4, p.lz
        if ((u - 0.5) / 0.52) ** 2 + ((v - 0.5) / 0.56) ** 2 >= 1:
            return WG + (0,)
        return VN if int(u * p.w) % 2 == 1 and int(v * p.h) % 2 == 0 else WG

    m.box("thorax", "Body", (-2, 2.2, -3), (2, 6.2, 0.5), thorax)
    m.box("abdomen", "Body", (-2.5, 1.5, 0), (2.5, 6.5, 6.5), abdomen)
    m.box("sting", "Body", (-0.5, 3.4, 6.3), (0.5, 4.4, 8), K)
    wings(m, (-1, 6.3, -0.5), (-6, 6.1, -1.5), (-0.6, 6.5, 4), wing, rot=(0, 24, -20), skip=("north", "south", "east", "west"))
    m.bone("Head", (0, 4, -3))
    m.box("head", "Head", (-1.8, 2, -5.6), (1.8, 5.8, -2.8), K, head_d)
    m.pair("antenna", "Head", (-1.5, 5.4, -7.4), (-0.9, 6, -5.2), K, rot=(30, 16, 0), origin=(-1.2, 5.7, -5.2))
    side_legs(m, ((-2.6, -26), (-1.2, 0), (0.2, 26)), -1.6, -3.6, 3.2, 0.7, K, mix(K, Y, 0.3))
    return m


def beetle():
    m = Model("beetle", 310, noise=5)
    SH, HL, K, LG, DK = (38, 118, 96), (120, 200, 160), (30, 28, 36), (74, 54, 44), (24, 70, 62)

    def shell(p):
        if p.face == "down":
            return LG
        if p.face in ("up", "south") and abs(p.x) < 0.5:
            return K
        if p.face == "up" and int(p.z + 100) % 2 == 0 and int(abs(p.x) + 0.5) % 2 == 1:
            return DK
        g = vnoise(310, p.x / 2.5 + 4, p.y / 2.0, p.z / 4.0)
        return HL if g > 0.68 else mix(SH, DK, 0.5) if g < 0.3 else SH

    def pro(p):
        if p.face == "down":
            return LG
        return mix(DK, HL, 0.5) if p.face == "up" and abs(abs(p.x) - 1.8) < 0.6 and 0.25 < p.lz < 0.75 else DK

    def head_d(c):
        if c.face == "north":
            c.sym(0, 0, (200, 180, 90))
            c.rect(1, c.h - 1, 2, 1, (110, 70, 50))
        c.side_eye(1, 0, (200, 180, 90))

    m.box("shell", "Body", (-4, 1.8, -2.5), (4, 6.2, 6.5), shell)
    m.box("dome", "Body", (-3, 5.8, -1.5), (3, 7.6, 5.5), shell)
    m.box("pronotum", "Body", (-3.3, 1.8, -5.3), (3.3, 5.6, -2.2), pro)
    m.bone("Head", (0, 3.2, -5.3))
    m.box("head", "Head", (-2, 1.6, -7.5), (2, 4.4, -5), K, head_d)
    HO, HR = (0, 3.9, -7), (38, 0, 0)
    m.box("horn", "Head", (-0.5, 3.4, -10.2), (0.5, 4.4, -7), lambda p: mix(K, SH, 0.4), rot=HR, origin=HO)
    m.box("horn-tip", "Head", (-0.45, 4.2, -10.15), (0.45, 6, -9.2), lambda p: mix(K, SH, 0.4), rot=HR, origin=HO)
    m.pair("antenna", "Head", (-2.7, 3.5, -8.8), (-2.1, 4.1, -6.8), LG, rot=(0, 30, 0), origin=(-2.4, 3.8, -6.8))
    side_legs(m, ((-4.6, -32), (-1.4, 0), (2.4, 32)), -3, -6.4, 3.3, 0.9, K, LG)
    return m


def spider():
    m = Model("spider", 311, noise=6)
    K, D, R, H, EY = (56, 48, 50), (34, 28, 34), (206, 52, 42), (104, 88, 82), (220, 50, 46)

    def hairy(p):
        if p.face == "down":
            return D
        return H if hsh(311, p.i, p.j, len(p.face) + int(p.y)) > 0.78 else K

    def abdomen(p):
        if p.face == "up":
            ax = abs(p.x)
            if ax < 0.6 or (ax < 2.6 and int(p.lz * 8) in (2, 5)) or (ax < 1.6 and int(p.lz * 8) in (1, 3, 4, 6)):
                return R
        return hairy(p)

    def eyes_d(c):
        if c.face == "north":
            c.sym(0, 0, mul(EY, 0.7))
            c.sym(1, 0, (250, 120, 90))
            c.sym(1, 1, EY)
        c.side_eye(0, 0, EY)

    def shin(p):
        return H if int(p.y) % 3 == 0 else K

    m.box("thorax", "Body", (-2.5, 3, -5), (2.5, 6.2, 0.5), hairy)
    m.box("abdomen", "Body", (-4, 2.8, 0), (4, 8.8, 8.5), abdomen)
    m.box("spinneret", "Body", (-1, 3.8, 8.2), (1, 5.8, 9.6), D)
    m.bone("Head", (0, 4.5, -5))
    m.box("face", "Head", (-2, 3.4, -6.4), (2, 6, -4.8), K, eyes_d)
    m.pair("fang", "Head", (-1.5, 1.6, -6.9), (-0.5, 3.6, -5.9), lambda p: (230, 224, 210) if p.ly < 0.3 else D)
    side_legs(m, ((-4.4, -44), (-3.1, -15), (-1.8, 15), (-0.5, 44)), -2.2, -7.6, 5.8, 0.9, K, shin, knee=2.2)
    return m


def crab():
    m = Model("crab", 312, noise=6)
    R, D, Y, TP, K0 = (216, 86, 52), (160, 52, 38), (240, 214, 172), (250, 236, 214), (60, 30, 30)

    def shell(p):
        if p.face == "down":
            return Y
        if p.side() and p.ly < 0.3:
            return mix(R, Y, 0.6)
        if p.face == "north" and p.ly < 0.62 and abs(p.x) < 2.2:
            return mix(D, K0, 0.3) if p.i % 2 else D
        return D if vnoise(312, p.x / 1.6, p.y / 1.6, p.z / 1.6) > 0.66 else R

    def stalk_d(c):
        if c.face in ("north", "east", "west"):
            c.rect(0, 0, c.w, 1, BLACK)
        elif c.face == "up":
            c.rect(0, 0, c.w, c.h, (40, 30, 34))

    def claw(p):
        if p.lz < 0.3:
            return TP
        return D if (p.i + p.j) % 3 == 0 else R

    def leg(p):
        return mix(R, Y, 0.5) if p.y < 1.2 else R

    m.box("body", "Body", (-6, 2.6, -3.5), (6, 6.6, 3.5), shell)
    m.box("shell-top", "Body", (-4.5, 6.2, -2.8), (4.5, 7.3, 2.8), shell)
    m.bone("Head", (0, 6.5, -3))
    m.pair("eye", "Head", (-2.6, 6.4, -3.7), (-1.6, 9, -2.7), R, stalk_d)
    m.bone("claw-l", (-6, 4, -2.5))
    m.bone("claw-r", (6, 4, -2.5))
    CL, CO, CR = ("claw-l", "claw-r"), (-6, 4, -2.5), (0, -16, 0)
    m.pair("claw-arm", CL, (-8.8, 3, -5.2), (-5.8, 5, -2.5), leg, rot=CR, origin=CO)
    m.pair("pincer-up", CL, (-9.6, 3.9, -9.6), (-6.6, 6.3, -4.6), claw, rot=CR, origin=CO)
    m.pair("pincer-low", CL, (-9.2, 2.5, -9.1), (-7, 3.7, -4.8), claw, rot=CR, origin=CO)
    side_legs(m, ((-1.3, -20), (0.3, 8), (1.9, 32)), -5.6, -9.4, 4.6, 1.0, R, leg, knee=1.2)
    return m


def turtle():
    m = Model("turtle", 313)
    SH, SD, SL, G, GD, Y = (104, 124, 62), (60, 76, 44), (150, 160, 84), (126, 156, 84), (78, 110, 60), (224, 208, 144)

    def scute(p):
        if p.face == "down":
            return Y
        a = (p.x + 4.5) % 3.0
        b = (p.z + 6.0) % 3.0
        if p.face == "up":
            if a < 1 or b < 1:
                return SD
            return SL if 1.3 < a < 2.4 and 1.3 < b < 2.4 else SH
        if p.side() and p.j == p.h - 1:
            return mix(Y, SH, 0.4)
        k = b if p.face in ("east", "west") else a
        return SD if k < 1 or p.j == 0 else SH

    def skin(p):
        if p.face == "down":
            return Y
        return GD if vnoise(313, p.x / 1.4, p.y / 1.4, p.z / 1.4) > 0.6 else G

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
            c.rect(0, c.h - 2, c.w, 1, (60, 80, 44))
        c.side_eye(1, 1, BLACK, Y)
        if c.face in ("east", "west"):
            c.px(c.fx(0), c.h - 2, (60, 80, 44)), c.px(c.fx(1), c.h - 2, (60, 80, 44))

    def leg_d(c):
        if c.face == "north":
            for i in range(0, c.w, 2):
                c.px(i, c.h - 1, (236, 230, 200))

    m.box("shell", "Body", (-5, 3.2, -6), (5, 7.5, 6.5), scute)
    m.box("dome", "Body", (-4, 7, -4.8), (4, 9.3, 5.2), scute)
    m.box("plastron", "Body", (-4.5, 2, -5.5), (4.5, 3.4, 6), lambda p: Y if (int(p.x + 50) // 2 + int(p.z + 50) // 3) % 2 else mul(Y, 0.9))
    m.bone("tail", (0, 3.2, 6.3))
    m.box("tail", "tail", (-0.8, 2.4, 6), (0.8, 3.6, 8.8), skin)
    m.bone("Head", (0, 4.8, -6))
    m.box("head", "Head", (-2, 3.6, -10.8), (2, 7.2, -5.5), skin, head_d)
    for tag, z in (("F", -6.4), ("B", 3.6)):
        m.bone("Leg-%sL" % tag, (-5, 3.2, z + 1.5))
        m.bone("Leg-%sR" % tag, (5, 3.2, z + 1.5))
        m.pair("leg-" + tag.lower(), ("Leg-%sL" % tag, "Leg-%sR" % tag), (-6.9, 0, z), (-3.9, 3.6, z + 3), skin, leg_d)
    return m


def penguin():
    m = Model("penguin", 314)
    K, W, OR, Y = (44, 46, 58), (246, 246, 240), (240, 150, 40), (250, 212, 80)

    def body(p):
        if p.face == "down":
            return W
        if p.face == "north":
            return W if abs(p.x) < 3 else K
        if p.face in ("east", "west") and p.lz < 0.28 and p.ly < 0.9:
            return W
        return K if (p.i + p.j) % 5 else mul(K, 1.25)

    def head_d(c):
        if c.face == "north":
            c.rect(1, 3, 4, 3, W)
            c.eyes(1, 2)
        elif c.face in ("east", "west"):
            c.rect(c.fx(1) - (1 if c.face == "east" else 0), 3, 2, 3, W)
            c.px(c.fx(1), 2, WHITE)

    def flipper(p):
        inner = p.face == ("east" if p.x < 0 else "west")
        return W if inner else mul(K, 1.2) if p.j == p.h - 1 else K

    def foot(p):
        gap = p.face in ("up", "down") and p.i == 1 and (p.j < 2 if p.face == "up" else p.j > p.h - 3)
        return OR + ((0,) if gap else (255,))

    m.box("body", "Body", (-4, 2, -3.5), (4, 13, 3.5), body)
    m.bone("tail", (0, 3, 3.5))
    m.box("tail", "tail", (-1.5, 2, 3), (1.5, 3.6, 5.8), K, rot=(-10, 0, 0), origin=(0, 3, 3.5))
    wings(m, (-4, 12, 0), (-5.1, 4, -1.8), (-3.9, 12.3, 2), flipper, rot=(0, 0, -8))
    m.bone("Head", (0, 13, 0))
    m.box("head", "Head", (-3, 12.5, -3.5), (3, 18.5, 3), K, head_d)
    m.box("beak", "Head", (-1, 14.2, -6), (1, 15.8, -3.3), lambda p: (120, 60, 30) if p.lz < 0.3 else OR)
    m.pair("crest", "Head", (-3.6, 16.4, -3.2), (-2.9, 17.5, 1.5), lambda p: Y + ((0,) if p.face in ("east", "west") and p.j == 0 and p.i % 2 else (255,)))
    for tag, sx in (("L", -1), ("R", 1)):
        m.bone("Leg-" + tag, (2 * sx, 2, 0))
        m.box("foot-" + tag.lower(), "Leg-" + tag, (2 * sx - 1.5, 0, -4), (2 * sx + 1.5, 2.3, 1.5), foot)
    return m


def monkey():
    m = Model("monkey", 315)
    B, D, F, FD = (138, 98, 66), (100, 70, 48), (234, 198, 162), (170, 120, 100)
    coat = fur(315, B, D, None, scale=2.2)

    def body(p):
        if p.face == "north" and abs(p.x) < 2 and p.ly < 0.85:
            return mix(F, B, 0.25)
        return coat(p)

    def limb(p):
        return F if p.y < (1.2 if p.part.startswith("leg") else 5) else coat(p)

    def face_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, 1, D)
            c.sym(1, 1, BLACK)
            c.sym(0, 1, FD)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(0, 0, (90, 60, 50))
            c.rect(0, 1, c.w, 1, (120, 70, 66))

    m.box("body", "Body", (-3, 6.5, -2), (3, 14.5, 2.6), body)
    m.bone("tail-a", (0, 7.5, 2.6))
    m.box("tail-a", "tail-a", (-0.6, 6.9, 2.3), (0.6, 8.1, 7.5), coat)
    m.bone("tail-b", (0, 7.5, 7), "tail-a")
    m.box("tail-b", "tail-b", (-0.58, 6.92, 6.4), (0.58, 13, 7.55), coat)
    m.bone("tail-c", (0, 12.5, 7), "tail-b")
    m.box("tail-c", "tail-c", (-0.56, 11.9, 4.6), (0.56, 13.05, 7.5), coat)
    m.bone("Head", (0, 14.5, 0))
    m.box("head", "Head", (-3.5, 14, -3.5), (3.5, 20.5, 3), coat)
    m.box("face", "Head", (-2.6, 14.4, -4.2), (2.6, 19, -3.3), F, face_d)
    m.box("muzzle", "Head", (-1.6, 14.4, -5.2), (1.6, 16.6, -4), mix(F, WHITE, 0.3), muzzle_d)
    ears(m, (-4.9, 16.2, -1), (-3.3, 18.6, 0.6), lambda p: FD if p.face == "north" and p.i in (0, p.w - 1) and p.j > 0 else F,
         pivot=(-3.5, 17.4, -0.2))
    for tag, sx in (("L", -1), ("R", 1)):
        m.bone("Arm-" + tag, (3.9 * sx, 13.5, 0.3))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (3.9 * sx - 1, 3.5, -0.9), (3.9 * sx + 1, 14.3, 1.5), limb)
        m.bone("Leg-" + tag, (1.6 * sx, 6.5, 0.3))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (1.6 * sx - 1.1, 0, -1), (1.6 * sx + 1.1, 7, 1.6), limb)
    return m


def mouse():
    m = Model("mouse", 316, noise=5)
    G, D, W, PK = (176, 166, 160), (134, 124, 120), (244, 240, 236), (238, 172, 172)
    coat = fur(316, G, D, W, 1.9, 1.6)

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1)

    def snout_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, 1, PK)
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 0, PK)
            c.px(c.fx(1), 1, (120, 110, 110))

    def ear(p):
        if p.face in ("north", "south") and p.j == 0 and p.i in (0, p.w - 1) and p.w > 2:
            return G + (0,)
        return PK if p.face == "north" and p.j > 0 else G

    m.box("body", "Body", (-2, 1, -3), (2, 5, 1), coat)
    m.box("rump", "Body", (-2.4, 0.9, 0.5), (2.4, 5.5, 3.6), coat)
    TP, TR = (0, 2, 3.5), (-10, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-0.4, 1.6, 3.3), (0.4, 2.4, 7.6), PK, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 2, 7.5), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-0.35, 1.65, 7.4), (0.35, 2.35, 12), mul(PK, 0.92), rot=TR, origin=TP)
    m.bone("Head", (0, 3, -3))
    m.box("head", "Head", (-1.6, 1.2, -6), (1.6, 4.6, -2.7), fur(316, G, D, W, 1.9, 1.6), head_d)
    m.box("snout", "Head", (-0.8, 1.4, -7.4), (0.8, 3, -5.8), lambda p: W if p.face == "down" else G, snout_d)
    ears(m, (-3, 4, -4.4), (-0.6, 6.6, -3.6), ear)
    four_legs(m, -2, -1, -2.8, 2, 1, 1, PK, sink=0.6)
    return m


def owl():
    m = Model("owl", 317)
    B, D, C, Y, BK = (156, 116, 78), (102, 74, 52), (238, 222, 192), (250, 200, 40), (216, 176, 90)

    def body(p):
        if p.face == "north":
            return D if (p.j + abs(p.i * 2 - p.w + 1) // 2) % 3 == 0 and p.j > 0 else C
        if p.face == "down":
            return C
        return D if (p.i + 2 * p.j) % 4 == 0 else B

    def head(p):
        if p.face == "north":
            return B
        return D if (p.i + p.j) % 3 == 0 else B

    def head_d(c):
        if c.face == "north":
            c.rect(1, 1, 7, 5, C)
            c.rect(0, 2, 9, 3, C)
            c.symrect(1, 2, 3, 3, Y)
            c.sym(2, 3, BLACK)
            c.sym(1, 1, D), c.sym(2, 1, D), c.sym(3, 2, D)
            c.px(4, 1, B), c.px(4, 2, B)

    def wing(p):
        return D if (p.i + 2 * p.j) % 4 == 0 or p.j == p.h - 1 else mul(B, 0.9)

    def foot(p):
        gap = p.face in ("up", "down") and p.i == 1 and (p.j < 2 if p.face == "up" else p.j > p.h - 3)
        return BK + ((0,) if gap else (255,))

    m.box("body", "Body", (-4, 2, -3.5), (4, 12.5, 3.5), body)
    m.bone("tail", (0, 3.8, 3.5))
    m.box("tail", "tail", (-2.5, 0.8, 3), (2.5, 4.5, 5), lambda p: D if p.j % 2 else B)
    wings(m, (-4, 11.5, 0), (-5.2, 3.5, -2.5), (-3.9, 12, 3), wing)
    m.bone("Head", (0, 12.5, 0))
    m.box("head", "Head", (-4.5, 12, -4), (4.5, 19, 3.5), head, head_d)
    m.box("beak", "Head", (-0.6, 13.8, -5), (0.6, 15.6, -3.8), BK)
    ears(m, (-4.5, 18.5, -2.5), (-2.7, 21, -1), pointed(D), rot=(0, 0, 12))
    for tag, sx in (("L", -1), ("R", 1)):
        x = 2.0 * sx
        m.bone("Leg-" + tag, (x, 2.2, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (x - 0.8, 0.6, -0.8), (x + 0.8, 2.6, 1), C)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (x - 1.5, 0, -2.8), (x + 1.5, 0.8, 1), foot)
    return m


# -- monsters

def slime():
    m = Model("slime", 318, noise=5)
    G, L, D, K = (122, 216, 112), (184, 244, 168), (44, 128, 72), (18, 58, 34)

    def jelly(p):
        # A darker core shows through the middle of every face.
        dx, dy, dz = abs(p.x) / 7.0, abs(p.y - 6.5) / 6.0, abs(p.z) / 7.0
        if p.face in ("up", "down"):
            d = max(dx, dz)
        elif p.face in ("north", "south"):
            d = max(dx, dy)
        else:
            d = max(dz, dy)
        if p.face == "down":
            return mix(D, G, 0.5)
        if d < 0.5:
            return mix(D, G, 0.25 + 0.5 * (d / 0.5) ** 2)
        if d < 0.62:
            return mix(D, G, 0.8)
        edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0
        return L if edge and (p.face == "up" or p.j == 0 or hsh(318, p.i, p.j) > 0.5) else G

    def face_d(c):
        if c.face == "north":
            c.symrect(3, 1, 2, 3, K)
            c.sym(3, 1, WHITE)
            c.rect(5, 6, 4, 1, K)
            c.sym(4, 5, K)

    def blob(p):
        return L if p.face == "up" else G

    m.box("base", "Body", (-8, 0, -8), (8, 4, 8), jelly)
    m.box("middle", "Body", (-7, 3.6, -7), (7, 11.5, 7), jelly, face_d)
    m.box("cap", "Body", (-5.5, 11.1, -5.5), (5.5, 13.5, 5.5), jelly)
    m.box("drip-a", "Body", (-9.6, 0, -4), (-7.6, 2.2, -1), blob)
    m.box("drip-b", "Body", (7.6, 0, 1.5), (9.8, 2.8, 5), blob)
    m.box("drip-c", "Body", (-3, 0, 7.6), (0.5, 1.8, 9.6), blob)
    m.box("drip-d", "Body", (2.5, 0, -9.4), (5.5, 1.6, -7.6), blob)
    m.box("bubble-a", "Body", (-3.5, 13.2, -1), (-1, 15, 1.5), blob)
    m.box("bubble-b", "Body", (1.5, 13.2, 1.5), (3, 14.2, 3), blob)
    return m


def golem():
    m = Model("golem", 319, noise=9)
    S, D, L, M, EY = (130, 126, 122), (90, 86, 88), (164, 160, 152), (88, 124, 62), (255, 176, 44)

    def rock(p):
        n = vnoise(319, p.x / 4.0 + 7, p.y / 4.0, p.z / 4.0)
        c = D if abs(n - 0.5) < 0.035 else L if n > 0.66 else S
        if p.face == "down":
            return D
        if p.face == "up":
            if vnoise(320, p.x / 2.0, p.y, p.z / 2.0) > 0.5:
                c = M
        elif p.j == 0 and hsh(319, p.i, int(p.x + p.z + 50)) > 0.55:
            c = M
        elif min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
            c = mul(c, 0.84)
        return c

    def head_d(c):
        if c.face == "north":
            c.rect(0, 2, c.w, 1, D)
            c.symrect(1, 3, 2, 1, EY)
            c.sym(1, 3, (255, 236, 150))
            c.symrect(1, 4, 2, 1, (150, 80, 30))
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 3, EY)

    def jaw_d(c):
        if c.face == "north":
            for i in range(0, c.w, 2):
                c.px(i, 0, D)

    m.box("pelvis", "Body", (-5, 9, -3), (5, 12.5, 3.5), rock)
    m.box("torso", "Body", (-7.5, 12, -4.5), (7.5, 23.5, 5), rock)
    m.box("belly-rock", "Body", (-5, 11.6, -5.6), (4, 16.5, -4.2), rock, rot=(0, 0, 5), origin=(0, 14, -5))
    m.box("back-rock", "Body", (-5, 19, 4.4), (5.5, 25.5, 7.4), rock, rot=(0, 0, -6), origin=(0, 22, 6))
    m.pair("shoulder", "Body", (-11.6, 19.5, -3.6), (-7, 25.6, 4), rock, rot=(0, 0, 8), origin=(-9, 22, 0))
    m.bone("Head", (0, 23.5, -1.5))
    m.box("head", "Head", (-3.5, 23, -5.5), (3.5, 29.8, 1.5), rock, head_d)
    m.bone("jaw", (0, 23.6, -2), "Head")
    m.box("jaw", "jaw", (-2.9, 21.6, -6), (2.9, 23.5, -1.6), rock, jaw_d)
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 9.3 * sx, 3.2 * sx
        m.bone("Arm-" + tag, (ax, 21, 0.2))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 1.7, 11.5, -2.4), (ax + 1.7, 20.5, 2.8), rock)
        m.box("fist-" + tag.lower(), "Arm-" + tag, (ax - 2.6, 4, -3.4), (ax + 2.4, 12, 3.8), rock)
        m.bone("Leg-" + tag, (lx, 9.5, 0.2))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 2.2, 0, -2.6), (lx + 2.2, 10, 3), rock)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 2.8, 0, -3.8), (lx + 2.8, 2.6, 3.5), rock)
    return m


def wraith():
    m = Model("wraith", 320)
    R, D, L, VD, EY, BN = (60, 54, 82), (36, 32, 52), (96, 88, 126), (8, 6, 14), (140, 240, 255), (214, 214, 204)

    def robe(p):
        if p.face == "up":
            return mix(R, L, 0.4)
        c = D if int(math.floor(p.x * 0.9 + p.z * 0.9 + 100)) % 3 == 0 else R
        if p.face in ("up", "down"):
            return c
        return mix(c, D, 0.5 * (1 - min(1.0, p.y / 16.0)))

    def ragged(n):
        def sh(p):
            c = robe(p)
            if p.side():
                cut = int(hsh(320, int(p.x * 1.01 + 50), int(p.z * 1.01 + 50)) * (n + 1))
                if p.h - 1 - p.j < cut:
                    return c + (0,)
            return c
        return sh

    def hood_d(c):
        if c.face == "north":
            c.rect(1, 1, 6, 7, VD)
            c.sym(2, 3, EY)
            c.sym(2, 4, mul(EY, 0.45))
            c.rect(0, 0, c.w, 1, L)
            c.symrect(0, 0, 1, c.h, L)

    def hand(p):
        gap = p.side() and p.j >= p.h - 1 and p.i % 2 == 1
        return BN + ((0,) if gap else (255,))

    m.box("shoulders", "Body", (-6, 16.5, -3), (6, 21, 3.5), robe)
    m.box("torso", "Body", (-4.5, 9, -2.8), (4.5, 17, 3.2), robe)
    m.box("skirt", "Body", (-4, 3.5, -2.4), (4, 9.5, 3.6), ragged(3), skip=("down",))
    m.bone("tail-a", (0, 7, 3.4))
    m.box("trail-a", "tail-a", (-3.2, 1.5, 3), (3.2, 7.5, 7.5), ragged(3), skip=("down",))
    m.bone("tail-b", (0, 4, 7.2), "tail-a")
    m.box("trail-b", "tail-b", (-2.4, 0, 7), (2.4, 4.5, 11.5), ragged(3), skip=("down",))
    m.bone("Head", (0, 21, 0))
    m.box("hood", "Head", (-4, 20, -4), (4, 28, 4.5), robe, hood_d)
    m.box("hood-peak", "Head", (-2.5, 27.6, 0), (2.5, 30, 5.5), robe)
    for tag, sx in (("L", -1), ("R", 1)):
        ax = 6.1 * sx
        org = (ax, 19.5, 0)
        m.bone("Arm-" + tag, org)
        m.box("sleeve-" + tag.lower(), "Arm-" + tag, (ax - 1.3, 10.5, -2), (ax + 1.3, 20.5, 2.4), ragged(2), rot=(55, 0, 0), origin=org,
              skip=("down",))
        m.box("hand-" + tag.lower(), "Arm-" + tag, (ax - 0.8, 8.4, -1.2), (ax + 0.8, 11, 1.4), hand, rot=(55, 0, 0), origin=org)
    return m


def crawler():
    m = Model("crawler", 321, noise=6)
    C, D, L, U, EY, MN = (74, 62, 90), (44, 36, 58), (126, 108, 138), (150, 122, 112), (236, 64, 50), (214, 200, 172)

    def plate(p):
        if p.face == "down":
            return U if p.j % 2 else mul(U, 0.88)
        if p.side() and p.ly < 0.28:
            return mix(U, C, 0.5)
        if p.lz > 0.84 and p.face != "south":
            return D
        if p.face == "up" and abs(p.x) < 0.8:
            return L
        if p.face == "up" and abs(abs(p.x) - 2.5) < 0.5 and 0.3 < p.lz < 0.6:
            return (120, 200, 170)
        return D if vnoise(321, p.x / 1.5, p.y / 1.5, p.z / 1.5) > 0.66 else C

    def head(p):
        if p.face == "down":
            return U
        return D if p.face == "up" and p.lz > 0.6 else C

    def head_d(c):
        if c.face == "north":
            c.sym(1, 1, EY), c.sym(2, 1, EY)
            c.sym(0, 0, mul(EY, 0.7)), c.sym(2, 0, (255, 190, 120))
            c.rect(2, 3, c.w - 4, 1, (30, 20, 30))
        c.side_eye(1, 1, EY)
        c.side_eye(3, 0, mul(EY, 0.7))

    def mandible(p):
        return MN if p.lz < 0.35 else mix(MN, D, 0.6)

    def shin(p):
        return MN if p.y < 1 else D if int(p.y) % 2 else C

    for i, (w, top) in enumerate(((4.2, 6), (4.6, 6.7), (4.2, 6.1), (3.3, 5.2))):
        z0 = -7 + i * 5.2
        m.box("seg-%d" % i, "Body", (-w, 1.8 + 0.03 * i, z0), (w, top, z0 + 5.6), plate)
    m.pair("tail-spike", "Body", (-1.7, 3, 13.6), (-0.8, 4, 17.5), mandible, rot=(-12, -16, 0), origin=(-1.25, 3.5, 13.6))
    m.bone("Head", (0, 4, -7))
    m.box("head", "Head", (-3.6, 1.6, -11.5), (3.6, 5.8, -6.6), head, head_d)
    m.pair("mandible", "Head", (-3.4, 1.2, -15.4), (-2.2, 2.6, -11), mandible, rot=(0, -22, 0), origin=(-2.8, 1.9, -11))
    side_legs(m, ((-6, -32), (-2.4, -16), (1.2, 0), (4.8, 16), (8.4, 32)), -3.8, -8, 4, 1.0, C, shin, knee=1.6)
    return m


def skeleton_hound():
    m = Model("skeleton_hound", 322, noise=5)
    BN, SH, DK, EY = (234, 228, 208), (184, 174, 152), (58, 52, 54), (255, 72, 50)

    def bone(p):
        return SH if p.face == "down" or hsh(322, int(p.x * 2 + 99), int(p.y * 2), int(p.z * 2 + 99)) > 0.82 else BN

    def spine(p):
        return SH if int(math.floor(p.z + 100)) % 2 else BN

    def ribs(p):
        rib = int(math.floor(p.z + 100)) % 2 == 0
        if p.face in ("east", "west"):
            on = rib and not (p.ly < 0.2 and p.lz > 0.55)
        elif p.face == "up":
            on = rib or abs(p.x) < 0.8
        elif p.face == "down":
            on = abs(p.x) < 0.8 and p.lz < 0.7
        else:
            on = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0 and not (p.j == p.h - 1 and abs(p.x) > 1)
        return bone(p) + ((255,) if on else (0,))

    def pelvis(p):
        if p.face in ("east", "west") and 0.3 < p.lz < 0.7 and 0.25 < p.ly < 0.65:
            return DK
        return bone(p)

    def leg(p):
        if p.y < 1.4 or abs(p.y - 5.5) < 0.8 or p.y > 9.5:
            return BN
        return SH

    def skull_d(c):
        if c.face == "north":
            c.symrect(0, 1, 2, 2, DK)
            c.sym(1, 1, EY)
            c.px(2, 3, DK)
        elif c.face in ("east", "west"):
            x = c.fx(1) - (1 if c.face == "east" else 0)
            c.rect(x, 1, 2, 2, DK)
            c.px(c.fx(1), 1, EY)
            c.px(c.fx(3), 3, SH), c.px(c.fx(4), 3, SH)

    def snout_d(c):
        if c.face == "north":
            c.px(1, 0, DK)
            c.sym(0, c.h - 1, WHITE)
        elif c.face in ("east", "west"):
            for k in range(0, c.w, 2):
                c.px(c.fx(k), c.h - 1, DK)
            c.px(c.fx(0), c.h - 1, WHITE)

    def jaw(p):
        teeth = p.face in ("east", "west") and p.j == 0 and p.i % 2 == 1
        return (DK if teeth else SH if p.face == "down" else BN)

    m.box("spine", "Body", (-0.8, 12.5, -8.5), (0.8, 14, 9.5), spine)
    m.box("ribcage", "Body", (-3, 7.5, -7.5), (3, 13.2, 0.5), ribs)
    m.box("pelvis", "Body", (-2.5, 10, 6), (2.5, 13.5, 9.5), pelvis)
    m.pair("blade", "Body", (-3.5, 9.5, -8.3), (-2.7, 14, -5.5), bone)
    TP, TR = (0, 13.2, 9.5), (-24, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-0.6, 12.6, 9.3), (0.6, 13.8, 14), spine, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 13.2, 14), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-0.45, 12.75, 13.8), (0.45, 13.65, 18.5), spine, rot=TR, origin=TP)
    NP, NR = (0, 13, -8.3), (30, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-0.9, 12.2, -11.8), (0.9, 13.8, -8), spine, rot=NR, origin=NP)
    m.bone("Head", (0, 14.5, -11), "neck")
    m.box("skull", "Head", (-2.5, 12.5, -16), (2.5, 17.5, -10.5), bone, skull_d)
    m.box("snout", "Head", (-1.5, 13, -20), (1.5, 15.5, -15.5), bone, snout_d)
    m.bone("jaw", (0, 13, -15), "Head")
    m.box("jaw", "jaw", (-1.3, 11.7, -19.5), (1.3, 12.8, -13.5), jaw)
    four_legs(m, -3.2, -1.9, -8, 7, 1.3, 10, leg)
    return m


# -- more breeds

def dog_shepherd():
    m = Model("dog_shepherd", 323)
    T, K, W, NB, PK = (208, 160, 104), (50, 44, 44), (236, 214, 176), (26, 22, 22), (214, 150, 150)

    def coat(p):
        if p.face == "down" or p.y < 10:
            return W
        if p.face == "up" or p.y > 13 + 1.6 * vnoise(323, p.x / 3.0, 0, p.z / 3.0):
            return K if (p.i + p.j) % 4 else mul(K, 1.4)
        return T

    def chest(p):
        if p.face == "north" and p.y < 14:
            return W if abs(p.x) < 2.2 else T
        return coat(p)

    def head(p):
        if p.face == "up":
            return K if abs(p.x) < 1.5 else T
        return W if p.face == "down" else T

    def head_d(c):
        if c.face == "north":
            c.rect(1, 3, 4, 3, K)
            c.rect(2, 1, 2, 2, K)
            c.sym(1, 2, (120, 70, 30))
            c.sym(1, 1, K)
        c.side_eye(1, 2, (120, 70, 30))

    def snout(p):
        return T if p.face == "down" or (p.side() and p.ly < 0.3 and p.lz > 0.4) else K

    def tail(p):
        return K if p.lz > 0.7 or p.face == "up" else T

    m.box("body", "Body", (-3.2, 9.6, -3), (3.2, 16.4, 9), coat, rot=(5, 0, 0), origin=(0, 13, -3))
    m.box("chest", "Body", (-3.6, 8.5, -9), (3.6, 17, -2.5), chest)
    m.box("ruff", "Body", (-3.9, 11.5, -9.6), (3.9, 16.2, -7.6), lambda p: T if p.y > 14 else W)
    TP, TR = (0, 14.3, 8.8), (58, 0, 0)
    m.bone("tail", TP)
    m.box("tail", "tail", (-1.3, 13, 8.4), (1.3, 15.6, 17), tail, rot=TR, origin=TP)
    m.bone("Head", (0, 16, -9))
    m.box("head", "Head", (-3, 14, -14.5), (3, 20, -8.5), head, head_d)
    m.box("snout", "Head", (-1.5, 14, -20), (1.5, 17, -14), snout, nose_d(NB, (150, 70, 80)))
    ears(m, (-3, 19.5, -11), (-1, 24.5, -9.8), pointed(mix(T, K, 0.5), PK, K))
    four_legs(m, -3.4, -1.4, -8.2, 6.3, 2, 9, lambda p: W if p.y < 1.5 else T)
    return m


def dog_terrier():
    m = Model("dog_terrier", 324, noise=9)
    W, BR, G, NB = (242, 238, 228), (172, 114, 68), (212, 206, 194), (28, 24, 24)

    def wiry(p):
        return G if hsh(324, p.i, p.j // 2, len(p.face)) > 0.72 else W

    def body(p):
        if p.face != "down" and vnoise(324, p.x / 4.0 + 2, p.y / 4.0, p.z / 4.0 + 1) > 0.6:
            return BR
        return wiry(p)

    def head(p):
        if p.x > 0.4 and p.face != "down" and p.y > 10.4:
            return BR
        return wiry(p)

    def head_d(c):
        if c.face == "north":
            c.sym(1, 2, BLACK)
            c.rect(0, 1, 2, 1, mul(BR, 0.8)), c.rect(4, 1, 2, 1, G)

    def beard(p):
        thin = p.side() and p.j == p.h - 1 and p.i % 2 == 0
        return wiry(p) + ((0,) if thin else (255,))

    m.box("body", "Body", (-2.6, 4.5, -5), (2.6, 10, 5.5), body)
    m.bone("tail", (0, 9.5, 5.2))
    m.box("tail", "tail", (-0.7, 9, 4.6), (0.7, 13.6, 5.9), lambda p: BR if p.ly < 0.4 else W, rot=(12, 0, 0), origin=(0, 9.5, 5.2))
    m.bone("Head", (0, 10, -5))
    m.box("head", "Head", (-2.8, 8.5, -9.5), (2.8, 13.5, -4.5), head, head_d)
    m.box("snout", "Head", (-1.6, 8.6, -12.6), (1.6, 11.4, -9), wiry, nose_d(NB))
    m.box("beard", "Head", (-1.9, 7.2, -12.3), (1.9, 9, -9.4), beard, skip=("up",))
    m.pair("brow", "Head", (-2.6, 12.2, -9.9), (-0.6, 13.1, -9.3), wiry)
    m.bone("ear-l", (-2.1, 13.4, -7.6), "Head")
    m.bone("ear-r", (2.1, 13.4, -7.6), "Head")
    m.pair("ear", ("ear-l", "ear-r"), (-2.9, 12.8, -8.4), (-1.3, 14.3, -6.9), BR)
    m.pair("ear-flap", ("ear-l", "ear-r"), (-2.95, 12.1, -9.4), (-1.35, 13.6, -8.3), mul(BR, 0.85))
    four_legs(m, -2.7, -1.1, -4.6, 3.4, 1.8, 4.5, wiry)
    return m


def dog_hound():
    m = Model("dog_hound", 325)
    W, BR, K, NB = (244, 240, 230), (178, 112, 62), (52, 44, 42), (26, 22, 22)

    def body(p):
        if p.face == "down" or p.y < 7:
            return W
        if p.face == "up" or p.y > 9.3 + vnoise(325, p.x / 3.0, 0, p.z / 3.0):
            return K if -4 < p.z < 7.5 else BR
        return BR

    def head(p):
        if (p.face in ("north", "up")) and abs(p.x) < 1:
            return W
        return W if p.face == "down" else BR

    def head_d(c):
        if c.face == "north":
            c.sym(1, 2, (70, 40, 26))
            c.sym(1, 3, mul(BR, 0.75))
        c.side_eye(1, 2, (70, 40, 26))

    def snout_d(c):
        nose_d(NB)(c)
        if c.face in ("east", "west"):
            c.rect(0, c.h - 2, c.w, 1, mix(W, BR, 0.3))

    def ear(p):
        return mul(BR, 0.8) if p.j >= p.h - 2 or p.face in ("north", "south") else mul(BR, 0.9)

    m.box("body", "Body", (-3, 5, -9), (3, 11.5, 9), body)
    m.box("chest", "Body", (-3.3, 4.5, -9.5), (3.3, 11, -5), lambda p: BR if p.y > 9 and p.face != "north" else W)
    TP, TR = (0, 11, 8.8), (-58, 0, 0)
    m.bone("tail", TP)
    m.box("tail", "tail", (-0.7, 10.4, 8.4), (0.7, 11.8, 16.5), lambda p: W if p.lz > 0.7 else K if p.lz < 0.35 else BR, rot=TR, origin=TP)
    m.bone("Head", (0, 11.5, -9))
    m.box("head", "Head", (-3, 10.5, -15), (3, 16.5, -9), head, head_d)
    m.box("snout", "Head", (-1.8, 10, -20.5), (1.8, 13.8, -14.5), lambda p: BR if p.face == "up" and abs(p.x) > 1 else W, snout_d)
    ears(m, (-4.3, 7.5, -13.5), (-3, 15.8, -10), ear, pivot=(-3, 15.5, -11.5))
    four_legs(m, -3.2, -1, -8.4, 6, 2.2, 5, lambda p: W if p.y < 3 else BR)
    return m


def cat_fluffy():
    m = Model("cat_fluffy", 326, noise=8)
    L, D, W, PK = (234, 230, 224), (190, 184, 180), (252, 250, 246), (236, 160, 160)

    def longfur(p):
        if p.face == "down":
            return W
        if p.side() and (int(p.x + p.z + 100) * 3 + p.j // 2) % 4 == 0:
            return D
        if p.side() and p.j == p.h - 1 and p.i % 2:
            return D
        return mix(L, D, 0.5) if p.face == "up" and abs(p.x) < 1 else L

    def head_d(c):
        if c.face == "north":
            c.sym(1, 2, (90, 170, 210))
            c.sym(1, 1, D), c.sym(2, 1, D)
            c.rect(2, 3, 3, 3, W)

    def muzzle_d(c):
        if c.face == "north":
            c.px(1, 0, PK)
            c.px(1, 1, (150, 120, 120))

    def plume(p):
        return W if p.lz > 0.72 else longfur(p)

    m.box("body", "Body", (-3.2, 5, -7), (3.2, 11.5, 7), longfur)
    m.box("mane", "Body", (-3.9, 5.6, -8.2), (3.9, 12.5, -5.2), lambda p: W if p.face == "north" or p.y < 8 else longfur(p))
    TP, TR = (0, 10, 6.8), (-48, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-1.7, 8.4, 6.4), (1.7, 11.8, 12), longfur, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 10.1, 11.6), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-2.3, 7.8, 11.4), (2.3, 12.4, 18), plume, rot=TR, origin=TP)
    m.bone("Head", (0, 11, -7.5))
    m.box("head", "Head", (-3.5, 8.5, -12.5), (3.5, 14.5, -7), lambda p: W if p.face == "down" else L, head_d)
    m.pair("cheek", "Head", (-4.6, 8.3, -11.6), (-3.3, 11.6, -8.4), lambda p: (W if p.j % 2 else D) if p.face in ("east", "west") else W)
    m.box("muzzle", "Head", (-1.5, 9, -13.3), (1.5, 11, -12), W, muzzle_d)
    ears(m, (-3.5, 14, -10.8), (-1.6, 16.3, -9.6), pointed(L, PK, D))
    four_legs(m, -3, -1, -6.5, 4.5, 2.2, 5, lambda p: W if p.y < 1.6 else longfur(p))
    return m


def cat_lynx():
    m = Model("cat_lynx", 327)
    T, D, W, K, PK = (210, 174, 124), (124, 92, 64), (246, 240, 228), (36, 30, 30), (226, 150, 150)

    def spots(p):
        return hsh(327, int(math.floor(p.x / 1.5 + 50)), int(math.floor(p.y / 1.5)), int(math.floor(p.z / 1.5 + 50))) > 0.7 and \
            (int(p.x * 2 + 100) + int(p.y * 2) + int(p.z * 2 + 100)) % 3 == 0

    def coat(p):
        if p.face == "down" or p.y < 10.4:
            return W
        return D if spots(p) else mix(T, D, 0.35) if p.face == "up" and abs(p.x) < 1 else T

    def leg(p):
        if p.y < 2:
            return mix(T, W, 0.5)
        return D if spots(p) or (p.y < 6 and int(p.y) % 3 == 0 and p.face != "north") else T

    def head_d(c):
        if c.face == "north":
            c.rect(2, 3, 3, 3, W)
            c.rect(1, 4, 5, 2, W)
            c.sym(1, 2, (226, 190, 70))
            c.sym(1, 1, K), c.sym(0, 2, K)
            c.px(3, 0, D), c.px(3, 1, D)

    def muzzle_d(c):
        if c.face == "north":
            c.px(1, 0, PK)
            c.px(1, 1, (140, 100, 100))
            c.sym(0, 1, (150, 150, 150))

    def ruff(p):
        if p.face in ("east", "west"):
            return K if p.i % 2 == 0 and p.j >= p.h - 2 else W if p.j else T
        return T if p.face == "up" else W

    m.box("body", "Body", (-3, 9, -7), (3, 15, 7), coat)
    m.bone("tail", (0, 14, 7))
    m.box("tail", "tail", (-1, 12.4, 6.6), (1, 14.6, 9.6), lambda p: K if p.lz > 0.55 else T, rot=(-14, 0, 0), origin=(0, 14, 7))
    m.bone("Head", (0, 14.5, -7))
    m.box("head", "Head", (-3.5, 12, -12.5), (3.5, 18, -7), lambda p: W if p.face == "down" else D if p.face == "up" and spots(p) else T, head_d)
    m.pair("ruff", "Head", (-4.9, 10.8, -11), (-3.3, 14.6, -8.4), ruff)
    m.box("muzzle", "Head", (-1.5, 12.5, -13.5), (1.5, 14.5, -12), W, muzzle_d)
    ears(m, (-3.5, 17.5, -10.5), (-1.6, 20.5, -9.3), pointed(T, W, K))
    m.pair("ear-tuft", ("ear-l", "ear-r"), (-2.5, 20.2, -10.2), (-1.75, 22.6, -9.6), K)
    four_legs(m, -3.2, -1, -6.6, 4.2, 2.4, 9, leg)
    return m


def cow_longhorn():
    m = Model("cow_longhorn", 328)
    BR, DB, W, HN, HF, PK = (180, 106, 62), (128, 72, 44), (242, 236, 224), (234, 224, 198), (60, 50, 46), (226, 168, 160)

    def hide(p):
        n = vnoise(328, p.x / 5.0 + 2, p.y / 5.0, p.z / 5.0)
        if n > 0.56:
            return W if hsh(328, p.i, p.j, len(p.face)) > 0.18 else BR
        if n < 0.34:
            return DB
        return BR if hsh(328, p.i, p.j, len(p.face) + 3) > 0.08 else W

    def leg(p):
        return HF if p.y < 2 else hide(p) if p.y > 6 else BR

    def head(p):
        if p.face in ("north", "up") and abs(p.x) < 1.2:
            return W
        return W if p.face == "down" else BR

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 3)
        c.side_eye(1, 3, BLACK, WHITE)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (150, 90, 95))
            c.rect(1, 3, c.w - 2, 1, (170, 105, 108))

    def horn(p):
        return mix(HN, (190, 170, 130), 0.6) if int(abs(p.x) * 1.2) % 3 == 0 else HN

    m.box("body", "Body", (-5.5, 10, -9), (5.5, 20.5, 10), hide)
    m.box("hump", "Body", (-4, 20, -8.5), (4, 22, -3), hide)
    m.box("dewlap", "Body", (-1.5, 7.8, -9.4), (1.5, 11, -3), lambda p: mix(BR, W, 0.5))
    m.bone("tail", (0, 20, 10))
    m.box("tail", "tail", (-0.5, 9, 9.5), (0.5, 20, 10.5), lambda p: DB if p.ly < 0.25 else BR, rot=(-8, 0, 0), origin=(0, 20, 10))
    m.bone("Head", (0, 18, -9))
    m.box("head", "Head", (-4, 14, -16), (4, 22, -8.5), head, head_d)
    m.box("muzzle", "Head", (-3, 14, -18), (3, 18, -15.5), PK, muzzle_d)
    HO, HR = (-9.8, 21.4, -12.75), (0, 0, -20)
    m.pair("horn-a", "Head", (-10, 20.6, -13.5), (-3.8, 22.1, -12), horn)
    m.pair("horn-b", "Head", (-15.5, 20.75, -13.4), (-9.6, 22.05, -12.1), horn, rot=HR, origin=HO)
    m.pair("horn-c", "Head", (-15.45, 21.6, -13.35), (-14.3, 25.5, -12.15), lambda p: (50, 40, 36) if p.ly > 0.55 else HN, rot=HR, origin=HO)
    ears(m, (-6.8, 17.5, -11.5), (-3.6, 19.3, -10.5), lambda p: PK if p.face == "north" and 0.2 < p.lx < 0.8 else BR,
         pivot=(-3.8, 18.4, -11))
    four_legs(m, -5.5, -2.7, -8, 6, 2.8, 10, leg)
    return m


def pig_boar():
    m = Model("pig_boar", 329, noise=9)
    B, D, L, TK, HF = (100, 76, 58), (62, 48, 42), (146, 116, 90), (242, 234, 212), (40, 34, 32)

    def hide(p):
        if p.face == "down":
            return L
        if p.side() and (int(p.x + p.z + 100) * 2 + p.j) % 3 == 0:
            return D
        return D if p.face == "up" and abs(p.x) < 1.5 else B

    def bristle(p):
        cut = p.face in ("east", "west") and (p.j == 0 and p.i % 2 == 1 or p.j == 1 and p.i % 4 == 3)
        return (D if p.j > 1 else mix(D, L, 0.5)) + ((0,) if cut else (255,))

    def head_d(c):
        if c.face == "north":
            c.sym(1, 3, (230, 120, 40))
            c.sym(1, 2, D), c.sym(2, 2, D)
        c.side_eye(1, 3, BLACK)

    def snout_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, c.h - 1, (190, 130, 124))
            c.sym(1, 1, (90, 50, 54))
            c.rect(0, c.h - 1, c.w, 1, D)

    m.box("body", "Body", (-5, 6, -8), (5, 15, 9), hide)
    m.box("shoulders", "Body", (-4.5, 14.5, -8), (4.5, 17, 0.5), hide)
    m.box("bristles-f", "Body", (-0.5, 16.6, -8), (0.5, 19.4, 0.5), bristle, skip=("up",))
    m.box("bristles-b", "Body", (-0.5, 14.6, 0.5), (0.5, 17, 8.5), bristle, skip=("up",))
    m.bone("tail", (0, 13, 9))
    m.box("tail", "tail", (-0.5, 7.5, 8.7), (0.5, 13.5, 9.7), lambda p: D if p.ly < 0.3 else B, rot=(-14, 0, 0), origin=(0, 13, 9))
    m.bone("Head", (0, 11, -8))
    m.box("head", "Head", (-4, 6.5, -15), (4, 15, -7.5), hide, head_d)
    m.box("snout", "Head", (-2, 7, -19.5), (2, 10.5, -14.5), lambda p: mix(B, L, 0.5) if p.face != "up" else B, snout_d)
    m.pair("tusk", "Head", (-3, 8, -18.2), (-2, 11.2, -17.2), lambda p: TK if p.ly > 0.3 else mix(TK, (200, 180, 140), 0.7),
           rot=(-14, 0, 14), origin=(-2.5, 8, -17.7))
    ears(m, (-4.6, 14.3, -12.5), (-2.4, 17.3, -11.3), pointed(D, (150, 100, 96)), rot=(0, 0, 10))
    four_legs(m, -5, -2.4, -7.5, 5.5, 2.8, 6, lambda p: HF if p.y < 1.6 else D if p.y < 4 else hide(p))
    return m


def chicken_rooster():
    m = Model("chicken_rooster", 330)
    RB, DB, GD, TG, TB, RED, YL, OR = (176, 74, 38), (104, 44, 30), (238, 172, 60), (30, 78, 66), (20, 40, 60), (206, 40, 36), (240, 190, 60), (226, 150, 50)

    def feathers(p):
        if p.side() and (int(p.z + 50) + int(p.y + 50) * 2) % 4 == 0:
            return DB
        return TB if p.face == "north" or (p.side() and p.lz < 0.2 and p.ly < 0.7) or p.face == "down" else RB

    def wing(p):
        if p.ly < 0.3:
            return TG
        return DB if (p.i + p.j * 2) % 4 == 0 else RB

    def hackle(p):
        if p.side() and p.j == p.h - 1 and p.i % 2:
            return GD + (0,)
        return mix(GD, RB, 0.6) if p.i % 2 == 0 and p.side() else GD

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK)
        if c.face in ("east", "west"):
            c.rect(c.fx(2) - (1 if c.face == "east" else 0), 2, 2, 2, RED)

    def comb(p):
        cut = p.face in ("east", "west") and (p.j == 0 and p.i % 2 == 1 or (p.j < 2 and p.i in (0, p.w - 1)))
        return RED + ((0,) if cut else (255,))

    def wattle(p):
        cut = p.face in ("north", "south") and p.i == p.w // 2 and p.j > 0 and p.w > 2
        return RED + ((0,) if cut else (255,))

    def plume(col):
        def sh(p):
            round_ = p.face in ("east", "west") and p.j == 0 and p.i in (0, p.w - 1)
            c = col if (p.j // 2 + p.i) % 3 else mix(col, (90, 200, 150), 0.45)
            return c + ((0,) if round_ else (255,))
        return sh

    def foot(p):
        gap = p.face in ("up", "down") and p.i == 1 and (p.j < 2 if p.face == "up" else p.j > p.h - 3)
        return OR + ((0,) if gap else (255,))

    m.box("body", "Body", (-4, 5.5, -5), (4, 13.5, 5), feathers)
    wings(m, (-4, 12, -1), (-5.1, 7, -4), (-3.8, 12.5, 3.5), wing)
    TP = (0, 12.5, 4.8)
    m.bone("tail", TP)
    m.box("plume-a", "tail", (-1, 12, 4.4), (1, 22, 7.4), plume(TG), rot=(22, 0, 0), origin=TP)
    m.box("plume-b", "tail", (-0.6, 11.5, 5), (0.6, 24, 8), plume(TB), rot=(50, 0, 0), origin=TP)
    m.box("plume-c", "tail", (-0.5, 11.5, 5.4), (0.5, 22, 8), plume(TG), rot=(80, 0, 0), origin=TP)
    m.pair("plume-side", "tail", (-2.6, 11.6, 4.4), (-1.6, 19, 6.9), plume(TB), rot=(38, 0, 8), origin=(-2, 12.5, 4.8))
    NP = (0, 13, -3.5)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2.2, 11.5, -6.5), (2.2, 17.5, -2.5), hackle)
    m.bone("Head", (0, 17, -4.5), "neck")
    m.box("head", "Head", (-2.3, 16.5, -8), (2.3, 21, -3.5), lambda p: mix(RB, GD, 0.5), head_d)
    m.box("beak", "Head", (-1, 17.8, -10.5), (1, 19.3, -7.6), YL,
          lambda c: c.face in ("east", "west", "north") and c.rect(0, 1, c.w, 1, OR))
    m.box("wattle", "Head", (-1.3, 13.8, -8.9), (1.3, 17.6, -7.7), wattle)
    m.box("comb", "Head", (-0.5, 20.5, -9), (0.5, 24.6, -3), comb, skip=("up",))
    for tag, sx in (("L", -1), ("R", 1)):
        x = 2.2 * sx
        m.bone("Leg-" + tag, (x, 5.6, 0.5))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (x - 0.5, 0.5, 0), (x + 0.5, 6.5, 1), OR)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (x - 1.5, 0, -2.7), (x + 1.5, 0.8, 1.2), foot)
        m.box("spur-" + tag.lower(), "Leg-" + tag, (x - 0.3, 2.2, 0.9), (x + 0.3, 2.8, 2.3), YL)
    return m


def horse_pony():
    m = Model("horse_pony", 331)
    C, D, MN, MD, HF, W = (216, 162, 110), (180, 126, 84), (248, 238, 212), (214, 198, 164), (56, 48, 44), (250, 246, 238)

    def coat(p):
        if p.face == "down":
            return D
        return D if vnoise(331, p.x / 4.0, p.y / 4.0, p.z / 4.0) > 0.62 else C

    def leg(p):
        if p.y < 1.5:
            return HF
        if p.y < 4:
            return (MN if p.i % 2 else MD) if p.face != "north" else MD
        return coat(p)

    def head(p):
        if p.face in ("north", "up") and abs(p.x) < 1:
            return W
        return coat(p)

    def head_d(c):
        c.side_eye(2, 3, BLACK, WHITE)
        if c.face == "north":
            c.sym(0, 3, BLACK)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (70, 50, 48))
            c.rect(0, c.h - 1, c.w, 1, (110, 84, 78))

    def hair(p):
        return MN if (p.i + p.j // 2) % 3 else MD

    def fringe(p):
        cut = p.face in ("north", "east", "west") and p.j == p.h - 1 and p.i % 2 == 1
        return hair(p) + ((0,) if cut else (255,))

    NP, NR = (0, 17.5, -8), (-30, 0, 0)
    m.box("body", "Body", (-5.5, 9, -9), (5.5, 19.5, 9), coat)
    m.box("belly", "Body", (-4.5, 7.8, -6), (4.5, 9.5, 6), coat)
    m.bone("tail", (0, 18.5, 9))
    m.box("tail", "tail", (-1.8, 3, 8.6), (1.8, 19, 11.4), hair, rot=(-8, 0, 0), origin=(0, 18.5, 9))
    m.bone("neck", NP)
    m.box("neck", "neck", (-3.2, 14.5, -13), (3.2, 25, -6.5), coat, rot=NR, origin=NP)
    m.box("mane", "neck", (-1.4, 16, -7.5), (1.4, 27, -4.8), hair, rot=NR, origin=NP)
    m.bone("Head", (0, 23.5, -13), "neck")
    m.box("head", "Head", (-3.2, 20, -21), (3.2, 27, -12), head, head_d)
    m.box("muzzle", "Head", (-2.4, 20, -24.5), (2.4, 24.5, -20.5), lambda p: (160, 124, 104), muzzle_d)
    m.box("forelock", "Head", (-2.6, 24.4, -19), (2.6, 28.2, -12.5), fringe, skip=("down",))
    ears(m, (-2.9, 27.6, -14.8), (-1.5, 30, -13.7), pointed(C, (200, 150, 140)))
    four_legs(m, -5.3, -2.3, -8.5, 5.5, 3, 9, leg)
    return m


def sheep_ram():
    m = Model("sheep_ram", 332, noise=9)
    W, SH, F, FD, HF, HN, HD = (240, 236, 226), (202, 198, 188), (120, 100, 84), (86, 70, 60), (60, 52, 48), (212, 192, 152), (150, 128, 96)

    def wool(p):
        n = vnoise(332, p.x / 1.7, p.y / 1.7, p.z / 1.7)
        c = SH if n > 0.6 else W
        if (int(p.x + 100) + int(p.y + 100) + int(p.z + 100)) % 5 == 0 and n > 0.4:
            c = mul(SH, 0.93)
        return c

    def leg(p):
        if p.y < 1.5:
            return HF
        return wool(p) if p.y > 6 else F

    def head_d(c):
        if c.face == "north":
            c.eyes(0, 3, BLACK, (236, 200, 90))
            c.rect(2, 5, 2, 1, FD)
            c.rect(2, 6, 2, 1, (60, 46, 42))
            c.rect(2, 0, 2, 3, mix(F, W, 0.4))
        c.side_eye(1, 3, BLACK)

    def horn(p):
        ring = int(math.floor((p.y + p.z) * 1.1 + 100)) % 2 == 0
        return HD if ring else HN

    m.box("body", "Body", (-6.5, 9, -9), (6.5, 21.5, 9), wool)
    m.box("chest", "Body", (-5.5, 8.5, -10.5), (5.5, 19, -8.5), wool)
    m.bone("tail", (0, 17, 9))
    m.box("tail", "tail", (-1.5, 13, 8.5), (1.5, 18, 10.5), wool)
    m.bone("Head", (0, 17.5, -9.5))
    m.box("head", "Head", (-3.2, 13.5, -16.5), (3.2, 21, -10), lambda p: FD if p.face == "down" else F, head_d)
    m.box("cap", "Head", (-3.8, 19.5, -14), (3.8, 22.5, -9.5), wool)
    m.pair("horn-a", "Head", (-5.2, 19.6, -13.6), (-3, 21.9, -9), horn)
    m.pair("horn-b", "Head", (-5.7, 14.4, -10.7), (-3.5, 20.1, -8.4), horn)
    m.pair("horn-c", "Head", (-6.2, 13.5, -14.6), (-4, 15.7, -10.2), horn)
    m.pair("horn-d", "Head", (-6.9, 15.2, -15.3), (-5, 17.8, -13.5), lambda p: HN if p.face != "up" else (236, 224, 196))
    ears(m, (-5.4, 16.8, -12.6), (-3, 18.1, -11.5), lambda p: (200, 150, 140) if p.face == "north" else F, pivot=(-3.2, 17.4, -12))
    four_legs(m, -5.5, -2.5, -8, 5, 3, 9, leg)
    return m


MORE_ANIMALS = [wolf, fox, rabbit, deer, goat, bird, bat, butterfly, bee, beetle, spider, crab, turtle, penguin, monkey, mouse, owl,
                slime, golem, wraith, crawler, skeleton_hound,
                dog_shepherd, dog_terrier, dog_hound, cat_fluffy, cat_lynx, cow_longhorn, pig_boar, chicken_rooster, horse_pony, sheep_ram]


# ---------------------------------------------------------------- vehicles

def planks(base, seam_of, tone_of=None, period=3, seed=7):
    """Wood: `seam_of(p)` is the coordinate across the planks."""
    def sh(p):
        s = seam_of(p)
        k = int(math.floor(s / period + 100))
        c = mul(base, 0.9 + 0.16 * hsh(seed, k))
        if int(math.floor(s + 1000)) % period == 0:
            c = mul(c, 0.72)
        g = tone_of(p) if tone_of else p.x + p.z
        if hsh(seed, k, int(math.floor(g / 2.5))) > 0.82:
            c = mul(c, 0.86)
        return c
    return sh


def chest(m, lo, hi, bone="Body"):
    WD, FR, MT = (168, 116, 60), (88, 56, 30), (196, 196, 204)
    lid = lo[1] + (hi[1] - lo[1]) * 0.62

    def sh(p):
        if p.face in ("up", "down"):
            edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0
        else:
            edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0 or abs(p.y - lid) < 0.5
        if edge:
            return FR
        s = p.x + p.z if p.side() else p.x
        return planks(WD, lambda q: s, lambda q: q.y + q.z, 3, 31)(p)

    def dec(c):
        if c.face == "north":
            x, y = c.w // 2 - 1, int(round(c.h * 0.38)) - 1
            c.rect(x, y, 2, 3, MT)
            c.px(x, y + 2, (90, 90, 100)), c.px(x + 1, y + 2, (90, 90, 100))
        if c.face in ("north", "south", "up"):
            for x in (1, c.w - 2):
                c.rect(x, 0, 1, c.h, (70, 70, 78))

    m.box("chest", bone, lo, hi, sh, dec)


def boat_hull(m):
    WD, TR = (176, 128, 72), (110, 74, 40)

    def side(p):
        if p.face == "up":
            return TR
        return planks(WD, lambda q: q.y * 1.5, lambda q: q.z + q.x, 3, 21)(p)

    def floor(p):
        return planks(mul(WD, 0.9), lambda q: q.x, lambda q: q.z, 3, 22)(p)

    def seat(p):
        return planks(mul(WD, 1.1), lambda q: q.z, lambda q: q.x, 2, 23)(p)

    m.box("floor", "Body", (-5.5, 0.5, -10), (5.5, 2, 10.5), floor)
    m.box("keel", "Body", (-0.8, 0, -15), (0.8, 1, 11), TR)
    m.pair("side", "Body", (-7, 1, -10), (-5, 7, 11), side)
    m.box("stern", "Body", (-5.2, 1, 10), (5.2, 7, 12), side)
    m.box("bow-floor", "Body", (-3.5, 0.6, -15.5), (3.5, 2.5, -9.8), floor)
    m.pair("bow-a", "Body", (-5.8, 1.2, -13.5), (-3.6, 7, -9.5), side)
    m.pair("bow-b", "Body", (-4, 1.6, -16.5), (-1.8, 7, -13), side)
    m.box("prow", "Body", (-2, 1.8, -18.5), (2, 8, -16), side)
    m.pair("oarlock", "Body", (-6.6, 6.8, 0.5), (-5.4, 8.2, 1.7), (80, 80, 88))
    return side, seat


def boat():
    m = Model("boat", 201)
    _, seat = boat_hull(m)
    m.box("seat-fore", "Body", (-5.2, 4.6, -5), (5.2, 5.6, -2), seat)
    m.box("seat-aft", "Body", (-5.2, 4.6, 4), (5.2, 5.6, 7.5), seat)
    for sx in (-1, 1):
        m.box("oar-%s" % ("l" if sx < 0 else "r"), "Body", (sx * 6 - 0.5, 7.2, -8), (sx * 6 + 0.5, 8.2, 8),
              lambda p: (150, 108, 60) if p.lz > 0.25 else (196, 150, 90), rot=(0, sx * -10.0, 0), origin=(sx * 6, 7.5, 1))
        m.box("blade-%s" % ("l" if sx < 0 else "r"), "Body", (sx * 6 - 1.3, 7.4, -11.5), (sx * 6 + 1.3, 8.0, -7.5),
              (196, 150, 90), rot=(0, sx * -10.0, 0), origin=(sx * 6, 7.5, 1))
    return m


def cargo_boat():
    m = Model("cargo_boat", 202)
    _, seat = boat_hull(m)
    m.box("seat-aft", "Body", (-5.2, 4.6, 5.5), (5.2, 5.6, 8.5), seat)
    chest(m, (-4.5, 2, -6.5), (4.5, 9.5, 2.5))
    m.box("rope", "Body", (-0.6, 2, -6.8), (0.6, 9.8, 2.8), (206, 186, 130), skip=("down",))
    return m


def cart_base(m):
    IR, DK, RV, RS = (124, 126, 134), (78, 80, 88), (176, 178, 186), (132, 96, 70)

    def metal(p):
        c = IR
        if p.side():
            if p.j == 0:
                return DK
            if p.j in (1, p.h - 2) and p.i % 3 == 1:
                return RV
            if p.j == p.h - 1:
                return DK
            if vnoise(203, p.x / 2.0, p.y / 3.0, p.z / 2.0) > 0.72:
                c = mix(IR, RS, 0.55)
        elif p.face == "up":
            return DK
        return c

    m.box("floor", "Body", (-6, 3, -8), (6, 4.5, 8), metal)
    m.pair("wall", "Body", (-7, 3.2, -9), (-5.8, 11, 9), metal)
    m.box("wall-front", "Body", (-6, 3.2, -9), (6, 11, -7.8), metal)
    m.box("wall-back", "Body", (-6, 3.2, 7.8), (6, 11, 9), metal)
    m.box("bumper-front", "Body", (-2, 4.5, -10), (2, 6.5, -8.8), DK)
    m.box("bumper-back", "Body", (-2, 4.5, 8.8), (2, 6.5, 10), DK)
    for tag, z in (("F", -5), ("B", 5)):
        m.box("axle-" + tag.lower(), "Body", (-6.5, 1.5, z - 0.5), (6.5, 2.5, z + 0.5), DK)
        for side, sx in (("L", -1), ("R", 1)):
            b = "Wheel-%s%s" % (tag, side)
            m.bone(b, (sx * 6.5, 2, z))
            x0, x1 = (sx * 7.4, sx * 6.2) if sx < 0 else (sx * 6.2, sx * 7.4)
            wheel(m, "wheel-%s%s" % (tag.lower(), side.lower()), b, x0, x1, 2, z, 2, tire=(70, 72, 80), hub=(150, 152, 160), cut=1)


def cart():
    m = Model("cart", 203)
    cart_base(m)
    return m


def chest_cart():
    m = Model("chest_cart", 204)
    cart_base(m)
    chest(m, (-5, 4.4, -6.5), (5, 14, 6.5))
    return m


def plane():
    m = Model("plane", 205)
    RD, CR, DK, GL, WD = (196, 52, 44), (238, 228, 200), (60, 60, 66), (170, 214, 232), (150, 104, 60)

    def fus(p):
        if p.side() and abs(p.y - 8.5) < 0.6:
            return CR
        if p.face == "up" and -3 < p.z < 2.5 and abs(p.x) < 2:
            return (50, 40, 36)  # open cockpit
        if p.face == "down":
            return mul(RD, 0.8)
        return RD

    def wing(p):
        if p.face in ("up", "down"):
            if abs(abs(p.x) - 15.5) < 1.5:
                return RD
            if int(math.floor(p.x + 100)) % 4 == 0:
                return mul(CR, 0.88)
            if p.face == "up" and p.j == p.h - 1:
                return mul(CR, 0.8)
        return CR

    def fin(p):
        a = 255 if p.lz > (p.ly - 0.25) * 0.75 else 0
        c = CR if p.ly > 0.62 and a else RD
        return c + (a,)

    def nose_d(c):
        if c.face == "north":
            c.rect(1, 1, c.w - 2, c.h - 2, DK)
            c.rect(2, 2, c.w - 4, c.h - 4, (110, 110, 118))
        elif c.face in ("east", "west"):
            for j in range(1, c.h - 1, 2):
                c.rect(1, j, c.w - 2, 1, DK)

    def blade(p):
        return (232, 200, 60) if abs(p.lx - 0.5) > 0.42 else WD

    m.box("fuselage", "Body", (-3, 5, -10), (3, 11, 8), fus)
    m.box("cowl", "Body", (-2.6, 5.4, -13), (2.6, 10.6, -9.5), lambda p: (150, 152, 160), nose_d)
    m.box("tail-boom", "Body", (-2, 7, 7.5), (2, 11, 17), fus)
    m.box("windshield", "Body", (-2.6, 10.8, -4), (2.6, 13.4, -3), lambda p: GL + (255,) if min(p.i, p.w - 1 - p.i) > 0 and p.j > 0 else DK,
          rot=(-18, 0, 0), origin=(0, 11, -3.5))
    m.box("headrest", "Body", (-1.8, 10.8, 2.2), (1.8, 12.6, 3.4), (96, 62, 40))
    m.pair("wing", "Body", (-19, 5.6, -6.5), (-2.8, 6.6, 0), wing, rot=(0, 0, -5), origin=(-3, 6, -3))
    m.pair("strut", "Body", (-10.5, 6.2, -3.6), (-9.7, 10, -2.8), DK, rot=(0, 0, 38), origin=(-10, 6.4, -3.2))
    m.pair("tailplane", "Body", (-7.5, 9.4, 12.5), (-1.8, 10.2, 17), wing)
    m.box("fin", "Body", (-0.4, 10.8, 10.5), (0.4, 17.5, 17.5), fin, skip=("north", "up"))
    m.box("tail-skid", "Body", (-0.4, 4.6, 13.5), (0.4, 7.2, 14.5), DK, rot=(30, 0, 0), origin=(0, 7, 14))
    m.bone("Prop", (0, 8, -14))
    m.box("hub", "Prop", (-0.9, 7.1, -14.8), (0.9, 8.9, -12.8), DK)
    m.box("blade", "Prop", (-7.5, 7.4, -14.2), (7.5, 8.6, -13.8), blade, rot=(0, 0, 24), origin=(0, 8, -14))
    for side, sx in (("L", -1), ("R", 1)):
        m.box("gear-" + side.lower(), "Body", (sx * 4.5 - 0.5, 2, -6.4), (sx * 4.5 + 0.5, 5.6, -5.4), DK)
        b = "Wheel-" + side
        m.bone(b, (sx * 4.5, 2, -6))
        x0, x1 = (sx * 5.9, sx * 4.9) if sx < 0 else (sx * 4.9, sx * 5.9)
        wheel(m, "wheel-" + side.lower(), b, x0, x1, 2, -6, 2, cut=1)
    return m


def car():
    m = Model("car", 206)
    GN, DG, BRS, LT, SEAT, DK = (58, 122, 84), (38, 84, 58), (214, 176, 76), (250, 240, 170), (140, 70, 44), (52, 52, 58)

    def body(p):
        if p.face == "up":
            return GN if not (p.part == "hood" and int(p.x + 100) % 2 == 0 and 0.2 < p.lz < 0.8) else DG
        if p.side() and p.j == 0:
            return BRS
        if p.face == "down":
            return DK
        return GN

    def hood_d(c):
        if c.face == "north":
            c.rect(2, 1, c.w - 4, c.h - 1, DK)
            for x in range(3, c.w - 3, 2):
                c.rect(x, 1, 1, c.h - 1, (130, 130, 138))
            c.symrect(0, 1, 2, 2, LT)
            c.sym(0, 1, BRS)

    def back_d(c):
        if c.face == "south":
            c.symrect(1, 2, 2, 1, (210, 50, 44))

    def seat(p):
        return mul(SEAT, 0.8) if (p.i % 3 == 0 and p.face in ("north", "up")) else SEAT

    m.box("chassis", "Body", (-6, 3, -12), (6, 5, 12), lambda p: DK if p.face == "down" else (96, 72, 50) if p.face == "up" else DG)
    m.box("hood", "Body", (-5, 4.8, -12.5), (5, 9.5, -4), body, hood_d)
    m.box("dash", "Body", (-5.5, 4.8, -4.5), (5.5, 10.5, -3.3), body)
    m.pair("door", "Body", (-6.2, 4.8, -4.2), (-5, 9, 10), body)
    m.box("back", "Body", (-6.2, 4.8, 9.5), (6.2, 10, 11.5), body, back_d)
    m.box("seat", "Body", (-4.8, 4.9, 2.5), (4.8, 6.8, 8), seat)
    m.box("seat-back", "Body", (-4.8, 6.5, 7), (4.8, 12, 8.8), seat)
    m.box("windshield", "Body", (-5.5, 10.3, -4.4), (5.5, 14.5, -3.8),
          lambda p: ((170, 214, 232, 255) if 0 < p.i < p.w - 1 and p.j > 0 else BRS + (255,)), skip=("up", "down", "east", "west"))
    m.box("column", "Body", (-3.4, 8, -3.6), (-2.6, 11, -0.4), DK, rot=(-30, 0, 0), origin=(-3, 9.5, -3.4))
    m.box("steering", "Body", (-4.6, 9.2, -0.2), (-1.4, 12.4, 0.5),
          lambda p: DK + ((255 if (min(p.i, p.w - 1 - p.i) == 0 or min(p.j, p.h - 1 - p.j) == 0 or p.i == p.w // 2 or p.j == p.h // 2) and not (p.i in (0, p.w - 1) and p.j in (0, p.h - 1)) else 0),),
          skip=("up", "down", "east", "west"), rot=(-30, 0, 0), origin=(-3, 10.8, 0.1), noise=0)
    for tag, z in (("F", -8), ("B", 7)):
        m.box("axle-" + tag.lower(), "Body", (-7, 2.5, z - 0.5), (7, 3.5, z + 0.5), DK)
        m.pair("fender-" + tag.lower(), "Body", (-8.6, 6.2, z - 3.5), (-6, 7, z + 3.5), DG)
        for side, sx in (("L", -1), ("R", 1)):
            b = "Wheel-%s%s" % (tag, side)
            m.bone(b, (sx * 7.5, 3, z))
            x0, x1 = (sx * 8.6, sx * 6.6) if sx < 0 else (sx * 6.6, sx * 8.6)
            wheel(m, "wheel-%s%s" % (tag.lower(), side.lower()), b, x0, x1, 3, z, 3, hub=BRS, cut=1)
    return m


ANIMALS = [cow, pig, chicken, cat, dog, tiger, elephant, bear, gator, snake, fish, squid, worm, ant, sheep, horse, frog, duck]
VEHICLES = [boat, cargo_boat, cart, chest_cart, plane, car]

# ---------------------------------------------------------------- creatures.txt

# id, name, model, tint, height, width, speed, health, damage, behavior, biomes, night, weight
ENTITIES = [
    ("cow", "Cow", "cow", None, 1.4, 0.9, 1.6, 10, 0, "wander", "plains forest savanna", False, 24),
    ("cow_brown", "Brown Cow", "cow", (190, 140, 100), 1.4, 0.9, 1.6, 10, 0, "wander", "plains savanna mountain", False, 14),
    ("cow_red", "Red Cow", "cow", (220, 130, 90), 1.35, 0.9, 1.6, 10, 0, "wander", "plains taiga", False, 8),
    ("pig", "Pig", "pig", None, 0.9, 0.8, 1.7, 8, 0, "wander", "plains forest swamp", False, 24),
    ("pig_spotted", "Dusky Pig", "pig", (200, 170, 160), 0.9, 0.8, 1.7, 8, 0, "wander", "forest taiga", False, 10),
    ("pig_black", "Black Pig", "pig", (95, 85, 90), 0.95, 0.8, 1.8, 9, 0, "wander", "forest jungle swamp", False, 8),
    ("chicken", "Chicken", "chicken", None, 0.6, 0.4, 1.4, 4, 0, "wander", "plains forest savanna", False, 26),
    ("chicken_brown", "Brown Hen", "chicken", (205, 150, 100), 0.6, 0.4, 1.4, 4, 0, "wander", "plains forest", False, 14),
    ("chicken_black", "Black Hen", "chicken", (100, 100, 110), 0.6, 0.4, 1.4, 4, 0, "wander", "forest jungle", False, 8),
    ("cat", "Tabby Cat", "cat", None, 0.5, 0.4, 2.6, 6, 0, "wander", "plains forest", False, 8),
    ("cat_ginger", "Ginger Cat", "cat", (255, 176, 104), 0.5, 0.4, 2.6, 6, 0, "wander", "plains forest savanna", False, 6),
    ("cat_black", "Black Cat", "cat", (84, 82, 92), 0.5, 0.4, 2.6, 6, 0, "wander", "forest swamp", True, 5),
    ("cat_cream", "Cream Cat", "cat", (255, 236, 204), 0.5, 0.4, 2.6, 6, 0, "wander", "plains desert", False, 4),
    ("dog", "Dog", "dog", None, 0.75, 0.5, 3.0, 10, 0, "wander", "plains forest", False, 8),
    ("dog_golden", "Golden Dog", "dog", (255, 214, 140), 0.8, 0.5, 3.0, 10, 0, "wander", "plains savanna", False, 6),
    ("dog_brown", "Brown Dog", "dog", (176, 130, 100), 0.75, 0.5, 3.0, 10, 0, "wander", "forest mountain", False, 6),
    ("dog_grey", "Grey Dog", "dog", (170, 176, 190), 0.85, 0.5, 3.2, 12, 0, "wander", "tundra taiga", False, 6),
    ("tiger", "Tiger", "tiger", None, 1.25, 0.9, 4.2, 30, 7, "hostile", "jungle", False, 8),
    ("elephant", "Elephant", "elephant", None, 3.2, 1.9, 2.0, 80, 0, "wander", "savanna", False, 6),
    ("bear", "Bear", "bear", None, 1.5, 1.1, 3.0, 40, 6, "hostile", "taiga forest", False, 6),
    ("gator", "Gator", "gator", None, 0.55, 1.0, 2.4, 28, 6, "hostile", "swamp", False, 12),
    ("snake", "Snake", "snake", None, 0.45, 0.6, 1.8, 6, 3, "hostile", "swamp jungle desert", False, 10),
    ("fish", "Silver Fish", "fish", None, 0.4, 0.3, 2.0, 3, 0, "swim", "", False, 30),
    ("fish_orange", "Orange Fish", "fish", (255, 160, 80), 0.35, 0.3, 2.0, 3, 0, "swim", "jungle swamp plains", False, 16),
    ("fish_blue", "Blue Fish", "fish", (120, 180, 255), 0.4, 0.3, 2.2, 3, 0, "swim", "jungle savanna desert", False, 12),
    ("fish_green", "Green Fish", "fish", (150, 215, 130), 0.45, 0.3, 1.8, 4, 0, "swim", "swamp forest taiga", False, 12),
    ("squid", "Squid", "squid", None, 1.1, 0.7, 1.6, 10, 0, "swim", "", False, 8),
    ("worm", "Worm", "worm", None, 0.2, 0.2, 0.5, 1, 0, "wander", "plains forest swamp jungle", False, 10),
    ("ant", "Ant", "ant", None, 0.2, 0.25, 1.2, 1, 0, "wander", "desert savanna jungle badlands forest", False, 12),
    ("sheep", "Sheep", "sheep", None, 1.25, 0.9, 1.6, 8, 0, "wander", "plains mountain tundra", False, 22),
    ("sheep_black", "Black Sheep", "sheep", (88, 84, 88), 1.25, 0.9, 1.6, 8, 0, "wander", "plains mountain", False, 5),
    ("sheep_brown", "Brown Sheep", "sheep", (176, 136, 104), 1.25, 0.9, 1.6, 8, 0, "wander", "mountain taiga", False, 7),
    ("horse", "Grey Horse", "horse", None, 2.0, 1.0, 5.0, 24, 0, "wander", "plains savanna", False, 8),
    ("horse_bay", "Bay Horse", "horse", (190, 124, 84), 2.0, 1.0, 5.0, 24, 0, "wander", "plains savanna", False, 10),
    ("horse_black", "Black Horse", "horse", (92, 86, 90), 2.05, 1.0, 5.2, 24, 0, "wander", "plains mountain", False, 5),
    ("horse_palomino", "Palomino Horse", "horse", (255, 214, 150), 1.95, 1.0, 5.0, 24, 0, "wander", "savanna badlands", False, 6),
    ("frog", "Frog", "frog", None, 0.35, 0.4, 1.5, 3, 0, "wander", "swamp jungle", False, 16),
    ("duck", "Duck", "duck", None, 0.6, 0.45, 1.5, 4, 0, "wander", "swamp plains forest", False, 12),
]


def creatures_text(models):
    out = ["# Creatures and vehicle models. Generated by tools/gen_models.py; edit that, not this.",
           "#",
           "# model <id> file <path relative to this file>",
           "# entity <id> ... see base.txt for the properties.",
           "", "# --- animal models"]
    for fn in ANIMALS:
        out.append("model %s file models/%s.bbmodel" % (fn.__name__, fn.__name__))
    out += ["", "# --- vehicle models (the vehicles themselves are declared elsewhere)"]
    for fn in VEHICLES:
        out.append("model %s file models/%s.bbmodel" % (fn.__name__, fn.__name__))
    out.append("")
    for eid, name, model, tint, height, width, speed, health, damage, beh, biomes, night, weight in ENTITIES:
        assert model in models, model
        assert 0.2 <= height <= 4 and 0.2 <= width <= 2 and 0 <= speed <= 12 and 1 <= health <= 1000 and 0 <= damage <= 100
        out.append("entity " + eid)
        out.append("  name " + name)
        out.append("  model " + model)
        if tint:
            out.append("  tint %d %d %d" % tint)
        out.append("  height %g" % height)
        out.append("  width %g" % width)
        out.append("  speed %g" % speed)
        out.append("  health %d" % health)
        if beh == "hostile":
            out.append("  damage %d" % damage)
        out.append("  behavior " + beh)
        if biomes:
            out.append("  biomes " + biomes)
        if night:
            out.append("  night")
        out.append("  weight %d" % weight)
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------- validation

def validate(path):
    """Mirrors what src/bbmodel.rs and src/model.rs need from a file."""
    d = json.load(open(path))
    assert isinstance(d, dict) and isinstance(d["elements"], list) and d["elements"], "no elements"
    tex = d["textures"][0]["source"]
    assert tex.startswith("data:image/png;base64,")
    img = Image.open(io.BytesIO(base64.b64decode(tex.split("base64,", 1)[1])))
    img.load()
    assert img.mode == "RGBA" and img.width <= MAX_TEX and img.height <= MAX_TEX
    assert d["resolution"] == {"width": img.width, "height": img.height}
    ids = set()
    for e in d["elements"]:
        assert e["type"] == "cube" and e["uuid"] not in ids
        ids.add(e["uuid"])
        for key in ("from", "to", "origin", "rotation"):
            assert len(e[key]) == 3 and all(isinstance(v, (int, float)) and math.isfinite(v) for v in e[key]), key
        assert all(e["from"][a] < e["to"][a] for a in range(3)), e["name"]
        assert e["faces"], e["name"]
        for f, face in e["faces"].items():
            assert f in FACES and face["texture"] == 0
            x1, y1, x2, y2 = face["uv"]
            assert 0 <= x1 < x2 <= img.width and 0 <= y1 < y2 <= img.height, (e["name"], f)
    seen, names = [], []

    def walk(n, depth):
        assert depth < 16
        if isinstance(n, str):
            assert n in ids, "outliner names an unknown cube"
            seen.append(n)
            return
        assert isinstance(n, dict) and isinstance(n["name"], str) and len(n["origin"]) == 3 and len(n["rotation"]) == 3
        names.append(n["name"])
        for c in n["children"]:
            walk(c, depth + 1)

    for n in d["outliner"]:
        assert isinstance(n, dict), "top-level cubes would land in the root bone"
        walk(n, 0)
    assert sorted(seen) == sorted(ids), "every cube must sit in exactly one bone"
    assert len(names) == len(set(names)), "duplicate bone name"
    assert min(e["from"][1] for e in d["elements"]) == 0, "the lowest box must stand on y = 0"
    return len(d["elements"]), img.size, names


# ---------------------------------------------------------------- preview

def rot_matrix(deg):
    rx, ry, rz = (math.radians(v) for v in deg)
    sx, cx, sy, cy, sz, cz = math.sin(rx), math.cos(rx), math.sin(ry), math.cos(ry), math.sin(rz), math.cos(rz)
    return [[cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
            [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
            [-sy, cy * sx, cy * cx]]


def render(m, view, size):
    """Point-splat render of a model with a depth buffer. Returns an RGB image."""
    ya, pa = {"side": (90, 0), "front": (0, 0), "iso": (38, 24), "back": (215, 20)}[view]
    ya, pa = math.radians(ya), math.radians(pa)
    ca, sa, cp, sp = math.cos(ya), math.sin(ya), math.cos(pa), math.sin(pa)

    def proj(x, y, z):
        x1, z1 = x * ca + z * sa, -x * sa + z * ca
        return -x1, -(y * cp + z1 * sp), z1 * cp - y * sp

    pts = []
    for part in m.parts:
        r, o = rot_matrix(part.rot), part.origin
        for f, px in part.tex.items():
            tl, tr, bl = face_frame(f, part.lo, part.hi)
            h, w = len(px), len(px[0])
            shade = FACE_SHADE[f] / 255.0
            pts.append((part, r, o, tl, tr, bl, w, h, px, shade))
    lo = [min(p.lo[a] for p in m.parts) - 1 for a in range(3)]
    hi = [max(p.hi[a] for p in m.parts) + 1 for a in range(3)]
    corners = [proj(x, y, z) for x in (lo[0], hi[0]) for y in (lo[1], hi[1]) for z in (lo[2], hi[2])]
    x0, x1 = min(c[0] for c in corners), max(c[0] for c in corners)
    y0, y1 = min(c[1] for c in corners), max(c[1] for c in corners)
    scale = min((size - 8) / (x1 - x0), (size - 8) / (y1 - y0), 9.0)
    ox, oy = (size - (x1 - x0) * scale) / 2 - x0 * scale, (size - (y1 - y0) * scale) / 2 - y0 * scale
    bg = (92, 108, 124)
    img = Image.new("RGB", (size, size), bg)
    out = img.load()
    zb = [[1e9] * size for _ in range(size)]
    for part, r, o, tl, tr, bl, w, h, px, shade in pts:
        eu = [tr[a] - tl[a] for a in range(3)]
        ev = [bl[a] - tl[a] for a in range(3)]
        nu = int(math.ceil(math.sqrt(sum(v * v for v in eu)) * scale * 1.5)) + 1
        nv = int(math.ceil(math.sqrt(sum(v * v for v in ev)) * scale * 1.5)) + 1
        for b in range(nv):
            v = (b + 0.5) / nv
            row = px[min(h - 1, int(v * h))]
            for a_ in range(nu):
                u = (a_ + 0.5) / nu
                c = row[min(w - 1, int(u * w))]
                if c[3] == 0:
                    continue
                q = [tl[k] + eu[k] * u + ev[k] * v - o[k] for k in range(3)]
                x = r[0][0] * q[0] + r[0][1] * q[1] + r[0][2] * q[2] + o[0]
                y = r[1][0] * q[0] + r[1][1] * q[1] + r[1][2] * q[2] + o[1]
                z = r[2][0] * q[0] + r[2][1] * q[1] + r[2][2] * q[2] + o[2]
                sx_, sy_, d = proj(x, y, z)
                ix, iy = int(sx_ * scale + ox), int(sy_ * scale + oy)
                if 0 <= ix < size and 0 <= iy < size and d < zb[iy][ix]:
                    zb[iy][ix] = d
                    out[ix, iy] = (int(c[0] * shade), int(c[1] * shade), int(c[2] * shade))
    return img


def contact_sheet(models, path, cell=190, cols=2, views=("iso", "side", "front", "back")):
    from PIL import ImageDraw
    rows = (len(models) + cols - 1) // cols
    cw = cell * len(views)
    sheet = Image.new("RGB", (cols * cw, rows * (cell + 12)), (40, 44, 52))
    draw = ImageDraw.Draw(sheet)
    for i, m in enumerate(models):
        x, y = (i % cols) * cw, (i // cols) * (cell + 12)
        for k, v in enumerate(views):
            sheet.paste(render(m, v, cell - 2), (x + k * cell + 1, y + 12))
        draw.text((x + 3, y), "%s  (%d cubes, %dx%d)" % (m.id, len(m.parts), m.image.width, m.image.height), fill=(240, 240, 240))
    sheet.save(path)


# ---------------------------------------------------------------- main

def main(argv):
    sheet = argv[argv.index("--sheet") + 1] if "--sheet" in argv else None
    only = argv[argv.index("--only") + 1].split(",") if "--only" in argv else None
    os.makedirs(OUT_DIR, exist_ok=True)
    built = []
    for fn in ANIMALS + MORE_ANIMALS + VEHICLES:
        if only and fn.__name__ not in only:
            continue
        m = fn()
        assert m.id == fn.__name__
        path = os.path.join(OUT_DIR, m.id + ".bbmodel")
        with open(path, "w") as f:
            json.dump(m.to_json(), f, separators=(",", ":"))
            f.write("\n")
        n, size, bones = validate(path)
        lo = [min(p.lo[a] for p in m.parts) for a in range(3)]
        hi = [max(p.hi[a] for p in m.parts) for a in range(3)]
        print("%-15s %2d cubes  %3dx%-3d  size %4.1f x %4.1f x %4.1f  bones: %s" % (
            m.id, n, size[0], size[1], hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], " ".join(bones)))
        built.append(m)
    if "--creatures" in argv and not only:
        with open(CREATURES, "w") as f:
            f.write(creatures_text({m.id for m in built}))
        print("wrote", os.path.relpath(CREATURES, ROOT), "with", len(ENTITIES), "entities")
    if sheet:
        contact_sheet(built, sheet)
        print("wrote", sheet)


if __name__ == "__main__":
    main(sys.argv[1:])
