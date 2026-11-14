#!/usr/bin/env python3
"""Draws the Wildlife mod (mods/wildlife): every creature's model, its texture
and its entry in pack.txt, all from the tables in this file.

    python3 tools/wildlife_textures.py                 # write models, textures and pack.txt
    python3 tools/wildlife_textures.py --sheet out.png # also write a contact sheet to look at
    python3 tools/wildlife_textures.py --only otter,toad

What it writes, in mods/wildlife/:
    models/<id>.bbmodel         the model, with its texture embedded (what the game reads)
    textures/<id>.png           the same texture on its own, to look at or repaint
    pack.txt                    the sounds, models and `entity` sections

A creature is a handful of boxes hung on bones the game animates by name
(Head, Leg-FL, Shin-FL, tail-a, wing-l, ear-l ... see docs/DATA.md). Every box
face is painted in the OCD style (docs/TEXTURING.md): flat fills, a few colours,
a one pixel darker frame, no noise. The colours are the PALETTE table below:
one base set of named colours for each species, and for each of its variants
only the colours that differ. Change a colour and run the script again and
every model, texture and pack.txt line follows. The first look of a species is
the creature itself; the others become its `variant` lines.

Conventions: y is up, the front faces -z, the left side is -x, feet stand on
y = 0, one model unit is one texel.
"""
import base64
import io
import json
import math
import os
import sys
import uuid

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))  # the mods live outside the repository
MOD = os.path.join(MODS_DIR, "wildlife")
FACES = ("north", "south", "east", "west", "up", "down")
SIDES = ("north", "south", "east", "west")
FRAME = 0.80          # how much darker the one pixel frame of every face is
MAX_TEX = 128


# ======================================================================
#  Painting machinery
# ======================================================================

def hsh(*a):
    h = 2166136261
    for v in a:
        h ^= int(v) & 0xFFFFFFFF
        h = (h * 16777619) & 0xFFFFFFFF
        h ^= h >> 13
        h = (h * 0x5BD1E995) & 0xFFFFFFFF
        h ^= h >> 15
    return h / 4294967296.0


def rgb(h):
    if isinstance(h, tuple):
        return h
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


def face_frame(f, lo, hi):
    """Top-left, top-right and bottom-left corners of a face, as the game maps them."""
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


def rot_matrix(deg):
    rx, ry, rz = (math.radians(v) for v in deg)
    sx, cx, sy, cy, sz, cz = math.sin(rx), math.cos(rx), math.sin(ry), math.cos(ry), math.sin(rz), math.cos(rz)
    return [[cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
            [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
            [-sy, cy * sx, cy * cx]]


class P:
    """What a rule gets to know about one texel."""
    __slots__ = ("x", "y", "z", "lx", "ly", "lz", "face", "i", "j", "w", "h", "part")


def colour(pal, slot):
    """A slot name -> RGBA. `clear` is see-through; `~name` is a darker name, `+name` a lighter one."""
    if slot == "clear":
        return (0, 0, 0, 0)
    if slot[0] == "~":
        c = pal[slot[1:]]
        return tuple(int(v * 0.78) for v in c) + (255,)
    if slot[0] == "+":
        c = pal[slot[1:]]
        return tuple(min(255, int(v * 1.18 + 14)) for v in c) + (255,)
    return tuple(pal[slot]) + (255,)


class Part:
    pass


class Model:
    def __init__(self, mid, pal):
        self.id, self.pal = mid, pal
        self.parts = []
        self.bones = {"Body": ((0, 0, 0), None)}

    def bone(self, name, pivot, parent="Body"):
        self.bones[name] = (tuple(float(v) for v in pivot), parent)

    def box(self, name, bone, lo, hi, slot, rules=(), rot=None, origin=None, skip=(), frame=True):
        assert all(lo[a] < hi[a] for a in range(3)), (self.id, name, lo, hi)
        assert bone in self.bones, (self.id, name, bone)
        assert slot == "clear" or slot.lstrip("~+") in self.pal, (self.id, name, slot)
        p = Part()
        p.name, p.bone = name, bone
        p.lo, p.hi = tuple(float(v) for v in lo), tuple(float(v) for v in hi)
        p.slot, p.rules, p.skip, p.frame = slot, [r for r in rules if r], tuple(skip), frame
        p.rot = tuple(float(v) for v in rot) if rot else (0.0, 0.0, 0.0)
        p.origin = tuple(float(v) for v in origin) if origin else tuple((lo[a] + hi[a]) / 2 for a in range(3))
        self.parts.append(p)
        return p

    def pair(self, name, bones, lo, hi, slot, rules=(), rot=None, origin=None, skip=(), frame=True):
        """A box on the left (-x) side and its mirror image on the right."""
        bl, br = bones if isinstance(bones, tuple) else (bones, bones)
        self.box(name + "-l", bl, lo, hi, slot, rules, rot, origin, skip, frame)
        mrot = (rot[0], -rot[1], -rot[2]) if rot else None
        morg = (-origin[0], origin[1], origin[2]) if origin else None
        self.box(name + "-r", br, (-hi[0], lo[1], lo[2]), (-lo[0], hi[1], hi[2]), slot, rules, mrot, morg, skip, frame)

    # -- texture

    def paint(self):
        self.faces = {}
        for pi, part in enumerate(self.parts):
            part.tex = {}
            for f in FACES:
                if f in part.skip:
                    continue
                w, h = face_dims(f, part.lo, part.hi)
                tl, tr, bl = face_frame(f, part.lo, part.hi)
                px = []
                for j in range(h):
                    row = []
                    for i in range(w):
                        u, v = (i + 0.5) / w, (j + 0.5) / h
                        q = P()
                        q.x, q.y, q.z = (tl[a] + (tr[a] - tl[a]) * u + (bl[a] - tl[a]) * v for a in range(3))
                        q.lx = (q.x - part.lo[0]) / (part.hi[0] - part.lo[0])
                        q.ly = (q.y - part.lo[1]) / (part.hi[1] - part.lo[1])
                        q.lz = (q.z - part.lo[2]) / (part.hi[2] - part.lo[2])
                        q.face, q.i, q.j, q.w, q.h, q.part = f, i, j, w, h, part.name
                        slot = part.slot
                        for r in part.rules:
                            s = r(q)
                            if s:
                                slot = s
                        c = colour(self.pal, slot)
                        if part.frame and w >= 4 and h >= 4 and c[3] and (i in (0, w - 1) or j in (0, h - 1)):
                            c = (int(c[0] * FRAME), int(c[1] * FRAME), int(c[2] * FRAME), 255)
                        row.append(c)
                    px.append(row)
                part.tex[f] = px
                self.faces[(pi, f)] = px

    def pack(self):
        """Shelf-packs every face, with a 1 px replicated gutter, into the smallest texture."""
        rects = [(len(px), len(px[0]), key) for key, px in self.faces.items()]
        rects.sort(key=lambda r: (-r[0], -r[1], r[2]))
        for tw, th in ((32, 32), (64, 32), (64, 64), (128, 64), (128, 128)):
            pos, x, y, shelf, ok = {}, 0, 0, 0, True
            for h, w, key in rects:
                if x + w + 2 > tw:
                    x, y, shelf = 0, y + shelf, 0
                if y + h + 2 > th or w + 2 > tw:
                    ok = False
                    break
                pos[key] = (x + 1, y + 1)
                x += w + 2
                shelf = max(shelf, h + 2)
            if ok:
                break
        else:
            raise SystemExit("%s: texture does not fit in %d" % (self.id, MAX_TEX))
        img = Image.new("RGBA", (tw, th), (0, 0, 0, 0))
        out = img.load()
        for key, (x0, y0) in pos.items():
            px = self.faces[key]
            h, w = len(px), len(px[0])
            for j in range(-1, h + 1):
                for i in range(-1, w + 1):
                    out[x0 + i, y0 + j] = px[min(max(j, 0), h - 1)][min(max(i, 0), w - 1)]
        for pi, part in enumerate(self.parts):
            part.uv = {}
            for f in part.tex:
                x0, y0 = pos[(pi, f)]
                part.uv[f] = [x0, y0, x0 + len(part.tex[f][0]), y0 + len(part.tex[f])]
        self.image = img

    def build(self):
        self.paint()
        self.pack()
        return self

    def bounds(self):
        lo, hi = [1e9] * 3, [-1e9] * 3
        for p in self.parts:
            r = rot_matrix(p.rot)
            for cx in (p.lo[0], p.hi[0]):
                for cy in (p.lo[1], p.hi[1]):
                    for cz in (p.lo[2], p.hi[2]):
                        q = [cx - p.origin[0], cy - p.origin[1], cz - p.origin[2]]
                        w = [sum(r[i][k] * q[k] for k in range(3)) + p.origin[i] for i in range(3)]
                        for a in range(3):
                            lo[a], hi[a] = min(lo[a], w[a]), max(hi[a], w[a])
        return lo, hi

    def to_json(self):
        ns = uuid.UUID("3b6c1e2a-5d0f-4c1e-9a77-0c4d2b8f6a11")
        elements, by_bone = [], {}
        for i, p in enumerate(self.parts):
            p.uuid = str(uuid.uuid5(ns, "%s/%d/%s" % (self.id, i, p.name)))
            elements.append({
                "name": p.name, "type": "cube", "uuid": p.uuid,
                "from": list(p.lo), "to": list(p.hi),
                "origin": list(p.origin), "rotation": list(p.rot),
                "faces": {f: {"uv": p.uv[f], "texture": 0} for f in FACES if f in p.uv},
            })
            by_bone.setdefault(p.bone, []).append(p.uuid)

        def node(name):
            pivot, _ = self.bones[name]
            kids = list(by_bone.get(name, []))
            kids += [node(b) for b, (_, par) in self.bones.items() if par == name]
            return {"name": name, "origin": list(pivot), "rotation": [0.0, 0.0, 0.0], "children": kids}

        buf = io.BytesIO()
        self.image.save(buf, "PNG")
        return {
            "meta": {"format_version": "4.10", "model_format": "free", "box_uv": False},
            "name": self.id,
            "resolution": {"width": self.image.width, "height": self.image.height},
            "elements": elements,
            "outliner": [node(b) for b, (_, par) in self.bones.items() if par is None],
            "textures": [{"name": self.id + ".png", "source": "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode("ascii")}],
        }


# ----------------------------------------------------------------------
#  Rules: small functions from a texel to a colour slot name (or None)
# ----------------------------------------------------------------------

def belly(slot, h=0.4):
    return lambda p: slot if (p.face == "down" or (p.face in SIDES and p.ly < h)) else None


def topc(slot, h=0.3):
    return lambda p: slot if (p.face == "up" or (p.face in SIDES and p.ly > 1 - h)) else None


def on(slot, *faces):
    return lambda p: slot if p.face in faces else None


def where(slot, fn, faces=None):
    return lambda p: slot if ((faces is None or p.face in faces) and fn(p)) else None


def px(slot, face, i, j, mirror=True):
    """One texel of a face (i from the left, j from the top), and its mirror image."""
    return lambda p: slot if (p.face == face and p.j == j and (p.i == i or (mirror and p.i == p.w - 1 - i))) else None


def pxs(slot, face, cells, mirror=True):
    fs = [px(slot, face, i, j, mirror) for i, j in cells]
    return lambda p: next((s for s in (f(p) for f in fs) if s), None)


def side_px(slot, k, j):
    """A texel of an east/west face, k texels back from the front, j down from the top."""
    def f(p):
        if p.face in ("east", "west") and p.j == j and p.i == (p.w - 1 - k if p.face == "east" else k):
            return slot
        return None
    return f


def lateral(slot, j, faces=("east", "west")):
    return lambda p: slot if (p.face in faces and p.j == j) else None


def dorsal(slot, half=0.6):
    return lambda p: slot if (p.face == "up" and abs(p.x) < half) else None


def band(slot, period, width, off=0, axis="z", faces=("up", "down", "east", "west")):
    def f(p):
        if p.face not in faces:
            return None
        v = {"x": p.x, "y": p.y, "z": p.z}[axis]
        return slot if (int(math.floor(v)) + off) % period < width else None
    return f


def spots(slot, seed, dens=0.3, cell=2, size=1, faces=("up", "east", "west")):
    """Flat spots of size x size texels, one at most in each cell x cell block of a face."""
    def f(p):
        if p.face not in faces:
            return None
        a, b = {"up": (p.x, p.z), "down": (p.x, p.z), "east": (p.z, p.y), "west": (p.z, p.y), "north": (p.x, p.y), "south": (p.x, p.y)}[p.face]
        ia, ib = int(math.floor(a)), int(math.floor(b))
        ca, cb = ia // cell, ib // cell
        if hsh(seed, ca, cb, FACES.index(p.face)) >= dens:
            return None
        oa = int(hsh(seed + 1, ca, cb) * (cell - size + 1))
        ob = int(hsh(seed + 2, ca, cb) * (cell - size + 1))
        return slot if (ia - ca * cell - oa) in range(size) and (ib - cb * cell - ob) in range(size) else None
    return f


def zigzag(slot, step=2):
    """A row of dark diamonds down the back, left and right in turn."""
    def f(p):
        if p.face != "up":
            return None
        k = int(math.floor(p.z / step))
        return slot if (k % 2 == 0) == (p.i < p.w / 2.0) else None
    return f


def grid(slot, n=3, faces=("up",)):
    return lambda p: slot if (p.face in faces and (p.i % n == 0 or p.j % n == 0)) else None


def checks(slot, faces=("up",)):
    return lambda p: slot if (p.face in faces and (p.i + p.j) % 2 == 0) else None


def nose(slot="nose", w=2):
    def f(p):
        if p.face == "north" and p.j == 0 and abs(p.i - (p.w - 1) / 2.0) <= (w - 1) / 2.0 + 0.01:
            return slot
        return None
    return f


# ----------------------------------------------------------------------
#  Shared body plans
# ----------------------------------------------------------------------

def tail_chain(m, y, z0, segs, slot, rules=(), droop=0.0):
    """A tail of boxes, each a child of the one before it (`tail`, `tail-a`, `tail-b` ...)
    so the pieces trail one another when it wags. segs: (width, height, length)."""
    parent, z = "Body", z0
    for k, (w, h, l) in enumerate(segs):
        name = "tail" if k == 0 else "tail-" + chr(96 + k)
        yy = y - droop * k
        m.bone(name, (0, yy, z), parent)
        m.box("tail-%d" % k, name, (-w / 2, yy - h / 2, z), (w / 2, yy + h / 2, z + l), slot, rules)
        parent, z = name, z + l
    return z


def legs4(m, zf, zb, top, xc, w, d, slot, foot, rules=(), foot_rules=(), knee=0.5, wb=None, db=None):
    """Four legs, each in two pieces (Leg-FL and its Shin-FL) with the knee at `knee` of the way down."""
    kn = top * knee
    for tag, z, ww, dd in (("F", zf, w, d), ("B", zb, wb or w, db or d)):
        for side, sx in (("L", -1), ("R", 1)):
            cx = xc * sx
            m.bone("Leg-%s%s" % (tag, side), (cx, top, z))
            m.bone("Shin-%s%s" % (tag, side), (cx, kn, z), "Leg-%s%s" % (tag, side))
            m.box("leg-%s%s" % (tag, side), "Leg-%s%s" % (tag, side), (cx - ww / 2, kn, z - dd / 2), (cx + ww / 2, top + 0.5, z + dd / 2), slot, rules)
            m.box("shin-%s%s" % (tag, side), "Shin-%s%s" % (tag, side), (cx - ww / 2, 0, z - dd / 2), (cx + ww / 2, kn, z + dd / 2), foot, foot_rules)


def mammal(m, p, a):
    """A four legged mammal. Returns the places an extra part may hang from."""
    bw, bh, bl = a["body"]
    leg = a["leg"]
    y0, y1, zf, zb = leg, leg + bh, -bl / 2.0, bl / 2.0
    body_r = []
    if "belly" in p:
        body_r.append(belly("belly", a.get("belly_h", 0.35)))
    if "back" in p:
        body_r.append(topc("back", a.get("back_h", 0.3)))
    if "spot" in p:
        body_r.append(spots("spot", a.get("seed", 5), a.get("dens", 0.3), 3, a.get("ssize", 1), ("up", "east", "west")))
    body_r += a.get("body_r", [])
    m.box("body", "Body", (-bw / 2, y0, zf), (bw / 2, y1, zb), "body", body_r)
    lw, ld = a["legsz"]
    wb, db = a.get("legsz_b", (lw, ld))
    xc = bw / 2 - lw / 2
    legs4(m, zf + ld / 2 + a.get("fz", 0.3), zb - db / 2 - a.get("bz", 0.3), leg, xc, lw, ld, "leg", "sock" if "sock" in p else "leg",
          a.get("leg_r", []), a.get("foot_r", []), a.get("knee", 0.5), wb, db)
    hw, hh, hl = a["head"]
    if a.get("neck"):
        nl, nt, ang = a["neck"]
        by, bz = y1 - nt * 0.5, zf + nt * 0.6 + a.get("neck_z", 0.0)
        m.box("neck", "Body", (-nt / 2, by, bz - nt / 2), (nt / 2, by + nl, bz + nt / 2), "neck" if "neck" in p else "body",
              a.get("neck_r", []), rot=(-ang, 0, 0), origin=(0, by, bz))
        hcy = by + nl * math.cos(math.radians(ang))
        fz = bz - nl * math.sin(math.radians(ang)) + hl * 0.3
    else:
        hcy = y0 + bh * a.get("head_h", 0.6)
        fz = zf + 1.2
    hpiv = (0, hcy, fz - hl * 0.35)
    m.bone("Head", hpiv)
    head_r = [on("muzzle", "down")] if "muzzle" in p and not a.get("snout") else []
    head_r += a.get("head_r", [])
    ek, ej = a.get("eye", (max(1, int(hl * 0.35)), max(0, int(hh * 0.3))))
    head_r.append(side_px("eye", ek, ej))
    if a.get("eye_front"):
        head_r.append(px("eye", "north", *a["eye_front"]))
    m.box("head", "Head", (-hw / 2, hcy - hh / 2, fz - hl), (hw / 2, hcy + hh / 2, fz), "head", head_r)
    G = dict(y0=y0, y1=y1, zf=zf, zb=zb, hcy=hcy, fz=fz, hw=hw, hh=hh, hl=hl, bw=bw)
    if a.get("snout"):
        sw, sh, sl = a["snout"]
        sy = hcy - hh / 2 + a.get("snout_dy", 0.0)
        sr = [nose("nose", a.get("nose_w", 2))] + a.get("snout_r", [])
        if "muzzle" in p:
            sr.insert(0, on("head", "up"))
        m.box("snout", "Head", (-sw / 2, sy, fz - hl - sl + 0.6), (sw / 2, sy + sh, fz - hl + 0.6), "muzzle" if "muzzle" in p else "head", sr)
        G["snout_front"] = fz - hl - sl + 0.6
    if a.get("ears"):
        ew, eh, et = a["ears"]
        tilt = a.get("ear_tilt", 0)
        ex = hw / 2 - ew / 2 - a.get("ear_in", 0.2) + a.get("ear_out", 0.0)
        ez = fz - hl * 0.45
        ey = hcy + hh / 2 - 0.3
        m.bone("ear-l", (-ex, ey, ez), "Head")
        m.bone("ear-r", (ex, ey, ez), "Head")
        er = []
        if "earin" in p:
            er.append(lambda q: "earin" if q.face == "north" and 0 < q.i < q.w - 1 and q.j > 0 else None)
        m.pair("ear", ("ear-l", "ear-r"), (-ex - ew / 2, ey, ez - et / 2), (-ex + ew / 2, ey + eh, ez + et / 2), "ear" if "ear" in p else "head", er,
               rot=(0, 0, -tilt) if tilt else None, origin=(-ex, ey, ez) if tilt else None)
    if a.get("tail"):
        tw, th, tl = a["tail"]
        ty = y1 - th - 0.2 + a.get("tail_dy", 0.0) + th * 0.5
        m.bone("tail", (0, ty + th / 2, zb - 0.3))
        tr = [where("tailtip", lambda q: q.lz > 0.72, SIDES + ("up", "down"))] if "tailtip" in p else []
        tr += a.get("tail_r", [])
        m.box("tail", "tail", (-tw / 2, ty, zb - 0.3), (tw / 2, ty + th, zb - 0.3 + tl), "tail" if "tail" in p else "body", tr,
              rot=(a.get("tail_rot", 0), 0, 0) if a.get("tail_rot") else None, origin=(0, ty + th / 2, zb - 0.3))
    if a.get("tail_box"):
        lo, hi, rot = a["tail_box"]
        m.bone("tail", (0, lo[1] + 1, lo[2] + 1))
        tr = [where("tailtip", lambda q: q.ly > 0.7 or q.lz > 0.8)] if "tailtip" in p else []
        m.box("tail", "tail", lo, hi, "tail" if "tail" in p else "body", tr + a.get("tail_r", []), rot=rot)
    return G


# ======================================================================
#  Amphibians and reptiles
# ======================================================================

def frog(m, p, a):
    bw, bh, bl = a["body"]
    hw, hh, hl = a["head"]
    e = a.get("eye", 1.8)
    y0 = a.get("lift", 1.4)
    zf, zb = -bl / 2.0, bl / 2.0
    dec = []
    if "dorsal" in p:
        dec.append(dorsal("dorsal", 0.6))
    for k in ("spot", "wart"):
        if k in p:
            dec.append(spots(k, a["seed"], a.get("dens", 0.3), a.get("cell", 2), a.get("ssize", 1)))
    if "flank" in p:
        dec.append(lateral("flank", 1))
    if "patch" in p:
        dec.append(where("patch", lambda q: q.lz < 0.4 and q.ly > 0.5, ("east", "west")))
    body_r = ([belly("belly", 0.4)] if "belly" in p else []) + dec
    m.box("body", "Body", (-bw / 2, y0, zf), (bw / 2, y0 + bh, zb), "body", body_r)
    hy = y0 + bh * 0.1
    m.bone("Head", (0, y0 + bh * 0.5, zf + 0.8))
    head_r = ([on("belly", "down")] if "belly" in p else []) + dec + [pxs("mouth", "north", [(0, hh - 2)] + [(i, hh - 2) for i in range(1, 6)], False) if "mouth" in p else None]
    m.box("head", "Head", (-hw / 2, hy, zf - hl + 1.0), (hw / 2, hy + hh, zf + 1.0), "head" if "head" in p else "body", head_r)
    top = hy + hh
    ez = zf - hl + 2.3
    pupil = [lambda q: "pupil" if q.face in ("north", "east", "west") and q.j == q.h // 2 and (q.face != "north" or q.i == q.w // 2) and (q.face == "north" or q.i == (q.w - 1 if q.face == "east" else 0) or q.w == 1) else None]
    m.pair("eye", "Head", (-hw / 2 + 0.2, top - 0.3, ez), (-hw / 2 + 0.2 + e, top - 0.3 + e, ez + e), "eye", pupil)
    # Front legs, short and straight.
    for side, sx in (("L", -1), ("R", 1)):
        x = sx * (bw / 2 - 0.5)
        m.bone("Leg-F" + side, (x, y0 + 1.2, zf + 1.2))
        m.box("leg-f" + side.lower(), "Leg-F" + side, (x - 0.65, 0, zf + 0.5), (x + 0.65, y0 + 1.3, zf + 1.9), "leg" if "leg" in p else "body",
              [on("pad", "down") if "pad" in p else None, where("pad", lambda q: q.j >= q.h - 1, ("north", "east", "west")) if "pad" in p else None])
    # Back legs: a thigh folded along the side, and a long foot lying forward from the knee.
    lt, th, tl = a.get("thigh", (1.7, bh * 0.95, 3.4))
    kz = zb - 0.9
    fl = a.get("foot", 4.4)
    for side, sx in (("L", -1), ("R", 1)):
        xc = sx * (bw / 2 + lt / 2 - 0.5)
        m.bone("Leg-B" + side, (xc, y0 + th * 0.7, zb - 1.5))
        m.bone("Shin-B" + side, (xc, 1.0, kz), "Leg-B" + side)
    m.pair("thigh", ("Leg-BL", "Leg-BR"), (-(bw / 2 + lt - 0.5), 0.9, zb - tl), (-(bw / 2 - 0.5), 0.9 + th, zb - 0.1),
           "leg" if "leg" in p else "body")
    xf = bw / 2 + lt / 2 - 0.5
    m.pair("foot", ("Shin-BL", "Shin-BR"), (-xf - 0.65, 0, kz - fl), (-xf + 0.65, 1.0, kz + 0.5), "leg" if "leg" in p else "body",
           [where("pad", lambda q: q.j >= q.h - 1 or q.lz < 0.2, ("north", "down")) if "pad" in p else None])


def lizard(m, p, a):
    """A low animal with a long tail: lizards, geckos and newts."""
    bw, bh, bl = a["body"]
    y0 = a.get("lift", 1.6)
    zf, zb = -bl / 2.0, bl / 2.0
    dec = []
    if "dorsal" in p:
        dec.append(dorsal("dorsal", a.get("dhalf", 0.6)))
    if "spot" in p:
        dec.append(spots("spot", a["seed"], a.get("dens", 0.3), 2, a.get("ssize", 1), ("up", "east", "west")))
    if "bars" in p:
        dec.append(band("bar", a.get("bperiod", 4), 1, 0, "z", ("up",)))
    if "flank" in p:
        dec.append(lateral("flank", 1))
    body_r = ([belly("belly", a.get("belly_h", 0.4))] if "belly" in p else []) + dec
    m.box("body", "Body", (-bw / 2, y0, zf), (bw / 2, y0 + bh, zb), "body", body_r)
    hw, hh, hl = a["head"]
    hy = y0 + bh * 0.05
    m.bone("Head", (0, y0 + bh * 0.5, zf + 0.5))
    hr = ([on("belly", "down")] if "belly" in p else []) + [side_px("eye", 1, 0)] + ([dorsal("dorsal", 0.6)] if "dorsal" in p else [])
    hr.append(pxs("eye", "up", [(0, 1)]))
    hr.append(pxs("nostril", "north", [(1, 0)], False) if "nostril" in p else None)
    if "collar" in p:
        hr.append(where("collar", lambda q: q.lz > 0.7, ("up", "east", "west")))
    m.box("head", "Head", (-hw / 2, hy, zf - hl + 0.8), (hw / 2, hy + hh, zf + 0.8), "head" if "head" in p else "body", hr)
    if a.get("crest"):
        cw, ch = a["crest"]
        m.box("crest", "Body", (-cw / 2, y0 + bh, zf + 1), (cw / 2, y0 + bh + ch, zb), "crest")
    ll = a.get("legl", 1.8)
    lt = a.get("legt", 1.1)
    for tag, z in (("F", zf + 1.3), ("B", zb - 1.5)):
        m.bone("Leg-%sL" % tag, (-bw / 2, y0 + 0.9, z))
        m.bone("Leg-%sR" % tag, (bw / 2, y0 + 0.9, z))
        m.bone("Shin-%sL" % tag, (-bw / 2 - ll, y0 + 0.5, z), "Leg-%sL" % tag)
        m.bone("Shin-%sR" % tag, (bw / 2 + ll, y0 + 0.5, z), "Leg-%sR" % tag)
        m.pair("leg-" + tag.lower(), ("Leg-%sL" % tag, "Leg-%sR" % tag), (-bw / 2 - ll, y0 - 0.1, z - lt / 2), (-bw / 2 + 0.3, y0 + 1.0, z + lt / 2), "leg" if "leg" in p else "body")
        m.pair("foot-" + tag.lower(), ("Shin-%sL" % tag, "Shin-%sR" % tag), (-bw / 2 - ll - 0.2, 0, z - lt / 2 - 0.9), (-bw / 2 - ll + 1.1, y0 + 0.1, z + lt / 2),
               "foot" if "foot" in p else ("leg" if "leg" in p else "body"),
               [where("pad", lambda q: q.j >= q.h - 1 or q.face == "down", ("north", "down")) if "pad" in p else None])
    tr = [dorsal("dorsal", 0.6)] if "dorsal" in p and a.get("dorsal_tail", True) else []
    if "tailband" in p:
        tr.append(band("tailband", 4, 1, 0, "z", ("up", "down", "east", "west")))
    if "tailcol" in p:
        tr.append(where("tailcol", lambda q: True))
    tail_chain(m, y0 + bh * 0.45, zb - 0.3, a["tail"], "tail" if "tail" in p else "body", tr, a.get("droop", 0.15))


def snake(m, p, a):
    segs = a["body"]
    z = 0.0
    dec = []
    if "zig" in p:
        dec.append(zigzag("zig", 2))
    if "bands" in p:
        dec.append(band("bands", a.get("period", 4), a.get("bw", 2), 0, "z"))
    if "stripe" in p:
        dec.append(dorsal("stripe", 0.6))
        dec.append(lambda q: "stripe" if q.face in ("east", "west") and q.j == 0 and False else None)
    if "side" in p:
        dec.append(lateral("side", 0))
    if "spot" in p:
        dec.append(spots("spot", a["seed"], 0.35, 2, 1))
    zs = []
    for k, (w, h, l) in enumerate(segs):
        zs.append((z, z + l, w, h))
        z += l
    zlen = z
    # Head forward (-z), body from z = 0 backwards.
    for k, (z0, z1, w, h) in enumerate(zs):
        m.box("body-%d" % k, "Body", (-w / 2, 0, z0), (w / 2, h, z1), "body", ([belly("belly", 0.3)] if "belly" in p else []) + dec)
    hw, hh, hl = a["head"]
    m.bone("Head", (0, hh * 0.5, 0))
    hr = [side_px("eye", 1, 0), pxs("eye", "up", [(0, hl - 2)])] + ([on("belly", "down")] if "belly" in p else []) + ([where("headmark", lambda q: True, ("up",))] if "headmark" in p else [])
    m.box("head", "Head", (-hw / 2, 0, -hl + 0.5), (hw / 2, hh, 0.5), "head" if "head" in p else "body", hr)
    if "tongue" in p:
        m.box("tongue", "Head", (-0.3, 0.1, -hl - 1.6), (0.3, 0.5, -hl + 0.6), "tongue")
    t0 = zlen - 0.3
    tw, th = a["tail"]
    tsegs = []
    n = a.get("tail_n", 5)
    for k in range(n):
        f = 1.0 - k / float(n)
        tsegs.append((max(0.5, tw * f), max(0.5, th * f), a.get("tail_l", 2.2)))
    tail_chain(m, 0.0, t0, tsegs, "body", dec[:1] + ([band("bands", a.get("period", 4), a.get("bw", 2), 0, "z")] if "bands" in p else []))
    for part in m.parts:
        if part.name.startswith("tail-"):
            k = int(part.name.split("-")[1])
            f = 1.0 - k / float(n)
            h = max(0.5, th * f)
            part.lo = (part.lo[0], 0.0, part.lo[2])
            part.hi = (part.hi[0], h, part.hi[2])
    for name, (pv, par) in list(m.bones.items()):
        if name.startswith("tail"):
            m.bones[name] = ((pv[0], 0.4, pv[2]), par)


def turtle(m, p, a):
    bw, bh, bl = a["shell"]
    y0 = a.get("lift", 1.6)
    zf, zb = -bl / 2.0, bl / 2.0
    rim = [where("rim", lambda q: q.ly < 0.35, SIDES)] if "rim" in p else []
    m.box("plastron", "Body", (-bw / 2 + 0.3, y0 - 0.9, zf + 0.3), (bw / 2 - 0.3, y0 + 0.4, zb - 0.3), "under")
    m.box("shell", "Body", (-bw / 2, y0, zf), (bw / 2, y0 + bh, zb), "shell", rim + [grid("scute", 3, ("up",))] + ([spots("fleck", a.get("seed", 3), 0.3, 3, 1, ("up",))] if "fleck" in p else []))
    m.box("shell-top", "Body", (-bw / 2 + 1.3, y0 + bh, zf + 1.4), (bw / 2 - 1.3, y0 + bh + a.get("dome", 1.1), zb - 1.4), "shell", [grid("scute", 3, ("up",))], frame=True)
    hw, hh, hl = a["head"]
    nl = a.get("neck", 2.0)
    m.bone("Head", (0, y0 + 0.8, zf + 0.4))
    hr = [side_px("eye", 1, 1), pxs("eye", "north", [(0, 1)]) if a.get("eye_front") else None] + ([where("cheek", lambda q: q.ly < 0.5, ("east", "west"))] if "cheek" in p else []) + ([on("under", "down")])
    m.box("neck", "Head", (-hw / 2 + 0.3, y0 - 0.3, zf - nl + 0.6), (hw / 2 - 0.3, y0 + hh - 0.6, zf + 0.8), "skin")
    m.box("head", "Head", (-hw / 2, y0 - 0.3, zf - nl - hl + 0.8), (hw / 2, y0 + hh - 0.3, zf - nl + 0.8), "skin", hr)
    if a.get("flippers"):
        fl, ft, fw = a["flippers"]
        for side, sx in (("L", -1), ("R", 1)):
            m.bone("Arm-" + side, (sx * bw / 2, y0 + 0.6, zf + 2.0))
        m.pair("flipper", ("Arm-L", "Arm-R"), (-bw / 2 - fl, y0 + 0.1, zf + 1.0), (-bw / 2 + 0.6, y0 + 0.1 + ft, zf + 1.0 + fw), "skin", [on("under", "down")], rot=(0, 0, 14), origin=(-bw / 2, y0 + 0.6, zf + 2.0))
        for side, sx in (("L", -1), ("R", 1)):
            m.bone("Leg-B" + side, (sx * (bw / 2 - 1.2), y0 + 0.6, zb - 1.0))
        m.pair("hind", ("Leg-BL", "Leg-BR"), (-bw / 2 + 0.4 - 1.8, y0 + 0.1, zb - 2.6), (-bw / 2 + 1.2, y0 + 0.1 + 0.8, zb - 0.6), "skin")
    else:
        for tag, z in (("F", zf + 1.6), ("B", zb - 1.8)):
            for side, sx in (("L", -1), ("R", 1)):
                x = sx * (bw / 2 - 0.9)
                m.bone("Leg-%s%s" % (tag, side), (x, y0 + 0.2, z))
            m.pair("leg-" + tag.lower(), ("Leg-%sL" % tag, "Leg-%sR" % tag), (-bw / 2 + 0.1 - 0.2, 0, z - 0.9), (-bw / 2 + 1.7, y0 + 0.4, z + 0.9), "skin", [on("claw", "north", "down") if "claw" in p else None])
    m.bone("tail", (0, y0 + 0.5, zb - 0.2))
    m.box("tail", "tail", (-0.5, y0 - 0.3, zb - 0.2), (0.5, y0 + 0.5, zb + a.get("tail", 2.0)), "skin")


def tadpole(m, p, a):
    m.box("body", "Body", (-1.6, 1.0, -2.2), (1.6, 3.8, 2.0), "body", [belly("belly", 0.3) if "belly" in p else None, spots("spot", 4, 0.25, 2, 1) if "spot" in p else None, where("eye", lambda q: q.ly > 0.4 and q.lz < 0.5 and (q.i in (0, q.w - 1)), ("north",))])
    m.bone("Head", (0, 2.4, -2.0))
    m.pair("eye", "Head", (-1.7, 3.0, -2.4), (-0.7, 3.9, -1.4), "eye", [pxs("pupil", "north", [(0, 0)])], skip=("down",))
    tail_chain(m, 2.4, 1.8, [(0.9, 2.6, 2.4), (0.8, 2.0, 2.0), (0.7, 1.4, 1.8)], "tail", [])


def crab(m, p, a):
    y0 = 1.6
    bw, bh, bl = a["body"]
    zf, zb = -bl / 2.0, bl / 2.0
    m.box("body", "Body", (-bw / 2, y0, zf), (bw / 2, y0 + bh, zb), "body", [belly("belly", 0.3) if "belly" in p else None, dorsal("mark", 0.6) if "mark" in p else None, where("body2", lambda q: q.lz < 0.12, ("up",)) if "body2" in p else None])
    m.bone("Head", (0, y0 + bh, zf))
    for side, sx in (("l", -1), ("r", 1)):
        x = sx * 1.4
        m.box("stalk-" + side, "Head", (x - 0.35, y0 + bh, zf + 0.3), (x + 0.35, y0 + bh + 1.4, zf + 1.1), "legs")
        m.box("eye-" + side, "Head", (x - 0.6, y0 + bh + 1.2, zf + 0.1), (x + 0.6, y0 + bh + 2.2, zf + 1.2), "eye", [pxs("pupil", "north", [(0, 0)], False)])
    for k, z in enumerate((zf + 1.2, 0.0, zb - 1.2)):
        for side, sx in (("L", -1), ("R", 1)):
            m.bone("Leg-%s%d" % (side, k + 1), (sx * bw / 2, y0 + 0.7, z))
        m.pair("leg%d" % (k + 1), ("Leg-L%d" % (k + 1), "Leg-R%d" % (k + 1)), (-bw / 2 - 2.6, 0, z - 0.4), (-bw / 2 + 0.3, y0 + 1.1, z + 0.4), "legs",
               [where("legs2", lambda q: q.ly < 0.4 or q.face == "down")])
    # One huge claw on the left, one small on the right.
    for side, sx, sc in (("L", -1, a.get("big", 2.6)), ("R", 1, 1.0)):
        m.bone("Arm-" + side, (sx * (bw / 2 - 0.6), y0 + bh * 0.6, zf + 0.6))
    big = a.get("big", 2.6)
    m.box("arm-l", "Arm-L", (-bw / 2 - 1.0, y0 + 0.6, zf - 1.8), (-bw / 2 + 0.8, y0 + 1.6, zf + 0.9), "legs")
    m.box("claw-l", "Arm-L", (-bw / 2 - big, y0 + 0.4, zf - 2.2 - big), (-bw / 2 + 0.4, y0 + 0.4 + big, zf - 1.6), "claw", [where("clawtip", lambda q: q.lz < 0.35, SIDES + ("up", "down"))])
    m.box("arm-r", "Arm-R", (bw / 2 - 0.8, y0 + 0.6, zf - 1.4), (bw / 2 + 0.6, y0 + 1.4, zf + 0.9), "legs")
    m.box("claw-r", "Arm-R", (bw / 2 - 0.5, y0 + 0.5, zf - 2.6), (bw / 2 + 1.1, y0 + 1.7, zf - 1.3), "claw", [where("clawtip", lambda q: q.lz < 0.4)])


# ======================================================================
#  Birds and flying things
# ======================================================================

def bird(m, p, a):
    bw, bh, bl = a["body"]
    lh = a.get("leg", 3.0)
    y0, y1 = lh, lh + bh
    zf, zb = -bl / 2.0, bl / 2.0
    body_r = []
    if "belly" in p:
        body_r.append(belly("belly", a.get("belly_h", 0.4)))
    if "chest" in p:
        body_r.append(where("chest", lambda q: q.lz < 0.4 and q.ly > 0.15, SIDES + ("up",)))
    if "rump" in p:
        body_r.append(where("rump", lambda q: q.lz > 0.8, SIDES + ("up",)))
    if "back" in p:
        body_r.append(where("back", lambda q: q.lz > 0.25 and q.lz < 0.85, ("up",)))
    if "spot" in p:
        body_r.append(spots("spot", a["seed"], a.get("dens", 0.35), 2, 1, ("up", "east", "west")))
    body_r += a.get("body_r", [])
    m.box("body", "Body", (-bw / 2, y0, zf), (bw / 2, y1, zb), "body", body_r)
    hw, hh, hl = a["head"]
    nl, nt = a.get("neck", (0.0, 2.0))
    if nl > 0:
        nz = zf + nt / 2 + a.get("neck_z", 0.2)
        nr = [where("collar", lambda q: q.ly < 0.3, SIDES)] if "collar" in p else []
        m.box("neck", "Body", (-nt / 2, y1 - 1, nz - nt / 2), (nt / 2, y1 + nl, nz + nt / 2), "neck" if "neck" in p else "body", nr)
        hz = nz
    else:
        hz = zf + hl / 2 + a.get("head_z", 0.6)
    hcy = y1 + nl + hh / 2 - a.get("head_dy", 0.5)
    m.bone("Head", (0, hcy, hz))
    hr = []
    if "disc" in p:
        hr.append(where("disc", lambda q: q.i > 0 and q.i < q.w - 1 and q.j > 0, ("north",)))
    if "cheek" in p:
        hr.append(where("cheek", lambda q: q.ly < 0.5 and q.lz < 0.75, ("east", "west")))
    if "cap" in p:
        hr.append(where("cap", lambda q: q.ly > 0.6, ("up", "north", "east", "west")))
    if "eyestripe" in p:
        hr.append(where("eyestripe", lambda q: q.ly > 0.3 and q.ly < 0.7 and q.lz < 0.8, ("east", "west")))
    if a.get("eye_front"):
        hr.append(px("eye", "north", *a["eye_front"]))
    else:
        ek, ej = a.get("eye", (1, 1))
        hr.append(side_px("eye", ek, ej))
    hr += a.get("head_r", [])
    m.box("head", "Head", (-hw / 2, hcy - hh / 2, hz - hl * 0.5), (hw / 2, hcy + hh / 2, hz + hl * 0.5), "head" if "head" in p else "body", hr)
    fz = hz - hl * 0.5
    if a.get("bill"):
        bwd, bht, bln = a["bill"]
        br = []
        if "billtip" in p:
            br.append(where("billtip", lambda q: q.lz < 0.3 or (q.face == "north")))
        if "billspot" in p:
            br.append(px("billspot", "down", 0, 0, False) if False else where("billspot", lambda q: q.face in ("down",) and q.lz < 0.5))
        m.box("bill", "Head", (-bwd / 2, hcy - bht / 2 - a.get("bill_dy", 0.4), fz - bln), (bwd / 2, hcy + bht / 2 - a.get("bill_dy", 0.4), fz + 0.4), "bill", br)
    if a.get("ruff"):
        pass
    wl, wh = a["wing"]
    wz = zf + bl * 0.12
    m.bone("wing-l", (-bw / 2, y1 - 0.4, wz))
    m.bone("wing-r", (bw / 2, y1 - 0.4, wz))
    wr = [where("wingtip", lambda q: q.lz > 0.7, SIDES + ("up", "down")) if "wingtip" in p else None,
          where("spec", lambda q: 0.3 < q.lz < 0.6 and q.ly < 0.6 and q.ly > 0.2, ("east", "west")) if "spec" in p else None,
          spots("wspot", a["seed"] + 3, 0.3, 2, 1, ("east", "west")) if "wspot" in p else None]
    m.pair("wing", ("wing-l", "wing-r"), (-bw / 2 - 0.9, y1 - wh, wz), (-bw / 2 + 0.3, y1 - 0.2, wz + wl), "wing", wr)
    tw, tl = a["tail"]
    m.bone("tail", (0, y1 - 0.6, zb - 0.4))
    m.box("tail", "tail", (-tw / 2, y1 - a.get("tail_h", 1.6) - 0.2 + a.get("tail_dy", 0), zb - 0.4), (tw / 2, y1 - 0.2 + a.get("tail_dy", 0), zb + tl), "tail" if "tail" in p else "wing",
          [where("tailtip", lambda q: q.lz > 0.7) if "tailtip" in p else None], rot=(a.get("tail_rot", 0), 0, 0) if a.get("tail_rot") else None, origin=(0, y1 - 0.6, zb - 0.4))
    if lh > 0.5:
        fw = a.get("foot_w", 1.8)
        for side, sx in (("L", -1), ("R", 1)):
            x = sx * min(bw / 2 - 0.9, 1.6)
            m.bone("Leg-" + side, (x, lh + 0.4, zf + bl * 0.5))
            m.box("leg-" + side.lower(), "Leg-" + side, (x - 0.4, 0.4, zf + bl * 0.5 - 0.4), (x + 0.4, lh + 0.5, zf + bl * 0.5 + 0.4), "foot")
            m.box("foot-" + side.lower(), "Leg-" + side, (x - fw / 2, 0, zf + bl * 0.5 - 2.2), (x + fw / 2, 0.5, zf + bl * 0.5 + 0.5), "foot")


def bat(m, p, a):
    m.box("body", "Body", (-1.6, 1.0, -2.5), (1.6, 4.4, 2.5), "body", [belly("belly", 0.35) if "belly" in p else None])
    m.bone("Head", (0, 3.4, -2.2))
    m.box("head", "Head", (-1.8, 2.6, -5.0), (1.8, 5.4, -1.6), "head" if "head" in p else "body",
          [px("eye", "north", 0, 1), nose("nose", 2), where("leaf", lambda q: q.j == 1 and 0 < q.i < q.w - 1, ("north",)) if "leaf" in p else None])
    m.bone("ear-l", (-1.2, 5.2, -3.0), "Head")
    m.bone("ear-r", (1.2, 5.2, -3.0), "Head")
    m.pair("ear", ("ear-l", "ear-r"), (-2.2, 5.2, -3.4), (-0.6, 8.2, -2.6), "ear" if "ear" in p else "body",
           [lambda q: "earin" if "earin" in p and q.face == "north" and 0 < q.i < q.w - 1 and q.j > 0 else None])
    m.bone("wing-l", (-1.5, 3.8, -0.8))
    m.bone("wing-r", (1.5, 3.8, -0.8))

    def inside(u, v):
        return 0 <= u < 1 and v >= 0 and v <= 0.30 + 0.70 * (1 - u) ** 0.8 and not (u > 0.55 and v > 0.45 + 0.4 * (1 - u) and False)

    def wing(q):
        u = (abs(q.x) - 1.5) / 8.5
        v = q.lz
        if not inside(u, v):
            return "clear"
        ii, jj = int(u * q.w), int(v * q.h)
        if q.j == 0 or (q.i % 4 == 3 and q.j % 2 == 0 and u > 0.1 and False):
            return "bone"
        if q.i % 3 == 2 and q.j > 0:
            return "bone2"
        return None
    m.pair("wing", ("wing-l", "wing-r"), (-10.0, 3.8, -2.6), (-1.5, 4.2, 4.2), "membrane", [wing], skip=("south", "north", "east", "west"), frame=False)
    m.bone("tail", (0, 2.2, 2.5))
    m.box("tail", "tail", (-1.0, 1.6, 2.4), (1.0, 2.4, 5.0), "membrane" if "tailweb" in p else "body")


def dragonfly(m, p, a):
    m.box("thorax", "Body", (-1.3, 0, -1.8), (1.3, 2.4, 1.8), "thorax", [band("stripe", 3, 1, 0, "z", ("east", "west")) if "stripe" in p else None])
    m.bone("Head", (0, 1.2, -1.8))
    m.box("head", "Head", (-1.6, 0.2, -3.6), (1.6, 2.4, -1.6), "eye", [on("head", "down")])
    zs = 1.6
    segs = [(1.0, 1.0, 3.2), (0.9, 0.9, 3.2), (0.8, 0.8, 3.2)]
    tail_chain(m, 1.9, zs, segs, "body", [band("band", 2, 1, 0, "z", ("up", "east", "west")) if "band" in p else None])
    m.bone("wing-l", (-0.8, 2.3, 0.0))
    m.bone("wing-r", (0.8, 2.3, 0.0))
    for nm, z0, z1 in (("fore", -2.0, -0.2), ("hind", 0.2, 2.0)):
        def wing(q, z0=z0):
            if q.j == 0 or q.i == 0:
                return "vein"
            if q.i >= q.w - 3 and q.j == 1 and z0 < 0:
                return "stigma" if "stigma" in p else None
            return None
        m.pair("wing-" + nm, ("wing-l", "wing-r"), (-9.2, 2.3, z0), (-0.8, 2.6, z1), "wing", [wing], skip=("south", "north", "east", "west"), frame=False)


def butterfly(m, p, a):
    pat = p["pat"]
    m.box("body", "Body", (-0.6, 0, -2.5), (0.6, 1.4, 2.5), "body")
    m.bone("Head", (0, 0.8, -2.5))
    m.box("head", "Head", (-0.8, 0.1, -3.9), (0.8, 1.5, -2.4), "body", [px("eye", "north", 0, 0) if "eye" in p else None])
    m.pair("antenna", "Head", (-0.9, 1.3, -6.0), (-0.4, 1.6, -3.7), "body", rot=(25, 18, 0), origin=(-0.65, 1.4, -3.7))

    def inside(fore, u, v):
        if not (0 <= u < 1 and 0 <= v < 1):
            return False
        if fore:
            return v < 1.0 - 0.58 * u * u - 0.1 * u
        return ((u - 0.35) / 0.65) ** 2 + ((v - 0.45) / 0.55) ** 2 < 1.0 or (u < 0.3 and v < 0.7)

    def wing(fore, x0, span):
        def f(q):
            u, v = (abs(q.x) - x0) / span, q.lz
            if not inside(fore, u, v):
                return "clear"
            eu, ev = 1.05 / q.w, 1.05 / q.h
            edge = not inside(fore, u + eu, v) or not inside(fore, u, v + ev) or (fore and v < ev * 0.9)
            ii, jj = q.i, q.j
            if pat == "monarch":
                if edge:
                    return "dot" if (ii + jj) % 2 == 0 and u > 0.35 else "edge"
                if (ii % 3 == 1 and jj % 2 == 0) or jj == 0:
                    return "vein"
                return None
            if pat == "swallow":
                if edge:
                    return "edge"
                if jj % 3 == 1 and u > 0.25:
                    return "vein"
                if not fore and u > 0.5 and v > 0.55:
                    return "dot"
                return None
            if pat == "morpho":
                if edge or u > 0.86:
                    return "edge"
                if (u > 0.5 and (ii + jj) % 4 == 0):
                    return "dot"
                return None
            if pat == "white":
                if edge:
                    return None
                if fore and u > 0.68 and v < 0.45:
                    return "edge"
                if (ii, jj) in ((int(0.4 * q.w), int(0.35 * q.h)), (int(0.5 * q.w), int(0.6 * q.h))) and fore:
                    return "dot"
                return None
            if pat == "emperor":
                if edge:
                    return "edge"
                if 0.42 < u < 0.62 and jj % 2 == 0:
                    return "dot"
                if jj == 0:
                    return "vein"
                return None
            if pat == "ladybird":
                return None
            return None
        return f
    m.bone("wing-l", (-0.5, 1.2, 0.0))
    m.bone("wing-r", (0.5, 1.2, 0.0))
    m.pair("wing-fore", ("wing-l", "wing-r"), (-9.5, 1.2, -5.5), (-0.5, 1.6, 0.3), "wing", [wing(True, 0.5, 9.0)], rot=(0, 0, -34), origin=(-0.5, 1.2, 0), skip=("north", "south", "east", "west"), frame=False)
    m.pair("wing-hind", ("wing-l", "wing-r"), (-8.0, 1.22, -0.2), (-0.5, 1.58, 5.4), "wing", [wing(False, 0.5, 7.5)], rot=(0, 0, -34), origin=(-0.5, 1.2, 0), skip=("north", "south", "east", "west"), frame=False)


def bee(m, p, a):
    m.box("abdomen", "Body", (-1.9, 0.6, 0.4), (1.9, 4.2, 4.8), "body", [band("dark", 3, 1, 0, "z", ("up", "down", "east", "west")), where("tip", lambda q: q.lz > 0.8, SIDES + ("up", "down")) if "tip" in p else None, on("belly", "down") if "belly" in p else None])
    m.box("thorax", "Body", (-2.0, 0.8, -2.2), (2.0, 4.4, 0.8), "thorax", [band("dark", 99, 99) if False else None, where("dark", lambda q: q.face == "up" and q.lz > 0.55 and abs(q.x) < 1.2)])
    m.bone("Head", (0, 2.6, -2.2))
    m.box("head", "Head", (-1.6, 1.4, -4.2), (1.6, 3.8, -1.9), "head", [px("eye", "north", 0, 1), px("eye", "north", 0, 0) if False else None, where("dark", lambda q: q.j == q.h - 1, ("north",))])
    m.box("sting", "Body", (-0.3, 1.8, 4.8), (0.3, 2.4, 5.5), "dark")
    m.bone("wing-l", (-1.2, 4.3, -1.0))
    m.bone("wing-r", (1.2, 4.3, -1.0))
    m.pair("wing", ("wing-l", "wing-r"), (-5.2, 4.3, -1.8), (-1.2, 4.6, 1.8), "wing", [lambda q: "vein" if q.j == 0 else None], skip=("north", "south", "east", "west"), frame=False)
    for k, z in enumerate((-1.4, 0.0)):
        for side, sx in (("L", -1), ("R", 1)):
            m.bone("Leg-%s%d" % (side, k + 1), (sx * 1.2, 1.0, z))
        m.pair("leg%d" % (k + 1), ("Leg-L%d" % (k + 1), "Leg-R%d" % (k + 1)), (-1.8, 0, z - 0.3), (-1.2, 1.0, z + 0.3), "dark")


def ladybird(m, p, a):
    m.box("shell", "Body", (-2.5, 0.8, -2.0), (2.5, 2.6, 2.4), "shell", [spots("dot", a["seed"], 0.45, 2, 1, ("up", "east", "west")), where("dark", lambda q: q.face == "up" and abs(q.x) < 0.6, ("up",))])
    m.box("under", "Body", (-2.1, 0.2, -1.6), (2.1, 0.8, 2.0), "dark")
    m.bone("Head", (0, 1.4, -2.0))
    m.box("head", "Head", (-1.2, 0.6, -3.4), (1.2, 2.0, -1.8), "head", [px("eye", "north", 0, 0, True) if False else pxs("white", "north", [(0, 0)])])
    for k, z in enumerate((-1.2, 0.2, 1.4)):
        for side, sx in (("L", -1), ("R", 1)):
            m.bone("Leg-%s%d" % (side, k + 1), (sx * 1.8, 0.8, z))
        m.pair("leg%d" % (k + 1), ("Leg-L%d" % (k + 1), "Leg-R%d" % (k + 1)), (-3.0, 0, z - 0.3), (-1.7, 0.7, z + 0.3), "dark")


def boomer_marks(p, seed):
    """The markings of a boomer, by the palette's `pat`: what it carries to blend into its ground."""
    pat = p.get("pat", "patch")
    every = ("up", "east", "west", "north", "south")
    if pat == "spines":       # a cactus: rows of spines on sand
        return [where("mark", lambda q: q.i % 3 == 1 and q.j % 3 == 1, every)]
    if pat == "frost":        # snow: a frosted top and a few flakes
        return [topc("mark", 0.3), spots("mark", seed, 0.2, 3, 1, every)]
    if pat == "moss":         # a swamp: moss on the lower half and in clumps
        return [where("mark", lambda q: q.face in SIDES and q.ly < 0.3), spots("mark2", seed, 0.35, 3, 2, every)]
    if pat == "bands":        # badlands: stripes of clay
        return [band("mark", 4, 1, 0, "y", SIDES), band("mark2", 4, 1, 2, "y", SIDES)]
    if pat == "ore":          # mountain and crystal: specks in the rock
        return [spots("mark", seed, 0.3, 3, 1, every), spots("mark2", seed + 5, 0.2, 4, 1, every)]
    if pat == "flowers":      # jungle: blossoms
        return [spots("mark", seed, 0.3, 3, 1, every), spots("mark2", seed + 9, 0.3, 3, 1, every)]
    if pat == "glow":         # a cave: glowing spots
        return [spots("mark", seed, 0.35, 3, 1, every), spots("mark2", seed + 2, 0.2, 3, 2, every)]
    if pat == "embers":       # deep down: cracks with embers
        return [where("mark", lambda q: q.face in SIDES and (q.i + 2 * q.j) % 7 == 0), spots("mark2", seed, 0.2, 3, 1, every)]
    return [spots("mark", seed, 0.4, 3, 2, every), spots("mark2", seed + 7, 0.25, 3, 1, every)]   # bark patches


def boomer(m, p, a):
    """A walking gourd with a fuse for a stem and a grim face: it blows up when you come close."""
    m.box("body", "Body", (-4, 3, -3.5), (4, 9, 3.5), "body", boomer_marks(p, a["seed"]))
    m.bone("Head", (0, 9, 0))
    face = [pxs("eye", "north", [(1, 1), (2, 1), (1, 2), (2, 2)]), pxs("mouth", "north", [(2, 3), (3, 3), (1, 4)])]
    m.box("head", "Head", (-3.5, 9, -3.5), (3.5, 15, 3.5), "head", boomer_marks(p, a["seed"] + 1) + face)
    m.box("stem", "Head", (-0.8, 15, -0.8), (0.8, 18, 0.8), "stem")
    m.box("leaf", "Head", (0.8, 16.5, -0.8), (3.8, 17.5, 0.8), "stem")
    legs4(m, -2.0, 2.0, 3.0, 2.2, 2.6, 2.6, "leg", "leg")


# ======================================================================
#  The species
# ======================================================================
# Each builder takes the model and its palette. Colour slots a builder looks
# for with `in p` are optional: leave one out of a species' palette and that
# marking is simply not drawn.

def b_tree_frog(m, p):
    frog(m, p, dict(body=(4.6, 3.4, 5.6), head=(4.8, 2.6, 3.2), eye=1.7, seed=11, foot=3.8, thigh=(1.5, 3.0, 3.0)))


def b_dart_frog(m, p):
    frog(m, p, dict(body=(3.8, 2.8, 4.6), head=(4.0, 2.2, 2.8), eye=1.6, seed=12, dens=0.4, cell=3, ssize=2, foot=3.4, thigh=(1.2, 2.4, 2.6)))


def b_pond_frog(m, p):
    frog(m, p, dict(body=(5.8, 4.0, 7.0), head=(6.0, 3.0, 3.8), eye=2.0, seed=13, dens=0.4, ssize=2, foot=5.6, thigh=(2.0, 3.6, 4.0)))


def b_toad(m, p):
    frog(m, p, dict(body=(6.4, 4.4, 6.2), head=(6.6, 3.2, 3.6), eye=1.8, seed=14, dens=0.5, foot=3.2, thigh=(1.8, 2.6, 3.0), lift=1.0))


def b_tadpole(m, p):
    tadpole(m, p, {})


def b_newt(m, p):
    lizard(m, p, dict(body=(2.2, 1.9, 5.4), head=(2.4, 1.7, 2.4), seed=21, legl=1.1, legt=0.9, lift=1.1, tail=[(0.8, 2.2, 3.4), (0.7, 2.0, 3.2), (0.6, 1.6, 2.8), (0.5, 1.2, 2.2)], dens=0.4,
                      crest=(0.5, 1.2) if "crest" in p else None, belly_h=0.45, droop=0.0))


def b_pond_turtle(m, p):
    turtle(m, p, dict(shell=(6.6, 2.6, 8.4), head=(2.6, 2.2, 2.4), neck=2.2, seed=31))


def b_sea_turtle(m, p):
    turtle(m, p, dict(shell=(8.0, 2.8, 10.0), head=(3.0, 2.6, 3.0), neck=2.4, flippers=(5.6, 0.7, 3.2), dome=1.2, seed=32, lift=1.8))


def b_gecko(m, p):
    lizard(m, p, dict(body=(2.8, 2.0, 6.0), head=(3.0, 1.8, 3.2), seed=41, legl=1.6, legt=1.0, lift=1.2, dens=0.35, ssize=1,
                      tail=[(1.6, 1.6, 3.0), (1.3, 1.3, 3.0), (1.0, 1.0, 2.8), (0.7, 0.7, 2.4)], droop=0.0))


def b_lizard(m, p):
    lizard(m, p, dict(body=(3.4, 2.6, 7.0), head=(3.0, 2.2, 3.4), seed=42, legl=2.0, legt=1.2, lift=1.8, dens=0.3,
                      tail=[(2.2, 2.0, 3.6), (1.8, 1.7, 3.6), (1.4, 1.4, 3.4), (1.0, 1.0, 3.0), (0.7, 0.7, 2.6)], droop=0.1))


def b_garter_snake(m, p):
    snake(m, p, dict(body=[(2.0, 1.8, 3.0), (2.2, 2.0, 3.0), (2.2, 2.0, 3.0), (2.0, 1.8, 3.0)], head=(2.2, 1.6, 3.0), tail=(1.8, 1.6), tail_n=5, tail_l=2.4, seed=51, period=4, bw=2))


def b_viper(m, p):
    snake(m, p, dict(body=[(2.6, 2.0, 3.0), (2.8, 2.2, 3.0), (2.8, 2.2, 3.0), (2.4, 1.8, 2.5)], head=(3.4, 1.6, 3.4), tail=(2.0, 1.6), tail_n=4, tail_l=2.0, seed=52, period=4, bw=2))


def b_barn_owl(m, p):
    bird(m, p, dict(body=(5.0, 7.0, 4.6), leg=1.6, head=(6.2, 4.6, 4.6), neck=(0.0, 2.0), wing=(5.0, 6.2), tail=(3.4, 3.0), tail_h=1.0, tail_rot=18, eye_front=(1, 2), seed=61,
                    bill=(0.9, 1.2, 1.0), foot_w=1.6, head_dy=0.2, head_z=0.0, belly_h=0.6))


def b_mallard(m, p):
    bird(m, p, dict(body=(5.0, 3.6, 8.0), leg=2.2, head=(3.0, 2.8, 3.4), neck=(2.2, 2.2), wing=(5.4, 3.0), tail=(2.6, 2.0), tail_rot=14, tail_h=1.0, bill=(2.0, 0.9, 2.6), eye=(1, 1), seed=62, foot_w=2.4,
                    neck_z=0.4, head_dy=0.2, belly_h=0.3))


def b_canada_goose(m, p):
    bird(m, p, dict(body=(6.0, 4.6, 10.0), leg=3.4, head=(3.0, 2.8, 4.2), neck=(5.8, 2.2), wing=(7.0, 3.8), tail=(3.2, 2.4), tail_rot=8, tail_h=1.0, bill=(1.8, 1.0, 2.2), eye=(1, 1), seed=63, foot_w=2.6,
                    neck_z=0.6, head_dy=0.2, belly_h=0.35))


def b_seagull(m, p):
    bird(m, p, dict(body=(4.6, 3.6, 8.0), leg=3.4, head=(3.2, 3.0, 3.6), neck=(1.2, 2.4), wing=(7.0, 2.8), tail=(3.2, 3.0), tail_rot=6, tail_h=1.0, bill=(1.2, 1.0, 3.0), eye=(1, 1), seed=64, foot_w=1.8,
                    neck_z=0.6, head_dy=0.3, belly_h=0.4))


def b_horseshoe_bat(m, p):
    bat(m, p, {})


def b_dragonfly(m, p):
    dragonfly(m, p, {})


def b_meadow_butterfly(m, p):
    butterfly(m, p, {})


def b_bumblebee(m, p):
    bee(m, p, {})


def b_ladybird(m, p):
    ladybird(m, p, dict(seed=71))


def b_fiddler_crab(m, p):
    crab(m, p, dict(body=(6.4, 2.4, 4.4), big=2.8))


# -- mammals

def b_roe_deer(m, p):
    def extras(G):
        if "antler" in p:
            hcy, fz, hw = G["hcy"], G["fz"], G["hw"]
            ax = hw / 2 - 0.9
            m.pair("antler", "Head", (-ax - 0.35, hcy + 1.5, fz - 3.6), (-ax + 0.35, hcy + 1.5 + float(p.get("tall", 4.2)), fz - 2.9), "antler", rot=(-8, 0, -10), origin=(-ax, hcy + 1.5, fz - 3.2), frame=False)
            m.pair("tine", "Head", (-ax - 0.35, hcy + 3.2, fz - 4.9), (-ax + 0.35, hcy + 3.2 + 2.0, fz - 3.2), "antler", rot=(35, 0, -10), origin=(-ax, hcy + 3.2, fz - 3.2), frame=False)
    G = mammal(m, p, dict(body=(5.0, 5.6, 12.0), leg=9.5, legsz=(1.6, 1.6), neck=(6.0, 2.8, 24), head=(3.2, 3.4, 5.0), snout=(2.0, 2.0, 2.8), ears=(2.6, 1.4, 0.8), ear_tilt=20, ear_in=0.0, ear_out=2.0,
                          tail=(1.6, 2.4, 1.6), tail_dy=-0.5, seed=81, dens=0.4, ssize=1, knee=0.55, belly_h=0.3, fz=0.5, bz=0.5, eye=(2, 1), head_h=0.7,
                          body_r=[where("rumpw", lambda q: q.lz > 0.9, ("south",)) if "rumpw" in p else None]))
    extras(G)


def b_arctic_fox(m, p):
    mammal(m, p, dict(body=(4.4, 4.2, 8.4), leg=5.0, legsz=(1.5, 1.5), head=(3.8, 3.4, 3.4), snout=(2.0, 1.6, 2.4), ears=(1.8, 2.4, 0.8), ear_in=0.2, tail=(3.0, 3.0, 7.0), tail_dy=-0.2, tail_rot=-8, seed=82, belly_h=0.3, head_h=0.65, eye=(1, 1)))


def b_fennec_fox(m, p):
    mammal(m, p, dict(body=(3.4, 3.4, 7.0), leg=4.8, legsz=(1.2, 1.2), head=(3.4, 3.0, 3.0), snout=(1.6, 1.4, 2.0), ears=(2.2, 6.4, 0.7), ear_in=0.0, ear_tilt=8, tail=(2.0, 2.0, 6.0), tail_dy=-0.2, tail_rot=-8, seed=83, belly_h=0.3, head_h=0.65, eye=(1, 1)))


def b_cottontail(m, p):
    mammal(m, p, dict(body=(4.2, 4.4, 6.4), leg=2.4, legsz=(1.1, 1.4), legsz_b=(1.7, 3.6), head=(3.4, 3.2, 3.6), snout=(1.9, 1.5, 1.2), ears=(1.3, 5.6, 0.8), ear_in=0.3, ear_tilt=6, tail_box=((-1.2, 3.4, 3.0), (1.2, 5.6, 5.0), None),
                      seed=84, belly_h=0.3, head_h=0.7, eye=(1, 1), knee=0.5, bz=0.2))


def b_hedgehog(m, p):
    G = mammal(m, p, dict(body=(5.0, 3.4, 7.0), leg=1.6, legsz=(1.1, 1.1), head=(3.2, 2.8, 2.6), snout=(1.6, 1.4, 2.4), ears=(1.2, 1.0, 0.8), ear_in=0.1, seed=85, belly_h=0.45, head_h=0.45, eye=(1, 1), fz=0.6, bz=0.6))
    y1, zf, zb = G["y1"], G["zf"], G["zb"]
    m.box("quills", "Body", (-3.0, 2.4, zf + 1.5), (3.0, y1 + 2.0, zb + 0.6), "quill", [band("quill2", 2, 1, 0, "z", ("up", "east", "west", "south")), where("quill2", lambda q: q.face == "up" and (q.i + q.j) % 3 == 0)])


def b_otter(m, p):
    mammal(m, p, dict(body=(3.8, 3.6, 9.0), leg=2.0, legsz=(1.5, 1.7), head=(3.6, 2.8, 3.4), snout=(2.0, 1.5, 1.2), ears=(0.9, 0.9, 0.7), ear_in=0.1, seed=86, belly_h=0.35, head_h=0.6, eye=(1, 0), fz=0.5, bz=0.5, knee=0.5))
    # a long thick tail, in pieces
    tail_chain(m, 4.2, 4.2, [(2.8, 2.6, 3.6), (2.0, 2.0, 3.2), (1.2, 1.2, 2.6)], "tail" if "tail" in p else "body", [])


def b_beaver(m, p):
    mammal(m, p, dict(body=(6.0, 5.2, 9.0), leg=2.4, legsz=(1.8, 1.7), head=(4.2, 3.6, 3.6), snout=(2.4, 1.8, 1.4), ears=(1.3, 1.2, 0.8), ear_in=0.2, seed=87, belly_h=0.3, head_h=0.6, eye=(1, 1), fz=0.4, bz=0.4,
                      snout_r=[pxs("teeth", "north", [(0, 1)] ) if False else where("teeth", lambda q: q.face == "north" and q.j >= q.h - 1 and abs(q.i - (q.w - 1) / 2.0) < 0.9)],
                      tail=(5.0, 0.9, 7.0), tail_dy=-3.6, tail_r=[checks("scale", ("up", "down"))]))


def b_warthog(m, p):
    G = mammal(m, p, dict(body=(5.8, 6.0, 11.0), leg=5.8, legsz=(2.0, 2.0), head=(4.8, 5.0, 5.4), snout=(3.2, 3.0, 3.0), snout_dy=0.2, ears=(1.8, 2.6, 0.8), ear_in=0.0, ear_tilt=16, tail=(0.8, 4.4, 0.8), tail_dy=-1.0,
                          seed=88, belly_h=0.3, head_h=0.55, eye=(2, 1), body_r=[]))
    y1, zf, zb, hcy, fz = G["y1"], G["zf"], G["zb"], G["hcy"], G["fz"]
    m.box("mane", "Body", (-0.6, y1, zf + 1), (0.6, y1 + 2.4, zb - 1.5), "mane", [where("manetip", lambda q: q.ly > 0.7)] if False else [])
    m.pair("tusk", "Head", (-2.2, hcy - 2.2, fz - 8.2), (-1.5, hcy - 0.2, fz - 6.9), "tusk", rot=(0, 0, 10), origin=(-1.8, hcy - 2.2, fz - 7.5), frame=False)
    m.pair("wartp", "Head", (-3.0, hcy - 0.6, fz - 5.0), (-2.4, hcy + 1.0, fz - 3.6), "wart", frame=False)


def b_boomer(m, p):
    boomer(m, p, dict(seed=101))


def b_cave_boomer(m, p):
    boomer(m, p, dict(seed=111))


def b_squirrel(m, p):
    mammal(m, p, dict(body=(3.2, 3.2, 5.6), leg=2.6, legsz=(1.1, 1.3), legsz_b=(1.4, 2.4), head=(3.0, 2.8, 2.8), snout=(1.4, 1.2, 1.2), ears=(1.0, 1.8, 0.6), ear_in=0.3, seed=89, belly_h=0.35, head_h=0.7, eye=(1, 0),
                      tail_box=((-1.4, 3.4, 1.9), (1.4, 10.6, 4.1), (-20, 0, 0)), knee=0.5, bz=0.2))


def b_badger(m, p):
    mammal(m, p, dict(body=(6.0, 5.4, 10.0), leg=3.2, legsz=(2.0, 1.8), head=(4.0, 3.4, 4.4), snout=(2.2, 1.8, 2.4), ears=(1.4, 1.4, 0.8), ear_in=0.3, tail=(2.0, 2.0, 2.6), tail_dy=-0.6, seed=90, belly_h=0.3, head_h=0.55, eye=(1, 1),
                      head_r=[where("stripe", lambda q: q.j in (1, 2) and q.lz < 0.9, ("east", "west")), where("stripe", lambda q: abs(q.x) < 0.7, ("up",)), where("stripe", lambda q: q.i in (1, q.w - 2) and q.j > 0, ("north",))] if "stripe" in p else []))


# ======================================================================
#  Palettes: base colours of each species, and what each variant changes
# ======================================================================
# Colours are hex. Slots a builder does not find are simply not drawn, so a
# palette only lists the markings its species has.

PALETTE = {
    "tree_frog": dict(base=dict(body="5cb23c", belly="e8f0b8", head="5cb23c", flank="f4e04a", eye="e8c23a", pupil="1c2a14", leg="4d9a34", pad="b8e060"),
                      variants=[("Blue", dict(body="3c8ed8", head="3c8ed8", leg="3077c0", pad="9ed8f0", flank="f0f4ff", belly="e8f0f8")),
                                ("Red_Eyed", dict(eye="d83a2c", flank="3c7ed8", leg="e8802c", pad="f0b040")),
                                ("Grey", dict(body="8a9484", head="8a9484", leg="768070", flank="d8d8c0", belly="e0e0d0", pad="b8c0a8", eye="c8b060"))]),
    "dart_frog": dict(base=dict(body="2a6ed8", belly="2a6ed8", head="2a6ed8", spot="161820", eye="161820", pupil="e8e8f0", leg="2a6ed8", pad="161820"),
                      variants=[("Strawberry", dict(body="e03a30", belly="e03a30", head="e03a30", leg="2a52c0", pad="161820", spot="161820")),
                                ("Golden", dict(body="f2c424", belly="f2c424", head="f2c424", leg="f2c424", spot="d89a18", pad="d89a18", eye="161820")),
                                ("Green_And_Black", dict(body="161820", belly="161820", head="161820", leg="161820", spot="5ce040", pad="5ce040", eye="3a3c48"))]),
    "pond_frog": dict(base=dict(body="6aa840", belly="e8e4b0", head="6aa840", spot="3a6a28", eye="e0c040", pupil="181c10", leg="5a9438", pad="6aa840", dorsal="b8d860", bar="3a6a28"),
                      variants=[("Brown", dict(body="9c7448", head="9c7448", leg="86603c", spot="5c4028", dorsal="c8a468", belly="eadcb8", eye="d8a830")),
                                ("Olive", dict(body="8a9440", head="8a9440", leg="788238", spot="4c5420", dorsal="c8cc68", belly="e8e8b8"))]),
    "toad": dict(base=dict(body="8c6a44", belly="d8c498", head="8c6a44", wart="5c4228", eye="d89a30", pupil="181410", leg="7c5c3a", pad="8c6a44"),
                 variants=[("Green", dict(body="7a8a48", head="7a8a48", leg="6a7a3c", wart="4a5a28", belly="d8d8a8")),
                           ("Red", dict(body="b0603c", head="b0603c", leg="9c5232", wart="743820", belly="e0c098")),
                           ("Natterjack", dict(body="a89060", head="a89060", leg="948050", wart="6c5a34", dorsal="e8d048", belly="e0d0a0"))]),
    "tadpole": dict(base=dict(body="2c3228", belly="5c6450", eye="2c3228", pupil="e0e0d0", tail="3c4434"),
                    variants=[("Olive", dict(body="5a5c28", belly="8c8c50", tail="6c7034", eye="5a5c28")),
                              ("Spotted", dict(body="3a3c34", spot="b8b898", belly="5c6450", tail="4c5240"))]),
    "newt": dict(base=dict(body="6a7438", belly="f08c2c", head="6a7438", spot="3c4220", eye="141410", leg="5c6630", tail="6a7438"),
                 variants=[("Fire", dict(body="1c1c20", head="1c1c20", leg="1c1c20", tail="1c1c20", spot="f0c020", belly="f0c020")),
                           ("Crested", dict(body="2c2a24", head="2c2a24", leg="2c2a24", tail="2c2a24", spot="504c40", belly="f0782c", crest="3c3a30")),
                           ("Alpine", dict(body="48607c", head="48607c", leg="3c526c", tail="48607c", spot="304460", belly="f09030"))]),
    "pond_turtle": dict(base=dict(shell="4a5c2a", scute="2c3c1c", under="e8d890", rim="e8c840", skin="5c6c34", eye="141410", cheek="e8c840", fleck="707c38"),
                        variants=[("Painted", dict(shell="2c3428", scute="1c2418", rim="e03c2c", skin="2c3428", cheek="f0d838")),
                                  ("Snapping", dict(shell="5c4a34", scute="3a2c1c", rim="5c4a34", skin="6c6048", cheek="6c6048", under="b8a478")),
                                  ("Box", dict(shell="6c4a2c", scute="3a2814", rim="d8a838", skin="8c6c3c", cheek="d8a838", under="c8a860", fleck="d89030"))]),
    "sea_turtle": dict(base=dict(shell="6a7c34", scute="40501c", under="e8e0a8", skin="788040", eye="141410"),
                       variants=[("Hawksbill", dict(shell="b87428", scute="5c3010", skin="7a6a3c", under="e8d890")),
                                 ("Loggerhead", dict(shell="a05a30", scute="5c2c14", skin="c8a050", under="f0e0a0")),
                                 ("Leatherback", dict(shell="2c3438", scute="1c2428", skin="3c4448", under="a8b0b4", fleck="f0f0f0"))]),
    "gecko": dict(base=dict(body="d8b868", belly="f0e4b8", head="d8b868", spot="4c3a24", eye="e8d038", leg="c8a858", tail="d8b868", tailband="4c3a24", pad="f0e4b8"),
                  variants=[("Green", dict(body="58b840", head="58b840", leg="4ca438", tail="58b840", spot="e84a30", tailband="58b840", belly="d8f0a8", pad="d8f0a8")),
                            ("Banded", dict(body="c89058", head="c89058", leg="b88048", tail="c89058", spot="f4e8c8", tailband="5c3c20", belly="f0dcc0", pad="f0dcc0")),
                            ("Blue", dict(body="4a8ed0", head="4a8ed0", leg="3c7ab8", tail="4a8ed0", spot="1c3c68", tailband="4a8ed0", belly="d0e4f4", pad="d0e4f4"))]),
    "lizard": dict(base=dict(body="c8a068", belly="f0e0b0", head="c8a068", spot="7c5c34", eye="141410", leg="b89058", tail="c8a068", dorsal="e8d090", tailband="8c6c40"),
                   variants=[("Green", dict(body="5ca03c", head="5ca03c", leg="4c8c34", tail="5ca03c", spot="2c5c20", dorsal="b8d868", belly="e0f0b0", tailband="3c7028")),
                             ("Blue_Tailed", dict(body="6c4c34", head="6c4c34", leg="5c3e2a", tail="38a0e0", spot="3c281c", dorsal="a88460", belly="d8c8a0", tailband="38a0e0")),
                             ("Collared", dict(body="8ca048", head="8ca048", leg="7c9040", tail="8ca048", spot="1c1c18", collar="1c1c18", belly="f0e890", tailband="6c8030")),
                             ("Red", dict(body="b85c34", head="b85c34", leg="a44e2c", tail="b85c34", spot="702c14", dorsal="e89850", belly="f0d4a0", tailband="8c4020"))]),
    "garter_snake": dict(base=dict(body="3c4430", belly="e0e4b0", head="3c4430", stripe="e8dc58", side="e8dc58", eye="141410", tongue="d83030"),
                         variants=[("Red_Sided", dict(body="4c3c2c", head="4c3c2c", stripe="d8b848", side="e04830")),
                                   ("Olive", dict(body="6c7438", head="6c7438", stripe="c8d478", side="a8b458", belly="e8ecc0")),
                                   ("Ribbon", dict(body="1c2428", head="1c2428", stripe="e8e8d8", side="e8e8d8", belly="b8c0b0"))]),
    "viper": dict(base=dict(body="a8946c", belly="d8cca0", head="a8946c", zig="3c2c1c", eye="e8a020", tongue="d83030", headmark="3c2c1c"),
                  variants=[("Grey", dict(body="9ca0a0", head="9ca0a0", zig="2c3030", headmark="2c3030", belly="c8ccc8")),
                            ("Copper", dict(body="b0602c", head="b0602c", zig="5c2c14", headmark="5c2c14", belly="e0b080", eye="f0c030")),
                            ("Black", dict(body="1c1c20", head="1c1c20", zig="282830", headmark="282830", belly="3c3c44", eye="e84020"))]),
    "barn_owl": dict(base=dict(body="d09c5c", belly="f4ece0", head="d09c5c", disc="f8f2e8", wing="c08c4c", wingtip="8c8c8c", tail="d8c8a0", eye="161412", bill="e0d4b0", foot="e8dcc0", spot="987040", chest="f4ece0"),
                     variants=[("Tawny", dict(body="a46c3c", head="a46c3c", wing="946034", belly="d8b890", chest="d8b890", disc="c89868", wingtip="5c3c20", tail="9c7444", spot="6c4624")),
                               ("Grey", dict(body="9a9c9c", head="9a9c9c", wing="8a8c8c", belly="e4e4e0", chest="e4e4e0", disc="eeeeea", wingtip="5c5e5e", tail="b8b8b4", spot="6c6e6e")),
                               ("Dark", dict(body="5c4634", head="5c4634", wing="4c3828", belly="a89070", chest="a89070", disc="d8c8a8", wingtip="2c2018", tail="6c5640", spot="38281c"))]),
    "mallard": dict(base=dict(body="9c9c94", belly="d4d4cc", head="1c7a44", neck="1c7a44", collar="f4f4f0", chest="7c4428", rump="1c1c20", wing="8c8c84", wingtip="5c5c58", spec="2c5ccc", tail="d8d8d0", bill="e8c42c", foot="e88c2c", eye="141410"),
                    variants=[("Hen", dict(body="94704c", belly="a88a60", head="7c5c3c", neck="7c5c3c", collar="7c5c3c", chest="94704c", rump="5c4430", wing="84643f", wingtip="5c4430", tail="7c5c3c", bill="c8903c", spot="5c4430", spec="2c5ccc")),
                              ("White", dict(body="f4f4f0", belly="f4f4f0", head="f4f4f0", neck="f4f4f0", collar="f4f4f0", chest="f4f4f0", rump="f4f4f0", wing="e8e8e4", wingtip="e8e8e4", tail="f4f4f0", bill="f09c28", foot="f09c28", spec="e8e8e4")),
                              ("Mandarin", dict(body="d8783c", belly="f4ecdc", head="3c9c8c", neck="e8c878", collar="f4f4f0", chest="8c3c78", rump="3c3c58", wing="d8a030", wingtip="2c3c78", spec="2c3c78", bill="d84030", foot="e8a030", tail="2c3c58"))]),
    "canada_goose": dict(base=dict(body="8c7a64", belly="e8e4d8", head="1c1c20", neck="1c1c20", cheek="f4f4f0", rump="f4f4f0", wing="7a6a54", wingtip="4c4034", tail="1c1c20", bill="1c1c20", foot="2c2c30", eye="e8e0c0", chest="b8aa94"),
                         variants=[("Grey", dict(body="8c8c8c", belly="d4d4d0", head="5c5c60", neck="5c5c60", cheek="5c5c60", rump="f4f4f0", wing="7c7c7c", wingtip="4c4c50", tail="3c3c40", bill="f09c3c", foot="f09c3c", chest="b0b0b0")),
                                   ("Snow", dict(body="f4f4f4", belly="f4f4f4", head="f4f4f4", neck="f4f4f4", cheek="f4f4f4", rump="f4f4f4", wing="f0f0f0", wingtip="2c2c30", tail="f4f4f4", bill="e0508c", foot="e0508c", chest="f4f4f4")),
                                   ("Barnacle", dict(body="7c8088", belly="f0f0ec", head="f4f4f0", neck="1c1c20", cheek="f4f4f0", rump="1c1c20", wing="6c7078", wingtip="3c4048", tail="1c1c20", chest="1c1c20", bill="1c1c20"))]),
    "seagull": dict(base=dict(body="f4f4f4", belly="f4f4f4", head="f4f4f4", neck="f4f4f4", back="9ca4ac", wing="9ca4ac", wingtip="1c1c20", tail="f4f4f4", bill="f0c828", billspot="d83c2c", foot="f0a0a0", eye="e8d048"),
                    variants=[("Black_Headed", dict(cap="5c3c34", bill="d83c2c", foot="d83c2c", back="b8bcc4", wing="b8bcc4", wingtip="1c1c20", eye="141410")),
                              ("Great_Black_Backed", dict(back="2c3038", wing="2c3038", wingtip="1c1c20", foot="f0a0a0", bill="f0c828")),
                              ("Juvenile", dict(body="c8bcac", belly="d8d0c4", head="c8bcac", neck="c8bcac", back="8c7c68", wing="8c7c68", wingtip="3c3028", tail="d8d0c4", bill="3c3030", billspot="3c3030", foot="c89c94", eye="141410"))]),
    "horseshoe_bat": dict(base=dict(body="6c5444", belly="a89480", head="6c5444", nose="d8a0a0", eye="141410", ear="6c5444", earin="b88c84", membrane="4c3c34", bone="3c2e28", bone2="5c4a40", leaf="a87870"),
                          variants=[("Grey", dict(body="747478", head="747478", belly="b8b8b8", ear="747478", membrane="545458", bone="3c3c40", bone2="64646a", earin="c0a8a8", leaf="a89090")),
                                    ("Ginger", dict(body="b86c34", head="b86c34", belly="e8b878", ear="b86c34", membrane="8c5428", bone="5c3418", bone2="a06430", earin="e8b090", leaf="d89080")),
                                    ("Dark", dict(body="342c38", head="342c38", belly="5c5060", ear="342c38", membrane="2c2430", bone="1c1620", bone2="3c3440", earin="7c5c6c", leaf="6c4c5c"))]),
    "dragonfly": dict(base=dict(thorax="3c9ed8", body="3c9ed8", eye="2c78c0", head="2c78c0", wing="d8ecf4", vein="5c6c78", stigma="1c2028", stripe="1c2028", band="1c2028"),
                      variants=[("Green", dict(thorax="5cc03c", body="5cc03c", eye="3c9c2c", head="3c9c2c", stripe="1c2820", band="1c2820")),
                                ("Scarlet", dict(thorax="d83c2c", body="d83c2c", eye="a02418", head="a02418", stripe="3c1814", band="3c1814", wing="f4e4dc")),
                                ("Amber", dict(thorax="e8a828", body="e8a828", eye="b87818", head="b87818", stripe="3c2c14", band="3c2c14", wing="f4ecd0")),
                                ("Emperor", dict(thorax="3c8c58", body="2c78d8", eye="38a870", head="38a870", stripe="1c3c28", band="1c2c68", wing="e4f0e8"))]),
    "meadow_butterfly": dict(base=dict(pat="monarch", wing="f08c24", edge="1c1818", vein="1c1818", dot="f8f4e8", body="2c2420", eye="f8f4e8", under="f08c24"),
                             variants=[("Swallowtail", dict(pat="swallow", wing="f4dc38", edge="1c1818", vein="1c1818", dot="3c8cd8")),
                                       ("Morpho", dict(pat="morpho", wing="2c78e0", edge="1c2038", dot="8cc8f8", vein="1c2038")),
                                       ("Cabbage_White", dict(pat="white", wing="f8f8f0", edge="2c2824", dot="2c2824", vein="d8d8cc")),
                                       ("Emperor", dict(pat="emperor", wing="6c3c9c", edge="2c1c3c", dot="f4f0f8", vein="3c2058"))]),
    "bumblebee": dict(base=dict(body="f0c428", thorax="f0c428", head="2c2418", dark="2c2418", eye="d8d0c0", wing="dceaf0", vein="8c9ca4", tip="f8f4ec", belly="3c3020"),
                      variants=[("Red_Tailed", dict(body="3c3028", thorax="e8a830", tip="d8482c", dark="2c2418")),
                                ("Golden", dict(body="f0a020", thorax="f8d048", tip="f8d048", dark="4c2c14")),
                                ("White_Tailed", dict(body="e8c030", thorax="f0d048", tip="f8f8f4", dark="2c2418"))]),
    "ladybird": dict(base=dict(shell="d82c24", dot="161414", dark="161414", head="161414", white="f8f4e8"),
                     variants=[("Orange", dict(shell="f08c1c", dot="f8f4e8")),
                               ("Yellow", dict(shell="f4d428", dot="161414")),
                               ("Black", dict(shell="1c1c20", dot="d82c24"))]),
    "fiddler_crab": dict(base=dict(body="e08c34", belly="f0d8a0", mark="b8601c", legs="d07428", legs2="b8601c", eye="161414", pupil="161414", claw="f0a040", clawtip="f8e8c0"),
                         variants=[("Red", dict(body="c83c2c", mark="982418", legs="b83024", legs2="982418", claw="e04c34", clawtip="f8e0d0", belly="f0c8b0")),
                                   ("Blue", dict(body="3c78c0", mark="2c5c98", legs="3068a8", legs2="2c5c98", claw="e08c34", clawtip="f8e8c0", belly="c8d8f0")),
                                   ("Sand", dict(body="d8c088", mark="b8a068", legs="c8ae78", legs2="b8a068", claw="e8d8a0", clawtip="f8f0d8", belly="f0e8cc"))]),
    "roe_deer": dict(base=dict(body="a8683c", belly="d8b88c", head="a8683c", neck="a8683c", muzzle="d8b88c", nose="1c1814", eye="1c1814", ear="a8683c", earin="e8d0a8", leg="a8683c", sock="5c3c24", tail="e8d8bc", antler="d8c8a0", rumpw="f0e8d4", back="8c5430"),
                 variants=[("Fallow", dict(body="b8844c", head="b8844c", neck="b8844c", ear="b8844c", leg="b8844c", spot="f0e4cc", belly="f0e4cc", muzzle="f0e4cc", back="9c6c3c", sock="6c4c2c", antler="c8b890")),
                           ("Red", dict(body="94441c", head="94441c", neck="94441c", ear="94441c", leg="94441c", belly="c89c6c", muzzle="c89c6c", sock="3c2414", back="743414", rumpw="e8d4a8", tall="5.4")),
                           ("Doe", dict(antler=None))]),
    "arctic_fox": dict(base=dict(body="f4f6f8", belly="f4f6f8", head="f4f6f8", muzzle="f4f6f8", nose="1c1c20", eye="1c1c20", ear="f4f6f8", earin="e8d0d0", leg="f4f6f8", sock="e8ecf0", tail="f4f6f8"),
                       variants=[("Blue", dict(body="6c7280", belly="a8aeb8", head="6c7280", muzzle="a8aeb8", ear="6c7280", leg="6c7280", sock="4c5260", tail="8c92a0", back="5c6270")),
                                 ("Silver", dict(body="b8bcc4", belly="e8ecf0", head="b8bcc4", muzzle="e8ecf0", ear="b8bcc4", leg="b8bcc4", sock="6c7280", tail="d0d4dc", tailtip="4c5260")),
                                 ("Autumn", dict(body="b89878", belly="e8dcc8", head="b89878", muzzle="e8dcc8", ear="b89878", leg="b89878", sock="6c5440", tail="c8ac8c", tailtip="f4f0e8"))]),
    "fennec_fox": dict(base=dict(body="e0bc8c", belly="f8f0e0", head="e0bc8c", muzzle="f8f0e0", nose="1c1c20", eye="1c1c20", ear="e0bc8c", earin="f4d8d0", leg="e0bc8c", sock="d0a874", tail="e0bc8c", tailtip="3c3028"),
                       variants=[("Pale", dict(body="f0dcc0", head="f0dcc0", ear="f0dcc0", leg="f0dcc0", sock="e0c8a0", tail="f0dcc0", tailtip="b8a080")),
                                 ("Rufous", dict(body="c88858", head="c88858", ear="c88858", leg="c88858", sock="a86c40", tail="c88858", belly="f0d8b8", muzzle="f0d8b8"))]),
    "cottontail": dict(base=dict(body="9c7c5c", belly="e8dcc8", head="9c7c5c", muzzle="e8dcc8", nose="d89898", eye="1c1814", ear="9c7c5c", earin="e0b0a8", leg="9c7c5c", sock="e8dcc8", tail="f8f4ec", tailtip="f8f4ec", back="7c5e42"),
                       variants=[("Grey", dict(body="8c8c88", head="8c8c88", ear="8c8c88", leg="8c8c88", back="6c6c68", belly="e0e0dc", muzzle="e0e0dc", sock="d0d0cc")),
                                 ("Black", dict(body="2c2830", head="2c2830", ear="2c2830", leg="2c2830", back="1c1820", belly="4c4854", muzzle="4c4854", sock="2c2830", tail="e8e4ec", tailtip="e8e4ec", earin="6c5c64", nose="b88c94")),
                                 ("White", dict(body="f4f4f2", head="f4f4f2", ear="f4f4f2", leg="f4f4f2", back="e8e8e4", belly="f4f4f2", muzzle="f4f4f2", sock="f4f4f2", nose="e0a0a0", eye="d83c4c")),
                                 ("Spotted", dict(body="f0ece4", head="f0ece4", ear="2c2830", leg="f0ece4", back="e4dfd4", belly="f0ece4", muzzle="f0ece4", sock="f0ece4", spot="3c3640"))]),
    "hedgehog": dict(base=dict(body="c8a888", belly="e8d8c0", head="c8a888", muzzle="e8d8c0", nose="1c1814", eye="1c1814", ear="c8a888", leg="b89878", quill="6c5438", quill2="e0d4b8"),
                     variants=[("Pale", dict(quill="b8a888", quill2="f4f0e4", body="e8d8c4", head="e8d8c4", muzzle="f4ece0", belly="f4ece0", ear="e8d8c4", leg="d8c8b0", eye="a82c3c")),
                               ("Dark", dict(quill="342c28", quill2="8c7c68", body="6c5444", head="6c5444", muzzle="a88c74", belly="a88c74", ear="6c5444", leg="5c4838"))]),
    "otter": dict(base=dict(body="6c4a30", belly="c8a478", head="6c4a30", muzzle="d8bc94", nose="1c1814", eye="1c1814", ear="6c4a30", leg="5c3e28", tail="5c3e28"),
                  variants=[("Dark", dict(body="3c2c24", head="3c2c24", ear="3c2c24", leg="342620", tail="342620", belly="8c7058", muzzle="a08868")),
                            ("Sea", dict(body="5c4430", head="e0c8a0", muzzle="e8d8bc", ear="5c4430", leg="4c3828", tail="4c3828", belly="8c7458")),
                            ("Pale", dict(body="a88454", head="a88454", ear="a88454", leg="947444", tail="947444", belly="e8d4a8", muzzle="f0e4c4"))]),
    "beaver": dict(base=dict(body="7c5434", belly="a88058", head="7c5434", muzzle="a88058", nose="1c1814", eye="1c1814", ear="5c3c24", leg="5c3c24", tail="3c2c20", scale="4c382a", teeth="f09c30"),
                   variants=[("Dark", dict(body="4c3828", head="4c3828", belly="7c5c40", muzzle="7c5c40", ear="342618", leg="342618", tail="2c2018", scale="3c2c20")),
                             ("Blond", dict(body="b88c54", head="b88c54", belly="e0c490", muzzle="e0c490", ear="946c3c", leg="946c3c", tail="6c5034", scale="84643f"))]),
    "warthog": dict(base=dict(body="8c7860", belly="a89478", head="8c7860", muzzle="a89478", nose="2c2420", eye="1c1814", ear="8c7860", leg="7c6850", sock="4c3c30", tail="8c7860", tailtip="2c2420", mane="d8c8a0", tusk="f4ecd4", wart="b8a484", back="7c6850"),
                    variants=[("Red", dict(body="b86c3c", head="b86c3c", ear="b86c3c", leg="a45c30", sock="5c3418", belly="d8a878", muzzle="d8a878", mane="f0e0b8", back="a45c30", tail="b86c3c", wart="d89c68")),
                              ("Dark", dict(body="4c4038", head="4c4038", ear="4c4038", leg="403630", sock="2c2420", belly="6c5c50", muzzle="6c5c50", mane="a89880", back="403630", tail="4c4038", wart="7c6c5c"))]),
    # Boomers blend into the ground they appear on: the look is chosen by the biome
    # (`biomes` below), the plain look being the forest's and the fallback.
    "boomer": dict(base=dict(body="5c9a3c", head="5c9a3c", leg="4a7c30", mark="7a5a38", mark2="3c6a28", eye="181c10", mouth="181c10", stem="3c7a2c", pat="patch"),
                   variants=[("Desert", dict(body="d8c080", head="d8c080", leg="c4a868", mark="6a4a28", mark2="6a9a48", stem="6a9a48", eye="2c2418", mouth="2c2418", pat="spines", biomes="desert savanna")),
                             ("Snow", dict(body="eef2f6", head="eef2f6", leg="d0dae4", mark="a8d0ec", mark2="a8d0ec", stem="8cb0c8", eye="28303c", mouth="28303c", pat="frost", biomes="tundra")),
                             ("Swamp", dict(body="5a6a38", head="5a6a38", leg="48562c", mark="94b84c", mark2="38401e", stem="4c7a38", eye="d8e070", mouth="2c3418", pat="moss", biomes="swamp")),
                             ("Badlands", dict(body="b4583c", head="b4583c", leg="9c4830", mark="e0a878", mark2="7c3824", stem="8a5a34", eye="2c1810", mouth="2c1810", pat="bands", biomes="badlands")),
                             ("Mountain", dict(body="8a8a90", head="8a8a90", leg="707078", mark="d8c060", mark2="58c0d0", stem="64646c", eye="181820", mouth="181820", pat="ore", biomes="mountain")),
                             ("Jungle", dict(body="3cae3c", head="3cae3c", leg="2c8a34", mark="e84c78", mark2="f0d848", stem="287a28", eye="182010", mouth="182010", pat="flowers", biomes="jungle"))]),
    "cave_boomer": dict(base=dict(body="2c2c38", head="2c2c38", leg="22222c", mark="58e8c8", mark2="38a890", eye="9cf8e0", mouth="9cf8e0", stem="3c3c4c", pat="glow"),
                        variants=[("Ember", dict(body="3a2420", head="3a2420", leg="2c1a18", mark="ff8a28", mark2="ffd048", eye="ffd048", mouth="ff8a28", stem="4c2c24", pat="embers", biomes="magma_cave")),
                                  ("Crystal", dict(body="7088b8", head="7088b8", leg="586898", mark="d8f0ff", mark2="a8c8f0", eye="182038", mouth="182038", stem="8098c8", pat="ore", biomes="crystal_cave frozen_cave"))]),
    "squirrel": dict(base=dict(body="b8582c", belly="f0e4d0", head="b8582c", muzzle="f0e4d0", nose="1c1814", eye="1c1814", ear="b8582c", leg="a44c24", sock="a44c24", tail="b8582c", tailtip="f0d4b0"),
                     variants=[("Grey", dict(body="8c8c88", head="8c8c88", ear="8c8c88", leg="7c7c78", sock="7c7c78", tail="a8a8a4", tailtip="e8e8e4", belly="f0f0ec", muzzle="f0f0ec")),
                               ("Black", dict(body="2c2830", head="2c2830", ear="2c2830", leg="241f28", sock="241f28", tail="342f38", tailtip="4c4654", belly="4c4654", muzzle="4c4654")),
                               ("Ginger", dict(body="d88838", head="d88838", ear="d88838", leg="c47830", sock="c47830", tail="e8a458", tailtip="f8e0b0"))]),
    "badger": dict(base=dict(body="7c7c80", belly="3c3a3c", head="f0ece4", muzzle="f0ece4", nose="1c1c20", eye="1c1c20", ear="f0ece4", leg="3c3a3c", sock="2c2a2c", tail="8c8c90", stripe="1c1c20", back="9c9ca0"),
                    variants=[("Honey", dict(body="b89860", head="f4ecd4", muzzle="f4ecd4", belly="6c5438", ear="f4ecd4", leg="6c5438", sock="4c3822", tail="c8a870", back="d0b078", stripe="3c2c18")),
                              ("Dark", dict(body="3c3c44", head="d8d4cc", muzzle="d8d4cc", belly="1c1c20", ear="d8d4cc", leg="1c1c20", sock="141418", tail="4c4c54", back="505058", stripe="141418"))]),
}


# ======================================================================
#  What the creatures are like in play
# ======================================================================
# len: how long the creature is, in blocks (the game wants a height; it is worked out from the model).
# Everything else is the `entity` section as in docs/DATA.md.

ENTITIES = [
    # id, name, builder, dict
    ("tree_frog", "Tree Frog", dict(len=0.38, speed=1.7, hp=11, behavior="wander", biomes="swamp jungle forest", weight=6, trait="jumps flees", anim="hop", sound="peep", hunts="ant beetle")),
    ("dart_frog", "Dart Frog", dict(len=0.26, speed=1.5, hp=8, behavior="wander", biomes="jungle", weight=5, trait="jumps flees", anim="hop", sound="peep", hunts="ant beetle")),
    ("pond_frog", "Pond Frog", dict(len=0.5, speed=1.7, hp=13, behavior="wander", biomes="swamp plains forest", weight=7, trait="jumps flees", anim="hop", sound="croak", hunts="ant beetle worm")),
    ("toad", "Toad", dict(len=0.5, speed=1.3, hp=15, behavior="wander", biomes="swamp forest plains taiga", weight=6, trait="jumps", anim="hop", sound="trill", hunts="worm ant beetle")),
    ("tadpole", "Tadpole", dict(len=0.3, speed=1.0, hp=3, behavior="swim", biomes="", weight=4, anim="float")),
    ("newt", "Newt", dict(len=0.46, speed=1.0, hp=9, behavior="wander", biomes="swamp forest taiga", weight=5, trait="lazy", anim="walk", hunts="worm")),
    ("pond_turtle", "Pond Turtle", dict(len=0.6, speed=0.8, hp=24, behavior="wander", biomes="swamp jungle", weight=5, trait="lazy", anim="walk", baby="0.4 420")),
    ("sea_turtle", "Sea Turtle", dict(len=1.5, speed=1.6, hp=30, behavior="swim", biomes="", weight=3, anim="walk", baby="0.35 480", drops="raw_meat 2")),
    ("gecko", "Gecko", dict(len=0.4, speed=2.2, hp=7, behavior="wander", biomes="desert jungle badlands savanna", weight=5, trait="flees", anim="scuttle", hunts="ant beetle")),
    ("lizard", "Lizard", dict(len=0.62, speed=2.4, hp=12, behavior="wander", biomes="desert badlands savanna plains", weight=6, trait="flees", anim="scuttle", hunts="ant beetle worm")),
    ("garter_snake", "Garter Snake", dict(len=1.0, speed=1.6, hp=10, behavior="wander", biomes="plains forest swamp", weight=5, trait="flees", anim="slither", sound="hiss", hunts="worm frog tree_frog pond_frog")),
    ("viper", "Viper", dict(len=0.85, speed=1.7, hp=16, behavior="wander", biomes="forest taiga mountain badlands", weight=4, trait="territorial", damage=2, anim="slither", sound="hiss", hunts="mouse cottontail pond_frog")),
    ("barn_owl", "Barn Owl", dict(len=0.34, speed=3.0, hp=18, behavior="fly", biomes="forest plains taiga", weight=6, night=True, anim="flap", sound="hoot")),
    ("mallard", "Mallard", dict(len=0.62, speed=1.6, hp=7, behavior="wander", biomes="swamp plains forest", weight=7, trait="herd flees", anim="waddle", baby="0.4 300", gives="egg 220", drops="raw_meat 1", sound="quack")),
    ("canada_goose", "Canada Goose", dict(len=0.85, speed=1.5, hp=14, behavior="wander", biomes="swamp plains tundra", weight=5, trait="herd territorial", damage=1, anim="waddle", baby="0.4 360", gives="egg 240", drops="raw_meat 1", sound="honk")),
    ("seagull", "Seagull", dict(len=0.5, speed=3.2, hp=9, behavior="fly", biomes="plains swamp tundra savanna", weight=6, trait="flees", anim="flap", sound="gullcry")),
    ("horseshoe_bat", "Horseshoe Bat", dict(len=0.3, speed=3.4, hp=6, behavior="fly", biomes="cave mushroom_cave crystal_cave lush_cave webbed_cave", weight=9, anim="flap", sound="squeak")),
    ("dragonfly", "Dragonfly", dict(len=0.4, speed=3.0, hp=4, behavior="fly", biomes="swamp jungle forest plains", weight=7, anim="flap", sound="whirr")),
    ("meadow_butterfly", "Meadow Butterfly", dict(len=0.26, speed=1.5, hp=3, behavior="fly", biomes="plains forest jungle savanna taiga", weight=9, anim="flap")),
    ("bumblebee", "Bumblebee", dict(len=0.22, speed=2.2, hp=5, behavior="fly", biomes="plains forest taiga savanna", weight=6, anim="flap", sound="buzz")),
    ("ladybird", "Ladybird", dict(len=0.13, speed=0.8, hp=3, behavior="wander", biomes="plains forest swamp", weight=4, anim="scuttle")),
    ("fiddler_crab", "Fiddler Crab", dict(len=0.34, speed=1.9, hp=8, behavior="wander", biomes="desert swamp", weight=4, trait="flees", anim="scuttle", sound="tick")),
    ("roe_deer", "Roe Deer", dict(len=1.25, speed=4.6, hp=26, behavior="wander", biomes="forest taiga plains", weight=6, trait="herd flees prey", anim="walk", baby="0.5 480", drops="raw_meat 2", sound="bark_deer")),
    ("arctic_fox", "Arctic Fox", dict(len=0.8, speed=4.2, hp=17, behavior="wander", biomes="tundra taiga", weight=4, trait="flees", anim="walk", baby="0.5 420", sound="yip", hunts="mouse cottontail")),
    ("fennec_fox", "Fennec Fox", dict(len=0.58, speed=4.4, hp=12, behavior="wander", biomes="desert", weight=4, trait="flees", anim="walk", baby="0.5 380", sound="yip", hunts="mouse lizard gecko")),
    ("cottontail", "Cottontail", dict(len=0.5, speed=3.6, hp=6, behavior="wander", biomes="plains forest taiga tundra desert savanna", weight=9, trait="flees prey jumps", anim="hop", baby="0.45 300", drops="raw_meat 1", sound="squeak")),
    ("hedgehog", "Hedgehog", dict(len=0.4, speed=1.0, hp=10, behavior="wander", biomes="forest plains taiga", weight=5, trait="lazy", anim="walk", baby="0.45 360", sound="snuffle", hunts="worm beetle")),
    ("otter", "Otter", dict(len=0.95, speed=3.6, hp=18, behavior="swim", biomes="", weight=3, anim="walk", baby="0.5 420", sound="chitter", drops="raw_fish 1")),
    ("beaver", "Beaver", dict(len=0.85, speed=1.8, hp=21, behavior="wander", biomes="swamp forest taiga", weight=4, trait="lazy", anim="waddle", baby="0.45 420", drops="stick 3", sound="grunt")),
    ("warthog", "Warthog", dict(len=1.1, speed=3.6, hp=34, behavior="wander", biomes="savanna", weight=5, trait="herd territorial pack", damage=4, anim="walk", baby="0.45 420", drops="raw_meat 3", sound="grunt")),
    ("squirrel", "Squirrel", dict(len=0.42, speed=3.2, hp=7, behavior="wander", biomes="forest taiga jungle", weight=8, trait="flees jumps", anim="hop", baby="0.5 300", sound="chitter")),
    # Hostile, and it blows up: `explodes <power> <fuse seconds> <trigger range>` (docs/CREATURES.md).
    ("boomer", "Boomer", dict(len=0.56, speed=3.3, hp=20, behavior="hostile", weight=3, damage=0, anim="walk", sound="rustle", drops="gunpowder 2", explodes="2.0 1.5 3.0")),
    ("cave_boomer", "Cave Boomer", dict(len=0.56, speed=3.0, hp=20, behavior="hostile", biomes="cave mushroom_cave crystal_cave lush_cave frozen_cave magma_cave webbed_cave", weight=4, damage=0, anim="walk", sound="rustle",
                                        drops="gunpowder 2", explodes="2.0 1.5 3.0", glow=6)),
    ("badger", "Badger", dict(len=0.9, speed=2.4, hp=24, behavior="wander", biomes="forest taiga mountain", weight=4, trait="territorial", damage=3, anim="walk", baby="0.45 420", drops="raw_meat 2", sound="snuffle")),
]

BUILDERS = {e[0]: globals()["b_" + e[0]] for e in ENTITIES}

SOUNDS = [
    "sound croak effects 0.32 0.35 190 120 0.32 0.50 A frog croaks",
    "sound peep effects 0.09 0.04 2900 3500 0.90 0.28 A tree frog peeps",
    "sound trill effects 0.55 0.10 640 700 0.55 0.30 A toad trills",
    "sound quack effects 0.20 0.28 560 340 0.50 0.50 A duck quacks",
    "sound honk effects 0.32 0.20 500 360 0.45 0.55 A goose honks",
    "sound hoot effects 0.50 0.05 400 310 0.28 0.45 An owl hoots",
    "sound gullcry effects 0.38 0.20 1900 1200 0.80 0.45 A gull cries",
    "sound squeak effects 0.10 0.10 2300 1800 0.80 0.35 A squeak",
    "sound snuffle effects 0.28 0.65 170 130 0.18 0.40 Snuffling",
    "sound yip effects 0.15 0.25 950 640 0.60 0.50 A fox yips",
    "sound chitter effects 0.22 0.25 1900 2500 0.80 0.35 Chittering",
    "sound grunt effects 0.30 0.50 130 90 0.20 0.55 A grunt",
    "sound whirr effects 0.45 0.45 280 320 0.60 0.18 Wings whirr",
    "sound tick effects 0.06 0.40 3000 2500 0.90 0.25 Ticking",
    "sound bark_deer effects 0.22 0.45 300 240 0.35 0.50 A deer barks",
    "sound rustle effects 0.30 0.90 800 500 0.50 0.25 Leaves rustle",
    "sound fuse_hiss effects 1.50 0.95 2800 1800 0.90 0.50 A fuse hisses",
    "sound boomer_boom effects 0.50 0.70 120 40 0.25 0.70 Something goes boom",
]


# ======================================================================
#  Building everything
# ======================================================================

def palette_of(sid, k):
    """The colours of look k of species sid: 0 is the creature itself."""
    spec = PALETTE[sid]
    pal = {key: v for key, v in spec["base"].items() if v is not None}
    if k > 0:
        for key, v in spec["variants"][k - 1][1].items():
            if key == "biomes":      # where a look is for: not a colour (see write_pack)
                continue
            if v is None:
                pal.pop(key, None)
            else:
                pal[key] = v
    out = {}
    for key, v in pal.items():
        out[key] = v if key in ("pat", "tall") else rgb(v)
    return out


def model_id(sid, k):
    return sid if k == 0 else "%s_%s" % (sid, PALETTE[sid]["variants"][k - 1][0].lower())


def make(sid, k):
    m = Model(model_id(sid, k), palette_of(sid, k))
    BUILDERS[sid](m, m.pal)
    return m.build()


def write_all(only=None):
    os.makedirs(os.path.join(MOD, "models"), exist_ok=True)
    os.makedirs(os.path.join(MOD, "textures"), exist_ok=True)
    keep = set()
    models = {}
    for sid, name, spec in ENTITIES:
        if only and sid not in only:
            continue
        for k in range(len(PALETTE[sid]["variants"]) + 1):
            m = make(sid, k)
            models[m.id] = m
            with open(os.path.join(MOD, "models", m.id + ".bbmodel"), "w") as f:
                json.dump(m.to_json(), f, separators=(",", ":"))
            m.image.save(os.path.join(MOD, "textures", m.id + ".png"))
    return models


def write_pack(models):
    out = ["""# Wildlife - a mod that comes with the game, in its own folder.
#
# More creatures for the meadows, ponds, woods, deserts and caves: frogs,
# toads, newts, turtles, lizards and snakes; owls, ducks, geese and gulls;
# bats, dragonflies, butterflies and bees; deer, foxes, rabbits, hedgehogs,
# otters, beavers, boars, squirrels and badgers; and the Boomer, a walking
# gourd that explodes, dressed to match the biome it appears in. Most come in several looks
# (a blue tree frog, a black cottontail), and the ones that have young have
# them. Each look is its own model with its own texture, drawn in the OCD
# style (docs/TEXTURING.md).
#
# This file, the models/ and the textures/ are written by
# tools/wildlife_textures.py from the tables in that script: change a colour
# there and run it again. To change how a creature lives, edit its `entity`
# section below (see docs/DATA.md) or run the script after editing ENTITIES.
# Switch the mod off in Settings > MODS, or delete the folder, and they are gone.
"""]
    out.append("\n".join(SOUNDS))
    out.append("")
    for sid, name, spec in ENTITIES:
        for k in range(len(PALETTE[sid]["variants"]) + 1):
            out.append("model %s file models/%s.bbmodel" % (model_id(sid, k), model_id(sid, k)))
    out.append("")
    for sid, name, spec in ENTITIES:
        m = models[sid]
        lo, hi = m.bounds()
        sx, sy, sz = hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]
        scale = spec["len"] / max(sz, 0.1)
        height = max(0.1, min(6.0, round(sy * scale, 2)))
        width = max(0.15, min(3.0, round(max(min(sx, sz * 0.8), sz * 0.6) * scale, 2)))
        beh = spec["behavior"]
        if beh == "fly":
            width = max(0.15, min(width, 0.4))
        out.append("entity " + sid)
        out.append("  name " + name)
        out.append("  model " + sid)
        out.append("  height %g" % height)
        out.append("  width %g" % width)
        out.append("  speed %g" % spec["speed"])
        out.append("  health %d" % spec["hp"])
        out.append("  behavior " + beh)
        out.append("  weight %d" % spec["weight"])
        if spec.get("biomes"):
            out.append("  biomes " + spec["biomes"])
        if spec.get("night"):
            out.append("  night")
        if spec.get("drops"):
            out.append("  drops " + spec["drops"])
        if spec.get("trait"):
            out.append("  trait " + spec["trait"])
        if "damage" in spec:
            out.append("  damage %d" % spec["damage"])
        if spec.get("anim"):
            out.append("  anim " + spec["anim"])
        if spec.get("baby"):
            out.append("  baby " + spec["baby"])
        if spec.get("gives"):
            out.append("  gives " + spec["gives"])
        if spec.get("sound"):
            out.append("  sound " + spec["sound"])
        if spec.get("hunts"):
            out.append("  hunts " + spec["hunts"])
        if spec.get("explodes"):
            out.append("  explodes " + spec["explodes"])
        if spec.get("glow"):
            out.append("  glow %d" % spec["glow"])
        for k, (vname, look) in enumerate(PALETTE[sid]["variants"]):
            where_ = (" biomes " + look["biomes"]) if look.get("biomes") else ""
            out.append("  variant %s model %s%s" % (vname, model_id(sid, k + 1), where_))
    with open(os.path.join(MOD, "pack.txt"), "w") as f:
        f.write("\n".join(out) + "\n")


# ----------------------------------------------------------------------
#  A look at them: a contact sheet of three-quarter views
# ----------------------------------------------------------------------

def render(m, size=160, yaw=-38, pitch=22):
    ya, pa = math.radians(yaw), math.radians(pitch)
    ca, sa, cp, sp = math.cos(ya), math.sin(ya), math.cos(pa), math.sin(pa)
    lo, hi = m.bounds()
    cx, cz = (lo[0] + hi[0]) / 2, (lo[2] + hi[2]) / 2
    ext = max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], 4.0)
    sc = size * 0.78 / ext
    quads = []

    def proj(pt):
        x, y, z = pt[0] - cx, pt[1] - (lo[1] + hi[1]) / 2, pt[2] - cz
        x, z = x * ca + z * sa, -x * sa + z * ca
        y, z = y * cp - z * sp, y * sp + z * cp
        return (size / 2 + x * sc, size / 2 - y * sc, z)
    for part in m.parts:
        r = rot_matrix(part.rot)

        def tw(pt, r=r, part=part):
            q = [pt[k] - part.origin[k] for k in range(3)]
            return [sum(r[i][k] * q[k] for k in range(3)) + part.origin[i] for i in range(3)]
        for f in FACES:
            if f not in part.tex:
                continue
            w, h = len(part.tex[f][0]), len(part.tex[f])
            tl, tr, bl = face_frame(f, part.lo, part.hi)
            shade = {"east": 0.8, "west": 0.8, "up": 1.0, "down": 0.55, "south": 0.9, "north": 0.9}[f]
            for j in range(h):
                for i in range(w):
                    c = part.tex[f][j][i]
                    if c[3] == 0:
                        continue
                    def pt(u, v):
                        return tw([tl[a] + (tr[a] - tl[a]) * u + (bl[a] - tl[a]) * v for a in range(3)])
                    cs = [proj(pt(i / w, j / h)), proj(pt((i + 1) / w, j / h)), proj(pt((i + 1) / w, (j + 1) / h)), proj(pt(i / w, (j + 1) / h))]
                    nrm = (cs[1][0] - cs[0][0]) * (cs[2][1] - cs[1][1]) - (cs[1][1] - cs[0][1]) * (cs[2][0] - cs[1][0])
                    if nrm <= 0:
                        continue
                    quads.append((sum(q[2] for q in cs) / 4, [(q[0], q[1]) for q in cs], tuple(int(c[n] * shade) for n in range(3))))
    quads.sort(key=lambda q: -q[0])
    img = Image.new("RGB", (size, size), (196, 214, 228))
    d = ImageDraw.Draw(img)
    for _, poly, col in quads:
        d.polygon(poly, fill=col)
    return img


def sheet(models, path):
    ids = [i for i in models]
    cols = 6
    size = 150
    rows = (len(ids) + cols - 1) // cols
    img = Image.new("RGB", (cols * size, rows * size), (196, 214, 228))
    d = ImageDraw.Draw(img)
    for n, i in enumerate(ids):
        img.paste(render(models[i], size), ((n % cols) * size, (n // cols) * size))
        d.text(((n % cols) * size + 3, (n // cols) * size + 2), i, fill=(20, 20, 30))
    img.save(path)


def main():
    args = sys.argv[1:]
    only = None
    if "--only" in args:
        only = set(args[args.index("--only") + 1].split(","))
    models = write_all(only)
    if not only:
        write_pack(models)
    if "--sheet" in args:
        path = args[args.index("--sheet") + 1]
        pick = {k: v for k, v in models.items() if "--all" in args or k in PALETTE}
        sheet(pick, path)
    print("%d models, %d species" % (len(models), len(ENTITIES)))


if __name__ == "__main__":
    main()
