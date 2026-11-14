# Building

## What you need

- **Rust.** `Cargo.toml` declares `rust-version = "1.71"`. The oldest
  toolchain the code is actually tested with is **1.77**; current stable
  works too.
- **No crates.** There is nothing to download: `Cargo.toml` has no
  dependencies.
- **Python 3**, only for the Mac OS X 10.6 build, the icon script and regenerating data.
- **SDL2 is not needed to build**, only to run: the game opens it when it
  starts. See [PORTING.md](PORTING.md) for where it looks.

## Linux

```sh
cargo build --release
```

The program is `target/release/voxelrust`. To run it, install the SDL2
runtime library (`libsdl2-2.0-0` on Debian and Ubuntu).

The mods are not in this repository. They live in a separate folder,
`VoxelRust-Mods`, which the build looks for beside the repository
(`../VoxelRust-Mods`) or in the folder named by the environment variable
`VOXELRUST_MODS_DIR` (see [Mod code](#mod-code)). Without it the build still
works, with a warning, and the game has no native mods.

The mods that come with the game are not inside the program either: they are
the folders named in `SHIPPED` in the mods folder. The game finds them in a
`mods` folder beside the program or in `./mods`, so copy those folders into a
`mods` folder next to the program (the release scripts do). Without them the
game still runs, with no Earth worlds, disasters, boats, trains, planes, land
vehicles or holidays.

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

Each wraps one of the two Mac programs (if the legacy one is missing, both
are built first) as a double-clickable app, with the icon from `assets/icon.icns`
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

The script also copies the mods listed in `SHIPPED` (in the mods folder, see
[Mod code](#mod-code)) into the app, as
`Contents/Resources/mods`, which is one of the places the game looks.

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

Each program archive also holds `README.md`, `LICENSE`, `docs/` and `examples/`.
The Linux and Windows archives hold the mods listed in `SHIPPED` in the mods
folder (one folder name a line; `scripts/package-release.sh` copies each one
from `VOXELRUST_MODS_DIR`, default `../VoxelRust-Mods`, and refuses to run
without it) in a `mods/`
folder beside the program; the Mac apps carry them inside, in
`Contents/Resources/mods`. The game's own data (`data/`) is inside the
program. SDL2 is not in the Linux archive; the Windows and Mac files include
it when `vendor/sdl2` holds it (see `vendor/README.md`).

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

A program built without `VOXELRUST_COMMIT` in the environment takes the
commit from git (plain `cargo build` in a checkout). Only outside git does it
say `commit dev`. The title screen shows version, build name and commit at
the bottom.

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
| THIS IS THE NEWEST VERSION | the release is not newer (by its release number), or has nothing for this build name |
| A NEWER VERSION IS INSTALLED - THE GAME RESTARTS WHEN YOU QUIT | replaced; it starts when you quit, never during play |
| UPDATED - NOW RUNNING THE NEWEST VERSION | this copy was started by an update |
| A NEWER VERSION IS OUT (AUTOMATIC UPDATES ARE OFF) | there is one, and it was not installed |
| (no message) | a download failed, the list could not be read, the checksum did not match, or the program could not be replaced |

Switching it off:

- `--no-update` skips the check for that run.
- UPDATE THE GAME AUTOMATICALLY in SETTINGS > MODS (`auto_update` in
  `settings.txt`, on by default): when off, the game still checks but only
  reports that a newer version is out.
- A program built by hand with no release number is never replaced; it too
  only reports a newer version. A release replaces a program only if its
  number (`seq` in `nightly.txt`) is larger, so an old list cannot move a
  program back. The new program must pass its checksum and `--stamp` check.
- A server (`--server`) updates itself as well, and restarts into the update
  once nobody is connected, with the same options.
- There is no check at all in scripted runs (`--screenshot`, `--bench`),
  for `--server`, or in the browser.

What it cannot do:

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

## Mod code

Mods are kept outside this repository, in their own repository,
`VoxelRust-Mods`. `build.rs` reads that folder from the environment variable
`VOXELRUST_MODS_DIR` (a relative path is taken from the top of this
repository), and when it is not set from `../VoxelRust-Mods`, beside the
repository. If the folder is missing the build prints a warning and goes on
with no native mods and no shipped mods, and the tests of the shipped mods
skip themselves. `scripts/` read the same variable.

```sh
VOXELRUST_MODS_DIR=/path/to/VoxelRust-Mods cargo build --release
```

A mod is a folder in the mods folder. Its `pack.txt` is plain text and needs no
build. A mod that needs Rust brings it in `<mods>/<name>/code/mod.rs`, and may
have more files beside it (`vehicles/code/physics.rs` does). At build
time `build.rs` (`embed_mods`) finds every folder whose `code/mod.rs` exists
and compiles each one into the program as a module. The folder name must be
letters, digits or `_`, starting with a letter. The build does not check git:
a local mod that git ignores is compiled in too. The game names none of the
mods itself.

Each mod offers a `Native` (struct `Native` in `src/native.rs`): a table of
hooks the game calls at fixed points. A hook that is `None` is not called.
The code runs only while a loaded pack says `feature <name>`, so switching
the mod off in SETTINGS > MODS switches its code off too. Mod code cannot be
loaded at run time: after changing anything under `code/`, rebuild.

Two shipped mods have code: `earth` and `vehicles`.

`SHIPPED` (in the mods folder) lists the mods that come with the game, one folder name a
line: `disasters`, `earth`, `vehicles`, `boats`, `trains`, `planes`,
`landvehicles` and `holidays`. The release scripts copy them (see
[Packaging a release](#packaging-a-release)). `build.rs` also includes their
files in the test build only. The mods folder's `tests.rs` is compiled in as a test module,
so `cargo test --lib` checks the shipped mods (it skips with a message when
the folder is missing).

## Tests

```sh
cargo test --lib
```

The tests are unit tests inside the library. They open no window and need no
SDL2, graphics card or network. (The game itself is `no_std`; the tests build
against `std` so that plain `cargo test` works.) They include the mods folder's `tests.rs`,
the tests of the shipped mods (see [Mod code](#mod-code)).

## Performance notes

Where the time and the memory go, and what has been done about it. Run with
`--log FILE` to see the start-up timings: one line for each phase before the first
frame, `voxelrust: startup  <ms since start>  <phase> (+<ms for this phase>)`:
content parsed (base data and mods), texture packs indexed, window and OpenGL, the
renderer (the atlas is painted and uploaded here), audio and mods started, and the
first frame. A slow start can be pinned on one of them.

- **Models are parsed when first drawn**, not at start-up. A mod with hundreds of
  creature models (`orespawn`, `witherstorm`) used to parse every model file and
  upload every picture before the first frame. `Renderer::load_models` now only
  forgets the old ones (and gives their textures back to the card); `ensure_model`
  parses each on its first use. A model that will not parse is logged then, and is
  drawn as a copy of the first one that does.
- **The bundle of packs for joining players is made on demand** (`client/pack_text.rs`).
  At start-up only the names of the files the packs pulled in are kept; the files are
  read and copied when the player hosts a game or a server's pack is compared.
  Before, every model and picture of every mod was cloned twice at start.
- **Things that were leaked on every pack reload are freed.** The old content registry
  (`block::replace`), the texture pack index, the list of files in `textures/`, the
  alpha masks of the atlas and the creature picture index are each kept for one more
  change, so that a reader still holding a reference is safe, and freed at the one
  after. Model and creature pictures are deleted from the graphics card when models
  are forgotten, and a player's skin picture is written over, not made anew.
- **Lookups made every frame are indexed**: `Registry::tile(id)` and `Registry::model(id)`
  instead of searching hundreds of names (the hand, the arms, distant terrain).
  The texture pack index is a map, not a list searched (and filtered) file by file.
- **Saves are atomic**: world regions, the world's own files and the settings are
  written to `<file>.tmp` and moved over the old file (`sys::write_file_atomic`), so a
  crash in the middle of a save cannot leave half a file.
- Things sent by other players are bounded: creature snapshots (64), chests (65536),
  content packs (16 MB, and a pack that is too big is dropped whole), positions (a
  world coordinate, finite and below 2.2e9).

What is still heavy, if the start is slow: the content registry keeps the text of
every model file (`Registry::models`) for the whole run; the atlas is painted on the
main thread (a texture pack with big pictures is decoded and shrunk there); the window
opens only after the content is read, so nothing shows while that runs.

## GitHub workflows

The repository is on GitHub as `KyttuXD/VoxelRust`, and it is public. Two
workflows are in `.github/workflows`. **Neither has been able to run**:
the account's GitHub Actions budget blocks them. Treat both workflows as
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
program (`build.rs` embeds them). Four scripts write those files:

| Script | Writes | Needs |
| --- | --- | --- |
| `python3 tools/gen_base.py` | `data/base.txt`: the textures, blocks, items and recipes, generated from tables (sixteen colours times several materials, six woods, stairs and slabs of each, doors, beds, furniture, rail track, crops, tools, food, sounds, dimensions) | Python 3 |
| `python3 tools/gen_models.py` | `data/models/*.bbmodel` (creature models; the vehicle models and those of the other mods go to `mods/<name>/models`). With `--creatures` it also rewrites `data/creatures.txt`, the list of base models | Python 3 with Pillow |
| `python3 tools/entity_textures.py` | `data/textures/entity/*.png`: the picture of every creature look and player skin, painted in the OCD style from the colour table `tools/entity_palettes.py` (see [TEXTURING.md](TEXTURING.md#creatures-and-players)) | Python 3 with Pillow |
| `python3 tools/gen_creatures.py` | `data/wildlife.txt`: every creature, where it lives and how it behaves | Python 3 |

`tools/gen_models.py --sheet out.png` also renders a contact sheet of the
models, and `--only wolf,fox` remakes just those. `data/base.txt` and
`data/wildlife.txt` may be edited by hand as well, but a later run of their
script will overwrite such edits; change the script instead. Run
`gen_models.py` before `gen_creatures.py` when adding a model, since the
latter only uses models whose files exist, and `entity_textures.py` before
`gen_creatures.py` when adding a look, since the latter checks the pictures exist.

Rebuild after changing anything under `data/`.

The mods that come with the game are **not** embedded in the data. Their
packs are read from the `mods` folders when the game starts, and their code
is compiled in (see [Mod code](#mod-code)). Each is a folder written by hand,
named in `SHIPPED`, and shows in SETTINGS > MODS under its folder name.
All of them are in the `VoxelRust-Mods` repository, not this one. Changing a `pack.txt` needs no
rebuild: `/reload` reads it again. Changing the Rust under `code/` does.

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
