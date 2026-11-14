#!/usr/bin/env python3
"""Which mods run through the script engine, and what each one needs.

Prints the matrix of docs/SCRIPT-COMPAT.md: for every mod in a mods folder,
whether it is data only, has a script, or has compiled code (code/), and
which capabilities that code or script uses, marked as covered by the
engine's host calls (src/script/host.rs, src/script/ext/*.rs) or not.

    python3 tools/script_compat.py [MODS_DIR]            the matrix as text
    python3 tools/script_compat.py --markdown [MODS_DIR] the matrix as a Markdown table
    python3 tools/script_compat.py --functions           the host functions, as the Markdown tables
                                                         of docs/SCRIPTING.md

MODS_DIR defaults to $VOXELRUST_MODS_DIR, then ../VoxelRust-Mods. Nothing is
written: this only reads.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROW = re.compile(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*\)')


def unescape(s):
    return s.replace('\\"', '"').replace("\\\\", "\\")


def table_rows(path, const="FUNCTIONS"):
    """The (signature, what) rows of `pub const NAME: &[(&str, &str)] = &[ ... ];` in a file."""
    text = open(path, encoding="utf-8").read()
    m = re.search(r"pub const %s: &\[\(&str, &str\)\] = &\[(.*?)\n\];" % const, text, re.S)
    if not m:
        m = re.search(r"pub const %s: &\[\(&str, &str\)\] = &\[(.*?)\];" % const, text, re.S)
    if not m:
        return []
    return [(unescape(a), unescape(b)) for a, b in ROW.findall(m.group(1))]


def host_functions():
    """All host functions: {name: (file, signature, what)}."""
    out = {}
    for sig, what in table_rows(os.path.join(ROOT, "src/script/host.rs"), "HOST_FUNCTIONS"):
        out[sig.split("(")[0]] = ("host.rs", sig, what)
    ext = os.path.join(ROOT, "src/script/ext")
    for f in sorted(os.listdir(ext)):
        if f.endswith(".rs"):
            for sig, what in table_rows(os.path.join(ext, f)):
                out[sig.split("(")[0]] = (f, sig, what)
    return out


def hooks():
    """The hooks a script may define."""
    text = open(os.path.join(ROOT, "src/script/mod.rs"), encoding="utf-8").read()
    return re.findall(r'HookDoc \{ name: "(on_[a-z_]+)"', text)


# What a mod can need, how to see it in compiled code, and which host calls / hooks give it to a script.
# `None` for the last field: the engine has no way to do it (yet).
CAPS = [
    ("blocks", r"world\.(get|set_block|is_loaded|raycast)|g\.world", ["get_block", "set_block", "scan_blocks", "is_loaded", "is_solid"]),
    ("creatures", r"mobs\.(list|spawn|hurt|hit_test)|spawn_in", ["spawn_mob", "mobs_near", "mob_info", "mob_set_pos", "mob_remove", "mob_hurt", "mob_set_size", "mob_set_vel"]),
    ("player", r"g\.player\.|g\.seat|g\.shake|g\.flash|g\.health|g\.sleeping", ["player_look", "set_look", "hold_player", "place_player", "flash", "shake", "push_player", "player_health"]),
    ("commands", r"command:\s*Some", ["on_command"]),
    ("per-frame", r"\bupdate:\s*Some|on_tick:\s*Some", ["on_update", "on_tick"]),
    ("clicks", r"interact:\s*Some|on_item_use", ["on_interact", "on_item_use"]),
    ("block hooks", r"on_block_(place|break)", ["on_block_place", "on_block_break"]),
    ("damage hooks", r"\bhurt:\s*Some|killed:\s*Some|\bhit:\s*Some|on_entity_death|on_player_hurt", ["on_player_hurt", "on_entity_death"]),
    ("explosions", r"before_explode|after_explode|explosion::explode", ["on_explode", "explode"]),
    ("conditions", r"condition:\s*Some", ["on_condition"]),
    ("HUD", r"on_draw_hud", ["hud_rect", "hud_text"]),
    ("world drawing", r"draw:\s*Some|draw_model|draw_sparks|draw_quads", ["draw_model", "draw_speck"]),
    ("sounds", r"sound_queue|play_named|audio\.", ["play_sound"]),
    ("saved state", r"\bsave:\s*Some|\bopen:\s*Some|on_world_(save|load)", ["state_get", "state_set"]),
    ("files in tmp", r"mod_tmp_dir|write_file|read_file", ["tmp_read", "tmp_write"]),
    ("pack lines", r"lines:\s*&\[\"|sections:\s*&\[\"|r\.(lines|sections)|reg\(\)\.(lines|sections)", ["pack_lines", "pack_sections", "pack_prop"]),
    ("bodies", r"Player::with_size|Vehicles|body\.update", ["body_new", "body_step"]),
    ("storage UI", r"storage:\s*Some|opened_ext", ["storage_open"]),
    ("networking", r"message:\s*Some|share:\s*Some|send_ext|serve:\s*Some", ["net_send", "on_net"]),
    ("projectiles", r"g\.shots|Shot \{", ["shoot"]),
    ("sky", r"forced_sky|g\.sky", ["force_sky"]),
    ("effects", r"effect_apply|effect_tick|effects::apply", ["effect"]),
    ("pathfinding", r"path::find_path|ground_near", ["find_path"]),
    ("custom screens", r"map_screen|map_open|settings:\s*Some|terrain_form|\bui\.", None),
    ("world generation", r"terrain_open|terrain_parse|dimension_terrain|terrain_label|worldgen::Source", None),
    ("HTTP", r"\bhttp\b|HttpClient|fetch_tile|https?://", None),
]
# Mods that are the user's own work: reported on, never ported.
PROTECTED = ["orespawn", "witherstorm"]
DATA_KEYS = ["block", "item", "entity", "texture", "model", "recipe", "craft", "shaped", "shapeless", "sound", "vehicle", "when", "command", "episode", "dimension", "ore", "plant"]


def read_all(folder, ext):
    text = ""
    for dp, _, names in os.walk(folder):
        for n in sorted(names):
            if n.endswith(ext):
                text += open(os.path.join(dp, n), encoding="utf-8", errors="replace").read() + "\n"
    return text


def analyse(mods_dir, name, funcs):
    base = os.path.join(mods_dir, name)
    info = {"name": name, "data": 0, "scripts": 0, "native": 0, "used": {}, "hooks": []}
    pack = ""
    for dp, _, names in os.walk(base):
        for n in names:
            if n.endswith(".txt") and "/code" not in dp and "/scripts" not in dp and n != "README.txt":
                try:
                    pack += open(os.path.join(dp, n), encoding="utf-8", errors="replace").read() + "\n"
                except OSError:
                    pass
    info["data"] = sum(1 for line in pack.splitlines() if line.split(" ")[:1] and line.split(" ")[0] in DATA_KEYS)
    script_text = read_all(os.path.join(base, "scripts"), ".rs") if os.path.isdir(os.path.join(base, "scripts")) else ""
    code_text = read_all(os.path.join(base, "code"), ".rs") if os.path.isdir(os.path.join(base, "code")) else ""
    info["scripts"] = script_text.count("\nfn ") + (1 if script_text.startswith("fn ") else 0)
    info["native"] = len(code_text.splitlines())
    info["hooks"] = sorted(set(re.findall(r"^fn (on_[a-z_]+)", script_text, re.M)))
    if script_text:
        called = set(re.findall(r"\b([a-z_][a-z0-9_]*)\(", script_text))
        info["calls"] = sorted(c for c in called if c in funcs)
        for cap, _, give in CAPS:
            if give and (set(give) & called or set(give) & set(info["hooks"])):
                info["used"][cap] = "script"
    if code_text and not script_text:
        for cap, rx, give in CAPS:
            if re.search(rx, code_text):
                # Native code uses it: covered if the engine has host calls for it, missing otherwise.
                info["used"].setdefault(cap, "native-covered" if give else "native-missing")
    return info


def kind_of(i):
    if i["name"] in PROTECTED:
        return "report only (protected)"
    if i["scripts"] and i["native"]:
        return "script (native code kept for native-mods)"
    if i["scripts"]:
        return "script"
    if i["native"]:
        return "native only (terrain and HTTP are not scriptable)" if i["name"] == "earth" else "native only"
    return "data only"


MARK = {"script": "S", "native-covered": "c", "native-missing": "X"}


def mods_dir_arg(args):
    for a in args:
        if not a.startswith("--"):
            return a
    return os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(os.path.dirname(ROOT), "VoxelRust-Mods")


def matrix(mods_dir):
    funcs = host_functions()
    names = sorted(n for n in os.listdir(mods_dir) if os.path.isfile(os.path.join(mods_dir, n, "pack.txt")))
    return [analyse(mods_dir, n, funcs) for n in names]


def print_text(infos):
    caps = [c[0] for c in CAPS]
    w = max(len(i["name"]) for i in infos) + 2
    print("S = a script uses it; c = compiled code uses it, and the engine has host calls for it; X = compiled code uses it, the engine has no way")
    print()
    print("%-*s %-40s %s" % (w, "mod", "how it runs", "data lines"))
    for i in infos:
        print("%-*s %-40s %d" % (w, i["name"], kind_of(i), i["data"]))
    print()
    print("capabilities (columns, left to right): " + ", ".join(caps))
    for i in infos:
        cells = "".join(MARK.get(i["used"].get(c), ".") for c in caps)
        print("%-*s %s" % (w, i["name"], cells))


def print_markdown(infos):
    """Only the columns somebody uses, and the data-only mods in one line below the table."""
    caps = [c[0] for c in CAPS if any(i["used"].get(c[0]) for i in infos)]
    rows = [i for i in infos if kind_of(i) != "data only"]
    print("| mod | how it runs | " + " | ".join(caps) + " |")
    print("| --- | --- | " + " | ".join(":---:" for _ in caps) + " |")
    for i in rows:
        cells = " | ".join(MARK.get(i["used"].get(c), "") for c in caps)
        print("| %s | %s | %s |" % (i["name"], kind_of(i), cells))
    plain = [i["name"] for i in infos if kind_of(i) == "data only"]
    print()
    print("`S` a script of the mod uses it (the engine covers it). `c` the mod's compiled code uses it and the engine has host calls for it. `X` compiled code uses it and the engine has no way to do it.")
    print()
    print("Data only (nothing to run; every line loads through the runtime loader): " + ", ".join(plain) + ".")


def print_functions():
    ext = os.path.join(ROOT, "src/script/ext")
    order = ["world", "mobs", "player", "draw", "bodies", "storage", "pack", "files", "net", "effects"]
    titles = {
        "world": "The world (read only)",
        "mobs": "Creatures",
        "player": "The player, input and the camera",
        "draw": "Pictures: HUD and models",
        "bodies": "Physics bodies",
        "storage": "Storage on the chest screen",
        "pack": "What the packs say",
        "files": "Files in `tmp`",
        "net": "Other players",
        "effects": "Shots, sky, pushes",
    }
    for key in order:
        rows = table_rows(os.path.join(ext, key + ".rs"))
        print("### %s\n" % titles[key])
        print("| Function | What it does |")
        print("| --- | --- |")
        for sig, what in rows:
            print("| `%s` | %s |" % (sig, what.replace("|", "\\|")))
        print()


def main(argv):
    args = argv[1:]
    if "--functions" in args:
        print_functions()
        return 0
    d = mods_dir_arg(args)
    if not os.path.isdir(d):
        print("no mods folder at %s (give it, or set VOXELRUST_MODS_DIR)" % d, file=sys.stderr)
        return 1
    infos = matrix(d)
    (print_markdown if "--markdown" in args else print_text)(infos)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
