#!/usr/bin/env python3
"""Paints every creature's and player's texture in the OCD style.

    python3 tools/entity_textures.py                  # write the pictures (the wildlife mod's textures/ folder; the player's and the skins' to data/textures/entity/)
    python3 tools/entity_textures.py --sheet out.png  # also a contact sheet of the result
    python3 tools/entity_textures.py --only cow,pig   # just the textures of these models
    python3 tools/entity_textures.py --template cow   # layout map of a model (for pack makers)

The palette table is tools/entity_palettes.py: one entry per model gives its
colours by body part ("zone") and a few patterns; every look of a creature
(`cow_brown`, `cow_red`...) is the same entry with some colours changed.
Change a colour there, run this, and every picture is repainted: nothing is
painted by hand. See docs/TEXTURING.md, "Creatures and players".

How a picture is made. The models (the wildlife mod's models/*.bbmodel, written by
tools/gen_models.py) say which rectangle of the texture each face of each box
uses. For every face this script

 1. finds the zone of the box from its name (`head`, `ear-l`, `shin-f-r`...),
 2. fills the face with the zone's colour, one flat colour per face,
 3. lets patterns recolour parts of it (spots, stripes, socks...). Patterns
    are worked out from the 3D position of each pixel, so a spot continues
    across the edge from one face to the next,
 4. draws a one-pixel frame 22% darker round every face that is at least
    3 x 3 pixels (the OCD frame), and
 5. puts the eyes, nose holes and the like on the faces that have them.

There is no noise, no dithering and no gradient: 3 to 8 colours a picture.
The output is deterministic. Pixels that no face uses stay see-through.

The game's own pictures (the player, the skins) go to data/textures/entity/<name>.png,
which the game embeds. Every other picture goes to the wildlife mod's textures/ folder
(tools/export_mods.py moves each creature's pictures into the mod it belongs to and
writes the `picture` lines that make the game read them). The game uses `<name>` for
the model of that name, and for any creature whose data says `tex <name>` (see docs/DATA.md). Pictures in a texture pack's
textures/entity folder replace them (src/content/pack_entities.rs)."""
import argparse
import base64
import io
import json
import math
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))  # the mods live outside the repository
ART = os.path.join(MODS_DIR, "wildlife")                 # the creatures' models and pictures are made here, then tools/export_mods.py sorts them into their mods
MODELS = os.path.join(ART, "models")
CORE_MODELS = os.path.join(ROOT, "data", "models")       # the game's own: the player and the hands
OUT = os.path.join(ART, "textures")
CORE_OUT = os.path.join(ROOT, "data", "textures", "entity")   # the game's own pictures: the player and the skins


def is_core(name):
    return name.startswith("skin_") or name in ("player", "player_slim")
sys.path.insert(0, HERE)

CLEAR = (-1, -1, -1)  # a pattern's answer for "leave this pixel see-through"
FRAME = 0.78       # the frame is this much of the fill's brightness
LIGHT = 0.22       # how far "lighter" colours go towards white


# ---------------------------------------------------------------- colours

def hexc(s):
    s = s.lstrip("#")
    return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))


def dark(c, f=FRAME):
    return tuple(max(0, min(255, int(round(v * f)))) for v in c[:3])


def light(c, t=LIGHT):
    return tuple(max(0, min(255, int(round(v + (255 - v) * t)))) for v in c[:3])


def mixc(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


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


def name_hash(s):
    h = 2166136261
    for ch in s.encode():
        h = ((h ^ ch) * 16777619) & 0xFFFFFFFF
    return h


# ---------------------------------------------------------------- models

class Face:
    __slots__ = ("elem", "zone", "face", "rect", "lo", "hi")


def _walk(node, out):
    for c in node:
        if isinstance(c, dict):
            _walk(c.get("children", []), out)
        else:
            out.add(c)


def load_model(model):
    path = os.path.join(MODELS, model + ".bbmodel")
    if not os.path.exists(path):
        path = os.path.join(CORE_MODELS, model + ".bbmodel")
    d = json.load(open(path))
    res = d["resolution"]
    return d, res["width"], res["height"]


# Name of a box -> zone. First match wins. A zone with no colour of its own in
# the palette takes the colour of FALLBACK[zone], and finally of `body`.
ZONE_RULES = [
    ("eye", "eye"),
    ("claw-arm", "arm"), ("pincer", "arm"), ("plastron", "belly"), ("crest", "mane"), ("tongue", "comb"),
    ("trunk", "head"), ("hood", "head"), ("trail", "tail"),
    ("horn", "horn"), ("tusk", "horn"), ("claw", "horn"), ("sting", "horn"),
    ("mandible", "horn"), ("spine", "horn"), ("fang", "horn"),
    ("antenna", "dark"),
    ("ear", "ear"),
    ("beak", "beak"), ("bill", "beak"),
    ("jaw", "snout"), ("muzzle", "snout"), ("snout", "snout"), ("cheek", "snout"),
    ("wattle", "wattle"), ("comb", "comb"),
    ("mane", "mane"), ("forelock", "mane"), ("beard", "mane"), ("ruff", "mane"),
    ("dewlap", "mane"), ("crown", "mane"), ("hair", "mane"),
    ("udder", "udder"),
    ("tail-tuft", "tail_tip"),
    ("tail-fin", "fin"), ("dorsal", "fin"), ("pectoral", "fin"), ("belly-fin", "fin"),
    ("tail", "tail"),
    ("wing", "wing"),
    ("hand", "hand"), ("fist", "hand"),
    ("sleeve", "sleeve"),
    ("arm", "arm"), ("forearm", "arm"),
    ("tentacle", "leg"), ("shin-tentacle", "leg"),
    ("leg", "leg"), ("thigh", "leg"), ("shin", "leg"), ("calf", "leg"), ("foot", "leg"),
    ("skirt", "leg"),
    ("shell", "shell"), ("dome", "head"),
    ("head", "head"), ("cap", "head"), ("face", "head"), ("skull", "head"), ("brow", "head"),
    ("hat", "mane"),
    ("neck", "neck"),
]
# Pieces of the lower leg: the `hoof` colour covers their bottom third.
LOWER_LEG = ("shin", "calf", "foot")
FALLBACK = {
    "tail_tip": "tail", "hoof": "leg", "hand": "arm", "arm": "leg", "armtop": "arm", "sleeve": "body",
    "fin": "tail", "shell": "body", "mane": "tail", "neck": "body", "dark": "ear", "snout": "head",
    "beak": "snout", "wing": "body", "udder": "belly", "horn": "body", "comb": "body", "wattle": "body",
    "ear": "head", "tail": "body", "leg": "body", "head": "body", "belly": "body", "pants": "leg", "shoe": "leg",
}


def zone_of(model, name, remap=None):
    n = name.lower()
    for key, zone in (remap or {}).items():
        if n.startswith(key):
            return zone
    if n.endswith("-plug"):
        return None  # fill-ins that borrow a row of another face's pixels
    if model.startswith("player"):
        # The overlay layers are left see-through unless the palette has them.
        if n == "hat":
            return "o_hat"
        if n.startswith("jacket"):
            return "o_jacket"
        if "sleeve" in n:
            return "o_sleeve"
        if n.endswith("-pants"):
            return "o_pants"
        if n in ("r-arm", "l-arm"):
            return "armtop"
        if "forearm" in n:
            return "arm"
        if "hand" in n:
            return "hand"
        if "thigh" in n or "calf" in n:
            return "pants"
        if "foot" in n:
            return "shoe"
        if n in ("chest", "belly"):
            return "body"
    for key, zone in ZONE_RULES:
        if n.startswith(key) or ("-" + key) in n and key in ("tail", "fin", "wing", "ear", "horn"):
            return zone
    return "body"


def faces_of(model, remap=None):
    """Every textured face of the model, with its box and zone. `remap` gives
    boxes (by the start of their name) another zone than the usual."""
    d, tw, th = load_model(model)
    out = []
    lo = [1e9] * 3
    hi = [-1e9] * 3
    for e in d["elements"]:
        for k in range(3):
            lo[k] = min(lo[k], e["from"][k], e["to"][k])
            hi[k] = max(hi[k], e["from"][k], e["to"][k])
    for e in d["elements"]:
        zone = zone_of(model, e["name"], remap)
        if zone is None:
            continue
        for fname, f in e["faces"].items():
            uv = f.get("uv")
            if not uv or f.get("texture", 0) is None:
                continue
            x0, y0, x1, y1 = uv
            x0, x1 = sorted((x0, x1))
            y0, y1 = sorted((y0, y1))
            if x1 - x0 < 1 or y1 - y0 < 1:
                continue
            fc = Face()
            fc.elem = e
            fc.zone = zone
            fc.face = fname
            fc.rect = (int(round(x0)), int(round(y0)), int(round(x1)), int(round(y1)))
            out.append(fc)
    return out, tw, th, lo, hi


def texel_pos(fc, i, j):
    """3D position of texel (i, j) of the face, in model units, and the position
    within the box as fractions 0..1 (x from the left, y from the bottom, z from the front)."""
    x0, y0, x1, y1 = fc.rect
    w, h = x1 - x0, y1 - y0
    fu, fv = (i + 0.5) / w, (j + 0.5) / h
    a, b = fc.elem["from"], fc.elem["to"]
    lo = [min(a[k], b[k]) for k in range(3)]
    hi = [max(a[k], b[k]) for k in range(3)]
    f = fc.face
    fx = fy = fz = 0.5
    if f in ("north", "south", "east", "west"):
        fy = 1.0 - fv
    if f == "north":
        fx, fz = 1.0 - fu, 0.0
    elif f == "south":
        fx, fz = fu, 1.0
    elif f == "east":
        fx, fz = 1.0, fu
    elif f == "west":
        fx, fz = 0.0, 1.0 - fu
    elif f == "up":
        fx, fy, fz = fu, 1.0, fv
    elif f == "down":
        fx, fy, fz = fu, 0.0, 1.0 - fv
    fr = (fx, fy, fz)
    pos = tuple(lo[k] + (hi[k] - lo[k]) * fr[k] for k in range(3))
    return pos, fr


# ---------------------------------------------------------------- palette

ZONE_KEYS = ("body", "head", "belly", "snout", "ear", "leg", "hoof", "tail", "tail_tip", "mane", "horn", "eye", "nose",
             "wing", "neck", "arm", "armtop", "hand", "beak", "comb", "wattle", "udder", "sleeve", "fin", "shell",
             "dark", "pants", "shoe", "o_hat", "o_jacket", "o_sleeve", "o_pants")


def derive(spec):
    """The colour of every zone for a spec: {'main': colour, <zone>: colour, 'pat': [...]}."""
    main = spec["main"]
    z = {"body": main}
    z["head"] = spec.get("head", main)
    z["belly"] = spec.get("belly", light(main, 0.30))
    z["snout"] = spec.get("snout", light(z["head"], 0.25))
    z["ear"] = spec.get("ear", dark(z["head"], 0.86))
    z["leg"] = spec.get("leg", dark(main, 0.88))
    z["tail"] = spec.get("tail", dark(main, 0.86))
    z["mane"] = spec.get("mane", dark(main, 0.74))
    z["horn"] = spec.get("horn", hexc("e8deb8"))
    z["eye"] = spec.get("eye", hexc("1c1a20"))
    z["nose"] = spec.get("nose", dark(z["snout"], 0.45))
    z["beak"] = spec.get("beak", hexc("f0a830"))
    z["comb"] = spec.get("comb", hexc("d83a32"))
    z["wattle"] = spec.get("wattle", hexc("d83a32"))
    z["udder"] = spec.get("udder", hexc("eeaaa0"))
    z["dark"] = spec.get("dark", dark(main, 0.5))
    for k in ZONE_KEYS:
        if k in spec and k not in z:
            z[k] = spec[k]
    for k in ZONE_KEYS:
        if k in spec:
            z[k] = spec[k]
    return z


def zone_colour(pal, zone):
    seen = 0
    while zone not in pal and seen < 8:
        zone = FALLBACK.get(zone, "body")
        seen += 1
    return pal.get(zone, pal["body"])


# ---------------------------------------------------------------- painting

class Ctx:
    """What a pattern may look at: the texel's place on the model."""
    __slots__ = ("pos", "fr", "n", "face", "zone", "elem", "i", "j", "w", "h", "H", "seed")


def pattern_colour(p, cx, current):
    """The colour a pattern gives the texel, or None to leave it."""
    kind = p[0]
    zones = None
    if kind == "spots":            # ("spots", colour, density, cell, zones)
        _, col, density, cell, zones = p
        if cx.zone not in zones:
            return None
        # A spot is `cell` of the model's height across, but never less than 2 pixels:
        # one unit of a model is one pixel of its picture.
        s = max(cell * cx.H, 2.0)
        n = cx.pos
        v = hsh(cx.seed, math.floor(n[0] / s), math.floor(n[1] / s), math.floor(n[2] / s))
        return col if v < density else None
    if kind == "stripes":          # ("stripes", colour, period, duty, zones): blocky bars along the body
        _, col, period, duty, zones = p
        if cx.zone not in zones:
            return None
        s = max(period * cx.H, 2.0)
        band = math.floor(cx.pos[2] / s)
        v = hsh(cx.seed, band, math.floor(cx.pos[1] / (s * 1.6)))
        return col if v < duty else None
    if kind == "hbands":           # ("hbands", colour, period, duty, zones): rings round the body (banded snakes)
        _, col, period, duty, zones = p
        if cx.zone not in zones:
            return None
        return col if (math.floor(cx.pos[2] / max(period * cx.H, 2.0)) % 3 == 0) else None
    if kind == "socks":            # ("socks", colour, height): legs below a height
        _, col, hgt = p
        if cx.zone in ("leg", "hand") and cx.n[1] < hgt:
            return col
        return None
    if kind == "blaze":            # ("blaze", colour): a stripe down the front of the face
        _, col = p
        if cx.zone == "head" and cx.face == "north" and abs(cx.fr[0] - 0.5) < 0.2:
            return col
        return None
    if kind == "mask":             # ("mask", colour): the lower half of the face
        _, col = p
        if cx.zone in ("head", "snout") and cx.face in ("north", "east", "west") and cx.fr[1] < 0.5:
            return col
        return None
    if kind == "cap":              # ("cap", colour, frac): the top of the head and neck
        _, col, frac = p
        if cx.zone == "head" and (cx.fr[1] > 1 - frac or cx.face == "up"):
            return col
        return None
    if kind == "belly":            # ("belly", colour): the underside
        _, col = p
        if cx.zone == "body" and (cx.face == "down" or (cx.face != "up" and cx.fr[1] < 0.3)):
            return col
        return None
    if kind == "back":             # ("back", colour): the top of the body
        _, col = p
        if cx.zone in ("body", "neck") and (cx.face == "up" or (cx.face != "down" and cx.fr[1] > 0.72)):
            return col
        return None
    if kind == "zones":            # ("zones", colour, zones): whole zones
        _, col, zones = p
        return col if cx.zone in zones else None
    if kind == "tip":              # ("tip", colour): the end of the tail
        _, col = p
        if cx.zone in ("tail", "tail_tip") and cx.fr[1] < 0.35:
            return col
        return None
    if kind == "band":             # ("band", colour, y0, y1, zones): a belt or stripe, as fractions of the model height
        _, col, y0, y1, zones = p
        if cx.zone in zones and y0 <= cx.n[1] < y1:
            return col
        return None
    if kind == "front":            # ("front", colour, y0, y1, zones): only on the front face (a bib, an apron)
        _, col, y0, y1, zones = p
        if cx.zone in zones and cx.face == "north" and y0 <= cx.n[1] < y1:
            return col
        return None
    if kind == "hair":             # ("hair", colour, style): style short | long | none
        _, col, style = p
        if cx.zone != "head" or style == "none":
            return None
        fy = cx.fr[1]
        if cx.face == "up":
            return col
        if cx.face == "north":
            return col if fy > 0.78 else None
        if cx.face == "down":
            return None
        if style == "long":
            return col if (cx.face == "south" and fy > 0.1) or (fy > 0.4 and cx.fr[2] > 0.3) else None
        return col if fy > 0.58 and (cx.face == "south" or cx.fr[2] > 0.15) else None
    if kind == "beard":            # ("beard", colour)
        _, col = p
        if cx.zone == "head" and cx.face in ("north", "east", "west") and cx.fr[1] < 0.3:
            return col
        return None
    if kind == "keep_top":         # ("keep_top", frac): of a hat or hood layer, only the top fraction is there
        _, frac = p
        if cx.zone == "o_hat" and not (cx.fr[1] >= 1 - frac and cx.face != "down"):
            return CLEAR
        return None
    if kind == "swap":             # ("swap", colour_from, colour_to): recolour (by current colour) anything
        _, a, b = p
        return b if current == a else None
    return None


def paint_model(model, pal, pats, eyes="pair", side_eyes=False, human=False, seed=0, remap=None):
    faces, tw, th, lo, hi = faces_of(model, remap)
    img = [[(0, 0, 0, 0)] * tw for _ in range(th)]
    H = max(hi[1] - lo[1], 1.0)
    cxm = (lo[0] + hi[0]) / 2
    czm = (lo[2] + hi[2]) / 2
    for fc in faces:
        x0, y0, x1, y1 = fc.rect
        w, h = x1 - x0, y1 - y0
        base = zone_colour(pal, fc.zone)
        if fc.zone.startswith("o_") and fc.zone not in pal:
            continue
        ex0, ey0 = min(fc.elem["from"][1], fc.elem["to"][1]), 0
        cx = Ctx()
        cx.face, cx.zone, cx.elem, cx.w, cx.h, cx.H, cx.seed = fc.face, fc.zone, fc.elem, w, h, H, seed
        colours = {}
        for j in range(h):
            for i in range(w):
                pos, fr = texel_pos(fc, i, j)
                cx.pos, cx.fr, cx.i, cx.j = pos, fr, i, j
                cx.n = ((pos[0] - cxm) / H, (pos[1] - lo[1]) / H, (pos[2] - czm) / H)
                c = base
                # The bottom third of a lower leg is the hoof (or paw, or boot).
                if "hoof" in pal and any(fc.elem["name"].lower().startswith(k) for k in LOWER_LEG) and fr[1] < 0.34 and fc.face not in ("up",):
                    c = pal["hoof"]
                if fc.face == "down" and fc.zone == "body":
                    c = pal["belly"]
                for p in pats:
                    r = pattern_colour(p, cx, c)
                    if r is not None:
                        c = r
                colours[(i, j)] = c
        # The frame: a darker ring round faces that are big enough to take it.
        framed = w >= 3 and h >= 3 and not fc.zone.startswith("o_")
        for j in range(h):
            for i in range(w):
                c = colours[(i, j)]
                if c == CLEAR:
                    continue
                if framed and (i == 0 or j == 0 or i == w - 1 or j == h - 1):
                    c = dark(c)
                img[y0 + j][x0 + i] = (c[0], c[1], c[2], 255)
        decals(img, fc, pal, eyes, side_eyes, human, lo, hi)
    return img, tw, th


def put(img, fc, i, j, c):
    x0, y0, x1, y1 = fc.rect
    if 0 <= i < x1 - x0 and 0 <= j < y1 - y0:
        img[y0 + j][x0 + i] = (c[0], c[1], c[2], 255)


def decals(img, fc, pal, eyes, side_eyes, human, lo, hi):
    x0, y0, x1, y1 = fc.rect
    w, h = x1 - x0, y1 - y0
    name = fc.elem["name"].lower()
    zone = fc.zone
    eye = pal["eye"]
    if zone == "eye":
        return
    if zone == "head" and name in ("head", "face", "skull", "cap", "dome"):
        if human and fc.face == "north" and w >= 8:
            # Two eyes (the white outside) and a mouth, as in every player skin.
            sx = w / 8.0
            ey = int(round(h * 0.5))
            put(img, fc, int(1 * sx), ey, (250, 250, 250))
            put(img, fc, int(2 * sx), ey, eye)
            put(img, fc, int(5 * sx), ey, eye)
            put(img, fc, int(6 * sx), ey, (250, 250, 250))
            for mx in (3, 4):
                put(img, fc, int(mx * sx), int(round(h * 0.78)), dark(pal["head"], 0.62))
            return
        if eyes == "none":
            return
        if fc.face == "north" and name != "cap":
            big = 2 if w >= 9 else 1
            ey = max(0, int(round(h * 0.36)) - (1 if big == 2 else 0))
            ex = max(1 if w >= 5 else 0, int(round(w * 0.2)))
            if eyes == "wide":
                ex = max(0, int(round(w * 0.14)))
            for dy in range(big):
                for dx in range(big):
                    put(img, fc, ex + dx, ey + dy, eye)
                    put(img, fc, w - 1 - ex - dx, ey + dy, eye)
        elif side_eyes and fc.face in ("east", "west"):
            # One eye on each side, near the front of the head.
            fz_front = 0.28
            for j in range(h):
                for i in range(w):
                    pos, fr = texel_pos(fc, i, j)
                    if abs(fr[2] - fz_front) < 0.5 / max(w, 1) + 0.001 and abs(fr[1] - 0.62) < 0.5 / max(h, 1) + 0.001:
                        put(img, fc, i, j, eye)
    elif zone == "snout" and fc.face == "north" and w >= 4 and h >= 3 and name in ("muzzle", "snout"):
        # Two nose holes.
        nx = max(1, int(round(w * 0.25)))
        put(img, fc, nx, 1, pal["nose"])
        put(img, fc, w - 1 - nx, 1, pal["nose"])
    elif zone == "beak" and fc.face == "north" and w >= 3 and h >= 3:
        put(img, fc, 1, 1, dark(pal["beak"], 0.6))
        put(img, fc, w - 2, 1, dark(pal["beak"], 0.6))


# ---------------------------------------------------------------- output

def write_png(path, img, tw, th):
    im = Image.new("RGBA", (tw, th))
    im.putdata([p for row in img for p in row])
    im.save(path, optimize=True)


def build(only=None, quiet=False):
    import entity_palettes as P
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(CORE_OUT, exist_ok=True)
    made = {}
    for name, (model, spec) in sorted(P.textures().items()):
        if only and not (model in only or name in only):
            continue
        pal = derive(spec)
        pats = list(spec.get("pat", []))
        img, tw, th = paint_model(model, pal, pats, spec.get("eyes", "pair"), spec.get("side_eyes", False),
                                  model.startswith("player"), name_hash(name) & 0xFFFF, spec.get("remap"))
        # The player model's pictures are skins: 64 x 64, the usual layout.
        write_png(os.path.join(CORE_OUT if is_core(name) else OUT, name + ".png"), img, tw, th)
        made[name] = (model, img, tw, th)
    if not quiet:
        print("wrote", len(made), "pictures to", OUT, "and", os.path.relpath(CORE_OUT, ROOT))
    return made


def check(made):
    """Every model has a picture, and every `tex` the data names exists."""
    import re
    have = set(made) if made else {f[:-4] for f in os.listdir(OUT) + os.listdir(CORE_OUT) if f.endswith(".png")}
    models = {f[:-8] for f in os.listdir(MODELS) + os.listdir(CORE_MODELS) if f.endswith(".bbmodel")}
    skip = {"hand", "hand_slim", "player_slim", "grazer"}
    for m in sorted(models - skip):
        if m not in have:
            print("warning: model", m, "has no base picture")
    for path in (os.path.join(ART, "animals.txt"),):
        if os.path.exists(path):
            for line in open(path):
                m = re.search(r"\btex (\w+)", line)
                if m and m.group(1) not in have:
                    print("warning:", os.path.basename(path), "names a picture that is not made:", m.group(1))


def sheet(made, path):
    """A contact sheet: every picture at 2x on a grey ground, with its name."""
    from PIL import ImageDraw
    items = sorted(made.items())
    cols = 6
    cell_w = 270
    rows = []
    x = y = 0
    cw = ch = 0
    cells = []
    for name, (model, img, tw, th) in items:
        s = 2 if tw <= 64 else 1
        cells.append((name, tw * s, th * s + 12, img, tw, th, s))
    # simple shelf packing
    W = cell_w * cols
    placed = []
    x = y = rowh = 0
    for c in cells:
        if x + c[1] + 6 > W:
            x, y, rowh = 0, y + rowh + 6, 0
        placed.append((x, y, c))
        x += c[1] + 6
        rowh = max(rowh, c[2])
    im = Image.new("RGBA", (W, y + rowh + 6), (96, 98, 104, 255))
    dr = ImageDraw.Draw(im)
    for (x, y, c) in placed:
        name, w, h, img, tw, th, s = c
        big = Image.new("RGBA", (tw * s, th * s))
        big.putdata([img[y // s][x // s] for y in range(th * s) for x in range(tw * s)])
        im.alpha_composite(big, (x, y + 12))
        dr.text((x, y), name, fill=(255, 255, 255, 255))
    im.save(path)


def template(model, path):
    """The model's texture layout: each face a different flat colour, named by the
    box's zone, so a pack maker knows where every part goes."""
    faces, tw, th, lo, hi = faces_of(model)
    img = [[(0, 0, 0, 0)] * tw for _ in range(th)]
    zones = sorted({f.zone for f in faces})
    for fc in faces:
        k = zones.index(fc.zone)
        c = tuple(int(60 + 190 * hsh(k, ch)) for ch in (1, 2, 3))
        x0, y0, x1, y1 = fc.rect
        for j in range(y0, y1):
            for i in range(x0, x1):
                edge = i in (x0, x1 - 1) or j in (y0, y1 - 1)
                img[j][i] = (*(dark(c, 0.7) if edge else c), 255)
    write_png(path, img, tw, th)
    print("zones:", ", ".join(zones))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--only", help="comma-separated model or picture names")
    ap.add_argument("--sheet", help="also write a contact sheet PNG here")
    ap.add_argument("--template", help="write the layout map of this model to <model>_layout.png and stop")
    a = ap.parse_args()
    if a.template:
        template(a.template, a.template + "_layout.png")
        return
    only = set(a.only.split(",")) if a.only else None
    made = build(only)
    if not only:
        check(made)
    if a.sheet:
        sheet(made, a.sheet)


if __name__ == "__main__":
    main()
