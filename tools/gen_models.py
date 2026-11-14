#!/usr/bin/env python3
"""Generates the creature and vehicle models in data/models/ (and the packs'
in mods/spellsNmagic/models/, mods/titanlands/models/, mods/landvehicles/models/
and mods/hearthgarden/models/, see OUTPUTS) and data/creatures.txt.

    python3 tools/gen_models.py                 # write the models, validate
    python3 tools/gen_models.py --creatures     # also rewrite creatures.txt (it has hand edits)
    python3 tools/gen_models.py --sheet out.png # also render a contact sheet
    python3 tools/gen_models.py --only wolf,fox # just these models

Each model is a set of boxes hung on named bones (the game animates bones by
name: `Head`, `Leg-FL`, `L-Leg`, ...). Every box face gets its own area in the
model's texture. The base colour of a texel comes from a "shader" evaluated at
the texel's 3D position (so stripes, patches and belly shading run coherently
across faces), then per-face "decals" paint the eyes, noses and so on.

Limbs are built straight; articulate() adds the knees, elbows and tail/neck
joints (`Shin-*`, `Forearm-*`, `tail-b`, `neck-b` child bones) to every model
after painting, without changing how it looks at rest.

Conventions: y is up, the front of a model faces -z, its left side is -x,
feet stand on y = 0, one model unit is one texel. Output is deterministic.
"""
import base64
import io
import json
import math
import os
import re
import sys
import uuid

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))  # the mods live outside the repository
OUT_DIR = os.path.join(ROOT, "data", "models")
PACK_DIR = os.path.join(MODS_DIR, "spellsNmagic", "models")  # the fantasy pack
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
        self.jointed = False
        self.rects = {}   # what pack() lays out: key -> texels

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

    def rename_bone(self, old, new):
        self.bones = {(new if b == old else b): (piv, new if par == old else par) for b, (piv, par) in self.bones.items()}
        for p in self.parts:
            if p.bone == old:
                p.bone = new

    def split(self, part, axis, k):
        """Cuts an already painted box in two, k texels from its low end along
        axis, sharing out its texture so that it looks the same as before.
        Returns the (low, high) pieces, which take the box's place in the model."""
        n = max(1, int(round(part.hi[axis] - part.lo[axis])))
        assert 0 < k < n, (self.id, part.name, axis, k, n)
        at = round(part.lo[axis] + k * (part.hi[axis] - part.lo[axis]) / n, 6)
        lo_t, hi_t, lo_s, hi_s, cap = {}, {}, {}, {}, None
        for f, px in part.tex.items():
            tl, tr, bl = face_frame(f, part.lo, part.hi)
            key, dx, dy = part.src[f]
            if tr[axis] != tl[axis]:     # the cut runs down this face
                asc = tr[axis] > tl[axis]
                c = k if asc else n - k
                (lo_t[f], hi_t[f]) = ([row[:c] for row in px], [row[c:] for row in px])[::1 if asc else -1]
                (lo_s[f], hi_s[f]) = ((key, dx, dy), (key, dx + c, dy))[::1 if asc else -1]
                spot = (px[len(px) // 2][c], (key, dx + c, dy + len(px) // 2))
            elif bl[axis] != tl[axis]:   # ... or across it
                asc = bl[axis] > tl[axis]
                c = k if asc else n - k
                (lo_t[f], hi_t[f]) = (px[:c], px[c:])[::1 if asc else -1]
                (lo_s[f], hi_s[f]) = ((key, dx, dy), (key, dx, dy + c))[::1 if asc else -1]
                spot = (px[c][len(px[c]) // 2], (key, dx + len(px[c]) // 2, dy + c))
            else:
                if tl[axis] == part.lo[axis]:
                    lo_t[f], lo_s[f] = px, part.src[f]
                else:
                    hi_t[f], hi_s[f] = px, part.src[f]
                continue
            # The cut ends show only when the joint bends: they borrow a texel beside the cut.
            if cap is None or (cap[0][3] < 255 and spot[0][3] == 255):
                cap = spot
        pieces = []
        for tex, src, face, end in ((lo_t, lo_s, ("east", "up", "south")[axis], "hi"), (hi_t, hi_s, ("west", "down", "north")[axis], "lo")):
            q = Part()
            q.__dict__.update(part.__dict__)
            if end == "hi":
                q.hi = tuple(at if a == axis else v for a, v in enumerate(part.hi))
            else:
                q.lo = tuple(at if a == axis else v for a, v in enumerate(part.lo))
            if cap:
                tex[face], src[face] = [[cap[0]]], cap[1]
            q.tex, q.src = tex, src
            pieces.append(q)
        i = self.parts.index(part)
        self.parts[i:i + 1] = pieces
        return pieces

    # -- texture

    def paint(self):
        for pi, part in enumerate(self.parts):
            # tex: the texels of each face. src: where they sit in the atlas, as
            # (key of a packed rectangle, x and y offset in it) - see split().
            part.tex, part.src = {}, {}
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
                part.src[f] = ((pi, f), 0, 0)
                self.rects[(pi, f)] = px

    def pack(self):
        """Shelf-packs every face (with a 1 px replicated gutter) into the smallest texture."""
        rects = [(len(px), len(px[0]), pi, f) for (pi, f), px in self.rects.items()]
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
        for key, (x0, y0) in pos.items():
            px = self.rects[key]
            h, w = len(px), len(px[0])
            for j in range(-1, h + 1):
                for i in range(-1, w + 1):
                    out[x0 + i, y0 + j] = px[min(max(j, 0), h - 1)][min(max(i, 0), w - 1)]
        for part in self.parts:
            part.uv = {}
            for f, (key, dx, dy) in part.src.items():
                x0, y0 = pos[key][0] + dx, pos[key][1] + dy
                part.uv[f] = [x0, y0, x0 + len(part.tex[f][0]), y0 + len(part.tex[f])]
        self.image = img

    # -- output

    def to_json(self):
        self.paint()
        if not self.jointed and self.id not in RIGID:
            self.jointed = True
            articulate(self)
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


# ---------------------------------------------------------------- joints
#
# Limbs are modelled as straight boxes; articulate() then gives every model
# its knees, elbows and tail joints in one place. It runs after painting, so a
# box cut in two at a joint keeps exactly the texels it had and the model looks
# the same at rest. The lower part of each limb moves to a child bone whose
# pivot is the joint: `Shin-FL` under `Leg-FL`, `Forearm-L` under `Arm-L`,
# `tail-b` under `tail-a`, `neck-b` under `neck`.

RIGID = {"boat", "cargo_boat", "cart", "chest_cart", "plane", "car", "spellbook"}  # models articulate() leaves alone
MIN_LIMB = 4.0   # legs and arms no longer than this stay in one piece
MIN_TAIL = 6.0   # and tails shorter than this
MIN_NECK = 9.0


def _same_frame(p, q):
    return (not any(p.rot) and not any(q.rot)) or (p.rot == q.rot and p.origin == q.origin)


def _unrot(pt, part):
    """pt in the unrotated frame of a (rotated) box."""
    if not any(part.rot):
        return tuple(pt)
    r, o = rot_matrix(part.rot), part.origin
    q = [pt[k] - o[k] for k in range(3)]
    return tuple(sum(r[k][i] * q[k] for k in range(3)) + o[i] for i in range(3))


def _centre(p):
    return tuple((p.lo[a] + p.hi[a]) / 2 for a in range(3))


def _span(p, main, a):
    """The extent of p along axis a of main's frame."""
    if _same_frame(p, main):
        return p.lo[a], p.hi[a]
    c = _unrot(rotp(_centre(p), p.rot, p.origin) if any(p.rot) else _centre(p), main)[a]
    half = (p.hi[a] - p.lo[a]) / 2
    return c - half, c + half


def joint(m, bone, child, explicit=None, axis=None, min_len=0.0, prefix="lower", fixed=(), cut=True):
    """Splits what hangs on `bone` in two at a joint and moves the far part to
    the new bone `child`, whose pivot is the joint. If a box whose name matches
    the `explicit` pattern is there, that box is the far segment (a modelled
    shin, forearm, ...); otherwise the longest box is cut in half way between
    the bone's pivot and the limb's far end. Returns whether it did."""
    parts = [p for p in m.parts if p.bone == bone]
    if not parts or child in m.bones:
        return False
    vol = lambda p: (p.hi[0] - p.lo[0]) * (p.hi[1] - p.lo[1]) * (p.hi[2] - p.lo[2])
    dim = lambda p, a: p.hi[a] - p.lo[a]
    ex = sorted((p for p in parts if explicit and re.match(explicit, p.name)), key=vol)
    if ex:
        main = ex[-1]
    elif not cut:
        return False
    elif axis is None:
        main = max(parts, key=lambda p: max(dim(p, a) for a in range(3)))
    else:
        main = max(parts, key=lambda p: dim(p, axis))
    if main.name.startswith(fixed):
        return False
    a = axis if axis is not None else max(range(3), key=lambda k: dim(main, k))
    span = {id(p): _span(p, main, a) for p in parts}
    mid = lambda p: sum(span[id(p)]) / 2
    mn, mx = min(v[0] for v in span.values()), max(v[1] for v in span.values())
    if mx - mn <= min_len:
        return False
    hip = min(max(_unrot(m.bones[bone][0], main)[a], mn), mx)
    root_hi = abs(hip - mx) < abs(hip - mn)   # is the limb's root at its high end?
    beyond = (lambda v, j: v <= j) if root_hi else (lambda v, j: v >= j)
    j, up = None, None
    if ex:
        uppers = [p for p in parts if p not in ex and not beyond(mid(p), mid(main))]
        if not uppers:
            return False
        up = max(uppers, key=vol)
        if not _same_frame(up, main):
            # The far segment has a rotation of its own, about the joint.
            pivot = main.origin
            lower = [p for p in parts if p is main or (any(p.rot) and _same_frame(p, main))]
        else:
            o0, o1 = max(main.lo[a], up.lo[a]), min(main.hi[a], up.hi[a])
            j = (o0 + o1) / 2 if o0 <= o1 else (main.hi[a] if root_hi else main.lo[a])
            lower = [main]
    else:
        n = max(1, int(round(dim(main, a))))
        if n < 2:
            return False
        far = mn if root_hi else mx
        root = main.hi[a] if root_hi else main.lo[a]
        want = abs(root - (hip + far) / 2)
        k = min(range(1, n), key=lambda k: (abs(k * dim(main, a) / n - want), k))   # texels from the root end
        lo, hi = m.split(main, a, n - k if root_hi else k)
        near, farp = (hi, lo) if root_hi else (lo, hi)
        farp.name = prefix if prefix.startswith(main.name) else prefix + "-" + re.sub(r"^(leg|arm)-", "", main.name)
        j = near.lo[a] if root_hi else near.hi[a]
        lower = [farp]
        up = main = near
    if j is not None:
        c = list(_centre(lower[0]))
        c[a] = j
        pivot = rotp(c, main.rot, main.origin) if any(main.rot) else tuple(round(v, 4) for v in c)
        for p in parts:
            if p is up or p in lower or not any(p is q for q in m.parts):
                continue
            n = max(1, int(round(dim(p, a))))
            if _same_frame(p, main) and n >= 2 and p.lo[a] + 0.9 < j < p.hi[a] - 0.9:
                # It runs across the joint: cut it there too (at its nearest texel edge).
                k = min(range(1, n), key=lambda k: (abs(p.lo[a] + k * dim(p, a) / n - j), k))
                lo, hi = m.split(p, a, k)
                farp = lo if root_hi else hi
                farp.name = p.name + "-b"
                lower.append(farp)
            elif beyond(mid(p), j):
                lower.append(p)
    m.bone(child, pivot, bone)
    for p in lower:
        p.bone = child
    if j is not None:
        for b, (piv, par) in list(m.bones.items()):
            if par == bone and b != child and beyond(_unrot(piv, main)[a], j) and _unrot(piv, main)[a] != j:
                m.bones[b] = (piv, child)
    return True


def articulate(m):
    """Knees, elbows, tail and neck joints for a painted model (see above)."""
    for b in list(m.bones):
        leg = re.match(r"Leg-(.+)$", b) or re.match(r"([LR])-Leg$", b)
        arm = re.match(r"Arm-(.+)$", b) or re.match(r"([LR])-Arm$", b)
        if leg:
            # A folded hind leg (a haunch sitting on a long foot) has no knee to bend.
            joint(m, b, "Shin-" + leg.group(1), r"(shin|tibia)", 1, MIN_LIMB, "shin", fixed=("haunch", "thigh"))
        elif arm:
            joint(m, b, "Forearm-" + arm.group(1), r"(forearm|fist)-[lr]$", None, MIN_LIMB, "forearm")

    # Tails. The older models have theirs as plain boxes on the body.
    if not any(b.startswith("tail") for b in m.bones):
        boxes = [p for p in m.parts if p.bone == "Body" and re.match(r"tail(-[a-c]|-fin|-tuft|-brush)?$", p.name)]
        base = [p for p in boxes if p.name in ("tail", "tail-a")]
        if base and _tail_ok(boxes, base[0], base[0].name == "tail"):
            p = base[0]
            a = max((1, 2), key=lambda k: p.hi[k] - p.lo[k])
            end = list(_centre(p))
            end[a] = p.lo[a] if a == 2 else p.hi[a]   # tails leave the body backwards, or hang from it
            m.bone("tail-a", p.origin if any(p.rot) else end)
            for q in boxes:
                q.bone = "tail-a"
    elif "tail" in m.bones and "tail-a" not in m.bones:
        boxes = [p for p in m.parts if p.bone == "tail"]
        if boxes and _tail_ok(boxes, max(boxes, key=lambda p: max(p.hi[k] - p.lo[k] for k in (1, 2))), True):
            m.rename_bone("tail", "tail-a")
    if "tail-a" in m.bones and "tail-b" not in m.bones:
        boxes = [p for p in m.parts if p.bone == "tail-a"]
        main = max(boxes, key=lambda p: max(p.hi[k] - p.lo[k] for k in (1, 2)))
        a = max((1, 2), key=lambda k: main.hi[k] - main.lo[k])
        if joint(m, "tail-a", "tail-b", r"tail-b$", a, 0.0, "tail-b"):
            joint(m, "tail-b", "tail-c", r"tail-c$", a, cut=False)

    # Long necks.
    if "neck" in m.bones and "neck-b" not in m.bones:
        boxes = [p for p in m.parts if p.bone == "neck"]
        if boxes and all(_same_frame(p, boxes[0]) for p in boxes):
            joint(m, "neck", "neck-b", r"neck-b$", 1, MIN_NECK, "neck-b")


def _tail_ok(boxes, main, single):
    """Is this tail worth a second joint? Long, thin, and all of one piece."""
    if not single:   # modelled in segments already: tail-a, tail-b, ...
        return sum(max(p.hi[k] - p.lo[k] for k in range(3)) for p in boxes) >= MIN_TAIL
    length = max(main.hi[k] - main.lo[k] for k in (1, 2))
    return length >= MIN_TAIL and main.hi[0] - main.lo[0] < length and all(_same_frame(p, main) for p in boxes)


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


# ---------------------------------------------------------------- farm animals, ground monsters, fantasy pack
#
# Shared painting helpers first. The upright monsters use `Arm-L`/`Arm-R` and
# `Leg-L`/`Leg-R`; the fantasy pack (SPELLS_N_MAGIC) is written to its own folder.

def ragged_hem(seed, shader, n):
    """Wraps a shader so the side faces lose 0..n texels off the bottom, column by column."""
    sh = shader if callable(shader) else (lambda _p, c=shader: c)

    def out(p):
        c = sh(p)
        if p.side() and p.h - 1 - p.j < int(hsh(seed, int(p.x * 1.01 + 50), int(p.z * 1.01 + 50)) * (n + 1)):
            return tuple(c[:3]) + (0,)
        return c
    return out


def shag(seed, base, dark, light=None):
    """Long hair hanging in strands."""
    def sh(p):
        k = hsh(seed, int(math.floor(p.x * 1.3 + 50)), int(math.floor(p.z * 1.3 + 50)))
        if p.face in ("up", "down"):
            return mix(base, dark, 0.4) if k > 0.6 else base
        if k > 0.66:
            return dark
        if light is not None and k < 0.2:
            return light
        return base
    return sh


def scaly(base, edge, belly=None, belly_y=-1e9):
    """Rows of scales, with paler plates underneath and below belly_y."""
    def sh(p):
        if belly is not None and (p.face == "down" or p.y < belly_y):
            return belly if int(math.floor(p.z + 100)) % 2 else mul(belly, 0.88)
        if (p.i + 2 * p.j) % 4 == 0:
            return edge
        return base if (p.i + p.j) % 2 else mul(base, 0.94)
    return sh


def membrane(x0, period, arm, finger, skin, scallop=0.3):
    """A wing plate lying flat: arm bone along the leading edge, fingers, a scalloped trailing edge."""
    def sh(p):
        u = abs(p.x) - x0
        if p.face in ("up", "down"):
            t = (u % period) / period
            if p.lz > 1 - scallop * math.sin(math.pi * t) - 0.02:
                return skin + (0,)
        if p.lz < 0.14 or p.face == "north":
            return arm
        if int(math.floor(u)) % period == 0:
            return finger
        return skin if p.face == "up" else mul(skin, 0.9)
    return sh


def spines(col, tip, along="z", period=4):
    """A thin plate of triangular spines: rooted at the bottom (along z) or at the front (along y)."""
    def sh(p):
        k = int(math.floor((p.z if along == "z" else p.y) + 100)) % period
        frac = 1 - abs(k - period // 2) / (period // 2 + 1.0)
        d = p.ly if along == "z" else p.lz
        return tuple(mix(col, tip, d)) + ((255,) if d <= frac else (0,))
    return sh


def claws_d(col):
    """Decal for a foot: claws along the front edge."""
    def d(c):
        if c.face == "north":
            for x in range(0, c.w, 2):
                c.px(x, c.h - 1, col)
        elif c.face == "up":
            for x in range(0, c.w, 2):
                c.px(x, 0, col)
    return d


def bird_legs(m, x, top, z, col, toe, th=0.6, foot=1.6):
    """Two thin legs with three-toed feet on Leg-L / Leg-R."""
    def toes(p):
        gap = p.face in ("up", "down") and p.i % 2 == 1 and (p.j < 2 if p.face == "up" else p.j > p.h - 3)
        return toe + ((0,) if gap else (255,))

    for tag, sx in (("L", -1), ("R", 1)):
        cx = x * sx
        m.bone("Leg-" + tag, (cx, top, z + th))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (cx - th, 0.5, z), (cx + th, top + 1, z + 2 * th), col)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (cx - foot, 0, z - 2.6), (cx + foot, 0.8, z + 2 * th + 0.4), toes)


# -- farm animals

def turkey():
    m = Model("turkey", 401, noise=6)
    BZ, DK, CP, W = (124, 88, 60), (60, 44, 40), (182, 122, 64), (240, 234, 220)
    RD, BL, BK, LG = (206, 48, 44), (126, 164, 198), (226, 196, 120), (196, 150, 124)

    def feathers(p):
        if p.face == "down":
            return mul(BZ, 0.8)
        if (p.i + 2 * p.j) % 4 == 0:
            return DK
        return CP if (p.i + p.j) % 5 == 0 else BZ

    def wing(p):
        if p.j >= p.h - 3:
            return W if p.i % 2 else DK
        return DK if (p.i + 2 * p.j) % 4 == 0 else CP if p.j == 0 else BZ

    def fan(p):
        r = math.hypot(p.x, p.y - 9.0)
        if r > 12.1:
            return DK + (0,)
        k = int(math.floor(math.atan2(p.y - 9.0, p.x) / (math.pi / 15) + 100))
        if r > 10.6:
            return W
        if r > 9.0:
            return DK
        if r > 4.5:
            return (CP if k % 2 else BZ) if int(r) % 3 else DK
        return DK

    def neck(p):
        if p.ly > 0.55:
            return BL
        return RD if hsh(401, p.i, p.j, len(p.face)) > 0.3 else mul(RD, 0.8)

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK, WHITE)
        if c.face in ("east", "west"):
            c.px(c.fx(3), 0, RD), c.px(c.fx(2), 3, RD)

    m.box("body", "Body", (-4.5, 5, -5.5), (4.5, 13.5, 5), feathers)
    m.box("breast", "Body", (-3.5, 6, -7), (3.5, 12.5, -5), feathers,
          lambda c: c.face == "north" and c.rect(3, 3, 1, 3, (40, 32, 30)))
    m.bone("tail", (0, 10, 5))
    m.box("fan", "tail", (-10, 9, 4.6), (10, 21, 5.6), fan, rot=(10, 0, 0), origin=(0, 9, 5))
    m.box("rump", "tail", (-3, 9, 4.5), (3, 12.5, 6.4), feathers)
    wings(m, (-4.5, 12, -2), (-5.6, 6, -4), (-4.3, 12.5, 4), wing)
    m.bone("Head", (0, 12.5, -5.5))
    m.box("neck", "Head", (-1.3, 12, -7.5), (1.3, 18, -5), neck)
    m.box("head", "Head", (-1.8, 17, -9), (1.8, 20.5, -5), BL, head_d)
    m.box("beak", "Head", (-0.7, 18, -10.8), (0.7, 19, -8.8), BK)
    m.box("snood", "Head", (0.1, 16.4, -10.4), (0.9, 19.3, -9.6), RD)
    m.box("wattle", "Head", (-1, 13.4, -8.7), (1, 17.2, -7.4), lambda p: RD if (p.i + p.j) % 3 else mul(RD, 0.78))
    bird_legs(m, 2.2, 5, 0, LG, mul(LG, 0.9))
    return m


def goose():
    m = Model("goose", 402, noise=5)
    W, SH, GY, OR, DO = (246, 244, 238), (222, 220, 212), (200, 198, 192), (238, 142, 44), (204, 108, 36)

    def body(p):
        if p.face == "down":
            return W
        if p.side() and (int(p.z + 50) + int(p.y + 50) * 2) % 4 == 0:
            return SH
        if p.face == "up" and abs(p.x) < 1.5 and p.lz > 0.3:
            return SH
        return W

    def wing(p):
        if p.j >= p.h - 2:
            return GY if p.i % 2 else mul(GY, 0.82)
        return SH if (p.i + 2 * p.j) % 4 == 0 else mul(W, 0.96)

    def neck(p):
        return SH if int(p.y) % 3 == 0 and p.face != "north" else W

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK)
        if c.face in ("east", "west"):
            c.px(c.fx(1), 0, (120, 170, 220))

    def bill_d(c):
        if c.face == "up":
            c.sym(0, 2, (120, 70, 30))
        elif c.face in ("east", "west", "north"):
            c.rect(0, c.h - 1, c.w, 1, DO)
        if c.face == "north":
            c.rect(1, 0, 1, 1, (250, 220, 190))

    m.box("body", "Body", (-4.2, 4.5, -7), (4.2, 12.5, 7), body)
    m.box("breast", "Body", (-3.5, 5.5, -8.6), (3.5, 12, -6.6), body)
    m.bone("tail", (0, 10.5, 7))
    m.box("tail", "tail", (-2.5, 9.5, 6.5), (2.5, 11.5, 10.5), lambda p: SH if p.i % 2 == 0 else W, rot=(-22, 0, 0), origin=(0, 10.5, 7))
    wings(m, (-4.2, 11.5, -3), (-5.2, 6, -5), (-4, 12, 6), wing)
    NP, NR = (0, 11.5, -6.5), (-9, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-1.4, 11, -8), (1.4, 23, -5.2), neck, rot=NR, origin=NP)
    m.bone("Head", rotp((0, 22.5, -6.6), NR, NP), "neck")
    m.box("head", "Head", (-2, 21.5, -11.2), (2, 25.6, -6), W, head_d)
    m.box("bill", "Head", (-1.5, 21.8, -14.6), (1.5, 23.4, -10.8), OR, bill_d)
    m.box("knob", "Head", (-1, 23.2, -12.2), (1, 24.6, -10.8), DO)
    bird_legs(m, 2.3, 4.8, 0.8, OR, OR, foot=1.8)
    return m


def donkey():
    m = Model("donkey", 403)
    G, D, W, MN, HF, PK = (170, 162, 154), (124, 116, 110), (240, 236, 226), (62, 56, 54), (48, 42, 40), (206, 150, 144)

    def coat(p):
        if p.face == "down" or p.y < 12.6:
            return W
        if p.face == "up" and abs(p.x) < 0.8:
            return MN
        if abs(p.z + 5.5) < 0.8 and p.y > 15.5:
            return MN
        return D if vnoise(403, p.x / 4.0, p.y / 4.0, p.z / 4.0) > 0.64 else G

    def leg(p):
        if p.y < 1.6:
            return HF
        if p.y < 7.5 and int(p.y) % 2 == 0 and p.y > 3:
            return D
        return G if p.y > 2.5 else D

    def head(p):
        if p.face == "down":
            return W
        return G

    def head_d(c):
        c.side_eye(2, 2, BLACK, WHITE)
        if c.face == "north":
            c.sym(0, 2, BLACK)
            c.symrect(0, 1, 1, 1, W)
        if c.face in ("east", "west"):
            c.px(c.fx(1), 1, W), c.px(c.fx(2), 1, W), c.px(c.fx(3), 2, W)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(0, 1, (70, 54, 54))
            c.rect(0, c.h - 1, c.w, 1, (150, 130, 124))

    def ear(p):
        if p.face == "north" and 0 < p.j < p.h - 1:
            return PK
        return MN if p.j == 0 else G

    def hair(p):
        return MN if (p.i + p.j // 2) % 3 else mul(MN, 0.7)

    NP, NR = (0, 19, -8.5), (-35, 0, 0)
    m.box("body", "Body", (-4.5, 11, -9.5), (4.5, 20.5, 9.5), coat)
    m.bone("tail", (0, 19.5, 9.5))
    m.box("tail", "tail", (-0.6, 11, 9.2), (0.6, 20, 10.4), G, rot=(-10, 0, 0), origin=(0, 19.5, 9.5))
    m.box("tail-tuft", "tail", (-1.1, 7.5, 9), (1.1, 12, 10.8), hair, rot=(-10, 0, 0), origin=(0, 19.5, 9.5))
    m.bone("Head", NP)
    m.box("neck", "Head", (-2.5, 16.5, -13.5), (2.5, 27, -7.5), lambda p: W if p.face == "north" and p.ly < 0.5 else G, rot=NR, origin=NP)
    m.box("mane", "Head", (-0.8, 19, -7.9), (0.8, 28, -6.5), hair, rot=NR, origin=NP)
    m.box("head", "Head", (-2.8, 21, -21.5), (2.8, 27.5, -12.5), head, head_d)
    m.box("muzzle", "Head", (-2.3, 21, -24.5), (2.3, 25, -21), W, muzzle_d)
    ears(m, (-2.7, 27, -15), (-1.1, 35, -13.8), ear, rot=(8, 0, 10))
    four_legs(m, -4.5, -2, -9, 6.5, 2.5, 11, leg)
    return m


def llama():
    m = Model("llama", 404, noise=9)
    W, SH, BR, F, HF, PK = (238, 228, 208), (208, 194, 168), (156, 108, 72), (226, 212, 190), (70, 58, 52), (214, 160, 150)

    def wool(p):
        if vnoise(404, p.x / 6.0 + 4, p.y / 6.0, p.z / 6.0) > 0.6:
            return BR if vnoise(404, p.x / 1.7, p.y / 1.7, p.z / 1.7) < 0.62 else mul(BR, 0.82)
        n = vnoise(404, p.x / 1.7, p.y / 1.7, p.z / 1.7)
        c = SH if n > 0.6 else W
        if (int(p.x + 100) + int(p.y + 100) + int(p.z + 100)) % 5 == 0 and n > 0.4:
            c = mul(SH, 0.93)
        return c

    def leg(p):
        if p.y < 1.5:
            return HF
        return wool(p) if p.y > 7 else F

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(2, 1, BLACK, WHITE)
        if c.face in ("east", "west"):
            c.px(c.fx(2), 0, BR), c.px(c.fx(3), 0, BR)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 0, (90, 66, 62))
            c.rect(c.w // 2 - 1, 1, 2, 1, (150, 110, 104))
            c.rect(1, 2, c.w - 2, 1, (120, 86, 80))
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 2, (120, 86, 80)), c.px(c.fx(1), 2, (120, 86, 80))

    m.box("body", "Body", (-5, 13, -8.5), (5, 24, 8.5), wool)
    m.bone("tail", (0, 22, 8.5))
    m.box("tail", "tail", (-1.5, 18, 8), (1.5, 23, 10.5), wool)
    NP, NR = (0, 21, -7), (-6, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2.5, 19, -10.5), (2.5, 35, -5.5), wool, rot=NR, origin=NP)
    m.bone("Head", rotp((0, 34, -8), NR, NP), "neck")
    m.box("head", "Head", (-2.6, 32.5, -14.5), (2.6, 38, -7.5), lambda p: mul(F, 0.9) if p.face == "down" else F, head_d)
    m.box("topknot", "Head", (-2.2, 37.5, -13), (2.2, 39.2, -8.8), wool)
    m.box("muzzle", "Head", (-1.8, 32.5, -17), (1.8, 35.6, -14), mix(F, W, 0.6), muzzle_d)
    ears(m, (-2.5, 37.6, -9.6), (-0.9, 42, -8.6), pointed(BR, PK), rot=(0, 0, 7))
    four_legs(m, -4.8, -2.2, -8, 5.4, 2.6, 13, leg)
    return m


def bison():
    m = Model("bison", 405, noise=8)
    DB, VD, BR, LB, HN, HF, NB = (96, 66, 48), (60, 42, 34), (146, 104, 70), (176, 134, 94), (60, 54, 50), (44, 38, 36), (40, 32, 32)
    mane = shag(405, DB, VD)

    def rear(p):
        if p.face == "down":
            return mul(BR, 0.8)
        return LB if vnoise(405, p.x / 3.0, p.y / 3.0, p.z / 3.0) > 0.64 else BR

    def leg(p):
        if p.y < 1.6:
            return HF
        if p.z < 0 and p.y > 4:
            return mane(p)
        return mul(BR, 0.8) if p.y < 5 else rear(p)

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 4, BLACK, (230, 210, 180))
        c.side_eye(2, 4, BLACK, (230, 210, 180))

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (120, 90, 86))
            c.rect(1, c.h - 1, c.w - 2, 1, (110, 76, 72))

    m.box("rear", "Body", (-5.5, 11, -1), (5.5, 22, 11), rear)
    m.box("hump", "Body", (-7, 11, -11), (7, 27.5, 0), ragged_hem(405, mane, 2))
    m.box("hump-top", "Body", (-4.5, 27, -9.5), (4.5, 29, -2.5), mane)
    m.bone("tail", (0, 20, 11))
    m.box("tail", "tail", (-0.6, 12, 10.6), (0.6, 20.5, 11.8), BR, rot=(-8, 0, 0), origin=(0, 20, 11))
    m.box("tail-tuft", "tail", (-1, 9, 10.4), (1, 13, 12), VD, rot=(-8, 0, 0), origin=(0, 20, 11))
    m.bone("Head", (0, 18, -10.5))
    m.box("head", "Head", (-4.5, 10, -18), (4.5, 20, -10), lambda p: VD if p.face == "down" else DB, head_d)
    m.box("crown", "Head", (-5, 18.5, -17), (5, 22.5, -10.5), ragged_hem(406, mane, 1))
    m.box("muzzle", "Head", (-3, 10, -20), (3, 14.5, -17.5), NB, muzzle_d)
    m.box("beard", "Head", (-1.5, 5, -17.5), (1.5, 10.5, -13.5), ragged_hem(407, mane, 3), skip=("down",))
    m.pair("horn-a", "Head", (-7.2, 17.6, -14.6), (-4.3, 19.4, -12.8), HN)
    m.pair("horn-b", "Head", (-7.4, 19, -14.5), (-5.8, 22.6, -12.9), lambda p: (214, 204, 180) if p.ly > 0.6 else HN)
    ears(m, (-6.6, 15, -12.2), (-4.3, 16.6, -11.2), VD, pivot=(-4.5, 15.8, -11.7))
    four_legs(m, -6, -2.8, -9.5, 7, 3.2, 11, leg)
    return m


def yak():
    m = Model("yak", 406, noise=8)
    K, VD, LK, GY, HN, HF = (78, 64, 58), (48, 40, 38), (112, 94, 82), (200, 192, 182), (226, 216, 190), (40, 34, 34)
    hair = shag(406, K, VD, LK)

    def leg(p):
        return HF if p.y < 1.6 else hair(p)

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 4)
        c.side_eye(2, 4, BLACK, WHITE)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (70, 56, 58))
            c.rect(1, c.h - 1, c.w - 2, 1, (120, 104, 100))

    def horn(p):
        return (50, 42, 40) if p.part.startswith("horn-b") and p.ly > 0.6 else HN

    m.box("body", "Body", (-6, 11, -10), (6, 22.5, 10), hair)
    m.box("hump", "Body", (-5, 22, -9.5), (5, 25, -2), hair)
    m.pair("skirt", "Body", (-7, 3, -9.5), (-5.9, 15, 9.5), ragged_hem(406, hair, 4), skip=("up", "down"))
    m.box("skirt-f", "Body", (-6, 4, -11), (6, 14, -9.9), ragged_hem(408, hair, 4), skip=("up", "down", "south"))
    m.box("skirt-b", "Body", (-6, 3, 9.9), (6, 14, 11), ragged_hem(409, hair, 4), skip=("up", "down", "north"))
    m.bone("tail", (0, 21, 10.5))
    m.box("tail", "tail", (-2, 5, 10.6), (2, 21.5, 13.4), ragged_hem(410, shag(410, LK, K, (150, 130, 116)), 3), rot=(-6, 0, 0), origin=(0, 21, 10.5), skip=("down",))
    m.bone("Head", (0, 18, -10))
    m.box("head", "Head", (-4, 11, -17.5), (4, 20.5, -9.5), lambda p: VD if p.face == "down" else K, head_d)
    m.box("forelock", "Head", (-4.4, 18.5, -18.1), (4.4, 22, -11), ragged_hem(411, hair, 2))
    m.box("muzzle", "Head", (-2.8, 11, -19.6), (2.8, 15, -17), GY, muzzle_d)
    m.pair("horn-a", "Head", (-9.5, 19.6, -13.6), (-3.8, 21.2, -12), horn)
    m.pair("horn-b", "Head", (-9.7, 20.8, -13.5), (-8.2, 27, -12.1), horn, rot=(0, 0, 8), origin=(-9, 21, -12.8))
    ears(m, (-6.6, 16.5, -12), (-3.8, 18, -11), VD, pivot=(-4, 17.2, -11.5))
    four_legs(m, -5.5, -2.5, -9, 6, 3, 11, leg)
    return m


def camel():
    m = Model("camel", 407)
    S, D, L, DK, HF = (216, 178, 122), (178, 140, 92), (238, 210, 162), (120, 88, 60), (90, 70, 56)
    coat = fur(407, S, D, L, 18.2, 4.0)

    def hump(p):
        if p.face == "up" or p.ly > 0.7:
            return DK if hsh(407, p.i, p.j, len(p.face)) > 0.45 else D
        return coat(p)

    def leg(p):
        if p.y < 1.8:
            return HF
        if 8 < p.y < 10:
            return D
        return S if p.y > 3 else L

    def neck(p):
        if p.face == "north" and hsh(407, p.i, p.j) > 0.4:
            return D
        return S

    def head_d(c):
        c.side_eye(2, 1, BLACK, None)
        if c.face in ("east", "west"):
            c.px(c.fx(1), 0, D), c.px(c.fx(2), 0, D), c.px(c.fx(3), 0, D)
        if c.face == "north":
            c.sym(0, 1, BLACK)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 0, (110, 80, 66))
            c.rect(c.w // 2 - 1, 1, 2, 1, (150, 112, 90))
            c.rect(0, c.h - 1, c.w, 1, (130, 96, 76))
        elif c.face in ("east", "west"):
            for k in range(3):
                c.px(c.fx(k), c.h - 1, (130, 96, 76))

    m.box("body", "Body", (-5, 17, -10), (5, 27, 10), coat)
    for tag, z in (("f", -7.5), ("b", 1)):
        m.box("hump-" + tag, "Body", (-3.5, 26.5, z), (3.5, 32, z + 6.5), hump)
        m.box("hump-top-" + tag, "Body", (-2.4, 31.6, z + 1.2), (2.4, 34, z + 5.3), hump)
    m.bone("tail", (0, 25, 10))
    m.box("tail", "tail", (-0.6, 15, 9.7), (0.6, 25.5, 10.9), lambda p: DK if p.ly < 0.25 else S, rot=(-8, 0, 0), origin=(0, 25, 10))
    NP = (0, 22, -9.5)
    m.bone("neck", NP)
    m.box("neck-a", "neck", (-2.5, 18.5, -15.5), (2.5, 25, -9), neck)
    m.box("neck-b", "neck", (-2.2, 22, -18), (2.2, 37, -13.5), neck)
    m.bone("Head", (0, 36, -15.5), "neck")
    m.box("head", "Head", (-2.6, 34.5, -22), (2.6, 39.5, -14), lambda p: L if p.face == "down" else S, head_d)
    m.box("muzzle", "Head", (-2.2, 34, -25), (2.2, 37.6, -21.5), mix(S, L, 0.5), muzzle_d)
    ears(m, (-3.4, 38.6, -15.6), (-1.8, 40.4, -14.6), D)
    four_legs(m, -4.6, -2.2, -9.5, 7, 2.6, 17, leg)
    return m


def bull():
    m = Model("bull", 408)
    K, VD, LB, HN, HF, PK, GD = (74, 56, 50), (44, 34, 34), (118, 90, 72), (232, 222, 196), (40, 34, 34), (196, 150, 142), (238, 198, 72)

    def hide(p):
        if p.face == "down":
            return VD
        n = vnoise(408, p.x / 4.0, p.y / 4.0, p.z / 4.0)
        return LB if n > 0.68 else VD if n < 0.3 else K

    def leg(p):
        return HF if p.y < 1.8 else VD if p.y < 5 else hide(p)

    def head(p):
        if p.face in ("north", "up") and abs(p.x) < 1:
            return LB
        return hide(p)

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 3, (120, 20, 20), WHITE)
            c.symrect(1, 2, 2, 1, VD)
        c.side_eye(1, 3, BLACK, WHITE)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (110, 70, 72))
            c.rect(1, c.h - 1, c.w - 2, 1, (150, 104, 104))

    def ring(p):
        hole = 0 < p.i < p.w - 1 and p.j < p.h - 1
        return GD + ((0,) if hole else (255,))

    def horn(p):
        return (60, 50, 46) if p.part.startswith("horn-c") and p.ly > 0.5 else HN

    m.box("body", "Body", (-6.5, 10, -3), (6.5, 22, 11), hide)
    m.box("shoulders", "Body", (-7.5, 10.5, -11), (7.5, 24.5, -2), hide)
    m.box("dewlap", "Body", (-1.5, 7.5, -11.5), (1.5, 11, -4), lambda p: mix(K, LB, 0.5))
    m.bone("tail", (0, 21, 11))
    m.box("tail", "tail", (-0.6, 10, 10.6), (0.6, 21.5, 11.8), K, rot=(-8, 0, 0), origin=(0, 21, 11))
    m.box("tail-tuft", "tail", (-1.1, 7, 10.3), (1.1, 11, 12.1), VD, rot=(-8, 0, 0), origin=(0, 21, 11))
    m.bone("Head", (0, 19, -11))
    m.box("head", "Head", (-4.5, 13, -18.5), (4.5, 22.5, -10.5), head, head_d)
    m.box("muzzle", "Head", (-3.2, 13, -21), (3.2, 17.5, -18), PK, muzzle_d)
    m.box("ring", "Head", (-1.5, 11.2, -21.5), (1.5, 14.2, -20.9), ring, skip=("up", "down", "east", "west"), noise=0)
    m.pair("horn-a", "Head", (-8, 21, -15.5), (-4.2, 23, -13.5), horn)
    m.pair("horn-b", "Head", (-8.3, 21.2, -19.5), (-6.5, 22.9, -15), horn)
    m.pair("horn-c", "Head", (-8.2, 22.4, -19.5), (-6.7, 25.5, -18), horn)
    ears(m, (-7.2, 17.3, -13.6), (-4.2, 19.2, -12.5), lambda p: PK if p.face == "north" and 0.2 < p.lx < 0.8 else K,
         pivot=(-4.4, 18.3, -13))
    four_legs(m, -6.5, -3.3, -10, 7.5, 3.2, 10, leg)
    return m


def rabbit_lop():
    m = Model("rabbit_lop", 409)
    C, BR, DB, W, PK = (238, 224, 200), (166, 120, 82), (126, 88, 60), (250, 246, 238), (236, 168, 166)

    def coat(p):
        if p.face == "down" or p.y < 3:
            return W
        if vnoise(409, p.x / 3.0 + 9, p.y / 3.0, p.z / 3.0) > 0.58:
            return BR
        return C

    def head(p):
        if p.face == "down":
            return W
        if abs(p.x) > 1.2 and p.y > 8.2 and p.face != "south":
            return BR
        return C

    def head_d(c):
        if c.face == "north":
            c.rect(1, 3, c.w - 2, 2, W)
            c.px(c.w // 2, 2, PK)
            c.px(c.w // 2, 3, (150, 100, 100))
            c.px(c.w // 2, 4, WHITE)
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK, None)

    def ear(p):
        if p.face == ("west" if p.x < 0 else "east") and 0 < p.i < p.w - 1 and p.j > 0:
            return BR if p.j < p.h - 1 else DB
        if p.face == ("east" if p.x < 0 else "west"):
            return PK
        return DB

    m.box("body", "Body", (-3.2, 2, -4), (3.2, 8.4, 2), coat)
    m.box("rump", "Body", (-3.8, 1.6, 0), (3.8, 9.4, 5), coat)
    m.bone("tail", (0, 5.5, 5))
    m.box("tail", "tail", (-1.5, 4, 4.5), (1.5, 7, 6.5), W)
    m.bone("Head", (0, 7.5, -4))
    m.box("head", "Head", (-2.7, 6, -8.8), (2.7, 11.2, -3.5), head, head_d)
    m.box("crown", "Head", (-2.4, 10.8, -7), (2.4, 11.9, -4.2), BR)
    ears(m, (-4, 3.6, -6.8), (-2.8, 11, -4.2), ear, rot=(0, 0, -8), pivot=(-2.9, 10.8, -5.5))
    m.bone("Leg-FL", (-1.7, 3, -3.2))
    m.bone("Leg-FR", (1.7, 3, -3.2))
    m.pair("leg-f", ("Leg-FL", "Leg-FR"), (-2.4, 0, -4), (-1, 3.5, -2.5), lambda p: W if p.y < 1.2 else C)
    m.bone("Leg-BL", (-3.5, 4.5, 2.5))
    m.bone("Leg-BR", (3.5, 4.5, 2.5))
    m.pair("haunch", ("Leg-BL", "Leg-BR"), (-4.7, 1, 0.2), (-2.8, 6.8, 4.8), coat)
    m.pair("foot", ("Leg-BL", "Leg-BR"), (-4.7, 0, -2.6), (-2.8, 1.3, 3.6), lambda p: W if p.lz < 0.35 or p.face == "down" else C)
    return m


def cow_highland():
    m = Model("cow_highland", 410, noise=8)
    G, D, L, HN, HF, PK = (198, 122, 64), (150, 86, 46), (228, 164, 100), (236, 226, 200), (60, 50, 46), (214, 160, 140)
    hair = shag(410, G, D, L)

    def leg(p):
        return HF if p.y < 1.8 else hair(p)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(1, 1, (140, 90, 86))
            c.rect(1, 3, c.w - 2, 1, (160, 104, 100))

    def horn(p):
        return (70, 58, 50) if p.part.startswith("horn-b") and p.ly > 0.65 else HN

    m.box("body", "Body", (-6, 10, -9), (6, 21.5, 10), hair)
    m.pair("skirt", "Body", (-6.9, 5, -8.5), (-5.9, 15, 9.5), ragged_hem(410, hair, 4), skip=("up", "down"))
    m.box("chest-hair", "Body", (-5, 6, -10), (5, 14, -8.9), ragged_hem(412, hair, 3), skip=("up", "down", "south"))
    m.bone("tail", (0, 20, 10))
    m.box("tail", "tail", (-0.6, 10, 9.6), (0.6, 20.5, 10.8), G, rot=(-8, 0, 0), origin=(0, 20, 10))
    m.box("tail-tuft", "tail", (-1.4, 5, 9.2), (1.4, 11, 11.2), ragged_hem(413, shag(413, L, G), 2), rot=(-8, 0, 0), origin=(0, 20, 10), skip=("down",))
    m.bone("Head", (0, 18, -9))
    m.box("head", "Head", (-4, 14, -16), (4, 22, -8.5), hair,
          lambda c: c.side_eye(1, 4, BLACK, None))
    m.box("fringe", "Head", (-4.5, 17, -16.8), (4.5, 23.2, -11.5), ragged_hem(414, shag(414, L, G, (244, 196, 136)), 3), skip=("down",))
    m.box("muzzle", "Head", (-3, 14, -18), (3, 18, -15.5), PK, muzzle_d)
    m.pair("horn-a", "Head", (-11, 20.8, -13.5), (-3.8, 22.4, -12), horn)
    m.pair("horn-b", "Head", (-11.2, 22, -13.4), (-9.7, 28, -12.1), horn, rot=(0, 0, 14), origin=(-10.4, 22, -12.75))
    ears(m, (-7, 17.5, -11.5), (-3.8, 19.2, -10.5), D, pivot=(-4, 18.4, -11))
    four_legs(m, -6, -3, -8, 6, 3, 10, leg)
    return m


def sheep_merino():
    m = Model("sheep_merino", 411, noise=8)
    W, SH, F, HF = (250, 248, 242), (228, 226, 218), (240, 230, 214), (120, 104, 96)

    def wool(p):
        n = vnoise(411, p.x / 1.9, p.y / 1.9, p.z / 1.9)
        c = SH if n > 0.62 else W
        if (int(p.x + 100) + int(p.y + 100) + int(p.z + 100)) % 5 == 0 and n > 0.4:
            c = mul(SH, 0.95)
        return c

    def fold(p):
        # deep rolls of wool around the neck
        if p.side() and p.j == p.h - 1:
            return mul(SH, 0.9)
        return wool(p)

    def leg(p):
        if p.y < 1.5:
            return HF
        return wool(p) if p.y > 2.6 else F

    def head_d(c):
        if c.face == "north":
            c.eyes(0, 3)
            c.rect(2, 5, 2, 1, (200, 150, 140))
            c.px(2, 6, (170, 124, 116)), c.px(3, 6, (170, 124, 116))
        c.side_eye(1, 3)

    m.box("body", "Body", (-7, 7.5, -10), (7, 22.5, 10), wool)
    m.box("back-roll", "Body", (-5, 22, -8), (5, 24, 8), wool)
    m.box("collar-a", "Body", (-6, 8.5, -12), (6, 21.5, -9.5), fold)
    m.box("collar-b", "Body", (-4.8, 10.5, -13.6), (4.8, 20.5, -11.6), fold)
    m.bone("tail", (0, 17, 10))
    m.box("tail", "tail", (-2, 11, 9.5), (2, 18, 12), wool)
    m.bone("Head", (0, 17.5, -12))
    m.box("head", "Head", (-3, 13.5, -19), (3, 20, -13), lambda p: mul(F, 0.94) if p.face == "down" else F, head_d)
    m.box("cap", "Head", (-3.8, 18.5, -17.6), (3.8, 23, -12.5), wool)
    m.pair("cheek", "Head", (-4, 13.8, -16), (-2.8, 19, -12.8), wool)
    ears(m, (-6, 16.2, -15.4), (-3.6, 17.8, -14.2), lambda p: (232, 190, 180) if p.face == "north" and 0 < p.i < p.w - 1 else F,
         rot=(0, 0, 14), pivot=(-3.8, 17.2, -14.8))
    four_legs(m, -6.2, -3, -9, 6, 3.2, 7.5, leg)
    return m


FARM = [turkey, goose, donkey, llama, bison, yak, camel, bull, rabbit_lop, cow_highland, sheep_merino]


# -- ground monsters: upright, on Arm-L / Arm-R and Leg-L / Leg-R

def skeleton():
    m = Model("skeleton", 421, noise=5)
    BN, SH, DK, EY = (236, 230, 210), (186, 176, 154), (54, 48, 52), (255, 96, 60)

    def bone(p):
        return SH if p.face == "down" or hsh(421, int(p.x * 2 + 99), int(p.y * 2), int(p.z * 2 + 99)) > 0.84 else BN

    def long_bone(p):
        return BN if p.ly < 0.14 or p.ly > 0.86 else SH if p.face in ("east", "west", "south") else mul(BN, 0.95)

    def spine(p):
        return SH if int(math.floor(p.y * 1.0 + 100)) % 2 else BN

    def ribs(p):
        rib = int(math.floor(p.y + 100)) % 2 == 0
        if p.face == "north":
            on = (rib and not (p.ly < 0.3 and abs(p.x) < 1.6)) or (abs(p.x) < 0.8 and p.ly > 0.3)
        elif p.face == "south":
            on = rib or abs(p.x) < 0.8
        elif p.face in ("east", "west"):
            on = rib
        else:
            on = p.face == "up" and min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0
        return bone(p) + ((255,) if on else (0,))

    def pelvis(p):
        if p.face in ("north", "south") and 0.15 < p.ly < 0.7 and 0.9 < abs(p.x) < 2.2:
            return DK + (0,)
        return bone(p)

    def skull_d(c):
        if c.face == "north":
            c.symrect(1, 2, 2, 2, DK)
            c.sym(1, 3, EY)
            c.px(2, 4, DK), c.px(3, 4, DK)
            for x in range(1, c.w - 1):
                c.px(x, c.h - 1, DK if x % 2 else WHITE)
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 2, DK), c.px(c.fx(0), 3, DK)
            c.px(c.fx(2), 3, SH), c.px(c.fx(3), 4, SH)
            for k in range(0, 3):
                c.px(c.fx(k), c.h - 1, DK if k % 2 else WHITE)

    def jaw(p):
        teeth = p.face in ("east", "west", "north") and p.j == 0 and p.i % 2 == 1
        return DK if teeth else SH if p.face == "down" else BN

    def hand(p):
        gap = p.side() and p.j >= p.h - 2 and p.i % 2 == 1
        return BN + ((0,) if gap else (255,))

    m.box("pelvis", "Body", (-3, 11.5, -1.5), (3, 14.5, 1.5), pelvis)
    m.box("spine", "Body", (-0.8, 14, 0), (0.8, 24, 1.4), spine)
    m.box("ribcage", "Body", (-3.5, 16.5, -2.2), (3.5, 22.5, 1.8), ribs)
    m.box("collar", "Body", (-5.2, 22.2, -1), (5.2, 23.4, 1), bone)
    m.bone("Head", (0, 24.5, 0.5))
    m.box("neck", "Head", (-0.7, 23, -0.2), (0.7, 25.5, 1.2), spine)
    m.box("skull", "Head", (-3, 25, -3.4), (3, 31, 2.8), bone, skull_d)
    m.bone("jaw", (0, 25, 1), "Head")
    m.box("jaw", "jaw", (-2.5, 23.6, -3.1), (2.5, 25, 0.6), jaw)
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 5.7 * sx, 1.9 * sx
        m.bone("Arm-" + tag, (ax, 22.8, 0))
        m.box("humerus-" + tag.lower(), "Arm-" + tag, (ax - 0.7, 16, -0.7), (ax + 0.7, 23.2, 0.7), long_bone)
        m.box("forearm-" + tag.lower(), "Arm-" + tag, (ax - 0.6, 10, -0.9), (ax + 0.6, 16.4, 0.3), long_bone)
        m.box("hand-" + tag.lower(), "Arm-" + tag, (ax - 0.5, 7.4, -1.5), (ax + 0.5, 10.4, 0.5), hand)
        m.bone("Leg-" + tag, (lx, 12, 0))
        m.box("femur-" + tag.lower(), "Leg-" + tag, (lx - 0.8, 6, -0.8), (lx + 0.8, 12.5, 0.8), long_bone)
        m.box("tibia-" + tag.lower(), "Leg-" + tag, (lx - 0.65, 0.8, -0.5), (lx + 0.65, 6.4, 0.8), long_bone)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 1, 0, -2.8), (lx + 1, 1, 1), bone, claws_d(SH))
    return m


def werewolf():
    m = Model("werewolf", 422)
    F, D, L, NB, EY, CL = (116, 104, 100), (72, 64, 66), (178, 162, 144), (30, 26, 28), (250, 200, 50), (232, 226, 206)
    coat = fur(422, F, D, None, scale=2.2, thr=0.58)

    def chest(p):
        if p.face == "north" and abs(p.x) < 3:
            return L if (p.i + p.j) % 4 else mix(L, F, 0.5)
        if p.face == "south" or p.face == "up":
            return D if (p.i + p.j) % 3 else F
        return coat(p)

    def head(p):
        if p.face == "down":
            return L
        if p.face == "up":
            return D if abs(p.x) < 2 else F
        return F

    def head_d(c):
        if c.face == "north":
            c.sym(1, 2, EY), c.sym(2, 2, (255, 240, 150))
            c.sym(0, 1, D), c.sym(1, 1, D), c.sym(2, 1, D)
            c.symrect(0, 4, 2, 2, L)
        c.side_eye(1, 2, EY)
        if c.face in ("east", "west"):
            c.px(c.fx(0), 1, D), c.px(c.fx(1), 1, D)

    def snout_d(c):
        nose_d(NB, (70, 30, 34))(c)
        if c.face in ("east", "west"):
            c.px(c.fx(0), c.h - 1, WHITE), c.px(c.fx(3), c.h - 1, WHITE)
        elif c.face == "north":
            c.sym(0, c.h - 1, WHITE)

    def jaw(p):
        if p.face in ("east", "west", "north") and p.j == 0 and p.i % 2 == 0:
            return WHITE
        return (120, 44, 50) if p.face == "up" else D

    def arm(p):
        return coat(p)

    def hand(p):
        if p.side() and p.j >= p.h - 2:
            return CL + ((255,) if p.i % 2 == 0 else (0,))
        return D

    def tail(p):
        return D if p.face == "up" or p.lz > 0.8 else L if p.face == "down" else F

    CP, CR = (0, 14, 2), (-22, 0, 0)
    m.box("hips", "Body", (-3.8, 11, -0.5), (3.8, 16.5, 6), fur(422, F, D, L, 12.2, 2.2, 0.58))
    m.box("chest", "Body", (-5.5, 13.5, -3), (5.5, 24.5, 4), chest, rot=CR, origin=CP)
    m.box("mane", "Body", (-1.6, 16, 3.6), (1.6, 26, 5.6), ragged_hem(422, shag(422, D, (44, 40, 44)), 2), rot=CR, origin=CP)
    m.bone("tail", (0, 14.5, 6))
    m.box("tail", "tail", (-1.2, 13, 5.5), (1.2, 15.4, 13.5), tail, rot=(28, 0, 0), origin=(0, 14.5, 6))
    m.box("tail-brush", "tail", (-1.9, 12.4, 8), (1.9, 15.8, 12.5), tail, rot=(28, 0, 0), origin=(0, 14.5, 6))
    m.bone("Head", (0, 22.5, -4))
    m.box("head", "Head", (-3.3, 20, -11), (3.3, 26, -4.5), head, head_d)
    m.box("snout", "Head", (-1.7, 20.4, -16), (1.7, 23.2, -10.5), lambda p: F if p.face == "up" else D if p.face == "down" else mix(F, L, 0.5), snout_d)
    m.bone("jaw", (0, 20.6, -10), "Head")
    m.box("jaw", "jaw", (-1.4, 19, -15.4), (1.4, 20.5, -9.5), jaw, rot=(-10, 0, 0), origin=(0, 20.6, -10))
    m.pair("cheek", "Head", (-4.3, 19.6, -8.5), (-3.1, 23.5, -5), lambda p: L if p.ly < 0.5 else F)
    ears(m, (-3.2, 25.5, -7.5), (-1.2, 29.5, -6.3), pointed(F, (206, 150, 150), D))
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 6.7 * sx, 2.6 * sx
        ap = rotp((ax, 22.5, 0.5), CR, CP)
        ar = (16, 0, 0)
        m.bone("Arm-" + tag, ap)
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 1.5, ap[1] - 9, ap[2] - 1.6), (ax + 1.5, ap[1] + 1.2, ap[2] + 1.6), arm, rot=ar, origin=ap)
        m.box("forearm-" + tag.lower(), "Arm-" + tag, (ax - 1.3, ap[1] - 16, ap[2] - 1.4), (ax + 1.3, ap[1] - 8.6, ap[2] + 1.4),
              lambda p: D if p.ly < 0.3 else F, rot=ar, origin=ap)
        m.box("claws-" + tag.lower(), "Arm-" + tag, (ax - 1.5, ap[1] - 18.6, ap[2] - 1.6), (ax + 1.5, ap[1] - 15.6, ap[2] + 0.6), hand, rot=ar, origin=ap)
        m.bone("Leg-" + tag, (lx, 12, 3))
        m.box("thigh-" + tag.lower(), "Leg-" + tag, (lx - 1.7, 6.5, 0.2), (lx + 1.7, 13, 5), coat)
        m.box("shin-" + tag.lower(), "Leg-" + tag, (lx - 1.1, 1, 2.6), (lx + 1.1, 7.5, 5.2), lambda p: D if p.y < 3 else F)
        m.box("paw-" + tag.lower(), "Leg-" + tag, (lx - 1.5, 0, -1.6), (lx + 1.5, 1.7, 4.6), D, claws_d(CL))
    return m


def zombie():
    m = Model("zombie", 423)
    SK, SD, SHT, SHD, PT, PD, DK, BN = (126, 162, 110), (88, 122, 84), (72, 116, 134), (48, 82, 100), (84, 70, 104), (58, 48, 76), (30, 34, 30), (228, 222, 200)

    def skin(p):
        return SD if vnoise(423, p.x / 2.0, p.y / 2.0, p.z / 2.0) > 0.62 else SK

    def shirt(p):
        n = vnoise(423, p.x / 2.5 + 5, p.y / 2.5, p.z / 2.5)
        if n > 0.7:
            return skin(p)
        if p.face == "north" and 2 < p.i < 6 and 3 < p.j < 7:
            return (120, 40, 44) if (p.i + p.j) % 2 else BN
        return SHD if n < 0.36 or p.i % 4 == 0 else SHT

    def pants(p):
        if p.y < 1.4:
            return (56, 46, 40)
        if p.y < 3.2:
            return skin(p)
        if hsh(423, int(p.x * 2 + 50), int(p.y), int(p.z * 2 + 50)) > 0.9:
            return skin(p)
        return PD if int(p.y) % 4 == 0 else PT

    def head_d(c):
        if c.face == "north":
            c.symrect(1, 2, 2, 2, DK)
            c.px(1, 3, WHITE), c.px(c.w - 3, 2, (200, 60, 50))
            c.rect(2, 5, 3, 1, DK)
            c.px(2, 5, BN), c.px(4, 5, BN)
            c.px(5, 4, (120, 40, 44)), c.px(5, 5, (120, 40, 44))
            c.rect(0, 0, c.w, 1, (50, 60, 44)), c.rect(0, 1, 2, 1, (50, 60, 44))
        elif c.face in ("east", "west", "south"):
            c.rect(0, 0, c.w, 2 if c.face == "south" else 1, (50, 60, 44))
            if c.face == "south":
                c.rect(1, 2, 3, 1, (50, 60, 44))
        elif c.face == "up":
            c.rect(0, 0, c.w, c.h, (50, 60, 44))
            c.rect(4, 2, 2, 2, SD)

    def forearm(p):
        if p.part.endswith("-l") and 0.3 < p.ly < 0.7 and p.face in ("north", "west"):
            return BN
        return skin(p)

    def hand(p):
        gap = p.side() and p.j >= p.h - 1 and p.i % 2 == 1
        return tuple(SD) + ((0,) if gap else (255,))

    m.box("torso", "Body", (-4, 12, -2), (4, 22.5, 2), shirt)
    m.box("shirt-tail", "Body", (-4.2, 9.5, -2.2), (4.2, 12.6, 2.2), ragged_hem(423, shirt, 3), skip=("down", "up"))
    m.bone("Head", (0, 22.5, 0))
    m.box("head", "Head", (-3.5, 22.5, -3.5), (3.5, 29.5, 3.5), skin, head_d, rot=(0, 0, -7), origin=(0, 22.5, 0))
    m.box("hair", "Head", (-3.8, 27.6, 0), (3.8, 30.2, 3.9), ragged_hem(424, (50, 60, 44), 2), rot=(0, 0, -7), origin=(0, 22.5, 0), skip=("down",))
    for tag, sx, reach in (("L", -1, 78), ("R", 1, 60)):
        ax, lx = 5.2 * sx, 2.0 * sx
        ap = (ax, 21.3, 0)
        m.bone("Arm-" + tag, ap)
        m.box("sleeve-" + tag.lower(), "Arm-" + tag, (ax - 1.3, 16, -1.4), (ax + 1.3, 22.5, 1.4), ragged_hem(425, shirt, 2), rot=(reach, 0, 0), origin=ap,
              skip=("down",))
        m.box("forearm-" + tag.lower(), "Arm-" + tag, (ax - 1.1, 11.5, -1.2), (ax + 1.1, 17, 1.2), forearm, rot=(reach, 0, 0), origin=ap)
        m.box("hand-" + tag.lower(), "Arm-" + tag, (ax - 1.2, 9, -1.3), (ax + 1.2, 11.8, 1.3), hand, rot=(reach, 0, 0), origin=ap)
        m.bone("Leg-" + tag, (lx, 11.5, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 1.9, 0, -1.8), (lx + 1.9, 12, 1.8), pants)
        m.box("cuff-" + tag.lower(), "Leg-" + tag, (lx - 2.1, 2.2, -2), (lx + 2.1, 5.5, 2), ragged_hem(426 + sx, PT, 3), skip=("down", "up"))
    return m


def ogre():
    m = Model("ogre", 424, noise=8)
    SK, SD, BL, CL, CD, WD, IV, DK = (152, 162, 106), (112, 124, 80), (190, 192, 136), (116, 82, 52), (82, 56, 38), (140, 100, 60), (238, 230, 204), (40, 36, 30)

    def skin(p):
        n = vnoise(424, p.x / 3.0, p.y / 3.0, p.z / 3.0)
        c = SD if n > 0.64 else SK
        if hsh(424, int(p.x + 50), int(p.y), int(p.z + 50)) > 0.95:
            c = mul(SD, 0.8)
        return c

    def belly(p):
        if p.y < 12.5:
            return CD if int(p.x + 100) % 3 == 0 else CL
        if p.y < 13.6:
            return (200, 180, 120)
        if p.face == "north" and abs(p.x) < 6 and p.y < 21:
            if abs(p.x) < 0.6 and 15 < p.y < 16.2:
                return SD
            return BL
        return skin(p)

    def chest_d(c):
        if c.face == "north":
            c.symrect(3, 3, 5, 1, SD)
            c.sym(5, 4, (190, 130, 120))

    def head_d(c):
        if c.face == "north":
            c.symrect(1, 3, 2, 1, (250, 230, 120))
            c.sym(2, 3, BLACK)
            c.rect(3, 4, 3, 2, SD)
            c.sym(3, 5, DK)
        c.side_eye(1, 3, BLACK)

    def jaw_d(c):
        if c.face == "north":
            c.rect(1, 0, c.w - 2, 1, (90, 40, 40))

    def loin(p):
        return CD if (p.i + p.j) % 3 == 0 else CL

    def club(p):
        c = planks(WD, lambda q: q.x + q.y, lambda q: q.z, 3, 424)(p)
        if p.part.startswith("club-head") and hsh(425, p.i, p.j, len(p.face)) > 0.86:
            return (190, 190, 196)
        return c

    def foot_d(c):
        if c.face == "north":
            for x in range(0, c.w, 2):
                c.rect(x, c.h - 1, 1, 1, IV)
        elif c.face == "up":
            for x in range(1, c.w, 2):
                c.rect(x, 0, 1, 2, SD)

    m.box("belly", "Body", (-9, 10, -6.5), (9, 22, 6.5), belly)
    m.box("chest", "Body", (-10, 21, -5.5), (10, 30, 5.5), skin, chest_d)
    m.box("loin-f", "Body", (-4, 4.5, -7.1), (4, 11, -6.3), ragged_hem(424, loin, 3), skip=("up", "down"))
    m.box("loin-b", "Body", (-4, 5, 6.3), (4, 11, 7.1), ragged_hem(427, loin, 3), skip=("up", "down"))
    m.bone("Head", (0, 29.5, -2))
    m.box("head", "Head", (-4.5, 29.5, -6.5), (4.5, 36.5, 1.5), skin, head_d)
    m.box("brow", "Head", (-4.8, 33.6, -7.2), (4.8, 35.2, -5.6), SD)
    ears(m, (-6.6, 31.5, -3), (-4.3, 35, -1.8), lambda p: (190, 150, 130) if p.face == "north" and p.i > 0 and p.j > 0 else SD,
         rot=(0, 0, 12), pivot=(-4.4, 32.5, -2.4))
    m.bone("jaw", (0, 30, -2.5), "Head")
    m.box("jaw", "jaw", (-4, 27.8, -7.6), (4, 30.2, -2.5), skin, jaw_d)
    m.pair("tusk", "jaw", (-3.5, 30, -7.5), (-2.4, 32.4, -6.4), IV)
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 12.3 * sx, 4.6 * sx
        m.bone("Arm-" + tag, (ax, 28, 0))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 2.4, 18.5, -2.6), (ax + 2.4, 29.2, 2.8), skin)
        m.box("fist-" + tag.lower(), "Arm-" + tag, (ax - 2.9, 9.5, -3.3), (ax + 2.9, 19, 3.5),
              lambda p: CD if 0.72 < p.ly < 0.86 else skin(p))
        m.bone("Leg-" + tag, (lx, 10, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 3, 0, -3), (lx + 3, 11, 3.5), skin)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 3.4, 0, -5.6), (lx + 3.4, 2.6, 3.9), skin, foot_d)
    cx = 12.3
    m.box("club-handle", "Arm-R", (cx - 1.2, 10.5, -11), (cx + 1.2, 13, -2.5), club)
    m.box("club-head", "Arm-R", (cx - 2.7, 8.8, -22), (cx + 2.7, 14.6, -10.5), club)
    m.box("club-knot", "Arm-R", (cx - 1.8, 14.2, -19.5), (cx + 1.8, 16, -15.5), club)
    return m


def ghoul():
    m = Model("ghoul", 425)
    SK, SD, LT, DK, EY, RG, CL = (178, 186, 170), (128, 140, 130), (206, 212, 196), (40, 36, 46), (190, 255, 120), (96, 84, 76), (60, 54, 58)

    def skin(p):
        return SD if vnoise(425, p.x / 1.8, p.y / 1.8, p.z / 1.8) > 0.64 else SK

    def torso(p):
        if p.face == "north":
            if int(p.j) % 2 == 1 and p.j < 7 and abs(p.x) > 0.6:
                return SD
            return LT if abs(p.x) < 0.6 else SK
        if p.face == "south":
            return LT if abs(p.x) < 0.7 and p.j % 2 == 0 else SD if abs(p.x) < 0.7 else SK
        if p.face in ("east", "west") and p.j % 2 == 1 and p.j < 7:
            return SD
        return skin(p)

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 2, 2, 2, DK)
            c.sym(1, 2, EY), c.sym(1, 3, mul(EY, 0.6))
            c.px(2, 4, DK), c.px(3, 4, DK)
            for x in range(c.w):
                c.px(x, c.h - 1, WHITE if x % 2 == 0 else DK)
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 2, DK), c.px(c.fx(0), 3, DK)
            for k in range(4):
                c.px(c.fx(k), c.h - 1, WHITE if k % 2 == 0 else DK)

    def jaw(p):
        if p.face == "up":
            return (110, 30, 44)
        if p.side() and p.j == 0 and p.face != "south":
            return WHITE if p.i % 2 else DK
        return SD

    def claws(p):
        if p.side() and p.j >= 1:
            return CL + ((255,) if p.i % 2 == 0 else (0,))
        return SD

    def rag(p):
        return mul(RG, 0.75) if (p.i + p.j) % 3 == 0 else RG

    CP, CR = (0, 12.5, 0.5), (-25, 0, 0)
    m.box("pelvis", "Body", (-3, 10.5, -1.6), (3, 13.2, 2.4), skin)
    m.box("rag", "Body", (-3.3, 6.5, -1.9), (3.3, 12, 2.7), ragged_hem(425, rag, 4), skip=("up", "down"))
    m.box("torso", "Body", (-3.4, 12.5, -2), (3.4, 22, 2.2), torso, rot=CR, origin=CP)
    m.box("spine", "Body", (-0.6, 13, 2), (0.6, 22.4, 3.1), spines(LT, SD, "y", 3), rot=CR, origin=CP)
    m.bone("Head", (0, 20.5, -4))
    m.box("head", "Head", (-3, 19, -9.6), (3, 25, -3.5), lambda p: LT if p.face == "up" else skin(p), head_d)
    ears(m, (-5.2, 21.5, -5.6), (-2.8, 23.2, -4.6), pointed(SD, (190, 150, 150)), rot=(0, 0, -20), pivot=(-2.9, 22, -5.1))
    m.bone("jaw", (0, 19.4, -4.5), "Head")
    m.box("jaw", "jaw", (-2.6, 17, -9.3), (2.6, 19, -4.6), jaw, rot=(-12, 0, 0), origin=(0, 19.4, -4.5))
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 4.4 * sx, 1.8 * sx
        ap = rotp((ax, 21, 0), CR, CP)
        m.bone("Arm-" + tag, ap)
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 0.9, ap[1] - 9.5, ap[2] - 0.9), (ax + 0.9, ap[1] + 0.8, ap[2] + 0.9), skin, rot=(22, 0, 0), origin=ap)
        el = rotp((ax, ap[1] - 9.3, ap[2]), (22, 0, 0), ap)
        m.box("forearm-" + tag.lower(), "Arm-" + tag, (ax - 0.8, el[1] - 8.4, el[2] - 0.8), (ax + 0.8, el[1] + 0.5, el[2] + 0.8),
              lambda p: SD if p.ly < 0.2 else SK, rot=(-8, 0, 0), origin=el)
        m.box("claws-" + tag.lower(), "Arm-" + tag, (ax - 1.1, max(0.2, el[1] - 11.4), el[2] - 1.2), (ax + 1.1, el[1] - 8, el[2] + 0.8), claws,
              rot=(-8, 0, 0), origin=el)
        m.bone("Leg-" + tag, (lx, 11, 0.5))
        m.box("thigh-" + tag.lower(), "Leg-" + tag, (lx - 1.1, 5, -0.8), (lx + 1.1, 11.5, 1.6), skin)
        m.box("shin-" + tag.lower(), "Leg-" + tag, (lx - 0.9, 0.6, -0.2), (lx + 0.9, 5.5, 1.5), lambda p: SD if abs(p.y - 5) < 0.8 else SK)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 1.2, 0, -3.2), (lx + 1.2, 1, 1.7), SD, claws_d(CL))
    return m


def scarecrow():
    m = Model("scarecrow", 426)
    ST, SD, BU, BD, RD, RK, BLU, BLD, HT, HD, WD, DK = ((228, 198, 98), (186, 150, 62), (198, 168, 118), (160, 130, 88), (182, 62, 54), (122, 38, 38),
                                                        (76, 98, 146), (54, 70, 112), (150, 112, 62), (112, 80, 44), (120, 86, 54), (36, 30, 28))

    def straw(p):
        return SD if hsh(426, int(p.x * 2 + 60), int(p.z * 2 + 60), len(p.face)) > 0.55 else ST

    def strands(p):
        c = straw(p)
        if p.side() and p.i % 2 == 1:
            return c + (0,)
        return c

    def hand(p):
        c = straw(p)
        if p.face in ("up", "down", "north", "south") and (p.j if p.face in ("north", "south") else p.j) % 2 == 1 and abs(p.x) > 13:
            return c + (0,)
        return c

    def plaid(p):
        a = int(math.floor(p.x + 100)) % 3 == 0 if p.face in ("north", "south", "up", "down") else int(math.floor(p.z + 100)) % 3 == 0
        b = int(math.floor(p.y + 100)) % 3 == 0 if p.side() else int(math.floor(p.z + 100)) % 3 == 0
        return RK if a and b else mix(RD, RK, 0.5) if a or b else RD

    def shirt(p):
        if p.y < 12.6:
            return (214, 196, 150) if (p.i % 3) else BD
        return plaid(p)

    def shirt_d(c):
        if c.face == "north":
            c.rect(5, 6, 2, 2, BU), c.px(5, 6, DK), c.px(6, 7, DK)
            c.px(3, 2, (240, 230, 200)), c.px(3, 5, (240, 230, 200))

    def pants(p):
        if p.face == "north" and p.x < 0 and 6 < p.y < 9:
            return BU
        return BLD if int(math.floor(p.x * 1.0 + 100)) % 4 == 0 and p.face in ("north", "south") else BLU

    def sack(p):
        return BD if (p.i + p.j) % 2 == 0 and hsh(426, p.i, p.j, len(p.face)) > 0.5 else BU

    def head_d(c):
        if c.face == "north":
            c.rect(1, 2, 2, 2, DK), c.px(1, 2, (90, 80, 70))
            c.px(c.w - 3, 2, DK), c.px(c.w - 2, 3, DK), c.px(c.w - 2, 2, BD), c.px(c.w - 3, 3, BD)
            c.rect(3, 3, 1, 2, (226, 120, 40))
            for x in range(1, c.w - 1):
                c.px(x, 5 if x % 2 else 4, DK)
        if c.face in ("north", "south", "east", "west"):
            for x in range(0, c.w, 2):
                c.px(x, c.h - 1, ST)

    def hat(p):
        if p.part == "hat-crown" and p.side() and p.j == p.h - 1:
            return RK
        return HD if (p.i + p.j) % 3 == 0 else HT

    m.box("pole", "Body", (-0.7, 0, 1.2), (0.7, 24, 2.6), planks(WD, lambda q: q.x + q.z, lambda q: q.y, 3, 426))
    m.box("torso", "Body", (-4, 11.5, -2), (4, 22, 2), shirt, shirt_d)
    m.box("collar", "Body", (-2.6, 21.6, -2.4), (2.6, 23.2, 2.4), strands, skip=("down",))
    m.bone("Head", (0, 22.6, 0))
    m.box("head", "Head", (-3.3, 22.8, -3.3), (3.3, 29.4, 3.3), sack, head_d)
    m.box("hat-brim", "Head", (-5.6, 28.8, -5.6), (5.6, 29.7, 5.6), hat)
    m.box("hat-crown", "Head", (-3.1, 29.5, -3.1), (3.1, 33, 3.1), hat)
    m.box("hat-tip", "Head", (-1.8, 32.6, -1.2), (1.8, 35.4, 2.4), hat, rot=(18, 0, 6), origin=(0, 32.8, 0.6))
    for tag, sx in (("L", -1), ("R", 1)):
        lx = 2.0 * sx
        x0, x1 = (-12.5, -4) if sx < 0 else (4, 12.5)
        h0, h1 = (-15.2, -12.2) if sx < 0 else (12.2, 15.2)
        m.bone("Arm-" + tag, (4.0 * sx, 20.4, 0))
        m.box("sleeve-" + tag.lower(), "Arm-" + tag, (x0, 18.8, -1.5), (x1, 22, 1.5), plaid)
        m.box("hand-" + tag.lower(), "Arm-" + tag, (h0, 19.2, -1.1), (h1, 21.6, 1.1), hand)
        m.bone("Leg-" + tag, (lx, 11.5, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 1.7, 2.6, -1.6), (lx + 1.7, 12, 1.6), pants)
        m.box("cuff-" + tag.lower(), "Leg-" + tag, (lx - 1.3, 0.4, -1.2), (lx + 1.3, 3, 1.2), strands)
    return m


MONSTERS = [skeleton, werewolf, zombie, ogre, ghoul, scarecrow]


# -- the spellsNmagic pack (written to mods/spellsNmagic/models/)

def dragon():
    m = Model("dragon", 501, noise=6)
    SC, SE, BL, HN, MB, SP, EY = (216, 224, 206), (168, 182, 164), (242, 234, 198), (240, 230, 204), (232, 216, 198), (150, 164, 152), (255, 186, 40)
    hide = scaly(SC, SE, BL, 16.2)
    top = scaly(SC, SE)

    def leg(p):
        if p.y < 1.2:
            return SE
        return SE if (p.i + 2 * p.j) % 4 == 0 else SC if (p.i + p.j) % 2 else mul(SC, 0.94)

    def neck(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.88)
        return top(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 2, EY), c.px(c.fx(3), 2, BLACK), c.px(c.fx(4), 2, EY)
            for k in range(1, 6):
                c.px(c.fx(k), 1, SP)
        elif c.face == "up":
            c.symrect(1, 2, 2, c.h - 2, SE)

    def snout_d(c):
        if c.face == "up":
            c.sym(1, 1, (70, 70, 66))
            c.rect(c.w // 2 - 1, 3, 2, c.h - 3, SE)
        elif c.face in ("east", "west"):
            for k in range(0, c.w, 2):
                c.px(c.fx(k), c.h - 1, WHITE)
            c.rect(0, c.h - 2, c.w, 1, mul(SE, 0.8))
        elif c.face == "north":
            c.sym(1, 1, (70, 70, 66))
            c.sym(0, c.h - 1, WHITE), c.sym(2, c.h - 1, WHITE)

    def jaw(p):
        if p.face == "up":
            return (176, 70, 80)
        if p.face in ("east", "west", "north") and p.j == 0 and p.i % 2 == 1:
            return WHITE
        return BL if p.face == "down" else mul(SC, 0.92)

    def horn(p):
        return mix(HN, (150, 140, 120), p.lz) if int(p.lz * 6) % 2 else mix(HN, (150, 140, 120), p.lz * 0.6)

    def spade(p):
        if p.face in ("up", "down"):
            w = (p.lz * 1.8 if p.lz < 0.5 else (1 - p.lz) * 2.2) + 0.12
            if abs(p.lx - 0.5) * 2 > w:
                return SP + (0,)
        return SP if p.face != "up" else mix(SP, SC, 0.4)

    m.box("body", "Body", (-6, 14, -11), (6, 25.5, 11), hide)
    m.box("back-spines", "Body", (-0.5, 25.3, -9), (0.5, 28.3, 11), spines(SP, SC), noise=3)
    # neck -> head
    NP, NR = (0, 21, -10), (-40, 0, 0)
    m.bone("neck", NP)
    m.box("neck-a", "neck", (-3, 19.5, -13.5), (3, 30, -8), neck, rot=NR, origin=NP)
    m.box("neck-b", "neck", (-2.6, 29, -13), (2.6, 38, -8.5), neck, rot=NR, origin=NP)
    m.box("neck-spines", "neck", (-0.5, 22, -8.2), (0.5, 38, -6.2), spines(SP, SC, "y"), rot=NR, origin=NP, noise=3)
    m.bone("Head", (0, 33, -21), "neck")
    m.box("head", "Head", (-4, 31, -28.5), (4, 38, -19.5), top, head_d)
    m.box("snout", "Head", (-3, 31.5, -35), (3, 36, -28), top, snout_d)
    m.bone("jaw", (0, 31.8, -21.5), "Head")
    m.box("jaw", "jaw", (-2.7, 29.8, -34), (2.7, 31.6, -21), jaw, rot=(-8, 0, 0), origin=(0, 31.8, -21.5))
    m.pair("horn", "Head", (-3.8, 36.8, -21.5), (-2.2, 38.8, -13.5), horn, rot=(-24, 0, 0), origin=(-3, 37.8, -21))
    m.pair("frill", "Head", (-4.9, 31.5, -22.5), (-3.9, 36, -17.5), spines(SP, BL, "y", 3), noise=3)
    # wings: an inner plate at the shoulder and an outer one on its own bone
    WP, WR = (-5, 25, -5), (0, 0, -18)
    web = membrane(5, 4, SC, SE, MB)
    wings(m, WP, (-21, 24.6, -10), (-5, 25.4, 6), web, rot=WR, skip=("south",), name="wing-in")
    EL = rotp((-21, 25, -2), WR, WP)
    m.bone("wing-l-tip", EL, "wing-l")
    m.bone("wing-r-tip", (-EL[0], EL[1], EL[2]), "wing-r")
    tip = membrane(-EL[0], 4, SC, SE, MB, 0.36)
    m.pair("wing-out", ("wing-l-tip", "wing-r-tip"), (EL[0] - 16, EL[1] - 0.38, -9.5), (EL[0], EL[1] + 0.38, 5), tip,
           rot=(0, 0, 6), origin=EL, skip=("south", "east", "west"))
    # legs
    four_legs(m, -7.5, -3.5, -10, 6, 4.5, 14, leg)
    m.pair("foot-f", ("Leg-FL", "Leg-FR"), (-8, 0, -12.5), (-3, 2.2, -5), leg, claws_d(WHITE))
    m.pair("foot-b", ("Leg-BL", "Leg-BR"), (-8, 0, 3.5), (-3, 2.2, 11), leg, claws_d(WHITE))
    m.pair("haunch", ("Leg-BL", "Leg-BR"), (-8, 11, 4.5), (-5.2, 21, 11.5), top)
    # tail chain, drooping away from the hips
    TP, TR = (0, 21, 11), (12, 0, 0)
    segs = ((3.5, 17, 24, 10.5, 19.5), (2.6, 17.6, 22.8, 19, 28), (1.8, 18, 21.6, 27.5, 36), (1.1, 18.4, 20.6, 35.5, 43))
    par = "Body"
    for k, (w, y0, y1, z0, z1) in enumerate(segs):
        name = "tail-" + "abcd"[k]
        m.bone(name, rotp((0, (y0 + y1) / 2 + (1 if k == 0 else 0), z0 + 0.5), TR, TP), par)
        m.box(name, name, (-w, y0, z0), (w, y1, z1), scaly(SC, SE, BL, y0 + 1.2), rot=TR, origin=TP)
        par = name
    m.box("tail-spines-a", "tail-a", (-0.45, 23.8, 11.5), (0.45, 26.2, 19.5), spines(SP, SC), rot=TR, origin=TP, noise=3)
    m.box("tail-spines-b", "tail-b", (-0.45, 22.6, 19.5), (0.45, 24.6, 27.5), spines(SP, SC), rot=TR, origin=TP, noise=3)
    m.box("tail-spade", "tail-d", (-3, 19, 42), (3, 20, 48), spade, rot=TR, origin=TP)
    return m


def wyvern():
    m = Model("wyvern", 502, noise=6)
    SC, SE, BL, MB, SP, EY, CL = (192, 208, 178), (140, 160, 136), (234, 226, 188), (218, 200, 172), (110, 128, 116), (250, 220, 60), (60, 56, 54)
    top = scaly(SC, SE)

    def neck(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.88)
        return top(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, EY), c.px(c.fx(3), 1, BLACK)
            for k in range(1, 5):
                c.px(c.fx(k), 0, SP)
        elif c.face == "north":
            c.sym(0, 1, EY)

    def snout_d(c):
        if c.face in ("east", "west"):
            for k in range(0, c.w, 2):
                c.px(c.fx(k), c.h - 1, WHITE)
        elif c.face == "up":
            c.sym(0, 1, CL)
        elif c.face == "north":
            c.sym(0, c.h - 1, WHITE)

    def jaw(p):
        if p.face == "up":
            return (170, 66, 78)
        if p.face in ("east", "west", "north") and p.j == 0 and p.i % 2 == 1:
            return WHITE
        return BL

    def leg(p):
        return SE if (p.i + 2 * p.j) % 4 == 0 else SC

    BP, BR = (0, 13, 2), (18, 0, 0)
    m.box("body", "Body", (-4, 11, -7), (4, 19.5, 8), scaly(SC, SE, BL, 12.6), rot=BR, origin=BP)
    m.box("back-spines", "Body", (-0.45, 19.3, -5), (0.45, 21.6, 8), spines(SP, SC), rot=BR, origin=BP, noise=3)
    NP, NR = (0, 20.5, -5), (-20, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2, 19.5, -7.5), (2, 31, -3.5), neck, rot=NR, origin=NP)
    m.box("neck-spines", "neck", (-0.45, 21, -3.7), (0.45, 31, -2), spines(SP, SC, "y"), rot=NR, origin=NP, noise=3)
    m.bone("Head", (0, 30, -9), "neck")
    m.box("head", "Head", (-3, 28, -15), (3, 33.5, -8), top, head_d)
    m.box("snout", "Head", (-2.2, 28.3, -20), (2.2, 31.8, -14.5), top, snout_d)
    m.bone("jaw", (0, 28.6, -9.5), "Head")
    m.box("jaw", "jaw", (-2, 27, -19.3), (2, 28.5, -9.5), jaw, rot=(-9, 0, 0), origin=(0, 28.6, -9.5))
    m.pair("horn", "Head", (-2.9, 32.4, -10), (-1.5, 34, -3.5), lambda p: mix((236, 226, 200), (130, 120, 104), p.lz), rot=(-22, 0, 0), origin=(-2.2, 33.2, -9.5))
    # wings for arms
    WP, WR = rotp((-4, 18.5, -4.5), BR, BP), (0, 0, -14)
    web = membrane(4, 4, SC, SE, MB)
    wings(m, WP, (WP[0] - 14, WP[1] - 0.4, WP[2] - 4.5), (WP[0], WP[1] + 0.4, WP[2] + 9), web, rot=WR, skip=("south",), name="wing-in")
    EL = rotp((WP[0] - 14, WP[1], WP[2] - 1), WR, WP)
    m.bone("wing-l-tip", EL, "wing-l")
    m.bone("wing-r-tip", (-EL[0], EL[1], EL[2]), "wing-r")
    m.pair("wing-out", ("wing-l-tip", "wing-r-tip"), (EL[0] - 14, EL[1] - 0.38, WP[2] - 4), (EL[0], EL[1] + 0.38, WP[2] + 7.5),
           membrane(-EL[0], 4, SC, SE, MB, 0.36), rot=(0, 0, 8), origin=EL, skip=("south", "east", "west"))
    m.pair("wing-claw", ("wing-l-tip", "wing-r-tip"), (EL[0] - 0.7, EL[1] - 0.6, WP[2] - 6.6), (EL[0] + 0.7, EL[1] + 0.6, WP[2] - 4), CL)
    for tag, sx in (("L", -1), ("R", 1)):
        lx = 3.6 * sx
        m.bone("Leg-" + tag, (lx, 13, 2.5))
        m.box("thigh-" + tag.lower(), "Leg-" + tag, (lx - 1.9, 7, -0.5), (lx + 1.9, 14.5, 5), leg)
        m.box("shin-" + tag.lower(), "Leg-" + tag, (lx - 1.2, 1, 1.2), (lx + 1.2, 8, 4), leg)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 2, 0, -3.6), (lx + 2, 1.7, 4.6), lambda p: SE if p.face == "up" else SC, claws_d(CL))
    TP, TR = (0, 13, 8), (8, 0, 0)
    segs = ((2.5, 10.5, 15.5, 7.5, 16), (1.8, 11.2, 14.8, 15.5, 24), (1.1, 11.8, 14.2, 23.5, 31))
    par = "Body"
    for k, (w, y0, y1, z0, z1) in enumerate(segs):
        name = "tail-" + "abc"[k]
        m.bone(name, rotp((0, (y0 + y1) / 2, z0 + 0.5), TR, TP), par)
        m.box(name, name, (-w, y0, z0), (w, y1, z1), scaly(SC, SE, BL, y0 + 1.0), rot=TR, origin=TP)
        par = name
    m.box("tail-barb", "tail-c", (-0.7, 12, 30.5), (0.7, 17.5, 32.5), lambda p: mix(CL, (190, 60, 60), p.ly), rot=TR, origin=TP)
    return m


def phoenix():
    m = Model("phoenix", 503, noise=5)
    RD, OR, YL, GD, DK, BK = (228, 72, 38), (246, 142, 42), (252, 212, 88), (255, 238, 160), (156, 38, 32), (250, 206, 90)

    def body(p):
        if p.face == "down" or (p.side() and p.ly < 0.3):
            return YL if (p.i + p.j) % 3 else GD
        if (p.i + 2 * p.j) % 4 == 0:
            return DK
        return OR if p.side() and p.ly < 0.55 else RD

    def flame(p):
        # feather colours running outwards: red at the root, gold at the tips
        u = (abs(p.x) - 3.3) / 20.5
        c = RD if u < 0.3 else OR if u < 0.62 else YL if u < 0.86 else GD
        if p.face in ("up", "down"):
            notch = (p.i % 3 == 0)
            if p.lz > (0.72 if notch else 0.9) + 0.1 * math.sin(abs(p.x) * 1.3):
                return c + (0,)
            if p.lz < 0.2:
                return DK if u < 0.5 else OR
            if notch:
                return mul(c, 0.82)
        return c

    def tail(k):
        def sh(p):
            t = (k + p.lz) / 3.0
            c = RD if t < 0.3 else OR if t < 0.6 else YL
            if p.face in ("up", "down"):
                if k == 2:
                    if abs(abs(p.x) - 1.5) < 0.5:
                        return c + (0,)
                    if p.lz > 0.62:
                        d = math.hypot((p.x - round(p.x / 3.0) * 3.0), (p.lz - 0.8) * 8)
                        return (60, 90, 200) if d < 0.9 else GD
                if abs(p.x) < 0.5:
                    return DK
                if (p.i + p.j) % 3 == 0:
                    return mul(c, 0.84)
            return c
        return sh

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
        c.side_eye(1, 1, BLACK, GD)
        if c.face in ("east", "west"):
            c.px(c.fx(2), 2, YL), c.px(c.fx(3), 2, YL)

    def plume(p):
        return GD if p.ly > 0.75 else YL if p.ly > 0.5 else OR

    m.box("body", "Body", (-3.5, 8, -5), (3.5, 15.5, 5), body)
    m.box("breast", "Body", (-3, 9, -6.6), (3, 15, -4.6), lambda p: YL if p.ly < 0.4 else OR if (p.i + p.j) % 3 else RD)
    NP, NR = (0, 14.5, -4.5), (-10, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-1.5, 13.5, -6.5), (1.5, 21, -3.5), lambda p: OR if int(p.y) % 2 else RD, rot=NR, origin=NP)
    m.bone("Head", (0, 20.5, -6), "neck")
    m.box("head", "Head", (-2.2, 19.5, -9.5), (2.2, 24, -4.5), RD, head_d)
    m.box("beak", "Head", (-0.8, 20.6, -12.2), (0.8, 22, -9.2), BK)
    m.box("beak-hook", "Head", (-0.8, 19.7, -12.2), (0.8, 20.8, -11.1), mul(BK, 0.8))
    m.box("crest-a", "Head", (-0.5, 23.5, -8), (0.5, 29.5, -6.8), plume, rot=(20, 0, 0), origin=(0, 23.5, -7.4))
    m.pair("crest-b", "Head", (-1.7, 23.5, -6.8), (-0.8, 28, -5.8), plume, rot=(28, 0, 14), origin=(-1.2, 23.5, -6.3))
    WP, WR = (-3.5, 14.5, -1.5), (0, 0, -20)
    wings(m, WP, (-15, 14.1, -5), (-3.3, 14.8, 5.5), flame, rot=WR, skip=("south",), name="wing-in")
    m.pair("wing-out", ("wing-l", "wing-r"), (-24, 14.15, -4.2), (-15, 14.75, 4.5), flame, rot=WR, origin=WP, skip=("south", "east"))
    TP, TR = (0, 13, 5), (22, 0, 0)
    segs = ((2.5, 4.5, 12), (2, 11.5, 19.5), (3.5, 19, 28))
    par = "Body"
    for k, (w, z0, z1) in enumerate(segs):
        name = "tail-" + "abc"[k]
        m.bone(name, rotp((0, 13, z0 + 0.5), TR, TP), par)
        m.box(name, name, (-w, 12.6 + 0.02 * k, z0), (w, 13.4 - 0.02 * k, z1), tail(k), rot=TR, origin=TP, skip=("north",))
        par = name
    bird_legs(m, 1.7, 8, 0, BK, BK, th=0.5, foot=1.5)
    for tag, sx in (("l", -1), ("r", 1)):
        m.box("thigh-" + tag, "Leg-" + tag.upper(), (1.7 * sx - 1.2, 4.6, -0.8), (1.7 * sx + 1.2, 9, 1.8),
              ragged_hem(503, lambda p: OR if (p.i + p.j) % 3 else RD, 1), skip=("down",))
    return m


def unicorn():
    m = Model("unicorn", 504)
    C, D, HN, HD, HF, PK = (250, 248, 252), (226, 224, 238), (252, 224, 124), (214, 172, 70), (230, 200, 120), (240, 190, 200)
    HAIR = ((246, 190, 226), (192, 204, 248), (252, 238, 184))

    def coat(p):
        if p.face == "down" or vnoise(504, p.x / 5.0, p.y / 5.0, p.z / 5.0) > 0.64:
            return D
        return C

    def leg(p):
        if p.y < 2:
            return HF
        if p.y < 4.6:
            return C if (p.i + int(p.y)) % 2 else D
        return coat(p)

    def head_d(c):
        c.side_eye(2, 2, (70, 60, 150), None)
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, (120, 110, 150)), c.px(c.fx(3), 1, (120, 110, 150))
        if c.face == "north":
            c.sym(0, 2, (70, 60, 150))
            c.rect(2, 1, 2, 2, HN), c.px(2, 1, HD)

    def muzzle_d(c):
        if c.face == "north":
            c.sym(0, 1, (150, 110, 124))
            c.rect(0, c.h - 1, c.w, 1, (200, 150, 164))

    def hair(p):
        return HAIR[(p.i + p.j // 3) % 3]

    def horn(p):
        return HD if int(math.floor(p.y * 1.4 + p.i)) % 2 == 0 else HN

    NP, NR = (0, 21, -9.5), (-28, 0, 0)
    m.box("body", "Body", (-5, 13, -11), (5, 23.5, 11), coat)
    m.bone("tail", (0, 22.5, 11))
    m.box("tail", "tail", (-1.8, 6, 10.5), (1.8, 22.8, 13.6), ragged_hem(504, hair, 3), rot=(-18, 0, 0), origin=(0, 22.5, 11), skip=("down",))
    m.bone("Head", NP)
    m.box("neck", "Head", (-3, 18, -15.5), (3, 32, -8.5), coat, rot=NR, origin=NP)
    m.box("mane", "Head", (-1.2, 20, -9), (1.2, 34, -6), hair, rot=NR, origin=NP)
    m.box("mane-fall", "Head", (-3.7, 20.5, -14), (-2.9, 31.5, -8.6), ragged_hem(505, hair, 4), rot=NR, origin=NP, skip=("down", "east"))
    m.box("head", "Head", (-3, 26.5, -23.5), (3, 33.5, -14), coat, head_d)
    m.box("muzzle", "Head", (-2.2, 26.5, -27), (2.2, 31, -22.5), PK, muzzle_d)
    m.box("forelock", "Head", (-1.8, 31.5, -20), (1.8, 34.6, -13.5), hair)
    m.box("horn", "Head", (-0.6, 33, -21.2), (0.6, 41.5, -20), horn, rot=(-30, 0, 0), origin=(0, 33.2, -20.6))
    ears(m, (-2.8, 33, -16.2), (-1.4, 36.2, -15), pointed(C, PK))
    four_legs(m, -5, -2, -10.5, 7.5, 3, 13, leg)
    return m


def griffin():
    m = Model("griffin", 505)
    WH, WS, BR, DB, GB, TW, TD, CR, YL, YD, BK = ((246, 242, 232), (216, 210, 196), (170, 122, 70), (112, 78, 50), (214, 168, 96), (220, 176, 110),
                                                  (184, 138, 80), (244, 226, 184), (240, 196, 70), (198, 150, 50), (40, 34, 34))

    def feathers(p):
        if (p.i + 2 * p.j) % 4 == 0:
            return DB
        return GB if (p.i + p.j) % 5 == 0 else BR

    def front(p):
        if p.face == "north":
            return WS if (p.i + 2 * p.j) % 4 == 0 else WH if p.ly > 0.35 else GB
        return feathers(p)

    lion = fur(505, TW, TD, CR, 13.4, 3.0)

    def leg(p):
        if p.z < 0:
            if p.y < 1.4:
                return BK if p.face == "north" and p.i % 2 == 0 else YD
            if p.y < 6:
                return YD if int(p.y) % 2 else YL
            return feathers(p)
        return CR if p.y < 2 else TW

    def neck(p):
        if p.ly < 0.3 and (p.i + p.j) % 2:
            return BR
        return WS if (p.i + 2 * p.j) % 4 == 0 else WH

    def head_d(c):
        if c.face == "north":
            c.sym(0, 2, (250, 190, 40))
            c.sym(0, 1, WS), c.sym(1, 1, WS)
        c.side_eye(1, 2, BLACK, (250, 190, 40))
        if c.face in ("east", "west"):
            c.px(c.fx(0), 1, BR), c.px(c.fx(1), 1, BR), c.px(c.fx(2), 1, BR)

    def wing(p):
        u = (abs(p.x) - 5) / 23.0
        if p.face in ("up", "down"):
            if p.lz > (0.78 if p.i % 3 == 0 else 0.94):
                return DB + (0,)
            if p.lz < 0.22:
                return BR if (p.i + p.j) % 2 else GB
            if p.lz > 0.6:
                return (WS if int(abs(p.x)) % 2 else DB) if u > 0.25 else DB
            return DB if p.i % 3 == 0 else BR
        return BR

    m.box("chest", "Body", (-5, 12, -10), (5, 23.5, 0), front)
    m.box("rear", "Body", (-4.5, 12, -0.5), (4.5, 21.5, 11), lion)
    TP, TR = (0, 19.5, 11), (30, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-0.8, 18.7, 10.5), (0.8, 20.3, 18), TW, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 19.5, 17.8), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-0.7, 18.8, 17.5), (0.7, 20.2, 24.5), TW, rot=TR, origin=TP)
    m.box("tail-tuft", "tail-b", (-1.5, 18.1, 23.5), (1.5, 20.9, 28), lambda p: DB if (p.i + p.j) % 3 else BR, rot=TR, origin=TP)
    m.bone("Head", (0, 22, -9))
    m.box("neck", "Head", (-3.2, 19, -13), (3.2, 27, -7.5), neck)
    m.box("head", "Head", (-3.2, 25, -16.5), (3.2, 31, -9.5), lambda p: WS if p.face == "up" and (p.i + p.j) % 3 == 0 else WH, head_d)
    m.box("beak", "Head", (-1.4, 26, -20), (1.4, 28.6, -16), YL,
          lambda c: c.face in ("east", "west") and [c.px(c.fx(2), 0, BK), c.rect(0, c.h - 1, c.w, 1, YD)])
    m.box("beak-hook", "Head", (-1.2, 24.6, -20), (1.2, 26.2, -18.4), YD)
    ears(m, (-3, 30.5, -12), (-1.4, 33.5, -10.8), pointed(WH, None, BR))
    WP, WR = (-5, 22.5, -5), (0, 0, -25)
    wings(m, WP, (-17, 22.1, -9), (-4.8, 22.9, 3.5), wing, rot=WR, skip=("south",), name="wing-in")
    m.pair("wing-out", ("wing-l", "wing-r"), (-28, 22.15, -8), (-17, 22.85, 2.5), wing, rot=WR, origin=WP, skip=("south", "east"))
    four_legs(m, -5.2, -2.4, -9.5, 7, 3, 12, leg)
    return m


def wisp():
    m = Model("wisp", 506, noise=0)
    CO, GL, DP, DK = (226, 252, 255), (136, 224, 250), (64, 156, 224), (20, 60, 120)

    def core(p):
        d = max(abs(p.i - (p.w - 1) / 2.0) / p.w, abs(p.j - (p.h - 1) / 2.0) / p.h)
        return CO if d < 0.22 else mix(CO, GL, 0.6) if d < 0.38 else GL

    def core_d(c):
        if c.face == "north":
            c.sym(1, 2, DK), c.sym(1, 3, DP)

    def shell(p):
        edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j)
        corner = p.i in (0, p.w - 1) and p.j in (0, p.h - 1)
        on = not corner and ((edge == 0 and (p.i + p.j) % 2 == 0) or (edge > 0 and hsh(506, p.i, p.j, len(p.face)) > 0.86))
        return (DP if edge == 0 else CO) + ((255,) if on else (0,))

    def tatter(n, a, b):
        def sh(p):
            c = mix(a, b, 1 - p.ly)
            if (p.i + p.j) % 3 == 0:
                c = mul(c, 0.86)
            return c
        return ragged_hem(506 + n, sh, n)

    TR, TP = (-28, 0, 0), (0, 9.5, 0.5)
    m.box("shell", "Body", (-4, 8, -4), (4, 16, 4), shell)
    m.bone("Head", (0, 12, 0))
    m.box("core", "Head", (-3, 9, -3), (3, 15, 3), core, core_d)
    m.bone("tail-a", TP)
    m.box("tatter-a", "tail-a", (-2.6, 5, 0), (2.6, 9.6, 1), tatter(3, GL, DP), rot=TR, origin=TP, skip=("down",))
    m.pair("tatter-side", "tail-a", (-3.8, 5.5, -1.5), (-3, 9.4, 1.2), tatter(3, GL, DP), rot=TR, origin=TP, skip=("down",))
    m.bone("tail-b", rotp((0, 5.2, 0.5), TR, TP), "tail-a")
    m.box("tatter-b", "tail-b", (-1.9, 1.8, 0.1), (1.9, 5.4, 0.9), tatter(3, DP, DK), rot=TR, origin=TP, skip=("down",))
    m.bone("tail-c", rotp((0, 2, 0.5), TR, TP), "tail-b")
    m.box("tatter-c", "tail-c", (-1.2, 0, 0.15), (1.2, 2.2, 0.85), tatter(2, DP, DK), rot=TR, origin=TP, skip=("down",))
    for k, (x, y, z) in enumerate(((-6, 14, -2), (5.5, 10, 2.5), (1.5, 18, 1), (-4.5, 6.5, 3.5))):
        m.box("mote-%d" % k, "Body", (x - 0.5, y - 0.5, z - 0.5), (x + 0.5, y + 0.5, z + 0.5), CO if k % 2 else GL)
    return m


def imp():
    m = Model("imp", 507)
    RD, DR, BL, HN, EY, HF, WG = (208, 68, 58), (150, 40, 46), (234, 124, 92), (240, 226, 190), (255, 226, 60), (40, 30, 34), (120, 36, 50)

    def skin(p):
        return DR if vnoise(507, p.x / 1.8, p.y / 1.8, p.z / 1.8) > 0.66 else RD

    def head_d(c):
        if c.face == "north":
            c.symrect(1, 2, 2, 1, EY)
            c.sym(2, 2, BLACK)
            c.sym(0, 1, DR), c.sym(1, 1, DR), c.sym(2, 0, DR)
            c.rect(1, 4, c.w - 2, 1, HF)
            c.px(0, 3, HF), c.px(c.w - 1, 3, HF)
            c.sym(1, 4, WHITE), c.sym(3, 4, WHITE)
        c.side_eye(0, 2, EY)

    def horn(p):
        return HN if p.ly > 0.5 else mix(DR, HN, p.ly * 2)

    def ear(p):
        out = (p.lx < 0.5) == (p.x < 0)
        if p.face in ("north", "south") and out and (p.j == 0 or p.j == p.h - 1):
            return RD + (0,)
        if p.face == "north" and 0 < p.j < p.h - 1:
            return BL
        return DR

    def wing(p):
        u = abs(p.x) - 1.3
        if p.face in ("north", "south"):
            t = (u % 2.5) / 2.5
            if p.ly < 0.42 * math.sin(math.pi * t) * min(1.0, u / 2.0 + 0.3):
                return WG + (0,)
            if p.j == 0 or int(u / 2.5 + 0.02) != int((u - 1) / 2.5 + 0.02):
                return DR
        return WG

    def leg(p):
        return HF if p.y < 1.4 else DR if p.y < 2.4 else skin(p)

    def arm(p):
        if p.side() and p.j == p.h - 1:
            return HF + ((255,) if p.i % 2 == 0 else (0,))
        return skin(p)

    def spade(p):
        if p.face in ("up", "down"):
            w = (p.lz * 2.4 if p.lz < 0.4 else (1 - p.lz) * 1.8) + 0.15
            if abs(p.lx - 0.5) * 2 > w:
                return DR + (0,)
        return DR

    m.box("torso", "Body", (-2.6, 5.5, -1.8), (2.6, 11.6, 1.8), lambda p: BL if p.face == "north" and abs(p.x) < 1.6 and p.ly < 0.7 else skin(p))
    m.box("belly", "Body", (-2, 6, -2.7), (2, 9.6, -1.6), BL, lambda c: c.face == "north" and c.px(2, 2, DR))
    m.bone("Head", (0, 11.5, 0))
    m.box("head", "Head", (-3.6, 11.3, -3.6), (3.6, 17.6, 2.8), skin, head_d)
    m.box("nose", "Head", (-0.6, 13.3, -4.6), (0.6, 14.6, -3.4), DR)
    m.pair("horn", "Head", (-3.2, 17.2, -2), (-2, 20.4, -0.8), horn, rot=(0, 0, 16), origin=(-2.6, 17.2, -1.4))
    ears(m, (-6.6, 13.4, -0.8), (-3.4, 16.2, 0.2), ear, rot=(0, 0, -18), pivot=(-3.5, 14.6, -0.3))
    wings(m, (-1.3, 10.5, 2), (-9.3, 7, 1.9), (-1.3, 15, 2.5), wing, rot=(0, 28, 0))
    TP, TR = (0, 6.5, 1.8), (-26, 0, 0)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-0.5, 6, 1.5), (0.5, 7, 6.5), DR, rot=TR, origin=TP)
    m.bone("tail-b", rotp((0, 6.5, 6.3), TR, TP), "tail-a")
    m.box("tail-b", "tail-b", (-0.45, 6.05, 6), (0.45, 6.95, 10.5), DR, rot=TR, origin=TP)
    m.box("tail-spade", "tail-b", (-1.5, 6.1, 10), (1.5, 6.9, 13.2), spade, rot=TR, origin=TP)
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 3.3 * sx, 1.5 * sx
        m.bone("Arm-" + tag, (ax, 10.8, 0))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 0.8, 4.6, -0.8), (ax + 0.8, 11.3, 0.8), arm)
        m.bone("Leg-" + tag, (lx, 5.6, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 1.1, 0, -1), (lx + 1.1, 6, 1.2), leg)
    return m


def mimic():
    m = Model("mimic", 508)
    WD, FR, MT, MD, IN, TG, IV, EY = (170, 118, 62), (86, 54, 30), (220, 184, 84), (150, 116, 44), (112, 26, 48), (228, 92, 114), (244, 238, 216), (255, 220, 60)

    def wood(p):
        edge = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0
        if p.face == "up" and p.part == "base" or p.face == "down" and p.part == "lid":
            return FR if edge else IN if (p.i + p.j) % 4 else mul(IN, 0.8)
        if edge:
            return FR
        if p.face in ("north", "south", "up") and (p.i in (2, p.w - 3)):
            return MD if p.j % 2 else MT
        s = p.x + p.z if p.side() else p.x
        return planks(WD, lambda q: s, lambda q: q.y + q.z, 3, 508)(p)

    def lid_d(c):
        if c.face == "north":
            c.symrect(3, 1, 3, 2, EY)
            c.sym(4, 1, BLACK), c.sym(4, 2, BLACK)
            c.symrect(3, 0, 3, 1, FR)
            x = c.w // 2 - 1
            c.rect(x, c.h - 3, 2, 3, MT), c.px(x, c.h - 2, FR), c.px(x + 1, c.h - 2, FR)

    def teeth(up):
        def sh(p):
            if p.face in ("up", "down"):
                return IV + (0,)
            row = p.j if up else p.h - 1 - p.j
            on = p.i % 2 == 0 or row < p.h // 2
            if p.face in ("north", "south") and p.part.endswith(("-l", "-r")):
                on = False
            return (IV if row < p.h - 1 else mul(IV, 0.85)) + ((255,) if on else (0,))
        return sh

    def tongue(p):
        return mul(TG, 0.82) if p.face == "up" and abs(p.x) < 0.5 else TG

    def leg(p):
        if p.y < 1.2:
            return IV if p.face == "north" and p.i % 2 == 0 else FR
        return FR if p.i % 2 else mul(WD, 0.8)

    JP, JR = (0, 9, 5), (30, 0, 0)
    m.box("base", "Body", (-7, 3, -5), (7, 9, 5), wood)
    m.box("teeth-lo", "Body", (-6.5, 9, -4.9), (6.5, 11, -4.1), teeth(False), noise=2)
    m.pair("teeth-lo-side", "Body", (-6.9, 9, -4.5), (-6.1, 10.8, 1.5), teeth(False), noise=2)
    m.box("tongue", "Body", (-2, 8.9, -10.5), (2, 9.9, -2), tongue, rot=(-10, 0, 0), origin=(0, 9.4, -2))
    m.bone("jaw", JP)
    m.box("lid", "jaw", (-7, 9, -5), (7, 14, 5), wood, lid_d, rot=JR, origin=JP)
    m.box("teeth-hi", "jaw", (-6.5, 7, -4.9), (6.5, 9, -4.1), teeth(True), rot=JR, origin=JP, noise=2)
    m.pair("teeth-hi-side", "jaw", (-6.9, 7.2, -4.5), (-6.1, 9, 1.5), teeth(True), rot=JR, origin=JP, noise=2)
    four_legs(m, -6.6, -4, -4.6, 2, 2.6, 3, leg)
    return m


def fairy():
    m = Model("fairy", 509, noise=4)
    SK, SD, DR, DD, PT, HR, HD, WG, WE, SL = ((252, 224, 202), (232, 190, 170), (156, 224, 176), (110, 186, 140), (248, 176, 206), (252, 218, 116),
                                              (222, 176, 76), (220, 242, 255), (170, 210, 245), (120, 190, 150))

    def dress(p):
        return DD if (p.i + p.j) % 3 == 0 else DR

    def petals(p):
        c = PT if int(math.floor(p.x * 1.0 + p.z * 1.0 + 100)) % 2 else mul(PT, 0.88)
        if p.face == "up":
            return DR
        return c

    def head_d(c):
        if c.face == "north":
            c.sym(0, 2, (60, 110, 200))
            c.rect(1, 3, 2, 1, (220, 120, 130))
            c.rect(0, 0, c.w, 1, HR)
            c.sym(0, 1, HR)
        elif c.face in ("east", "west"):
            c.rect(0, 0, c.w, 2, HR)

    def hair(p):
        return HD if (p.i + p.j // 2) % 3 == 0 else HR

    def wing(p):
        if p.face not in ("north", "south"):
            return WE + (0,)
        dx, dy = (p.lx - 0.5) * 2, (p.ly - 0.5) * 2
        r = dx * dx + dy * dy
        if r > 1.0:
            return WE + (0,)
        if r > 0.62 or abs(dy) < 0.12 or (abs(dx + dy) < 0.16 and p.w > 5):
            return WE
        return WG

    def leg(p):
        return SL if p.y < 1.2 else SK

    m.box("torso", "Body", (-1.6, 6.8, -1), (1.6, 10.6, 1), dress)
    m.box("skirt", "Body", (-2.6, 3.6, -2), (2.6, 7.2, 2), ragged_hem(509, petals, 2), skip=("down",))
    m.bone("Head", (0, 10.5, 0))
    m.box("head", "Head", (-2.2, 10.4, -2.2), (2.2, 14.6, 2.2), SK, head_d)
    m.box("hair", "Head", (-2.5, 13.4, -2.5), (2.5, 15.3, 2.6), hair)
    m.box("hair-back", "Head", (-2.5, 8.6, 1.5), (2.5, 13.6, 2.8), ragged_hem(510, hair, 2), skip=("down",))
    m.box("flower", "Head", (1.2, 14.6, -2.9), (2.8, 16, -1.6), PT, lambda c: c.face in ("north", "up") and c.px(1, 0, HR))
    WP = (-0.6, 9.6, 1.1)
    wings(m, WP, (-8, 9.2, 1.1), (-0.5, 16, 1.5), wing, rot=(0, 22, 0), name="wing-hi")
    m.pair("wing-lo", ("wing-l", "wing-r"), (-6, 3.8, 1.15), (-0.5, 9.4, 1.45), wing, rot=(0, 22, 0), origin=WP)
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 2.1 * sx, 0.9 * sx
        m.bone("Arm-" + tag, (ax, 10, 0))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 0.5, 5.8, -0.5), (ax + 0.5, 10.4, 0.5), lambda p: DR if p.ly > 0.8 else SK)
        m.bone("Leg-" + tag, (lx, 5, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 0.6, 0, -0.6), (lx + 0.6, 5.4, 0.6), leg)
    m.box("wand", "Arm-R", (1.85, 5.4, -3.6), (2.35, 5.9, 0.2), (150, 110, 70))
    m.box("wand-star", "Arm-R", (1.35, 4.9, -4.8), (2.85, 6.4, -3.5), (255, 240, 130),
          lambda c: c.face == "north" and c.px(0, 0, WHITE), noise=0)
    return m


def golem_crystal():
    m = Model("golem_crystal", 510, noise=4)
    C, L, D, V, CORE, EY = (172, 228, 242), (232, 250, 255), (98, 162, 204), (182, 158, 240), (255, 250, 204), (255, 255, 255)

    def crystal(p):
        n = vnoise(510, p.x / 5.0 + 3, p.y / 5.0, p.z / 5.0)
        c = mix(C, V, 0.8) if n > 0.6 else C
        if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
            return mul(c, 0.8) if p.face != "up" else L
        if (p.i - p.j) % 6 == 0:
            return L
        if (p.i + 2 * p.j) % 7 == 0:
            return D
        return c if p.face != "down" else D

    def shard(p):
        """A crystal point: the corners of the top are cut away."""
        c = crystal(p)
        cut = max(1, min(p.w, 6) // 2)
        if p.side() and p.j < cut and min(p.i, p.w - 1 - p.i) < cut - p.j:
            return c[:3] + (0,)
        if p.face == "up" and min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) < min(p.w, p.h) // 2 - (0 if min(p.w, p.h) % 2 else 1):
            return c[:3] + (0,)
        return c

    def torso_d(c):
        if c.face == "north":
            x, y = c.w // 2 - 2, 3
            c.rect(x, y, 4, 4, D)
            c.rect(x + 1, y, 2, 4, CORE), c.rect(x, y + 1, 4, 2, CORE)
            c.px(x + 1, y + 1, (255, 214, 120)), c.px(x + 2, y + 2, (255, 214, 120))

    def head_d(c):
        if c.face == "north":
            c.rect(0, 2, c.w, 3, D)
            c.symrect(1, 3, 2, 1, EY)
            c.sym(1, 3, CORE)
        elif c.face in ("east", "west"):
            c.rect(0, 2, c.w, 1, D)

    m.box("pelvis", "Body", (-4, 9, -3), (4, 12.5, 3), crystal)
    m.box("torso", "Body", (-6.5, 12, -4), (6.5, 23, 4.5), crystal, torso_d)
    m.pair("shoulder-shard", "Body", (-10.5, 19.5, -2.5), (-6, 29, 2.5), shard, rot=(0, 0, 24), origin=(-7.5, 20, 0))
    m.box("back-shard-a", "Body", (-2, 17, 3.5), (2, 31, 7.5), shard, rot=(24, 0, 0), origin=(0, 18, 4.5))
    m.box("back-shard-b", "Body", (-5.5, 15, 3.6), (-2.5, 25, 6.6), shard, rot=(30, 0, 18), origin=(-4, 16, 4.5))
    m.box("back-shard-c", "Body", (2.5, 14, 3.6), (5.5, 23, 6.6), shard, rot=(34, 0, -20), origin=(4, 15, 4.5))
    m.bone("Head", (0, 23, -0.5))
    m.box("head", "Head", (-3, 23, -3.6), (3, 29.6, 2.4), crystal, head_d)
    m.box("head-shard", "Head", (-1.5, 29.2, -2), (1.5, 34.5, 1), shard, rot=(8, 0, 6), origin=(0, 29.4, -0.5))
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 8.4 * sx, 3.0 * sx
        m.bone("Arm-" + tag, (ax, 21, 0.2))
        m.box("arm-" + tag.lower(), "Arm-" + tag, (ax - 1.6, 13, -1.8), (ax + 1.6, 21.5, 2.2), crystal)
        m.box("fist-" + tag.lower(), "Arm-" + tag, (ax - 2.4, 4.5, -2.8), (ax + 2.4, 13.5, 3.2), crystal)
        m.box("fist-shard-" + tag.lower(), "Arm-" + tag, (ax + sx * 1.6 - 1, 9, -1), (ax + sx * 1.6 + 1, 17, 1), shard, rot=(0, 0, -20 * sx), origin=(ax + sx * 1.6, 10, 0))
        m.bone("Leg-" + tag, (lx, 9.5, 0.2))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 2, 0, -2.4), (lx + 2, 10, 2.6), crystal)
        m.box("foot-" + tag.lower(), "Leg-" + tag, (lx - 2.6, 0, -3.6), (lx + 2.6, 2.4, 3), crystal)
    return m


def wizard():
    m = Model("wizard", 511)
    RB, RD, GD, GDD, SK, SD, BD, BS, BT, WD, ORB = ((72, 92, 184), (48, 62, 134), (238, 200, 84), (190, 150, 50), (240, 200, 170), (212, 164, 138),
                                                    (242, 242, 238), (204, 206, 212), (84, 58, 42), (130, 92, 56), (140, 240, 255))

    def star(p, seed):
        return hsh(seed, int(math.floor(p.x * 0.5 + 40)), int(math.floor(p.y * 0.5)), int(math.floor(p.z * 0.5 + 40))) > 0.86 and (p.i + p.j) % 2 == 0

    def robe(p):
        if p.part == "skirt" and p.side() and p.j == p.h - 1:
            return GD if p.i % 2 else GDD
        if p.part == "torso" and p.side() and abs(p.y - 13.6) < 0.6:
            return GD if p.i % 3 else GDD
        if p.part == "torso" and p.face == "north" and abs(p.x) < 0.6:
            return RD
        if star(p, 511):
            return GD
        return RD if int(math.floor(p.x * 0.9 + p.z * 0.9 + 100)) % 4 == 0 else RB

    def sleeve(p):
        if p.side() and p.j >= p.h - 2:
            return GD if p.j == p.h - 2 else RD
        return robe(p)

    def leg(p):
        return BT if p.y < 2.2 else RD

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 2, (50, 80, 150), WHITE)
            c.symrect(1, 1, 2, 1, BD)
            c.rect(2, 3, 2, 2, SD)
            c.rect(1, 5, c.w - 2, 1, BD), c.rect(0, 6, c.w, 1, BD)
        elif c.face in ("east", "west"):
            c.rect(0, 5, c.w, 2, BD)
            c.rect(c.fx(1) if c.face == "west" else c.fx(1) - 1, 3, 2, 1, SD)

    def beard(p):
        return BS if (p.i + p.j // 2) % 3 == 0 else BD

    def hat(p):
        if p.part == "hat-a" and p.side() and p.j == p.h - 1:
            return GD if p.i % 2 else GDD
        if star(p, 512):
            return GD
        return RD if p.part == "hat-brim" and p.face != "up" else RB if (p.i + p.j) % 4 else RD

    def orb(p):
        d = abs(p.i - (p.w - 1) / 2.0) + abs(p.j - (p.h - 1) / 2.0)
        return (236, 255, 255) if d < 1 else ORB

    m.box("torso", "Body", (-3.8, 12, -2.2), (3.8, 21.5, 2.2), robe)
    m.box("skirt", "Body", (-4.3, 4, -2.7), (4.3, 12.6, 2.9), robe, skip=("down",))
    m.bone("Head", (0, 21.5, 0))
    m.box("head", "Head", (-3.2, 21.5, -3.2), (3.2, 28, 3.2), SK, head_d)
    m.box("beard", "Head", (-2.8, 14.5, -4), (2.8, 23, -3), ragged_hem(511, beard, 3), skip=("down",))
    m.box("hair", "Head", (-3.5, 19.5, 2.4), (3.5, 27, 3.7), ragged_hem(512, beard, 2), skip=("down",))
    m.box("hat-brim", "Head", (-5.4, 27.4, -5.4), (5.4, 28.4, 5.4), hat)
    m.box("hat-a", "Head", (-3.5, 28.2, -3.5), (3.5, 32, 3.5), hat)
    m.box("hat-b", "Head", (-2.4, 31.8, -2.4), (2.4, 35.6, 2.4), hat)
    m.box("hat-c", "Head", (-1.3, 35.2, -1.1), (1.3, 39, 1.5), hat, rot=(28, 0, 0), origin=(0, 35.3, 0.2))
    for tag, sx in (("L", -1), ("R", 1)):
        ax, lx = 5.0 * sx, 1.9 * sx
        m.bone("Arm-" + tag, (ax, 20.5, 0))
        m.box("sleeve-" + tag.lower(), "Arm-" + tag, (ax - 1.4, 12.5, -1.6), (ax + 1.4, 21.4, 1.6), sleeve)
        m.box("hand-" + tag.lower(), "Arm-" + tag, (ax - 0.8, 10.8, -0.9), (ax + 0.8, 12.8, 0.9), SK)
        m.bone("Leg-" + tag, (lx, 6, 0))
        m.box("leg-" + tag.lower(), "Leg-" + tag, (lx - 1.5, 0, -1.6), (lx + 1.5, 6.5, 1.6), leg)
        m.box("toe-" + tag.lower(), "Leg-" + tag, (lx - 1.4, 0, -2.8), (lx + 1.4, 1.6, -1.4), BT)
    m.box("staff", "Arm-R", (4.5, 6, -2.4), (5.5, 25, -1.4), planks(WD, lambda q: q.y, lambda q: q.x, 4, 511))
    m.box("staff-orb", "Arm-R", (3.8, 24.6, -3.1), (6.2, 27, -0.7), orb, noise=0)
    return m


def familiar_cat():
    m = Model("familiar_cat", 512)
    K, D, W, PK, EY, HT, HD, GD = (70, 64, 92), (44, 40, 62), (240, 238, 244), (232, 150, 170), (150, 240, 110), (128, 76, 178), (88, 50, 130), (244, 208, 90)

    def body(p):
        if p.face == "up" and abs(p.x) < 0.8:
            return D
        return D if vnoise(512, p.x / 2.0, p.y / 2.0, p.z / 3.0) > 0.62 else K

    def chest(p):
        if p.face == "north" and abs(p.x) < 1.4 and p.ly < 0.7:
            return W
        return body(p)

    def head_d(c):
        if c.face == "north":
            c.sym(1, 2, EY), c.sym(2, 2, BLACK)
            c.sym(1, 1, D)
            c.rect(3, 4, 1, 1, W)
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 4, W), c.px(c.fx(0), 3, (200, 200, 210))

    def muzzle_d(c):
        if c.face == "north":
            c.px(1, 0, PK)
            c.px(1, 1, (120, 100, 120))

    def hat(p):
        if p.part == "hat-a" and p.side() and p.j == p.h - 1:
            return GD
        return HD if (p.i + p.j) % 3 == 0 else HT

    m.box("body", "Body", (-2.5, 6, -7), (2.5, 11.5, 7), chest)
    TP = (0, 10.2, 6.5)
    m.bone("tail-a", TP)
    m.box("tail-a", "tail-a", (-1, 9.2, 6.5), (1, 11.2, 12), body, rot=(-40, 0, 0), origin=TP)
    TB = rotp((0, 10.2, 11.8), (-40, 0, 0), TP)
    m.bone("tail-b", TB, "tail-a")
    m.box("tail-b", "tail-b", (-0.9, TB[1] - 0.9, TB[2] - 0.3), (0.9, TB[1] + 0.9, TB[2] + 5), lambda p: W if p.lz > 0.7 else K, rot=(-75, 0, 0), origin=TB)
    m.bone("Head", (0, 11, -7))
    m.box("head", "Head", (-3.5, 8.5, -12), (3.5, 14.5, -6.5), body, head_d)
    m.box("muzzle", "Head", (-1.5, 9, -13), (1.5, 11, -11.5), mix(K, W, 0.35), muzzle_d)
    m.box("charm", "Head", (-0.7, 7.3, -11), (0.7, 8.7, -10.2), GD, lambda c: c.face == "north" and c.px(0, 0, WHITE), noise=0)
    ears(m, (-3.5, 14, -9.2), (-1.7, 16.6, -8), pointed(K, PK, D))
    HP, HR = (0, 14.4, -10.2), (-10, 0, -8)
    m.box("hat-brim", "Head", (-2.9, 14.4, -12.6), (2.9, 15.1, -8.4), hat, rot=HR, origin=HP)
    m.box("hat-a", "Head", (-1.7, 15, -11.6), (1.7, 17.6, -9), hat, rot=HR, origin=HP)
    m.box("hat-b", "Head", (-0.9, 17.4, -11), (0.9, 20.2, -9.4), hat, rot=(-10, 0, -30), origin=(0, 17.2, -10.2))
    four_legs(m, -2.5, -0.7, -6.5, 4.5, 2, 6, lambda p: W if p.y < 1.6 else K)
    return m


def spellbook():
    m = Model("spellbook", 513, noise=5)
    LE, LD, GD, GDD, PG, PS, INK, RB, EY = (126, 52, 78), (88, 34, 56), (238, 198, 92), (188, 148, 54), (246, 238, 212), (214, 204, 176), (96, 74, 66), (206, 52, 62), (150, 240, 255)

    def cover(p):
        if p.face in ("up", "down"):
            ei, ej = min(p.i, p.w - 1 - p.i), min(p.j, p.h - 1 - p.j)
            if ei + ej < 3:
                return GD if ei + ej < 2 else GDD
            if p.face == "down":
                d = abs(p.i - (p.w - 1) / 2.0) + abs(p.j - (p.h - 1) / 2.0)
                if abs(d - 2.5) < 0.6 or d < 0.6:
                    return GD
            if ei == 1 or ej == 1:
                return LD
        return LD if (p.i + p.j) % 5 == 0 else LE

    def pages(p):
        if p.face == "up":
            if min(p.i, p.w - 1 - p.i) == 0 or min(p.j, p.h - 1 - p.j) == 0:
                return PG
            if p.x < 0:
                # a column of text on the left page
                return INK if p.j % 2 == 1 and hsh(513, p.i, p.j) > 0.25 else PG
            d = abs(p.i - (p.w - 1) / 2.0) + abs(p.j - 4)
            if p.j < 8:
                return (150, 70, 170) if abs(d - 2.2) < 0.6 else (230, 170, 60) if d < 0.6 else PG
            return INK if p.j % 2 == 1 and hsh(514, p.i, p.j) > 0.3 else PG
        return PS if p.j % 2 else PG

    def spine(p):
        if p.face in ("up", "down") and p.j % 4 == 1:
            return GD
        return LD if p.face == "up" else LE

    def clasp_d(c):
        if c.face == "north":
            c.rect(0, 0, c.w, c.h, GDD)
            c.rect(1, 1, c.w - 2, c.h - 2, EY)
            c.px(c.w // 2, c.h // 2, BLACK)

    def ribbon(p):
        if p.j >= p.h - 2 and p.i == p.w // 2 and p.face in ("north", "south"):
            return RB + (0,)
        return mul(RB, 0.82) if p.j % 3 == 0 else RB

    WP, WR = (-0.8, 8.6, 0), (0, 0, -20)
    m.box("spine", "Body", (-1, 8, -6.6), (1, 9.2, 6.6), spine)
    m.box("clasp", "Body", (-1.5, 7.6, -7.4), (1.5, 10.6, -6.5), GD, clasp_d)
    wings(m, WP, (-9.8, 8.2, -6.6), (-0.8, 9, 6.6), cover, rot=WR, name="cover")
    m.pair("pages", ("wing-l", "wing-r"), (-9.3, 9, -6), (-0.8, 10.8, 6), pages, rot=WR, origin=WP, skip=("down",))
    m.bone("tail", (0, 8.2, 6.3))
    m.box("ribbon", "tail", (-1, 0, 6.2), (1, 8.4, 6.6), ribbon, skip=("up", "down", "east", "west"))
    for k, (x, y, z) in enumerate(((-5, 15.5, -2), (4, 17, 2.5), (0.5, 14, -4.5))):
        m.box("spark-%d" % k, "Body", (x - 0.5, y - 0.5, z - 0.5), (x + 0.5, y + 0.5, z + 0.5), (255, 244, 170) if k % 2 else EY, noise=0)
    return m


SPELLS_N_MAGIC = [dragon, wyvern, phoenix, unicorn, griffin, wisp, imp, mimic, fairy, golem_crystal, wizard, familiar_cat, spellbook]


# >>> titanlands pack
# ---------------------------------------------------------------- the Titanlands pack
#
# Giants, dragons and prehistoric beasts, written to mods/titanlands/models/.
# The dragons, the stag and the moth are pale so that entities can tint them.

TITAN_DIR = os.path.join(MODS_DIR, "titanlands", "models")


def banded(base, dark, period=4, axis="z", belly=None, belly_y=-1e9):
    """Rings: a dark groove and a highlight every `period` texels along an axis."""
    def sh(p):
        if belly is not None and (p.face == "down" or p.y < belly_y):
            return belly
        k = int(math.floor((p.x, p.y, p.z)["xyz".index(axis)] + 100)) % period
        if k == 0:
            return dark
        return mix(base, WHITE, 0.14) if k == 1 else base
    return sh


def cracked(seed, rock, dark, seam, glow, scale=4.2):
    """Broken rock with glowing seams where a noise field crosses its middle."""
    def sh(p):
        d = abs(vnoise(seed, p.x / scale, p.y / scale, p.z / scale) - 0.5)
        if d < 0.011:
            return glow
        if d < 0.026:
            return seam
        return dark if vnoise(seed + 3, p.x / 1.7, p.y / 1.7, p.z / 1.7) > 0.6 else rock
    return sh


def armour(base, trim, dark, period=5):
    """Lapped plates: a shadow line, a trim line and rivets every `period` texels up."""
    def sh(p):
        if p.face in ("up", "down"):
            return base if (p.i + p.j) % 2 else mul(base, 0.94)
        k = int(math.floor(p.y + 100)) % period
        if k == 0:
            return dark
        if k == period - 1:
            return trim
        if k == period // 2 and p.i % 4 == 1:
            return mix(base, WHITE, 0.4)
        return base
    return sh


def crystal(base, light, dark):
    """Facets: diagonal bands of light and shade."""
    def sh(p):
        k = (p.i - p.j) % 4
        if p.face == "up" or k == 0:
            return light
        return mix(base, dark, 0.6) if k == 2 else base
    return sh


def tl_jaw(skin, under, mouth=(170, 66, 78), tooth=WHITE):
    """A lower jaw: tongue on top, a row of teeth along the upper edge."""
    def sh(p):
        if p.face == "up":
            return mouth
        if p.face in ("east", "west", "north") and p.j == 0 and p.i % 2 == 1:
            return tooth
        return under if p.face == "down" else skin
    return sh


def tl_teeth(col=WHITE, step=2, also=None):
    """Decal: upper teeth along the bottom edge of a snout."""
    def d(c):
        if c.face in ("east", "west", "north"):
            for x in range(0, c.w, step):
                c.px(x, c.h - 1, col)
        if also:
            also(c)
    return d


def taper(shader, x0, span, keep=0.2):
    """Wraps a wing shader so the plate narrows to `keep` of its depth at the tip."""
    def sh(p):
        c = shader(p)
        if p.face in ("up", "down") and p.lz > 1 - (1 - keep) * (abs(p.x) - x0) / span:
            return tuple(c[:3]) + (0,)
        return c
    return sh


def tl_chain(m, prefix, segs, shader, rot=None, origin=None, parent="Body"):
    """A chain of boxes running back along +z, each on a bone that is the child of
    the one before. segs: (half width, y0, y1, z0, z1); shader(k, y0) makes each one's."""
    for k, (w, y0, y1, z0, z1) in enumerate(segs):
        name = "%s-%s" % (prefix, "abcdefgh"[k])
        piv = (0, (y0 + y1) / 2, z0 + 0.5)
        m.bone(name, rotp(piv, rot, origin) if rot else piv, parent)
        m.box(name, name, (-w, y0, z0), (w, y1, z1), shader(k, y0), rot=rot, origin=origin)
        parent = name


def tl_wings(m, wp, wr, span, z0, z1, web, tip_span, tz0, tz1, tip_web, tip_rot=(0, 0, 6), elbow=0.0, th=0.4):
    """Two-part wings: an inner plate on wing-l / wing-r (pivot wp, the shoulder) and
    an outer one on wing-l-tip / wing-r-tip. z ranges are relative to the shoulder
    and to the elbow; tip_web(x0) makes the outer shader. Returns the left elbow."""
    x, y, z = wp
    wings(m, wp, (x - span, y - th, z + z0), (x, y + th, z + z1), web, rot=wr, skip=("south",), name="wing-in")
    el = rotp((x - span, y, z + elbow), wr, wp)
    m.bone("wing-l-tip", el, "wing-l")
    m.bone("wing-r-tip", (-el[0], el[1], el[2]), "wing-r")
    m.pair("wing-out", ("wing-l-tip", "wing-r-tip"), (el[0] - tip_span, el[1] - th + 0.02, el[2] + tz0),
           (el[0], el[1] + th - 0.02, el[2] + tz1), tip_web(-el[0]), rot=tip_rot, origin=el, skip=("south", "east", "west"))
    return el


def titan():
    m = Model("titan", 601, noise=6)
    ST, TR, DK, GL, HN, HD = (156, 160, 174), (208, 174, 94), (68, 70, 86), (120, 226, 255), (228, 216, 188), (124, 108, 92)
    plate = armour(ST, TR, DK, 5)

    def chest(p):
        d = abs(p.x) + abs(p.y - 28)
        if p.face == "north" and d < 3.4:
            return GL if d < 1.8 else TR
        return plate(p)

    def mail(p):
        return DK if (p.i + p.j) % 2 else mul(DK, 1.4)

    def helm_d(c):
        if c.face == "north":
            c.rect(1, 3, c.w - 2, 2, BLACK)
            c.sym(2, 3, GL), c.sym(2, 4, mul(GL, 0.6))
            c.rect(c.w // 2 - 1, 1, 2, c.h - 1, TR)
            c.sym(1, 6, DK), c.sym(2, 6, DK)
        elif c.face in ("east", "west"):
            for k in range(3):
                c.px(c.fx(k), 3, BLACK)
            c.rect(0, c.h - 1, c.w, 1, TR)
        elif c.face == "up":
            c.rect(c.w // 2 - 1, 0, 2, c.h, TR)

    def boot(p):
        return DK if p.y < 1 else TR if p.y > 2.2 else ST

    def vambrace(p):
        if p.side() and p.j in (0, p.h - 1):
            return TR
        if p.side() and p.i % 3 == 1 and 2 < p.j < p.h - 3:
            return mul(ST, 0.8)
        return ST

    def fist_d(c):
        if c.face in ("north", "east", "west"):
            for x in range(1, c.w, 2):
                c.px(x, c.h - 2, TR)

    legs, arms = ("Leg-L", "Leg-R"), ("Arm-L", "Arm-R")
    m.bone("Leg-L", (-3.5, 18, 0)), m.bone("Leg-R", (3.5, 18, 0))
    m.pair("leg", legs, (-6, 0, -3), (-1, 19, 3), plate)
    m.pair("boot", legs, (-6.6, 0, -5), (-0.4, 3, 3.6), boot)
    m.pair("knee", legs, (-6.4, 10.2, -4), (-0.6, 13.6, -2.6), TR, lambda c: c.face == "north" and c.rect(2, 1, c.w - 4, 1, HN))
    m.box("waist", "Body", (-6, 17, -3.5), (6, 22, 3.5), mail, lambda c: c.side() if False else c.rect(0, 0, c.w, 1, TR))
    m.box("tasset", "Body", (-3, 10, -4.4), (3, 18, -3.4), plate)
    m.pair("tasset-side", "Body", (-7.2, 12, -3), (-6, 19, 3), plate)
    m.box("chest", "Body", (-9, 21, -5), (9, 34, 5), chest)
    m.box("gorget", "Body", (-5, 33, -4.8), (5, 35.5, 4.6), lambda p: TR if p.j == 0 else ST)
    m.pair("pauldron", "Body", (-14.5, 31, -4.8), (-8.4, 37, 4.8), lambda p: TR if p.side() and p.j == p.h - 1 else plate(p))
    m.pair("pauldron-spike", "Body", (-12.6, 37, -1), (-11, 40.5, 1), lambda p: mix(HD, HN, p.ly))
    m.bone("Arm-L", (-11.5, 32, 0)), m.bone("Arm-R", (11.5, 32, 0))
    m.pair("arm", arms, (-13.6, 24, -2.5), (-9.4, 32, 2.5), mail)
    m.pair("forearm", arms, (-15.2, 11, -3.8), (-8.4, 24, 3.8), vambrace)
    m.pair("fist", arms, (-14.6, 6, -3.2), (-9, 11.5, 3.2), DK, fist_d)
    m.bone("Head", (0, 34.5, 0))
    m.box("helm", "Head", (-4, 34.5, -4.5), (4, 42.5, 4), ST, helm_d)
    m.pair("horn", "Head", (-7.6, 39, -1.2), (-4, 41, 1.2), lambda p: HD if int(p.x + 100) % 2 else mul(HD, 1.2))
    m.pair("horn-tip", "Head", (-8.2, 40.4, -1), (-6.4, 46.5, 1), lambda p: mix(HD, HN, p.ly))
    m.box("crest", "Head", (-0.5, 42.3, -4), (0.5, 45, 4.5), spines(TR, (250, 232, 170), "z", 3), noise=3)
    return m


def behemoth():
    m = Model("behemoth", 602)
    HD, HK, BL, PL, PE, IV, HF, EY = (128, 136, 150), (96, 104, 120), (178, 172, 162), (152, 114, 80), (104, 76, 54), (238, 228, 200), (58, 54, 58), (255, 196, 60)
    hide = fur(602, HD, HK, BL, 12.5, scale=3.5, thr=0.6, amt=0.7)

    def plate(p):
        if p.face == "up":
            e = min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j)
            return PE if e == 0 else mix(PL, WHITE, 0.22) if (p.i + 2 * p.j) % 5 == 0 else PL
        return PE if p.j == p.h - 1 else PL

    def leg(p):
        if p.y < 2:
            return IV if p.face == "north" and p.i % 2 == 0 else HF
        return HK if int(math.floor(p.y)) % 4 == 0 else HD

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(3), 4, EY), c.px(c.fx(2), 4, BLACK)
            for k in range(1, 5):
                c.px(c.fx(k), 3, HK)
        elif c.face == "north":
            c.sym(0, 4, EY)

    def snout_d(c):
        if c.face == "north":
            c.sym(2, 1, BLACK)
            c.rect(1, c.h - 2, c.w - 2, 1, (92, 52, 56))
        elif c.face in ("east", "west"):
            c.rect(0, c.h - 2, c.w, 1, (92, 52, 56))

    m.box("body", "Body", (-8, 10, -12), (8, 25, 12), hide)
    m.box("plate-a", "Body", (-6.5, 25, -11.5), (6.5, 27, -4.5), plate)
    m.box("plate-b", "Body", (-7.5, 25, -4), (7.5, 28.5, 4), plate)
    m.box("plate-c", "Body", (-6.5, 25, 4.5), (6.5, 27, 11.5), plate)
    m.box("ridge", "Body", (-0.6, 26.8, -10), (0.6, 31, 10), spines(PE, IV, "z", 5), noise=3)
    m.pair("flank", "Body", (-8.9, 17, -9), (-7.9, 24, 9), plate)
    m.bone("Head", (0, 19, -12))
    m.box("head", "Head", (-6, 11, -21), (6, 23, -11.5), hide, head_d)
    m.box("brow", "Head", (-5, 22, -20.5), (5, 25, -13), plate)
    m.box("snout", "Head", (-4.5, 9.5, -25.5), (4.5, 17, -20.5), hide, snout_d)
    m.box("nose-horn", "Head", (-1, 16.5, -25), (1, 21.5, -22.6), lambda p: mix((170, 150, 122), IV, p.ly))
    m.pair("tusk", "Head", (-7, 9.6, -25), (-4.8, 12, -18.5), lambda p: mix(IV, (170, 150, 122), p.lz))
    m.pair("tusk-tip", "Head", (-7.2, 11, -27.6), (-5, 18.5, -25), lambda p: mix((200, 184, 150), WHITE, p.ly))
    ears(m, (-9, 17.5, -14), (-6, 21.5, -13), HK)
    four_legs(m, -8.5, -3, -11.5, 5.5, 6, 10, leg)
    m.bone("tail", (0, 22, 12))
    m.box("tail", "tail", (-1.2, 14, 11.6), (1.2, 22.5, 13.6), HK)
    m.box("tail-club", "tail", (-2, 11, 11), (2, 15, 14.4), plate)
    return m


def hydra():
    m = Model("hydra", 603, noise=6)
    SC, SE, BL, SP, EY = (74, 146, 128), (44, 104, 98), (226, 214, 150), (232, 150, 70), (255, 230, 90)
    hide, top = scaly(SC, SE, BL, 9.5), scaly(SC, SE)

    def throat(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.88)
        return top(p)

    def leg(p):
        return SE if p.y < 1.2 or (p.i + 2 * p.j) % 4 == 0 else SC

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, EY), c.px(c.fx(3), 1, BLACK)
            for k in range(1, 5):
                c.px(c.fx(k), 0, SE)
        elif c.face == "north":
            c.sym(0, 1, EY)

    m.box("body", "Body", (-7, 7, -9), (7, 18, 10), hide)
    m.box("hump", "Body", (-5.5, 18, -8), (5.5, 20, 6), top)
    m.box("back-spines", "Body", (-0.5, 19.8, -3), (0.5, 22.5, 10), spines(SP, BL), noise=3)
    four_legs(m, -9, -4.5, -8.5, 4.5, 4.5, 8, leg)
    m.pair("foot-f", ("Leg-FL", "Leg-FR"), (-9.5, 0, -10.5), (-4, 2, -3.5), leg, claws_d(BL))
    m.pair("foot-b", ("Leg-BL", "Leg-BR"), (-9.5, 0, 2.5), (-4, 2, 9.5), leg, claws_d(BL))

    def neck(bone, head, piv, rot, length, yaw):
        x, y, z = piv
        m.bone(bone, piv)
        m.box(bone, bone, (x - 2.2, y - 1.5, z - 2.2), (x + 2.2, y + length, z + 2.2), throat, rot=rot, origin=piv)
        m.box(bone + "-fins", bone, (x - 0.45, y + 1, z + 2), (x + 0.45, y + length, z + 4), spines(SP, BL, "y"), rot=rot, origin=piv, noise=3)
        hp = rotp((x, y + length - 1, z), rot, piv)
        hx, hy, hz = hp
        m.bone(head, hp, bone)
        tag, hr = head.lower(), (0, yaw, 0)
        m.box(tag, head, (hx - 2.6, hy - 1, hz - 6.5), (hx + 2.6, hy + 4, hz + 1.5), top, head_d, rot=hr, origin=hp)
        m.box(tag + "-snout", head, (hx - 2, hy - 0.8, hz - 11), (hx + 2, hy + 2.6, hz - 6), top, tl_teeth(), rot=hr, origin=hp)
        m.box(tag + "-crest", head, (hx - 0.45, hy + 3.8, hz - 5), (hx + 0.45, hy + 6.2, hz + 2), spines(SP, BL, "z", 3), rot=hr, origin=hp, noise=3)
        jb, jp = head, hp
        if head == "Head":
            jb, jp = "jaw", (hx, hy - 0.9, hz + 0.5)
            m.bone("jaw", jp, "Head")
        m.box(tag + "-jaw", jb, (hx - 1.8, hy - 2.2, hz - 10.5), (hx + 1.8, hy - 0.9, hz + 0.5), tl_jaw(SC, BL), rot=(-9, yaw, 0), origin=jp)

    neck("neck", "Head", (0, 17, -7), (-28, 0, 0), 17, 0)
    neck("neck-l", "head-l", (-4.5, 16.5, -6.5), (-42, 0, 26), 14, 20)
    neck("neck-r", "head-r", (4.5, 16.5, -6.5), (-42, 0, -26), 14, -20)
    TP, TR = (0, 12, 10), (10, 0, 0)
    tl_chain(m, "tail", ((3.2, 9, 15, 9.5, 17), (2.3, 9.6, 14.2, 16.5, 24), (1.4, 10.2, 13.4, 23.5, 31)),
             lambda k, y0: scaly(SC, SE, BL, y0 + 1.2), TR, TP)
    m.box("tail-spines", "tail-a", (-0.45, 14.8, 10), (0.45, 16.8, 17), spines(SP, BL), rot=TR, origin=TP, noise=3)
    return m


def wyrm():
    m = Model("wyrm", 604, noise=6)
    SC, SE, BL, SP, EY, HN = (62, 124, 160), (36, 84, 124), (236, 222, 176), (236, 120, 60), (255, 214, 70), (232, 224, 200)
    top = scaly(SC, SE)

    def throat(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.88)
        return top(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(3), 2, EY), c.px(c.fx(4), 2, BLACK), c.px(c.fx(5), 2, EY)
            for k in range(2, 7):
                c.px(c.fx(k), 1, SE)
        elif c.face == "up":
            c.symrect(1, 2, 2, c.h - 3, SE)

    def snout_d(c):
        if c.face == "up":
            c.sym(1, 1, BLACK)
        elif c.face == "north":
            c.sym(1, 1, BLACK)

    def fin(p):
        a = 255 if abs(p.ly - 0.5) * 2 < 0.25 + 0.75 * math.sin(math.pi * min(1.0, p.lz * 1.15)) else 0
        return tuple(mix(SP, BL, p.lz)) + (a,)

    m.box("body", "Body", (-4, 0, -5), (4, 8, 5), scaly(SC, SE, BL, 1.6))
    m.box("body-spines", "Body", (-0.45, 7.8, -1), (0.45, 10.5, 5), spines(SP, BL), noise=3)
    NP, NR = (0, 5, -3.5), (-24, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-3.4, 3.5, -7), (3.4, 21, -0.5), throat, rot=NR, origin=NP)
    m.box("neck-spines", "neck", (-0.45, 7, -0.7), (0.45, 21, 1.6), spines(SP, BL, "y"), rot=NR, origin=NP, noise=3)
    hp = rotp((0, 20, -4), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-3.7, hy - 1.5, hz - 8), (3.7, hy + 4.5, hz + 2), top, head_d)
    m.box("snout", "Head", (-2.8, hy - 1.2, hz - 14.5), (2.8, hy + 3, hz - 7.5), top, tl_teeth(also=snout_d))
    m.bone("jaw", (0, hy - 1.4, hz + 0.5), "Head")
    m.box("jaw", "jaw", (-2.5, hy - 3, hz - 14), (2.5, hy - 1.4, hz + 0.5), tl_jaw(mul(SC, 0.92), BL), rot=(-10, 0, 0), origin=(0, hy - 1.4, hz + 0.5))
    m.pair("horn", "Head", (-3.2, hy + 3.6, hz - 2), (-1.8, hy + 5.2, hz + 6), lambda p: mix(HN, (140, 130, 110), p.lz),
           rot=(-22, 0, 0), origin=(-2.5, hy + 4.4, hz - 1.5))
    m.pair("frill", "Head", (-4.6, hy - 1, hz - 1.5), (-3.6, hy + 4, hz + 4), spines(SP, BL, "y", 3), noise=3)
    m.pair("barbel", "Head", (-3.3, hy - 5.5, hz - 13.6), (-2.7, hy - 0.8, hz - 13), lambda p: mix(SP, BL, 1 - p.ly), rot=(-14, 0, 8),
           origin=(-3, hy - 0.8, hz - 13.3))
    segs = ((3.6, 0, 7.2, 4.5, 14), (3.2, 0, 6.4, 13.5, 23), (2.7, 0, 5.4, 22.5, 32), (2.1, 0, 4.2, 31.5, 40), (1.4, 0, 2.8, 39.5, 48))
    tl_chain(m, "tail", segs, lambda k, y0: scaly(SC, SE, BL, 1.4))
    for k, (w, y0, y1, z0, z1) in enumerate(segs[:4]):
        m.box("tail-spines-" + "abcd"[k], "tail-" + "abcd"[k], (-0.45, y1 - 0.2, z0 + 1), (0.45, y1 + 2.4 - 0.3 * k, z1), spines(SP, BL), noise=3)
    m.box("tail-fin", "tail-e", (-0.4, 0, 45), (0.4, 6.5, 53), fin, noise=3)
    return m


def emberwing():
    m = Model("emberwing", 605, noise=6)
    SC, SE, BL, HN, MB, SP, EY = (230, 216, 202), (188, 162, 146), (248, 236, 206), (214, 198, 176), (240, 206, 188), (204, 172, 156), (255, 170, 40)
    hide, top = scaly(SC, SE, BL, 16.2), scaly(SC, SE)

    def leg(p):
        if p.y < 1.2:
            return SE
        return SE if (p.i + 2 * p.j) % 4 == 0 else SC if (p.i + p.j) % 2 else mul(SC, 0.94)

    def throat(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.88)
        return top(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(3), 2, EY), c.px(c.fx(4), 2, BLACK), c.px(c.fx(3), 3, mul(EY, 0.8))
            for k in range(2, 7):
                c.px(c.fx(k), 1, SP)
            c.rect(c.fx(1) - (1 if c.face == "east" else 0), 5, 2, 2, SE)   # cheek plate
        elif c.face == "up":
            c.symrect(1, 1, 2, c.h - 2, SE)
        elif c.face == "north":
            c.sym(0, 2, EY)

    def snout_d(c):
        if c.face in ("up", "north"):
            c.sym(1, 1, (84, 70, 66))
        if c.face in ("east", "west"):
            c.rect(0, c.h - 2, c.w, 1, mul(SE, 0.85))

    def horn(p):
        return mix(HN, (150, 128, 112), p.lz) if int(p.lz * 8) % 2 else mix(HN, (150, 128, 112), p.lz * 0.6)

    def club(p):
        return SE if (p.i + p.j) % 3 == 0 else SP

    m.box("body", "Body", (-6.5, 14, -10), (6.5, 25, 10), hide)
    m.box("shoulders", "Body", (-5.5, 25, -9.5), (5.5, 27, 1), top)
    m.box("back-spines", "Body", (-0.6, 26.8, -8), (0.6, 29.6, 1), spines(SP, BL, "z", 5), noise=3)
    m.box("rump-spines", "Body", (-0.6, 24.8, 1), (0.6, 27.2, 10), spines(SP, BL, "z", 5), noise=3)
    NP, NR = (0, 22, -9), (-34, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-3.6, 19.5, -13), (3.6, 33, -6.5), throat, rot=NR, origin=NP)
    m.box("neck-spines", "neck", (-0.6, 23, -6.7), (0.6, 33, -4.6), spines(SP, BL, "y", 5), rot=NR, origin=NP, noise=3)
    hp = rotp((0, 32, -9.5), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-4.6, hy - 2.5, hz - 8.5), (4.6, hy + 4.5, hz + 1.5), top, head_d)
    m.box("snout", "Head", (-3.6, hy - 2.2, hz - 14.5), (3.6, hy + 2.6, hz - 8), top, tl_teeth(also=snout_d))
    m.box("nose-horn", "Head", (-0.8, hy + 2.4, hz - 13.6), (0.8, hy + 5.4, hz - 11.6), lambda p: mix((150, 128, 112), HN, p.ly))
    m.bone("jaw", (0, hy - 2.3, hz + 0.5), "Head")
    m.box("jaw", "jaw", (-3.3, hy - 4.2, hz - 14), (3.3, hy - 2.4, hz + 0.5), tl_jaw(mul(SC, 0.92), BL), rot=(-8, 0, 0), origin=(0, hy - 2.3, hz + 0.5))
    m.pair("horn", "Head", (-4.6, hy + 3.2, hz - 2), (-2.8, hy + 5.4, hz + 7.5), horn, rot=(-30, 0, 0), origin=(-3.7, hy + 4.3, hz - 1.5))
    m.pair("cheek-horn", "Head", (-6, hy - 1.2, hz - 1.5), (-4.5, hy + 0.6, hz + 5), horn, rot=(0, 16, 0), origin=(-4.6, hy - 0.3, hz - 1.5))
    WP = (-6, 25.5, -4)
    tl_wings(m, WP, (0, 0, -16), 13, -5, 8, membrane(6, 4, SC, SE, MB), 13, -4.5, 6.5,
             lambda x0: membrane(x0, 4, SC, SE, MB, 0.36), elbow=1.0)
    four_legs(m, -8.5, -4.5, -9.5, 5, 4.5, 14, leg)
    m.pair("foot-f", ("Leg-FL", "Leg-FR"), (-9, 0, -12), (-4, 2.4, -4.5), leg, claws_d(WHITE))
    m.pair("foot-b", ("Leg-BL", "Leg-BR"), (-9, 0, 2.5), (-4, 2.4, 10), leg, claws_d(WHITE))
    m.pair("haunch", ("Leg-BL", "Leg-BR"), (-8.9, 11, 4), (-6.2, 20, 10.4), top)
    TP, TR = (0, 20, 10), (10, 0, 0)
    tl_chain(m, "tail", ((3.6, 16, 23, 9.5, 18), (2.7, 16.6, 22, 17.5, 26), (1.9, 17.2, 21.2, 25.5, 33.5), (1.3, 17.7, 20.5, 33, 40)),
             lambda k, y0: scaly(SC, SE, BL, y0 + 1.2), TR, TP)
    m.box("tail-spines", "tail-a", (-0.5, 22.8, 10.5), (0.5, 25, 18), spines(SP, BL, "z", 5), rot=TR, origin=TP, noise=3)
    m.box("tail-club", "tail-d", (-2.4, 17.2, 39), (2.4, 21.2, 43.5), club, rot=TR, origin=TP)
    m.pair("tail-club-spike", "tail-d", (-4.4, 18.4, 40.4), (-2.2, 20, 42.4), lambda p: mix(HN, (150, 128, 112), 1 - abs(p.lx - 0.5) * 2), rot=TR, origin=TP)
    return m


def stormwing():
    m = Model("stormwing", 606, noise=5)
    SC, SE, BL, MB, SP, EY, CL = (216, 228, 238), (164, 188, 210), (242, 242, 230), (204, 222, 238), (176, 204, 232), (255, 232, 90), (96, 104, 120)
    top = scaly(SC, SE)

    def throat(p):
        if p.face == "north":
            return BL if p.j % 2 else mul(BL, 0.9)
        return top(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, EY), c.px(c.fx(3), 1, BLACK)
            for k in range(1, 6):
                c.px(c.fx(k), 0, SE)
        elif c.face == "north":
            c.sym(0, 1, EY)

    def leg(p):
        return SE if (p.i + 2 * p.j) % 4 == 0 else SC

    def blade(p):
        a = 255 if abs(p.ly - 0.5) * 2 < (p.lz * 2.4 if p.lz < 0.4 else (1 - p.lz) * 1.7) + 0.1 else 0
        return tuple(mix(SP, WHITE, p.lz * 0.6)) + (a,)

    m.box("body", "Body", (-3, 13, -8), (3, 19, 8), scaly(SC, SE, BL, 14.4))
    m.box("keel", "Body", (-2, 11.6, -7.5), (2, 14, -1), lambda p: BL if int(math.floor(p.z + 100)) % 2 else mul(BL, 0.9))
    m.box("back-fins", "Body", (-0.4, 18.8, -6), (0.4, 22.4, 8), spines(SP, WHITE, "z", 5), noise=3)
    NP, NR = (0, 18, -7), (-38, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-1.8, 16.5, -9.4), (1.8, 31, -5.4), throat, rot=NR, origin=NP)
    m.box("neck-fins", "neck", (-0.4, 19, -5.6), (0.4, 31, -3.4), spines(SP, WHITE, "y", 4), rot=NR, origin=NP, noise=3)
    hp = rotp((0, 30, -7.4), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-2.3, hy - 1.5, hz - 6.5), (2.3, hy + 3.5, hz + 1.5), top, head_d)
    m.box("snout", "Head", (-1.6, hy - 1.2, hz - 13), (1.6, hy + 2, hz - 6), top, tl_teeth())
    m.bone("jaw", (0, hy - 1.4, hz + 0.5), "Head")
    m.box("jaw", "jaw", (-1.4, hy - 2.6, hz - 12.5), (1.4, hy - 1.4, hz + 0.5), tl_jaw(mul(SC, 0.92), BL), rot=(-8, 0, 0), origin=(0, hy - 1.4, hz + 0.5))
    m.box("crest", "Head", (-0.4, hy + 3.3, hz - 5.5), (0.4, hy + 8, hz + 4.5), spines(SP, WHITE, "z", 5), rot=(-12, 0, 0), origin=(0, hy + 3.3, hz), noise=3)
    m.pair("crest-side", "Head", (-2.7, hy + 1.5, hz - 2.5), (-2.1, hy + 6.5, hz + 4.5), spines(SP, WHITE, "z", 4), rot=(-12, 0, 32),
           origin=(-2.3, hy + 2, hz), noise=3)
    WP = (-3, 18.6, -5)
    tl_wings(m, WP, (0, 14, -12), 16, -3.5, 6.5, membrane(3, 4, SC, SE, MB), 21, -2.5, 5.5,
             lambda x0: taper(membrane(x0, 4, SC, SE, MB, 0.3), x0, 21), tip_rot=(0, 24, 5), elbow=0.5)
    for tag, sx in (("L", -1), ("R", 1)):
        lx, t = 3.2 * sx, tag.lower()
        m.bone("Leg-" + tag, (lx, 14.5, 3.5))
        m.box("thigh-" + t, "Leg-" + tag, (lx - 1.3, 8, 1), (lx + 1.3, 15.5, 6), leg)
        m.box("shin-" + t, "Leg-" + tag, (lx - 0.8, 1, 3.2), (lx + 0.8, 9, 5.2), leg)
        m.box("foot-" + t, "Leg-" + tag, (lx - 1.5, 0, -0.6), (lx + 1.5, 1.4, 5.8), lambda p: SE if p.face == "up" else SC, claws_d(CL))
        m.bone("Arm-" + tag, (lx * 0.95, 14, -5.8))
        m.box("arm-" + t, "Arm-" + tag, (lx * 0.95 - 0.65, 8.5, -6.6), (lx * 0.95 + 0.65, 14.5, -5), leg)
        m.box("hand-" + t, "Arm-" + tag, (lx * 0.95 - 0.9, 7.6, -7.8), (lx * 0.95 + 0.9, 8.8, -5), SE, claws_d(CL))
    TP, TR = (0, 16, 8), (6, 0, 0)
    tl_chain(m, "tail", ((1.9, 14, 18, 7.5, 15), (1.4, 14.5, 17.5, 14.5, 23), (1.0, 15, 17, 22.5, 31), (0.7, 15.3, 16.7, 30.5, 39), (0.5, 15.5, 16.5, 38.5, 47)),
             lambda k, y0: scaly(SC, SE, BL, y0 + 0.8), TR, TP)
    m.box("tail-blade", "tail-e", (-0.35, 12.5, 45), (0.35, 19.5, 53), blade, rot=TR, origin=TP, noise=3)
    return m


def pocket_dragon():
    m = Model("pocket_dragon", 607, noise=5)
    SC, SE, BL, MB, HN, EYC, PK = (226, 238, 214), (182, 208, 176), (250, 244, 216), (246, 216, 204), (250, 240, 222), (70, 170, 210), (240, 170, 170)

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 2, 3, 3, EYC)
            c.symrect(1, 2, 2, 3, BLACK)
            c.sym(1, 2, WHITE)
        elif c.face in ("east", "west"):
            x = c.fx(1) - (1 if c.face == "east" else 0)
            c.rect(x, 2, 2, 3, EYC), c.rect(c.fx(0), 2, 1, 3, BLACK), c.px(c.fx(0), 2, WHITE)
            c.px(c.fx(2), 6, PK), c.px(c.fx(3), 6, PK)     # blush
        elif c.face == "up":
            c.rect(c.w // 2 - 1, 2, 2, c.h - 2, SE)

    def snout_d(c):
        if c.face == "north":
            c.sym(1, 0, (90, 96, 90))
            c.rect(1, c.h - 1, c.w - 2, 1, (150, 96, 100))
            c.sym(0, c.h - 1, WHITE)
        elif c.face == "up":
            c.sym(1, 0, (90, 96, 90))

    def leaf(p):
        if p.face in ("up", "down"):
            w = (p.lz * 2.6 if p.lz < 0.4 else (1 - p.lz) * 1.6) + 0.14
            if abs(p.lx - 0.5) * 2 > w:
                return SE + (0,)
        return MB if p.face == "up" else SE

    def leg(p):
        return SE if p.y < 1 else SC

    m.box("body", "Body", (-3.6, 3, -4), (3.6, 10, 4.6), scaly(SC, SE, BL, 5.4))
    m.box("belly", "Body", (-3, 2.4, -3.4), (3, 4, 3.8), lambda p: BL if int(math.floor(p.z + 100)) % 2 else mul(BL, 0.9))
    m.box("back-spines", "Body", (-0.4, 9.8, -2), (0.4, 11.6, 4.6), spines(MB, HN, "z", 3), noise=3)
    m.bone("Head", (0, 9, -3.5))
    m.box("head", "Head", (-4.5, 7.5, -11), (4.5, 15.5, -3), scaly(SC, SE), head_d)
    m.box("snout", "Head", (-2.5, 8, -13.4), (2.5, 11.4, -10.6), SC, snout_d)
    m.pair("horn", "Head", (-3.6, 15.3, -6.2), (-2, 17.6, -4.6), lambda p: mix(SE, HN, p.ly))
    ears(m, (-7, 10, -5.6), (-4.4, 14.5, -4.8), spines(MB, HN, "y", 3), pivot=(-4.5, 12, -5.2))
    wings(m, (-3.5, 9.6, -1), (-10, 9.3, -3.5), (-3.5, 9.9, 2.5), membrane(3.5, 3, SC, SE, MB, 0.34), rot=(0, 0, -32), skip=("south",))
    four_legs(m, -4.2, -1.6, -3.6, 1.6, 2.6, 3.5, leg, claws_d(WHITE), sink=0.5)
    tl_chain(m, "tail", ((1.6, 3.4, 6.6, 4, 9), (1.0, 3.9, 6, 8.5, 13.5)), lambda k, y0: scaly(SC, SE, BL, y0 + 0.9), (8, 0, 0), (0, 5, 4.5))
    m.box("tail-leaf", "tail-b", (-2, 4.6, 12.8), (2, 5.4, 17), leaf, rot=(8, 0, 0), origin=(0, 5, 4.5))
    return m


def mammoth():
    m = Model("mammoth", 608)
    FU, FD, FL, SK, SKD, IV, EY = (152, 106, 72), (104, 70, 48), (188, 142, 100), (134, 112, 100), (100, 82, 74), (240, 230, 204), (30, 22, 20)
    coat = shag(608, FU, FD, FL)

    def leg(p):
        if p.y < 1.6:
            return IV if p.face == "north" and p.i % 2 == 1 else SKD
        return coat(p)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(3), 5, EY), c.px(c.fx(4), 5, WHITE)
            c.px(c.fx(3), 4, FD), c.px(c.fx(4), 4, FD)
        elif c.face == "north":
            c.sym(0, 5, EY)

    def tusk(p):
        return mix(IV, (196, 178, 140), 0.5) if (p.i + p.j) % 5 == 0 else IV

    trunk = banded(SK, SKD, 3, "y")
    m.box("body", "Body", (-8, 13, -10), (8, 29, 12), coat)
    m.box("skirt", "Body", (-8.4, 8, -10.4), (8.4, 15, 12.4), ragged_hem(608, coat, 4), skip=("up", "down"))
    m.box("hump", "Body", (-6, 29, -10), (6, 33, -1), coat)
    m.box("rump", "Body", (-6.5, 29, -1), (6.5, 30.6, 10), coat)
    m.bone("Head", (0, 27, -10))
    m.box("head", "Head", (-5.5, 19, -18.5), (5.5, 31, -9.5), coat, head_d)
    m.box("dome", "Head", (-4, 31, -17), (4, 35, -10.5), shag(609, FD, mul(FD, 0.7), FU))
    ears(m, (-8.4, 21, -12.5), (-5.4, 27, -11.5), lambda p: SKD if p.face == "north" and 0.2 < p.lx < 0.8 and p.ly > 0.2 else FD,
         pivot=(-5.5, 26, -12))
    m.bone("trunk-a", (0, 21.5, -18), "Head")
    m.box("trunk-a", "trunk-a", (-2.2, 11, -21.2), (2.2, 22, -17.2), trunk)
    m.bone("trunk-b", (0, 11.5, -19.2), "trunk-a")
    m.box("trunk-b", "trunk-b", (-1.7, 2.5, -20.7), (1.7, 11.5, -17.7), trunk)
    m.box("trunk-tip", "trunk-b", (-1.7, 2.5, -23.2), (1.7, 5, -20.2), trunk, lambda c: c.face == "north" and c.sym(0, 1, (70, 50, 50)))
    m.pair("tusk-a", "Head", (-5.7, 13, -20.6), (-3.5, 20.5, -18.2), tusk)
    m.pair("tusk-b", "Head", (-5.7, 11, -28), (-3.5, 13.4, -18.6), tusk)
    m.pair("tusk-c", "Head", (-5.3, 12.6, -30), (-3.4, 20.5, -27.8), tusk)
    m.pair("tusk-d", "Head", (-4.4, 19.6, -29.6), (-2.6, 21.2, -25), lambda p: mix(WHITE, IV, p.lz))
    four_legs(m, -8, -3.5, -9.5, 6.5, 5, 13, leg)
    m.bone("tail", (0, 26, 12))
    m.box("tail", "tail", (-0.8, 17, 11.8), (0.8, 27, 13.2), FD)
    m.box("tail-tuft", "tail", (-1.3, 14.5, 11.5), (1.3, 18, 13.6), ragged_hem(610, mul(FD, 0.7), 2))
    return m


def sabertooth():
    m = Model("sabertooth", 609)
    TW, DK, BL, IV, NS, PK, EY = (208, 162, 98), (130, 88, 52), (240, 224, 190), (246, 240, 220), (60, 44, 44), (226, 150, 140), (150, 220, 120)

    def coat(p):
        if p.face == "down" or p.y < 10.6:
            return BL
        if p.y > 13.4 and int(math.floor(p.z * 0.5 + 50)) % 3 == 0 and vnoise(609, p.x / 2.0, p.y / 2.0, p.z / 2.0) > 0.42:
            return DK
        return TW

    def leg(p):
        if p.y < 1.4:
            return BL
        return DK if p.y > 4 and int(math.floor(p.y)) % 3 == 0 and p.face in ("east", "west") else TW

    def head(p):
        if p.face == "down":
            return BL
        if p.face == "up" and abs(p.x) > 1 and p.j % 2 == 0:
            return DK
        return TW

    def head_d(c):
        if c.face == "north":
            c.eyes(1, 2, BLACK, EY)
            c.sym(1, 1, DK), c.sym(2, 1, DK)
        c.side_eye(1, 2, BLACK, EY)

    def fang(p):
        if p.j == p.h - 1 and p.face in ("north", "south") and p.i != p.w // 2:
            return IV + (0,)
        return mix(WHITE, IV, p.ly)

    m.box("body", "Body", (-4.5, 9, -9), (4.5, 17, 9), coat)
    m.box("shoulders", "Body", (-5, 9.5, -9.6), (5, 18.6, -3.4), coat)
    m.bone("Head", (0, 16, -9.5))
    m.box("head", "Head", (-4, 12.5, -16), (4, 20, -9), head, head_d)
    m.pair("ruff", "Head", (-5, 12.2, -12.8), (-3.9, 17, -9.4), ragged_hem(609, BL, 2))
    m.box("muzzle", "Head", (-2.6, 12.6, -19), (2.6, 16.6, -15.6), BL, nose_d(NS))
    m.pair("fang", "Head", (-2.5, 7.6, -18.8), (-1.3, 12.8, -17.6), fang)
    m.bone("jaw", (0, 12.6, -10), "Head")
    m.box("jaw", "jaw", (-2, 11.2, -18), (2, 12.6, -10), lambda p: PK if p.face == "up" else BL)
    ears(m, (-3.9, 20, -12), (-1.9, 22.2, -11), pointed(TW, PK, DK))
    four_legs(m, -5, -2, -8.5, 5.5, 3.5, 9, leg)
    m.pair("paw-f", ("Leg-FL", "Leg-FR"), (-5.4, 0, -9.8), (-1.6, 1.7, -4.8), BL, claws_d(NS))
    m.pair("paw-b", ("Leg-BL", "Leg-BR"), (-5.4, 0, 4.2), (-1.6, 1.7, 9.2), BL, claws_d(NS))
    m.bone("tail", (0, 16, 9))
    m.box("tail", "tail", (-1.1, 13.8, 8.6), (1.1, 16.4, 12.6), lambda p: DK if p.lz > 0.6 else TW, rot=(-25, 0, 0), origin=(0, 15.5, 9))
    return m


def raptor():
    m = Model("raptor", 610)
    SK, DK, BL, CR, CT, EY, CL = (192, 130, 72), (112, 72, 46), (238, 222, 180), (40, 150, 150), (226, 240, 200), (255, 210, 50), (48, 44, 44)

    def hide(belly_y):
        def sh(p):
            if p.face == "down" or p.y < belly_y:
                return BL
            if int(math.floor(p.z + 100)) % 5 == 0 and p.y > belly_y + 2:
                return DK
            return SK
        return sh

    def neck_sh(p):
        if p.face == "north":
            return BL
        return DK if int(math.floor(p.y + 100)) % 4 == 0 and p.face != "south" else SK

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, EY), c.px(c.fx(3), 1, BLACK)
            for k in range(1, 6):
                c.px(c.fx(k), 0, DK)
            for k in range(2, c.w):
                c.px(c.fx(k), 3, CR)
        elif c.face == "north":
            c.sym(0, 1, EY)

    def plume(p):
        if p.side() and p.j == p.h - 1 and p.i % 2 == 0:
            return CR + (0,)
        return CT if p.j >= p.h - 2 else CR

    def fan(p):
        if p.face in ("up", "down"):
            if abs(p.lx - 0.5) * 2 > 0.3 + 0.7 * p.lz or (p.lz > 0.85 and p.i % 2 == 1):
                return CR + (0,)
            return CT if p.lz > 0.7 else DK if p.i % 2 else CR
        return CR

    def leg(p):
        return DK if (p.i + 2 * p.j) % 5 == 0 else SK

    m.box("body", "Body", (-3, 13, -6), (3, 20, 6), hide(15))
    m.box("back-plumes", "Body", (-0.45, 19.8, -5), (0.45, 22, 6), spines(CR, CT, "z", 3), noise=3)
    NP, NR = (0, 18.5, -5), (-36, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2, 17, -7.4), (2, 27.5, -3.4), neck_sh, rot=NR, origin=NP)
    m.box("neck-plumes", "neck", (-0.45, 19, -3.6), (0.45, 27.5, -1.4), spines(CR, CT, "y", 3), rot=NR, origin=NP, noise=3)
    hp = rotp((0, 26.5, -5.4), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-2.5, hy - 1.5, hz - 6), (2.5, hy + 3.5, hz + 1.5), lambda p: BL if p.face == "down" else SK, head_d)
    m.box("snout", "Head", (-1.9, hy - 1.2, hz - 12.5), (1.9, hy + 2.2, hz - 5.5), lambda p: DK if p.face == "up" and p.j % 3 == 0 else SK,
          tl_teeth(also=lambda c: c.face in ("up", "north") and c.sym(0, 1, CL)))
    m.bone("jaw", (0, hy - 1.3, hz + 0.5), "Head")
    m.box("jaw", "jaw", (-1.7, hy - 2.6, hz - 12), (1.7, hy - 1.3, hz + 0.5), tl_jaw(SK, BL), rot=(-8, 0, 0), origin=(0, hy - 1.3, hz + 0.5))
    m.box("crest", "Head", (-0.45, hy + 3.2, hz - 5), (0.45, hy + 7.5, hz + 5), spines(CR, CT, "z", 3), rot=(-18, 0, 0), origin=(0, hy + 3.2, hz), noise=3)
    m.pair("crest-side", "Head", (-2.4, hy + 2.6, hz - 2), (-1.7, hy + 6, hz + 5), spines(CR, CT, "z", 3), rot=(-18, 0, 30),
           origin=(-2, hy + 2.8, hz), noise=3)
    for tag, sx in (("L", -1), ("R", 1)):
        t = tag.lower()
        ax = 3.6 * sx
        m.bone("Arm-" + tag, (ax, 15.5, -4.6))
        m.box("arm-" + t, "Arm-" + tag, (ax - 0.7, 10.4, -5.6), (ax + 0.7, 16, -3.6), leg)
        m.box("arm-plumes-" + t, "Arm-" + tag, (ax + (0.3 if sx > 0 else -0.8), 10, -3.8), (ax + (0.8 if sx > 0 else -0.3), 15.4, -0.6), plume, noise=3)
        m.box("claw-" + t, "Arm-" + tag, (ax - 0.8, 9, -6.8), (ax + 0.8, 10.6, -4.4), CL)
        lx = 3.1 * sx
        m.bone("Leg-" + tag, (lx, 14.5, 2))
        m.box("thigh-" + t, "Leg-" + tag, (lx - 1.5, 9, -0.5), (lx + 1.5, 16.5, 4.5), hide(11))
        m.box("shin-" + t, "Leg-" + tag, (lx - 0.9, 1, 2), (lx + 0.9, 10, 4), leg)
        m.box("foot-" + t, "Leg-" + tag, (lx - 1.5, 0, -2.6), (lx + 1.5, 1.5, 4.4), lambda p: DK if p.face == "up" else SK, claws_d(CL))
        m.box("sickle-" + t, "Leg-" + tag, (lx - sx * 1.2 - 0.4, 1.4, -3), (lx - sx * 1.2 + 0.4, 3.8, -1.4), lambda p: mix(CL, (150, 140, 130), p.ly))
    TP, TR = (0, 17, 6), (-3, 0, 0)
    tl_chain(m, "tail", ((2.2, 14.5, 19, 5.5, 14), (1.7, 15, 18.6, 13.5, 22), (1.2, 15.5, 18.2, 21.5, 30), (0.8, 15.9, 17.7, 29.5, 38)),
             lambda k, y0: hide(y0 + 1.2), TR, TP)
    m.box("tail-fan", "tail-d", (-3, 16.4, 33), (3, 17.1, 42), fan, rot=TR, origin=TP, noise=3)
    return m


def tyrant():
    m = Model("tyrant", 611)
    OL, RS, BL, DK, EY, CL, MO = (116, 108, 72), (156, 88, 52), (216, 202, 162), (64, 58, 44), (255, 120, 40), (42, 38, 36), (150, 50, 60)

    def hide(belly_y):
        def sh(p):
            if p.face == "down" or p.y < belly_y:
                return BL if int(math.floor(p.z + 100)) % 2 else mul(BL, 0.9)
            s = vnoise(611, p.x / 5.0, p.y / 1.4 + p.z / 6.0, p.z / 1.6)
            if p.y > belly_y + 3 and s > 0.62:
                return RS
            return DK if (p.i * 3 + p.j * 5) % 11 == 0 else OL
        return sh

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(4), 2, EY), c.px(c.fx(5), 2, BLACK), c.px(c.fx(4), 3, mul(EY, 0.7))
            for k in range(3, 8):
                c.px(c.fx(k), 1, DK)
        elif c.face == "north":
            c.sym(0, 2, EY)

    def snout_d(c):
        if c.face in ("up", "north"):
            c.sym(1, 1, CL)
        if c.face in ("east", "west", "north"):
            c.rect(0, c.h - 2, c.w, 1, DK)
            for x in range(0, c.w, 2):
                c.px(x, c.h - 1, WHITE)

    def leg(p):
        return DK if (p.i + 2 * p.j) % 5 == 0 else OL

    m.box("body", "Body", (-6, 16, -9), (6, 29, 9), hide(19))
    m.box("neck", "Body", (-4.6, 21, -14), (4.6, 31, -8), hide(23.5), rot=(-12, 0, 0), origin=(0, 24, -9))
    m.box("back-ridge", "Body", (-0.5, 28.8, -8), (0.5, 30.6, 9), spines(DK, RS, "z", 3), noise=3)
    m.bone("Head", (0, 28, -12))
    m.box("head", "Head", (-5.5, 24.5, -21), (5.5, 34.5, -11), hide(26), head_d)
    m.box("snout", "Head", (-4.5, 25, -29), (4.5, 32.6, -20.5), hide(25), snout_d)
    m.pair("brow", "Head", (-5.9, 33.4, -19.5), (-3, 35.6, -14), lambda p: RS if p.face == "up" else DK)
    m.bone("jaw", (0, 24.8, -12), "Head")
    m.box("jaw", "jaw", (-4.3, 21, -28), (4.3, 24.8, -12), tl_jaw(mul(OL, 0.9), BL, MO), rot=(-7, 0, 0), origin=(0, 24.8, -12))
    for tag, sx in (("L", -1), ("R", 1)):
        t = tag.lower()
        ax = 6.5 * sx
        m.bone("Arm-" + tag, (ax, 19.5, -7.8))
        m.box("arm-" + t, "Arm-" + tag, (ax - 0.8, 16, -9), (ax + 0.8, 20, -6.8), leg)
        m.box("claw-" + t, "Arm-" + tag, (ax - 0.8, 15.2, -10.4), (ax + 0.8, 16.6, -8.4), CL, claws_d(WHITE))
        lx = 6.2 * sx
        m.bone("Leg-" + tag, (lx, 20, 1.5))
        m.box("thigh-" + t, "Leg-" + tag, (lx - 2.8, 11, -3.5), (lx + 2.8, 23.5, 5.5), hide(13))
        m.box("shin-" + t, "Leg-" + tag, (lx - 1.8, 2, 1), (lx + 1.8, 12.5, 5), leg)
        m.box("foot-" + t, "Leg-" + tag, (lx - 2.8, 0, -5), (lx + 2.8, 2.6, 5.6), lambda p: DK if p.face == "up" and p.i % 2 else OL, claws_d(CL))
    TP, TR = (0, 22, 9), (4, 0, 0)
    tl_chain(m, "tail", ((4.6, 17, 27, 8.5, 18), (3.6, 18, 26, 17.5, 27), (2.6, 19, 25, 26.5, 36), (1.6, 20, 24, 35.5, 45)),
             lambda k, y0: hide(y0 + 2), TR, TP)
    m.box("tail-ridge", "tail-a", (-0.5, 26.8, 9), (0.5, 28.4, 18), spines(DK, RS, "z", 3), rot=TR, origin=TP, noise=3)
    return m


def pterosaur():
    m = Model("pterosaur", 612)
    HD, DK, BL, CR, BK, MB, EY = (198, 152, 106), (122, 86, 62), (238, 222, 192), (226, 86, 50), (238, 208, 122), (226, 198, 152), (250, 240, 120)
    fuzz = fur(612, HD, DK, BL, 6.4, scale=1.6, thr=0.6, amt=0.4)

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(2), 1, EY), c.px(c.fx(3), 1, BLACK)
        elif c.face == "north":
            c.sym(0, 1, EY)

    def beak(p):
        if p.side() and p.j == p.h - 1:
            return mul(BK, 0.7)
        return mix(BK, CR, max(0.0, 0.5 - p.lz) * 1.2)

    def crest(p):
        a = 255 if abs(p.ly - 0.5) * 2 < 1.05 - p.lz * 0.95 else 0
        c = mix(CR, BK, p.lz) if int(math.floor(p.lz * 9)) % 3 else mul(CR, 0.7)
        return tuple(c) + (a,)

    m.box("body", "Body", (-2.5, 5, -5), (2.5, 10, 5), fuzz)
    m.box("chest", "Body", (-2, 4.2, -4.6), (2, 6, 0), BL)
    NP, NR = (0, 9, -4.4), (-40, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-1.4, 8, -5.9), (1.4, 17, -3.1), fuzz, rot=NR, origin=NP)
    hp = rotp((0, 16.4, -4.5), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-1.8, hy - 1.2, hz - 5), (1.8, hy + 2.8, hz + 1.5), fuzz, head_d)
    m.box("beak", "Head", (-1, hy - 0.4, hz - 15.5), (1, hy + 1.8, hz - 4.6), beak)
    m.box("beak-tip", "Head", (-0.6, hy - 0.4, hz - 18), (0.6, hy + 0.9, hz - 15.2), lambda p: mul(BK, 0.8))
    m.bone("jaw", (0, hy - 0.5, hz), "Head")
    m.box("jaw", "jaw", (-0.8, hy - 1.5, hz - 15), (0.8, hy - 0.4, hz), lambda p: (170, 80, 80) if p.face == "up" else mul(BK, 0.88),
          rot=(-6, 0, 0), origin=(0, hy - 0.5, hz))
    m.box("crest", "Head", (-0.4, hy + 0.8, hz - 2.5), (0.4, hy + 5.4, hz + 9.5), crest, rot=(-20, 0, 0), origin=(0, hy + 2.5, hz - 2), noise=3)
    WP = (-2.5, 9.5, -2.5)
    el = tl_wings(m, WP, (0, 0, -8), 14, -3.5, 6.5, membrane(2.5, 5, HD, DK, MB, 0.26), 19, -2.5, 5.5,
                  lambda x0: taper(membrane(x0, 5, HD, DK, MB, 0.2), x0, 19, 0.12), tip_rot=(0, 10, 5), elbow=0.5)
    m.pair("wing-claw", ("wing-l-tip", "wing-r-tip"), (el[0] - 0.7, el[1] - 0.5, el[2] - 4.6), (el[0] + 0.7, el[1] + 0.5, el[2] - 2.4), DK)
    for tag, sx in (("L", -1), ("R", 1)):
        lx, t = 1.8 * sx, tag.lower()
        m.bone("Leg-" + tag, (lx, 5.5, 3.6))
        m.box("leg-" + t, "Leg-" + tag, (lx - 0.6, 0.5, 3), (lx + 0.6, 6, 4.2), DK)
        m.box("foot-" + t, "Leg-" + tag, (lx - 1.1, 0, 1), (lx + 1.1, 0.8, 4.6), mul(DK, 0.8), claws_d(BLACK))
    m.bone("tail", (0, 8, 5))
    m.box("tail", "tail", (-0.7, 6.8, 4.8), (0.7, 8.2, 9.6), fuzz)
    return m


def giant_mantis():
    m = Model("giant_mantis", 613, noise=5)
    GR, GD, BL, EYC, PK, WG, WV = (128, 188, 86), (80, 136, 60), (216, 228, 152), (226, 96, 126), (238, 164, 124), (176, 218, 136), (110, 160, 90)

    def head_d(c):
        if c.part != "head-top":
            return
        if c.face == "north":
            c.symrect(0, 0, 2, 3, EYC)
            c.sym(1, 1, BLACK)
        elif c.face in ("east", "west"):
            c.rect(0, 0, c.w, c.h, EYC)
            c.px(c.fx(1), 1, BLACK)
        elif c.face == "up":
            c.symrect(0, 0, 2, c.h, EYC)

    def forearm(p):
        if p.face in ("east", "west") and p.j == p.h - 1 and p.i % 2 == 0:
            return GD + (0,)
        if p.face == ("east" if p.x < 0 else "west"):
            return PK if (p.i + p.j) % 3 else WHITE
        return GR if p.j < p.h - 1 else GD

    def hook(p):
        if p.face in ("east", "west") and p.i % 2 == 0 and (p.lz < 0.3 if p.face == "west" else p.lz < 0.3):
            return GD
        return mix(GD, GR, p.ly)

    def wing(p):
        if p.face in ("up", "down"):
            if p.lz > 0.9 and abs(p.lx - 0.5) > 0.5 - (p.lz - 0.9) * 4:
                return WG + (0,)
            if int(math.floor(p.z + 100)) % 4 == 0 or p.i == p.w // 2:
                return WV
        return WG

    TP, TR = (0, 12, 0), (-25, 0, 0)
    m.box("abdomen", "Body", (-2.6, 9, 1), (2.6, 14, 15), banded(GR, GD, 3, "z", BL, 10.2))
    m.box("abdomen-tip", "Body", (-1.6, 10, 15), (1.6, 13, 18.5), banded(GR, GD, 3, "z", BL, 10.8))
    m.box("thorax", "Body", (-2.8, 9.5, -3.4), (2.8, 14.4, 1.6), lambda p: BL if p.face == "down" else GR)
    m.box("prothorax", "Body", (-1.8, 12, -2.2), (1.8, 27, 1.8), lambda p: BL if p.face == "north" else GD if p.face == "south" and p.i == p.w // 2 else GR,
          rot=TR, origin=TP)
    hp = rotp((0, 27, 0), TR, TP)
    hx, hy, hz = hp
    m.bone("Head", hp)
    m.box("head-top", "Head", (-3.7, hy + 0.5, hz - 2.6), (3.7, hy + 3.5, hz + 1.4), GR, head_d)
    m.box("head-mid", "Head", (-2.4, hy - 1.4, hz - 2.9), (2.4, hy + 0.6, hz + 1.1), lambda p: BL if p.face == "north" else GR)
    m.box("head-chin", "Head", (-1.2, hy - 3.2, hz - 3.1), (1.2, hy - 1.3, hz + 0.5), GD, lambda c: c.face == "north" and c.sym(0, c.h - 1, (90, 50, 40)))
    m.pair("antenna", "Head", (-1.7, hy + 3.4, hz - 1.3), (-1.2, hy + 11, hz - 0.8), lambda p: mix(GD, BL, p.ly), rot=(-28, 0, 14), origin=(-1.45, hy + 3.4, hz - 1))
    sx_, sy_, sz_ = rotp((-2.6, 23.5, -1.5), TR, TP)
    m.bone("Arm-L", (sx_, sy_, sz_)), m.bone("Arm-R", (-sx_, sy_, sz_))
    arms = ("Arm-L", "Arm-R")
    m.pair("arm", arms, (sx_ - 0.9, sy_ - 7, sz_ - 1), (sx_ + 0.7, sy_ + 0.6, sz_ + 1), lambda p: GD if p.j == 0 else GR)
    m.pair("forearm", arms, (sx_ - 1.1, sy_ - 8.6, sz_ - 9), (sx_ + 0.9, sy_ - 5.6, sz_ + 0.6), forearm)
    m.pair("hook", arms, (sx_ - 0.7, sy_ - 6, sz_ - 9.6), (sx_ + 0.5, sy_ + 0.4, sz_ - 8.2), hook)
    side_legs(m, ((-2.6, -30), (0.6, 26)), -2.6, -9.5, 12.4, 1.2, lambda p: GR if int(math.floor(p.x + 100)) % 3 else GD, lambda p: mix(GD, GR, p.ly), 3.0)
    wings(m, (-0.2, 14.7, 1.5), (-3.2, 14.1, 1.5), (-0.2, 14.7, 18), wing, rot=(0, 0, 9))
    return m


def giant_scorpion():
    m = Model("giant_scorpion", 614, noise=6)
    CH, CD, CL, ST, EY = (130, 78, 42), (72, 42, 30), (200, 136, 64), (206, 52, 40), (250, 220, 90)

    def shell(p):
        if p.face == "down":
            return CL
        if p.face == "up" and (abs(p.x) < 0.6 or p.j == p.h - 1):
            return CD
        return CH if vnoise(614, p.x / 2.0, p.y / 2.0, p.z / 2.0) < 0.62 else mix(CH, CD, 0.5)

    def shell_d(c):
        if c.face == "north":
            c.sym(2, 1, EY), c.sym(0, 2, EY), c.sym(1, 2, BLACK)
        elif c.face == "up":
            c.sym(c.w // 2 - 1, 2, BLACK)

    def claw(p):
        if p.face in ("up", "down") and p.part.startswith("pincer") and p.lz < 0.3 and (p.i == 0 if p.x < 0 else p.i == p.w - 1) == p.part.startswith("pincer-b"):
            return CD + (0,)
        return CL if p.face == "down" else CD if (p.i + p.j) % 4 == 0 else CH

    def tail_sh(axis):
        return banded(CH, CD, 3, axis)

    def sting(p):
        a = 255 if abs(p.lx - 0.5) * 2 < 0.35 + p.ly * 0.75 else 0
        return tuple(mix(BLACK, ST, p.ly)) + (a,)

    m.box("carapace", "Body", (-5, 3, -7), (5, 7.5, 1), shell, shell_d)
    m.box("abdomen", "Body", (-4.6, 3, 1), (4.6, 7, 8), banded(CH, CD, 2, "z", CL, 3.6))
    m.box("abdomen-b", "Body", (-3.4, 3.2, 8), (3.4, 6.4, 10.5), banded(CH, CD, 2, "z", CL, 3.8))
    m.pair("chelicera", "Body", (-1.9, 3, -8.6), (-0.3, 5.2, -6.8), CL, lambda c: c.face == "north" and c.rect(0, c.h - 1, c.w, 1, CD))
    side_legs(m, ((-6, -34), (-3.6, -12), (-1.2, 12), (1.2, 34)), -4.6, -11, 6.6, 1.2,
              lambda p: CH if int(math.floor(p.x + 100)) % 3 else CD, lambda p: mix(CD, CL, p.ly), 1.6)
    AP, AR = (-3.5, 4.6, -6.5), (0, 22, 0)
    m.bone("Arm-L", AP), m.bone("Arm-R", (3.5, 4.6, -6.5))
    arms = ("Arm-L", "Arm-R")
    m.pair("arm", arms, (-4.4, 3.6, -12), (-2.6, 5.6, -6), banded(CH, CD, 3, "z"), rot=AR, origin=AP)
    m.pair("fist", arms, (-5.7, 3, -17), (-1.5, 6.6, -11.5), claw, rot=AR, origin=AP)
    m.pair("pincer-a", arms, (-5.7, 3.3, -21.5), (-4.1, 6.3, -16.8), claw, rot=AR, origin=AP)
    m.pair("pincer-b", arms, (-3.1, 3.6, -21), (-1.5, 5.8, -16.8), claw, rot=AR, origin=AP)
    tail = (("a", (0, 5.5, 10.2), (-1.7, 4, 10), (1.7, 7.4, 14.6), "z"),
            ("b", (0, 6.2, 14.8), (-1.55, 5.6, 13.6), (1.55, 12, 16.6), "y"),
            ("c", (0, 11.6, 15.1), (-1.4, 11, 13.8), (1.4, 17.6, 16.6), "y"),
            ("d", (0, 17.6, 15.2), (-1.3, 16.6, 8.4), (1.3, 19.4, 15.6), "z"),
            ("e", (0, 18, 8.8), (-1.7, 15.6, 3.6), (1.7, 19.8, 9.2), "z"))
    par = "Body"
    for tag, piv, lo, hi, axis in tail:
        m.bone("tail-" + tag, piv, par)
        m.box("tail-" + tag, "tail-" + tag, lo, hi, tail_sh(axis) if tag != "e" else (lambda p: ST if p.face == "down" else mix(CH, ST, 0.35)))
        par = "tail-" + tag
    m.box("sting", "tail-e", (-0.9, 10.6, 3), (0.9, 16, 4.4), sting, rot=(28, 0, 0), origin=(0, 16, 4), noise=3)
    return m


def broodmother():
    m = Model("broodmother", 615, noise=6)
    PL, PD, MK, YL, EY, EG, FG = (74, 54, 88), (42, 32, 54), (216, 172, 232), (242, 222, 124), (232, 52, 62), (230, 226, 204), (30, 22, 30)

    def abdomen(p):
        if p.face == "down":
            return PD
        d = abs(p.x)
        if p.face in ("up", "south", "north"):
            if d < 0.8:
                return YL if int(math.floor(p.z + p.y + 100)) % 3 == 0 else PL
            if d < 5.4 and int(math.floor(p.z + p.y * 0.8 + d * 0.9 + 100)) % 4 == 0:
                return MK
        elif int(math.floor(p.z - p.y * 0.9 + 100)) % 4 == 0 and p.ly > 0.3:
            return MK
        return PL if vnoise(615, p.x / 2.2, p.y / 2.2, p.z / 2.2) < 0.6 else PD

    def ceph(p):
        if p.face == "up" and abs(p.x) < 2.5 and p.j % 3 == 1:
            return MK
        return PD if (p.i + p.j) % 5 == 0 else PL

    def ceph_d(c):
        if c.face == "north":
            c.sym(0, 1, EY), c.sym(2, 0, EY)
            c.symrect(1, 2, 2, 2, EY)
            c.sym(1, 2, WHITE)
            c.sym(3, 1, EY)
        elif c.face == "up":
            c.sym(1, 0, EY)

    def leg_t(p):
        return MK if int(math.floor(p.x + 100)) % 4 == 0 else PL

    def leg_s(p):
        return MK if int(math.floor(p.y + 100)) % 5 == 0 else mix(PD, PL, p.ly)

    def eggs(p):
        return mix(EG, PL, 0.3) if (p.i + p.j) % 2 else EG

    m.box("cephalothorax", "Body", (-4, 8, -7), (4, 13, 0.6), ceph, ceph_d)
    m.box("abdomen", "Body", (-7, 7, 0), (7, 17, 14), abdomen)
    m.box("abdomen-top", "Body", (-5.5, 17, 1.5), (5.5, 19, 12.5), abdomen)
    m.box("abdomen-rear", "Body", (-5, 8.5, 14), (5, 15.5, 16.6), abdomen)
    m.box("spinneret", "Body", (-1.2, 8.8, 16.4), (1.2, 11.2, 18.2), PD)
    m.box("egg-sac", "Body", (-3, 4.6, 5), (3, 7.2, 11.5), eggs)
    m.pair("fang", "Body", (-2.1, 4.4, -8.4), (-0.6, 9, -6.8), lambda p: mix(EY, FG, min(1.0, p.ly * 2.2)))
    m.pair("palp", "Body", (-3.6, 6, -9.8), (-2.5, 9.6, -6.6), leg_s)
    side_legs(m, ((-6.4, -40), (-4.6, -14), (-2.8, 14), (-1, 40)), -3.8, -15.5, 12, 1.4, leg_t, leg_s, 4.0)
    return m


def sand_worm():
    m = Model("sand_worm", 616, noise=6)
    SD, SK, BL, MO, MD, TE, LP = (218, 182, 120), (166, 126, 82), (242, 224, 178), (104, 40, 50), (44, 14, 22), (250, 244, 226), (206, 126, 112)

    def ring(belly_y=1.6):
        base = banded(SD, SK, 4, "z", BL, belly_y)

        def sh(p):
            if p.face in ("east", "west", "up") and p.y >= belly_y and int(math.floor(p.z + 100)) % 4 == 2 and (p.i + p.j) % 3 == 0:
                return SK
            return base(p)
        return sh

    def head(p):
        if p.face == "north":
            r = math.hypot(p.x, p.y - 6)
            if r < 1.6:
                return MD
            if r < 3.4:
                return TE if int(math.floor(math.atan2(p.y - 6, p.x) * 8 / math.pi + 16)) % 2 == 0 and r > 2.2 else MO
            return LP if r < 5 else SD
        return ring()(p)

    def lip(p):
        inner = {"lip-top": "down", "jaw": "up", "lip-l": "east", "lip-r": "west"}[p.part]
        if p.face == inner:
            return TE if (p.i + p.j) % 2 == 0 and p.lz < 0.5 else MO
        if p.face == "north":
            return LP
        return SD if int(math.floor(p.z + 100)) % 2 else SK

    def fang(p):
        return mix(TE, (200, 184, 150), p.lz)

    m.box("body", "Body", (-5.5, 0, -5), (5.5, 11, 5), ring())
    m.bone("Head", (0, 5.5, -5))
    m.box("head", "Head", (-6, 0, -12), (6, 12, -4.6), head)
    m.box("lip-top", "Head", (-3.5, 9, -15), (3.5, 12, -11.6), lip)
    m.pair("lip", "Head", (-6, 2.5, -15), (-3, 9.5, -11.6), lip)
    m.box("fang-top", "Head", (-0.6, 7.2, -16.4), (0.6, 9.2, -13.6), fang)
    m.pair("fang", "Head", (-3.2, 5.4, -16.4), (-1.6, 6.6, -13.6), fang)
    m.bone("jaw", (0, 2.6, -11.6), "Head")
    m.box("jaw", "jaw", (-3.5, 0, -15), (3.5, 3, -11.6), lip)
    m.box("fang-low", "jaw", (-0.6, 2.8, -16.4), (0.6, 4.8, -13.6), fang)
    segs = ((5.2, 0, 10.4, 4.5, 13), (4.8, 0, 9.6, 12.5, 21), (4.2, 0, 8.4, 20.5, 29), (3.4, 0, 6.8, 28.5, 37), (2.4, 0, 4.8, 36.5, 45))
    tl_chain(m, "tail", segs, lambda k, y0: ring(1.6 - 0.2 * k))
    m.box("bristles", "Body", (-0.5, 10.8, -4), (0.5, 12.6, 5), spines(SK, BL, "z", 4), noise=3)
    m.box("bristles-a", "tail-a", (-0.5, 10.2, 5), (0.5, 12, 13), spines(SK, BL, "z", 4), noise=3)
    m.box("bristles-b", "tail-b", (-0.5, 9.4, 13), (0.5, 11, 21), spines(SK, BL, "z", 4), noise=3)
    m.box("tail-spike", "tail-e", (-0.9, 0.6, 44.5), (0.9, 2.6, 49.5), lambda p: mix(SK, TE, p.lz))
    return m


def kraken():
    m = Model("kraken", 617, noise=6)
    MN, MD, MS, UN, EYC = (160, 66, 94), (110, 38, 70), (234, 172, 172), (240, 214, 200), (250, 220, 90)

    def mantle(p):
        if p.face == "down":
            return MD
        c = mix(MN, MD, (1 - p.ly) * 0.6) if p.side() else MN
        if vnoise(617, p.x / 1.7 + 5, p.y / 1.7, p.z / 1.7) > 0.68:
            c = MS
        return c

    def head(p):
        return MD if p.face == "down" else mix(MN, UN, 0.3)

    def head_d(c):
        if c.face == "north":
            c.symrect(0, 1, 4, 4, EYC)
            c.symrect(1, 2, 3, 2, BLACK)
            c.sym(0, 1, WHITE)
        elif c.face in ("east", "west"):
            x = c.fx(3) - (3 if c.face == "east" else 0)
            c.rect(x, 1, 4, 4, EYC), c.rect(x, 2, 3 if c.face == "west" else 4, 2, BLACK), c.rect(x, 2, 1 if c.face == "east" else 0, 2, EYC)

    def fin(p):
        outer = (1 - p.lx) if p.x < 0 else p.lx
        a = 255 if abs(p.ly - 0.55) < 0.5 - outer * 0.4 else 0
        return tuple(mix(MN, MS, 0.4 + 0.3 * outer)) + (a,)

    def arm(p):
        if p.side() and p.i == p.w // 2 and int(math.floor(p.y * 1.01 + 100)) % 2 == 0:
            return UN
        return mix(MD, MN, p.ly) if p.part.endswith("b") else MN

    m.box("mantle", "Body", (-6, 14, -6), (6, 28, 6), mantle)
    m.box("mantle-cap", "Body", (-4.5, 28, -4.5), (4.5, 31.5, 4.5), mantle)
    m.box("mantle-tip", "Body", (-2.5, 31.5, -2.5), (2.5, 33.5, 2.5), mantle)
    m.pair("fin", "Body", (-10, 22, -0.5), (-5.8, 31, 0.5), fin, skip=("up", "down", "east", "west"))
    m.bone("Head", (0, 14, 0))
    m.box("head", "Head", (-5, 8, -5), (5, 14.5, 5), head, head_d)
    m.box("siphon", "Head", (-1.3, 9.6, -6.6), (1.3, 11.8, -4.8), mix(MN, UN, 0.5), lambda c: c.face == "north" and c.rect(1, 0, 1, 2, MD))
    for k in range(8):
        ang = math.radians(22.5 + 45 * k)
        x, z = round(3.7 * math.sin(ang), 2), round(-3.7 * math.cos(ang), 2)
        piv = (x, 9.6, z)
        rot = (round(-20 * z / 3.7, 1), 0, round(20 * x / 3.7, 1))
        name = "tentacle-%d" % (k + 1)
        m.bone(name, piv)
        m.box(name, name, (x - 1.2, 4.6, z - 1.2), (x + 1.2, 10.2, z + 1.2), arm, rot=rot, origin=piv)
        m.bone(name + "-b", rotp((x, 5, z), rot, piv), name)
        m.box(name + "-b", name + "-b", (x - 0.85, 0, z - 0.85), (x + 0.85, 5.2, z + 0.85), arm, rot=rot, origin=piv)
    return m


def crystal_stag():
    m = Model("crystal_stag", 618, noise=5)
    CT, CD, BL, KR, KL, KD, HF, NS = (234, 232, 224), (208, 206, 200), (250, 248, 242), (206, 238, 250), (246, 253, 255), (150, 200, 232), (160, 176, 196), (92, 84, 96)
    coat = fur(618, CT, CD, BL, 15.4, scale=2.4, thr=0.64, amt=0.8)
    gem = crystal(KR, KL, KD)

    def leg(p):
        if p.y < 1.6:
            return KD if p.j % 2 else HF
        return CD if p.y < 7 and p.face == "north" else CT

    def head_d(c):
        if c.face == "north":
            c.sym(0, 1, BLACK)
            c.px(c.w // 2, 0, KR), c.px(c.w // 2 - 1, 0, KR), c.px(c.w // 2, 1, KL) if c.w % 2 else None
        c.side_eye(2, 1, BLACK, KD)

    m.box("body", "Body", (-3.5, 14, -8), (3.5, 21, 8), coat)
    m.box("ruff", "Body", (-3, 12.4, -8.6), (3, 16.5, -4), ragged_hem(618, BL, 2))
    NP, NR = (0, 19.5, -7), (-22, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2, 18, -9.5), (2, 29, -5.5), coat, rot=NR, origin=NP)
    hp = rotp((0, 28, -7.5), NR, NP)
    hx, hy, hz = hp
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-2.5, hy - 1.5, hz - 5.5), (2.5, hy + 3.5, hz + 1.5), lambda p: BL if p.face == "down" else CT, head_d)
    m.box("muzzle", "Head", (-1.7, hy - 1.5, hz - 9.5), (1.7, hy + 1.6, hz - 5), CT, nose_d(NS, (150, 130, 140)))
    ears(m, (-4.8, hy + 1.6, hz - 0.6), (-2.4, hy + 3.4, hz + 0.4), lambda p: (238, 200, 204) if p.face == "north" and p.lx > 0.2 else CT,
         pivot=(-2.5, hy + 2.5, hz - 0.1), rot=(0, 0, -24))
    base = (-2, hy + 3.3, hz - 0.4)
    m.pair("antler", "Head", (-2.6, hy + 3.3, hz - 1), (-1.4, hy + 9.5, hz + 0.2), gem, rot=(12, 0, 20), origin=base)
    m.pair("antler-brow", "Head", (-2.5, hy + 5, hz - 4.5), (-1.5, hy + 6, hz - 0.6), gem, rot=(12, 0, 20), origin=base)
    m.pair("antler-fork", "Head", (-5.4, hy + 8.4, hz - 0.9), (-2.4, hy + 9.5, hz + 0.1), gem, rot=(12, 0, 20), origin=base)
    m.pair("antler-tine", "Head", (-5.5, hy + 9, hz - 0.9), (-4.4, hy + 13.5, hz + 0.1), gem, rot=(12, 0, 20), origin=base)
    m.pair("antler-top", "Head", (-2.7, hy + 9, hz - 1.2), (-1.3, hy + 14, hz + 0.4), gem, rot=(26, 0, 12), origin=(-2, hy + 9, hz - 0.4))
    m.box("crystal-a", "Body", (-1.2, 20.4, -4), (1, 25.6, -1.8), gem, rot=(16, 0, -12), origin=(0, 20.5, -3))
    m.box("crystal-b", "Body", (0.2, 20.4, -0.6), (1.8, 23.6, 1), gem, rot=(-12, 0, 22), origin=(1, 20.5, 0.2))
    m.box("crystal-c", "Body", (-2, 20.4, 3.6), (-0.4, 23.4, 5.2), gem, rot=(22, 0, -18), origin=(-1.2, 20.5, 4.4))
    m.box("crystal-d", "Body", (-3.9, 16, -6.5), (-3.2, 19, -5), gem, rot=(0, 0, 14), origin=(-3.5, 17, -5.8))
    four_legs(m, -3.5, -1.5, -7.5, 5.5, 2, 14, leg)
    m.bone("tail", (0, 20, 8))
    m.box("tail", "tail", (-1, 17, 7.6), (1, 20.6, 9.4), lambda p: KR if p.ly < 0.3 else BL)
    return m


def lava_golem():
    m = Model("lava_golem", 619, noise=6)
    RK, RD, SM, GL = (64, 56, 58), (40, 34, 38), (228, 86, 28), (255, 200, 72)
    rock = cracked(619, RK, RD, SM, GL)

    def head_d(c):
        if c.face == "north":
            c.symrect(1, 2, 2, 1, GL)
            c.sym(1, 3, SM)
            c.rect(2, c.h - 2, c.w - 4, 1, SM)

    def core(p):
        if p.face == "north":
            d = math.hypot(p.lx - 0.5, p.ly - 0.5)
            return GL if d < 0.22 else SM if d < 0.4 else RD
        return RD

    def fist(p):
        return SM if p.side() and p.j == p.h - 2 and p.i % 2 else rock(p)

    legs, arms = ("Leg-L", "Leg-R"), ("Arm-L", "Arm-R")
    m.box("torso", "Body", (-8, 15, -5), (8, 29, 5), rock)
    m.box("hips", "Body", (-5.5, 11, -3.5), (5.5, 16, 3.5), rock)
    m.box("core", "Body", (-2.5, 19.5, -5.7), (2.5, 24.5, -4.7), core)
    m.box("spire-a", "Body", (-5, 27, 1.5), (-2, 35.5, 4.5), rock, rot=(14, 0, 10), origin=(-3.5, 27, 3))
    m.box("spire-b", "Body", (1.6, 27, 2), (4.6, 33.5, 5), rock, rot=(16, 0, -14), origin=(3, 27, 3.5))
    m.box("spire-c", "Body", (-1.4, 24, 3.6), (1.4, 31, 6.2), rock, rot=(28, 0, 0), origin=(0, 24, 5))
    m.bone("Head", (0, 28.5, -1))
    m.box("head", "Head", (-3.5, 27.5, -5.4), (3.5, 33.5, 1.6), rock, head_d)
    m.box("jaw-rock", "Head", (-2.8, 26, -5.8), (2.8, 28.4, -2), lambda p: RD if p.face != "up" else SM)
    m.bone("Arm-L", (-11, 28, 0)), m.bone("Arm-R", (11, 28, 0))
    m.pair("shoulder", arms, (-14.6, 24, -4.6), (-8, 31.5, 4.6), rock)
    m.pair("arm", arms, (-13.6, 16, -3), (-9, 24.5, 3), rock)
    m.pair("forearm", arms, (-15, 5, -4), (-8.4, 16, 4), rock)
    m.pair("fist", arms, (-14.6, 1, -3.5), (-8.8, 5.6, 3.5), fist)
    m.bone("Leg-L", (-3.6, 12, 0)), m.bone("Leg-R", (3.6, 12, 0))
    m.pair("leg", legs, (-6.6, 0, -3), (-1.4, 12.5, 3), rock)
    m.pair("foot", legs, (-7.2, 0, -4.8), (-0.8, 3, 3.6), rock)
    return m


def ghost_knight():
    m = Model("ghost_knight", 620, noise=5)
    AR, AL, AD, CK, CK2, GL, BD = (178, 196, 214), (220, 232, 242), (88, 102, 126), (60, 80, 98), (40, 56, 72), (150, 255, 236), (224, 238, 246)
    plate = armour(AR, AL, AD, 4)

    def cloak(p):
        k = hsh(620, int(math.floor(p.x * 0.7 + 50)))
        c = CK2 if k > 0.6 else CK
        return mix(c, GL, 0.35 * max(0.0, 1 - p.y / 8.0))

    def wisp(p):
        c = mix(GL, AD, min(1.0, p.ly * 1.2))
        if p.side() and hsh(621, int(p.x * 1.01 + 50), int(p.z * 1.01 + 50)) > 0.35 + p.ly * 0.7:
            return tuple(c) + (0,)
        return c

    def mail(p):
        return AD if (p.i + p.j) % 2 else mul(AD, 1.3)

    def helm_d(c):
        if c.face == "north":
            c.rect(1, 2, c.w - 2, 1, BLACK)
            c.rect(c.w // 2, 2, 1, 4, BLACK) if c.w % 2 else c.rect(c.w // 2 - 1, 2, 2, 4, BLACK)
            c.sym(2, 2, GL)
        elif c.face in ("east", "west"):
            c.px(c.fx(0), 2, BLACK), c.px(c.fx(1), 2, BLACK)
            c.rect(0, c.h - 1, c.w, 1, AL)

    def blade(p):
        if p.face in ("east", "west") and p.lz < 0.12 and abs(p.ly - 0.5) * 2 > p.lz / 0.12:
            return BD + (0,)
        if p.face in ("east", "west") and p.j == p.h // 2:
            return mix(BD, GL, 0.6)
        return BD

    m.box("cuirass", "Body", (-5, 13, -3), (5, 25, 3), plate)
    m.box("waist", "Body", (-3.6, 8, -2.4), (3.6, 13.5, 2.4), ragged_hem(620, mail, 3))
    m.box("wisp", "Body", (-2.2, 1.5, -1.6), (2.2, 9, 1.6), wisp, skip=("down",))
    m.box("cloak", "Body", (-6.5, 0, 3), (6.5, 25.5, 4.2), ragged_hem(622, cloak, 8))
    m.pair("cloak-side", "Body", (-6.6, 5, -1), (-5.4, 25, 3.2), ragged_hem(623, cloak, 7))
    m.box("collar", "Body", (-4, 24.6, -3.4), (4, 26.6, 4.4), lambda p: AL if p.j == 0 else CK)
    m.pair("pauldron", "Body", (-8.6, 22, -3.6), (-4.6, 26.8, 3.6), lambda p: AL if p.side() and p.j == p.h - 1 else plate(p))
    m.bone("Head", (0, 25.5, 0))
    m.box("helm", "Head", (-3.5, 25.5, -3.5), (3.5, 32.5, 3.5), lambda p: AL if p.face == "up" and p.i == p.w // 2 else AR, helm_d)
    m.box("plume", "Head", (-0.5, 32.3, -2.5), (0.5, 36, 5.5), spines(CK, GL, "z", 3), rot=(-10, 0, 0), origin=(0, 32.3, 0), noise=3)
    m.bone("Arm-L", (-6.6, 23.5, 0)), m.bone("Arm-R", (6.6, 23.5, 0))
    arms = ("Arm-L", "Arm-R")
    m.pair("arm", arms, (-8.1, 16, -2), (-5.1, 23.5, 2), mail)
    m.pair("forearm", arms, (-8.5, 9, -2.4), (-4.7, 17, 2.4), lambda p: AL if p.side() and p.j in (0, p.h - 1) else AR)
    m.box("sword-hilt", "Arm-R", (6, 9.2, -5.2), (7.2, 10.6, 3.6), lambda p: AD if int(math.floor(p.z + 100)) % 2 else CK)
    m.box("sword-guard", "Arm-R", (4.2, 8.6, -6), (9, 11.2, -5), lambda p: GL if abs(p.x - 6.6) < 0.6 else AL)
    m.box("sword", "Arm-R", (6.3, 8.4, -23), (6.9, 11.4, -6), blade, noise=2)
    return m


def moth():
    m = Model("moth", 621, noise=4)
    FZ, FD, WG, WB, SPD, SPL, AN, EY = (238, 230, 212), (206, 192, 166), (242, 236, 222), (204, 186, 158), (128, 116, 146), (252, 226, 160), (192, 172, 142), (40, 30, 40)

    def in_fore(u, v):
        if u < 0 or v < 0 or u > 0.98:
            return False
        if u > 0.78 and (v < (u - 0.78) * 1.4 or v > 1 - (u - 0.78) * 2.2):
            return False
        return v < 0.5 + 0.5 * min(1.0, u / 0.6)

    def in_hind(u, v):
        return u >= 0 and v >= 0 and (((u - 0.45) / 0.55) ** 2 + ((v - 0.45) / 0.6) ** 2 < 1 or (u < 0.35 and v < 0.7))

    def wing(inside, x0, span, depth, spot):
        def sh(p):
            if p.face not in ("up", "down"):
                return WB
            u, v = (abs(p.x) - x0) / span, p.lz
            if not inside(u, v):
                return WG + (0,)
            if not inside(u + 1.05 / p.w, v) or not inside(u, v + 1.05 / p.h) or not inside(u + 1.05 / p.w, v + 1.05 / p.h):
                return FD if (int(u * p.w) + int(v * p.h)) % 2 else WB
            d = math.hypot((u - spot[0]) * span, (v - spot[1]) * depth)
            if d < 0.8:
                return EY
            if d < 1.7:
                return SPL
            if d < 2.4:
                return SPD
            wave = u * span + 1.2 * math.sin(v * depth * 1.1)
            if int(math.floor(wave)) % 4 == 3:
                return WB
            return mix(WG, FZ, u)
        return sh

    WP, WR, SK = (-1.6, 5, -2), (0, 0, -8), ("north", "south", "east", "west")
    m.box("thorax", "Body", (-1.8, 2, -4), (1.8, 5.6, 0.6), fur(621, FZ, FD, scale=1.2, thr=0.55, amt=0.6))
    m.box("abdomen", "Body", (-1.4, 2.2, 0.6), (1.4, 4.8, 6.6), banded(FZ, FD, 2, "z"))
    m.box("tuft", "Body", (-1, 2.4, 6.4), (1, 4.4, 8), ragged_hem(621, FD, 1))
    m.box("collar", "Body", (-2.1, 3.4, -4.3), (2.1, 5.9, -2.6), ragged_hem(624, WG, 1))
    m.bone("Head", (0, 3.8, -4))
    m.box("head", "Head", (-1.3, 2.6, -6), (1.3, 5, -3.8), FZ,
          lambda c: (c.face == "north" and c.sym(0, 0, EY)) or (c.face in ("east", "west") and c.rect(c.fx(1) - (1 if c.face == "east" else 0), 0, 2, 2, EY)))
    m.pair("antenna", "Head", (-4, 4.8, -9), (-0.6, 5.2, -5.4),
           lambda p: AN + ((255,) if p.i == p.w - 1 - p.j * p.w // max(1, p.h) or (p.j % 2 == 0 and p.face in ("up", "down")) and abs(p.lx - (1 - p.lz)) < 0.34 else (0,)) if p.x < 0
           else AN + ((255,) if p.i == p.j * p.w // max(1, p.h) or (p.j % 2 == 0 and p.face in ("up", "down")) and abs(p.lx - p.lz) < 0.34 else (0,)),
           rot=(18, 0, 0), origin=(0, 5, -5.4), skip=SK, noise=2)
    wings(m, WP, (-13.6, 4.8, -7), (-1.6, 5.2, 1), wing(in_fore, 1.6, 12, 8, (0.62, 0.45)), rot=WR, skip=SK, name="wing-fore")
    wings(m, WP, (-10.6, 4.62, 0), (-1.6, 5.02, 8), wing(in_hind, 1.6, 9, 8, (0.5, 0.48)), rot=WR, skip=SK, name="wing-hind")
    for k, z in enumerate((-3.4, -2, -0.6)):
        m.pair("leg-%d" % (k + 1), "Body", (-2.8, 0, z), (-1.5, 2.6, z + 0.6), lambda p: FD if p.ly > 0.4 else AN)
    return m


def giant_snail():
    m = Model("giant_snail", 622, noise=6)
    SK, SKD, SKL, SH, SHL, SHD, EY = (216, 198, 152), (178, 154, 112), (236, 224, 186), (178, 114, 72), (228, 182, 122), (120, 72, 50), (26, 22, 26)

    def flesh(p):
        if p.face == "down" or p.y < 0.9:
            return SKL
        return SKD if vnoise(622, p.x / 1.3, p.y / 1.3, p.z / 1.3) > 0.62 else SK

    def spiral(p):
        dy, dz = p.y - 10.5, p.z - 1.5
        r, th = math.hypot(dy, dz), math.atan2(dy, -dz)
        v = (r - th / (2 * math.pi) * 3.4 + 0.6) % 3.4
        if p.face in ("east", "west"):
            if r < 1.1 or v < 0.85:
                return SHD
            return SHL if int(math.floor((th + math.pi) * 5.5)) % 3 == 0 else SH
        return SHD if v < 0.85 else SHL if (p.i + p.j) % 4 == 0 else SH

    def head_d(c):
        if c.face == "north":
            c.rect(1, c.h - 2, c.w - 2, 1, SKD)

    m.box("foot", "Body", (-3.5, 0, -10), (3.5, 3, 10), flesh)
    m.box("foot-tail", "Body", (-2, 0, 10), (2, 2, 13.6), flesh)
    m.box("mantle", "Body", (-3, 2.6, -6.6), (3, 5.2, -3.4), SKD)
    m.box("shell", "Body", (-3.2, 3.4, -5), (3.2, 17.6, 8), spiral)
    m.box("shell-bulge", "Body", (-4.2, 6, -2.6), (4.2, 15, 5.6), spiral)
    m.box("shell-top", "Body", (-2.6, 17.4, -2.6), (2.6, 19, 5.6), spiral)
    m.box("shell-rear", "Body", (-2.6, 6, 7.8), (2.6, 15, 9.6), spiral)
    m.box("shell-front", "Body", (-2.6, 7, -6.6), (2.6, 15, -4.8), spiral)
    m.bone("Head", (0, 3, -8))
    m.box("head", "Head", (-2.5, 1.4, -13), (2.5, 7, -7.6), flesh, head_d)
    m.pair("feeler", "Head", (-1.9, 1.6, -14.6), (-1.1, 2.4, -12.8), SKD)
    ER = (-14, 0, 12)
    ears(m, (-2.1, 6.8, -12), (-1.2, 12, -11.1), lambda p: mix(SKD, SK, p.ly), rot=ER)
    piv = m.bones["ear-l"][0]
    m.pair("eye", ("ear-l", "ear-r"), (-2.5, 11.6, -12.4), (-0.8, 13.2, -10.7), EY, lambda c: c.face in ("north", "up") and c.px(0, 0, WHITE), rot=ER, origin=piv)
    return m


def thunderbird():
    m = Model("thunderbird", 623, noise=5)
    FB, FD, FL, WH, BK, LT, TL = (74, 94, 144), (44, 58, 102), (152, 178, 222), (242, 242, 234), (250, 202, 62), (255, 238, 124), (40, 40, 50)

    def plumage(p):
        if p.face == "down":
            return FL
        return FD if (p.i + 2 * p.j) % 4 == 0 else FB

    def feathers(x0):
        def sh(p):
            if p.face not in ("up", "down"):
                return FD
            u = abs(p.x) - x0
            k = int(math.floor(u)) % 3
            if p.lz > 0.84 and k == 2:
                return FB + (0,)
            if p.lz < 0.28:
                return FD if (p.i + p.j) % 2 else FB
            if abs(p.lz - 0.52 - (0.07 if int(math.floor(u / 2)) % 2 else -0.07)) < 0.05:
                return LT
            if k == 0:
                return FD
            return FL if p.lz > 0.7 else FB
        return sh

    def fingers(x0, span):
        base = feathers(x0)

        def sh(p):
            c = base(p)
            u = (abs(p.x) - x0) / span
            if p.face in ("up", "down") and u > 0.72 and p.j % 3 == 2:
                return FB + (0,)
            return c
        return sh

    def fan(p):
        if p.face in ("up", "down"):
            if abs(p.lx - 0.5) * 2 > 0.5 + 0.5 * p.lz or (p.lz > 0.86 and p.i % 3 == 2):
                return FB + (0,)
            if p.lz > 0.7:
                return LT if p.lz > 0.86 else WH
            return FD if p.i % 3 == 2 else FB
        return FD

    def head_d(c):
        if c.face in ("east", "west"):
            c.px(c.fx(1), 2, LT), c.px(c.fx(2), 2, BLACK)
            for k in range(0, 4):
                c.px(c.fx(k), 1, FD)
        elif c.face == "north":
            c.sym(0, 2, LT)
            c.sym(0, 1, FD), c.sym(1, 1, FD)

    BP, BR = (0, 11, 0), (18, 0, 0)
    m.box("body", "Body", (-4, 7, -6), (4, 16, 6), plumage, rot=BR, origin=BP)
    m.box("breast", "Body", (-3.4, 6, -5.6), (3.4, 9, 1), lambda p: FL if (p.i + p.j) % 2 else mix(FL, WH, 0.5), rot=BR, origin=BP)
    hp = rotp((0, 15, -5), BR, BP)
    hx, hy, hz = hp
    m.box("ruff", "Body", (-3.3, hy - 3.6, hz - 2.4), (3.3, hy + 0.4, hz + 3.4), ragged_hem(623, WH, 2))
    m.bone("Head", hp)
    m.box("head", "Head", (-2.8, hy - 0.6, hz - 4.6), (2.8, hy + 4.6, hz + 2), WH, head_d)
    m.box("beak", "Head", (-1.2, hy + 0.4, hz - 8.6), (1.2, hy + 2.6, hz - 4.2), lambda p: mix(BK, (230, 150, 40), p.lz < 0.3))
    m.box("beak-hook", "Head", (-0.8, hy - 1.2, hz - 9), (0.8, hy + 0.8, hz - 7.4), (226, 160, 46))
    m.bone("jaw", (0, hy + 0.4, hz - 4), "Head")
    m.box("jaw", "jaw", (-1, hy - 0.5, hz - 7.6), (1, hy + 0.4, hz - 4), mul(BK, 0.85))
    m.box("crest", "Head", (-0.45, hy + 4.4, hz - 3.5), (0.45, hy + 8, hz + 5.5), spines(WH, LT, "z", 3), rot=(-14, 0, 0), origin=(0, hy + 4.4, hz), noise=3)
    m.pair("crest-side", "Head", (-2.8, hy + 3.4, hz - 1.5), (-2.1, hy + 6.6, hz + 5), spines(WH, LT, "z", 3), rot=(-14, 0, 34),
           origin=(-2.4, hy + 3.6, hz), noise=3)
    WP = rotp((-4, 15, -2), BR, BP)
    tl_wings(m, WP, (0, 0, -10), 14, -4.5, 7.5, feathers(4), 16, -4, 7, lambda x0: fingers(x0, 16), tip_rot=(0, 6, 5), elbow=0.5, th=0.5)
    tx, ty, tz = rotp((0, 9.5, 6), BR, BP)
    m.bone("tail-a", (0, ty, tz - 0.5))
    m.box("tail-a", "tail-a", (-2.6, ty - 0.9, tz - 1), (2.6, ty + 0.9, tz + 5), plumage)
    m.bone("tail-b", (0, ty, tz + 4.5), "tail-a")
    m.box("tail-fan", "tail-b", (-6, ty - 0.4, tz + 4), (6, ty + 0.4, tz + 13), fan, noise=3)
    bird_legs(m, 2.3, 6.5, 1, BK, mul(BK, 0.85), th=0.8, foot=2.0)
    m.pair("trouser", ("Leg-L", "Leg-R"), (-3.7, 4.6, 0), (-0.9, 8.6, 3.6), ragged_hem(625, FB, 2))
    m.pair("talon", ("Leg-L", "Leg-R"), (-4.3, 0, -2.4), (-0.3, 0.9, -1.2), lambda p: TL + ((255,) if p.i % 2 == 0 else (0,)))
    return m


def cave_troll():
    m = Model("cave_troll", 624)
    SK, SD, BL, MO, NS, IV, CLH, HR, EY = (130, 144, 116), (94, 108, 88), (172, 178, 142), (84, 128, 68), (172, 132, 124), (236, 226, 196), (112, 82, 58), (58, 50, 44), (250, 210, 80)

    def skin(p):
        if p.face in ("up", "south") and vnoise(624, p.x / 2.2, p.y / 2.2, p.z / 2.2) > 0.52:
            return MO
        if p.face == "north" and abs(p.x) < 4.5:
            return BL if (p.i + p.j) % 5 else SD
        return SD if vnoise(625, p.x / 1.8, p.y / 1.8, p.z / 1.8) > 0.66 else SK

    def limb(p):
        return SD if vnoise(625, p.x / 1.8, p.y / 1.8, p.z / 1.8) > 0.64 else SK

    def head_d(c):
        if c.face == "north":
            c.sym(1, 3, EY), c.sym(2, 3, BLACK)
            c.rect(1, c.h - 2, c.w - 2, 1, (70, 46, 46))
        c.side_eye(1, 3, BLACK, EY)

    def ear(p):
        if p.face in ("north", "south") and ((p.j == 0 or p.j == p.h - 1) and ((p.lx < 0.4) == (p.x < 0))):
            return SK + (0,)
        return NS if p.face == "north" and 0 < p.j < p.h - 1 and 0.15 < p.lx < 0.85 else SK

    def fist_d(c):
        if c.face == "north":
            for x in range(1, c.w, 2):
                c.px(x, c.h - 1, SD)

    def foot_d(c):
        if c.face == "north":
            for x in range(0, c.w, 2):
                c.px(x, c.h - 1, IV)

    TP, TR = (0, 10, 1), (-22, 0, 0)
    m.box("torso", "Body", (-7, 9, -4.5), (7, 24, 5), skin, rot=TR, origin=TP)
    m.box("hump", "Body", (-5, 22, -1), (5, 26.5, 5.6), skin, rot=TR, origin=TP)
    m.box("hips", "Body", (-5.6, 7, -3), (5.6, 12, 4.6), limb)
    m.box("loincloth", "Body", (-3.6, 2.5, -3.8), (3.6, 9.5, -2.9), ragged_hem(624, lambda p: CLH if int(p.x + 100) % 3 else mul(CLH, 0.8), 3))
    hp = rotp((0, 21.5, -4.5), TR, TP)
    hx, hy, hz = hp
    m.bone("Head", hp)
    m.box("head", "Head", (-4.2, hy - 3, hz - 8), (4.2, hy + 5, hz + 0.5), limb, head_d)
    m.box("brow", "Head", (-4.5, hy + 2.3, hz - 8.7), (4.5, hy + 3.8, hz - 6.4), SD)
    m.box("nose", "Head", (-1.7, hy - 2.4, hz - 12), (1.7, hy + 2.2, hz - 7.6), NS, lambda c: c.face == "north" and c.sym(0, c.h - 1, (90, 60, 60)))
    m.box("hair", "Head", (-2.4, hy + 4.8, hz - 6.5), (2.4, hy + 6.6, hz), shag(626, HR, mul(HR, 0.6)))
    m.bone("jaw", (0, hy - 2.4, hz - 0.5), "Head")
    m.box("jaw", "jaw", (-3.7, hy - 4.6, hz - 9), (3.7, hy - 2.6, hz - 0.6), lambda p: (120, 60, 64) if p.face == "up" else SD)
    m.pair("tusk", "jaw", (-3.4, hy - 3, hz - 9.5), (-2.2, hy - 0.2, hz - 8.4), lambda p: mix(IV, WHITE, p.ly))
    ears(m, (-9.4, hy - 0.5, hz - 4.4), (-4, hy + 5, hz - 3.4), ear, pivot=(-4.2, hy + 2, hz - 3.9), rot=(0, 24, 12))
    ax, ay, az = rotp((-8, 21.5, 0), TR, TP)
    m.bone("Arm-L", (ax - 1, ay, az)), m.bone("Arm-R", (1 - ax, ay, az))
    arms, legs = ("Arm-L", "Arm-R"), ("Leg-L", "Leg-R")
    m.pair("shoulder", arms, (ax - 4.4, ay - 2.5, az - 3), (ax + 1, ay + 2.6, az + 3), skin)
    m.pair("arm", arms, (ax - 3.8, 11, az - 2.3), (ax + 0.4, ay - 2.5, az + 2.3), limb)
    m.pair("forearm", arms, (ax - 4.4, 3, az - 3.4), (ax + 1, 11, az + 2.4), limb)
    m.pair("fist", arms, (ax - 4.8, 0, az - 4.6), (ax + 1.4, 4, az + 2.6), limb, fist_d)
    m.bone("Leg-L", (-3.6, 8, 1)), m.bone("Leg-R", (3.6, 8, 1))
    m.pair("leg", legs, (-6, 0, -1.4), (-1.4, 9, 3.6), limb)
    m.pair("foot", legs, (-6.6, 0, -4.2), (-0.8, 2, 4), limb, foot_d)
    return m


# -- Titanlands, second helping: dinosaurs, insects, worms, robots, sea life,
#    birds, small mammals, crystal folk and four trees. Most are one body plan
#    drawn several ways; tl_named() gives each drawing a function of its own.

def tl_named(name, build, seed):
    def f():
        return build(name, seed)
    f.__name__ = name
    return f


def tl_eyes(k=1, y=1, col=BLACK, white=None):
    """Decal: an eye on each side face, k texels back from the front, and at the front corners."""
    def d(c):
        if c.face in ("east", "west"):
            c.side_eye(k, y, col, white)
        elif c.face == "north":
            c.sym(0, y, col)
    return d


def tl_fringe(col, tip, period=4):
    """A flat plate of spikes pointing away from the middle of the body."""
    def sh(p):
        k = int(math.floor(p.z + 100)) % period
        frac = 1 - abs(k - period // 2) / (period // 2 + 1.0)
        d = (1 - p.lx) if p.x < 0 else p.lx
        return tuple(mix(col, tip, d)) + ((255,) if d <= frac else (0,))
    return sh


def tl_out(p):
    """How far out along a left or right plate a texel is, 0 at the body."""
    return p.lx if p.x > 0 else 1 - p.lx


def tl_quad_dino(name, seed):
    """Four-legged dinosaurs: one barrel, four pillars and a tail, dressed four ways."""
    m = Model(name, seed, noise=6)
    SK, DK, BL, HN, hw, y0, y1, zl = {
        "triceratops": ((152, 134, 94), (104, 88, 60), (222, 208, 162), (238, 230, 206), 6, 9, 20, 10),
        "stegosaurus": ((112, 148, 98), (72, 106, 72), (216, 208, 152), (222, 126, 70), 5, 9, 19, 10),
        "brachiosaurus": ((128, 152, 162), (88, 112, 126), (218, 216, 192), (236, 228, 204), 6, 15, 27, 11),
        "ankylosaurus": ((158, 130, 98), (108, 84, 62), (216, 198, 152), (234, 224, 198), 7.5, 6, 14, 10),
    }[name]
    hide = scaly(SK, DK, BL, y0 + 2.5)
    eyes = tl_eyes(2, 1, BLACK, (250, 214, 90))

    def leg(p):
        return DK if p.y < 1.5 or (p.i + 2 * p.j) % 5 == 0 else SK

    m.box("body", "Body", (-hw, y0, -zl), (hw, y1, zl), hide)
    four_legs(m, -hw - 0.5, -hw + 4, -zl + 0.5, zl - 5.5, 5, y0 + 1, leg, claws_d(HN))
    ty = y1 - 1
    tl_chain(m, "tail", ((hw * 0.6, ty - 6, ty, zl - 0.5, zl + 7), (hw * 0.4, ty - 5, ty - 1, zl + 6.5, zl + 14),
                         (hw * 0.22, ty - 4.2, ty - 1.6, zl + 13.5, zl + 21)), lambda k, yy: scaly(SK, DK, BL, yy + 1.2))
    if name == "triceratops":
        def frill(p):
            if p.face in ("north", "south"):
                e = min(p.i, p.w - 1 - p.i, p.j)
                if e == 0:
                    return HN if (p.i + p.j) % 2 == 0 else DK
                return DK if e == 1 else mix(SK, (196, 96, 70), 0.5) if (p.i // 3 + p.j // 3) % 2 == 0 else SK
            return DK

        m.bone("Head", (0, y0 + 7, -zl))
        m.box("head", "Head", (-4, y0 + 2, -zl - 8), (4, y0 + 10, -zl + 0.5), hide, eyes)
        m.box("beak", "Head", (-2.2, y0 + 1.5, -zl - 12), (2.2, y0 + 7, -zl - 7.6), lambda p: mix(HN, DK, 0.45) if p.ly < 0.5 else SK)
        m.box("frill", "Head", (-7.5, y0 + 6, -zl - 1), (7.5, y0 + 17, -zl + 0.6), frill, rot=(22, 0, 0), origin=(0, y0 + 8, -zl))
        m.pair("horn", "Head", (-3.6, y0 + 9.4, -zl - 15), (-2.2, y0 + 10.8, -zl - 6), lambda p: mix(HN, mul(HN, 0.75), p.lz),
               rot=(24, 0, 0), origin=(-2.9, y0 + 10, -zl - 6))
        m.box("nose-horn", "Head", (-0.7, y0 + 6.6, -zl - 11.2), (0.7, y0 + 10, -zl - 9.6), HN)
    elif name == "stegosaurus":
        plate = spines(HN, mix(HN, WHITE, 0.4), "z", 5)
        m.box("hump", "Body", (-hw + 1, y1, -zl + 3), (hw - 1, y1 + 2.5, zl - 3), hide)
        m.box("neck", "Body", (-2.4, y0 + 1.5, -zl - 4), (2.4, y0 + 7, -zl + 0.5), hide)
        m.bone("Head", (0, y0 + 4, -zl - 4))
        m.box("head", "Head", (-2, y0 + 1, -zl - 10), (2, y0 + 5.6, -zl - 3.6), hide, eyes)
        m.pair("plates", "Body", (-1.7, y1 + 2.2, -zl + 2), (-0.9, y1 + 8.5, zl - 2), plate, noise=3)
        m.pair("plates-tail", "tail-a", (-1.5, ty - 0.3, zl), (-0.7, ty + 4.5, zl + 7), plate, noise=3)
        for k, z in enumerate((15.5, 18.5)):
            m.pair("spike-%d" % k, "tail-c", (-5.5, ty - 3.4, zl + z), (-0.6, ty - 2.6, zl + z + 0.8), lambda p: mix(HN, WHITE, tl_out(p)))
    elif name == "brachiosaurus":
        NP, NR = (0, y1 - 3, -zl + 2), (-24, 0, 0)
        m.bone("neck", NP)
        m.box("shoulder", "Body", (-hw + 1, y1, -zl + 0.5), (hw - 1, y1 + 3, 2), hide)
        m.box("neck", "neck", (-2.5, y1 - 3, -zl - 0.5), (2.5, y1 + 20, -zl + 4.5), scaly(SK, DK), rot=NR, origin=NP)
        hx, hy, hz = hp = rotp((0, y1 + 19, -zl + 2), NR, NP)
        m.bone("Head", hp, "neck")
        m.box("head", "Head", (-2.4, hy - 1.5, hz - 6.5), (2.4, hy + 3, hz + 2), lambda p: BL if p.face == "down" else SK, eyes)
        m.box("crest", "Head", (-1.4, hy + 3, hz - 3), (1.4, hy + 5.4, hz + 1), SK)
    else:
        def shell(p):
            if p.face == "up":
                return HN if p.i % 3 == 1 and p.j % 3 == 1 else DK if p.i % 3 == 0 or p.j % 3 == 0 else SK
            return DK

        m.box("shell", "Body", (-hw + 0.5, y1, -zl + 1), (hw - 0.5, y1 + 1.6, zl - 1), shell)
        m.pair("side-spikes", "Body", (-hw - 2.2, y0 + 4.5, -zl + 1), (-hw + 0.2, y0 + 5.3, zl - 1), tl_fringe(HN, WHITE, 4), noise=3)
        m.bone("Head", (0, y0 + 4, -zl))
        m.box("head", "Head", (-3.5, y0 + 1, -zl - 7), (3.5, y0 + 6.5, -zl + 0.5), hide, eyes)
        m.pair("head-horn", "Head", (-4.8, y0 + 5, -zl - 2.2), (-3.2, y0 + 7.2, -zl - 0.4), HN)
        m.box("club", "tail-c", (-2.8, ty - 4.8, zl + 19), (2.8, ty - 1, zl + 24.5), lambda p: HN if (p.i + p.j) % 3 == 0 else DK)
    return m


def tl_biped_dino(name, seed):
    """Two-legged dinosaurs: a sail-back, a twin-crest and a duck-bill."""
    m = Model(name, seed, noise=6)
    SK, DK, BL, CR, CT, L = {
        "spinosaurus": ((112, 132, 122), (70, 88, 92), (226, 214, 172), (206, 98, 62), (244, 200, 120), 10),
        "dilophosaurus": ((150, 158, 92), (98, 108, 62), (234, 226, 180), (212, 84, 70), (250, 214, 130), 6),
        "parasaurolophus": ((176, 138, 96), (120, 92, 66), (236, 222, 184), (192, 86, 76), (240, 190, 140), 5),
    }[name]
    EY, CL = (255, 214, 60), (50, 46, 46)

    def hide(belly_y):
        def sh(p):
            if p.face == "down" or p.y < belly_y:
                return BL
            if vnoise(seed, p.x / 2.0, p.y / 2.0, p.z / 2.0) > 0.6 and p.y > belly_y + 2:
                return DK
            return SK
        return sh

    def leg(p):
        return DK if (p.i + 2 * p.j) % 5 == 0 else SK

    m.box("body", "Body", (-3.5, 13, -7), (3.5, 21, 7), hide(15.5))
    NP, NR = (0, 19.5, -6), (-34, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-2.2, 18, -8.4), (2.2, 28, -4), lambda p: BL if p.face == "north" else SK, rot=NR, origin=NP)
    hx, hy, hz = hp = rotp((0, 27, -6.2), NR, NP)
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-2.6, hy - 1.5, hz - 6), (2.6, hy + 3.5, hz + 1.5), lambda p: BL if p.face == "down" else SK, tl_eyes(2, 1, BLACK, EY))
    JP = (0, hy - 1.3, hz + 0.5)
    m.bone("jaw", JP, "Head")
    if name == "parasaurolophus":
        m.box("bill", "Head", (-2.4, hy - 1, hz - 6 - L), (2.4, hy + 1.6, hz - 5.5), lambda p: mix(DK, (60, 54, 50), 0.5) if p.lz < 0.3 else SK)
        m.box("jaw", "jaw", (-2, hy - 2.2, hz - 5 - L), (2, hy - 1.2, hz + 0.5), BL)
        m.box("crest", "Head", (-0.9, hy + 2.6, hz - 2), (0.9, hy + 4.6, hz + 10), lambda p: mix(CR, CT, p.lz), rot=(-24, 0, 0), origin=(0, hy + 3, hz - 2))
    else:
        m.box("snout", "Head", (-1.9, hy - 1.2, hz - 6 - L), (1.9, hy + 2.2, hz - 5.5), lambda p: DK if p.face == "up" and p.j % 3 == 0 else SK, tl_teeth())
        m.box("jaw", "jaw", (-1.7, hy - 2.6, hz - 5.5 - L), (1.7, hy - 1.3, hz + 0.5), tl_jaw(SK, BL), rot=(-8, 0, 0), origin=JP)
    if name == "spinosaurus":
        def sail(p):
            if p.ly > math.sin(math.pi * min(1.0, max(0.0, p.lz))) ** 0.6:
                return CR + (0,)
            return CT if int(math.floor(p.z + 100)) % 3 == 0 else mix(CR, DK, 1 - p.ly)

        m.box("sail", "Body", (-0.45, 20.8, -7), (0.45, 31, 7), sail, noise=3)
    elif name == "dilophosaurus":
        def crest(p):
            if p.ly > math.sin(math.pi * min(1.0, max(0.0, p.lz))) ** 0.5:
                return CR + (0,)
            return mix(CR, CT, p.ly)

        m.pair("crest", "Head", (-1.7, hy + 3.3, hz - 9), (-1, hy + 6.8, hz + 1.5), crest, noise=3)
    reach = 7 if name == "spinosaurus" else 5
    for tag, sx in (("L", -1), ("R", 1)):
        t = tag.lower()
        ax = 4.1 * sx
        m.bone("Arm-" + tag, (ax, 15.5, -5))
        m.box("arm-" + t, "Arm-" + tag, (ax - 0.8, 15.8 - reach, -6), (ax + 0.8, 16.2, -4), leg)
        m.box("claw-" + t, "Arm-" + tag, (ax - 0.8, 14.6 - reach, -7.2), (ax + 0.8, 16 - reach, -4.8), CL)
        lx = 3.4 * sx
        m.bone("Leg-" + tag, (lx, 14.5, 2))
        m.box("thigh-" + t, "Leg-" + tag, (lx - 1.7, 9, -0.8), (lx + 1.7, 16.5, 4.8), hide(11))
        m.box("shin-" + t, "Leg-" + tag, (lx - 1, 1, 2), (lx + 1, 10, 4.2), leg)
        m.box("foot-" + t, "Leg-" + tag, (lx - 1.7, 0, -2.8), (lx + 1.7, 1.5, 4.6), lambda p: DK if p.face == "up" else SK, claws_d(CL))
    tl_chain(m, "tail", ((2.6, 14.5, 20, 6.5, 15), (2, 15, 19.4, 14.5, 23), (1.4, 15.6, 18.8, 22.5, 31), (0.9, 16, 18.2, 30.5, 39)),
             lambda k, yy: hide(yy + 1.2), (-3, 0, 0), (0, 17.5, 7))
    return m


def tl_bug(name, seed):
    """Six legs on the ground: an ant pale enough to tint, a termite, a cricket and a clockwork mite."""
    m = Model(name, seed, noise=4)
    A, B, C, EY = {
        "worker_ant": ((226, 220, 212), (176, 168, 160), (140, 132, 128), (60, 56, 64)),
        "termite": ((236, 222, 186), (208, 186, 140), (188, 112, 58), (70, 40, 30)),
        "cricket": ((112, 132, 62), (78, 96, 46), (170, 160, 96), (30, 28, 24)),
        "tinker_mite": ((176, 150, 88), (120, 100, 62), (96, 102, 112), (120, 240, 255)),
    }[name]
    band = banded(A, B, 2, "z")
    if name == "cricket":
        m.box("thorax", "Body", (-1.8, 3, -4), (1.8, 6.4, 0), lambda p: B if p.face == "up" else A)
        m.box("abdomen", "Body", (-1.6, 3, 0), (1.6, 6, 8), band)
        m.box("wing-case", "Body", (-1.9, 5.8, -0.5), (1.9, 6.6, 7), lambda p: C if (p.i + p.j) % 3 else B)
        m.pair("cercus", "Body", (-1.4, 4, 8), (-0.8, 4.6, 11), B)
        lo, hi, legs = (-1.7, 3, -6.6), (1.7, 6.6, -3.8), ((-3.6, -30), (-2, 0))
    elif name == "termite":
        m.box("thorax", "Body", (-1.5, 2.6, -3), (1.5, 5.2, 0.5), A)
        m.box("abdomen", "Body", (-2.2, 2.2, 0.5), (2.2, 6.2, 8.5), band)
        lo, hi, legs = (-2.2, 2.4, -8), (2.2, 6.4, -2.8), ((-2.8, -30), (-1.4, 0), (0, 30))
    else:
        m.box("thorax", "Body", (-1.5, 3, -4), (1.5, 6, 1.5), lambda p: B if p.face == "up" and 0.2 < p.lx < 0.8 else A)
        m.box("waist", "Body", (-0.8, 3.6, 1), (0.8, 5.2, 2.5), C)
        m.box("abdomen", "Body", (-2.4, 2.6, 2), (2.4, 7.2, 8.6), band)
        lo, hi, legs = (-2, 3, -8), (2, 7, -3.6), ((-3.4, -32), (-1.4, 0), (0.6, 32))
    back = name == "cricket"
    m.bone("Head", (0, (lo[1] + hi[1]) / 2, hi[2]))
    m.box("head", "Head", lo, hi, C if name == "termite" else A, tl_eyes(1, 1, EY))
    m.pair("mandible", "Head", (-1.8, lo[1] + 0.2, lo[2] - (2.6 if name == "termite" else 1.6)), (-0.6, lo[1] + 1.2, lo[2] + 0.4), mul(C, 0.7))
    m.pair("antenna", "Head", (-1.9, hi[1] - 0.4, lo[2] + 0.4), (-1.3, hi[1] + (6 if back else 3.2), lo[2] + 1), mul(C, 0.8),
           rot=(48 if back else -32, 0, 14), origin=(-1.6, hi[1] - 0.2, lo[2] + 0.7))
    side_legs(m, legs, -1.4, -5.6, 4.6, 0.9, B, C)
    if back:   # the jumping legs
        hl = ("Leg-L3", "Leg-R3")
        m.bone("Leg-L3", (-1.8, 5, 2)), m.bone("Leg-R3", (1.8, 5, 2))
        m.pair("thigh-3", hl, (-3.2, 4, 1), (-1.8, 9.5, 3), A, rot=(40, 0, 0), origin=(-2.5, 4.5, 2))
        m.pair("shin-3", hl, (-3.1, 0, 5.2), (-2.1, 8.6, 6.2), C)
    if name == "tinker_mite":   # the key it is wound up with
        m.box("key-stem", "Body", (-0.4, 7, 4.6), (0.4, 9.5, 5.4), C)
        m.box("key", "Body", (-2, 9.2, 4.7), (2, 11.6, 5.3),
              lambda p: C + ((0,) if p.face in ("north", "south") and 0.3 < p.lx < 0.7 and 0.25 < p.ly < 0.75 else (255,)))
    return m


def tl_flutter(name, seed):
    """Small things on the wing: a dragonfly, a swallowtail butterfly and a lantern bug."""
    m = Model(name, seed, noise=3)
    SK4 = ("north", "south", "east", "west")
    if name == "dragonfly":
        A, B, EY, WG = (96, 200, 196), (40, 110, 130), (150, 60, 50), (226, 240, 246)

        def vein(p):
            if p.face not in ("up", "down"):
                return WG
            u = tl_out(p)
            if u > 0.6 and abs(p.lz - 0.5) > 0.5 - 0.3 * u * u:
                return WG + (0,)
            return mix(WG, B, 0.5) if p.j == 0 or (p.i % 3 == 0 and p.j % 2 == 0) else WG

        m.box("thorax", "Body", (-1.5, 2, -3), (1.5, 5, 1), lambda p: B if p.face == "up" and p.i % 2 else A)
        tl_chain(m, "tail", ((0.8, 2.8, 4.4, 0.5, 5), (0.7, 2.9, 4.3, 4.5, 9), (0.6, 3, 4.2, 8.5, 13)), lambda k, yy: banded(A, B, 2, "z"))
        m.bone("Head", (0, 3.6, -3))
        m.box("head", "Head", (-1.6, 2.2, -5.2), (1.6, 5, -2.8), A)
        m.pair("eye", "Head", (-2, 3, -5.6), (-0.3, 5.2, -3.6), EY, lambda c: c.face in ("north", "up") and c.px(0, 0, WHITE))
        WP = (-1.4, 4.8, -1.5)
        wings(m, WP, (-12, 4.6, -3.2), (-1.4, 5, -0.8), vein, rot=(0, 10, -6), skip=SK4, name="wing-fore")
        wings(m, WP, (-11, 4.5, -0.6), (-1.4, 4.9, 2), vein, rot=(0, -12, -6), skip=SK4, name="wing-hind")
        legs_z, LG = (-2.6, -1.4, -0.2), B
    elif name == "swallowtail":
        A, B, EY, WG, SP = (60, 52, 48), (40, 36, 36), (20, 18, 20), (250, 236, 150), (110, 150, 230)

        def wing(tail):
            def sh(p):
                if p.face not in ("up", "down"):
                    return B
                u, v = tl_out(p), p.lz
                if tail:
                    inside = (u < 0.75 and v < 0.75 - 0.3 * u) or (0.3 < u < 0.5 and v >= 0.4) or (u < 0.3 and v < 0.8)
                else:
                    inside = 0.45 * u * u < v < 1 - 0.5 * max(0.0, u - 0.4) and u < 0.97
                if not inside:
                    return WG + (0,)
                if p.i % 4 == 3 or min(u, v, 1 - v) < 0.08 or u > 0.82:
                    return B
                return SP if tail and v > 0.45 and u < 0.3 else WG
            return sh

        m.box("thorax", "Body", (-1, 2, -3), (1, 4.2, 0.5), A)
        m.box("abdomen", "Body", (-0.8, 2.2, 0.5), (0.8, 3.8, 6), banded(A, WG, 3, "z"))
        m.bone("Head", (0, 3.2, -3))
        m.box("head", "Head", (-0.9, 2.3, -4.6), (0.9, 4, -2.8), A, tl_eyes(0, 0, EY))
        m.pair("antenna", "Head", (-0.9, 3.8, -8), (-0.5, 4.2, -4.2), B, rot=(14, 12, 0), origin=(-0.7, 4, -4.2))
        WP = (-0.9, 3.8, -1.5)
        wings(m, WP, (-12.9, 3.6, -6.5), (-0.9, 4, 1), wing(False), rot=(0, 0, -10), skip=SK4, name="wing-fore")
        wings(m, WP, (-9.9, 3.45, 0.5), (-0.9, 3.85, 10.5), wing(True), rot=(0, 0, -10), skip=SK4, name="wing-hind")
        legs_z, LG = (-2.6, -1.6, -0.6), B
    else:
        A, B, EY, WG, GL = (70, 56, 50), (190, 70, 50), (20, 18, 20), (220, 226, 220), (214, 255, 120)

        def wing(p):
            if p.face not in ("up", "down"):
                return WG
            if tl_out(p) > 0.8 and abs(p.lz - 0.5) > 0.3:
                return WG + (0,)
            return mix(WG, A, 0.4) if p.i % 3 == 0 else WG

        m.box("thorax", "Body", (-1.6, 2, -3), (1.6, 4.6, 0), lambda p: B if p.face == "up" else A)
        m.box("abdomen", "Body", (-1.5, 2, 0), (1.5, 4.4, 3), A)
        m.box("lantern", "Body", (-1.4, 1.8, 3), (1.4, 4.2, 6.5), lambda p: mix(GL, WHITE, 0.4) if (p.i + p.j) % 2 else GL, noise=0)
        m.bone("Head", (0, 3.2, -3))
        m.box("head", "Head", (-1.2, 2.2, -4.6), (1.2, 4.2, -2.8), A, tl_eyes(0, 0, EY))
        m.pair("antenna", "Head", (-1, 4, -7), (-0.6, 4.4, -4.2), A, rot=(20, 14, 0), origin=(-0.8, 4.2, -4.2))
        wings(m, (-1.4, 4.6, -1), (-8, 4.4, -2.4), (-1.4, 4.8, 2.4), wing, rot=(0, -8, -14), skip=SK4)
        legs_z, LG = (-2.4, -1.4, -0.4), A
    for k, z in enumerate(legs_z):
        m.pair("foot-%d" % (k + 1), "Body", (-2.4, 0, z), (-1.2, 2.4, z + 0.6), LG)
    return m


def tl_worm(name, seed):
    """A burrowing worm, pale so that entities can tint it, and the armoured giant of its kind."""
    boss = name == "gravemaw"
    m = Model(name, seed, noise=6)
    if boss:
        SD, SK, BL, MO, TE, GL = (92, 74, 84), (54, 42, 54), (168, 140, 132), (130, 34, 48), (240, 232, 210), (255, 150, 60)
    else:
        SD, SK, BL, MO, TE, GL = (236, 198, 190), (204, 150, 146), (250, 230, 220), (150, 70, 80), (250, 246, 232), (236, 198, 190)
    R, n = (6.5, 6) if boss else (4, 5)

    def ring(belly_y=1.6):
        return banded(SD, SK, 5 if boss else 3, "z", BL, belly_y)

    def face(p):
        if p.face == "north":
            r = math.hypot(p.x, p.y - R)
            if r < R * 0.3:
                return BLACK
            if r < R * 0.62:
                return TE if int(math.floor(math.atan2(p.y - R, p.x) * 6 / math.pi + 12)) % 2 == 0 else MO
            return mix(SD, MO, 0.4) if r < R * 0.85 else SD
        return ring()(p)

    def fang(p):
        return mix(TE, (200, 184, 150), p.lz)

    fl = 5 if boss else 2.6
    m.box("body", "Body", (-R, 0, -5), (R, 2 * R, 5), ring())
    m.bone("Head", (0, R, -5))
    m.box("head", "Head", (-R - 0.5, 0, -12), (R + 0.5, 2 * R + 1, -4.6), face)
    m.pair("hook", "Head", (-R * 0.7, R - 0.6, -12 - fl), (-R * 0.7 + 1.2, R + 0.6, -11.6), fang)
    m.box("hook-top", "Head", (-0.6, R * 1.5, -12 - fl), (0.6, R * 1.5 + 1.2, -11.6), fang)
    m.box("hook-low", "Head", (-0.6, R * 0.35, -12 - fl), (0.6, R * 0.35 + 1.2, -11.6), fang)
    segs = tuple((round(R * (1 - 0.13 * (k + 1)), 2), 0, round(2 * R * (1 - 0.13 * (k + 1)), 2), 4.5 + 8 * k, 13 + 8 * k) for k in range(n))
    tl_chain(m, "tail", segs, lambda k, yy: ring(1.6 - 0.2 * k))
    if boss:
        def plate(p):
            if p.face == "up" and p.i in (1, p.w - 2) and p.j % 3 == 1:
                return GL
            return SK if (p.i + p.j) % 2 else mul(SK, 1.3)

        m.box("plate", "Body", (-R * 0.7, 2 * R, -4.5), (R * 0.7, 2 * R + 1.5, 4.5), plate)
        for k in range(3):
            w, _, top, z0, z1 = segs[k]
            b = "tail-" + "abc"[k]
            m.box("plate-" + "abc"[k], b, (-w * 0.7, top, z0 + 0.5), (w * 0.7, top + 1.4, z1 - 0.5), plate)
            m.box("spine-" + "abc"[k], b, (-0.5, top + 1.2, z0 + 1), (0.5, top + 4.5, z1 - 1), spines(SK, GL, "z", 4), noise=3)
        m.pair("mandible", "Head", (-R - 3, R - 1.5, -17), (-R - 0.4, R + 1.5, -9), lambda p: mix(SK, TE, 1 - p.lz), rot=(0, -18, 0), origin=(-R - 0.5, R, -9))
        m.box("crown", "Head", (-0.6, 2 * R + 0.8, -11), (0.6, 2 * R + 5, -5), spines(SK, GL, "z", 3), noise=3)
    else:
        m.box("saddle", "Body", (-R - 0.3, 1, -2.5), (R + 0.3, 2 * R + 0.3, 2.5), mix(SD, MO, 0.35))
        m.box("tail-tip", "tail-e", (-0.7, 0.2, 44), (0.7, 1.6, 46.5), SK)
    return m


def tl_robot(name, seed):
    """Riveted walkers: a stoker, a walking street lamp and the furnace king."""
    m = Model(name, seed, noise=4)
    ST, DK, TR, GL = {
        "cogwalker": ((150, 132, 100), (84, 74, 62), (200, 150, 70), (255, 190, 80)),
        "lamplighter": ((112, 128, 138), (64, 74, 84), (214, 196, 120), (255, 240, 170)),
        "iron_regent": ((96, 98, 110), (48, 48, 60), (196, 60, 50), (255, 96, 60)),
    }[name]
    plate = armour(ST, TR, DK, 5)
    boss, thin = name == "iron_regent", name == "lamplighter"
    hw, hip, top = (7, 14, 28) if boss else (3, 16, 27) if thin else (5, 10, 21)
    lw = 4 if boss else 2 if thin else 3
    lx = 1.6 if thin else hw - lw / 2
    d = 2.4 if thin else hw * 0.6
    aw = 2 if thin else 3.4
    ax = -hw - 0.5
    legs, arms = ("Leg-L", "Leg-R"), ("Arm-L", "Arm-R")

    def chest(p):
        if p.face == "north":
            r = math.hypot(p.x, p.y - (hip + top) / 2 - 1)
            if r < (3.2 if boss else 1.8):
                return GL if r < 1.0 or (boss and p.j % 2 == 0) else DK
            if r < (4.2 if boss else 2.6):
                return TR
        return plate(p)

    def limb(p):
        return DK if p.side() and p.j % 3 == 0 else mul(ST, 0.85)

    m.bone("Leg-L", (-lx, hip, 0)), m.bone("Leg-R", (lx, hip, 0))
    m.pair("leg", legs, (-lx - lw / 2, 0, -lw / 2), (-lx + lw / 2, hip + 1, lw / 2), limb)
    m.pair("boot", legs, (-lx - lw / 2 - 0.5, 0, -lw / 2 - 1.5), (-lx + lw / 2 + 0.5, 2, lw / 2 + 0.5), DK)
    m.box("torso", "Body", (-hw, hip, -d), (hw, top, d), chest)
    m.bone("Arm-L", (ax - aw / 2, top - 1.5, 0)), m.bone("Arm-R", (aw / 2 - ax, top - 1.5, 0))
    m.pair("arm", arms, (ax - aw, top - 7, -aw / 2), (ax, top, aw / 2), limb)
    m.pair("forearm", arms, (ax - aw - 0.4, top - 14, -aw / 2 - 0.4), (ax + 0.4, top - 7, aw / 2 + 0.4), plate)
    m.pair("fist", arms, (ax - aw - 0.2, top - 16.5, -aw / 2 - 1), (ax + 0.2, top - 13.8, aw / 2 + 0.2), TR)
    m.bone("Head", (0, top, 0))
    if thin:
        def glass(p):
            if p.side():
                if p.i in (0, p.w - 1) or p.j in (0, p.h - 1):
                    return DK
                return mix(GL, WHITE, 0.5) if abs(p.i - p.w / 2 + 0.5) < 1 and p.j > 1 else GL
            return DK

        m.box("neck", "Head", (-1, top, -1), (1, top + 1.5, 1), DK)
        m.box("lantern", "Head", (-2.5, top + 1.5, -2.5), (2.5, top + 8, 2.5), glass, noise=2)
        m.box("cap", "Head", (-3.2, top + 8, -3.2), (3.2, top + 9.2, 3.2), TR)
        m.box("finial", "Head", (-0.6, top + 9.2, -0.6), (0.6, top + 11, 0.6), TR)
    else:
        hs = 4 if boss else 3

        def face_d(c):
            if c.face != "north":
                return
            if boss:
                c.rect(1, 2, c.w - 2, 2, BLACK), c.rect(2, 2, c.w - 4, 1, GL)
            else:
                c.rect(c.w // 2 - 1, 1, 2, 2, GL), c.px(c.w // 2 - 1, 1, WHITE)

        m.box("head", "Head", (-hs, top, -hs), (hs, top + hs * 1.75, hs - 0.5), plate, face_d)
        m.box("antenna", "Head", (-0.4, top + hs * 1.75, -0.4), (0.4, top + hs * 1.75 + 3, 0.4), TR)
    if boss:
        m.pair("pauldron", "Body", (-hw - 5, top - 2, -4.6), (-hw + 1, top + 2.5, 4.6), lambda p: TR if p.side() and p.j == p.h - 1 else plate(p))
        m.pair("stack", "Body", (-hw + 0.5, top, 1.5), (-hw + 3.5, top + 9, 4.2), lambda p: GL if p.face == "up" else DK if p.j % 3 == 0 else mul(ST, 0.7))
        m.box("crown", "Head", (-4.4, top + 6.6, -4.4), (4.4, top + 9.6, 3.9),
              lambda p: TR + ((0,) if p.side() and p.j < 2 and p.i % 2 else (255,)), skip=("up", "down"))
    elif not thin:
        m.box("boiler", "Body", (-3, hip + 2, d), (3, top - 1, d + 2.5), lambda p: DK if p.j % 3 == 0 else mul(ST, 0.8))
        m.box("flue", "Body", (1, top - 1, d + 0.4), (2.6, top + 5, d + 2), DK)
    return m


def tread_sentry():
    m = Model("tread_sentry", 646, noise=4)
    ST, DK, TR, GL = (120, 138, 110), (60, 70, 60), (206, 170, 80), (255, 80, 60)
    plate = armour(ST, TR, DK, 4)

    def tread(p):
        if p.face in ("east", "west") and 0.2 < p.ly < 0.8:
            return (40, 40, 44) if p.i % 3 else (120, 120, 126)
        return (40, 40, 44) if (int(math.floor(p.z + 100)) + p.j) % 2 else (90, 90, 96)

    m.pair("tread", "Body", (-7, 0, -8), (-4, 5, 8), tread)
    m.box("hull", "Body", (-4.2, 2, -7), (4.2, 8, 7), plate)
    m.bone("Head", (0, 8, 0))
    m.box("turret", "Head", (-3.5, 8, -4), (3.5, 13, 4), plate, lambda c: c.face == "north" and (c.rect(1, 1, 2, 2, GL), c.px(1, 1, WHITE)))
    m.box("barrel", "Head", (1, 9.5, -12), (2.6, 11.1, -4), lambda p: DK if p.j % 4 == 0 and p.face != "north" else mul(ST, 0.7))
    m.box("antenna", "Head", (-2.8, 13, 2), (-2.2, 18, 2.6), TR)
    m.box("lamp", "Head", (-3, 17.6, 1.8), (-2, 18.6, 2.8), GL, noise=0)
    return m


def scout_drone():
    m = Model("scout_drone", 647, noise=4)
    ST, DK, TR, GL = (190, 196, 204), (84, 90, 104), (240, 160, 60), (110, 230, 255)

    def lens(c):
        if c.face == "north":
            c.rect(c.w // 2 - 2, 1, 4, 4, DK), c.rect(c.w // 2 - 1, 2, 2, 2, GL), c.px(c.w // 2 - 1, 2, WHITE)

    def rotor(p):
        if p.face not in ("up", "down"):
            return DK
        r = math.hypot(p.lx - 0.5, p.lz - 0.5) * 2
        if r > 1 or (r >= 0.2 and (p.i + p.j) % 2 == 0):
            return DK + (0,)
        return DK if r < 0.2 else (200, 210, 220)

    m.box("body", "Body", (-3, 3, -3), (3, 9, 3), lambda p: TR if p.side() and p.j == p.h - 2 else ST, lens)
    m.box("top", "Body", (-2, 9, -2), (2, 10.2, 2), DK)
    m.box("mast", "Body", (-0.4, 10.2, -0.4), (0.4, 12, 0.4), DK)
    m.pair("boom", "Body", (-6.5, 7, -0.5), (-3, 8, 0.5), DK)
    wings(m, (-3, 8, 0), (-10, 8.6, -3.5), (-3, 9, 3.5), rotor, skip=("north", "south", "east", "west"))
    m.pair("skid", "Body", (-2.6, 0, -3), (-1.8, 0.8, 3), DK)
    m.pair("strut", "Body", (-2.5, 0.8, -0.4), (-1.9, 3, 0.4), DK)
    return m


def tl_fish(name, seed):
    """A shark and a whale: the same torpedo, one with a blade of a tail and one with flukes."""
    m = Model(name, seed, noise=5)
    whale = name == "whale"
    TOP, DKC, BL = ((70, 96, 130), (44, 62, 92), (214, 222, 228)) if whale else ((112, 126, 140), (72, 84, 98), (238, 240, 240))
    hw, h, zl = (7, 12, 12) if whale else (3.5, 7, 9)

    def skin(by):
        def sh(p):
            if p.face == "down" or p.y < by:
                return mul(BL, 0.86) if whale and int(math.floor(p.x + 100)) % 2 else BL
            return DKC if vnoise(seed, p.x / 3.0, p.y / 3.0, p.z / 3.0) > 0.64 else TOP
        return sh

    def eye_d(k, y):
        def d(c):
            c.side_eye(k, y, BLACK, WHITE if whale else None)
            if not whale and c.face in ("east", "west"):
                for g in (5, 7, 9):
                    c.rect(c.fx(g), 2, 1, 3, DKC)
        return d

    def flipper(p):
        if p.face in ("up", "down"):
            u = tl_out(p)
            if p.lz < 0.5 * u or p.lz > 1 - 0.2 * u:
                return TOP + (0,)
        return DKC if p.face == "down" else TOP

    yc = h / 2
    m.box("body", "Body", (-hw, 0, -zl), (hw, h, zl), skin(h * 0.4))
    m.bone("Head", (0, yc, -zl))
    if whale:
        def fluke(p):
            if p.face in ("up", "down"):
                u = abs(p.lx - 0.5) * 2
                if p.lz < 0.6 * u - 0.1 or (p.lz > 0.75 and u < 0.2):
                    return TOP + (0,)
            return TOP

        m.box("head", "Head", (-hw + 0.5, 2.5, -zl - 10), (hw - 0.5, h - 0.5, -zl + 0.4), skin(h * 0.4), eye_d(8, 5))
        m.bone("jaw", (0, 2.6, -zl), "Head")
        m.box("jaw", "jaw", (-hw + 1, 0.2, -zl - 9.5), (hw - 1, 2.6, -zl + 0.4), lambda p: BL if p.i % 2 else mul(BL, 0.86))
        m.box("spout", "Body", (-1, h, -zl + 2), (1, h + 0.6, -zl + 4), DKC)
        m.box("hump", "Body", (-0.6, h, 4), (0.6, h + 1.6, 8), DKC)
    else:
        def blade(p):
            if p.side():
                if p.ly > min(1.0, p.lz * 1.5):
                    return TOP + (0,)
            return DKC if p.ly > 0.7 else TOP

        m.box("head", "Head", (-hw + 0.4, 0.6, -zl - 5), (hw - 0.4, h - 0.4, -zl + 0.4), skin(h * 0.4), eye_d(2, 2))
        m.box("snout", "Head", (-hw + 1.2, 2.4, -zl - 9), (hw - 1.2, h - 1.4, -zl - 4.6), skin(3.4))
        m.bone("jaw", (0, 2.2, -zl - 1), "Head")
        m.box("jaw", "jaw", (-hw + 1, 0.8, -zl - 7), (hw - 1, 2.4, -zl - 4.8), tl_jaw(BL, BL))
        m.box("dorsal", "Body", (-0.45, h, -2), (0.45, h + 5, 4), blade, noise=3)
    tl_chain(m, "tail", ((hw * 0.72, yc - h * 0.36, yc + h * 0.36, zl - 0.5, zl + 7), (hw * 0.4, yc - h * 0.2, yc + h * 0.2, zl + 6.5, zl + 13)),
             lambda k, yy: skin(yc - h * 0.1))
    if whale:
        m.box("fluke", "tail-b", (-8, yc - 0.6, zl + 12), (8, yc + 0.6, zl + 18), fluke, noise=3)
        m.pair("flipper", "Body", (-hw - 8, 1.6, -zl + 3), (-hw + 0.2, 2.6, -zl + 8), flipper, rot=(0, 0, 16), origin=(-hw, 2.1, -zl + 5), noise=3)
    else:
        def caudal(p):
            if p.side():
                dv = abs(p.ly - 0.42)
                if not 0.9 * (p.lz - 0.25) < dv < 0.15 + 1.2 * p.lz:
                    return TOP + (0,)
            return TOP

        m.box("caudal", "tail-b", (-0.45, 0, zl + 12), (0.45, yc + 6.5, zl + 18), caudal, noise=3)
        m.pair("flipper", "Body", (-hw - 5, 1, -zl + 3), (-hw + 0.2, 1.8, -zl + 7.5), flipper, rot=(0, 0, 18), origin=(-hw, 1.4, -zl + 5), noise=3)
    return m


def tl_serpent(name, seed):
    """Long swimmers made of a chain of pieces: an eel, and the sea serpent that rules them."""
    boss = name == "leviathan"
    m = Model(name, seed, noise=5)
    if boss:
        SK, DK, BL, FN, EY = (52, 110, 128), (30, 70, 96), (190, 220, 200), (120, 230, 210), (255, 220, 80)
    else:
        SK, DK, BL, FN, EY = (92, 108, 70), (60, 74, 52), (226, 220, 160), (150, 160, 90), (250, 240, 200)
    R, hl, n, fh = (4, 9, 7, 4.0) if boss else (2, 6, 6, 1.6)

    def hide(by):
        return scaly(SK, DK, BL, by) if boss else banded(SK, DK, 5, "z", BL, by)

    fin = spines(FN, mix(FN, WHITE, 0.5), "z", 4 if boss else 3)
    m.box("body", "Body", (-R, 0, -4), (R, 2 * R, 4), hide(R * 0.6))
    m.box("fin", "Body", (-0.4, 2 * R, -3), (0.4, 2 * R + fh, 4), fin, noise=3)
    m.bone("Head", (0, R, -4))
    m.box("head", "Head", (-R - 0.4, R * 0.5, -4 - hl), (R + 0.4, 2 * R + 0.4, -3.6), scaly(SK, DK), tl_teeth(also=tl_eyes(2, 1, BLACK, EY)))
    JP = (0, R * 0.5, -4)
    m.bone("jaw", JP, "Head")
    m.box("jaw", "jaw", (-R, 0, -3.5 - hl), (R, R * 0.5, -4), tl_jaw(SK, BL), rot=(-6, 0, 0), origin=JP)
    segs = tuple((round(R * (1 - 0.11 * (k + 1)), 2), round(R * 0.11 * (k + 1), 2), round(2 * R - R * 0.11 * (k + 1), 2), 3.5 + 7 * k, 11 + 7 * k)
                 for k in range(n))
    tl_chain(m, "tail", segs, lambda k, yy: hide(yy + R * 0.5))
    for k in range(n - 1):
        w, _, top, z0, z1 = segs[k]
        m.box("fin-" + "abcdefgh"[k], "tail-" + "abcdefgh"[k], (-0.4, top, z0), (0.4, round(top + fh * (1 - k / n), 2), z1), fin, noise=3)
    w, low, top, z0, z1 = segs[-1]
    m.box("tail-fin", "tail-" + "abcdefgh"[n - 1], (-0.4, low - 1, z1 - 1), (0.4, top + 1, z1 + 4), lambda p: mix(FN, DK, p.lz * 0.5))
    if boss:
        def frill(p):
            if p.face in ("north", "south"):
                u = tl_out(p)
                if math.hypot(u, (p.ly - 0.4) * 1.8) > 1:
                    return FN + (0,)
                return DK if p.j % 3 == 0 else mix(FN, SK, u)
            return DK

        m.pair("horn", "Head", (-R, 2 * R, -8), (-R + 1.4, 2 * R + 1.4, 0), lambda p: mix((236, 228, 200), DK, 1 - p.lz), rot=(-28, 0, 0),
               origin=(-R + 0.7, 2 * R + 0.4, -8))
        m.pair("frill", "Head", (-R - 5.4, R * 0.6, -5.5), (-R - 0.2, 2 * R + 2, -4.7), frill, skip=("up", "down", "east", "west"), noise=3)
        m.pair("flipper", "Body", (-R - 6, 1, -2), (-R + 0.2, 1.8, 3), lambda p: mix(FN, SK, 1 - tl_out(p)), rot=(0, 0, 14), origin=(-R, 1.4, 0))
    return m


def jelly():
    m = Model("jelly", 652, noise=3)
    BELL, RIM, IN, TN = (236, 226, 240), (200, 170, 226), (250, 190, 210), (226, 206, 240)

    def bell(p):
        if p.side() and p.j == p.h - 1:
            return RIM
        if p.face == "up" and abs(math.hypot(abs(p.lx - 0.5) - 0.2, abs(p.lz - 0.5) - 0.2) - 0.12) < 0.05:
            return IN
        return BELL if (p.i + p.j) % 5 else mix(BELL, RIM, 0.5)

    def strand(p):
        return TN + ((255,) if int(math.floor(p.y + 100)) % 3 else (0,))

    def frilly(p):
        return IN if (p.i + p.j) % 2 else mix(IN, WHITE, 0.4)

    m.box("bell", "Body", (-5, 10, -5), (5, 15, 5), bell)
    m.box("bell-top", "Body", (-3.5, 15, -3.5), (3.5, 17, 3.5), bell)
    m.box("skirt", "Body", (-5.5, 8.6, -5.5), (5.5, 10.2, 5.5), ragged_hem(652, RIM, 1), skip=("up", "down"))
    m.bone("tail-a", (0, 10, 0))
    m.box("arms-a", "tail-a", (-1.5, 5, -1.5), (1.5, 10, 1.5), frilly)
    m.bone("tail-b", (0, 5, 0), "tail-a")
    m.box("arms-b", "tail-b", (-1, 0, -1), (1, 5, 1), frilly)
    for k in range(6):
        ang = math.radians(30 + 60 * k)
        x, z = round(4 * math.sin(ang), 2), round(4 * math.cos(ang), 2)
        m.box("strand-%d" % (k + 1), "Body", (x - 0.3, 1.5 + (k % 2), z - 0.3), (x + 0.3, 9, z + 0.3), strand, noise=0)
    return m


def tl_ratite(name, seed):
    """Big ground birds: an ostrich, a cassowary and (short in the leg, long in the train) a peacock."""
    m = Model(name, seed, noise=5)
    FB, FD, NK, BK, LC, LG, NL = {
        "ostrich": ((52, 46, 46), (30, 28, 30), (226, 178, 170), (214, 170, 120), (206, 170, 160), 13, 13),
        "cassowary": ((36, 36, 44), (20, 20, 28), (60, 110, 210), (90, 84, 70), (120, 116, 100), 10, 8),
        "peacock": ((40, 96, 170), (24, 60, 120), (40, 110, 190), (200, 190, 160), (150, 140, 120), 6, 6),
    }[name]
    small = name == "peacock"
    hw, zl = (2.6, 4.5) if small else (4, 6)
    y0, y1 = LG, LG + hw * 2
    m.box("body", "Body", (-hw, y0, -zl), (hw, y1, zl), shag(seed, FB, FD))
    NP, NR = (0, y1 - 2, -zl + 1), (-14, 0, 0)
    m.bone("neck", NP)
    m.box("neck", "neck", (-1.1, y1 - 2.5, -zl), (1.1, y1 - 2 + NL, -zl + 2.2), NK, rot=NR, origin=NP)
    hx, hy, hz = hp = rotp((0, y1 - 2.5 + NL, -zl + 1.1), NR, NP)
    m.bone("Head", hp, "neck")
    m.box("head", "Head", (-1.5, hy - 0.5, hz - 2.6), (1.5, hy + 2.5, hz + 1.6), NK, tl_eyes(1, 1, BLACK, WHITE))
    m.box("beak", "Head", (-1, hy - 0.2, hz - 5.6), (1, hy + 1.1, hz - 2.4), BK)
    bird_legs(m, hw * 0.55, LG, -0.5, LC, mul(LC, 0.85), th=0.45 if small else 0.7, foot=1.3 if small else 1.8)
    if name == "ostrich":
        m.pair("wing-plume", "Body", (-hw - 1, y0 + 2, -2), (-hw + 0.2, y1 - 1, zl + 1), ragged_hem(seed, WHITE, 2))
        m.box("tail-tuft", "Body", (-2.5, y1 - 4, zl - 0.5), (2.5, y1 + 1, zl + 3), ragged_hem(seed + 1, WHITE, 2))
        m.pair("thigh", ("Leg-L", "Leg-R"), (-hw * 0.55 - 1.3, LG - 4, -1.6), (-hw * 0.55 + 1.3, LG + 0.5, 2), NK)
    elif name == "cassowary":
        def casque(p):
            if p.side() and p.ly > math.sin(math.pi * min(1.0, max(0.0, p.lz))) ** 0.5:
                return BK + (0,)
            return mix(BK, (150, 140, 110), p.ly)

        m.box("casque", "Head", (-0.5, hy + 2.3, hz - 2.4), (0.5, hy + 5.8, hz + 1.6), casque, noise=3)
        m.box("wattle", "Head", (-0.6, hy - 3.6, hz - 2.2), (0.6, hy - 0.5, hz - 1.4), (214, 50, 50))
        m.box("drape", "Body", (-hw - 0.4, y0 - 2.5, -zl + 1), (hw + 0.4, y0 + 2, zl + 2.5), ragged_hem(seed, shag(seed, FB, FD), 3), skip=("up", "down"))
    else:
        GR, GO, BLU = (40, 150, 96), (226, 190, 70), (40, 70, 200)

        def fan(p):
            if p.face not in ("north", "south"):
                return GR
            u, v = (p.lx - 0.5) * 2, p.ly
            r, ray = math.hypot(u, v), int(math.floor(math.atan2(v, u) * 9 / math.pi + 20))
            if r > 1 or r < 0.12 or (r > 0.92 and ray % 2):
                return GR + (0,)
            for ring in (0.82, 0.5):
                if ray % 2 == 0 and abs(r - ring) < 0.09:
                    return BLU if abs(r - ring) < 0.04 else GO
            return GR if ray % 2 == 0 else mul(GR, 0.75)

        m.box("crest", "Head", (-0.4, hy + 2.4, hz - 0.5), (0.4, hy + 5, hz + 2.5), spines(BLU, GO, "z", 2), noise=3)
        m.pair("wing", "Body", (-hw - 0.6, y0 + 1, -zl + 1), (-hw + 0.2, y1 - 0.6, zl - 0.5), lambda p: (150, 120, 90) if (p.i + p.j) % 3 else (110, 86, 64))
        m.box("train", "Body", (-12, y0 + 1, zl - 0.6), (12, y0 + 20, zl + 0.2), fan, skip=("up", "down", "east", "west"), noise=3)
    return m


def tl_mammal(name, seed):
    """Small furry things on four legs: a beaver, a chipmunk and a gazelle."""
    m = Model(name, seed, noise=5)
    FR, FD, BL, hw, y0, h, zl, lw = {
        "beaver": ((124, 84, 54), (88, 58, 40), (176, 136, 96), 3.5, 2.5, 6, 6, 2),
        "chipmunk": ((190, 132, 78), (120, 78, 48), (240, 226, 200), 2, 2, 3.6, 4, 1.2),
        "gazelle": ((206, 160, 104), (120, 80, 50), (246, 240, 228), 2.6, 11, 6, 7, 1.6),
    }[name]
    y1 = y0 + h
    base = fur(seed, FR, FD, BL, y0 + h * 0.3)

    def coat(p):
        if name == "chipmunk" and p.face == "up":
            return FD if abs(p.x) > 1 else BL
        if name == "gazelle" and p.side() and abs(p.y - y0 - h * 0.38) < 0.6:
            return FD
        return base(p)

    def face_d(c):
        tl_eyes(1, 1, BLACK)(c)
        if c.face == "north":
            c.rect(c.w // 2 - 1, c.h - 3, 2, 1, BLACK)
            if name == "beaver":
                c.rect(c.w // 2 - 1, c.h - 2, 2, 2, (240, 150, 60))

    m.box("body", "Body", (-hw, y0, -zl), (hw, y1, zl), coat)
    four_legs(m, -hw, -hw + lw, -zl + 0.3, zl - lw - 0.3, lw, y0, lambda p: FD if p.y < 1 else FR)
    if name == "gazelle":
        NP = (0, y1 - 1, -zl + 0.5)
        m.box("neck", "Body", (-1.3, y1 - 2, -zl - 1.5), (1.3, y1 + 5, -zl + 2), base, rot=(-20, 0, 0), origin=NP)
        hx, hy, hz = hp = rotp((0, y1 + 4.5, -zl), (-20, 0, 0), NP)
        m.bone("Head", hp)
        m.box("head", "Head", (-1.5, hy - 1.5, hz - 5), (1.5, hy + 2, hz + 1), lambda p: BL if p.face == "down" or p.lz < 0.3 else FR, face_d)
        m.pair("horn", "Head", (-1.3, hy + 1.8, hz - 0.6), (-0.7, hy + 7.5, hz), lambda p: (50, 44, 40) if p.j % 2 else (96, 84, 72), rot=(14, 0, -8),
               origin=(-1, hy + 2, hz - 0.3))
        ears(m, (-2.8, hy + 0.6, hz - 0.2), (-1.4, hy + 2.6, hz + 0.4), FR)
        m.box("scut", "Body", (-0.6, y1 - 3.5, zl - 0.2), (0.6, y1 - 0.5, zl + 0.8), FD)
    else:
        hl = hw * 1.5
        m.bone("Head", (0, y0 + h * 0.6, -zl))
        m.box("head", "Head", (-hw + 0.3, y0 + 0.5, -zl - hl), (hw - 0.3, y1 + 0.3, -zl + 0.3), coat, face_d)
        ears(m, (-hw + 0.2, y1, -zl - 1.6), (-hw + 1.4, y1 + 1.3, -zl - 0.8), FD)
        if name == "beaver":
            m.bone("tail", (0, y0 + 1.2, zl))
            m.box("tail", "tail", (-2.5, y0 + 0.6, zl - 0.3), (2.5, y0 + 1.8, zl + 7.5), lambda p: (60, 50, 46) if (p.i + p.j) % 2 else (84, 72, 64))
        else:
            m.bone("tail", (0, y1 - 0.5, zl))
            m.box("tail", "tail", (-1, y1 - 1, zl - 0.2), (1, y1 + 5.5, zl + 1.8), lambda p: FD if p.j % 3 == 0 else FR, rot=(25, 0, 0), origin=(0, y1 - 0.5, zl))
    return m


def shardling():
    m = Model("shardling", 660, noise=3)
    C1, C2, C3, EY = (170, 226, 250), (236, 250, 255), (96, 150, 214), (40, 40, 80)
    m.box("body", "Body", (-3, 2, -3.5), (3, 7, 3.5), crystal(C1, C2, C3), lambda c: c.face == "north" and (c.sym(1, 2, EY), c.sym(1, 3, EY)))
    four_legs(m, -3, -1.4, -3.3, 1.7, 1.6, 2.2, C3)
    for k, (x, z, hgt, rx, rz) in enumerate(((0, 0, 6, 0, 0), (-1.8, -1.5, 4, -14, 16), (1.8, -1, 4.5, -10, -18), (-1.4, 2, 3.6, 16, 14), (1.5, 2.2, 3, 18, -16))):
        m.box("shard-%d" % k, "Body", (x - 0.9, 6.6, z - 0.9), (x + 0.9, 7 + hgt, z + 0.9), lambda p: mix(C1, C2, p.ly), rot=(rx, 0, rz), origin=(x, 7, z))
    return m


def tl_prism(name, seed):
    """Crystal folk: a golem of shards and the monarch that grew a crown of them."""
    boss = name == "prism_monarch"
    m = Model(name, seed, noise=3)
    if boss:
        C1, C2, C3, CORE = (206, 150, 250), (246, 226, 255), (120, 70, 200), (255, 250, 170)
    else:
        C1, C2, C3, CORE = (150, 220, 240), (232, 250, 255), (70, 130, 190), (255, 180, 240)
    cr = crystal(C1, C2, C3)
    legs, arms = ("Leg-L", "Leg-R"), ("Arm-L", "Arm-R")

    def grad(p):
        return mix(C3, C2, p.ly)

    def chest(p):
        d = abs(p.x) + abs(p.y - 17)
        if p.face == "north" and d < 3:
            return CORE if d < 1.8 else C2
        return cr(p)

    m.bone("Leg-L", (-3, 10, 0)), m.bone("Leg-R", (3, 10, 0))
    m.pair("leg", legs, (-4.8, 0, -2), (-1.2, 11, 2), cr)
    m.box("hips", "Body", (-4.5, 10, -2.5), (4.5, 13, 2.5), C3)
    m.box("chest", "Body", (-6.5, 13, -3.5), (6.5, 22, 3.5), chest)
    m.bone("Arm-L", (-8, 20.5, 0)), m.bone("Arm-R", (8, 20.5, 0))
    m.pair("arm", arms, (-9.6, 13, -1.8), (-6.6, 22, 1.8), cr)
    m.pair("forearm", arms, (-10.4, 4.5, -2.4), (-6.2, 13, 2.4), cr)
    m.pair("shoulder-shard", "Body", (-8.5, 21.5, -1.2), (-6.1, 28, 1.2), grad, rot=(0, 0, 22), origin=(-7.3, 22, 0))
    m.bone("Head", (0, 22, 0))
    m.box("head", "Head", (-2.5, 22, -2.5), (2.5, 27.5, 2.5), cr, lambda c: c.face == "north" and (c.sym(1, 2, CORE), c.sym(1, 3, CORE)))
    m.box("head-shard", "Head", (-1, 27.5, -1), (1, 31, 1), grad)
    if boss:
        m.pair("crown", "Head", (-3.6, 27, -0.8), (-2.2, 32, 0.8), grad, rot=(0, 0, 20), origin=(-2.9, 27.2, 0))
        m.box("crown-back", "Head", (-0.8, 27, 1.6), (0.8, 31.5, 3), grad, rot=(20, 0, 0), origin=(0, 27.2, 2.3))
        m.pair("back-shard", "Body", (-4, 19, 2.5), (-2, 30, 4.5), grad, rot=(22, 0, 14), origin=(-3, 19.5, 3.5))
        m.pair("back-shard-low", "Body", (-5.5, 14, 2.5), (-3.9, 22, 4.1), grad, rot=(34, 0, 26), origin=(-4.7, 14.5, 3.3))
        m.pair("mote", "Body", (-14, 17, -1), (-12.4, 23, 0.6), grad, rot=(0, 0, -12), origin=(-13.2, 20, 0))
    return m


def glimmer_ray():
    m = Model("glimmer_ray", 663, noise=3)
    C1, C2, C3, EY = (190, 240, 232), (240, 255, 250), (90, 170, 180), (255, 240, 150)
    cr = crystal(C1, C2, C3)

    def wing(p):
        if p.face not in ("up", "down"):
            return C3
        u = tl_out(p)
        if p.lz < 0.45 * u or p.lz > 1 - 0.5 * u:
            return C1 + (0,)
        return cr(p)

    m.box("body", "Body", (-3, 1, -5), (3, 3.4, 5), cr, lambda c: c.face == "north" and c.sym(0, 0, EY))
    m.box("ridge", "Body", (-1, 3.4, -3), (1, 4.4, 4), C2)
    wings(m, (-3, 2.4, 0), (-14, 2, -5), (-3, 2.8, 5), wing, skip=("north", "south", "east", "west"))
    tl_chain(m, "tail", ((0.7, 1.6, 2.8, 4.5, 10), (0.5, 1.8, 2.6, 9.5, 15), (0.3, 1.9, 2.5, 14.5, 20)), lambda k, yy: mix(C1, C3, k / 3.0))
    m.pair("lobe", "Body", (-1.6, 0, -6), (-0.8, 1.2, -3.5), C3)
    return m


def tl_tree(name, seed):
    """Props: three fruit trees and a tree of crystal, each one block of trunk under a wide crown."""
    m = Model(name, seed, noise=5)
    if name == "prism_tree":
        C1, C2, C3 = (160, 226, 250), (236, 250, 255), (96, 140, 220)
        m.box("trunk", "Body", (-2, 0, -2), (2, 26, 2), crystal((176, 160, 230), (226, 216, 255), (110, 90, 180)))
        shards = ((0, 0, 24, 18, 3, 0, 0), (-3, 0, 20, 14, 2.4, 0, 38), (3, 0, 21, 13, 2.4, 0, -38), (0, -3, 19, 13, 2.2, -38, 0),
                  (0, 3, 22, 12, 2.2, 38, 0), (-2, 2, 12, 9, 1.6, 30, 44), (2, -2, 14, 9, 1.6, -30, -44))
        for k, (x, z, y, hgt, r, rx, rz) in enumerate(shards):
            m.box("shard-%d" % k, "Body", (x - r, y, z - r), (x + r, y + hgt, z + r), lambda p: mix(C1, C2, p.ly) if (p.i - p.j) % 4 else C3,
                  rot=(rx, 0, rz), origin=(x, y, z))
        return m
    FRU = {"pear_tree": (206, 214, 90), "cherry_tree": (206, 40, 60), "orange_tree": (244, 150, 40)}[name]
    LF, LD = {"pear_tree": ((84, 150, 70), (56, 110, 52)), "cherry_tree": ((70, 140, 76), (44, 100, 56)), "orange_tree": ((50, 120, 60), (34, 88, 46))}[name]
    BK, BD = (118, 86, 58), (84, 60, 42)

    def cell(s, p, k):
        return hsh(s, int(math.floor(p.x / k + 60)), int(math.floor(p.y / k + 60)), int(math.floor(p.z / k + 60)))

    def leaves(p):
        if p.face != "down" and cell(seed + 1, p, 2) > 0.9:
            return FRU
        n = cell(seed, p, 1)
        return LD if n > 0.62 else mix(LF, WHITE, 0.12) if n < 0.12 else LF

    m.box("trunk", "Body", (-2, 0, -2), (2, 22, 2), lambda p: BD if p.side() and p.i % 2 == 0 else BK)
    m.box("crown", "Body", (-11, 18, -11), (11, 34, 11), leaves)
    m.box("crown-top", "Body", (-7, 34, -7), (7, 41, 7), leaves)
    m.box("crown-low", "Body", (-8, 15, -8), (8, 18, 8), leaves, skip=("up",))
    m.pair("bough", "Body", (-6, 12, -0.8), (-1.5, 13.6, 0.8), BK, rot=(0, 0, -30), origin=(-1.5, 13, 0))
    return m


TL_MORE = [tl_named(n, build, 630 + k) for k, (n, build) in enumerate((
    ("triceratops", tl_quad_dino), ("stegosaurus", tl_quad_dino), ("brachiosaurus", tl_quad_dino), ("ankylosaurus", tl_quad_dino),
    ("spinosaurus", tl_biped_dino), ("dilophosaurus", tl_biped_dino), ("parasaurolophus", tl_biped_dino),
    ("worker_ant", tl_bug), ("termite", tl_bug), ("cricket", tl_bug), ("tinker_mite", tl_bug),
    ("dragonfly", tl_flutter), ("swallowtail", tl_flutter), ("lanternbug", tl_flutter),
    ("borer", tl_worm), ("gravemaw", tl_worm), ("cogwalker", tl_robot), ("lamplighter", tl_robot), ("iron_regent", tl_robot),
    ("shark", tl_fish), ("whale", tl_fish), ("eel", tl_serpent), ("leviathan", tl_serpent),
    ("ostrich", tl_ratite), ("cassowary", tl_ratite), ("peacock", tl_ratite), ("beaver", tl_mammal), ("chipmunk", tl_mammal), ("gazelle", tl_mammal),
    ("prism_golem", tl_prism), ("prism_monarch", tl_prism),
    ("pear_tree", tl_tree), ("cherry_tree", tl_tree), ("orange_tree", tl_tree), ("prism_tree", tl_tree)))]
TL_MORE += [tread_sentry, scout_drone, jelly, shardling, glimmer_ray]

TITANLANDS = [titan, behemoth, hydra, wyrm, emberwing, stormwing, pocket_dragon, mammoth, sabertooth, raptor, tyrant, pterosaur,
              giant_mantis, giant_scorpion, broodmother, sand_worm, kraken, crystal_stag, lava_golem, ghost_knight, moth, giant_snail,
              thunderbird, cave_troll] + TL_MORE
# <<< titanlands pack


# >>> hearthgarden pack
# ---------------------------------------------------------------- the Hearth & Garden pack
#
# Props: garden and household decorations, written to mods/hearthgarden/models/.
# 16 model units are one block; a prop stands on y = 0 around the origin (the
# two hanging ones, HG_HANGING, are built down from y = 16 instead). Nothing
# here moves: every box hangs on `Body` and articulate() leaves them alone.

HG_DIR = os.path.join(MODS_DIR, "hearthgarden", "models")
HG_HANGING = {"chandelier", "hanging_lantern"}

CLEAR = (0, 0, 0, 0)
HG_OAK, HG_DARK, HG_PINE, HG_WALNUT = (168, 120, 66), (98, 66, 40), (206, 170, 112), (122, 78, 48)
HG_STONE, HG_SLATE, HG_MARBLE, HG_SAND = (140, 140, 146), (98, 100, 110), (224, 222, 214), (204, 188, 150)
HG_LEAF, HG_PINEG, HG_LIME = (70, 138, 60), (44, 100, 58), (124, 180, 72)
HG_IRON, HG_BRASS, HG_GOLD, HG_STEEL = (66, 68, 78), (208, 168, 74), (238, 204, 96), (164, 170, 182)
HG_TERRA, HG_STRAW, HG_WATER, HG_SOIL = (186, 106, 72), (224, 192, 98), (84, 150, 212), (92, 64, 44)
HG_GLOW, HG_CREAM, HG_RED, HG_BARK = (255, 226, 130), (240, 230, 204), (176, 48, 52), (104, 76, 52)


def hg(mid, seed, noise=7):
    """A prop model and its two box makers (one box, a mirrored pair), all on `Body`."""
    m = Model(mid, seed, noise)

    def b(name, lo, hi, sh, dec=None, **kw):
        return m.box(name, "Body", lo, hi, sh, dec, **kw)

    def pr(name, lo, hi, sh, dec=None, **kw):
        m.pair(name, "Body", lo, hi, sh, dec, **kw)

    return m, b, pr


def hg_quad(b, name, lo, hi, sh, dec=None, **kw):
    """The box in the -x/-z corner and its three mirror images."""
    for k, (sx, sz) in enumerate(((1, 1), (-1, 1), (1, -1), (-1, -1))):
        xs, zs = sorted((lo[0] * sx, hi[0] * sx)), sorted((lo[2] * sz, hi[2] * sz))
        b("%s-%d" % (name, k), (xs[0], lo[1], zs[0]), (xs[1], hi[1], zs[1]), sh, dec, **kw)


def hg_walls(b, name, ro, ri, y0, y1, sh, **kw):
    """A square ring of four walls, ro outside and ri inside."""
    b(name + "-n", (-ro, y0, -ro), (ro, y1, -ri), sh, **kw)
    b(name + "-s", (-ro, y0, ri), (ro, y1, ro), sh, **kw)
    b(name + "-w", (-ro, y0, -ri), (-ri, y1, ri), sh, **kw)
    b(name + "-e", (ri, y0, -ri), (ro, y1, ri), sh, **kw)


def hg_wood(base, seed, d="v", period=3):
    """Boards: d = "v" runs them down a face, "h" across it."""
    if d == "v":
        return planks(base, lambda p: p.i, lambda p: p.j, period, seed)
    return planks(base, lambda p: p.j, lambda p: p.i, period, seed)


def hg_log(bark, core, seed, axis="z"):
    """A log lying along axis: growth rings on its ends, streaky bark around."""
    ends = {"x": ("east", "west"), "y": ("up", "down"), "z": ("north", "south")}[axis]
    a = "xyz".index(axis)

    def sh(p):
        if p.face in ends:
            if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
                return bark
            d = max(abs(p.i - (p.w - 1) / 2.0), abs(p.j - (p.h - 1) / 2.0))
            return mul(core, 0.8) if int(d + 0.5) % 2 == 0 else core
        q = [math.floor(v) for v in (p.x, p.y, p.z)]
        t = q[a]
        q[a] = 0
        c = mul(bark, 0.84 + 0.3 * hsh(seed, q[0], q[1], q[2]))
        if hsh(seed + 1, q[0], q[1], q[2], t // 3) > 0.78:
            c = mul(c, 0.78)
        return c
    return sh


def hg_stone(base, seed, moss=None):
    def sh(p):
        c = mul(base, 0.86 + 0.26 * vnoise(seed, p.x / 2.3, p.y / 2.3, p.z / 2.3))
        h = hsh(seed, math.floor(p.x * 2), math.floor(p.y * 2), math.floor(p.z * 2))
        if h > 0.88:
            c = mul(c, 0.74)
        elif h < 0.08:
            c = mul(c, 1.14)
        if moss and (p.face == "up" or p.j == 0) and vnoise(seed + 3, p.x / 1.7, p.y, p.z / 1.7) > 0.5:
            c = mix(c, moss, 0.8)
        return c
    return sh


def hg_leaves(base, seed, dots=(), thr=0.9):
    """Foliage: light and dark leaf texels, with flowers or berries (dots) here and there."""
    def sh(p):
        q = (math.floor(p.x), math.floor(p.y), math.floor(p.z))
        c = mix(mul(base, 0.68), mul(base, 1.24), hsh(seed, *q))
        if vnoise(seed, p.x / 3.1, p.y / 3.1, p.z / 3.1) > 0.6:
            c = mul(c, 0.84)
        if dots and hsh(seed + 9, *q) > thr:
            return dots[int(hsh(seed + 17, *q) * len(dots))]
        return c
    return sh


def hg_metal(base):
    """Metal: a bright top edge, a dark foot, one streak of shine."""
    def sh(p):
        if p.face == "up":
            return mul(base, 1.2)
        if p.face == "down":
            return mul(base, 0.7)
        c = base
        if p.j == 0:
            c = mul(base, 1.3)
        elif p.j == p.h - 1 and p.h > 2:
            c = mul(base, 0.72)
        if p.w > 2 and p.i == p.w // 3:
            c = mix(c, WHITE, 0.3)
        return c
    return sh


def hg_cloth(base, alt, kind="stripe", n=3):
    def sh(p):
        if kind == "stripe":
            k = p.i // n % 2
        elif kind == "band":
            k = p.j // n % 2
        elif kind == "check":
            k = (p.i // n + p.j // n) % 2
        else:
            k = (p.i + p.j) % 2
        c = alt if k else base
        return mul(c, 0.9) if (p.i * 3 + p.j) % 5 == 0 else c
    return sh


def hg_bricks(base, mortar, seed, bw=4, bh=3):
    def sh(p):
        row = p.j // bh
        s = p.i + (row % 2) * (bw // 2)
        if p.j % bh == bh - 1 or s % bw == bw - 1:
            return mortar
        return mul(base, 0.8 + 0.36 * hsh(seed, row, s // bw, FACES.index(p.face)))
    return sh


def hg_thatch(base, seed, d="v"):
    def sh(p):
        u, v = (p.i, p.j) if d == "v" else (p.j, p.i)
        c = mul(base, 0.78 + 0.4 * hsh(seed, u, v // 3, FACES.index(p.face)))
        return mul(c, 0.8) if hsh(seed + 1, u, v) > 0.86 else c
    return sh


def hg_edge(sh, col, t=1, faces=None):
    """sh with a border of col round every face (or just the faces named)."""
    f = sh if callable(sh) else (lambda p: sh)

    def out(p):
        if (faces is None or p.face in faces) and min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) < t:
            return col
        return f(p)
    return out


def hg_top(side, top):
    """One shader for the top face, another for the rest."""
    f = side if callable(side) else (lambda p: side)
    g = top if callable(top) else (lambda p: top)
    return lambda p: g(p) if p.face == "up" else f(p)


def hg_pot(base, band=None):
    def sh(p):
        if p.face == "up":
            return mul(base, 0.8)
        if p.side():
            if p.j == 0:
                return mul(base, 1.16)
            if band and p.j in (2, 3):
                return band if (p.i + p.j) % 2 else mul(band, 0.8)
            if hsh(77, p.i, p.j, FACES.index(p.face)) > 0.86:
                return mul(base, 0.84)
        return base
    return sh


def hg_frond(col, seed):
    """A leaf seen from above: a midrib with leaflets, cut out between them. The tip is at -z."""
    def sh(p):
        k = int(p.lz * p.h)
        if p.i == p.w // 2:
            return mul(col, 0.7)
        if k == 0 or k % 2 == 1 or (k == 1 and p.i in (0, p.w - 1)):
            return CLEAR
        if p.w > 3 and p.i in (0, p.w - 1) and k % 4 == 0:
            return CLEAR
        return mul(col, 0.9 + 0.3 * hsh(seed, k, p.i))
    return sh


def hg_fire(p):
    return mix((255, 236, 130), (234, 86, 30), p.ly * 1.1)


def hg_glass(frame, glow=HG_GLOW):
    """A lantern pane: glowing glass in a frame with a glazing bar."""
    def sh(p):
        if not p.side():
            return frame
        if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0 or (p.w > 4 and p.i == p.w // 2):
            return frame
        d = abs(p.i - (p.w - 1) / 2.0) / p.w + abs(p.j - (p.h - 1) / 2.0) / p.h
        return mix((255, 246, 200), glow, d * 2.2)
    return sh


def hg_bloom(b, name, x, z, y0, y1, col, eye=HG_GOLD, r=1.0, stem=(64, 126, 58)):
    """A flower: a stalk from y0 to y1 with a blossom on it."""
    b(name + "-stalk", (x - 0.4, y0, z - 0.4), (x + 0.4, y1, z + 0.4), stem, skip=("down",))
    b(name, (x - r, y1, z - r), (x + r, y1 + 2 * r * 0.8, z + r),
      lambda p: eye if (p.face == "up" and 0 < p.i < p.w - 1 and 0 < p.j < p.h - 1) else (mul(col, 0.84) if (p.i + p.j) % 2 else col), noise=4)


# ---- garden and wild

def boulder():
    m, b, pr = hg("boulder", 701)
    st = hg_stone(HG_STONE, 701, moss=(86, 132, 70))
    b("core", (-6, 0, -5), (6, 8, 5), st)
    b("crown", (-4, 8, -3.5), (3, 11, 3), st)
    b("lump-w", (-7.5, 0, -2), (-6, 5, 4), st)
    b("lump-e", (6, 0, -4), (7.5, 6, 2), st)
    b("lump-n", (-3, 0, -7), (4, 5, -5), st)
    b("lump-s", (-4, 0, 5), (2, 6, 6.5), st)
    b("chip", (4.5, 0, 4.5), (7, 2, 7), hg_stone(HG_SLATE, 702))
    return m


def rock_cairn():
    m, b, pr = hg("rock_cairn", 703)
    tones = [(128, 128, 136), (152, 146, 138), (116, 122, 132), (162, 156, 148), (134, 130, 126)]
    specs = [((-6, 0, -5), (6, 4, 5), 0), ((-4.5, 4, -4), (4, 7, 4.5), 14), ((-3.5, 7, -3), (3.5, 10, 3), -10),
             ((-2.5, 10, -2.5), (2, 12.5, 2), 22), ((-1.5, 12.5, -1.5), (1.5, 15, 1.5), 45)]
    for k, (lo, hi, ry) in enumerate(specs):
        b("stone-%d" % k, lo, hi, hg_stone(tones[k], 703 + k, moss=(90, 134, 74) if k == 0 else None), rot=(0, ry, 0))
    b("pebble-a", (5, 0, 4.5), (7.5, 2, 7), hg_stone(tones[2], 709))
    b("pebble-b", (-7.5, 0, -7), (-5.5, 1.5, -5.5), hg_stone(tones[3], 710))
    return m


def bush():
    m, b, pr = hg("bush", 711)
    lf = hg_leaves(HG_LEAF, 711)
    b("stem", (-1, 0, -1), (1, 2, 1), HG_BARK)
    b("core", (-6, 1, -6), (6, 10, 6), lf)
    b("crown", (-4, 10, -4), (4, 12.5, 4), lf)
    b("lump-w", (-7.5, 2, -3.5), (-6, 8, 3), lf)
    b("lump-e", (6, 3, -3), (7.5, 9, 4), lf)
    b("lump-n", (-3, 2, -7.5), (4, 8, -6), lf)
    b("lump-s", (-4, 3, 6), (3, 9, 7.5), lf)
    b("sprig", (2, 12.5, -1), (3, 14, 0), HG_LIME)
    return m


def berry_bush():
    m, b, pr = hg("berry_bush", 715)
    lf = hg_leaves((58, 124, 70), 715, dots=((204, 44, 66), (150, 30, 52)), thr=0.9)

    def berry(p):
        return (255, 170, 180) if (p.face == "up" and p.i == 0 and p.j == 0) else (204, 44, 66)

    b("stem", (-1, 0, -1), (1, 3, 1), HG_BARK)
    b("core", (-7, 2, -5), (7, 9, 5), lf)
    b("mid", (-5, 2, -7), (5, 8, 7), lf)
    b("crown", (-5, 9, -4), (4, 11.5, 4), lf)
    b("tuft", (-2, 11.5, -2), (1, 13, 1), lf)
    for k, (x, y, z) in enumerate(((-7.7, 6, -2), (6.3, 5, 1), (-2, 6, -7.7), (3, 4, 6.3), (1, 11, 3.2), (-5, 9.2, -4.8))):
        b("berry-%d" % k, (x, y, z), (x + 1.4, y + 1.4, z + 1.4), berry, noise=3)
    return m


def hedge():
    m, b, pr = hg("hedge", 717)
    lf = hg_leaves((52, 116, 56), 717)

    def trim(p):
        return mul(lf(p), 0.84) if p.side() and (p.j == 0 or p.i in (0, p.w - 1)) else lf(p)

    pr("stem", (-5.5, 0, -1), (-3.5, 2, 1), HG_BARK)
    b("box", (-8, 2, -5), (8, 14, 5), trim)
    b("cap", (-7, 14, -4), (7, 15, 4), lf)
    b("shoot-a", (-3, 15, -1), (-2, 16.5, 0), HG_LIME)
    b("shoot-b", (4, 15, 1), (5, 16, 2), HG_LIME)
    return m


def topiary():
    m, b, pr = hg("topiary", 719)
    lf = hg_leaves((60, 128, 58), 719)
    b("pot", (-3, 0, -3), (3, 4, 3), hg_pot(HG_TERRA))
    b("rim", (-3.5, 4, -3.5), (3.5, 5, 3.5), hg_top(mul(HG_TERRA, 1.1), HG_SOIL))
    b("trunk", (-1, 5, -1), (1, 15, 1), hg_log(HG_BARK, HG_PINE, 719, "y"))
    b("ball", (-5, 14, -5), (5, 24, 5), lf)
    b("ball-y", (-4, 13, -4), (4, 25, 4), lf)
    b("ball-x", (-6, 16, -3.5), (6, 22, 3.5), lf)
    b("ball-z", (-3.5, 16, -6), (3.5, 22, 6), lf)
    return m


def fern():
    m, b, pr = hg("fern", 721, noise=5)
    b("clump", (-2, 0, -2), (2, 3, 2), hg_leaves((48, 110, 52), 721))
    b("crozier", (-0.5, 3, -0.5), (0.5, 7, 0.5), HG_LIME)
    b("curl", (-0.5, 6, -1.5), (0.5, 7, -0.5), HG_LIME)
    for k in range(8):
        col = (66, 142, 62) if k % 2 == 0 else (52, 118, 56)
        b("frond-%d" % k, (-1.5, 2, -9 if k % 2 == 0 else -7), (1.5, 2.5, 0), hg_frond(col, 721 + k),
          rot=(28 if k % 2 == 0 else 52, k * 45, 0), origin=(0, 2, 0), skip=("north", "south", "east", "west"))
    return m


def cattail():
    m, b, pr = hg("cattail", 723, noise=5)
    GR, HD = (86, 140, 70), (112, 70, 44)
    b("tuft", (-3, 0, -3), (3, 2, 3), hg_leaves((70, 124, 66), 723))
    for k, (x, z, h) in enumerate(((-1.5, -1, 20), (1.5, 0.5, 25), (0, 2, 16), (-2.5, 1.5, 12))):
        b("reed-%d" % k, (x - 0.4, 2, z - 0.4), (x + 0.4, h, z + 0.4), GR)
        if k < 3:
            b("spike-%d" % k, (x - 0.9, h - 6, z - 0.9), (x + 0.9, h - 1, z + 0.9),
              lambda p: mul(HD, 0.84) if (p.i + p.j) % 3 == 0 else HD)
    for k, (ry, rz, h) in enumerate(((0, 12, 15), (120, -14, 18), (240, 16, 11))):
        b("blade-%d" % k, (-0.8, 1, -0.2), (0.8, h, 0.2), lambda p: mul(GR, 0.8) if p.i == p.w // 2 else mul(GR, 1.1),
          rot=(0, ry, rz), origin=(0, 1, 0), skip=("east", "west", "up", "down"))
    return m


def sunflower():
    m, b, pr = hg("sunflower", 725, noise=5)
    GR, YL, BR = (74, 136, 60), (248, 204, 52), (104, 70, 40)

    def petals(p):
        d = math.hypot(p.i - (p.w - 1) / 2.0, p.j - (p.h - 1) / 2.0)
        if d > 5.3 or (d > 3.4 and (p.i + p.j) % 2 == 1):
            return CLEAR
        return mix(YL, (240, 150, 40), (d - 2.5) / 5.0)

    def seeds(p):
        return mul(BR, 0.76) if (p.i + p.j) % 2 else BR

    b("mound", (-2, 0, -2), (2, 1, 2), HG_SOIL)
    b("stalk", (-0.7, 1, -0.7), (0.7, 23, 0.7), lambda p: mul(GR, 0.84) if p.j % 5 == 0 else GR)
    b("leaf-a", (-5, 9, -1.5), (-0.7, 9.6, 1.5), GR, rot=(0, 0, 18), origin=(-0.7, 9, 0))
    b("leaf-b", (0.7, 13, -1.5), (5, 13.6, 1.5), GR, rot=(0, 0, -18), origin=(0.7, 13, 0))
    b("petals", (-5, 19, -2), (5, 29, -1.4), petals, skip=("up", "down", "east", "west"))
    b("disc", (-2.5, 21.5, -2.8), (2.5, 26.5, -1.4), seeds)
    b("calyx", (-2, 22, -1.4), (2, 26, -0.4), mul(GR, 0.84))
    return m


def flower_bed():
    m, b, pr = hg("flower_bed", 727)
    wd = hg_wood(HG_OAK, 727, "v")
    b("planter", (-8, 0, -5), (8, 5, 5), hg_edge(wd, HG_DARK, faces=("up",)))
    b("soil", (-7, 5, -4), (7, 5.6, 4), HG_SOIL, skip=("down",))
    cols = [(232, 70, 84), (250, 208, 70), (150, 110, 220), (250, 246, 240), (242, 138, 60), (232, 120, 180)]
    for k, (x, z, h) in enumerate(((-5.5, -2, 10), (-2.5, 1.5, 12.5), (0.5, -2, 9), (3, 1.5, 11.5), (5.8, -1.5, 13), (-5, 2.5, 8.5))):
        hg_bloom(b, "bloom-%d" % k, x, z, 5.6, h, cols[k], eye=HG_GOLD if k != 1 else (120, 76, 40))
    lf = hg_leaves((70, 140, 62), 728)
    b("greens-a", (-4, 5.6, -3.5), (-1, 7.6, -1), lf)
    b("greens-b", (1.5, 5.6, 0), (4.5, 8, 3), lf)
    b("greens-c", (5, 5.6, 1.5), (7, 7, 3.5), lf)
    return m


def flowerpot():
    m, b, pr = hg("flowerpot", 729)
    GR = (72, 138, 62)
    b("pot", (-3, 0, -3), (3, 5, 3), hg_pot(HG_TERRA, band=(232, 214, 180)))
    b("rim", (-3.5, 5, -3.5), (3.5, 6.5, 3.5), hg_top(mul(HG_TERRA, 1.1), HG_SOIL))
    b("stalk", (-0.5, 6.5, -0.5), (0.5, 11, 0.5), GR)
    b("leaf-a", (-3.5, 7.5, -1), (-0.5, 8.1, 1), GR, rot=(0, 0, 20), origin=(-0.5, 7.5, 0))
    b("leaf-b", (0.5, 8.5, -1), (3.5, 9.1, 1), GR, rot=(0, 0, -20), origin=(0.5, 8.5, 0))
    b("petals-x", (-2.5, 11, -1), (2.5, 13, 1), (226, 62, 84))
    b("petals-z", (-1, 11, -2.5), (1, 13, 2.5), (226, 62, 84))
    b("eye", (-1, 12.5, -1), (1, 13.6, 1), HG_GOLD)
    return m


def potted_palm():
    m, b, pr = hg("potted_palm", 731, noise=5)
    BL = (70, 110, 160)
    b("pot", (-4, 0, -4), (4, 6, 4), hg_pot(BL, band=(226, 232, 240)))
    b("rim", (-4.5, 6, -4.5), (4.5, 7, 4.5), hg_top(mul(BL, 1.15), HG_SOIL))
    tr = lambda p: mul((150, 112, 70), 0.74) if p.j % 3 == 0 else (150, 112, 70)
    b("trunk-a", (-1.5, 7, -1.5), (1.5, 13, 1.5), tr)
    b("trunk-b", (-1.2, 13, -1), (1.3, 19, 1.5), tr)
    b("trunk-c", (-0.8, 19, -0.6), (1.2, 24, 1.4), tr)
    b("crown", (-1.3, 23.5, -1), (1.7, 25.5, 2), (60, 120, 56))
    for k in range(7):
        col = (64, 146, 66) if k % 2 == 0 else (48, 122, 58)
        b("frond-%d" % k, (-2.3, 25, -11 if k % 2 == 0 else -9), (2.7, 25.5, 0.4), hg_frond(col, 731 + k),
          rot=(18 if k % 2 == 0 else -16, k * 51.4 + 10, 0), origin=(0.2, 25, 0.4), skip=("north", "south", "east", "west"))
    return m


def ornamental_tree():
    m, b, pr = hg("ornamental_tree", 735)
    lf = hg_leaves((62, 132, 66), 735, dots=((244, 176, 200), (250, 214, 226)), thr=0.9)
    bark = hg_log(HG_BARK, HG_PINE, 735, "y")
    b("roots", (-2.5, 0, -2.5), (2.5, 2, 2.5), bark)
    b("trunk", (-1.5, 2, -1.5), (1.5, 20, 1.5), bark)
    b("bough-w", (-5, 15, -0.7), (-1.5, 16.4, 0.7), HG_BARK, rot=(0, 0, -28), origin=(-1.5, 15.5, 0))
    b("bough-e", (1.5, 13, -0.7), (5, 14.4, 0.7), HG_BARK, rot=(0, 0, 28), origin=(1.5, 13.5, 0))
    b("canopy", (-7, 18, -7), (7, 30, 7), lf)
    b("canopy-x", (-8, 21, -5), (8, 27, 5), lf)
    b("canopy-z", (-5, 21, -8), (5, 27, 8), lf)
    b("canopy-up", (-5, 30, -5), (5, 37, 5), lf)
    b("tip", (-3, 37, -3), (3, 41.5, 3), lf)
    return m


def stump():
    m, b, pr = hg("stump", 737)
    lg = hg_log((98, 72, 50), (206, 172, 116), 737, "y")
    b("bole", (-5, 0, -5), (5, 7, 5), lg)
    b("root-w", (-7.5, 0, -1.5), (-5, 3, 1.5), lg)
    b("root-e", (5, 0, -2), (7, 2.5, 1), lg)
    b("root-n", (-1, 0, -7.5), (2, 3, -5), lg)
    b("root-s", (-2, 0, 5), (1, 2, 7), lg)
    b("splinter", (2, 7, 2), (4, 9, 4), (188, 152, 100))
    b("shelf-fungus", (-6.5, 4, 1.5), (-5, 5, 4), (232, 206, 160))
    b("sprout", (-3, 7, -3), (-2.2, 9.5, -2.2), HG_LIME)
    b("sprout-leaf", (-3.6, 9.5, -3.2), (-1.6, 10.1, -2), HG_LIME)
    return m


def log_pile():
    m, b, pr = hg("log_pile", 739)
    barks = [(104, 76, 52), (120, 90, 62), (92, 70, 50), (132, 104, 78), (110, 84, 60), (98, 78, 58)]
    spots = [(-7.5, 0), (-2.5, 0), (2.5, 0), (-5, 5), (0, 5), (-2.5, 10)]
    for k, (x, y) in enumerate(spots):
        dz = (0, 1, -0.5, 0.5, -1, 0)[k]
        b("log-%d" % k, (x, y, -7 + dz), (x + 5, y + 5, 7 + dz), hg_log(barks[k], (212, 178, 122), 739 + k, "z"))
    pr("stake", (-8.6, 0, -0.75), (-7.5, 9, 0.75), HG_DARK)
    return m


def mushroom_ring():
    m, b, pr = hg("mushroom_ring", 745, noise=5)
    ST = (236, 226, 204)
    caps = [(196, 60, 50), (212, 122, 60), (176, 52, 60)]
    for k in range(7):
        a = math.radians(k * 360 / 7.0 + 12)
        x, z = round(math.sin(a) * 5.6 * 2) / 2.0, round(math.cos(a) * 5.6 * 2) / 2.0
        h, r = (3, 4.5, 2.5, 4, 3, 5, 2)[k], (1.5, 2, 1, 2, 1.5, 2, 1)[k]
        cap = caps[k % 3]
        b("stalk-%d" % k, (x - 0.5, 0, z - 0.5), (x + 0.5, h, z + 0.5), ST)
        b("cap-%d" % k, (x - r, h, z - r), (x + r, h + 1.5, z + r),
          lambda p, cap=cap, k=k: (250, 244, 230) if (p.face != "down" and hsh(745 + k, p.i, p.j, FACES.index(p.face)) > 0.8) else (ST if p.face == "down" else cap))
    b("moss", (-1.5, 0, -1.5), (1.5, 0.8, 1.5), hg_leaves((70, 128, 64), 745))
    return m


def pumpkin():
    m, b, pr = hg("pumpkin", 747)
    OR = (232, 132, 40)

    def rind(p):
        if p.side() and p.i % 3 == 0:
            return mul(OR, 0.78)
        if p.face == "up" and (p.i % 3 == 0 or p.j % 3 == 0):
            return mul(OR, 0.84)
        return OR

    b("flesh", (-6, 0, -6), (6, 9, 6), rind)
    b("bulge-x", (-7, 1, -4), (7, 8, 4), rind)
    b("bulge-z", (-4, 1, -7), (4, 8, 7), rind)
    b("shoulder", (-4, 9, -4), (4, 10, 4), rind)
    b("stalk", (-1, 10, -1), (1, 12.5, 1), (104, 128, 60))
    b("stalk-bend", (-1, 11.5, -2.5), (1, 12.5, -1), (104, 128, 60))
    b("leaf", (1, 10, 0), (5, 10.6, 3.5), (78, 140, 62))
    b("tendril", (-4, 10, 1), (-1, 10.5, 1.6), (120, 170, 80))
    return m


def hay_bale():
    m, b, pr = hg("hay_bale", 749)
    hay = hg_thatch(HG_STRAW, 749, "h")
    b("bale", (-8, 0, -6), (8, 10, 6), lambda p: hg_thatch(HG_STRAW, 749, "v")(p) if p.face in ("east", "west") else hay(p))
    pr("twine", (-5, 0, -6.2), (-4, 10.2, 6.2), (150, 104, 60))
    b("wisp-a", (-2, 10, -3), (3, 10.6, -2.4), HG_STRAW, rot=(0, 20, 0))
    b("wisp-b", (1, 10, 2), (6, 10.6, 2.6), mul(HG_STRAW, 0.9), rot=(0, -14, 0))
    b("wisp-c", (-7, 0, -7.4), (-2, 0.6, -6.6), HG_STRAW, rot=(0, 10, 0))
    return m


def lily_pad():
    m, b, pr = hg("lily_pad", 751, noise=5)
    GR = (64, 136, 74)

    def pad(p):
        cx, cy = (p.w - 1) / 2.0, (p.h - 1) / 2.0
        dx, dy = p.i - cx, p.j - cy
        d = math.hypot(dx, dy)
        if d > min(p.w, p.h) / 2.0 or (dx > 0 and abs(dy) < dx * 0.45):
            return CLEAR
        if p.face == "down":
            return mul(GR, 0.7)
        return mul(GR, 0.84) if (abs(dx) < 0.6 or abs(dy) < 0.6 or d > min(p.w, p.h) / 2.0 - 1) else GR

    side = ("north", "south", "east", "west")
    b("pad-a", (-7.5, 0, -7), (0.5, 0.5, 1), pad, skip=side)
    b("pad-b", (0, 0, -2), (7, 0.5, 5), pad, skip=side, rot=(0, 120, 0))
    b("pad-c", (-5, 0, 2), (0, 0.5, 7), pad, skip=side, rot=(0, -100, 0))
    PK = (246, 190, 214)
    b("petals-x", (-5.5, 0.5, -4), (-1.5, 1.4, -2), PK)
    b("petals-z", (-4.5, 0.5, -5), (-2.5, 1.4, -1), PK)
    b("petals-in", (-4.5, 1.4, -4), (-2.5, 2.6, -2), WHITE)
    b("eye", (-4, 2.6, -3.5), (-3, 3.2, -2.5), HG_GOLD)
    b("bud", (3, 0.5, 1), (4.2, 2.4, 2.2), lambda p: PK if p.ly > 0.5 else GR)
    return m


def bird_bath():
    m, b, pr = hg("bird_bath", 753)
    st = hg_stone((176, 174, 168), 753, moss=(110, 146, 96))
    b("foot", (-4, 0, -4), (4, 2, 4), st)
    b("plinth", (-2.5, 2, -2.5), (2.5, 3.5, 2.5), st)
    b("column", (-1.5, 3.5, -1.5), (1.5, 10, 1.5), st)
    b("collar", (-2.5, 10, -2.5), (2.5, 11, 2.5), st)
    b("bowl", (-6, 11, -6), (6, 12.5, 6), st)
    hg_walls(b, "lip", 7, 5.5, 12.5, 14.5, st)
    b("water", (-5.5, 12.5, -5.5), (5.5, 13.6, 5.5),
      lambda p: (190, 224, 246) if hsh(753, p.i, p.j) > 0.86 else HG_WATER, skip=("down", "north", "south", "east", "west"), noise=3)
    b("leaf-afloat", (1, 13.6, -3), (3, 13.9, -1.5), (196, 150, 60), skip=("down",))
    return m


def fountain():
    m, b, pr = hg("fountain", 755)
    st = hg_stone((168, 168, 172), 755)
    wt = lambda p: (196, 228, 248) if hsh(755, p.i, p.j, FACES.index(p.face)) > 0.84 else HG_WATER
    side = ("down", "north", "south", "east", "west")
    b("floor", (-8, 0, -8), (8, 2, 8), st)
    hg_walls(b, "basin", 8, 6.5, 2, 6, st)
    b("pool", (-6.5, 2, -6.5), (6.5, 5, 6.5), wt, skip=side, noise=3)
    b("pillar", (-2, 2, -2), (2, 12, 2), st)
    b("dish", (-5, 12, -5), (5, 13.5, 5), st)
    hg_walls(b, "dish-lip", 5, 4, 13.5, 15.5, st)
    b("dish-pool", (-4, 13.5, -4), (4, 14.8, 4), wt, skip=side, noise=3)
    b("spout", (-1, 13.5, -1), (1, 19, 1), st)
    b("jet", (-1.5, 19, -1.5), (1.5, 20.5, 1.5), wt, noise=3)
    for k, (x, z) in enumerate(((0, -5.3), (0, 5.3), (-5.3, 0), (5.3, 0))):
        w, d = (1, 0.3) if x == 0 else (0.3, 1)
        b("fall-%d" % k, (x - w, 5, z - d), (x + w, 15, z + d), lambda p: (214, 236, 250) if p.j % 3 == 0 else HG_WATER, noise=3)
    return m


def well():
    m, b, pr = hg("well", 757)
    st = hg_bricks((146, 144, 150), (100, 100, 108), 757, 4, 3)
    wd = hg_wood(HG_WALNUT, 757, "v")

    def tiles(p):
        k = p.j if p.face == "up" else p.i
        c = (168, 76, 58)
        return mul(c, 0.78) if (k % 3 == 0) else mul(c, 0.9 + 0.2 * hsh(757, p.i // 2, p.j // 2))

    hg_walls(b, "ring", 7, 5, 0, 8, st)
    b("water", (-5, 0.5, -5), (5, 4, 5), (40, 70, 110), skip=("down", "north", "south", "east", "west"), noise=3)
    pr("upright", (-7, 8, -1), (-5.5, 22, 1), wd)
    b("roof-a", (-8, 22, -6), (8, 23.5, 6), tiles)
    b("roof-b", (-8, 23.5, -4), (8, 25, 4), tiles)
    b("roof-c", (-8, 25, -2), (8, 26.5, 2), tiles)
    b("windlass", (-5.5, 17.5, -0.8), (5.5, 19.1, 0.8), hg_log(HG_OAK, HG_PINE, 758, "x"))
    b("crank", (7, 16, -0.4), (7.8, 19, 0.4), HG_IRON)
    b("crank-grip", (7.8, 16, -0.4), (9, 16.8, 0.4), HG_IRON)
    b("rope", (-0.3, 12, -0.3), (0.3, 17.5, 0.3), (206, 186, 130))
    b("bucket", (-1.5, 9, -1.5), (1.5, 12, 1.5), hg_edge(hg_wood(HG_OAK, 759, "v", 2), HG_IRON, faces=("up",)))
    return m


def beehive():
    m, b, pr = hg("beehive", 761)
    WH, BL = (236, 228, 200), (150, 190, 204)

    def super_(col):
        def sh(p):
            if p.side() and p.j == p.h - 1:
                return mul(col, 0.7)
            return mul(col, 0.92) if p.i % 4 == 0 and p.side() else col
        return sh

    def door(c):
        if c.face == "north":
            c.rect(c.w // 2 - 3, c.h - 2, 6, 1, (40, 30, 24))
            c.px(c.w // 2 - 1, c.h - 2, (240, 190, 50))

    hg_quad(b, "foot", (-6, 0, -5), (-4.5, 3, -3.5), HG_DARK)
    b("board", (-7, 3, -6.5), (7, 4, 6), hg_wood(HG_OAK, 761, "v"))
    b("brood", (-6, 4, -5), (6, 9, 5), super_(WH), door)
    b("honey", (-6, 9, -5), (6, 13, 5), super_(BL))
    b("lid", (-7, 13, -6), (7, 14.5, 6), hg_metal((150, 152, 160)))
    b("ridge", (-5, 14.5, -4), (5, 15.5, 4), hg_metal((150, 152, 160)))
    for k, (x, y, z) in enumerate(((-3, 6, -7.5), (2.5, 10, -6.8), (7.2, 12, 1))):
        b("bee-%d" % k, (x, y, z), (x + 1, y + 1, z + 1.4), lambda p: (40, 34, 30) if (p.j if p.face in ("up", "down") else p.i) % 2 else (246, 196, 50), noise=0)
    return m


def birdhouse():
    m, b, pr = hg("birdhouse", 763)
    wd = hg_wood((214, 184, 132), 763, "v")
    RF = (150, 62, 56)

    def hole(c):
        if c.face == "north":
            c.rect(c.w // 2 - 1, 1, 2, 2, (36, 26, 22))

    b("mound", (-2.5, 0, -2.5), (2.5, 1, 2.5), hg_leaves((84, 136, 68), 763))
    b("pole", (-1, 1, -1), (1, 18, 1), hg_wood(HG_DARK, 764, "v", 2))
    b("bracket", (-2, 16.5, -2), (2, 18, 2), HG_DARK)
    b("deck", (-4, 18, -4.5), (4, 19, 4), wd)
    b("house", (-3, 19, -3), (3, 25, 3), wd, hole)
    b("peg", (-0.4, 20, -4.6), (0.4, 20.8, -3), HG_DARK)
    b("roof-a", (-4.5, 25, -4.5), (4.5, 26.5, 4.5), hg_cloth(RF, mul(RF, 0.8), "band", 1))
    b("roof-b", (-3, 26.5, -4.5), (3, 28, 4.5), hg_cloth(RF, mul(RF, 0.8), "band", 1))
    b("roof-c", (-1.2, 28, -4.5), (1.2, 29.2, 4.5), mul(RF, 0.9))
    return m


def garden_statue():
    m, b, pr = hg("garden_statue", 765)
    mb = hg_stone(HG_MARBLE, 765)
    veined = lambda p: mul(mb(p), 0.82) if abs(vnoise(766, p.x / 2.0, p.y / 5.0, p.z / 2.0) - 0.5) < 0.04 else mb(p)
    st = hg_stone((150, 150, 154), 767, moss=(96, 140, 86))
    b("plinth", (-5, 0, -5), (5, 3, 5), st)
    b("block", (-4, 3, -4), (4, 8, 4), hg_edge(st, mul((150, 150, 154), 0.8), faces=("north", "south", "east", "west")))
    b("form-a", (-2.5, 8, -2), (2.5, 15, 2), veined, rot=(0, 18, 0))
    b("form-b", (-1.5, 15, -3), (1.5, 20, 3), veined, rot=(0, -20, 0))
    b("form-c", (-3.5, 20, -1.5), (3.5, 23, 1.5), veined, rot=(0, 30, 0))
    b("orb", (-2, 23.5, -2), (2, 27.5, 2), veined, rot=(0, 45, 0))
    b("spur", (-0.6, 27.5, -0.6), (0.6, 30, 0.6), veined)
    return m


def sundial():
    m, b, pr = hg("sundial", 769)
    st = hg_stone((172, 168, 158), 769)

    def dial(p):
        if p.face != "up":
            return hg_metal(HG_BRASS)(p)
        dx, dy = p.i - (p.w - 1) / 2.0, p.j - (p.h - 1) / 2.0
        d = math.hypot(dx, dy)
        if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
            return mul(HG_BRASS, 0.7)
        if d > 2.4 and (abs(dx) < 0.6 or abs(dy) < 0.6 or (abs(abs(dx) - abs(dy)) < 0.1 and d > 4)):
            return (84, 64, 40)     # the hour lines
        return HG_BRASS if d > 1.6 else mul(HG_BRASS, 1.2)

    b("foot", (-4, 0, -4), (4, 2, 4), st)
    b("step", (-3, 2, -3), (3, 3, 3), st)
    b("shaft", (-2, 3, -2), (2, 10, 2), hg_edge(st, mul((172, 168, 158), 0.84), faces=("north", "south", "east", "west")))
    b("capital", (-3.5, 10, -3.5), (3.5, 11, 3.5), st)
    b("dial", (-5, 11, -5), (5, 12, 5), dial)
    gn = hg_metal(mul(HG_BRASS, 0.86))
    b("gnomon", (-0.5, 12, -3), (0.5, 13.2, 3.5), gn)
    b("gnomon-mid", (-0.5, 13.2, -0.5), (0.5, 14.4, 3.5), gn)
    b("gnomon-tip", (-0.5, 14.4, 2), (0.5, 15.6, 3.5), gn)
    return m


def lamp_post():
    m, b, pr = hg("lamp_post", 771)
    ir = hg_metal(HG_IRON)
    b("foot", (-3, 0, -3), (3, 2, 3), ir)
    b("socle", (-2, 2, -2), (2, 5, 2), ir)
    b("pole", (-1, 5, -1), (1, 34, 1), lambda p: mul(HG_IRON, 1.25) if p.i == 0 else HG_IRON)
    b("band-a", (-1.5, 12, -1.5), (1.5, 13.5, 1.5), ir)
    b("band-b", (-1.5, 30, -1.5), (1.5, 31.5, 1.5), ir)
    b("rung", (-4, 28, -0.5), (4, 29, 0.5), ir)
    b("tray", (-3, 34, -3), (3, 35, 3), ir)
    b("pane", (-2.5, 35, -2.5), (2.5, 41, 2.5), hg_glass(HG_IRON), noise=0)
    b("hood", (-3.5, 41, -3.5), (3.5, 42, 3.5), ir)
    b("hood-top", (-2, 42, -2), (2, 43.5, 2), ir)
    b("finial", (-0.5, 43.5, -0.5), (0.5, 45.5, 0.5), ir)
    return m


def garden_lantern():
    m, b, pr = hg("garden_lantern", 773)
    mossy = hg_stone((150, 152, 150), 773, moss=(98, 140, 90))
    st = hg_stone((150, 152, 150), 773)

    def lamp(p):
        if p.side() and 1 <= p.i < p.w - 1 and 1 <= p.j < p.h - 1 and p.i != p.w // 2:
            return mix((255, 240, 180), (250, 190, 90), abs(p.j - 1.5) / 2.0)
        return hg_stone((150, 152, 150), 774)(p)

    b("foot", (-5, 0, -5), (5, 2, 5), mossy)
    b("stem", (-2.5, 2, -2.5), (2.5, 5, 2.5), st)
    b("shelf", (-4.5, 5, -4.5), (4.5, 6.5, 4.5), st)
    b("firebox", (-3, 6.5, -3), (3, 10.5, 3), lamp, noise=3)
    b("roof", (-6, 10.5, -6), (6, 12, 6), mossy)
    b("roof-top", (-4, 12, -4), (4, 13.5, 4), st)
    b("knob", (-1, 13.5, -1), (1, 15.5, 1), st)
    return m


def mailbox():
    m, b, pr = hg("mailbox", 775)
    TL = (60, 130, 140)

    def tin(p):
        if p.face == "north":
            if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
                return mul(TL, 0.7)
            return HG_BRASS if (p.j == 1 and p.i == p.w // 2) else mul(TL, 1.12)
        return mul(TL, 0.86) if (p.face in ("east", "west", "up") and p.i % 4 == 0) else TL

    b("mound", (-2.5, 0, -2.5), (2.5, 1, 2.5), hg_leaves((84, 136, 68), 775))
    b("post", (-1, 1, -1), (1, 13, 1), hg_wood(HG_OAK, 775, "v", 2))
    b("strut", (-0.6, 8, -4.5), (0.6, 9.2, 0), HG_OAK, rot=(40, 0, 0), origin=(0, 8.6, 0))
    b("shelf", (-2.5, 12.4, -5), (2.5, 13, 5), HG_DARK)
    b("box", (-3, 13, -6), (3, 17, 6), tin)
    b("arch", (-2, 17, -6), (2, 18.5, 6), tin)
    b("flag-staff", (3, 14, -3), (3.5, 20, -2.2), HG_RED)
    b("flag", (3, 18, -2.2), (3.5, 20, 0.5), HG_RED)
    b("letter", (-1.5, 15, -6.8), (1.5, 15.6, -6), HG_CREAM)
    return m


def wheelbarrow():
    m, b, pr = hg("wheelbarrow", 777)
    GN = (74, 120, 84)
    tin = hg_metal(GN)
    wd = hg_wood(HG_OAK, 777, "h", 2)
    wheel(m, "wheel", "Body", -1, 1, 3, -6.5, 3, hub=(170, 60, 50), cut=1)
    pr("fork", (-1.8, 2.5, -7), (-1, 3.5, -3), HG_IRON)
    b("floor", (-4.5, 5, -4.5), (4.5, 6, 5), mul(GN, 0.8))
    b("wall-n", (-4, 5.5, -6.5), (4, 9, -4.5), tin, rot=(-20, 0, 0), origin=(0, 5.5, -4.5))
    b("wall-s", (-5, 6, 4.5), (5, 11, 5.5), tin)
    pr("wall", (-5.5, 6, -4.5), (-4.5, 11, 5.5), tin)
    b("load", (-4.5, 6, -4), (4.5, 9.5, 4.5), hg_top(HG_SOIL, lambda p: (120, 150, 80) if hsh(777, p.i, p.j) > 0.84 else HG_SOIL))
    pr("shaft", (-5, 4, -4), (-4, 5, 10.5), wd)
    pr("stand", (-5, 0, 3), (-4, 4, 4), HG_IRON)
    pr("grip", (-5.2, 3.8, 9), (-3.8, 5.2, 11), (60, 50, 46))
    return m


def picnic_table():
    m, b, pr = hg("picnic_table", 779)
    wd = hg_wood((186, 138, 84), 779, "h", 3)
    dk = hg_wood((150, 106, 62), 780, "v", 2)
    b("top", (-8, 11, -4.5), (8, 12.5, 4.5), wd)
    b("bench-n", (-8, 6, -9.5), (8, 7.2, -6.5), wd)
    b("bench-s", (-8, 6, 6.5), (8, 7.2, 9.5), wd)
    for k, x in enumerate((-6.5, 5.5)):
        b("sill-%d" % k, (x, 4.8, -9), (x + 1, 6, 9), dk)
        b("cleat-%d" % k, (x, 9.8, -4), (x + 1, 11, 4), dk)
        b("strut-n-%d" % k, (x + 0.1, 0, -5.5), (x + 0.9, 11, -4.3), dk, rot=(12, 0, 0), origin=(x + 0.5, 5.5, -4.9))
        b("strut-s-%d" % k, (x + 0.1, 0, 4.3), (x + 0.9, 11, 5.5), dk, rot=(-12, 0, 0), origin=(x + 0.5, 5.5, 4.9))
    b("brace", (-5.5, 7.5, -0.5), (5.5, 8.5, 0.5), dk)
    return m


def garden_bench():
    m, b, pr = hg("garden_bench", 781)
    wd = hg_wood((170, 118, 70), 781, "h", 2)
    ir = hg_metal(HG_IRON)
    for k, z in enumerate((-4, -1.5, 1)):
        b("slat-%d" % k, (-7.5, 6, z), (7.5, 7, z + 2), wd)
    for k, y in enumerate((9, 12, 15)):
        b("backslat-%d" % k, (-7.5, y, 3.2), (7.5, y + 2, 4), wd)
    pr("foot-n", (-8, 0, -4), (-7, 6, -3), ir)
    pr("upright", (-8, 0, 3), (-7, 17.5, 4.2), ir)
    pr("rest", (-8.2, 10, -4.5), (-6.8, 11, 3), ir)
    pr("rest-prop", (-8, 6, -4), (-7, 10, -3), ir)
    pr("rail", (-8, 4.5, -3), (-7, 5.5, 3), ir)
    b("stretcher", (-7, 2.5, -0.5), (7, 3.5, 0.5), ir)
    return m


def swing():
    m, b, pr = hg("swing", 783)
    wd = hg_wood((178, 130, 78), 783, "v", 2)
    RP = (212, 192, 138)
    pr("post", (-8, 0, -1), (-6.5, 30, 1), wd)
    pr("sole", (-8.2, 0, -7), (-6.3, 1.5, 7), hg_wood((150, 106, 62), 784, "h", 2))
    for sx in (-1, 1):
        x = sx * 7.25
        b("prop-n-%d" % sx, (x - 0.5, 1, -1), (x + 0.5, 12, 0), wd, rot=(-28, 0, 0), origin=(x, 12, -0.5))
        b("prop-s-%d" % sx, (x - 0.5, 1, 0), (x + 0.5, 12, 1), wd, rot=(28, 0, 0), origin=(x, 12, 0.5))
    b("beam", (-8, 30, -1), (8, 32, 1), hg_wood((178, 130, 78), 785, "h", 2))
    pr("rope", (-3.6, 10, -0.3), (-3, 30, 0.3), lambda p: mul(RP, 0.78) if p.j % 2 else RP)
    pr("eye", (-3.9, 29.4, -0.6), (-2.7, 30, 0.6), HG_IRON)
    b("seat", (-4.6, 9, -2), (4.6, 10, 2), hg_wood((206, 150, 84), 786, "v", 3))
    return m


def trellis():
    m, b, pr = hg("trellis", 787, noise=5)
    PN = (220, 208, 180)

    def lattice(p):
        if (p.i + p.j) % 4 == 0 or (p.i - p.j) % 4 == 0:
            return mul(PN, 0.86) if (p.i - p.j) % 4 == 0 else PN
        return CLEAR

    def vine(p):
        n = vnoise(787, p.x / 2.6, p.y / 2.6, 0)
        if n < 0.5 - 0.22 * (1 - p.ly) or hsh(788, p.i, p.j) > 0.8:
            return CLEAR
        h = hsh(789, p.i, p.j)
        if h > 0.9:
            return (236, 120, 170)
        return mix((48, 110, 54), (96, 160, 74), h)

    flat = ("up", "down", "east", "west")
    b("trough", (-8, 0, -3), (8, 4, 3), hg_edge(hg_wood(HG_OAK, 787, "v"), HG_DARK, faces=("up",)))
    b("soil", (-7, 4, -2), (7, 4.5, 2), HG_SOIL, skip=("down",))
    pr("stile", (-7, 4, -0.5), (-6, 28, 0.5), PN)
    b("lintel", (-7.5, 28, -0.7), (7.5, 29.2, 0.7), PN)
    b("lattice", (-6, 4.5, -0.2), (6, 28, 0.2), lattice, skip=flat)
    b("vine-n", (-6.5, 4.5, -0.8), (6.5, 25.5, -0.4), vine, skip=flat)
    b("vine-s", (-6.5, 4.5, 0.4), (6.5, 22.5, 0.8), vine, skip=flat)
    lf = hg_leaves((62, 130, 60), 790, dots=((236, 120, 170),), thr=0.85)
    b("spray-a", (-5, 4.5, -2), (-1, 8, 1.5), lf)
    b("spray-b", (2, 4.5, -1.5), (6, 7, 2), lf)
    b("spray-c", (-7.8, 17, -1.2), (-5.5, 20, 1.2), lf)
    b("spray-d", (4, 24, -1.2), (7, 27, 1.2), lf)
    return m


def scarecrow_post():
    m, b, pr = hg("scarecrow_post", 791)
    SK, SH = (206, 180, 128), (86, 112, 150)
    straw = hg_thatch(HG_STRAW, 791, "v")
    shirt = hg_cloth(SH, (190, 96, 70), "check", 2)

    def face(c):
        if c.face == "north":
            for x in (1, c.w - 2):
                c.px(x, 1, (50, 36, 30))
                c.px(x, 2, (50, 36, 30))
            c.rect(1, c.h - 2, c.w - 2, 1, (120, 60, 50))
            c.px(2, c.h - 2, SK)
            c.px(c.w // 2, 3, (220, 120, 50))

    b("mound", (-2, 0, -2), (2, 1, 2), HG_SOIL)
    b("pole", (-0.8, 1, -0.8), (0.8, 18, 0.8), hg_wood(HG_DARK, 791, "v", 2))
    b("crossbar", (-7, 16, -0.6), (7, 17.2, 0.6), HG_DARK)
    b("smock", (-3, 10, -2), (3, 18, 2), shirt)
    b("skirt", (-3.3, 7.5, -2.3), (3.3, 10, 2.3), straw)
    pr("sleeve", (-6.5, 15.3, -1.1), (-3, 17.9, 1.1), shirt)
    pr("cuff", (-8, 15.6, -0.9), (-6.5, 17.6, 0.9), straw)
    b("ruff", (-2, 18, -1.6), (2, 18.8, 1.6), straw)
    b("sack", (-2.5, 18.8, -2.5), (2.5, 23.6, 2.5), hg_cloth(SK, mul(SK, 0.9), "weave"), face)
    b("brim", (-4, 23.6, -4), (4, 24.4, 4), straw)
    b("crown", (-2.5, 24.4, -2.5), (2.5, 27, 2.5), hg_top(lambda p: HG_RED if p.j == p.h - 1 else straw(p), straw))
    return m


def campfire_ring():
    m, b, pr = hg("campfire_ring", 795)
    tones = [(128, 128, 134), (150, 146, 140), (112, 114, 122)]
    for k in range(8):
        a = math.radians(k * 45)
        x, z = round(math.sin(a) * 6 * 2) / 2.0, round(math.cos(a) * 6 * 2) / 2.0
        r, h = (1.5, 2)[k % 2], (2.5, 3, 2, 3.5)[k % 4]
        b("stone-%d" % k, (x - r, 0, z - r), (x + r, h, z + r), hg_stone(tones[k % 3], 795 + k), rot=(0, k * 45, 0))
    b("ash", (-4, 0, -4), (4, 0.6, 4), lambda p: (60, 56, 56) if hsh(795, p.i, p.j) > 0.4 else (232, 110, 40))
    lg = hg_log((70, 52, 40), (190, 150, 100), 796, "z")
    b("log-a", (-1, 0.6, -4.5), (1, 2.6, 4.5), lg, rot=(0, 25, 0))
    b("log-b", (-1, 0.6, -4.5), (1, 2.6, 4.5), lg, rot=(0, -40, 0))
    b("log-c", (-0.9, 2.2, -4), (0.9, 4, 4), lg, rot=(14, 80, 0))
    b("flame-a", (-1.5, 2.6, -1.5), (1.5, 6.5, 1.5), hg_fire, noise=0)
    b("flame-b", (-0.8, 6.5, -0.8), (0.8, 9, 0.8), hg_fire, noise=0)
    b("flame-c", (1, 2.6, 0.5), (2.2, 5, 1.7), hg_fire, noise=0)
    b("flame-d", (-2.4, 2.6, -1.6), (-1.2, 4.4, -0.4), hg_fire, noise=0)
    return m


GARDEN = [boulder, rock_cairn, bush, berry_bush, hedge, topiary, fern, cattail, sunflower, flower_bed, flowerpot, potted_palm,
          ornamental_tree, stump, log_pile, mushroom_ring, pumpkin, hay_bale, lily_pad, bird_bath, fountain, well, beehive, birdhouse,
          garden_statue, sundial, lamp_post, garden_lantern, mailbox, wheelbarrow, picnic_table, garden_bench, swing, trellis,
          scarecrow_post, campfire_ring]


# ---- house

def grandfather_clock():
    m, b, pr = hg("grandfather_clock", 801)
    wd = hg_wood(HG_WALNUT, 801, "v", 4)
    DK = mul(HG_WALNUT, 0.7)

    def dial(c):
        if c.face == "north":
            c.rect(1, 1, c.w - 2, c.w - 2, HG_BRASS)
            c.rect(2, 2, c.w - 4, c.w - 4, HG_CREAM)
            cx, cy = c.w // 2, 1 + (c.w - 2) // 2
            for dx, dy in ((0, -2), (2, 0), (0, 2), (-3, 0)):
                c.px(cx + dx - (1 if dx > 0 else 0), cy + dy - (1 if dy > 0 else 0), (60, 50, 44))
            c.px(cx, cy, BLACK), c.px(cx, cy - 1, BLACK), c.px(cx - 1, cy, BLACK), c.px(cx - 2, cy, BLACK)

    def door(c):
        if c.face == "north":
            c.rect(1, 1, c.w - 2, c.h - 2, DK)
            c.rect(2, 2, c.w - 4, c.h - 4, (58, 44, 40))
            c.rect(c.w // 2, 2, 1, c.h - 8, HG_BRASS)
            c.rect(c.w // 2 - 1, c.h - 7, 3, 3, HG_GOLD)
            c.px(c.w // 2 - 1, c.h - 7, (255, 240, 170))
            c.rect(2, 3, 1, 5, HG_STEEL)

    b("plinth", (-4, 0, -3), (4, 5, 3), hg_edge(wd, DK, faces=("north",)))
    b("trunk", (-3, 5, -2.5), (3, 22, 2.5), wd, door)
    b("hood", (-4, 22, -3), (4, 31, 3), wd, dial)
    b("cornice", (-4.5, 31, -3.5), (4.5, 32.5, 3.5), DK)
    b("pediment", (-2.5, 32.5, -3), (2.5, 34.5, 3), wd)
    pr("finial", (-4, 32.5, -0.5), (-3, 34, 0.5), HG_BRASS)
    b("waist", (-3.5, 21, -3), (3.5, 22, 3), DK)
    return m


def piano():
    m, b, pr = hg("piano", 803)
    BK = (44, 38, 42)
    lac = lambda p: mul(BK, 1.5) if (p.side() and p.i == p.w // 4) or p.face == "up" else BK

    def keys(p):
        if p.face == "up":
            if p.j >= 2:
                return BK if (p.i % 2 == 1 and (p.i // 2) % 7 not in (2, 6)) else HG_CREAM
            return mul(HG_CREAM, 0.8) if p.i % 2 == 1 and p.j == 0 else HG_CREAM
        if p.face == "north":
            return HG_CREAM if p.j == 0 else BK
        return BK

    def panel(c):
        if c.face == "north":
            c.rect(1, 1, c.w - 2, 6, (60, 50, 54))
            c.rect(2, 2, c.w - 4, 4, (52, 44, 48))
            c.rect(1, c.h - 6, c.w - 2, 5, (60, 50, 54))
            c.px(c.w // 2 - 1, c.h - 1, HG_BRASS), c.px(c.w // 2 + 1, c.h - 1, HG_BRASS)

    b("case", (-8, 0, 0), (8, 19, 5), lac, panel)
    b("lid", (-8, 19, -0.5), (8, 20, 5.5), lac)
    b("keybed", (-7, 9, -4), (7, 10.5, 0), keys, noise=3)
    pr("cheek", (-8, 8, -4.2), (-7, 12, 0), lac)
    pr("post", (-8, 0, -4), (-6.8, 8, -2.8), lac)
    b("desk", (-4, 12.5, -0.8), (4, 16.5, 0), lambda p: HG_CREAM if (p.face == "north" and 1 <= p.i < p.w - 1 and p.j < p.h - 1) else BK,
      lambda c: [c.rect(2, y, c.w - 4, 1, (120, 110, 104)) for y in (1, 2)] if c.face == "north" else None, rot=(8, 0, 0), origin=(0, 12.5, 0))
    b("pedal-a", (-1.5, 0, -1.2), (-0.7, 0.6, 0), HG_BRASS)
    b("pedal-b", (0.7, 0, -1.2), (1.5, 0.6, 0), HG_BRASS)
    return m


def harp():
    m, b, pr = hg("harp", 805)
    wd = hg_wood((190, 140, 70), 805, "v", 3)
    gilt = hg_metal(HG_GOLD)

    def strings(p):
        if p.i % 2 == 1:
            return CLEAR
        return (236, 226, 190) if p.i % 6 else (210, 70, 60)

    b("foot", (-2.5, 0, -6.5), (2.5, 2, 7), wd)
    b("pillar", (-1, 2, -6), (1, 28, -4), lambda p: mul((190, 140, 70), 0.8) if p.j % 6 == 0 else (190, 140, 70))
    b("crown", (-1.5, 28, -6.5), (1.5, 31, -3.5), gilt)
    b("arch", (-1, 27, -3.5), (1, 29, 4), wd)
    b("shoulder", (-1, 26, 3), (1, 28.5, 7), wd)
    b("sound-a", (-1.5, 2, 1.5), (1.5, 10, 4.5), wd)
    b("sound-b", (-1.5, 10, 3), (1.5, 19, 5.8), wd)
    b("sound-c", (-1.5, 19, 4.5), (1.5, 26, 7), wd)
    b("strings", (-0.2, 2, -4), (0.2, 27, 5), strings, skip=("up", "down", "north", "south"), noise=0)
    b("scroll", (-1.3, 3, -6.6), (1.3, 5, -3.4), gilt)
    return m


def globe():
    m, b, pr = hg("globe", 807)
    br = hg_metal(HG_BRASS)

    def earth(p):
        if p.y > 17.2 or p.y < 6.8:
            return (236, 240, 244)
        n = vnoise(807, p.x / 3.2, p.y / 3.2, p.z / 3.2)
        if n > 0.56:
            return mix((96, 150, 80), (196, 170, 110), (n - 0.56) * 6)
        return mix((52, 104, 176), (80, 140, 206), n * 1.6)

    b("foot", (-4, 0, -4), (4, 1.5, 4), hg_wood(HG_WALNUT, 807, "v"))
    b("stem", (-1, 1.5, -1), (1, 5, 1), br)
    b("sphere", (-5, 7, -5), (5, 17, 5), earth)
    b("sphere-y", (-3.5, 6, -3.5), (3.5, 18, 3.5), earth)
    b("sphere-x", (-6, 9, -3.5), (6, 15, 3.5), earth)
    b("sphere-z", (-3.5, 9, -6), (3.5, 15, 6), earth)
    b("arc-low", (-6.5, 4.5, -0.6), (1, 5.5, 0.6), br)
    b("arc-side", (-7.5, 4.5, -0.6), (-6.5, 19.5, 0.6), br)
    b("arc-top", (-6.5, 18.5, -0.6), (1, 19.5, 0.6), br)
    b("pin", (-0.5, 5.5, -0.5), (0.5, 6, 0.5), br)
    b("pin-top", (-0.5, 18, -0.5), (0.5, 18.5, 0.5), br)
    return m


def telescope():
    m, b, pr = hg("telescope", 809)
    wd = hg_wood(HG_WALNUT, 809, "v", 2)
    R, O = (26, 0, 0), (0, 20.5, 0)

    def tube(p):
        if p.face in ("north", "south"):
            return (150, 200, 230) if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) > 0 else HG_BRASS
        k = int(p.lz * 15)
        return mul(HG_BRASS, 0.74) if k in (4, 10) else (HG_BRASS if p.face != "up" else mul(HG_BRASS, 1.2))

    for k in range(3):
        a = math.radians(k * 120)
        b("strut-%d" % k, (-0.6, 0, -0.6), (0.6, 17, 0.6), wd, rot=(18, k * 120, 0), origin=(0, 17, 0))
        x, z = round(-math.sin(a) * 5.3, 1), round(-math.cos(a) * 5.3, 1)
        b("shoe-%d" % k, (x - 0.9, 0, z - 0.9), (x + 0.9, 1, z + 0.9), HG_BRASS)
    b("hub", (-1.5, 16, -1.5), (1.5, 18, 1.5), hg_metal(HG_BRASS))
    b("yoke", (-0.7, 18, -0.7), (0.7, 19.5, 0.7), hg_metal(HG_IRON))
    b("tube", (-1.5, 19, -8), (1.5, 22, 5), tube, rot=R, origin=O)
    b("hood", (-2, 18.5, -10), (2, 22.5, -8), tube, rot=R, origin=O)
    b("draw", (-1, 19.5, 5), (1, 21.5, 8), hg_metal(mul(HG_BRASS, 0.86)), rot=R, origin=O)
    b("eyepiece", (-0.7, 19.8, 8), (0.7, 21.2, 9.5), (40, 38, 42), rot=R, origin=O)
    return m


def easel():
    m, b, pr = hg("easel", 811)
    wd = hg_wood(HG_PINE, 811, "v", 2)
    R, O = (9, 0, 0), (0, 0, -1)

    def picture(c):
        if c.face == "north":
            c.rect(0, 0, c.w, c.h, (236, 230, 214))
            c.rect(1, 1, c.w - 2, 6, (150, 200, 236))
            c.rect(c.w - 4, 2, 2, 2, (250, 214, 90))
            c.rect(1, 7, c.w - 2, c.h - 8, (104, 160, 86))
            for x in range(1, c.w - 1):
                hgt = int(2 + 2 * math.sin(x * 0.9))
                c.rect(x, 7 - hgt, 1, hgt, (86, 116, 150) if x < c.w // 2 else (72, 132, 78))
            c.rect(2, c.h - 4, 4, 1, (70, 120, 190))
            c.px(c.w - 3, c.h - 3, (226, 70, 80)), c.px(c.w - 5, c.h - 4, (250, 230, 110))

    pr("stile", (-5, 0, -1.5), (-4, 28, -0.5), wd, rot=R, origin=O)
    b("prop", (-0.5, 0, 3), (0.5, 26, 4), wd, rot=(-13, 0, 0), origin=(0, 26, 3.5))
    b("ledge", (-6, 9, -3), (6, 10, -0.5), wd, rot=R, origin=O)
    b("yoke", (-5, 24.5, -1.7), (5, 25.5, -0.3), wd, rot=R, origin=O)
    b("canvas", (-5.5, 10, -2.3), (5.5, 24, -1.5), (236, 230, 214), picture, rot=R, origin=O)
    b("clamp", (-1, 24, -2.6), (1, 25, -1.4), HG_DARK, rot=R, origin=O)
    b("jar", (3, 10, -2.9), (4.4, 11.8, -1.5), (120, 160, 190), rot=R, origin=O)
    return m


def vase():
    m, b, pr = hg("vase", 813)
    BL, WH = (48, 84, 150), (236, 238, 240)

    def glaze(p):
        if p.face in ("up", "down"):
            return WH
        if p.j in (0, p.h - 1):
            return BL
        if p.h > 5 and (abs(p.i - p.w // 2) + abs(p.j - p.h // 2)) % 4 == 3:
            return BL
        return WH

    b("foot", (-2.5, 0, -2.5), (2.5, 1, 2.5), BL)
    b("belly", (-4, 1, -4), (4, 8, 4), glaze)
    b("shoulder", (-3, 8, -3), (3, 10, 3), glaze)
    b("throat", (-1.5, 10, -1.5), (1.5, 13, 1.5), WH)
    b("lip", (-2.5, 13, -2.5), (2.5, 14, 2.5), hg_top(BL, (40, 40, 60)))
    pr("handle", (-3.5, 9.5, -0.5), (-1.5, 10.5, 0.5), BL)
    hg_bloom(b, "bloom-a", -0.8, 0, 14, 19, (236, 110, 150))
    hg_bloom(b, "bloom-b", 1, 0.8, 14, 17, (250, 220, 110), eye=(190, 110, 40))
    hg_bloom(b, "bloom-c", 0.6, -1, 14, 21.5, (190, 150, 236))
    return m


def candelabra():
    m, b, pr = hg("candelabra", 815)
    sv = hg_metal((186, 190, 204))
    wax = lambda p: (255, 250, 232) if p.i == 0 else (236, 226, 200)
    b("foot", (-3, 0, -3), (3, 1, 3), sv)
    b("step", (-2, 1, -2), (2, 2, 2), sv)
    b("stem", (-0.7, 2, -0.7), (0.7, 12, 0.7), sv)
    b("knop", (-1.3, 5.5, -1.3), (1.3, 7.5, 1.3), sv)
    b("branch", (-6, 9, -0.6), (6, 10, 0.6), sv)
    pr("riser", (-6, 10, -0.6), (-5, 11.5, 0.6), sv)
    for k, (x, y) in enumerate(((-5.5, 11.5), (0, 12), (5.5, 11.5))):
        b("cup-%d" % k, (x - 1.2, y, -1.2), (x + 1.2, y + 0.8, 1.2), sv)
        b("candle-%d" % k, (x - 0.7, y + 0.8, -0.7), (x + 0.7, y + 5 + (k == 1), 0.7), wax, noise=3)
        b("flame-%d" % k, (x - 0.4, y + 5.2 + (k == 1), -0.4), (x + 0.4, y + 6.8 + (k == 1), 0.4), hg_fire, noise=0)
    return m


def chandelier():
    m, b, pr = hg("chandelier", 817)
    br = hg_metal(HG_BRASS)
    wax = lambda p: (255, 250, 232) if p.i == 0 else (236, 226, 200)
    b("rose", (-2, 15.4, -2), (2, 16, 2), br)
    b("chain", (-0.4, 13, -0.4), (0.4, 15.4, 0.4), lambda p: HG_IRON if p.j % 2 else HG_STEEL)
    b("hub", (-1.5, 10.5, -1.5), (1.5, 13, 1.5), br)
    b("spoke-x", (-7, 9.6, -0.5), (7, 10.5, 0.5), br)
    b("spoke-z", (-0.5, 9.6, -7), (0.5, 10.5, 7), br)
    b("hoop-n", (-4.5, 9.8, -4.5), (4.5, 10.4, -3.9), br)
    b("hoop-s", (-4.5, 9.8, 3.9), (4.5, 10.4, 4.5), br)
    b("hoop-w", (-4.5, 9.8, -3.9), (-3.9, 10.4, 3.9), br)
    b("hoop-e", (3.9, 9.8, -3.9), (4.5, 10.4, 3.9), br)
    for k, (x, z) in enumerate(((-6.3, 0), (6.3, 0), (0, -6.3), (0, 6.3))):
        b("cup-%d" % k, (x - 1.1, 10.5, z - 1.1), (x + 1.1, 11.1, z + 1.1), br)
        b("candle-%d" % k, (x - 0.6, 11.1, z - 0.6), (x + 0.6, 14, z + 0.6), wax, noise=3)
        b("flame-%d" % k, (x - 0.4, 14.2, z - 0.4), (x + 0.4, 15.4, z + 0.4), hg_fire, noise=0)
    b("drop", (-0.8, 8, -0.8), (0.8, 10.5, 0.8), br)
    b("crystal", (-0.5, 6, -0.5), (0.5, 8, 0.5), (190, 230, 246), noise=3)
    return m


def hanging_lantern():
    m, b, pr = hg("hanging_lantern", 819)
    CU = (150, 96, 66)
    cu = hg_metal(CU)
    b("hook", (-1, 15.5, -1), (1, 16, 1), cu)
    b("chain", (-0.4, 13.5, -0.4), (0.4, 15.5, 0.4), lambda p: HG_IRON if p.j % 2 else HG_STEEL)
    b("knob", (-1.5, 12.6, -1.5), (1.5, 13.5, 1.5), cu)
    b("hood", (-3.5, 11.6, -3.5), (3.5, 12.6, 3.5), cu)
    b("pane", (-3, 7, -3), (3, 11.6, 3), hg_glass(mul(CU, 0.8), (255, 190, 90)), noise=0)
    b("tray", (-3.5, 6, -3.5), (3.5, 7, 3.5), cu)
    return m


def fireplace():
    m, b, pr = hg("fireplace", 821)
    bk = hg_bricks((166, 84, 66), (196, 186, 170), 821, 4, 3)
    soot = lambda p: mul(bk(p), 0.34)
    b("hearth", (-8, 0, -6), (8, 2, 5), hg_stone(HG_SLATE, 821))
    pr("jamb", (-8, 2, -3), (-4, 13, 5), bk)
    b("fireback", (-4, 2, 3), (4, 13, 5), soot)
    b("lintel", (-8, 13, -3), (8, 16, 5), bk)
    b("mantel", (-8, 16, -5), (8, 17.5, 5), hg_wood(HG_WALNUT, 822, "h", 3))
    b("breast", (-6, 17.5, -1), (6, 28, 5), bk)
    lg = hg_log((74, 54, 42), (186, 146, 98), 823, "x")
    b("log-a", (-3.5, 2, -1.5), (3.5, 4, 0.5), lg)
    b("log-b", (-3, 2, 0.8), (3, 3.8, 2.6), lg)
    b("log-c", (-2.5, 3.8, -0.5), (2.5, 5.4, 1.2), lg)
    b("flame-a", (-1.5, 5.4, -0.4), (1.5, 9.5, 1), hg_fire, noise=0)
    b("flame-b", (-3, 4, -0.8), (-1.8, 7, 0.4), hg_fire, noise=0)
    b("flame-c", (1.6, 4, 0.2), (2.8, 7.8, 1.4), hg_fire, noise=0)
    b("keepsake", (3.5, 17.5, -3.5), (5.5, 20.5, -1.5), hg_metal(HG_BRASS))
    return m


def bust():
    m, b, pr = hg("bust", 825)
    mb = hg_stone(HG_MARBLE, 825)
    gr = hg_stone((120, 124, 132), 826)

    def visage(c):
        if c.face == "north":
            c.rect(1, 2, c.w - 2, 1, mul(HG_MARBLE, 0.76))
            c.rect(1, c.h - 2, c.w - 2, 1, mul(HG_MARBLE, 0.86))

    b("foot", (-4, 0, -4), (4, 2, 4), gr)
    b("shaft", (-3, 2, -3), (3, 12, 3), hg_edge(gr, mul((120, 124, 132), 0.8), faces=("north", "south", "east", "west")))
    b("cap", (-4, 12, -4), (4, 13.5, 4), gr)
    b("chest", (-4, 13.5, -2), (4, 17, 2), mb)
    b("drape", (-4.3, 14.5, -2.3), (0, 17.3, 2.3), lambda p: mul(mb(p), 0.9 if (p.i + p.j) % 3 else 0.78))
    b("column", (-1.2, 17, -1.2), (1.2, 18.5, 1.2), mb)
    b("dome", (-2.5, 18.5, -2.5), (2.5, 23.5, 2.5), mb, visage)
    b("ridge", (-0.5, 20, -3.2), (0.5, 21.8, -2.5), mb)
    b("wreath", (-2.8, 22, -2.8), (2.8, 23, 2.8), lambda p: (120, 150, 96) if (p.i + p.j) % 2 else (150, 176, 120))
    return m


def knight_statue():
    m, b, pr = hg("knight_statue", 827)
    pl = hg_metal(HG_STEEL)
    DKS = mul(HG_STEEL, 0.62)

    def plate(p):
        c = pl(p)
        if p.side() and p.j % 4 == 3:
            return mul(HG_STEEL, 0.74)
        return c

    def helm(c):
        if c.face == "north":
            c.rect(1, 2, c.w - 2, 1, BLACK)
            c.rect(c.w // 2, 1, 1, c.h - 1, DKS)
            c.sym(1, 4, DKS)

    def breast(c):
        if c.face == "north":
            c.rect(c.w // 2, 1, 1, c.h - 2, mul(HG_STEEL, 1.2))
            c.sym(1, 1, HG_BRASS)

    b("base", (-5, 0, -5), (5, 2, 5), hg_stone(HG_SLATE, 827))
    pr("greave", (-3.5, 2, -1.5), (-0.8, 11, 1.5), plate)
    pr("sabaton", (-3.5, 2, -3), (-0.8, 3.5, -1.5), pl)
    b("tasset", (-4.3, 10.5, -2.4), (4.3, 13, 2.4), lambda p: HG_BRASS if p.j == 0 and p.side() else DKS)
    b("cuirass", (-4, 13, -2.2), (4, 22, 2.2), plate, breast)
    pr("pauldron", (-6.2, 19, -2.6), (-4, 22.5, 2.6), pl)
    pr("vambrace", (-5.8, 11.5, -1.4), (-4, 19, 1.4), plate)
    b("gorget", (-1.5, 22, -1.5), (1.5, 23, 1.5), DKS)
    b("helm", (-2.5, 23, -2.5), (2.5, 28.5, 2.5), pl, helm)
    b("plume", (-0.6, 28.5, -1), (0.6, 31, 3), lambda p: mul(HG_RED, 0.8) if p.i % 2 else HG_RED)
    b("blade", (-0.6, 2, -4.2), (0.6, 14, -3.6), lambda p: WHITE if p.i == 0 else HG_STEEL)
    b("guard", (-2.5, 14, -4.5), (2.5, 15, -3.3), HG_BRASS)
    b("grip", (-0.5, 15, -4.3), (0.5, 18, -3.5), (70, 48, 36))
    b("pommel", (-0.8, 18, -4.6), (0.8, 19.2, -3.2), HG_BRASS)
    return m


def hg_books(seed):
    pal = [(150, 50, 54), (48, 86, 140), (60, 116, 78), (190, 150, 70), (104, 66, 120), (176, 96, 52), (70, 70, 84), (206, 196, 170)]

    def sh(p):
        if p.face == "north":
            air = int(hsh(seed, p.i) * 2.3)
            if p.j < air or hsh(seed + 5, p.i) > 0.93:
                return (46, 32, 26)
            c = pal[int(hsh(seed + 1, p.i) * len(pal))]
            if p.j == air + 1 + int(hsh(seed + 2, p.i) * 2):
                return mix(c, HG_GOLD, 0.7)
            return c
        if p.face == "up":
            return (214, 204, 180)
        return (60, 44, 34)
    return sh


def tall_bookcase():
    m, b, pr = hg("tall_bookcase", 829)
    wd = hg_wood(HG_WALNUT, 829, "v", 4)
    sh = hg_wood(mul(HG_WALNUT, 1.12), 830, "h", 3)
    b("back", (-8, 0, 3), (8, 32, 4), lambda p: mul(wd(p), 0.6) if p.face == "north" else wd(p))
    pr("cheek", (-8, 0, -3), (-7, 32, 3), wd)
    b("kick", (-7, 0, -2.5), (7, 1.5, 3), mul(HG_WALNUT, 0.7))
    for k, y in enumerate((7.5, 13.5, 19.5, 25.5)):
        b("shelf-%d" % k, (-7, y, -3), (7, y + 1, 3), sh)
    b("cornice", (-8, 31, -3.5), (8, 32, -3), mul(HG_WALNUT, 0.76))
    b("roof", (-7, 31, -3), (7, 32, 3), sh)
    for k, y in enumerate((1.5, 8.5, 14.5, 20.5, 26.5)):
        b("books-%d" % k, (-7, y, -2.2), (7 - (3 if k in (1, 3) else 0), y + (4.5 if k else 5), 2.5), hg_books(829 + k), noise=4)
    b("urn", (4.6, 8.5, -1.5), (6.4, 11.5, 0.5), hg_metal(HG_BRASS))
    b("box", (4.4, 20.5, -1.5), (6.6, 22.5, 1), hg_wood(HG_PINE, 831, "h", 2))
    return m


def wing_chair():
    m, b, pr = hg("wing_chair", 833)
    RD = (140, 50, 62)
    vel = hg_cloth(RD, mul(RD, 0.88), "weave")

    def tufted(c):
        if c.face == "north":
            for y in range(2, c.h - 1, 4):
                for x in range(2 + (y // 4 % 2) * 2, c.w - 1, 4):
                    c.px(x, y, HG_GOLD)

    hg_quad(b, "foot", (-6, 0, -5), (-4.5, 2, -3.5), HG_DARK)
    b("seat", (-6, 2, -5.5), (6, 7, 5), vel)
    b("cushion", (-4.5, 7, -5.8), (4.5, 8.6, 3), hg_cloth(mul(RD, 1.14), RD, "weave"))
    b("back", (-6, 7, 3), (6, 23, 5.5), vel, tufted)
    pr("cheek", (-7, 13, -0.5), (-5.2, 22, 3.5), vel)
    pr("rest", (-7.5, 7, -5), (-4.8, 11.5, 3.5), vel)
    pr("scroll", (-7.8, 10.8, -5.6), (-4.6, 12.4, -3.6), hg_cloth(mul(RD, 1.14), RD, "weave"))
    b("crest", (-5, 23, 3.4), (5, 24, 5.2), mul(HG_WALNUT, 0.8))
    return m


def rocking_chair():
    m, b, pr = hg("rocking_chair", 835)
    wd = hg_wood((176, 124, 70), 835, "v", 2)
    DK = (136, 92, 52)
    pr("rocker", (-5.5, 0, -6), (-4.5, 1, 6), DK)
    pr("rocker-n", (-5.5, 1, -8.5), (-4.5, 2, -5.5), DK)
    pr("rocker-s", (-5.5, 1, 5.5), (-4.5, 2.5, 9), DK)
    pr("post-n", (-5.5, 1, -4), (-4.5, 10, -3), wd)
    pr("post-s", (-5.5, 1, 3), (-4.5, 21, 4), wd)
    b("seat", (-5.5, 6, -4.5), (5.5, 7, 4), hg_cloth((206, 176, 120), (176, 142, 90), "weave"))
    pr("rest", (-6, 10, -4.6), (-4.2, 10.9, 3), DK)
    for k, x in enumerate((-3, -0.5, 2)):
        b("spindle-%d" % k, (x, 7, 3.2), (x + 1, 19, 3.8), wd)
    b("rail", (-5.5, 19, 2.9), (5.5, 21.5, 4.1), DK)
    b("rung", (-4.5, 3, -3.8), (4.5, 3.8, -3.2), DK)
    b("pillow", (-3.5, 7, -1), (3.5, 8.4, 3), hg_cloth((110, 150, 170), (226, 226, 214), "check", 2))
    return m


def cradle():
    m, b, pr = hg("cradle", 837)
    wd = hg_wood((214, 176, 116), 837, "h", 2)
    DK = (160, 116, 68)

    def heart(c):
        if c.face in ("north", "south"):
            x, y = c.w // 2 - 1, 2
            c.rect(x - 1, y, 4, 1, DK), c.rect(x, y + 1, 2, 1, DK)

    for k, z in enumerate((-5.5, 4.5)):
        b("rocker-%d" % k, (-5, 0, z), (5, 1, z + 1), DK)
        b("rocker-w-%d" % k, (-7, 1, z), (-4.5, 2, z + 1), DK)
        b("rocker-e-%d" % k, (4.5, 1, z), (7, 2, z + 1), DK)
    b("floor", (-5, 2, -7), (5, 3, 7), wd)
    pr("side", (-5.5, 2, -7), (-4.5, 8, 7), hg_wood((214, 176, 116), 838, "v", 2))
    b("footboard", (-5.5, 2, -7.8), (5.5, 9, -7), wd, heart)
    b("headboard", (-5.5, 2, 7), (5.5, 12, 7.8), wd, heart)
    b("canopy", (-5.5, 11.2, 3.5), (5.5, 12, 7), wd)
    b("mattress", (-4.5, 3, -6.8), (4.5, 4.5, 6.8), (240, 238, 230))
    b("quilt", (-4.5, 4.5, -6.8), (4.5, 5.4, 2), hg_cloth((150, 190, 214), (244, 214, 150), "check", 2), skip=("down",))
    b("pillow", (-3, 4.5, 3), (3, 5.8, 6.2), WHITE)
    return m


def dining_table():
    m, b, pr = hg("dining_table", 839)
    wd = hg_wood((150, 98, 56), 839, "h", 3)
    DK = (112, 72, 42)
    b("top", (-8, 10.5, -6), (8, 12, 6), wd)
    b("apron", (-7, 9, -5), (7, 10.5, 5), DK)
    hg_quad(b, "post", (-7, 1, -5), (-5.5, 9, -3.5), lambda p: mul(DK, 1.2) if p.j % 4 == 1 else DK)
    hg_quad(b, "shoe", (-7.3, 0, -5.3), (-5.2, 1, -3.2), DK)
    b("runner", (-2, 12, -6.2), (2, 12.3, 6.2), hg_cloth((166, 60, 60), HG_CREAM, "band", 2), skip=("down",))
    plate = lambda p: (190, 200, 214) if (p.face == "up" and min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0) else WHITE
    b("plate-w", (-6.5, 12, -1.5), (-3.5, 12.5, 1.5), plate, noise=3)
    b("plate-e", (3.5, 12, -1.5), (6.5, 12.5, 1.5), plate, noise=3)
    b("bowl", (-1.5, 12.3, -1.5), (1.5, 13.6, 1.5), hg_pot((90, 130, 150)))
    b("apple", (-1, 13.6, -1), (0.2, 14.8, 0.2), (200, 50, 50))
    b("pear", (0, 13.6, -0.2), (1.2, 15, 1), (190, 200, 80))
    b("goblet", (4.5, 12, -4.5), (5.5, 14, -3.5), hg_metal(HG_STEEL))
    return m


def tea_set():
    m, b, pr = hg("tea_set", 841, noise=4)
    CH, BL = (242, 240, 232), (70, 110, 170)

    def china(p):
        if p.side() and p.j == 1:
            return BL if p.i % 2 else mul(BL, 1.3)
        if p.face == "up":
            return mul(CH, 0.94)
        return CH

    def cup(p):
        if p.face == "up":
            return CH if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0 else (150, 90, 50)
        return china(p)

    b("tray", (-7, 0, -4.5), (7, 0.8, 4.5), hg_edge(hg_wood(HG_WALNUT, 841, "h", 3), mul(HG_WALNUT, 0.7), faces=("up",)))
    pr("tray-grip", (-8, 0.4, -1.5), (-7, 1.4, 1.5), HG_BRASS)
    b("pot", (-4, 0.8, -2), (0, 4.8, 2), china)
    b("pot-lid", (-3.5, 4.8, -1.5), (-0.5, 5.5, 1.5), CH)
    b("pot-knob", (-2.5, 5.5, -0.5), (-1.5, 6.3, 0.5), BL)
    b("spout", (-5.5, 2.4, -0.5), (-4, 3.4, 0.5), CH)
    b("spout-tip", (-6.2, 3.2, -0.5), (-5.2, 4.4, 0.5), CH)
    b("pot-handle", (0, 1.6, -0.4), (1.2, 4.2, 0.4), lambda p: CLEAR if (p.face in ("north", "south") and p.i == (0 if p.face == "south" else p.w - 1) and 0 < p.j < p.h - 1) else CH)
    for k, (x, z) in enumerate(((3, -2.8), (4.5, 1.6))):
        b("saucer-%d" % k, (x - 0.5, 0.8, z - 0.5), (x + 2.5, 1.2, z + 2.5), china)
        b("cup-%d" % k, (x, 1.2, z), (x + 2, 3, z + 2), cup)
        b("cup-loop-%d" % k, (x + 2, 1.7, z + 0.7), (x + 2.7, 2.7, z + 1.3), BL)
    b("sugar", (0.8, 0.8, 2), (2.6, 2.4, 3.8), cup)
    return m


def coat_rack():
    m, b, pr = hg("coat_rack", 843)
    wd = hg_wood(HG_WALNUT, 843, "v", 2)
    b("sole-x", (-5, 0, -0.8), (5, 1.2, 0.8), wd)
    b("sole-z", (-0.8, 0, -5), (0.8, 1.2, 5), wd)
    b("pole", (-0.8, 1.2, -0.8), (0.8, 29, 0.8), lambda p: mul(HG_WALNUT, 0.76) if p.j % 7 == 3 else HG_WALNUT)
    b("knob", (-1.3, 29, -1.3), (1.3, 30.8, 1.3), hg_metal(HG_BRASS))
    for k, (dx, dz) in enumerate(((-1, 0), (1, 0), (0, -1), (0, 1))):
        lo = (min(dx * 0.8, dx * 4) - (0.4 if dz else 0), 25, min(dz * 0.8, dz * 4) - (0.4 if dx else 0))
        hi = (max(dx * 0.8, dx * 4) + (0.4 if dz else 0), 25.9, max(dz * 0.8, dz * 4) + (0.4 if dx else 0))
        b("peg-%d" % k, lo, hi, HG_BRASS)
        b("peg-tip-%d" % k, (dx * 3.6 - 0.45, 25.9, dz * 3.6 - 0.45), (dx * 3.6 + 0.45, 27.2, dz * 3.6 + 0.45), HG_BRASS)
    b("coat", (1.6, 12, -2.5), (4.2, 25.5, 2.5), hg_cloth((70, 90, 120), (58, 76, 104), "weave"),
      lambda c: [c.px(c.w // 2, y, HG_BRASS) for y in (3, 6, 9)] if c.face == "east" else None)
    b("coat-collar", (1.4, 24.5, -2.8), (4.4, 26.2, 2.8), (52, 68, 94))
    b("scarf", (-4.2, 15, -1), (-3.4, 25.2, 1), hg_cloth((196, 70, 60), HG_CREAM, "band", 2))
    b("hat-brim", (-2.6, 30.8, -2.6), (2.6, 31.3, 2.6), (84, 66, 52))
    b("hat", (-1.6, 31.3, -1.6), (1.6, 33, 1.6), lambda p: (40, 34, 30) if (p.side() and p.j == p.h - 1) else (84, 66, 52))
    return m


def crate():
    m, b, pr = hg("crate", 845)
    wd = hg_wood(HG_PINE, 845, "h", 3)
    DK = mul(HG_PINE, 0.72)

    def braced(c):
        if c.face in ("north", "south", "east", "west"):
            for k in range(c.w):
                c.px(k, int(k * (c.h - 1) / max(1, c.w - 1)), DK)
            c.rect(0, 0, c.w, 1, DK), c.rect(0, c.h - 1, c.w, 1, DK)
        if c.face == "north":
            c.rect(c.w // 2 - 2, c.h // 2 - 1, 4, 3, (236, 226, 200))
            c.rect(c.w // 2 - 1, c.h // 2, 2, 1, (120, 60, 50))

    pr("skid", (-6.5, 0, -7), (-4.5, 1, 7), DK)
    b("box", (-6.5, 1, -6.5), (6.5, 14, 6.5), wd, braced)
    hg_quad(b, "batten", (-7, 1, -7), (-5.5, 14, -5.5), hg_wood(mul(HG_PINE, 0.86), 846, "v", 2))
    pr("lid-slat", (-6.5, 14, -6.5), (-2.5, 14.8, 6.5), hg_wood(mul(HG_PINE, 1.06), 847, "h", 3))
    b("lid-slat-m", (-2, 14, -6.5), (2, 14.8, 6.5), hg_wood(HG_PINE, 848, "h", 3))
    return m


def barrel():
    m, b, pr = hg("barrel", 847)
    WD = (160, 112, 62)

    def stave(p):
        if p.face in ("up", "down"):
            if min(p.i, p.w - 1 - p.i, p.j, p.h - 1 - p.j) == 0:
                return mul(WD, 0.7)
            return mul(WD, 0.8) if p.i % 3 == 0 else mul(WD, 1.05)
        return hg_wood(WD, 847, "v", 2)(p)

    hoop = hg_metal((92, 94, 104))
    b("foot", (-5, 0, -5), (5, 3, 5), stave)
    b("belly", (-6, 3, -6), (6, 13, 6), stave)
    b("head", (-5, 13, -5), (5, 16, 5), stave)
    b("hoop-a", (-6.3, 3.5, -6.3), (6.3, 4.8, 6.3), hoop, skip=("up", "down"))
    b("hoop-b", (-6.3, 11.2, -6.3), (6.3, 12.5, 6.3), hoop, skip=("up", "down"))
    b("hoop-c", (-5.3, 14.4, -5.3), (5.3, 15.4, 5.3), hoop, skip=("up", "down"))
    b("tap", (-0.6, 5.5, -7.8), (0.6, 6.7, -6), HG_BRASS)
    b("tap-key", (-0.3, 6.7, -7.4), (0.3, 8, -6.8), HG_BRASS)
    return m


def anvil():
    m, b, pr = hg("anvil", 849)
    IR = (72, 74, 84)
    ir = hg_metal(IR)
    face = hg_top(ir, lambda p: mul(IR, 1.5) if (p.i + p.j) % 5 else mul(IR, 1.8))
    b("block", (-5, 0, -5), (5, 5, 5), hg_log((98, 72, 50), (200, 164, 110), 849, "y"))
    b("foot", (-4.5, 5, -3.5), (4.5, 7, 3.5), ir)
    b("waist", (-2.5, 7, -2.5), (2.5, 10, 2.5), ir)
    b("table", (-4, 10, -3), (6, 13.5, 3), face)
    b("horn", (-7, 11, -2), (-4, 13.5, 2), face)
    b("horn-tip", (-8.5, 12, -1), (-7, 13.5, 1), face)
    b("heel", (6, 12, -2.5), (7.5, 13.5, 2.5), face)
    b("hammer-haft", (-1, 13.5, -0.5), (5.5, 14.3, 0.3), HG_OAK, rot=(0, 24, 0))
    b("hammer", (-2.5, 13.5, -1.6), (-0.5, 15.3, 1.2), hg_metal((120, 122, 132)), rot=(0, 24, 0), origin=(2.25, 13.9, -0.1))
    return m


def spinning_wheel():
    m, b, pr = hg("spinning_wheel", 851)
    wd = hg_wood((170, 120, 68), 851, "v", 2)
    DK = (128, 86, 48)

    def rim(p):
        cx, cy = (p.w - 1) / 2.0, (p.h - 1) / 2.0
        dx, dy = p.i - cx, p.j - cy
        d = math.hypot(dx, dy)
        if d > 6.6:
            return CLEAR
        if d > 5.2:
            return DK
        if d < 1.3:
            return HG_BRASS
        if abs(dx) < 0.6 or abs(dy) < 0.6 or abs(abs(dx) - abs(dy)) < 0.7:
            return (170, 120, 68)
        return CLEAR

    b("bench", (-2, 6, -8), (2, 7.5, 4), hg_wood((170, 120, 68), 852, "h", 2), rot=(-6, 0, 0), origin=(0, 6.75, -2))
    pr("foot-n", (-2, 0, -7.5), (-1, 6.5, -6.5), wd)
    b("foot-s", (-0.5, 0, 2.5), (0.5, 6, 3.5), wd)
    b("wheel", (-0.4, 4, -4.5), (0.4, 17, 8.5), rim, skip=("up", "down", "north", "south"), noise=3)
    pr("upright", (-1.8, 6.5, 1.5), (-0.9, 12, 2.5), wd)
    b("axle", (-2.2, 10, 1.5), (2.2, 11, 2.5), DK)
    b("crank", (2.2, 8, 1.7), (2.8, 11, 2.3), HG_IRON)
    pr("maiden", (-1.6, 7, -7.5), (-0.8, 13, -6.7), wd)
    b("flyer", (-0.8, 11, -7.6), (0.8, 12.4, -6.6), (240, 236, 224))
    b("treadle", (-1.5, 0.6, -5), (1.5, 1.3, 1.5), DK, rot=(-8, 0, 0), origin=(0, 0.6, 1.5))
    b("footman", (2.3, 1, -0.2), (2.8, 9, 0.3), DK, rot=(-14, 0, 0), origin=(2.5, 9, 2))
    b("distaff", (1.2, 7, -6), (1.9, 20, -5.3), wd)
    b("wool", (0.3, 17, -6.9), (2.8, 22, -4.4), lambda p: (250, 248, 240) if hsh(851, p.i, p.j) > 0.4 else (222, 216, 204))
    return m


def aquarium():
    m, b, pr = hg("aquarium", 853)
    FR = (50, 54, 62)
    WT = (70, 150, 200)

    def water(p):
        if p.face == "up":
            return mix(WT, WHITE, 0.4) if (p.i + p.j * 2) % 5 == 0 else mix(WT, WHITE, 0.2)
        c = mix((48, 110, 176), (110, 190, 226), p.ly)
        if p.j >= p.h - 2:
            return (214, 196, 150) if hsh(853, p.i, p.j, FACES.index(p.face)) > 0.3 else (170, 150, 120)
        return c

    def life(c):
        if c.face in ("up", "down"):
            return
        s = FACES.index(c.face)
        for x in range(1, c.w - 1, 3):      # water weeds
            hgt = 2 + int(hsh(854, s, x) * 4)
            for y in range(hgt):
                c.px(x + (y % 2 if hsh(855, s, x) > 0.5 else 0), c.h - 3 - y, (60, 150, 80) if y % 2 else (44, 120, 70))
        if c.w < 10:
            fish = [(1, 2, (250, 150, 50), 1)]
        else:
            fish = [(2, 2, (250, 150, 50), 1), (8, 4, (240, 230, 90), -1), (4, 5, (236, 90, 110), 1)]
        for x, y, col, d in fish:
            c.rect(x, y, 3, 2, col)
            tx = x - 1 if d > 0 else x + 3
            c.px(tx, y, mul(col, 0.8)), c.px(tx, y + 1, mul(col, 0.8))
            c.px(x + (2 if d > 0 else 0), y, BLACK)
            c.px(x + 1, y + 1, WHITE)
        for x, y in ((c.w - 3, 1), (c.w - 2, 3), (c.w - 3, 5)):
            c.px(x, y, (220, 240, 250))

    b("cabinet", (-8, 0, -4), (8, 5, 4), hg_edge(hg_wood(HG_WALNUT, 853, "v", 4), mul(HG_WALNUT, 0.7), faces=("north",)),
      lambda c: [c.px(x, 2, HG_BRASS) for x in (c.w // 2 - 1, c.w // 2)] if c.face == "north" else None)
    b("sill", (-8, 5, -4), (8, 6, 4), FR)
    b("tank", (-7.5, 6, -3.5), (7.5, 15, 3.5), water, life, noise=3)
    hg_quad(b, "mullion", (-8, 6, -4), (-7.2, 15, -3.2), FR)
    b("hood", (-8, 15, -4), (8, 16, 4), FR)
    b("lamp", (-4, 16, -1), (4, 16.8, 1), hg_metal(HG_STEEL))
    return m


def bird_cage():
    m, b, pr = hg("bird_cage", 857)
    br = hg_metal(HG_BRASS)

    def bars(p):
        if p.j in (0, p.h - 1) or p.j == p.h // 2 or p.i % 2 == 0:
            return HG_BRASS if p.i % 4 else mul(HG_BRASS, 1.2)
        return CLEAR

    b("foot", (-4, 0, -4), (4, 1, 4), br)
    b("step", (-2, 1, -2), (2, 2, 2), br)
    b("pole", (-0.7, 2, -0.7), (0.7, 14, 0.7), br)
    b("tray", (-4.5, 14, -4.5), (4.5, 15, 4.5), hg_top(br, (206, 196, 160)))
    b("bars", (-4, 15, -4), (4, 24, 4), bars, skip=("up", "down"), noise=0)
    b("roof", (-4.5, 24, -4.5), (4.5, 25, 4.5), br)
    b("dome", (-2.5, 25, -2.5), (2.5, 26.5, 2.5), br)
    b("ring", (-0.5, 26.5, -0.5), (0.5, 28, 0.5), br)
    b("perch", (-4, 18, -0.3), (4, 18.6, 0.3), HG_OAK)
    b("songbird", (-1, 18.6, -1.2), (1, 20.8, 1), lambda p: (250, 214, 70) if p.ly > 0.3 else (240, 180, 60),
      lambda c: (c.px(0, 0, BLACK), c.px(c.w - 1, 0, BLACK)) if c.face == "north" else None, noise=3)
    b("bill", (-0.3, 19.6, -1.8), (0.3, 20.1, -1.2), (236, 130, 50))
    b("seed-cup", (2.5, 15, -3.5), (3.5, 16, -2.5), (236, 236, 240))
    return m


def ladder_shelf():
    m, b, pr = hg("ladder_shelf", 859)
    wd = hg_wood((196, 156, 100), 859, "v", 2)
    sh = hg_wood((214, 176, 120), 860, "h", 3)
    pr("stringer", (-7.5, 0, -4), (-6.5, 31, -3), wd, rot=(14, 0, 0), origin=(0, 0, -3.5))
    pr("upright", (-7.5, 0, 4), (-6.5, 30, 5), wd)
    for k, (y, z) in enumerate(((5, -3.5), (12, -1.8), (19, 0), (26, 1.8))):
        b("shelf-%d" % k, (-7.5, y, z), (7.5, y + 1, 5), sh)
    b("books", (-6, 6, 0), (-1, 10, 3.5), hg_books(861), noise=4)
    b("pot", (2.5, 6, -1.5), (5.5, 9, 1.5), hg_pot(HG_TERRA))
    b("pot-plant", (2, 9, -2), (6, 12, 2), hg_leaves(HG_LEAF, 862))
    b("basket", (-5.5, 13, 0.5), (-1.5, 16, 4), hg_cloth((190, 150, 90), (150, 110, 60), "weave"))
    b("jug", (2, 13, 1), (4.5, 17, 3.5), hg_pot((90, 130, 150), band=HG_CREAM))
    b("books-b", (-3, 20, 1.5), (3, 24, 4.5), hg_books(863), noise=4)
    b("candle", (-1, 27, 2.5), (0.2, 29.5, 3.7), HG_CREAM)
    b("frame", (2, 27, 3.6), (6, 30.5, 4.2), hg_edge((150, 190, 214), HG_GOLD))
    return m


def rug_stand():
    m, b, pr = hg("rug_stand", 865)
    wd = hg_wood((186, 142, 88), 865, "v", 2)
    DK = (140, 100, 58)
    pat = [(170, 56, 60), (236, 214, 150), (50, 96, 130), (236, 214, 150), (196, 120, 50)]

    def rug(p):
        k = p.j if p.face in ("north", "south", "up", "down") else p.i
        c = pat[(k // 2) % len(pat)]
        if k % 2 == 0 and p.i % 3 == 1:
            return mul(c, 0.76)
        return c

    def warp(p):
        k = int(p.lz * p.h)     # 0 at the weaver's end
        if k < 4:
            return rug(p)
        return (236, 228, 204) if p.i % 2 == 0 else CLEAR

    def heddle(p):
        if p.j in (0, p.h - 1):
            return DK
        return (226, 216, 190) if p.i % 2 == 0 else CLEAR

    flat = ("north", "south", "east", "west")
    pr("post-n", (-7.5, 0, -6), (-6.5, 14, -5), wd)
    pr("post-s", (-7.5, 0, 5), (-6.5, 24, 6), wd)
    pr("sill", (-7.5, 1, -5), (-6.5, 2.2, 5), DK)
    pr("rail", (-7.5, 12.5, -5), (-6.5, 13.7, 5), DK)
    b("cloth-beam", (-6.5, 11, -6.3), (6.5, 13.4, -4), rug)
    b("warp-beam", (-6.5, 17.5, 4.5), (6.5, 19.5, 6.5), lambda p: (236, 228, 204) if p.i % 2 == 0 else (206, 196, 170))
    b("warp", (-5.5, 13.4, -4.5), (5.5, 13.8, 6), warp, skip=flat, rot=(-27, 0, 0), origin=(0, 13.4, -4.5), noise=3)
    b("heddle", (-6, 14, 0.2), (6, 23, 0.7), heddle, skip=("up", "down", "east", "west"), noise=3)
    b("lintel", (-7.5, 23, 4.8), (7.5, 24.2, 6.2), DK)
    pr("hanger", (-6.4, 22, 0), (-5.8, 23.2, 5), DK)
    b("treadle-a", (-3, 0, -5.5), (-1, 0.8, 4), DK)
    b("treadle-b", (1, 0, -5.5), (3, 0.8, 4), DK)
    b("stretcher", (-6.5, 4, 5), (6.5, 5.2, 6), DK)
    b("rug", (-5, 2.5, -6.7), (5, 11.5, -6.3), rug, skip=("up", "down", "east", "west"))
    b("shuttle", (-1.5, 13.4, -6.2), (2, 14.1, -5.4), (110, 70, 44))
    return m


HOUSE = [grandfather_clock, piano, harp, globe, telescope, easel, vase, candelabra, chandelier, hanging_lantern, fireplace, bust,
         knight_statue, tall_bookcase, wing_chair, rocking_chair, cradle, dining_table, tea_set, coat_rack, crate, barrel, anvil,
         spinning_wheel, aquarium, bird_cage, ladder_shelf, rug_stand]
HEARTH_GARDEN = GARDEN + HOUSE
RIGID.update(fn.__name__ for fn in HEARTH_GARDEN)
# <<< hearthgarden pack


ANIMALS = [cow, pig, chicken, cat, dog, tiger, elephant, bear, gator, snake, fish, squid, worm, ant, sheep, horse, frog, duck]
VEHICLES = [boat, cargo_boat, cart, chest_cart, plane]
LAND_VEHICLES = [car]   # the car belongs to the landvehicles mod
LAND_DIR = os.path.join(MODS_DIR, "landvehicles", "models")

# Where each set of models is written: (output directory, model functions). A
# plain run writes them all; add a pack by adding a folder and its list here.
OUTPUTS = [
    (OUT_DIR, ANIMALS + MORE_ANIMALS + FARM + MONSTERS),
    # Vehicles belong to their mods.
    (os.path.join(MODS_DIR, "boats", "models"), [boat, cargo_boat]),
    (os.path.join(MODS_DIR, "trains", "models"), [cart, chest_cart]),
    (os.path.join(MODS_DIR, "planes", "models"), [plane]),
    (PACK_DIR, SPELLS_N_MAGIC),
    (TITAN_DIR, TITANLANDS),
    (LAND_DIR, LAND_VEHICLES),
    (HG_DIR, HEARTH_GARDEN),
]

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
    if d["name"] in HG_HANGING:   # these hang from the top of their block instead
        assert max(e["to"][1] for e in d["elements"]) == 16, "a hanging prop must reach y = 16"
    else:
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
    built = []
    for out_dir, fn in [(d, fn) for d, fns in OUTPUTS for fn in fns]:
        if only and fn.__name__ not in only:
            continue
        os.makedirs(out_dir, exist_ok=True)
        m = fn()
        assert m.id == fn.__name__
        path = os.path.join(out_dir, m.id + ".bbmodel")
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
