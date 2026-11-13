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

The three mods that come with the game are not inside the program: they are
the folders `mods/disasters`, `mods/earth` and `mods/landvehicles`. The game
finds them in a `mods` folder beside the program or in `./mods`, so run a
program you built from the top of the source tree (`cargo run --release`
does), or copy those three folders into a `mods` folder next to it. Without
them the game still runs, with no Earth worlds, disasters or land vehicles.

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

The script also copies the three mods that come with the game into the app,
as `Contents/Resources/mods`, which is one of the places the game looks.

## Packaging a release

```sh
scripts/package-release.sh [toolchain]
```

One script builds every version (Linux, Windows, both Mac programs and their
apps, and the browser version if its target is installed) and lays the files
of a release out in `dist/nightly/`:

| File | What it is |
| --- | --- |
| `voxelrust-linux-x86_64.tar.gz` | the Linux program, with `voxelrust.desktop`, `icon.png` and the icon in the sizes Linux desktops use |
| `voxelrust-windows-x86_64.zip` | `voxelrust.exe` |
| `voxelrust-macos-x86_64.zip` | `VoxelRust.app` for macOS 10.12 and later |
| `voxelrust-macos-legacy-x86_64.zip` | `VoxelRust.app` for Mac OS X 10.6 and later |
| `voxelrust-web.zip` | the browser version (`dist/web`) |
| `voxelrust-<build name>.bin`, `voxelrust-windows-x86_64.exe` | the bare programs, for the game's updater |
| `nightly.txt` | the commit, and each bare program's SHA-256 and size: what the updater reads |
| `BUILDS.txt`, `SHA256SUMS.txt` | for people: which file is for which machine, and checksums |

Every archive also holds `README.md`, `LICENSE`, `docs/` and `examples/`.
The Linux and Windows archives hold the three mods that come with the game
in a `mods/` folder beside the program; the Mac apps carry them inside, in
`Contents/Resources/mods`. The game's own data (`data/`) is inside the
program. SDL2 is not included.

The script takes the commit from git (or from `VOXELRUST_COMMIT` if that is
set) and builds it into every program; that is what `--version` prints and
what the updater compares.

The files are on the `nightly` release,
<https://github.com/KyttuXD/VoxelRust/releases/tag/nightly>. They are
uploaded by hand, because the workflows cannot run (see
[GitHub workflows](#github-workflows)):

```sh
gh release upload nightly --clobber dist/nightly/*
```

## Build names and updating

Every build has a name, which says what kind of machine it is for:

| Build name | For | Made by |
| --- | --- | --- |
| `linux-x86_64` | Linux, 64-bit Intel/AMD | `cargo build --release` |
| `windows-x86_64` | Windows, 64-bit | `scripts/build-windows.sh` |
| `macos-x86_64` | macOS 10.12 and later, Intel | `scripts/build-macos-legacy.sh` |
| `macos-legacy-x86_64` | Mac OS X 10.6 and later, 64-bit Intel | `scripts/build-macos-legacy.sh` |

The game prints its name with `--version`, together with the commit it was
built from, and shows the name on the F3 screen:

```
$ voxelrust --version
voxelrust 0.1.0 (linux-x86_64, commit 5f38cb02e2e3)
```

A program built without `VOXELRUST_COMMIT` in the environment (plain `cargo
build`) says `commit dev`. The title screen shows version, build name and
commit in its corner.

Nightly files are named after the build name (`voxelrust-<build name>`), and
`BUILDS.txt` in the nightly release lists them.

### How the game updates itself

When the game starts it looks at the nightly release in the background
(`src/client/update.rs`):

1. It downloads `nightly.txt` from
   `https://github.com/KyttuXD/VoxelRust/releases/download/nightly/` with
   the system's `curl`, into the data folder. The file looks like this:

   ```
   commit 1a2b3c4d5e6f
   linux-x86_64 voxelrust-linux-x86_64.bin <sha256> <bytes>
   windows-x86_64 voxelrust-windows-x86_64.exe <sha256> <bytes>
   ```

2. If the commit listed is the one the program was built from, it is up to
   date.
3. Otherwise it takes the line that begins with its own build name, and
   only that one, so the build for old Macs is never replaced by the one for
   new Macs. A line is used only if the file name is a plain name (no
   path) and the checksum is 64 hexadecimal digits.
4. It downloads that file as `<program>.new` beside the running program,
   checks its size and SHA-256 against the list, and puts it in the
   program's place. (On Windows the running program is first moved aside as
   `<program>.old`.) The new version is in use **from the next start**.

The title screen shows how far this has got:

| Message | Meaning |
| --- | --- |
| CHECKING FOR A NEWER VERSION... | the check is running |
| THIS IS THE NEWEST VERSION | same commit, or the release has nothing for this build name |
| A NEWER VERSION IS INSTALLED - START THE GAME AGAIN TO USE IT | replaced |
| A NEWER VERSION IS OUT (AUTOMATIC UPDATES ARE OFF) | there is one, and it was not installed |
| COULD NOT CHECK FOR UPDATES | a download failed, the list could not be read, the checksum did not match, or the program could not be replaced |

Switching it off:

- `--no-update` skips the check for that run.
- UPDATE THE GAME AUTOMATICALLY in SETTINGS > MODS (`auto_update` in
  `settings.txt`, on by default): when off, the game still checks but only
  reports that a newer version is out.
- A program built by hand with no commit stamp (`commit dev`) is never
  replaced; it too only reports a newer version.
- There is no check at all in scripted runs (`--screenshot`, `--bench`),
  for `--server`, or in the browser.

What it cannot do:

- **While the repository is private, it does not work.** The download is
  anonymous, GitHub refuses it, and the game says COULD NOT CHECK FOR
  UPDATES.
- **The old-Mac build cannot update itself on Mac OS X 10.6.** The `curl`
  that comes with 10.6 cannot make modern HTTPS connections. Replace the app
  by hand there.
- **Only the program is replaced.** The `mods` folder that came with the
  game (`mods/` beside the program, or `Contents/Resources/mods` in a Mac
  app), the docs and SDL2 stay as they were. If a newer version changes the
  mods that come with it, download the archive.

To update by hand, download the archive with the same build name as the one
you have and replace the program (or the app) and its `mods` folder. Worlds
and settings are kept in the data folder, not beside the program, so they
are untouched. The two Mac builds are different files on purpose: a Mac on
10.6 must keep using `macos-legacy-x86_64`.

## Tests

```sh
cargo test --lib
```

The tests are unit tests inside the library. They open no window and need no
SDL2, graphics card or network. (The game itself is `no_std`; the tests build
against `std` so that plain `cargo test` works.)

## GitHub workflows

The repository is on GitHub as `KyttuXD/VoxelRust`, and it is private. Two
workflows are in `.github/workflows`. **Neither has been able to run**:
the account's GitHub Actions budget blocks runs on private repositories.
(Public repositories run Actions free of charge.) Treat both workflows as
untested until a first run has passed. Until then the nightly files are
built on the author's machine with `scripts/package-release.sh` and uploaded
by hand (see [Packaging a release](#packaging-a-release)).

Both workflows keep compiled code between runs (`actions/cache` on `target`
and the cargo folders), so a run after a small change does not rebuild
everything.

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
2. On one Ubuntu runner with stable Rust it runs `cargo test --lib`.
3. It runs `scripts/package-release.sh`, the same script used by hand, which
   builds every version and fills `dist/nightly/` (see
   [Packaging a release](#packaging-a-release)).
4. It uploads `dist/nightly/*` to a rolling pre-release called `nightly`
   under Releases (created on first use), moving the `nightly` tag to the
   commit that was built and replacing the previous files. Only the default
   branch is published; a hand-started run on another branch builds and
   stops.

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

The mods that come with the game are **not** embedded. Each is a folder
written by hand, `mods/disasters`, `mods/earth` and `mods/landvehicles`,
with a `pack.txt` (Land Vehicles also has `models/car.bbmodel`), and is
listed in SETTINGS > MODS under its folder name. They are the only folders
under `mods/` that git tracks (`.gitignore` ignores the rest). Changing one
needs no rebuild: `/reload` reads it again.

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
