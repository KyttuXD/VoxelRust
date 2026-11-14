#!/usr/bin/env python3
"""Rewrites `craft` / `recipe` lines into the crafting grid's `shaped` syntax.

    craft  <item> <count> <ingredient> <count> [<ingredient> <count> ...]
    shaped <item> <count> <row> [<row> ...] with <letter> <ingredient> [<letter> <ingredient> ...]

A line is turned into a `shaped` one where its shape is well known (tools,
stairs, slabs, doors, beds, chests, furnaces ...); every other `craft` line is
left as it is, which the game reads as a shapeless recipe. `bake` and `smelt`
lines, comments and everything else are never touched. A line that is already
`shaped` is left alone, so running this again changes nothing.

Usage:
    tools/craft_convert.py FILE ...           rewrite the files in place
    tools/craft_convert.py --check FILE ...   only look for recipes two of which
                                              could be made from the same grid
                                              (exit status 1 if there are any)

Other tools (tools/gen_base.py) import `convert_text` and `find_conflicts`.
"""
import re
import shlex
import sys

WOODS = ["oak", "pine", "birch", "maple", "palm", "blossom", "acacia", "dark_oak"]
# Things made of one particular wood keep asking for that wood's planks; the
# rest (tools, beds, furniture) take any planks: `@planks`.
WOOD_PREFIXES = tuple(w + "_" for w in WOODS) + ("planks_",)

# A shape: output name pattern, rows, what each letter is, whether the counts
# of the old line are ignored (the shape sets its own cost), and the count made
# (None keeps the line's).
#   role "*"      the one ingredient of the line that no other letter names
#   other roles   that very item (taken from the line, or added if `adds`)
SHAPES = [
    # (out regex, rows, roles, loose, count)
    (r".+_pick", ["III", ".S.", ".S."], {"I": "*", "S": "stick"}, False, None),
    (r".+_axe", ["II", "IS", ".S"], {"I": "*", "S": "stick"}, False, None),
    (r".+_shovel", ["I", "S", "S"], {"I": "*", "S": "stick"}, False, None),
    (r".+_hoe", ["II", ".S", ".S"], {"I": "*", "S": "stick"}, False, None),
    (r".+_sword", ["I", "I", "S"], {"I": "*", "S": "stick"}, False, None),
    (r".+_stairs", ["M..", "MM.", "MMM"], {"M": "*"}, True, 4),
    (r".+_slab", ["MMM"], {"M": "*"}, True, 6),
    (r".+_wall", ["MMM", "MMM"], {"M": "*"}, True, 6),
    (r".+_fence", ["MSM", "MSM"], {"M": "*", "S": "stick"}, True, 3),
    (r".+_trapdoor", ["MMM", "MMM"], {"M": "*"}, True, 2),
    (r".+_gate", ["SMS", "SMS"], {"M": "*", "S": "stick"}, False, None),
    (r".+_door", ["MM", "MM", "MM"], {"M": "*"}, False, None),
    (r".+_bed", ["WWW", "PPP"], {"W": "*", "P": "planks"}, False, None),
    (r".+_chest", ["MMM", "M.M", "MMM"], {"M": "*"}, False, None),
    (r"furnace|kiln", ["MMM", "M.M", "MMM"], {"M": "*"}, False, None),
    (r"oven", ["MMM", "MCM", "MMM"], {"M": "*", "C": "coal"}, False, None),
    (r"blast_furnace", ["III", "IFI", "SSS"], {"I": "iron_ingot", "F": "furnace", "S": "stone_bricks"}, False, None),
    (r"smoker", [".L.", "LOL", ".L."], {"L": "log", "O": "oven"}, False, None),
    (r"campfire", [".S.", "SCS", "LLL"], {"S": "stick", "C": "coal", "L": "log"}, False, None),
    (r"crafting_table", ["MM", "MM"], {"M": "*"}, False, None),
    (r"stick", ["M", "M"], {"M": "*"}, False, None),
    (r"boat", ["M.M", "MMM"], {"M": "*"}, False, None),
    (r"stone_bricks|marble_tiles|slate_bricks|sandstone|white_wool", ["MM", "MM"], {"M": "*"}, False, None),
    (r"arrow", ["G", "S"], {"G": "gravel", "S": "stick"}, False, None),
    (r"bow", ["SM.", "S.M", "SM."], {"S": "stick", "M": "string"}, False, None),
    # Furniture. Each has a shape of its own so that no two recipes are made
    # from the same arrangement.
    (r"chair", ["M.", "MM", "SS"], {"M": "*", "S": "stick"}, False, None),
    (r"stool", [".M.", "S.S"], {"M": "*", "S": "stick"}, False, None),
    (r"park_bench", ["MMM", "S.S"], {"M": "*", "S": "stick"}, True, None),
    (r"desk", ["MMM", "M.M"], {"M": "*"}, True, None),
    (r"planks_table", ["MMM", ".M."], {"M": "*"}, True, None),
    (r"shelf", ["M.M"], {"M": "*"}, True, None),
    (r"bookshelf", ["MMM", "...", "MMM"], {"M": "*"}, False, None),
    (r"floor_lamp", ["L", "S", "S"], {"L": "lamp", "S": "stick"}, False, None),
    (r"signal_post", ["S", "L", "S"], {"L": "lamp", "S": "stick"}, False, None),
    (r"toilet", ["M.", "MM"], {"M": "*"}, False, None),
    (r"bathtub", ["M.M", "MMM"], {"M": "*"}, False, None),
    (r"bathroom_sink", ["MM"], {"M": "*"}, False, None),
    # Things the mods make, whose ingredients are the same as another thing's.
    (r".+_chest", ["CW"], {"C": "oak_chest", "W": "*"}, False, None),
    (r"rail", ["I.I", "SIS"], {"I": "*", "S": "stick"}, False, None),
    (r"gift_.+",["W", "C"], {"W": "*", "C": "oak_chest"}, False, None),
    (r"tree_star", ["C.C", ".S."], {"C": "crystal", "S": "stick"}, False, None),
    (r"icing_block", ["M", "E"], {"M": "milk", "E": "egg"}, False, None),
    (r"eggnog", ["ME"], {"M": "milk", "E": "egg"}, False, None),
    (r"coffin", ["P", "C", "P"], {"P": "planks", "C": "oak_chest"}, False, None),
    (r"cupboard", ["PCP"], {"P": "planks", "C": "oak_chest"}, False, None),
    (r"harvest_wreath", ["WSW", "WSW"], {"W": "wheat", "S": "stick"}, False, None),
    (r"egg_basket", ["W.W", "WSW", "S.."], {"W": "wheat", "S": "stick"}, False, None),
    (r"platform", ["M", "M", "M"], {"M": "*"}, False, None),
    (r"buffer_stop", ["P", "P", "P"], {"P": "*"}, False, None),
]


def words(line):
    """Words of a data line (rows in quotes are one word each)."""
    try:
        return shlex.split(line, comments=True)
    except ValueError:
        return []


def wood_specific(out):
    return out.startswith(WOOD_PREFIXES)


def ingredient_name(name, out):
    """`planks` means any planks, except in what is made of a certain wood."""
    if name == "planks" and not wood_specific(out):
        return "@planks"
    return name


def counts_of(rows):
    n = {}
    for row in rows:
        for ch in row:
            if ch not in ". ":
                n[ch] = n.get(ch, 0) + 1
    return n


def apply_shape(out, count, ings, shape):
    """The `shaped` words for a recipe, or None when the shape does not fit it."""
    pattern, rows, roles, loose, new_count = shape
    if not re.fullmatch(pattern, out):
        return None
    need = {}
    for name, n in ings:
        if name in need:
            return None  # the same item listed twice: leave it alone
        need[name] = n
    cnt = counts_of(rows)
    mapping = {}
    stars = [l for l, r in roles.items() if r == "*"]
    for letter, role in roles.items():
        if role == "*":
            continue
        if role in need:
            if not loose and need[role] != cnt[letter]:
                return None
            del need[role]
        elif not loose:
            return None
        mapping[letter] = role
    if stars:
        if len(need) != 1:
            return None
        (name, n), = need.items()
        if not loose and n != cnt[stars[0]]:
            return None
        mapping[stars[0]] = name
    elif need:
        return None
    # Letters the line did not name (`loose` shapes add sticks and the like).
    shaped_count = count if new_count is None else new_count
    pairs = []
    for letter in sorted(mapping):
        pairs += [letter, ingredient_name(mapping[letter], out)]
    return ["shaped", out, str(shaped_count)] + ['"%s"' % r for r in rows] + ["with"] + pairs


def convert_line(line):
    """One line of a data file: itself, or its `shaped` form."""
    w = words(line)
    if len(w) < 5 or w[0] not in ("craft", "recipe") or len(w) % 2 != 1:
        return line
    out, count = w[1], w[2]
    try:
        ings = [(w[i], int(w[i + 1])) for i in range(3, len(w), 2)]
        int(count)
    except ValueError:
        return line
    for shape in SHAPES:
        done = apply_shape(out, count, ings, shape)
        if done:
            return " ".join(done)
    # Not a known shape: the same line, with `planks` made `@planks` where
    # that is what is meant, and `recipe` spelt `craft`.
    return " ".join(["craft", out, count] + sum(([ingredient_name(n, out), str(c)] for n, c in ings), []))


def convert_text(text):
    """A whole file; identical recipe lines are kept once."""
    seen = set()
    out = []
    for line in text.split("\n"):
        new = convert_line(line)
        if new.split(" ", 1)[0] in ("shaped", "craft", "recipe"):
            if new in seen:
                continue
            seen.add(new)
        out.append(new)
    return "\n".join(out)


# ---- Looking for recipes that clash -------------------------------------
# The same rules as src/game/crafting/matcher.rs, in Python, so the data can
# be checked without building the game.

def read_recipes(texts):
    """(recipes, groups) from the text of data files. A recipe is
    (out, count, kind, data): `shaped` data is (rows of ingredients), a
    shapeless one's the list of ingredients; an ingredient is
    (set of items, need)."""
    groups = {}
    lines = []
    for text in texts:
        for line in text.split("\n"):
            w = words(line)
            if w and w[0] == "group" and len(w) > 2:
                groups.setdefault(w[1], []).extend(w[2:])
            elif w and w[0] in ("craft", "recipe", "shapeless", "shaped"):
                lines.append(w)

    def ing(name):
        need = 1
        if "*" in name:
            name, n = name.rsplit("*", 1)
            need = int(n)
        items = groups[name[1:]] if name.startswith("@") else [name]
        return (frozenset(items), need, items)

    recipes = []
    for w in lines:
        out, count = w[1], int(w[2])
        if w[0] == "shaped":
            k = w.index("with")
            rows = w[3:k]
            key = {w[i]: ing(w[i + 1]) for i in range(k + 1, len(w), 2)}
            grid = [[None if c in ". " else key[c] for c in r] for r in rows]
            recipes.append((out, count, "shaped", trim(grid), " ".join(w)))
        else:
            pairs = [(ing(w[i]), int(w[i + 1])) for i in range(3, len(w), 2)]
            units = sum(n for _, n in pairs)
            lst = []
            for (items, need, ordered), n in pairs:
                if units <= 9:
                    lst += [(items, need, ordered)] * n
                else:
                    lst.append((items, need * n, ordered))
            recipes.append((out, count, "shapeless", lst, " ".join(w)))
    return recipes


def trim(grid):
    rows = [r for r in grid if any(c for c in r)]
    if not rows:
        return []
    cols = [x for x in range(len(grid[0])) if any(r[x] for r in grid)]
    x0, x1 = cols[0], cols[-1]
    y0 = next(y for y, r in enumerate(grid) if any(r))
    y1 = max(y for y, r in enumerate(grid) if any(r))
    return [r[x0:x1 + 1] for r in grid[y0:y1 + 1]]


def fits(ingredient, item, count):
    return item in ingredient[0] and count >= ingredient[1]


def matches(recipe, size, cells):
    """cells: size x size list of rows, each None or (item, count)."""
    _, _, kind, data, _ = recipe
    flat = [c for r in cells for c in r if c]
    if kind == "shapeless":
        if len(flat) != len(data):
            return False
        return assign(data, flat)
    h, w = len(data), len(data[0])
    if sum(1 for r in data for c in r if c) != len(flat):
        return False
    for oy in range(size - h + 1):
        for ox in range(size - w + 1):
            for mirror in (False, True):
                if shaped_at(data, h, w, oy, ox, mirror, size, cells):
                    return True
    return False


def shaped_at(data, h, w, oy, ox, mirror, size, cells):
    for y in range(size):
        for x in range(size):
            want = None
            if oy <= y < oy + h and ox <= x < ox + w:
                px = x - ox
                want = data[y - oy][w - 1 - px if mirror else px]
            have = cells[y][x]
            if (want is None) != (have is None):
                return False
            if want and not fits(want, have[0], have[1]):
                return False
    return True


def assign(ingredients, flat):
    owner = [None] * len(flat)

    def augment(e, seen):
        for c, (item, count) in enumerate(flat):
            if seen[c] or not fits(ingredients[e], item, count):
                continue
            seen[c] = True
            if owner[c] is None or augment(owner[c], seen):
                owner[c] = e
                return True
        return False

    return all(augment(e, [False] * len(flat)) for e in range(len(ingredients)))


def canonical(recipe, last=False):
    """The 3 x 3 grid a recipe is made from, laid out at its top left, with
    the first (or last) item of each group."""
    _, _, kind, data, _ = recipe
    cells = [[None] * 3 for _ in range(3)]

    def put(ing):
        return (ing[2][-1 if last else 0], ing[1])

    if kind == "shaped":
        for y, row in enumerate(data):
            for x, c in enumerate(row):
                if c:
                    cells[y][x] = put(c)
    else:
        for i, c in enumerate(data):
            cells[i // 3][i % 3] = put(c)
    return cells


def find_conflicts(texts):
    """Pairs of recipes that both match what one of them is made from."""
    recipes = read_recipes(texts)
    bad = []
    for a in recipes:
        for grid in (canonical(a), canonical(a, last=True)):
            for b in recipes:
                if b is not a and b[4] != a[4] and (b[0], b[1]) != (a[0], a[1]) and matches(b, 3, grid):
                    bad.append((a[4], b[4]))
    # Several recipes for the same thing from the same grid are fine; two
    # things from one grid are not.
    return list(dict.fromkeys(bad))


def main(argv):
    check = "--check" in argv
    files = [a for a in argv if not a.startswith("--")]
    if check:
        texts = [open(f).read() for f in files]
        bad = find_conflicts(texts)
        for a, b in bad:
            print("clash:\n  %s\n  %s" % (a, b))
        print("%d clashes" % len(bad))
        return 1 if bad else 0
    for f in files:
        old = open(f).read()
        new = convert_text(old)
        if new != old:
            open(f, "w").write(new)
            print("rewrote", f)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
