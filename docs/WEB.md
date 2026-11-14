# The browser version

VoxelRust runs in a web browser as WebAssembly, drawing with WebGL. It is
the same Rust source as every other version: no crates, no bundler, no npm,
no wasm-bindgen, no emscripten. What is added is a small platform layer in
Rust (`src/sys/web/`) and one hand-written JavaScript file (`web/`).

## Building

```sh
scripts/build-web.sh            # or: scripts/build-web.sh 1.99.1
```

Needs a Rust toolchain with the `wasm32-unknown-unknown` target (`rustup
target add wasm32-unknown-unknown`). It has been built with Rust 1.99.1; the
desktop versions' oldest tested toolchain, 1.77, has not been tried for
this target. Python 3, if present, is used for a check after the build (see
below).

The result is the folder `dist/web/`:

| File | What it is |
| --- | --- |
| `index.html` | The page: a canvas, a click-to-start screen, an error box |
| `voxelrust.js` | The browser side of the platform layer |
| `voxelrust.wasm` | The game (about 2.4 MB; a little under 1 MB when the server compresses it) |
| `icon.png`, `favicon.ico` | The icon |

Under the script is one ordinary command, `cargo build --release --target
wasm32-unknown-unknown`, with two settings: an 8 MB stack (the linker's
default is 1 MB) and the build name `web`.

## Running it locally

A browser will not load WebAssembly from a `file://` address, so serve the
folder:

```sh
python3 -m http.server -d dist/web
```

and open <http://localhost:8000/>. Any static web server does.

## Publishing

**The website is not deployed.** The address meant for it is
`voxelrust.games.kyttuslab.org`, but its server could not be reached over
SSH without credentials, so nothing has been put there. The build exists
only as `dist/web/` on the machine that made it.

To publish it anywhere, copy the contents of `dist/web/` to any static host (GitHub Pages, an S3
bucket, a folder on a web server). There is nothing to configure: the page
loads no outside resources and needs no special headers, only that `.wasm`
files are served at all (every common server does; the page does not depend
on their MIME type). Turning on compression for `.wasm` more than halves the
download.

Worlds are stored per site address: the same page at another address starts
with none.

## What works and what does not

Works: single-player worlds in survival and creative, saving, settings and
key bindings, sound, chat commands, full screen (F11), the 3D DETAIL setting,
screenshots (F2 downloads a `.bmp`).

| Not in the browser | Why | What happens |
| --- | --- | --- |
| Joining a server | A page cannot open TCP sockets | JOIN says COULD NOT CONNECT (NOT POSSIBLE IN A BROWSER) |
| Hosting a game, `--server` | The same | HOST A GAME says COULD NOT LISTEN... |
| Earth worlds | The Earth mod is a folder, and the map data is fetched by running `curl`; a page has no folders and cannot run programs | The EARTH terrain is not offered |
| Mods and packs from a folder | There are no folders (`list_dir` finds nothing) | Only the built-in content. Mods are folders, so there are none in the browser version: no land vehicles, no disasters, and SETTINGS > MODS says NO MODS INSTALLED |
| Updating itself | The page is the program | No check is made; reload the page to get what the site serves |
| Command-line options | There is no command line | The game starts at the title screen |
| Worker threads | The page runs the game on its one thread | Terrain is made a little each frame: loading takes longer, and the frame rate dips while new terrain arrives |
| EXPORT / IMPORT of worlds | They read and write a path, which here is browser storage | Not useful |

Other differences:

- **Storage is small.** Files are kept in `localStorage` (keys beginning
  `voxelrust:`, values in base64), which browsers limit to about 5 MB per
  site, so about 3.5 MB of files. Only chunks the player has changed are
  saved, so that is a lot of building, but not unlimited: when it is full
  the game reports that the world could not be written, as for a full
  disk. Clearing the site's data deletes the worlds. In a private window
  with storage switched off the game still runs, and keeps nothing.
- **The mouse has to be handed over by the browser.** To look around, the
  game captures the mouse, and a browser only allows that during a click or
  a key press. So while the game wants the mouse and does not have it, the
  page asks again on every click and every key press (except Esc). Until
  the browser hands it over, a line at the top of the page says `Click the
  game to look around with the mouse`.
- **Where the capture does not last.** Some places (a browser view embedded
  in another program, some remote desktops) grant the capture and take it
  straight back. When that has happened twice, each within 0.7 seconds of
  being asked for or granted, the page stops asking and the game looks
  around by plain mouse movement over the page instead. The view then stops
  turning when the pointer reaches the edge of the page.
- **Esc belongs to the browser** while the mouse is captured: it frees the
  mouse, and the game opens its pause menu. Clicking BACK TO THE GAME
  captures it again; browsers refuse that for a second or so after Esc, in
  which case the next click or key press does it.
- **Ctrl+W closes the tab**, and Ctrl is SPRINT. The page asks before
  closing while a game is in progress; in full screen, browsers that can
  (Chrome, Edge) hand the game every key; or choose another sprint key in
  SETTINGS > CONTROLS.
- **Closing the tab saves** the open world and the settings, as QUIT does.
  After QUIT the page has to be reloaded to play again.
- The window is the page; FULL SCREEN resolutions, VSYNC and FRAME RATE
  LIMIT do nothing (the browser shows frames in step with the display).
- Text is typed a key at a time; pasting and input methods are not
  supported.

## How it works

On every other system the game takes three things from outside: a few C
library functions, SDL2 (opened at run time and looked up by name), and
OpenGL 1.5 (looked up through SDL). The browser build supplies the same
three, so the game code above `src/sys/` is unchanged apart from its main
loop:

- **C library** (`src/sys/web/mod.rs`, `heap.rs`): `malloc`, `realloc`,
  `free`, `calloc`, `puts`, `abort`, `getenv`, `system`. The allocator is a
  boundary-tag one over `memory.grow`; it is plain Rust and is tested with
  the other unit tests on every platform.
- **SDL2** (`src/sys/web/sdl.rs`): the functions the game uses, found by
  name like the real ones, so `src/sys/sdl.rs` and `thread.rs` are used as
  they are. Starting a thread fails, which the job pool already handles by
  working on the caller's thread. Files (SDL's `RWops`) go to
  `localStorage`.
- **OpenGL** (`src/sys/web/gl.rs` and `web/voxelrust.js`): each call is
  imported from JavaScript, which keeps the fixed-function state (matrices,
  the vertex, colour and texture-coordinate arrays, two texture units, fog,
  the alpha test) and draws with one shader. Quads are drawn as triangles
  through a shared index buffer. WebGL 2 is used when there is one and
  WebGL 1 otherwise; the difference is that WebGL 1 cannot stop at the
  atlas's fifth mip level, so there distant blocks are drawn without mip
  levels and shimmer a little. Anything the game asks for that the
  JavaScript does not do is reported in the console as `NOT IMPLEMENTED`,
  never ignored. The calls for the mods' post-processing shaders
  ([SHADERS.md](SHADERS.md): `glCreateShader`, `glUseProgram`...) hand the
  game's own program to WebGL for the length of one full-screen draw; its
  vertices come from a buffer object, since WebGL cannot read the game's memory.
- **Sockets** (`src/sys/web/net.rs`): always fail.

**The main loop.** A page must return to the browser after each frame. The
loop in `app::run` is therefore a closure: on a desktop it is called once
and loops as before; in the browser each call runs one frame. The page calls
`voxelrust_start()` once, on the click that starts the game (browsers allow
sound and mouse capture only after a click), then `voxelrust_frame()` from
`requestAnimationFrame` until it returns 0.

**Sound.** The game mixes its own samples. After each frame the page asks
`voxelrust_audio()` for enough to stay about a tenth of a second ahead and
queues them with WebAudio.

**Panics.** As everywhere, a panic logs its message and aborts; here abort
is a WebAssembly trap, which the page catches and shows with the last line
logged. The `rust_eh_personality` and `_Unwind_Resume` stubs in
`src/sys/mod.rs` are not needed on this target (a test program links
without them, optimised or not); they are left in and do no harm.

**Imports.** A WebAssembly program does not start unless every function it
imports is supplied. `web/check-imports.py` compares the two lists after
each build, and the page itself names anything missing.

For finding mistakes in the WebGL layer, add `?gldebug` to the address (ask
WebGL for errors after every call; slow) or `?webgl1` (use WebGL 1 even
where WebGL 2 exists). `window.voxelrust.stats` counts frames, draw calls
and vertices.

## What has been tested

Run in Chromium (the browser built into a development tool) on Linux, with
both WebGL 2 and `?webgl1`, with `?gldebug` reporting no errors: the menus,
creating and loading worlds, terrain, fog, water, rain, creatures, the HUD,
the inventory, digging, chat and text fields, 3D DETAIL at 50%, saving and
loading across page reloads, the multiplayer and Earth failures above, and
QUIT.

The current build (world height 256, flat worlds, the built-in mods, spawn
eggs, riding, the scripting commands) was made from the merged code and run
the same way, more briefly: the title screen, the NEW WORLD screen, and a
flat world drawn with its HUD and minimap, with no errors in the console.
The longer list above was checked on earlier builds and has not been gone
through again.

Mouse capture was tested in that same embedded Chromium, which is of the
kind that grants the capture and withdraws it at once. So only the fallback
(looking around by plain mouse movement) could be exercised there. Lasting
capture in an ordinary browser window, and the notice going away when it is
granted, have not been seen working.

Not tested: Firefox and Safari; lasting mouse capture and full screen (they
need a real person's click in an ordinary browser); whether the sound is
audible (the mixer was seen to produce samples); phones and touch screens
(there are no touch controls). The build made after the three mods moved out
of the program has not been gone through again either.
