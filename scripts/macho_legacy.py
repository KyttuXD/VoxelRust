#!/usr/bin/env python3
"""Make a 64-bit Intel Mach-O executable startable on Mac OS X 10.6.

Modern linkers describe the entry point with the LC_MAIN load command, which
the 10.6 and 10.7 loaders reject ("unknown required load command"). This
rewrites it to the older LC_UNIXTHREAD form, pointing at the same address,
and stamps the minimum OS version as 10.6.

The executable must have been linked with a raw entry point (one that reads
argc/argv from the stack, like `_voxelrust_start` in src/main.rs) as its
entry symbol, and with spare room after the load commands (`-headerpad`).

usage: macho_legacy.py IN OUT
"""
import struct
import sys

LC_REQ_DYLD = 0x80000000
LC_SEGMENT_64 = 0x19
LC_UNIXTHREAD = 0x5
LC_MAIN = 0x28 | LC_REQ_DYLD
LC_VERSION_MIN_MACOSX = 0x24
LC_BUILD_VERSION = 0x32
LC_DYLD_INFO_ONLY = 0x22 | LC_REQ_DYLD
# Load commands the 10.6 loader knows. Anything else must not be "required".
KNOWN_10_6 = {0x1, 0x2, 0x5, 0xB, 0xC, 0xE, 0x19, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24}


def fail(msg):
    sys.exit("macho_legacy: " + msg)


def main():
    if len(sys.argv) != 3:
        fail("usage: macho_legacy.py IN OUT")
    data = bytearray(open(sys.argv[1], "rb").read())
    magic, cpu, _sub, filetype, ncmds, sizeofcmds, _flags, _res = struct.unpack_from("<8I", data, 0)
    if magic != 0xFEEDFACF or cpu != 0x01000007 or filetype != 2:
        fail("not a 64-bit x86 Mach-O executable")

    cmds = []
    off = 32
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, off)
        cmds.append((cmd, bytes(data[off:off + size])))
        off += size

    text_vmaddr = None
    first_section = len(data)
    entryoff = None
    for cmd, body in cmds:
        if cmd == LC_SEGMENT_64:
            name = body[8:24].rstrip(b"\0")
            vmaddr, _vmsize, _fileoff, _filesize, _maxp, _initp, nsects, _fl = struct.unpack_from("<4Q4I", body, 24)
            if name == b"__TEXT":
                text_vmaddr = vmaddr
            for i in range(nsects):
                sect = 72 + i * 80
                (sect_off,) = struct.unpack_from("<I", body, sect + 48)
                (sect_flags,) = struct.unpack_from("<I", body, sect + 64)
                if sect_off and (sect_flags & 0xFF) not in (1, 12):  # skip zero-fill
                    first_section = min(first_section, sect_off)
                if (sect_flags & 0xFF) in (0x11, 0x12, 0x13):
                    fail("the program uses thread-local variables, which 10.6 cannot load")
        elif cmd == LC_MAIN:
            (entryoff,) = struct.unpack_from("<Q", body, 8)
        elif cmd == LC_UNIXTHREAD:
            fail("already has LC_UNIXTHREAD")
        elif cmd == 0x34 | LC_REQ_DYLD or cmd == 0x33 | LC_REQ_DYLD:
            fail("linked with chained fixups, which 10.6 cannot read; link for an older target")
    if entryoff is None or text_vmaddr is None:
        fail("no LC_MAIN / __TEXT found")
    if not any(cmd == LC_DYLD_INFO_ONLY for cmd, _ in cmds):
        fail("no LC_DYLD_INFO_ONLY: 10.6 needs classic dyld binding information")

    # x86_THREAD_STATE64: 21 registers; only rip (index 16) is set. The kernel
    # supplies the stack pointer.
    regs = [0] * 21
    regs[16] = text_vmaddr + entryoff
    thread = struct.pack("<4I21Q", LC_UNIXTHREAD, 16 + 21 * 8, 4, 42, *regs)

    out = []
    for cmd, body in cmds:
        if cmd == LC_MAIN:
            continue
        if cmd == LC_VERSION_MIN_MACOSX:
            body = struct.pack("<4I", cmd, 16, 0x000A0600, 0x000A0600)
        elif cmd == LC_BUILD_VERSION:
            # Unknown to 10.6 (and ignored there); say 10.6 the old way.
            body = struct.pack("<4I", LC_VERSION_MIN_MACOSX, 16, 0x000A0600, 0x000A0600) + b"\0" * (len(body) - 16)
            body = body[:16]
        elif (cmd & LC_REQ_DYLD) and (cmd & ~LC_REQ_DYLD) not in KNOWN_10_6:
            fail("required load command 0x%x is unknown to the 10.6 loader" % cmd)
        out.append(body)
    out.append(thread)
    blob = b"".join(out)
    if 32 + len(blob) > first_section:
        fail("no room for the new load command; link with -headerpad 0x400")

    struct.pack_into("<II", data, 16, len(out), len(blob))
    data[32:first_section] = b"\0" * (first_section - 32)
    data[32:32 + len(blob)] = blob
    open(sys.argv[2], "wb").write(data)
    print("macho_legacy: entry 0x%x, %d load commands, minimum OS 10.6" % (regs[16], len(out)))


if __name__ == "__main__":
    main()
