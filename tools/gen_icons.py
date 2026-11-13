#!/usr/bin/env python3
"""Makes the icon files in assets/ from the emblem the game draws itself.

    cargo build --release && python3 tools/gen_icons.py

Writes icon.png (256), icon.ico (16..256, for Windows) and icon.icns (for
macOS). The .icns carries both kinds of entry: the old packed ones that Mac
OS X 10.5 and 10.6 read (16, 32, 48 and 128 pixels) and the PNG ones newer
systems use (up to 1024), so the icon shows at every size on every Mac.
Also writes the icon at the sizes Linux desktops look for, in
assets/hicolor/<size>x<size>/apps/voxelrust.png."""
import os, struct, subprocess, sys, tempfile, zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
GAME = os.path.join(ROOT, "target", "release", "voxelrust")
ASSETS = os.path.join(ROOT, "assets")


def render(size):
    """The emblem at `size` pixels, as PNG bytes."""
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "icon.png")
        subprocess.run([GAME, "--write-icon", path, "--icon-size", str(size)], check=True)
        return open(path, "rb").read()


def pixels(png):
    """Decode an 8-bit RGBA PNG (what the game writes) into rows of bytes."""
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    at, data, w, h = 8, b"", 0, 0
    while at < len(png):
        n, kind = struct.unpack(">I4s", png[at:at + 8])
        body = png[at + 8:at + 8 + n]
        if kind == b"IHDR":
            w, h, depth, colour = struct.unpack(">IIBB", body[:10])
            assert (depth, colour) == (8, 6), "expected 8-bit RGBA"
        elif kind == b"IDAT":
            data += body
        at += 12 + n
    raw = zlib.decompress(data)
    stride, rows, prev = w * 4, [], bytearray(w * 4)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line
    return w, h, b"".join(rows)


def repack(png):
    """The same picture, packed as small as zlib can make it."""
    w, h, px = pixels(png)
    raw = b"".join(b"\0" + px[y * w * 4:(y + 1) * w * 4] for y in range(h))
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def packbits(channel):
    """The run-length packing old icon entries use; literal runs only,
    which every reader accepts."""
    out = bytearray()
    for i in range(0, len(channel), 128):
        part = channel[i:i + 128]
        out.append(len(part) - 1)
        out += part
    return bytes(out)


def icns(images):
    """images: {size: png bytes}."""
    chunks = []
    def add(kind, data):
        chunks.append(kind + struct.pack(">I", len(data) + 8) + data)
    # Old entries: packed RGB, and the transparency as a separate mask.
    for size, rgb_kind, mask_kind in ((16, b"is32", b"s8mk"), (32, b"il32", b"l8mk"), (48, b"ih32", b"h8mk"), (128, b"it32", b"t8mk")):
        w, h, px = pixels(images[size])
        r, g, b, a = (px[k::4] for k in range(4))
        body = packbits(r) + packbits(g) + packbits(b)
        add(rgb_kind, (b"\0\0\0\0" if size == 128 else b"") + body)
        add(mask_kind, a)
    # New entries: PNG as it is.
    for kind, size in ((b"icp4", 16), (b"icp5", 32), (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024), (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512)):
        add(kind, images[size])
    body = b"".join(chunks)
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def ico(images, sizes):
    head = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    table, blobs = b"", b""
    for s in sizes:
        png = images[s]
        table += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        blobs += png
    return head + table + blobs


if not os.path.exists(GAME):
    sys.exit("build the game first: cargo build --release")
SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
images = {s: repack(render(s)) for s in SIZES}
open(os.path.join(ASSETS, "icon.png"), "wb").write(images[256])
open(os.path.join(ASSETS, "icon.ico"), "wb").write(ico(images, (16, 24, 32, 48, 64, 128, 256)))
open(os.path.join(ASSETS, "icon.icns"), "wb").write(icns(images))
for s in (16, 24, 32, 48, 64, 128, 256, 512):
    d = os.path.join(ASSETS, "hicolor", f"{s}x{s}", "apps")
    os.makedirs(d, exist_ok=True)
    open(os.path.join(d, "voxelrust.png"), "wb").write(images[s])
print("wrote icon.png, icon.ico, icon.icns and assets/hicolor/")
