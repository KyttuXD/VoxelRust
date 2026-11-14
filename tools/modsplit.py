#!/usr/bin/env python3
"""Splits the game's content text into the small core and the mods.

tools/gen_base.py writes ONE long text (everything the game used to carry in
data/base.txt). This module cuts it into records (a `texture`, `block`,
`item`, recipe... line with its indented lines) and gives each record to the
mod that owns it. Whatever no mod owns is the core: the content the game
needs to boot and play with no mods installed.

The rules are the tables at the top of this file (OWNER_RULES, SOUNDS, the
rank list). Nothing here names a file: tools/export_mods.py does the writing.

    ownership   a block or item belongs to the first mod whose rule matches
                its id; a texture belongs to the lowest-ranked owner of the
                blocks and items that use it; a recipe belongs to the
                highest-ranked owner of the things it mentions (so it only
                exists when everything it needs does).
    rank        MODS lists the mods lowest first. A mod may refer to the core
                and to mods before it in the list, never to later ones;
                check() reports every reference that breaks this.
"""
import re

# Lowest rank first. The core is rank 0 and is not in this list.
MODS = ["stonework", "woods", "colours", "caves", "explosives", "farming", "cooking", "tools_extra", "furniture",
        "wildlife", "monsters", "villagers", "wanderer"]
RANK = {"core": 0, **{m: i + 1 for i, m in enumerate(MODS)}}

COLOUR = "white|light_gray|gray|black|brown|red|orange|yellow|lime|green|cyan|light_blue|blue|purple|magenta|pink"
EXTRA_WOOD = "birch|maple|palm|blossom|acacia|dark_oak"

# (mod, regex over the id), first match wins; no match = core.
OWNER_RULES = [
    # Every colour variant: blocks, boards (fences, trapdoors, gates), chests, beds, couches.
    ("colours", r"^(%s)_(concrete|wool|terracotta|glass|chest|bed|bed_head|couch|armchair|fence|trapdoor|trapdoor_open|gate|gate_open|concrete_stairs|concrete_slab)$" % COLOUR),
    # Cave dressing.
    ("caves", r"^(moss|glow_mushroom|brown_mushroom|crystal_cluster|icicle|magma|cave_vine|stalagmite|cobweb|mushroom)$"),
    # Explosives.
    ("explosives", r"^(tnt|gunpowder|flint_and_steel)$"),
    # Everything made of the extra woods, and what the oak and pine have beyond a log, leaves, planks, a door and a chest.
    ("woods", r"^(%s)_" % EXTRA_WOOD),
    ("woods", r"^pine_(?!(log|leaves|planks)$)"),
    ("woods", r"^oak_(?!(chest|door|door_open)$)"),
    # Extra stone: the rocks, and the stairs, slabs and walls that are not stone's or cobble's.
    ("stonework", r"^(marble|marble_tiles|slate|slate_bricks|granite|basalt)(_stairs|_slab|_wall)?$"),
    ("stonework", r"^(stone_bricks|brick|sandstone)_(stairs|slab|wall)$"),
    ("stonework", r"^(stone|cobble)_wall$"),
    # Crops and everything for the field.
    ("farming", r"^(farmland|(wheat|carrot|potato|tomato)(_seeds|_[1-4])?)$"),
    ("farming", r"^.+_hoe$"),
    # Food beyond the basics, ovens and the like, and the dishes set out on tables.
    ("cooking", r"^(oven|blast_furnace|kiln|smoker|campfire|flour|dough|bread|milk|cheese|egg|fried_egg|tomato_sauce|baked_potato|pizza|meat_pizza|veggie_pizza|cookie|apple_pie|berry_pie|meat_pie|stew|fish_soup|salad|sandwich|cake|plate|plated_pizza|pizza_box|pizza_box_open|cake_block|bread_block|fruit_bowl)$"),
    # Tool tiers beyond iron, and the other bows.
    ("tools_extra", r"^(copper|crystal|ruby)_(pick|axe|shovel|sword)$"),
    ("tools_extra", r"^(longbow|crossbow)$"),
    # Furniture and props.
    ("furniture", r"^(chair|stool|park_bench|desk|shelf|bookshelf|cupboard|wardrobe|counter|sink_counter|stove|fridge|television|floor_lamp|toilet|bathtub|bathroom_sink|towel_rail|planks_post|planks_table|lantern|glass_pane)$"),
    # The golem statue is a prop of the golem model, which the wildlife mod carries.
    ("wildlife", r"^golem_statue$"),
]
# Textures named after a thing but never mentioned by it (the game finds them by name).
TEXTURE_OF = [(r"^(.+)_pulling_\d$", 1)]

# Sounds stay in the core, creature calls too: mods that are not ours (OreSpawn, Hearth & Garden...) use them by name.
SOUND_OWNERS = {}

# Core blocks that name a moved item in an `also` line get a second copy, with the line, in the mod that owns the item.
OVERRIDES = {"dirt", "flower_red", "flower_yellow", "tall_grass"}


def owner_of(id):
    for mod, rx in OWNER_RULES:
        if re.search(rx, id):
            return mod
    return "core"


class Record:
    def __init__(self, comments, lines):
        self.comments = comments      # comment and blank lines just before it
        self.lines = lines            # the record's own lines
        self.kind = lines[0].split()[0]
        self.words = lines[0].split()
        self.id = self.words[1] if len(self.words) > 1 else ""

    def text(self):
        return "\n".join(self.lines)

    def tokens(self):
        toks = set()
        for l in self.lines:
            l = l.split("#", 1)[0]
            for t in re.split(r"[\s\"]+", l):
                if t and not t.startswith("@"):
                    toks.add(t)
        return toks


def parse(text):
    """The text as records, in order, plus any trailing comments."""
    recs, pend, cur = [], [], None
    for l in text.split("\n"):
        if cur is not None and l[:1] in (" ", "\t") and l.strip():
            cur.append(l)
            continue
        if cur is not None:
            recs.append(Record(pend, cur))
            pend, cur = [], None
        if not l.strip() or l.lstrip().startswith("#"):
            pend.append(l)
        else:
            cur = [l]
    if cur is not None:
        recs.append(Record(pend, cur))
        pend = []
    return recs, pend


def split(text):
    """Cut `text` into {mod: [Record]} (core is "core"). Returns (parts, tail comments, problems)."""
    recs, tail = parse(text)
    names = {}   # block/item id -> owner
    for r in recs:
        if r.kind in ("block", "item"):
            names[r.id] = owner_of(r.id)
    tex_users = {}   # texture or palette id -> owners of what mentions it
    tex_ids = {r.id for r in recs if r.kind == "texture"}
    pal_ids = {r.id for r in recs if r.kind == "palette"}
    for r in recs:
        if r.kind in ("block", "item"):
            for t in r.tokens() & tex_ids:
                tex_users.setdefault(t, set()).add(names[r.id])
    owner = dict(names)
    for t in tex_ids:
        if t in names:
            owner[t] = names[t]
            continue
        got = None
        for rx, g in TEXTURE_OF:
            m = re.match(rx, t)
            if m and m.group(g) in names:
                got = names[m.group(g)]
        if got is None:
            us = tex_users.get(t)
            got = min(us, key=lambda m: RANK[m]) if us else "core"
        owner[t] = got
    for r in recs:   # palettes: lowest owner of the textures that paint with them
        if r.kind == "palette":
            us = {owner[x.id] for x in recs if x.kind == "texture" and r.id in x.tokens()}
            owner[r.id] = min(us, key=lambda m: RANK[m]) if us else "core"
    parts = {m: [] for m in ["core"] + MODS}
    problems = []
    for r in recs:
        k = r.kind
        if k in ("block", "item", "texture", "palette"):
            parts[owner[r.id]].append(r)
        elif k == "bake":
            parts["cooking"].append(r)   # baked at an oven, which the cooking mod brings
        elif k in ("craft", "shaped", "recipe", "shapeless", "smelt"):
            owners = {owner[t] for t in r.tokens() if t in names}
            parts[max(owners or {"core"}, key=lambda m: RANK[m])].append(r)
        elif k == "group":
            members = r.words[2:]
            by = {}
            for mem in members:
                by.setdefault(owner.get(mem, "core"), []).append(mem)
            for m, mem in by.items():
                parts[m].append(Record(r.comments if m == "core" else [], ["group " + r.words[1] + " " + " ".join(mem)]))
        elif k == "sound":
            parts[SOUND_OWNERS.get(r.id, "core")].append(r)
        else:
            parts["core"].append(r)
    # Core blocks whose `also` lines name moved items.
    for r in list(parts["core"]):
        if r.kind == "block" and r.id in OVERRIDES:
            keep, extra = [], {}
            for l in r.lines:
                w = l.split()
                if w[:1] == ["also"] and len(w) > 1 and owner.get(w[1], "core") != "core":
                    extra.setdefault(owner[w[1]], []).append(l)
                else:
                    keep.append(l)
            if extra:
                parts["core"][parts["core"].index(r)] = Record(r.comments, keep)
                top = max(extra, key=lambda m: RANK[m])
                parts[top].append(Record([], r.lines))
    return parts, tail, owner, problems


def check(parts, owner):
    """References that break the ranks: [(mod, record text, offending name, its mod)]."""
    bad = []
    names = set(owner)
    for m, recs in parts.items():
        for r in recs:
            if r.kind in ("texture", "palette"):
                continue    # a texture may name a block's own painter style; its owner is what its users need
            for t in r.tokens():
                if t in names and t != r.id and owner[t] != m and RANK[owner[t]] > RANK[m]:
                    # `paint chest`, `paint glass` are painter names, not references: only blocks and items count
                    if r.kind in ("block", "item") or r.kind in ("craft", "shaped", "bake", "smelt", "group"):
                        bad.append((m, r.lines[0], t, owner[t]))
    return bad


def dependencies(parts, owner):
    """{mod: set of other mods its records mention}."""
    deps = {m: set() for m in parts}
    for m, recs in parts.items():
        for r in recs:
            for t in r.tokens():
                o = owner.get(t)
                if o and o != m and o != "core" and r.kind not in ("texture", "palette"):
                    deps[m].add(o)
    return deps
