# Running on old and new systems

## Why the game is built the way it is

| Constraint | Consequence |
| --- | --- |
| Rust's standard library needs thread-local storage from the loader, which Mac OS X gained in 10.7. Current Rust requires 10.12. | The game is `no_std`. Memory comes from `malloc`; threads, files, timing, window and input come from SDL2. |
| A 2008 Mac Pro on 10.6 offers OpenGL 2.1 at most. | Fixed-function OpenGL 1.5: vertex buffers and two texture units, no shaders. Every later OpenGL still provides this (macOS through its legacy context, Mesa and Windows drivers through the compatibility profile). |
| Old systems have old SDL2 builds. | Only functions present in SDL 2.0.0 are required. SDL2 is opened with `dlopen` / `LoadLibraryA`, so no headers or import libraries are needed to build. |
| No `std` means no `sin`, `sqrt`, `floor`. | They are implemented in `src/util/math.rs`. As a side effect, worlds are bit-identical across machines. |

What the finished program imports from the system on macOS is a few dozen
`libSystem` functions (memory, `dlopen`, sockets, `system`); the full list
is the stub in `scripts/build-macos-legacy.sh`. All of them exist in 10.6.

## Dependencies

`Cargo.toml` lists **zero crates**, and none are pulled in indirectly. At run
time the program needs only:

- **SDL2**, any release with the 2.0.0 API or later, found and opened when
  the game starts (not when it is built);
- an **OpenGL 1.5** driver;
- `curl` on the `PATH`, and only for Earth worlds.

Building needs a Rust toolchain and nothing else; see
[BUILDING.md](BUILDING.md).

## Mac OS X 10.6 and 10.7 (64-bit Intel)

```sh
scripts/build-macos-legacy.sh            # or: scripts/build-macos-legacy.sh 1.77.0
```

This works from Linux, Windows (in a POSIX shell) or a Mac, and needs only a
Rust toolchain with the `x86_64-apple-darwin` target (`rustup target add
x86_64-apple-darwin`) and Python 3. It needs no Xcode and no Apple SDK:

1. It writes a small text stub listing the `libSystem` functions above, which
   is all the linker needs to know about the system.
2. It links with `rust-lld`, the linker that ships with Rust, for a 10.6
   deployment target and the `core2` CPU.
3. Linkers today describe a program's entry point with the `LC_MAIN` load
   command, which the 10.6 and 10.7 loaders reject. So the program is linked
   with its own raw entry point (`_voxelrust_start` in `src/main.rs`, the job
   Apple's `crt1.o` used to do) and `scripts/macho_legacy.py` rewrites
   `LC_MAIN` into the older `LC_UNIXTHREAD`.

The result is `dist/macos-legacy/voxelrust` (build name
`macos-legacy-x86_64`). It should also start on every later Intel macOS.

**This has been run on a real machine**: a 2008 Mac Pro (Xeon E5462, Radeon
HD 2600) on Mac OS X 10.6.8, with the official SDL2 2.0.4 framework beside
the binary. The game starts, renders correctly, loads chunks on seven worker
threads, and can host a multiplayer server.

### SDL2 on Snow Leopard

Use the official **SDL2 2.0.4** disk image from libsdl.org
(`https://www.libsdl.org/release/SDL2-2.0.4.dmg`): open it and copy
`SDL2.framework` into the same folder as the `voxelrust` binary. Nothing
needs installing system-wide. Current SDL2 releases need a newer system.

The game also looks in `/Library/Frameworks`, MacPorts' `/opt/local/lib`, and
wherever `VOXELRUST_SDL2=/path/to/SDL2` points.

### On a 2008 Mac Pro

Measured on the machine above (Radeon HD 2600, 1920 x 1200 display), in full
screen, frames per second as the game itself counts them:

| 3D detail | View distance | Distant terrain | FPS |
| --- | --- | --- | --- |
| 100% | 4 | off | 132 |
| 100% | 6 | off | 120 (one run of three: 80) |
| 50% | 6 | off | 154-155 |
| 100% | 8 | 32 | 83 |
| 50% | 8 | 32 | 119 |
| 100% | 12 | off | 80 |
| 50% | 12 | off | 109 |
| 50% | 16 | off | 78 |

A good starting point there:

```sh
./voxelrust --fullscreen --render-scale 50 --distance 8 --far 32
```

What limits it:

Later measurements, with the game installed as `/Applications/VoxelRust.app`
and started with `open` (default view distance 8, distant terrain 32):

| How it runs | FPS |
|---|---|
| 1280 x 720 window | 256 |
| Full screen at the desktop's 1920 x 1200 | 158 |
| Full screen at 1600 x 900 | 208 |
| Full screen at 1280 x 800 | 274 |
| `--flypass 1200` in a window (flying over fresh terrain) | 225 |

- **Full screen at 1920 x 1200 is limited by how many pixels the card can
  fill**, not by the game's settings: turning off fog, the alpha test or the
  light texture each changed it by under 3%. A smaller full-screen
  resolution is what helps.
- **The 60 Hz lock seen earlier came from starting the bare program over
  SSH.** Started as an app (double-click, or `open`), no run has been capped.
- The second graphics card's display was locked to 60 Hz and slower.

SETTINGS > VIDEO has 3D DETAIL (the render scale) and HOLD FRAME RATE, which
lowers the view distance by itself to keep a rate you choose. F3 shows the
frame rate and how much is being drawn.

The game also tunes full screen once by itself: the first time it is played
full screen at the desktop's resolution with vsync off, it times four
seconds, and if it is below 170 FPS on a display wider than 1440 pixels it
switches full screen to 1280 x 800 and says so (`auto_tune` in
`src/client/app.rs`; see
[PLAYING.md](PLAYING.md#full-screen-on-a-slow-graphics-card)). On this Mac it measured 152 FPS and switched, giving the 274 above.

### Mac OS X 10.5 and older

Not supported by this build path. 10.5's loader predates the compressed
binding information (`LC_DYLD_INFO`) that modern linkers emit, so it needs
Apple's own old linker, and SDL2 for 10.5 is rare. PowerPC Macs running Mac
OS X have no Rust target at all (see the next section).

## PowerPC and very old machines

The code is not tied to Intel processors:

- It compiles for 32-bit big-endian PowerPC Linux:
  `cargo check --lib --target powerpc-unknown-linux-gnu` passes.
- Sound is produced in the machine's own byte order, so it is correct on
  big-endian processors. Save files and the network protocol are fixed
  little-endian on every machine.

**It has not been run on real PowerPC hardware.** Compiling is as far as the
testing goes.

What that means for a Power Mac G4:

- Under **Linux/PPC** it can in principle run, given SDL2 and a graphics
  driver that offers OpenGL 1.5.
- Under **Mac OS X 10.4 or 10.5** it cannot: Rust has no target for PowerPC
  Mac OS X, so there is nothing to build the game with.

The oldest machine the game has actually been run on is the 2008 Mac Pro
(Intel, Mac OS X 10.6.8) described above. That one is tested on the real
hardware.

## macOS 10.8 and later

`scripts/build-macos-legacy.sh` also writes `dist/macos/voxelrust`, an
ordinary build for 10.12 and later with the standard `LC_MAIN` entry (build
name `macos-x86_64`; the 10.6 one is `macos-legacy-x86_64`). Packaged as
apps, each declares its minimum system, so macOS refuses to start the 10.12
app on anything older (see
[BUILDING.md](BUILDING.md#packaging-the-mac-apps)). On a
Mac you can instead `brew install sdl2` and `cargo build --release`. The
game uses the legacy OpenGL 2.1 context, which macOS still ships (deprecated,
but present). Not yet run on a Mac.

## Linux

`cargo build --release`, with the SDL2 runtime library installed. Works on
X11 and Wayland. This is the platform the game was developed and tested on.

## Windows 10 / 11

```sh
scripts/build-windows.sh
```

writes `dist/windows/voxelrust.exe` from any host, with no Visual Studio,
Windows SDK or MinGW. Put a 64-bit `SDL2.dll` (from libsdl.org) next to it.

How: the game does not link the toolchain's C runtime. It has its own entry
point (`mainCRTStartup` in `src/main.rs`), takes `malloc` and friends from
`msvcrt.dll` and sockets from `ws2_32.dll`, both part of Windows, and
declares those imports as `raw-dylib` so the compiler writes the import
tables itself.

**Run under Wine 10** with the official SDL2 2.32 `SDL2.dll`: the game
renders the same frame as on Linux, and a server run this way accepted a
Linux client. It has not been run on real Windows. Earth worlds need
`curl.exe`, which Windows 10 and 11 include.

## Measured

Linux, AMD Radeon 840M (integrated), 1280 x 720, forest scene, 300 frames
while turning:

| View distance | Chunks | Frame time | Loading, 8 threads | Loading, 1 thread |
| --- | --- | --- | --- | --- |
| 8 | 225 | 0.7 ms | 0.1 s | 0.7 s |
| 16 | 861 | 2.3 ms | 0.5 s | 2.9 s |
| 32 | 3313 | 5.5 ms | 3.3 s | 14.1 s |

Reproduce with `voxelrust --seed 13 --distance 16 --bench 300`. With distant
terrain, view distance 8 plus terrain out to 128 chunks ran at about 800 FPS
on the same machine, against 182 FPS for 32 chunks of full detail.
