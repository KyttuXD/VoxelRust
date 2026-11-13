# Building

## What you need

- **Rust.** `Cargo.toml` declares `rust-version = "1.71"`. The oldest
  toolchain the code is actually tested with is **1.77**; current stable
  works too.
- **No crates.** There is nothing to download: `Cargo.toml` has no
  dependencies.
- **Python 3**, only for the Mac OS X 10.6 build and for regenerating data.
- **SDL2 is not needed to build**, only to run: the game opens it when it
  starts. See [PORTING.md](PORTING.md) for where it looks.

## Linux

```sh
cargo build --release
```

The program is `target/release/voxelrust`. To run it, install the SDL2
runtime library (`libsdl2-2.0-0` on Debian and Ubuntu).

## Windows (64-bit)

```sh
scripts/build-windows.sh
```

Works from any host (Linux, macOS, or a POSIX shell on Windows). It needs the
`x86_64-pc-windows-msvc` Rust target (`rustup target add
x86_64-pc-windows-msvc`) and nothing else: no Visual Studio, Windows SDK or
MinGW. It links with `rust-lld`, the linker shipped inside the Rust
toolchain.

Result: `dist/windows/voxelrust.exe`. Put a 64-bit `SDL2.dll` (from
libsdl.org) next to it. The program carries the game's icon: `build.rs` turns
`assets/icon.ico` into a resource and links it in, with no resource compiler.

## macOS and Mac OS X (Intel)

```sh
scripts/build-macos-legacy.sh
```

Works from any host. It needs the `x86_64-apple-darwin` Rust target and
Python 3; no Xcode and no Apple SDK. It produces two programs:

- `dist/macos-legacy/voxelrust`: for **Mac OS X 10.6 and later**. Built for
  the `core2` processor, then passed through `scripts/macho_legacy.py`, which
  rewrites the program's entry point into the form the 10.6 and 10.7 loaders
  accept.
- `dist/macos/voxelrust`: an ordinary build for macOS 10.12 and later.

How and why this works is explained in [PORTING.md](PORTING.md).

On a Mac you can also just `brew install sdl2` and `cargo build --release`.

Apple Silicon is not built by any script.

### A particular toolchain

Both scripts take an optional toolchain name, passed to cargo as
`+<toolchain>`:

```sh
scripts/build-macos-legacy.sh 1.77.0
```

```sh
scripts/build-windows.sh 1.77.0
```

### Packaging the Mac apps

```sh
scripts/package-macos-app.sh [path/to/SDL2.framework]
```

```sh
VARIANT=modern scripts/package-macos-app.sh [path/to/SDL2.framework]
```

Each wraps one of the two Mac programs (building them first if they are
missing) as a double-clickable app, with the icon from `assets/icon.icns`
and the version from `Cargo.toml`:

| Command | Wraps | Result | Declares |
| --- | --- | --- | --- |
| default | `dist/macos-legacy/voxelrust` | `dist/VoxelRust.app` | Mac OS X 10.6 or later |
| `VARIANT=modern` | `dist/macos/voxelrust` | `dist/macos/VoxelRust.app` | macOS 10.12 or later |

The minimum system is written into each app's `Info.plist`
(`LSMinimumSystemVersion`), so macOS itself refuses to start the modern app
on a system older than 10.12. If you give the script an `SDL2.framework`,
it is copied inside the app, which then needs nothing else installed. For
Mac OS X 10.6 use the official SDL2 2.0.4 framework.

## Build names and updating

Every build has a name, which says what kind of machine it is for:

| Build name | For | Made by |
| --- | --- | --- |
| `linux-x86_64` | Linux, 64-bit Intel/AMD | `cargo build --release` |
| `windows-x86_64` | Windows, 64-bit | `scripts/build-windows.sh` |
| `macos-x86_64` | macOS 10.12 and later, Intel | `scripts/build-macos-legacy.sh` |
| `macos-legacy-x86_64` | Mac OS X 10.6 and later, 64-bit Intel | `scripts/build-macos-legacy.sh` |

The game prints its name with `--version` and shows it on the F3 screen:

```
$ voxelrust --version
voxelrust 0.1.0 (linux-x86_64)
```

Nightly files are named after it (`voxelrust-<build name>`), and
`BUILDS.txt` in the nightly release lists them.

**There is no updater in the game.** It never downloads or replaces itself.
To update, download the file with the same build name as the one you have
and replace the program (or the app) with it. Worlds and settings are kept
in the data folder, not beside the program, so they are untouched. The two
Mac builds are different files on purpose: a Mac on 10.6 must keep using
`macos-legacy-x86_64`.

## Tests

```sh
cargo test --lib
```

The tests are unit tests inside the library. They open no window and need no
SDL2, graphics card or network. (The game itself is `no_std`; the tests build
against `std` so that plain `cargo test` works.)

## GitHub workflows

The repository is on GitHub as `KyttuXD/VoxelRust`, and it is private. Two
workflows are in `.github/workflows`. **Neither has been able to run yet**:
the account's GitHub Actions budget blocks runs on private repositories.
(Public repositories run Actions free of charge.) Treat both workflows as
untested until a first run has passed; until then there are no nightly
files.

### `ci.yml`

Runs on every push and pull request (except changes that only touch `docs/`
or Markdown files), and by hand. On an Ubuntu runner, once with stable Rust
and once with Rust 1.77.0, it runs:

1. `cargo test --lib`
2. `cargo build --release`

It publishes nothing.

### `nightly.yml`

Runs every night at 03:00 UTC, and by hand from the Actions tab.

1. A first job compares the commit the `nightly` tag points at with the
   current commit. A scheduled run stops there if nothing has changed; a run
   started by hand always builds.
2. On one Ubuntu runner with stable Rust it runs `cargo test --lib`, builds
   Linux (`cargo build --release`), Windows (`scripts/build-windows.sh`) and
   both Mac programs (`scripts/build-macos-legacy.sh`).
3. It wraps both Mac programs as apps (`scripts/package-macos-app.sh`, once
   plain and once with `VARIANT=modern`) and packages four archives, each
   holding the program or app plus `README.md`, `LICENSE`, `docs/` and
   `examples/`. The Linux archive also holds `voxelrust.desktop`, `icon.png`
   and the icon in the sizes Linux desktops use. The game's data is inside
   the program, so no `data/` folder is shipped.

   | File | Build name |
   | --- | --- |
   | `voxelrust-linux-x86_64.tar.gz` | `linux-x86_64` |
   | `voxelrust-windows-x86_64.zip` | `windows-x86_64` |
   | `voxelrust-macos-x86_64.zip` | `macos-x86_64` |
   | `voxelrust-macos-legacy-x86_64.zip` | `macos-legacy-x86_64` |

   With them go `BUILDS.txt` (which file is for which machine, and the
   commit) and `SHA256SUMS.txt`.
4. It uploads them to a rolling pre-release called `nightly` under Releases
   (created on first use), moving the `nightly` tag to the commit that was
   built and replacing the previous files. Only the default branch is
   published; a hand-started run on another branch builds and stops.

SDL2 is not included in the archives.

## Regenerating data

The game only ever reads the files under `data/`, which are compiled into the
program (`build.rs` embeds them). Three scripts write those files:

| Script | Writes | Needs |
| --- | --- | --- |
| `python3 tools/gen_base.py` | `data/base.txt`: the textures, blocks, items, recipes and vehicles, generated from tables (sixteen colours times several materials, six woods, stairs and slabs of each, doors, beds, furniture, rail track, crops, tools, food, sounds, dimensions) | Python 3 |
| `python3 tools/gen_models.py` | `data/models/*.bbmodel` (creature and vehicle models). With `--creatures` it also rewrites `data/creatures.txt`, the list of base models | Python 3 with Pillow |
| `python3 tools/gen_creatures.py` | `data/wildlife.txt`: every creature, where it lives and how it behaves | Python 3 |

`tools/gen_models.py --sheet out.png` also renders a contact sheet of the
models, and `--only wolf,fox` remakes just those. `data/base.txt` and
`data/wildlife.txt` may be edited by hand as well, but a later run of their
script will overwrite such edits; change the script instead. Run
`gen_models.py` before `gen_creatures.py` when adding a model, since the
latter only uses models whose files exist.

Rebuild after changing anything under `data/`.

The mods that come with the game are embedded the same way: each
`data/packs/<name>/pack.txt` (`disasters`, `earth`, `landvehicles`) is
written by hand and listed in SETTINGS > MODS under its folder name.

## Icons

The game draws its own emblem. `--write-icon` saves it as a PNG:

```sh
voxelrust --write-icon icon.png --icon-size 512
```

(256 pixels if `--icon-size` is left out; 8 to 1024.) The window's icon is
set from the same drawing when the game starts, so it needs no file.

`tools/gen_icons.py` runs the built game to make the icon files under
`assets/`:

```sh
cargo build --release && python3 tools/gen_icons.py
```

| File | Used for |
| --- | --- |
| `assets/icon.png` | 256 x 256 picture; shipped in the Linux archive |
| `assets/icon.ico` | Windows: sizes 16 to 256, embedded in `voxelrust.exe` by `build.rs` |
| `assets/icon.icns` | the Mac apps. It holds both kinds of entry, the old packed ones that Mac OS X 10.6 reads and the PNG ones newer systems use, so the icon shows on old and new Macs |
| `assets/hicolor/<size>x<size>/apps/voxelrust.png` | Linux desktops: 16, 24, 32, 48, 64, 128, 256 and 512 pixels |

`assets/voxelrust.desktop` is the Linux menu entry (it names the icon
`voxelrust` and the program `voxelrust`). To install by hand, copy it to
`~/.local/share/applications/`, the `hicolor` folders to
`~/.local/share/icons/hicolor/`, and put the program on your `PATH`.
