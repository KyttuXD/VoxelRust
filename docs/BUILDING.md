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

No mods are built into the program or packed with it. They live in a separate
folder, `VoxelRust-Mods`, and the player installs them in the data folder's
`mods` folder (or a server sends them). A plain `cargo build` never looks at
that folder. To build a program that also contains the Rust code of the mods
there (Earth, Vehicles...), for development or for your own use, turn on the
`native-mods` feature (see [Mod code](#mod-code)):

```sh
cargo build --release --features native-mods
```

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

No mods are copied into the app.

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
| `nightly.txt` | the commit, the release number, and each bare program's SHA-256 and size: what the updater reads |
| `nightly.txt.sig` | the Ed25519 signature of `nightly.txt` (64 bytes): the updater refuses the list without it |
| `BUILDS.txt`, `SHA256SUMS.txt` | for people: which file is for which machine, and checksums |

Each program archive also holds `README.md`, `LICENSE`, `docs/` and `examples/`.
No archive holds any mods: the release is built without the `native-mods`
feature, and players install mods in `<data folder>/mods` (the game makes it,
with a README.txt). The game's own data (`data/`) is inside the
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

### Signing the release

The updater only believes a `nightly.txt` whose `nightly.txt.sig` is a valid
Ed25519 signature (RFC 8032) of the list's exact bytes. The public key is built
into the game (`src/client/update_key.rs`); the private key stays on the
machine that makes releases, outside the repository:

```sh
sh scripts/make-signing-key.sh        # once: ~/.voxelrust/update-signing-key.pem
```

It makes the key (with `openssl genpkey -algorithm ED25519`), refuses to
overwrite one, and prints the public key as hex and as a Rust array. The key
path is `VOXELRUST_SIGN_KEY` (default `~/.voxelrust/update-signing-key.pem`);
`.gitignore` keeps `*.pem` and `update-signing-key*` out of git, and the key must
never be committed or shared. `scripts/package-release.sh` checks at the start
that the key exists **and is the one embedded in the game**, and
`scripts/write-nightly-list.sh` signs through `scripts/sign-file.sh`; with no
key the release is refused rather than made unsigned. **Back the key up.** If
it is lost, games already out cannot be updated (a new key means a new
program, installed by hand once). Needs `openssl` (1.1.1 or 3.x) at release time;
the game itself needs nothing but `curl`.

### A plain-HTTP mirror (for the old-Mac build)

Mac OS X 10.6's `curl` cannot do HTTPS, so the legacy build cannot reach the
release. It can update from a mirror: a folder served over plain HTTP that
holds `nightly.txt`, `nightly.txt.sig` and the bare programs. Since the list is
signed and each program is checked against the list's SHA-256 and size, the
mirror (and the network to it) does not have to be trusted: a changed or
replaced file is refused, with a line in the log, and nothing is installed.
It can only withhold updates, or keep serving an old release (which the game
never installs over a newer one, because of the release number).

1. Serve a folder as plain static files. With nginx:

   ```nginx
   server {
       listen 80;
       server_name updates.example.org;
       location /voxelrust/ { alias /var/www/voxelrust/; }
   }
   ```

   or for a trial, `python3 -m http.server 8000 --bind 0.0.0.0 -d /var/www/voxelrust`.
2. Publish from the release machine, after `scripts/package-release.sh`:

   ```sh
   sh scripts/publish-mirror.sh me@updates.example.org /var/www/voxelrust
   ```

   It copies `nightly.txt`, `nightly.txt.sig` and the `.bin`/`.exe` programs
   from `dist/nightly/` with `rsync` (or `scp`), the programs first and the
   list last. It is not run by any other script.
3. On the old Mac, add a line to `settings.txt` in the game's data folder:

   ```
   update_mirror=http://updates.example.org/voxelrust/
   ```

   (or start the game with `--update-mirror URL`). The legacy build only
   looks for updates when a mirror is set; other builds try the mirror first
   and the GitHub release second. Servers read `update_mirror` from the
   `settings.txt` in their `--data` folder, or take `--update-mirror`.

`scripts/test-update.sh` tests all of this end to end with no network: it
signs with a throwaway key (the programs it builds trust that key's public half
through `VOXELRUST_UPDATE_KEY`, which only the `update-test` build reads),
checks that a changed, foreign-signed or unsigned list is refused, and updates
a program through a plain-HTTP mirror run by `python3 -m http.server`.

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

1. It downloads `nightly.txt` and `nightly.txt.sig` from the mirror, if
   `update_mirror` is set, and else (or if that fails) from
   `https://github.com/KyttuXD/VoxelRust/releases/download/nightly/`, with
   the system's `curl`, into `tmp/update/` in the data folder. It then checks
   the signature against the public key built into the program; a list that
   fails is thrown away unread (and the title screen says an update was
   refused). The file looks like this:

   ```
   commit 1a2b3c4d5e6f
   seq 42
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
| AN UPDATE WAS REFUSED: ITS SIGNATURE IS NOT VALID | a list was found but not signed by the project's key (tampered, or from another key) |
| (no message) | a download failed, the list could not be read, the checksum did not match, or the program could not be replaced |

Switching it off:

- `--no-update` skips the check for that run.
- `update_mirror=URL` in `settings.txt` / `--update-mirror URL` adds a mirror
  (an address of `http://` or `https://`, with a folder; no spaces or quotes).
  There is no line for it in the settings screen, only the file.
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

- **The old-Mac build cannot reach the release on Mac OS X 10.6.** The `curl`
  that comes with 10.6 cannot make modern HTTPS connections. It updates only
  through a plain-HTTP mirror (see above); without `update_mirror`, replace the
  app by hand there.
- **Only the program is replaced.** The docs and SDL2 stay as they were, and
  so do your mods, which are in the data folder.

To update by hand, download the archive with the same build name as the one
you have and replace the program (or the app). Worlds
and settings are kept in the data folder, not beside the program, so they
are untouched. The two Mac builds are different files on purpose: a Mac on
10.6 must keep using `macos-legacy-x86_64`.

## Mod code

Mods are kept outside this repository, in their own repository,
`VoxelRust-Mods`. Most of a mod is text: its `pack.txt`, textures, models and
sounds need no build and are installed by the player (see
[MODDING.md](MODDING.md)). Only a mod with Rust of its own needs the program
built with it, and that is **opt-in**:

```sh
# The Rust code of the mods in ../VoxelRust-Mods is compiled in:
cargo build --release --features native-mods
# Or from another folder (a relative path is taken from the top of this repository):
VOXELRUST_MODS_DIR=/path/to/VoxelRust-Mods cargo build --release --features native-mods
```

Without the `native-mods` feature (the default, and what every release is),
`build.rs` does nothing with mods: no mod code is compiled in, no mod file is
embedded, no mods folder is read, and `cargo test --lib` skips the tests of
the mods. A mod whose pack says `feature <name>` then simply has no code.
With the feature, `build.rs` (`embed_mods`) finds every folder
`<mods>/<name>/code/mod.rs` and compiles each one into the program as a module.
The folder name must be letters, digits or `_`, starting with a letter. The
build does not check git: a local mod that git ignores is compiled in too.
If the folder is missing the build prints a warning and goes on with no native
mods. The game names none of the mods itself.

Each mod offers a `Native` (struct `Native` in `src/native.rs`): a table of
hooks the game calls at fixed points. A hook that is `None` is not called.
The code runs only while a loaded pack says `feature <name>`, so switching
the mod off in SETTINGS > MODS switches its code off too. Mod code cannot be
loaded at run time (and cannot be downloaded from a server): after changing
anything under `code/`, rebuild. Scripts (`.rs` files for the script engine) are
another matter: they can be downloaded (see [MULTIPLAYER.md](MULTIPLAYER.md)).

`SHIPPED` (in the mods folder) names the mods whose files the tests read, one
folder name a line. It is only used by `--features native-mods` test builds:
with the feature, `build.rs` includes their files in the test build and the
mods folder's `tests.rs` is compiled in as a test module, so
`cargo test --lib --features native-mods` checks those mods. Nothing is
shipped with the game.

## Tests

```sh
cargo test --lib
```

The tests are unit tests inside the library. They open no window and need no
SDL2, graphics card or network. (The game itself is `no_std`; the tests build
against `std` so that plain `cargo test` works.) With `--features native-mods` they include the mods folder's `tests.rs`,
the tests of the mods (see [Mod code](#mod-code)); `cargo test --lib` passes
both with and without the feature.

## Faster builds

The game is one crate of about 60,000 lines with no dependencies, so every
build compiles all of it, and `--release` (fat LTO, one code generation unit)
does it on one core. For working on the game, use the faster ways below. None
of them changes a shipped program: releases still use `--release`.

| Want | Command | Notes |
| --- | --- | --- |
| Does it compile? | `cargo check --lib` (alias: `cargo quick`) | no code generation, no linking; add `--tests` to check the tests too |
| Try it in the game | `cargo build --profile fastdev` (alias: `cargo fastdev`) | program in `target/fastdev/voxelrust` |
| Run the tests | `cargo test --lib` | the `test` profile has line tables only, not full debug information |
| Ship | `cargo build --release` | slow on purpose |

**The `fastdev` profile** (`Cargo.toml`) inherits `dev`: optimisation level 1
(playable, unlike level 0), no debug information, incremental compilation, 256
code generation units and no LTO. It is for edit-and-run; it is not what the
speed numbers in this file were measured with, and it is not for shipping.

**One target folder.** Every script in `scripts/` takes the folder from
`CARGO_TARGET_DIR` (default `target`), and so does cargo itself, so several
checkouts or worktrees can share one folder and one compile cache:

```sh
export CARGO_TARGET_DIR=$HOME/.cache/voxelrust-target
cargo check --lib
```

Different profiles, targets and toolchains keep separate files in it. Do not
run two cargo commands in the same folder at once: the second waits for the
first. Programs end up in `$CARGO_TARGET_DIR/release/` and so on, and the
scripts that package or install them look there.

**The linker.** Nothing is set in `.cargo/config.toml`, because there is
nothing on this machine to gain. On x86_64 Linux, Rust 1.90 and later link with
the `rust-lld` that ships with the toolchain by default. `mold` and a separate
`ld.lld` were not installed where this was checked, and a setting that names a
program that is missing breaks every build on a machine without it, so it is
not in the repository. For the old 1.77 toolchain, which still uses the system's
`ld`, or to use mold if you have installed it, put this in your own
`~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

`.cargo/config.toml` only defines the aliases `fastdev` and `quick`; it sets no
flags, so the Windows, macOS and web scripts (which pass their own `RUSTFLAGS`)
are unaffected.

**`build.rs`.** It runs again only when something it reads changes: `build.rs`
itself, the files under `data/`, the environment variables `VOXELRUST_COMMIT`,
`VOXELRUST_SEQ`, `VOXELRUST_RELEASE` and (with `native-mods`)
`VOXELRUST_MODS_DIR` and the mods folder, and git's `HEAD` and the file of the
current branch. The git paths come from `git rev-parse --git-path`, so they
exist in a worktree too (there `.git` is a file; the old fixed paths did not
exist, and cargo ran the script on every build). The files it writes are
replaced only when their text changed, so an unchanged table is not compiled
again. **Each commit changes the commit name built into the program**, which
recompiles the crate once. While you are making many small commits, set
`VOXELRUST_COMMIT=dev` to skip that (the program then says `commit dev`, and
the updater treats it as a build with no known commit: do not do this for a
program you give to someone). The tables under `src/content/painters/`
(`tool_shapes.rs` and the others) are ordinary source files written by the
scripts in `tools/` when you run them; the build never regenerates them.

**Why the crate is not split into several.** A workspace of path crates was
considered (`util/math`, `util/hash`, `util/crypto`, `util/image`, `script`).
The seams are not clean enough to be worth it: the pieces that depend on
nothing (math, hash, crypto) are tiny and compile in a fraction of a second,
and the large ones (`script`, `content`, `game`) reach into the rest through
`crate::sys` (the `logf!` macro and file access), `crate::session`, and the
registry. Cutting them out would mean moving those into a base crate and
changing every `crate::` path in 60,000 lines, with no way to prove the 1.77,
wasm and Mac builds still work. The cost of that, and the extra
`Cargo.toml` and build-script files for scripts that currently take one
`cargo build`, outweigh the gain. If it is done later, the order that works
is: `sys` and `util` first (everything depends on them), then `script`.

**Smaller files.** `client/app.rs` used to be 4,600 lines, so every edit to
the client recompiled and re-checked a file of that size. It is now about
1,560 lines; the rest lives in files by subject, as child modules of `app` (so
they keep access to its private items):

| Folder | Files |
| --- | --- |
| `client/screens/` | `title.rs` (title, worlds, new and edit world, pause, death), `settings.rs`, `join.rs` (servers, hosting), `inventory.rs`, `map.rs`, `overlays.rs` (chat, emotes), `skin.rs`, beside the existing `crafting.rs` |
| `client/frame/` | `input.rs` (keys, events), `update.rs`, `draw.rs`, `creatures.rs`, `hud.rs`, `scripted.rs` (line-up and showcase for pictures) |
| `client/launch/` | `args.rs` (help text, options, parsing), `mods_load.rs` (reading packs and mods, importing, exporting textures), `server_run.rs` (the dedicated server and the hosted one) |

They are declared in `app.rs` with `#[path = "..."] mod name;`. Note that
Rust compiles a crate as a whole; the gain from smaller files is mostly for
people (and for the incremental compiler, which reuses more per function), not
a dramatic drop in compile time. The functions `run` (about 780 lines, start-up
and the main loop in one closure) and `screen_settings` and `screen_inventory`
(about 260 lines each) are still long: `run` shares dozens of local variables
with the loop closure and was left whole.

**Timings.** TODO (the tester fills this in): clean `cargo build --release`,
clean and incremental `cargo build --profile fastdev`, and `cargo check --lib`,
before and after, on the same machine.

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
program (`build.rs` embeds them): the core. The rest of the content is the mods of the
`VoxelRust-Mods` folder, written by `tools/export_mods.py`. These scripts write them:

| Script | Writes | Needs |
| --- | --- | --- |
| `python3 tools/gen_base.py` | `data/base.txt` (the core) and `data/effects.txt`. It builds the whole content from tables (sixteen colours times several materials, seven woods, stairs and slabs of each, doors, beds, furniture, crops, tools, food, sounds, dimensions) and `tools/modsplit.py` keeps in the core only what the game needs to boot and play | Python 3 |
| `python3 tools/export_mods.py [DIR]` | the mods stonework, woods, colours, caves, explosives, farming, cooking, tools-extra, furniture (from `gen_base.py`) and wildlife, monsters, villagers, wanderer (from `gen_creatures.py`) into `DIR` (default `$VOXELRUST_MODS_DIR`, else `../VoxelRust-Mods`): each a `pack.txt`, a `README.txt` and for creatures their models and pictures. Which mod owns what is the table at the top of `tools/modsplit.py`; it also stops with an error if a mod would refer to a later one | Python 3 |
| `python3 tools/gen_models.py` | the creature models into the wildlife mod's `models/` (then `export_mods.py` sorts each into its mod); the vehicle models and those of the other mods go to `<mod>/models` | Python 3 with Pillow |
| `python3 tools/entity_textures.py` | the picture of every creature look into the wildlife mod's `textures/`, and the player's and the skins' into `data/textures/entity/`, painted in the OCD style from the colour table `tools/entity_palettes.py` (see [TEXTURING.md](TEXTURING.md#creatures-and-players)) | Python 3 with Pillow |
| `python3 tools/gen_creatures.py` | runs `export_mods.py --creatures`: every creature, where it lives and how it behaves, written into wildlife, monsters, villagers and wanderer | Python 3 |

Set `VOXELRUST_MODS_DIR` to the mods folder for all of them. To remake everything:
`gen_models.py`, `entity_textures.py`, `gen_base.py`, then `export_mods.py` (which
also does the creatures).

`tools/gen_models.py --sheet out.png` also renders a contact sheet of the
models, and `--only wolf,fox` remakes just those. `data/base.txt` and the mods'
`pack.txt` may be edited by hand as well, but a later run of their
script will overwrite such edits; change the script instead. Run
`gen_models.py` before `export_mods.py` when adding a model, since the
latter only uses models whose files exist, and `entity_textures.py` before
it when adding a look, since it checks the pictures exist.

Rebuild after changing anything under `data/`.

Mods are **not** embedded in the data. Their packs are read from the mods
folder when the game starts (or come from a server), and their Rust code, if
any, is compiled in only with `--features native-mods` (see
[Mod code](#mod-code)). Each is a folder written by hand and shows in
SETTINGS > MODS under its folder name. They are in the `VoxelRust-Mods`
repository, not this one. Changing a `pack.txt` needs no
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
