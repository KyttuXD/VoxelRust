#!/usr/bin/env python3
"""Make the slim-arm ("Alex") variants of the player and the first-person hand.

Slim arms are one pixel narrower than classic arms (3 wide instead of 4).
The outer edge of each arm moves in by that pixel and the body-side edge
stays where it is, as in the classic Minecraft skin layout. The arm's
UV faces are laid out again for the narrower box. Plugs (the small
filler pieces) only have their geometry trimmed; their UVs stay as they are.

Run from the repository root:

    python3 tools/gen_slim_models.py
"""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent / "data" / "models"
PX = 28.48 / 8.0  # one skin pixel in model units (the head is 8 px wide)
ARM_PARTS = ("arm", "forearm", "hand")


def relayout(faces, u0, w):
    """Give the faces of a 4-deep, `w`-wide arm box their slim UV columns.

    Same unwrap as a Minecraft box: right (east) face, then front (north),
    left (west), back (south); top (up) above the front, bottom (down) above
    the left. Only the x range changes; each face keeps its own y range.
    """
    cols = {
        "east": (u0, u0 + 4),
        "north": (u0 + 4, u0 + 4 + w),
        "west": (u0 + 4 + w, u0 + 8 + w),
        "south": (u0 + 8 + w, u0 + 8 + 2 * w),
        "up": (u0 + 4, u0 + 4 + w),
        "down": (u0 + 4 + w, u0 + 4 + 2 * w),
    }
    for name, face in faces.items():
        uv = face.get("uv")
        if not uv or uv == [0, 0, 0, 0] or name not in cols:
            continue
        x1, x2 = cols[name]
        face["uv"] = [x1, uv[1], x2, uv[3]]


def slim_element(e):
    name = e["name"]
    side = name[0]  # "R" or "L"
    if side == "R":
        outer = e["to"][0]
        e["to"][0] = round(outer - PX, 4)
    else:
        outer = e["from"][0]
        e["from"][0] = round(outer + PX, 4)
    e["origin"][0] = round((e["from"][0] + e["to"][0]) / 2, 4)
    if "plug" in name:
        return
    east = e["faces"]["east"]["uv"]
    u0 = east[0]
    before = e["faces"]["north"]["uv"]
    assert before[2] - before[0] == 4, (name, before)
    relayout(e["faces"], u0, 3)


def convert(src, dst):
    d = json.loads((ROOT / src).read_text())
    for e in d["elements"]:
        name = e["name"]
        if name[:2] in ("R-", "L-") and any(p in name for p in ARM_PARTS + ("sleeve", "plug")):
            slim_element(e)
    d["name"] = d["name"] + " (slim arms)"
    (ROOT / dst).write_text(json.dumps(d, indent=2) + "\n")
    print("wrote", dst)


if __name__ == "__main__":
    convert("player.bbmodel", "player_slim.bbmodel")
    convert("hand.bbmodel", "hand_slim.bbmodel")
