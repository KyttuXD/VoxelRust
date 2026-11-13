#!/usr/bin/env python3
"""Check that voxelrust.js supplies every function voxelrust.wasm imports.

    web/check-imports.py dist/web/voxelrust.wasm web/voxelrust.js

A WebAssembly program cannot start unless each of its imports is given to
it. This reads the import section of the .wasm file and looks for a
function of the same name in the JavaScript (they are all written as
`name(arguments) {` in the objects that make up `env`). It exits with an
error naming what is missing; functions the game no longer imports are
only mentioned. scripts/build-web.sh runs it after every build.
"""
import re
import sys


def leb(data, at):
    """An unsigned LEB128 number and the position after it."""
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            return value, at


def name(data, at):
    size, at = leb(data, at)
    return data[at:at + size].decode(), at + size


def limits(data, at):
    flags, at = leb(data, at)
    _, at = leb(data, at)
    if flags & 1:
        _, at = leb(data, at)
    return at


def imports(data):
    """(module, name) of every function the program imports."""
    if data[:4] != b"\0asm":
        sys.exit("not a WebAssembly file")
    found = []
    at = 8
    while at < len(data):
        section = data[at]
        size, at = leb(data, at + 1)
        if section == 2:
            count, p = leb(data, at)
            for _ in range(count):
                module, p = name(data, p)
                field, p = name(data, p)
                kind = data[p]
                p += 1
                if kind == 0:  # function: its type
                    _, p = leb(data, p)
                    found.append((module, field))
                elif kind == 1:  # table: element type, limits
                    p = limits(data, p + 1)
                elif kind == 2:  # memory: limits
                    p = limits(data, p)
                else:  # global: type, mutability
                    p += 2
        at += size
    return found


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    wanted = imports(open(sys.argv[1], "rb").read())
    script = open(sys.argv[2], encoding="utf-8").read()
    provided = set(re.findall(r"^\s+((?:gl|js_)\w+)\(.*\)\s*\{", script, re.M))
    foreign = [m + "." + f for m, f in wanted if m != "env"]
    missing = [f for m, f in wanted if m == "env" and f not in provided]
    unused = sorted(provided - {f for _, f in wanted})
    if unused:
        print("note: in the JavaScript but not imported: " + ", ".join(unused))
    if foreign or missing:
        sys.exit("error: voxelrust.js does not provide: " + ", ".join(foreign + missing))
    print("imports: all %d are provided by %s" % (len(wanted), sys.argv[2]))


if __name__ == "__main__":
    main()
