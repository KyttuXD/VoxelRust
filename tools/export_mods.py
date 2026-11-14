#!/usr/bin/env python3
"""Writes the mods that used to be the game's own content into the VoxelRust-Mods folder.

    python3 tools/gen_base.py                  # the core: data/base.txt and data/effects.txt
    python3 tools/export_mods.py [OUTDIR]      # every mod below into OUTDIR
    python3 tools/export_mods.py --base        # only the block and item mods
    python3 tools/export_mods.py --creatures   # only the creature mods

OUTDIR is the mods folder (default: $VOXELRUST_MODS_DIR, else ../VoxelRust-Mods).

The block and item mods are cut out of tools/gen_base.py's text by the rules
of tools/modsplit.py:

    stonework    extra rocks (marble, slate, granite, basalt, tiles) and stone stairs, slabs and walls
    woods        six more trees and everything made of them; the oak's and pine's extras
    colours      sixteen colours of concrete, wool, terracotta, glass, boards, chests, beds, couches
    caves        moss, glowcaps, crystal clusters, icicles, magma, vines, stalagmites, cobwebs
    explosives   TNT, gunpowder, flint and steel
    farming      farmland, wheat, carrots, potatoes, tomatoes, hoes
    cooking      ovens and the like, flour to cake, dishes set out on tables
    tools_extra  copper, crystal and ruby tools and swords; longbow and crossbow
    furniture    chairs to bathtubs, tables, lanterns, glass panes

The creature mods are cut out of tools/gen_creatures.py's table:

    wildlife     every animal (and the 32 new species the mod already had; the golem statue)
    monsters     what hunts at night and in caves
    villagers    the eight trades
    wanderer     the Wanderer

Each mod is `<mod>/pack.txt` (the wildlife mod's old pack.txt now includes
`animals.txt` too), a README.txt, and for creatures their `models/` and
`textures/` with `model` and `picture` lines that make the game read them.
Run it again after changing a generator: it rewrites every file it owns.
"""
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))

import modsplit  # noqa: E402

# Titles and a line about each mod: used for the first comment of its pack.txt and its README.
ABOUT = {
    "stonework": ("Stonework", "More rock to build with: marble, slate, granite, basalt, marble tiles and slate bricks, and stairs, slabs and walls of them and of the brick, stone brick and sandstone of the base game."),
    "woods": ("Woods", "Six more trees: birch, maple, palm, blossom, acacia and dark oak, with their logs, leaves, planks, stairs, slabs, fences, chests, doors, trapdoors, gates and the fancy doors; and the same extras for oak and pine."),
    "colours": ("Colours", "Sixteen colours of concrete, wool, terracotta and glass, boards (fences, trapdoors, gates), chests, beds, couches and armchairs, concrete stairs and slabs."),
    "caves": ("Caves", "What grows and glows underground: moss, glowcaps, mushrooms, crystal clusters, icicles, magma, glow vines, stalagmites and cobwebs. The terrain generator dresses caves with them when the mod is on."),
    "explosives": ("Explosives", "TNT, gunpowder and flint and steel. The explosions themselves are the game's (tag tnt)."),
    "farming": ("Farming", "Farmland, wheat, carrots, potatoes and tomatoes with their seeds (found in grass and flowers), and hoes of every tier."),
    "cooking": ("Cooking", "Ovens, kilns, smokers, blast furnaces and campfires; flour, dough, bread, cheese, pizzas, pies, stews, salads, sandwiches, cookies and cake; and plates, pizza boxes, cakes and bowls to set out."),
    "tools_extra": ("More Tools", "Copper, crystal and ruby pickaxes, axes, shovels and swords, and the longbow and the crossbow."),
    "furniture": ("Furniture", "Chairs, stools, benches, desks, shelves, cupboards, wardrobes, counters, a stove, a fridge, a television, a floor lamp, a toilet, a bath, a sink, a towel rail, posts, tables, lanterns and glass panes."),
    "wildlife": ("Wildlife", "Every animal: cows, pigs, sheep, chickens, horses, dogs, cats, wolves, deer, bears, fish, bats and the rest, with their young, their looks and the food chain, and the 32 newer species (frogs, owls, foxes, the Boomer...). Also the golem statue."),
    "monsters": ("Monsters", "What hunts at night and in the caves: the Lurker, the wraith, the zombie, the skeleton, the werewolf, the ogre, slimes, crawlers, cave spiders and the rock golem."),
    "villagers": ("Villagers", "The eight villagers and their trades: farmer, builder, guard, baker, smith, trader, shepherd and fisher."),
    "wanderer": ("The Wanderer", "The Wanderer, a traveller who trades rubies for apples and berries."),
}

# A note for the README of a mod, about what is not obvious from its lists.
EXTRA = {
    "farming": "Dirt, the two flowers and tall grass in the list are the core's own blocks, written again here with the seeds they drop (a mod may redefine a block; it keeps its place).",
    "woods": "The core keeps the oak (log, leaves, planks, door, chest) and the pine's log, leaves and planks, which the world generator needs. Without this mod the birches, maples, palms and the rest of the trees grow as oaks.",
    "caves": "Without this mod caves are plain and the creatures of the caves still live there.",
    "colours": "Several of its recipes use wood and wool: the boards, beds and couches need the woods mod too.",
}

MONSTERS = {"lurker", "wraith", "bone_hound", "skeleton", "zombie", "werewolf", "ghoul", "ogre", "scarecrow", "slime", "magma_slime",
            "crawler", "cave_spider", "frost_crawler", "golem"}
CREATURE_MODS = ["wildlife", "monsters", "villagers", "wanderer"]
# Mods that bring their own folder of code or art are left alone by this script apart from these files.
OWNED = {"pack.txt", "README.txt", "animals.txt"}


def folder(mod):
    """The folder (and the name in Settings > MODS) of a mod: tools_extra is tools-extra."""
    return mod.replace("_", "-")


def banner(mod, extra=""):
    title, blurb = ABOUT[mod]
    return ("# %s - a mod of the game, in its own folder.\n#\n# %s\n#\n"
            "# Written by tools/export_mods.py from the generators in the game's tools/ folder: edit those\n"
            "# (tools/gen_base.py, tools/modsplit.py, tools/gen_creatures.py) and run it again.\n"
            "# Switch the mod off in Settings > MODS, or delete the folder, and it is gone.\n%s\n") % (title, blurb, extra)


def wrap(text, width=96):
    out, line = [], ""
    for w in text.split():
        if len(line) + len(w) + 1 > width:
            out.append(line)
            line = w
        else:
            line = (line + " " + w).strip()
    out.append(line)
    return out


def write_pack_text(mod, parts, owner, needs, out):
    recs = parts[mod]
    order = ["palette", "texture", "block", "item", "group", "craft", "shaped", "recipe", "shapeless", "smelt", "bake", "sound"]
    lines = banner(mod, ("# Needs, besides the game itself: the mods %s.\n" % ", ".join(sorted(folder(n) for n in needs))) if needs else "").rstrip("\n").split("\n")
    heads = {"palette": "palettes", "texture": "textures", "block": "blocks", "item": "items", "group": "recipe groups", "craft": "recipes",
             "shaped": "recipes", "recipe": "recipes", "shapeless": "recipes", "smelt": "recipes at a furnace", "bake": "recipes at an oven", "sound": "sounds"}
    shown = set()
    for k in order:
        rs = [r for r in recs if r.kind == k]
        if not rs:
            continue
        # The recipe kinds share one heading.
        h = heads[k]
        if h not in shown:
            lines += ["", "# ---- %s ----" % h]
            shown.add(h)
        for r in rs:
            lines += r.lines
    return "\n".join(lines) + "\n"


def readme(mod, recs, needs, extra=""):
    title, blurb = ABOUT[mod]
    blocks = [r for r in recs if r.kind == "block" and not any(l.split()[:1] == ["hidden"] for l in r.lines[1:])]
    items = [r for r in recs if r.kind == "item"]
    names = lambda rs: ", ".join(next((l.split(None, 1)[1] for l in r.lines[1:] if l.split()[:1] == ["name"]), r.id) for r in rs)
    out = ["%s" % title, "=" * len(title), ""] + wrap(blurb) + [""]
    out += ["What it adds", "------------", "", "%d blocks, %d items, %d recipes, %d textures." % (
        len(blocks), len(items), sum(1 for r in recs if r.kind in ("craft", "shaped", "recipe", "shapeless", "smelt", "bake")), sum(1 for r in recs if r.kind == "texture")), ""]
    if extra:
        out += wrap(extra) + [""]
    if blocks and len(blocks) <= 60:
        out += wrap("Blocks: " + names(blocks) + ".") + [""]
    if items and len(items) <= 60:
        out += wrap("Items: " + names(items) + ".") + [""]
    out += ["Needs", "-----", ""]
    out += wrap("The game's core content (stone, dirt, wood, ores, tools, the crafting table, the furnace...)" +
                (" and the mods %s." % ", ".join(sorted(folder(n) for n in needs)) if needs else ".") +
                (" Anything that needs a missing mod is left out with a warning in the log; the rest still works." if needs else "")) + [""]
    out += ["Installing", "----------", "",
            "It comes with the game (in the `mods` folder beside the program). To add it to a game that lacks it, copy this folder",
            "into the game's `mods` folder (or Settings > MODS > ADD, or `voxelrust --add-mod PATH`); switch it off in",
            "Settings > MODS, or delete the folder, and its blocks, items and recipes are gone (worlds keep them by name and",
            "bring them back when the mod is back).", "",
            "How it is made", "--------------", "",
            "pack.txt is written by tools/export_mods.py in the game's repository from tools/gen_base.py and tools/modsplit.py;",
            "change the generators, not the file.", ""]
    return "\n".join(out)


def export_base(out_dir):
    import gen_base
    parts, tail, owner, problems = modsplit.split(gen_base.FULL_TEXT)
    bad = modsplit.check(parts, owner)
    if bad:
        for b in bad:
            print("BROKEN REFERENCE:", b, file=sys.stderr)
        raise SystemExit("a mod refers to a mod after it (see modsplit.RANK)")
    deps = modsplit.dependencies(parts, owner)
    for mod in modsplit.MODS:
        if mod in CREATURE_MODS or not parts[mod]:
            continue
        d = os.path.join(out_dir, folder(mod))
        os.makedirs(d, exist_ok=True)
        needs = deps[mod]
        open(os.path.join(d, "pack.txt"), "w").write(write_pack_text(mod, parts, owner, needs, d))
        open(os.path.join(d, "README.txt"), "w").write(readme(mod, parts[mod], needs, EXTRA.get(mod, "")))
        print("%-12s %4d records" % (mod, len(parts[mod])))
    return parts, owner, deps


# ---------------------------------------------------------------- creatures

def classify(rec):
    if rec.id == "wanderer":
        return "wanderer"
    if any(l.split()[:2] == ["behavior", "villager"] for l in rec.lines):
        return "villagers"
    if rec.id in MONSTERS:
        return "monsters"
    return "wildlife"


def export_creatures(out_dir, base_parts=None, base_owner=None):
    import gen_creatures as gc
    recs, _ = modsplit.parse(gc.ENTITY_TEXT)
    recs = [r for r in recs if r.kind == "entity"]
    by = {m: [] for m in CREATURE_MODS}
    for r in recs:
        by[classify(r)].append(r)
    model_ids = set(gc.MODEL_IDS)

    def find(kind, name):
        """The file of a model or picture, wherever the creature mods' folders have it."""
        ext = ".bbmodel" if kind == "models" else ".png"
        for d in gc.CREATURE_DIRS:
            f = os.path.join(d, kind, name + ext)
            if os.path.exists(f):
                return f
        return None

    def models_of(rs):
        got = set()
        for r in rs:
            for l in r.lines:
                w = l.split()
                if w[:1] == ["model"] and len(w) > 1:
                    got.add(w[1])
                if w[:1] == ["variant"] and "model" in w:
                    got.add(w[w.index("model") + 1])
        return got

    def pictures_of(rs, models):
        got = set()
        for r in rs:
            for l in r.lines:
                for m in re.finditer(r"\btex (\w+)", l):
                    got.add(m.group(1))
        for m in models:
            got.add(m)
        return {p for p in got if find("textures", p)}

    wild_extra = base_parts["wildlife"] if base_parts else []
    needed = {}
    for mod in CREATURE_MODS:
        ms = models_of(by[mod])
        # Block records of the mod (the golem statue) name a model too.
        for r in (wild_extra if mod == "wildlife" else []):
            ms |= {t for t in r.tokens() if t in model_ids}
        needed[mod] = {m for m in ms if m in model_ids}
    pics = {mod: pictures_of(by[mod], needed[mod]) for mod in CREATURE_MODS}
    for mod in CREATURE_MODS:
        d = os.path.join(out_dir, folder(mod))
        os.makedirs(d, exist_ok=True)
        # Models and pictures: copied in; for the creature art that began in the wildlife folder, moved out of it.
        if needed[mod]:
            os.makedirs(os.path.join(d, "models"), exist_ok=True)
        if pics[mod]:
            os.makedirs(os.path.join(d, "textures"), exist_ok=True)
        for m in needed[mod]:
            src, dst = find("models", m), os.path.join(d, "models", m + ".bbmodel")
            if os.path.abspath(src) != os.path.abspath(dst):
                shutil.copy(src, dst)
        for p in pics[mod]:
            src, dst = find("textures", p), os.path.join(d, "textures", p + ".png")
            if os.path.abspath(src) != os.path.abspath(dst):
                shutil.copy(src, dst)
        decls = ["model %s file models/%s.bbmodel" % (m, m) for m in sorted(needed[mod])]
        picl = ["picture %s file textures/%s.png" % (p, p) for p in sorted(pics[mod])]
        text = []
        if mod == "wildlife":
            text.append("# The animals that used to be the game's own (tools/gen_creatures.py; see docs/DATA.md for the `entity` lines).\n"
                        "# The newer species are in pack.txt, which includes this file.\n")
        else:
            text.append(banner(mod, ""))
        text.append("\n".join(decls))
        text.append("")
        text.append("\n".join(picl))
        text.append("")
        for r in by[mod]:
            text.append(r.text())
        if mod == "wildlife" and wild_extra:
            text.append("")
            text.append("# The golem statue: a prop of the golem model.")
            for r in wild_extra:
                text.append(r.text())
        name = "animals.txt" if mod == "wildlife" else "pack.txt"
        open(os.path.join(d, name), "w").write("\n".join(text) + "\n")
        # Leave a creature's art only in the mod that owns it.
        print("%-10s %3d creatures %3d models %3d pictures" % (mod, len(by[mod]), len(needed[mod]), len(pics[mod])))
    # What now sits in wildlife/ although another mod owns it, goes: it was only the working copy.
    keep_models = needed["wildlife"]
    keep_pics = pics["wildlife"]
    own_models = set().union(*[needed[m] for m in CREATURE_MODS if m != "wildlife"]) - keep_models
    own_pics = set().union(*[pics[m] for m in CREATURE_MODS if m != "wildlife"]) - keep_pics
    wild = os.path.join(gc.ART)
    for m in sorted(own_models):
        f = os.path.join(wild, "models", m + ".bbmodel")
        if os.path.exists(f):
            os.remove(f)
    for p in sorted(own_pics):
        f = os.path.join(wild, "textures", p + ".png")
        if os.path.exists(f):
            os.remove(f)
    # Creature art nobody uses any more (a creature that was dropped from the table): leave it, but say so.
    used_m = set().union(*needed.values())
    for m in sorted(model_ids - used_m):
        print("note: model", m, "is used by no creature")
    for mod in CREATURE_MODS:
        write_creature_readme(out_dir, mod, by[mod], needed[mod], pics[mod], base_owner)
    return by


def write_creature_readme(out_dir, mod, recs, models, pictures, owner):
    deps = set()
    for r in recs:
        for t in r.tokens():
            o = (owner or {}).get(t)
            if o and o != "core" and o not in CREATURE_MODS:
                deps.add(o)
    ids = ", ".join(next((l.split(None, 1)[1] for l in r.lines[1:] if l.split()[:1] == ["name"]), r.id) for r in recs)
    title, blurb = ABOUT[mod]
    out = [title, "=" * len(title), ""] + wrap(blurb) + [""]
    out += ["What it adds", "------------", "", "%d creatures (%s), %d models and %d pictures." % (len(recs), "all of them" if mod != "wildlife" else "the animals of the old base game; the newer species are described in pack.txt", len(models), len(pictures)), ""]
    if mod != "wildlife":
        out += wrap("Creatures: " + ids + ".") + [""]
    out += ["Needs", "-----", ""]
    out += wrap("The game itself%s. The creatures' drops and trades name items of the base game%s; one that is missing is left out with a warning in the log, and the creature still lives." % (
        "" if not deps else " and, for what they drop and trade, the mods " + ", ".join(sorted(folder(n) for n in deps)), "" if not deps else " and of those mods")) + [""]
    out += ["Installing", "----------", "",
            "It comes with the game (in the `mods` folder beside the program). To add it to a game that lacks it, copy this folder",
            "into the game's `mods` folder (or Settings > MODS > ADD, or `voxelrust --add-mod PATH`). Without the mod the world has",
            "no creatures of this kind. Creatures are read from `animals.txt` (wildlife) or `pack.txt`; their pictures are the",
            "`picture` lines there, each a PNG in `textures/`.", ""]
    open(os.path.join(out_dir, folder(mod), "README.txt"), "w").write("\n".join(out))


def main(argv):
    args = [a for a in argv if not a.startswith("--")]
    out_dir = os.path.abspath(args[0]) if args else MODS_DIR
    only_base = "--base" in argv
    only_creatures = "--creatures" in argv
    parts = owner = None
    if not only_creatures:
        parts, owner, deps = export_base(out_dir)
    else:
        import gen_base
        parts, tail, owner, problems = modsplit.split(gen_base.FULL_TEXT)
    if not only_base:
        export_creatures(out_dir, parts, owner)


if __name__ == "__main__":
    main(sys.argv[1:])
