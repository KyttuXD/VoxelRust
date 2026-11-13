"""The colours of every creature and player: the table tools/entity_textures.py paints from.

Each entry is `sp(model, base, looks)`:

  model   the model whose texture layout is painted (data/models/<model>.bbmodel)
  base    the model's own look, as a dict of colours (see below)
  looks   {suffix: changes}: another look, written to <model>_<suffix>.png

A look is the base with the given keys replaced. If a look gives `main`, the
colours that follow from it (belly, snout, legs...) follow too, unless the
look states them; add `"new": True` to start from nothing instead of from the
base. `"pat+"` adds patterns to the base's, `"pat"` replaces them.

Colours are '#rrggbb'. Keys (all optional but `main`):

  main                    the body
  head belly snout ear leg tail mane horn eye nose beak comb wattle udder
  hoof                    the bottom of the lower legs (hooves, paws, boots)
  tail_tip wing neck arm armtop hand sleeve fin shell dark pants shoe
  o_hat o_jacket o_sleeve o_pants    the player model's overlay layers
  pat                     patterns, in order (see pattern_colour in entity_textures.py)
  eyes                    "pair" (default), "wide" (set further apart) or "none"
  side_eyes               eyes on the sides of the head (birds, fish)
  remap                   {box name prefix: zone}, for boxes the usual rules get wrong

To add a look, add a line to `looks`. To add a creature, add a `sp(...)` and,
in tools/gen_creatures.py, `tex <name>` on the creature or its variant.
Keep it to few colours: this is the OCD style (docs/TEXTURING.md).
"""

_T = {}

BODY = ("body", "neck", "head", "tail")
ALL = ("body", "neck", "head", "tail", "leg", "arm", "hand")


def hexc(s):
    s = s.lstrip("#")
    return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))


def _col(v):
    if isinstance(v, str) and v.startswith("#"):
        return hexc(v)
    if isinstance(v, (list, tuple)):
        return type(v)(_col(x) for x in v)
    return v


def _spec(d):
    return {k: _col(v) for k, v in d.items()}


def sp(model, base, looks=None, name=None):
    name = name or model
    _T[name] = (model, _spec(base))
    for suffix, ov in (looks or {}).items():
        ov = dict(ov)
        if ov.pop("new", False):
            spec = dict(ov)
            for k in ("eyes", "side_eyes", "remap"):
                if k in base and k not in spec:
                    spec[k] = base[k]
        else:
            spec = dict(base)
            # What follows from `main` follows the new main.
            if "main" in ov:
                for k in ("head", "belly", "snout", "ear", "leg", "tail", "mane", "nose", "dark", "neck", "hoof"):
                    if k not in ov:
                        spec.pop(k, None)
                # ... except what the base said about the parts that never follow it.
                for k in ("hoof", "horn", "eye", "beak", "comb", "wattle", "udder"):
                    if k in base:
                        spec[k] = base[k]
            extra = ov.pop("pat+", None)
            spec.update(ov)
            if extra:
                spec["pat"] = list(spec.get("pat", [])) + list(extra)
        _T[name + "_" + suffix] = (model, _spec(spec))


# ------------------------------------------------------------------ farm animals

WHITE, BLACK = "#f2eee6", "#2c2a30"

sp("cow", dict(main=WHITE, snout="#f0a8a0", ear="#2c2a30", horn="#efe6c6", hoof="#3c3030", tail_tip="#2c2a30",
               pat=[("spots", BLACK, 0.42, 0.13, BODY)]),
   {"brown": dict(main="#8c5a38", snout="#d8b090", belly="#c49a6c", ear="#6a4228", pat=[("blaze", WHITE)]),
    "red": dict(main="#b84a2c", snout="#f0b090", ear="#8a3420", pat=[("blaze", "#f4ece0"), ("socks", "#f4ece0", 0.1)])})
sp("cow_longhorn", dict(main="#c8905a", snout="#f0c8a8", horn="#f4ecd0", hoof="#4a3a30", pat=[("spots", WHITE, 0.3, 0.14, BODY)]),
   {"dark": dict(main="#6a3e28", pat=[("blaze", "#f2eee6")])})
sp("cow_highland", dict(main="#b8602a", mane="#7a3a1a", snout="#d89a6a", horn="#efe6c6", hoof="#3c3030"),
   {"dun": dict(main="#d4b484", mane="#9a7c50", snout="#e8d0a8"),
    "black": dict(main="#34302e", mane="#1e1c1e", snout="#5a504c"),
    "red": dict(main="#8c3a22", mane="#5a2214", snout="#c88a66")})
sp("bull", dict(main="#5a3a28", snout="#a07858", horn="#efe6c6", hoof="#2c2420", mane="#3a2418"),
   {"black": dict(main="#2c2a2e", snout="#5a504c", mane="#1a181c"),
    "red": dict(main="#a8482c", snout="#e0a888", mane="#6a2c1a"),
    "pied": dict(main=WHITE, snout="#f0a8a0", mane="#2c2a30", pat=[("spots", "#2c2a30", 0.45, 0.14, BODY)])})
sp("bison", dict(main="#5c3c28", mane="#3a2418", snout="#7a5a44", horn="#d8cca4", hoof="#2c2420"),
   {"tan": dict(main="#8a6444", mane="#5a3c26", snout="#a88a68"),
    "dark": dict(main="#3a2c26", mane="#221a16", snout="#5a4a40"),
    "pale": dict(main="#a08a70", mane="#6a5a44", snout="#c0aa8c")})
sp("yak", dict(main="#3a2c2a", mane="#241c1a", snout="#5a4a46", horn="#d8cca4", hoof="#201816", pat=[("back", "#2a201e")]),
   {"brown": dict(main="#6a4a34", mane="#44301e", snout="#8a6a50", pat=[]),
    "cream": dict(main="#d8c8a8", mane="#a89878", snout="#ecdcc0", pat=[]),
    "grey": dict(main="#8c8c94", mane="#5a5a64", snout="#acacb4", pat=[])})
sp("pig", dict(main="#f0a8a4", snout="#e08a8c", ear="#e08a8c", leg="#e8989a", hoof="#6a4448"),
   {"spotted": dict(pat=[("spots", "#5a3c3c", 0.3, 0.16, ("body", "head", "neck"))]),
    "black": dict(main="#4a4448", snout="#6a5a5e", ear="#3a3438", hoof="#2a2428")})
sp("pig_boar", dict(main="#6e5a48", snout="#4a3a30", mane="#3e3228", horn="#efe6c6", hoof="#2c2420", pat=[("back", "#3e3228")]),
   {"grey": dict(main="#84807c", snout="#5a5652", mane="#4a4642", pat=[("back", "#4a4642")]),
    "black": dict(main="#3a3434", snout="#2a2424", mane="#1c1818", pat=[("back", "#1c1818")]),
    "ginger": dict(main="#8a5a38", snout="#5a3a22", mane="#4a2c18", pat=[("back", "#4a2c18")])})
WOOL = "#f0ece4"
sp("sheep", dict(main=WOOL, head="#cbb08a", leg="#cbb08a", ear="#b8986c", snout="#d8c0a0", hoof="#4a3c34", remap={"cap": "body"}),
   {"black": dict(main="#3a383c", head="#2a282c", leg="#2a282c", ear="#1e1c20", snout="#4a484c"),
    "grey": dict(main="#a4a4aa", head="#5a5a60", leg="#5a5a60", ear="#4a4a50", snout="#7a7a80"),
    "brown": dict(main="#8a6848", head="#5a4030", leg="#5a4030", ear="#4a3426", snout="#7a5a42")})
sp("sheep_ram", dict(main="#e8e0d0", head="#a89070", leg="#a89070", ear="#8a7458", horn="#b89c6a", snout="#c0aa88", hoof="#3c3028", remap={"cap": "body"}))
sp("sheep_merino", dict(main="#f4f0e6", head="#d8c4a0", leg="#d8c4a0", ear="#c0a67c", snout="#e4d4b8", hoof="#4a3c34", remap={"cap": "body"}),
   {"grey": dict(main="#b4b4ba", head="#6a6a70", leg="#6a6a70", ear="#5a5a60", snout="#8a8a90"),
    "brown": dict(main="#a88060", head="#6a4a34", leg="#6a4a34", ear="#5a3e2a", snout="#8a6a50"),
    "black": dict(main="#404044", head="#2a2a2e", leg="#2a2a2e", ear="#1e1e22", snout="#58585c")})
sp("goat", dict(main="#e8e4dc", mane="#c8c4bc", horn="#d8cca4", snout="#f4f0e8", hoof="#3c3430"),
   {"brown": dict(main="#8a6040", mane="#5a3c24", snout="#c4a07c"),
    "black": dict(main="#3a383c", mane="#242226", snout="#6a666a"),
    "pied": dict(main="#f0ece4", mane="#c8c4bc", pat=[("spots", "#8a6040", 0.34, 0.15, BODY)])})
sp("chicken", dict(main="#f4f2ea", leg="#e0a830", beak="#f0a830", snout="#f0a830", tail="#e8e4da"),
   {"brown": dict(main="#a86a38", tail="#8a5228", wing="#8a5228"),
    "black": dict(main="#34343a", tail="#26262a", wing="#26262a", leg="#6a6a72", beak="#8a8a90")})
sp("chicken_rooster", dict(main="#a8502a", head="#c8602a", mane="#d88a2a", tail="#2a4a3a", wing="#8a3e20", leg="#e0a830", beak="#f0a830"))
sp("duck", dict(main="#f6f4ec", beak="#f0a030", leg="#f0a030", tail="#e8e4da"),
   {"mallard": dict(main="#8e7050", head="#2e7a50", neck="#2e7a50", belly="#c8c0b0", tail="#4a4048", wing="#7a6048", beak="#e8c830", leg="#e08a30"),
    "brown": dict(main="#8a6a48", head="#6a4a30", tail="#6a4a30", wing="#6a4a30", beak="#d89a38", leg="#d89a38"),
    "black": dict(main="#2a3a3a", head="#2a3a3a", wing="#2e5a48", tail="#26302e", beak="#3a3a3a", leg="#3a3a3a")})
sp("goose", dict(main="#f4f2ec", beak="#f0902a", leg="#f0902a", tail="#e4e0d6"),
   {"grey": dict(main="#a8a49c", head="#7a766e", neck="#8a867e", belly="#d8d4cc", wing="#8a867e", beak="#e89830", leg="#e89830"),
    "brown": dict(main="#8a7a68", head="#5a4a3a", neck="#6a5a4a", belly="#c8baa4", wing="#6a5a4a", beak="#2a2a2e", leg="#2a2a2e"),
    "canada": dict(main="#8a7860", head="#26262a", neck="#26262a", belly="#e8e0d0", wing="#6a5a46", beak="#26262a", leg="#26262a",
                   pat=[("blaze", "#f2f0e8")])})
sp("turkey", dict(main="#6a4a2e", head="#c04a44", neck="#c04a44", wing="#4a3220", tail="#8a6a3e", beak="#d8a838", leg="#d88a40", wattle="#d83a32"),
   {"white": dict(main="#f2f0ea", wing="#dcd8cc", tail="#e8e4d8"),
    "black": dict(main="#2e2c30", wing="#1e1c20", tail="#3a3840", head="#6a8aa8", neck="#6a8aa8"),
    "wild": dict(main="#8a6a40", wing="#5a4228", tail="#b8944e")})
sp("horse", dict(main="#a85c2c", mane="#5a3018", tail="#5a3018", snout="#c8864e", hoof="#3a2c28"),
   {"bay": dict(main="#7a4224", mane="#1e1a1a", tail="#1e1a1a", leg="#26201e", hoof="#1e1a1a", snout="#5a3420"),
    "black": dict(main="#2c2a30", mane="#1a181c", tail="#1a181c", snout="#4a484e", hoof="#141214"),
    "palomino": dict(main="#e0b068", mane="#f4ecd4", tail="#f4ecd4", snout="#f0d098", hoof="#8a7a68"),
    "grey": dict(main="#d8d8dc", mane="#a0a0aa", tail="#a0a0aa", snout="#b8b8c0", hoof="#5a5a62",
                 pat=[("spots", "#b4b4be", 0.3, 0.1, ("body", "neck", "leg"))])})
sp("horse_pony", dict(main=WHITE, mane="#4a3020", tail="#4a3020", snout="#e8c8a8", hoof="#3a2c28",
                      pat=[("spots", "#8a5430", 0.4, 0.2, BODY)]),
   {"chestnut": dict(main="#b87a44", mane="#e8dcc0", tail="#e8dcc0", snout="#d8a878", pat=[]),
    "black": dict(main="#34323a", mane="#1a181c", tail="#1a181c", snout="#5a585e", pat=[])})
sp("donkey", dict(main="#8c8c94", belly="#d8d4cc", snout="#d8d4cc", mane="#4a4a52", ear="#6a6a72", hoof="#2c2c30", pat=[("back", "#5a5a62")]),
   {"brown": dict(main="#7a5a40", belly="#d0b898", snout="#c8aa88", mane="#4a3626", ear="#5a4230", pat=[("back", "#4a3626")]),
    "white": dict(main="#e4e0dc", belly="#f4f0ec", snout="#f0d8d0", mane="#b8b4b0", ear="#c8c4c0", pat=[]),
    "dark": dict(main="#4a4650", belly="#6a6670", snout="#7a767e", mane="#26242a", ear="#34323a", pat=[])})
sp("llama", dict(main="#e6d4b0", mane="#c8b48c", snout="#f0e4c8", ear="#c8b48c", hoof="#5a4a40"),
   {"brown": dict(main="#8a6244", mane="#5a3e28", snout="#b08a68"),
    "black": dict(main="#3a363a", mane="#242226", snout="#5a565a"),
    "patchy": dict(main="#f2eee6", mane="#d8c8a8", snout="#f0e4c8", pat=[("spots", "#8a6244", 0.34, 0.15, BODY)])})
sp("camel", dict(main="#c8a060", mane="#8a6838", snout="#e0c088", hoof="#5a4a38"),
   {"dark": dict(main="#7a5a38", mane="#4a3620", snout="#a88458"),
    "cream": dict(main="#e4d4aa", mane="#b8a478", snout="#f0e4c4"),
    "grey": dict(main="#a8a090", mane="#6a6458", snout="#c8c0b0")})
sp("elephant", dict(main="#8c8c98", ear="#b8909a", horn="#f2eee0", snout="#a8a8b4", hoof="#e8e4d8"),
   {"brown": dict(main="#8a6a54", ear="#a8806a", snout="#a88a74"),
    "pale": dict(main="#c8c4c8", ear="#e0b0b4", snout="#d8d4d8"),
    "dusk": dict(main="#6a6a7e", ear="#8a7a8a", snout="#8a8a9c")})

# ------------------------------------------------------------------ pets and small animals

sp("dog", dict(main="#d8a050", belly="#f0d8a8", snout="#f0d8a8", ear="#a8742e", nose="#2a2a2e", tail_tip="#f0d8a8"),
   {"brown": dict(main="#8a5a38", belly="#c8a07c", snout="#c8a07c", ear="#5a3a22", tail_tip="#c8a07c"),
    "grey": dict(main="#9ea4b2", belly="#dcdee6", snout="#dcdee6", ear="#6a707e", tail_tip="#dcdee6"),
    "tan": dict(main="#2c2a30", belly="#c88c48", snout="#c88c48", leg="#c88c48", ear="#1e1c20", tail_tip="#c88c48")})
sp("dog_shepherd", dict(main="#b88448", belly="#e0c088", snout="#2c2a30", ear="#2c2a30", leg="#b88448", nose="#101012", pat=[("back", "#2c2a30")]),
   {"sable": dict(main="#a06a38", belly="#d8b078", pat=[("back", "#4a3424")]), })
sp("dog_terrier", dict(main=WHITE, belly="#fafaf6", snout="#f4ece0", ear="#a8683a", nose="#2a2a2e", pat=[("spots", "#a8683a", 0.3, 0.18, BODY)]),
   {"black": dict(pat=[("spots", "#34323a", 0.3, 0.18, BODY)], ear="#34323a")})
sp("dog_hound", dict(main="#c8905a", belly="#f2eee6", snout="#f2eee6", ear="#6a4428", leg="#f2eee6", nose="#2a2a2e", tail_tip="#f2eee6",
                     pat=[("blaze", "#f2eee6")]),
   {"lemon": dict(main="#e0c070", ear="#a8844a")})
sp("cat", dict(main="#e08a3a", belly="#f8e4c4", snout="#f8e4c4", ear="#b8641e", nose="#e88a8a", eye="#4aa84a", leg="#d07a2e",
               pat=[("stripes", "#b8641e", 0.09, 0.45, BODY)]),
   {"black": dict(main="#302e36", belly="#46444e", snout="#46444e", ear="#201e24", eye="#e8c830", nose="#6a5a66", leg="#2a2830", pat=[]),
    "cream": dict(main="#f0dcb0", belly="#faf0d8", snout="#faf0d8", ear="#d8c090", leg="#e8d0a0", pat=[("stripes", "#d8b880", 0.09, 0.4, BODY)]),
    "grey": dict(main="#8c8c98", belly="#d4d4dc", snout="#d4d4dc", ear="#6a6a78", leg="#7c7c88", pat=[("stripes", "#6a6a78", 0.09, 0.45, BODY)])})
sp("cat_fluffy", dict(main="#f4f0ea", belly="#fffcf6", snout="#fffcf6", ear="#d8d0c4", nose="#e88a8a", eye="#4a88d8", pat=[("spots", "#a8a4a0", 0.28, 0.15, BODY)]),
   {"cream": dict(main="#f0dcb0", belly="#faf0d8", snout="#faf0d8", ear="#d8c090", pat=[]),
    "smoke": dict(main="#6a6a74", belly="#a0a0aa", snout="#a0a0aa", ear="#4a4a54", pat=[])})
sp("cat_lynx", dict(main="#c8a070", belly="#f0e4cc", snout="#f0e4cc", ear="#302e30", nose="#a86a6a", eye="#e8c830", tail_tip="#26242a",
                    pat=[("spots", "#6a4a30", 0.22, 0.07, BODY)]),
   {"snow": dict(main="#e0dcd4", belly="#f8f6f2", snout="#f8f6f2", pat=[("spots", "#8a8680", 0.22, 0.07, BODY)])})
sp("fox", dict(main="#d8682a", belly="#f4ece0", snout="#f4ece0", ear="#2a2428", leg="#2a2428", hoof="#2a2428", tail_tip="#f4f0ea", nose="#1c1a1e",
               pat=[("tip", "#f4f0ea")]),
   {"arctic": dict(main="#f4f6fa", belly="#fcfcfe", snout="#fcfcfe", ear="#d4d8e0", leg="#d4d8e0", hoof="#d4d8e0", pat=[("tip", "#fcfcfe")]),
    "silver": dict(main="#8a8e9a", belly="#c8ccd4", snout="#c8ccd4", ear="#2a2c34", leg="#3a3c46", hoof="#3a3c46"),
    "dark": dict(main="#8a4a22", belly="#d8b48a", snout="#d8b48a", ear="#26201e", leg="#26201e", hoof="#26201e")})
sp("wolf", dict(main="#7a7e88", belly="#d4d6dc", snout="#b0b2b8", ear="#585c66", nose="#1c1c20", leg="#6a6e78", pat=[("back", "#585c66")]),
   {"arctic": dict(main="#f2f4f8", belly="#fcfcfe", snout="#dcdee6", ear="#d0d4dc", leg="#e4e6ee", pat=[]),
    "black": dict(main="#34343c", belly="#54545e", snout="#54545e", ear="#24242a", leg="#2c2c34", pat=[("back", "#24242a")]),
    "timber": dict(main="#8a7a68", belly="#d8ccb8", snout="#c0b4a0", ear="#6a5a48", leg="#7a6a58", pat=[("back", "#6a5a48")])})
sp("rabbit", dict(main="#8a7058", belly="#e8dcc8", snout="#d8c8b0", ear="#6a5444", tail="#f4efe8", nose="#e08a8a"),
   {"white": dict(main="#f4f2ee", belly="#ffffff", snout="#f8f4f0", ear="#e8b8b8", tail="#ffffff", eye="#c83a4a"),
    "brown": dict(main="#8a5a38", belly="#d8b898", snout="#c8a07c", ear="#6a4228", tail="#f0e4d4"),
    "black": dict(main="#38363a", belly="#58565a", snout="#58565a", ear="#26242a", tail="#6a686c")})
sp("rabbit_lop", dict(main="#9a9aa2", belly="#d8d8de", snout="#d0d0d8", ear="#7a7a84", tail="#f4f2f0"),
   {"sandy": dict(main="#d8b890", belly="#f0dcc0", snout="#e8d0b0", ear="#b89868", tail="#f8f0e4"),
    "white": dict(main="#f4f2ee", belly="#ffffff", snout="#f8f4f0", ear="#f0c8c8", tail="#ffffff", eye="#c83a4a"),
    "dutch": dict(main=WHITE, belly="#ffffff", snout="#f8f4f0", ear="#7a5238", tail="#ffffff", pat=[("spots", "#7a5238", 0.32, 0.16, BODY)])})
sp("mouse", dict(main="#9a98a0", belly="#d8d4d0", snout="#c8c4c4", ear="#e8b0b0", tail="#e8b8b0", nose="#e88a8a"),
   {"brown": dict(main="#8a6848", belly="#d8c0a0", snout="#b89a78", ear="#e0a898"),
    "white": dict(main="#f4f2f0", belly="#ffffff", snout="#f8f2f0", ear="#f0b8b8", eye="#c83a4a")})
sp("deer", dict(main="#b27840", belly="#f0e4cc", snout="#e8d0b0", ear="#8a5a30", horn="#d8c8a0", hoof="#3a2c24", tail="#f4ecd8"),
   {"pale": dict(main="#d8bc90", belly="#f8f0e0", snout="#f0e0c4", ear="#b8986c"),
    "spotted": dict(pat=[("spots", "#f4ecd8", 0.3, 0.07, ("body", "neck"))]),
    "dark": dict(main="#6a4428", belly="#b8986c", snout="#a88458", ear="#4a2e1a")})
sp("monkey", dict(main="#7a5030", head="#7a5030", snout="#e8c8a0", belly="#d8b080", ear="#e8c8a0", tail="#6a4228", hand="#e8c8a0"),
   {"golden": dict(main="#d8a048", belly="#f0d098", snout="#f4dcb0", ear="#f4dcb0", hand="#f4dcb0"),
    "black": dict(main="#2e2a2c", belly="#46403e", snout="#6a5a54", ear="#6a5a54", hand="#6a5a54"),
    "grey": dict(main="#8a8a94", belly="#d0d0d8", snout="#d8d0c8", ear="#d8d0c8", hand="#d8d0c8")})
sp("bear", dict(main="#6a4630", snout="#b88e66", ear="#523420", nose="#1c1a1c", hoof="#3a2a20"),
   {"black": dict(main="#2c2a30", snout="#6a5448", ear="#201e24", hoof="#1a181c"),
    "cinnamon": dict(main="#a8703c", snout="#d8b080", ear="#8a5a2c", hoof="#5a3e24")})
sp("bear", dict(main="#f4f4f6", snout="#dcdee6", ear="#d8dae2", nose="#1c1a1c", hoof="#c8cad2"),
   {"cream": dict(main="#efe6d0", snout="#d8ccb0", ear="#d4c8ac", hoof="#bcb298"),
    "frosted": dict(main="#cfe2ee", snout="#a8c4d8", ear="#a8c4d8", hoof="#90acc0")}, name="bear_polar")
sp("tiger", dict(main="#e07a28", belly="#f8ecd8", snout="#f8ecd8", ear="#2a2428", nose="#e08a8a", eye="#e8c830",
                 pat=[("stripes", "#2a2428", 0.06, 0.55, ALL)]),
   {"white": dict(main="#f4f2ee", belly="#ffffff", snout="#ffffff", ear="#2a2428", eye="#5a9ad8", pat=[("stripes", "#6a6a7a", 0.06, 0.5, ALL)]),
    "golden": dict(main="#e8b860", belly="#faf0d8", snout="#faf0d8", pat=[("stripes", "#8a5a28", 0.06, 0.5, ALL)]),
    "dark": dict(main="#8a4a1c", belly="#d8b08a", snout="#d8b08a", pat=[("stripes", "#201a18", 0.06, 0.6, ALL)])})

# ------------------------------------------------------------------ reptiles, water, bugs

sp("gator", dict(main="#4a7a3a", belly="#c8c090", snout="#5a8a46", eye="#e8d030", horn="#f2eee0", pat=[("back", "#2e5a2a")]),
   {"dark": dict(main="#34502e", belly="#98a078", snout="#425e38", pat=[("back", "#22401e")]),
    "olive": dict(main="#7a8a40", belly="#d8d8a0", snout="#8a9a50", pat=[("back", "#5a6a2e")]),
    "albino": dict(main="#f2eee4", belly="#fffcf4", snout="#f8f2ea", eye="#d8506a", pat=[("back", "#e4dccc")])})
sp("snake", dict(main="#4c9a3a", belly="#e0e0a0", head="#3e8a2e", comb="#d83a32", pat=[("spots", "#2e6a2a", 0.4, 0.06, BODY)]),
   {"sand": dict(main="#d8b878", head="#c8a460", pat=[("spots", "#a8803c", 0.4, 0.06, BODY)]),
    "coral": dict(main="#d03a30", head="#26222a", pat=[("hbands", "#26222a", 0.07, 1, BODY), ("hbands", "#f0d040", 0.07, 1, ())]),
    "black": dict(main="#2e2c34", head="#26242a", belly="#6a6870", pat=[])})
sp("turtle", dict(main="#8aa058", shell="#6a8a3c", belly="#d8d0a0", head="#8aa058", pat=[("spots", "#4a6a2a", 0.3, 0.08, ("shell",))]),
   {"brown": dict(shell="#8a6a3a", main="#a89a68", head="#a89a68", pat=[("spots", "#5a4426", 0.3, 0.08, ("shell",))]),
    "sea": dict(shell="#3a8a8a", main="#6aa8a0", head="#6aa8a0", pat=[("spots", "#26606a", 0.3, 0.08, ("shell",))])})
sp("crab", dict(main="#d85a38", shell="#d85a38", arm="#c04a2c", eye="#1c1a20", belly="#f0b090"),
   {"blue": dict(main="#4a7ad8", shell="#4a7ad8", arm="#3a62b8", belly="#a8c0f0"),
    "sand": dict(main="#d8b070", shell="#d8b070", arm="#b8904e", belly="#f0dcb0"),
    "purple": dict(main="#8a4ab0", shell="#8a4ab0", arm="#6a3a8a", belly="#c8a0e0")})
sp("frog", dict(main="#5aa83a", belly="#e8e8b0", snout="#6ab84a", pat=[("spots", "#3a7a2a", 0.28, 0.07, BODY)], eyes="wide"),
   {"red": dict(main="#d84a38", belly="#f8d8b0", leg="#4a6ad8", arm="#4a6ad8", pat=[]),
    "blue": dict(main="#3a80e0", belly="#d8e8f8", pat=[("spots", "#26509a", 0.28, 0.07, BODY)]),
    "marsh": dict(main="#a88a3a", belly="#f0e4b0", pat=[("spots", "#5a4a28", 0.34, 0.07, BODY)])})
sp("frog", dict(main="#f0dce4", belly="#fff4f6", snout="#f8e8ec", eye="#1c1a20", eyes="wide", pat=[("spots", "#d8b0bc", 0.28, 0.07, BODY)]),
   {"spotted": dict(main="#e8d0b8", belly="#f8ecdc", pat=[("spots", "#a88a70", 0.3, 0.07, BODY)]),
    "grey": dict(main="#c4c4cc", belly="#eaeaf0", pat=[("spots", "#9a9aa6", 0.28, 0.07, BODY)])}, name="frog_newt")
sp("fish", dict(main="#b8c4d0", belly="#f0f4f8", fin="#8a9aae", pat=[("back", "#6a7a90")], side_eyes=True, eyes="none"),
   {"orange": dict(main="#f08a30", belly="#f8c888", fin="#f8b860", pat=[("back", "#c8641e")]),
    "blue": dict(main="#4a82d8", belly="#a8c8f0", fin="#7aa8e8", pat=[("back", "#2e5aa8")]),
    "green": dict(main="#5aaa58", belly="#c0e4a8", fin="#8ac878", pat=[("back", "#3a7a3a")]),
    "big": dict(main="#c8b070", belly="#f0e4b8", fin="#a8904a", pat=[("back", "#8a7238")]),
    "tiny": dict(main="#f0d040", belly="#faf0a8", fin="#f8e47a", pat=[("back", "#c8a828")])})
sp("squid", dict(main="#c06a9a", belly="#e8a8c8", fin="#a8527e", side_eyes=True, eyes="none", leg="#a8527e"),
   {"red": dict(main="#d84a44", belly="#f0928c", fin="#b0342e", leg="#b0342e"),
    "giant": dict(main="#4a4a98", belly="#8a8ad0", fin="#34347a", leg="#34347a"),
    "teal": dict(main="#3a9a98", belly="#8ad4cc", fin="#287a78", leg="#287a78")})
sp("penguin", dict(main="#26262e", belly="#f4f4f6", beak="#f0902a", leg="#f0902a", mane="#f0c038",
                   pat=[("front", "#f4f4f6", 0.0, 1.0, ("body", "neck"))], side_eyes=True),
   {"emperor": dict(pat=[("front", "#f4f4f6", 0.0, 1.0, ("body", "neck")), ("zones", "#f0c038", ("neck",))]),
    "little": dict(main="#4a5a78", pat=[("front", "#f4f4f6", 0.0, 1.0, ("body", "neck"))]),
    "rockhopper": dict(main="#2a2a30", beak="#d83a32", leg="#f0a0a0", pat=[("front", "#f4f4f6", 0.0, 1.0, ("body", "neck"))])})
sp("worm", dict(main="#e89a9a", belly="#f4c4c0", tail="#d88a8a", pat=[("hbands", "#d48484", 0.07, 1, ("body", "tail"))], eyes="none"),
   {"brown": dict(main="#8a6a50", belly="#a88a70", tail="#7a5a42", pat=[("hbands", "#6a4e3a", 0.07, 1, ("body", "tail"))]),
    "pale": dict(main="#f0d8c8", belly="#faeade", tail="#e0c8b8", pat=[("hbands", "#e0c4b0", 0.07, 1, ("body", "tail"))]),
    "green": dict(main="#8ab058", belly="#b0d080", tail="#7a9c4a", pat=[("hbands", "#6a9040", 0.07, 1, ("body", "tail"))])})
sp("ant", dict(main="#2a2428", leg="#1e1a1c", eye="#e8e0d8"),
   {"red": dict(main="#b8402a", leg="#8a2e1e"), "brown": dict(main="#6a4028", leg="#4a2c1a"), "black": dict(main="#34303a", leg="#24202a")})
sp("beetle", dict(main="#6a4228", leg="#3e2616", horn="#3e2616", eye="#e8e0d8"),
   {"green": dict(main="#3aa858", leg="#1e6a34", horn="#1e6a34"),
    "blue": dict(main="#3a62d8", leg="#24408a", horn="#24408a"),
    "gold": dict(main="#e0b030", leg="#8a6a1a", horn="#8a6a1a")})
sp("beetle", dict(main="#2a3040", leg="#1a1e2a", horn="#1a1e2a", pat=[("back", "#6af0e0")]),
   {"green": dict(pat=[("back", "#9af060")]), "amber": dict(pat=[("back", "#f0b840")])}, name="beetle_glow")
sp("spider", dict(main="#4a3a34", leg="#2e2420", eye="#d83030", pat=[("spots", "#6a5248", 0.3, 0.06, ("body",))]),
   {"black": dict(main="#26242a", leg="#18161a", pat=[]),
    "tan": dict(main="#a88860", leg="#6a5030", pat=[("spots", "#c8a878", 0.3, 0.06, ("body",))]),
    "green": dict(main="#5a7a3a", leg="#34502a", pat=[("spots", "#8aa850", 0.3, 0.06, ("body",))])})
sp("spider", dict(main="#3a3a58", leg="#26263c", eye="#e8e030", pat=[("spots", "#5a5a8a", 0.3, 0.06, ("body",))]),
   {"violet": dict(main="#5a3a6a", leg="#3a2448", pat=[("spots", "#8a5aa0", 0.3, 0.06, ("body",))]),
    "teal": dict(main="#2e5a62", leg="#1e3c42", pat=[("spots", "#4a8a90", 0.3, 0.06, ("body",))]),
    "ash": dict(main="#6a6a74", leg="#44444c", pat=[("spots", "#8e8e98", 0.3, 0.06, ("body",))])}, name="spider_cave")
sp("bee", dict(main="#f0c030", head="#2e2a28", wing="#dce8f0", leg="#2e2a28", pat=[("stripes", "#2e2a28", 0.06, 0.5, ("body",))]),
   {"honey": dict(main="#e08a28", pat=[("stripes", "#3a2a20", 0.06, 0.5, ("body",))]),
    "pale": dict(main="#f4e08a", pat=[("stripes", "#6a5a38", 0.06, 0.5, ("body",))]),
    "dark": dict(main="#8a6a28", pat=[("stripes", "#201a14", 0.06, 0.5, ("body",))])})
sp("bee", dict(main="#2e2a2a", head="#1e1a1a", wing="#dce8f0", leg="#1e1a1a", pat=[("stripes", "#d4f060", 0.07, 0.55, ("body",))]),
   {"amber": dict(pat=[("stripes", "#f0b840", 0.07, 0.55, ("body",))]),
    "cyan": dict(pat=[("stripes", "#6af0e0", 0.07, 0.55, ("body",))])}, name="firefly")
sp("butterfly", dict(main="#2a2428", head="#2a2428", wing="#f0922a", dark="#2a2428", pat=[("spots", "#2a2428", 0.28, 0.035, ("wing",))]),
   {"blue": dict(wing="#4a8af0", pat=[("spots", "#1e3a7a", 0.28, 0.035, ("wing",))]),
    "white": dict(wing="#f4f4f6", pat=[("spots", "#7a7a8a", 0.28, 0.035, ("wing",))]),
    "violet": dict(wing="#a45ae0", pat=[("spots", "#4a2a7a", 0.28, 0.035, ("wing",))]),
    "lime": dict(wing="#9ae04a", pat=[("spots", "#3a6a1e", 0.28, 0.035, ("wing",))])})
sp("bird", dict(main="#8a6a48", belly="#d8c8a8", wing="#6a4a30", tail="#6a4a30", beak="#e0a030", leg="#c8903a", side_eyes=True, eyes="none"),
   {"blue": dict(main="#4a7ad8", belly="#f0f0f4", wing="#2e52a8", tail="#2e52a8"),
    "red": dict(main="#d8443a", belly="#e8786a", wing="#a82e28", tail="#a82e28", head="#d8443a", pat=[("mask", "#2a2428")], beak="#f0b840"),
    "yellow": dict(main="#f0d040", belly="#faf0a0", wing="#8a7a28", tail="#8a7a28"),
    "green": dict(main="#5ab85a", belly="#b8e0a0", wing="#348a3a", tail="#348a3a")})
sp("owl", dict(main="#8a6a48", belly="#e0d0b0", head="#8a6a48", wing="#6a4a30", tail="#6a4a30", beak="#e8b030", eye="#f0c828", eyes="wide",
               pat=[("spots", "#6a4a30", 0.3, 0.05, ("body",))]),
   {"snowy": dict(main="#f4f4f6", belly="#ffffff", wing="#e0e0e6", tail="#e0e0e6", pat=[("spots", "#4a4a52", 0.22, 0.05, ("body", "wing"))]),
    "barn": dict(main="#c89a58", belly="#f4ecd8", head="#f4ecd8", wing="#a87a3e", tail="#a87a3e", pat=[]),
    "grey": dict(main="#7a7a84", belly="#c8c8d0", wing="#5a5a64", tail="#5a5a64", pat=[("spots", "#4a4a54", 0.3, 0.05, ("body",))])})
sp("bat", dict(main="#4a3a40", wing="#3a2c32", ear="#5a464e", eye="#e8d030", belly="#6a5560"),
   {"black": dict(main="#2a2630", wing="#201c26", ear="#34303a", belly="#46404e"),
    "grey": dict(main="#7a7a86", wing="#5e5e6a", ear="#8e8e9a", belly="#a8a8b4"),
    "pale": dict(main="#d8d0d8", wing="#b8b0be", ear="#e8e0e8", belly="#f0e8f0", eye="#d83a4a")})
sp("slime", dict(main="#5ac85a", dark="#3a9a3a", eye="#1c1a20", pat=[("spots", "#7ae07a", 0.25, 0.1, ("body",))]),
   {"blue": dict(main="#4a8af0", pat=[("spots", "#7aaaf8", 0.25, 0.1, ("body",))]),
    "small": dict(main="#98f098", pat=[("spots", "#b8f8b8", 0.25, 0.1, ("body",))]),
    "big": dict(main="#2e8a3e", pat=[("spots", "#4aaa58", 0.25, 0.1, ("body",))])})
sp("slime", dict(main="#e0501a", eye="#1c1a20", pat=[("spots", "#f8a830", 0.3, 0.1, ("body",))]),
   {"dark": dict(main="#5a2a22", pat=[("spots", "#e0501a", 0.2, 0.1, ("body",))]),
    "bright": dict(main="#f8c838", pat=[("spots", "#f8e888", 0.3, 0.1, ("body",))])}, name="slime_magma")
sp("crawler", dict(main="#4a3260", leg="#34224a", eye="#e8e030", pat=[("spots", "#6a4a88", 0.3, 0.07, BODY)]),
   {"grey": dict(main="#6a6a78", leg="#46465a", pat=[("spots", "#8a8a98", 0.3, 0.07, BODY)]),
    "green": dict(main="#3a6a48", leg="#244a30", pat=[("spots", "#5a8a66", 0.3, 0.07, BODY)]),
    "red": dict(main="#7a3030", leg="#521e1e", pat=[("spots", "#a84a4a", 0.3, 0.07, BODY)])})
sp("crawler", dict(main="#9ad4f0", leg="#6aa8d0", eye="#26508a", pat=[("spots", "#d8f0fa", 0.3, 0.07, BODY)]),
   {"white": dict(main="#eaf4fa", leg="#bcd4e4", pat=[("spots", "#ffffff", 0.3, 0.07, BODY)]),
    "teal": dict(main="#5ab8c8", leg="#3a8a9a", pat=[("spots", "#8ae0e8", 0.3, 0.07, BODY)]),
    "deep": dict(main="#5a7ac8", leg="#3a5aa0", pat=[("spots", "#8aa4e8", 0.3, 0.07, BODY)])}, name="crawler_frost")
sp("golem", dict(main="#8a8a90", belly="#6a6a72", head="#9a9aa2", leg="#7a7a82", eye="#f0c030", pat=[("spots", "#a8a8b0", 0.3, 0.1, ALL)]),
   {"mossy": dict(pat=[("spots", "#6a8a5a", 0.4, 0.1, ALL)]),
    "crystal": dict(main="#6a8ac8", head="#7a9ad8", leg="#5a78b0", belly="#4a68a0", eye="#fafafa", pat=[("spots", "#aaf0ff", 0.3, 0.1, ALL)]),
    "basalt": dict(main="#3e3e48", head="#4a4a56", leg="#34343e", belly="#2c2c36", eye="#f08a30", pat=[("spots", "#56566a", 0.3, 0.1, ALL)])})

# ------------------------------------------------------------------ night creatures

sp("skeleton", dict(main="#e8e4d4", leg="#d8d4c4", arm="#d8d4c4", hand="#e8e4d4", eye="#1a1818", eyes="wide", belly="#b8b4a4",
                    pat=[("band", "#b8b4a4", 0.5, 0.56, ("body",)), ("band", "#b8b4a4", 0.62, 0.68, ("body",))]),
   {"mossy": dict(main="#b8c4a0", leg="#a8b490", arm="#a8b490", hand="#b8c4a0", belly="#8a9a78", pat=[("spots", "#6a8a5a", 0.3, 0.07, ALL)]),
    "charred": dict(main="#5a5654", leg="#4a4644", arm="#4a4644", hand="#5a5654", belly="#3a3634", pat=[]),
    "bleached": dict(main="#f6f4ea", leg="#ece8da", arm="#ece8da", hand="#f6f4ea", belly="#d4d0c0", pat=[])})
sp("skeleton_hound", dict(main="#e8e4d4", leg="#d8d4c4", eye="#d83a2a", eyes="wide", belly="#b8b4a4"),
   {"charred": dict(main="#5a5654", leg="#4a4644", belly="#3a3634"),
    "mossy": dict(main="#b8c4a0", leg="#a8b490", belly="#8a9a78", pat=[("spots", "#6a8a5a", 0.3, 0.07, ALL)])})
sp("zombie", dict(main="#3a8aa8", head="#6a9a5a", arm="#6a9a5a", hand="#6a9a5a", sleeve="#3a8aa8", leg="#4a4a8a", mane="#2e3a24", eye="#1c1a20",
                  tail="#3a8aa8", pat=[("band", "#2e6a82", 0.5, 0.54, ("body",))]),
   {"pale": dict(main="#7a4a3a", head="#a8b8a0", arm="#a8b8a0", hand="#a8b8a0", sleeve="#7a4a3a", leg="#4a3a34", tail="#7a4a3a", mane="#3a3a34", pat=[]),
    "drowned": dict(main="#3a6a58", head="#5a9a8a", arm="#5a9a8a", hand="#5a9a8a", sleeve="#3a6a58", leg="#2a4a4a", tail="#3a6a58", mane="#26402e", pat=[]),
    "rotten": dict(main="#5a3a3a", head="#8a8a4a", arm="#8a8a4a", hand="#8a8a4a", sleeve="#5a3a3a", leg="#3a3a44", tail="#5a3a3a", mane="#3a301e", pat=[])})
sp("ghoul", dict(main="#4a3a3a", head="#7a8a6a", arm="#7a8a6a", hand="#8a9a78", sleeve="#4a3a3a", leg="#3a3030", mane="#2a2a24", eye="#e8d030", eyes="wide",
                 pat=[("spots", "#6a7a5a", 0.3, 0.06, ("head", "arm"))]),
   {"sallow": dict(main="#5a4a3a", head="#a89a6a", arm="#a89a6a", hand="#b8aa7a", sleeve="#5a4a3a", leg="#44382c", pat=[]),
    "dark": dict(main="#34343c", head="#4a4a54", arm="#4a4a54", hand="#5a5a64", sleeve="#34343c", leg="#26262c", eye="#d83a2a", pat=[]),
    "pale": dict(main="#6a6a6a", head="#c0c0b4", arm="#c0c0b4", hand="#d0d0c4", sleeve="#6a6a6a", leg="#4a4a4a", pat=[])})
sp("ogre", dict(main="#6a8a4a", head="#7a9a58", leg="#6a4a30", arm="#6a8a4a", hand="#7a9a58", belly="#8aaa68", horn="#efe6c6", eye="#e8c030", mane="#4a3220",
                pat=[("band", "#4a3220", 0.38, 0.5, ("body",))]),
   {"grey": dict(main="#8a8a82", head="#9a9a92", arm="#8a8a82", hand="#9a9a92", belly="#a8a8a0", leg="#5a5048"),
    "red": dict(main="#a8503a", head="#b8604a", arm="#a8503a", hand="#b8604a", belly="#c87a62", leg="#4a3a34"),
    "blue": dict(main="#5a7aa8", head="#6a8ab8", arm="#5a7aa8", hand="#6a8ab8", belly="#8aa4c8", leg="#3a4a64")})
sp("werewolf", dict(main="#5a4838", belly="#8a7458", snout="#7a6448", ear="#3e3226", leg="#4a3a2c", eye="#e8402a", nose="#1c1a1c", mane="#3e3226"),
   {"grey": dict(main="#8a8a94", belly="#b4b4be", snout="#a0a0aa", ear="#5a5a64", leg="#6a6a74", mane="#5a5a64"),
    "black": dict(main="#2e2c32", belly="#46444c", snout="#3a383e", ear="#201e24", leg="#26242a", mane="#1c1a20"),
    "pale": dict(main="#c8c0b0", belly="#e4ddd0", snout="#d8d0c0", ear="#a8a090", leg="#b4ac9c", mane="#a8a090")})
sp("wraith", dict(main="#7ac0e8", head="#5a9ac8", leg="#5a9ac8", arm="#7ac0e8", hand="#d8f0fa", sleeve="#5a9ac8", eye="#fafafa", eyes="wide", tail="#5a9ac8",
                  mane="#5a9ac8"),
   {"pale": dict(main="#d8e8f0", head="#b8ccd8", leg="#b8ccd8", arm="#d8e8f0", hand="#f4faff", sleeve="#b8ccd8", tail="#b8ccd8", mane="#b8ccd8"),
    "violet": dict(main="#9a7ae0", head="#7a5ac0", leg="#7a5ac0", arm="#9a7ae0", hand="#e0d4fa", sleeve="#7a5ac0", tail="#7a5ac0", mane="#7a5ac0"),
    "green": dict(main="#7ae0a0", head="#5ac080", leg="#5ac080", arm="#7ae0a0", hand="#d4faE0", sleeve="#5ac080", tail="#5ac080", mane="#5ac080")})
sp("scarecrow", dict(main="#b8884a", head="#d8b858", arm="#d8b858", hand="#d8b858", sleeve="#b8884a", leg="#8a6a3a", mane="#5a3a22", eye="#1c1a20",
                     pat=[("band", "#8a6a3a", 0.4, 0.46, ("body",))]),
   {"red": dict(main="#b8402a", sleeve="#b8402a", pat=[("band", "#8a2e1e", 0.4, 0.46, ("body",))]),
    "blue": dict(main="#3a5aa8", sleeve="#3a5aa8", pat=[("band", "#2a4080", 0.4, 0.46, ("body",))]),
    "green": dict(main="#4a7a3a", sleeve="#4a7a3a", pat=[("band", "#34562a", 0.4, 0.46, ("body",))])})
sp("grazer", dict(main="#a8784a"))

# ------------------------------------------------------------------ people: players, villagers, the wanderer

SKINTONES = {
    "fair": ("#f0c8a0", "#5a3a20", "short"),
    "tan": ("#c89060", "#2e2420", "short"),
    "dark": ("#7a4a30", "#1e1a1a", "long"),
    "ruddy": ("#e0a080", "#b8501e", "short"),
}


def human(skin="#f0c8a0", hair="#5a3a20", style="short", shirt="#3a9ad0", pants="#4a4a9a", shoe="#5a4a40", long_sleeve=False, pat=(), **kw):
    d = dict(main=shirt, head=skin, armtop=shirt, arm=shirt if long_sleeve else skin, hand=skin, pants=pants, shoe=shoe,
             leg=pants, eye="#2a2430", mane=hair, belly=shirt)
    d["pat"] = [("hair", hair, style)] + list(pat)
    d.update(kw)
    return d


def person(name, base, looks=None):
    """A person's outfit, and the same outfit on three other people."""
    sp("player", base, looks, name=name)


def outfit_variants(base_args):
    """Looks for the four people: only the skin and hair change."""
    out = {}
    for k, (skin, hair, style) in SKINTONES.items():
        if k == "fair":
            continue
        a = dict(base_args)
        a.update(skin=skin, hair=hair, style=style)
        out[k] = a
    return out


def people(name, **args):
    base = human(**args)
    looks = {}
    for k, (skin, hair, style) in SKINTONES.items():
        if k == "fair":
            continue
        a = dict(args)
        a.update(skin=skin, hair=hair, style=style)
        looks[k] = dict(new=True, **human(**a))
    sp("player", base, looks, name=name)


def apron(col, y0=0.38, y1=0.74):
    return [("front", col, y0, y1, ("body", "pants"))]


# --- Default skins, one picked for each player name (see src/content/pack_entities.rs).
people("skin_explorer", skin="#e8b890", hair="#5a3a20", shirt="#3a9ad0", pants="#4a4a9a", shoe="#5a4a40")
people("skin_miner", skin="#c88a58", hair="#2a2220", shirt="#d8a030", pants="#6a5a4a", shoe="#3a3430",
       pat=[("band", "#3a2c24", 0.43, 0.47, ("body",))], o_hat="#e8b838", **{})
people("skin_farmer", skin="#f0c8a0", hair="#c89a40", shirt="#c84a3a", pants="#3a5a9a", shoe="#4a3a2c",
       pat=[("front", "#3a5a9a", 0.45, 0.74, ("body",))], o_hat="#e0c070")
people("skin_sailor", skin="#8a5a3a", hair="#1e1a1a", shirt="#f0f0f4", pants="#2a3a6a", shoe="#1e1c20",
       pat=[("band", "#3a5aa8", 0.5, 0.54, ("body",)), ("band", "#3a5aa8", 0.6, 0.64, ("body",)), ("band", "#3a5aa8", 0.7, 0.74, ("body",))])
people("skin_knight", skin="#f0c8a0", hair="#8a6a3a", shirt="#aab0bc", pants="#6a707c", shoe="#4a4e58", long_sleeve=True,
       pat=[("front", "#c83a3a", 0.5, 0.78, ("body",))], o_hat="#aab0bc", arm="#aab0bc")
people("skin_mage", skin="#e8c0a0", hair="#e8e0d0", style="long", shirt="#6a3aa8", pants="#5a2e8a", shoe="#2a2030", long_sleeve=True,
       pat=[("band", "#e8c038", 0.47, 0.5, ("body",)), ("beard", "#e8e0d0")], o_hat="#5a2e8a")
people("skin_ranger", skin="#d8a878", hair="#4a3a24", shirt="#4a7a3a", pants="#6a4a30", shoe="#3a2c24", o_hat="#3a5a2a")
people("skin_baker", skin="#f4d0b0", hair="#7a4a28", shirt="#f4f2ec", pants="#8a7a6a", shoe="#5a4a40",
       pat=[("band", "#d8c8b0", 0.46, 0.5, ("body",))], o_hat="#fafafa")
# Pointed hats and helmets only cover the top of the head.
for _n in ("skin_miner", "skin_farmer", "skin_baker", "skin_mage", "skin_ranger"):
    for _s in ("", "_tan", "_dark", "_ruddy"):
        t = _T.get(_n + _s)
        if t:
            t[1]["pat"] = list(t[1].get("pat", [])) + [("keep_top", 0.4)]
for _s in ("", "_tan", "_dark", "_ruddy"):
    t = _T.get("skin_knight" + _s)
    t[1]["pat"] = list(t[1].get("pat", [])) + [("keep_top", 0.5)]

# The player models without a skin of their own wear the first one.
_T["player"] = ("player", _T["skin_explorer"][1])
_T["player_slim"] = ("player_slim", _T["skin_explorer"][1])

# --- Villagers: an outfit for each job, and three more faces in it.
people("villager_farmer", shirt="#c8a060", pants="#5a7a3a", shoe="#4a3a2c", hair="#7a5a30", o_hat="#e0c070",
       pat=[("front", "#5a7a3a", 0.45, 0.74, ("body",)), ("keep_top", 0.4)])
people("villager_builder", shirt="#d8782a", pants="#5a5a6a", shoe="#3a3430", hair="#3a2a20", o_hat="#e8c030",
       pat=[("band", "#3a2c24", 0.43, 0.47, ("body",)), ("keep_top", 0.45)])
people("villager_guard", shirt="#8a909c", pants="#4a4e58", shoe="#34363c", hair="#4a3a24", long_sleeve=True, arm="#8a909c", o_hat="#8a909c",
       pat=[("front", "#b83a3a", 0.5, 0.78, ("body",)), ("keep_top", 0.5)])
people("villager_baker", shirt="#f4f2ec", pants="#8a7a6a", shoe="#5a4a40", hair="#7a4a28", o_hat="#fafafa",
       pat=apron("#e8dcc8") + [("keep_top", 0.4)])
people("villager_smith", shirt="#6a5a50", pants="#4a403a", shoe="#2a2420", hair="#26201c", o_hat=None,
       pat=apron("#3a2c24", 0.3, 0.72) + [("beard", "#26201c")])
people("villager_trader", shirt="#3a8a8a", pants="#5a4a8a", shoe="#3a2c24", hair="#4a3020", o_hat="#8a5a2a",
       pat=[("band", "#e8c038", 0.45, 0.49, ("body",)), ("keep_top", 0.35)])
people("villager_shepherd", shirt="#e8e0d0", pants="#6a5a44", shoe="#3a2c24", hair="#6a4a30", o_hat="#8a6a44",
       pat=[("front", "#8a6a44", 0.38, 0.8, ("body",)), ("keep_top", 0.35)])
people("villager_fisher", shirt="#3a6a9a", pants="#d8c898", shoe="#3a3a2a", hair="#8a5a30", o_hat="#e8c850",
       pat=[("band", "#2a4a70", 0.5, 0.54, ("body",)), ("keep_top", 0.45)])
people("lurker", skin="#5a8a6a", hair="#26302a", shirt="#3a3a44", pants="#2a2a30", shoe="#1e1e22", eye="#e8e030")
# The wanderer: a hooded cloak, in four colours.
_cloak = dict(skin="#e8c0a0", hair="#8a8a94", shirt="#4a5a8a", pants="#3a3a4a", shoe="#2a2630", long_sleeve=True,
              o_hat="#3a4a7a", o_jacket="#3a4a7a", o_sleeve="#3a4a7a", pat=[("keep_top", 0.6)])
sp("player", human(**_cloak), {
    "green": dict(new=True, **human(**dict(_cloak, shirt="#4a7a4a", o_hat="#34602e", o_jacket="#34602e", o_sleeve="#34602e"))),
    "red": dict(new=True, **human(**dict(_cloak, shirt="#a84a3a", o_hat="#823022", o_jacket="#823022", o_sleeve="#823022"))),
    "grey": dict(new=True, **human(**dict(_cloak, shirt="#8a8a94", o_hat="#5a5a64", o_jacket="#5a5a64", o_sleeve="#5a5a64")))},
   name="wanderer")
# The lurker's other looks (it wears rags).
for _suffix, _skin in (("pale", "#a8b8a8"), ("ashen", "#8a8a96")):
    sp("player", human(skin=_skin, hair="#26302a", shirt="#3a3a44", pants="#2a2a30", shoe="#1e1e22", eye="#e8e030"), name="lurker_" + _suffix)


def textures():
    """{picture name: (layout model, spec)} for every picture to paint."""
    out = {}
    for name, (model, spec) in _T.items():
        s = dict(spec)
        # `o_hat=None` and the like mean "no such layer".
        for k in [k for k, v in s.items() if v is None]:
            del s[k]
        out[name] = (model, s)
    return out
