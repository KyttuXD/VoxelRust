# Shaders

A mod can bring **post-processing shaders**: small programs that run on the
graphics card over the finished 3D picture, before the interface is drawn on
top. They change how the world looks (a golden evening, a glow round lamps, a
wobble under water, the colours of an old console), never what is in it.

The player chooses one in **SETTINGS > VIDEO > DETAIL AND DISTANCE... > SHADER**
(OFF, or a stepper over the shaders the loaded mods bring). The choice is saved
in `settings.txt` as `shader=<id>`. The author's mod of four is `shaders`
(in `VoxelRust-Mods`): soft bloom, warm sunset, underwater ripple and retro dither.

## Adding shaders to a mod

A shader is one line in a mod's `pack.txt` (see also [DATA.md](DATA.md#shaders))
and one to three files:

```
shader <id> <file> [<file2> [<file3>]]
```

* `<id>`: letters, digits, `_` and `-`, up to 32 long. It is what the setting
  stores, and the row shows it in capitals with spaces (`soft_bloom` is SOFT BLOOM).
  A later pack that uses the same id replaces the shader.
* Files are written relative to the pack and end in `.glsl` (a mod that a
  server sends can only use that ending for them).
* Each file is one **pass**. With several files the first runs on the picture,
  the second on what the first made, and so on; at most **3 passes** in a
  shader. Use more than one when a pass needs the finished result of another,
  for example a blur that must be complete before it is combined.
* At most 32 shaders in all loaded packs; a file is at most 16 KB and plain text.

Mistakes in a line (no such file, a bad id, too many files) are written to the
log as a pack warning and that shader is simply not registered.

## Writing a shader

The language is **GLSL ES 1.00**, the dialect of WebGL 1. Desktop OpenGL 2.1
(the legacy Mac) reads the same text, so write only what both understand.
You write the **body** of a fragment shader. The game puts a header in front
of it, so:

* **Do not** write `#version`, `#extension` or a `precision` line (a file that
  starts a line with `#version` or `#extension` is refused), and **do not
  declare** the uniforms below: they are already there.
* The header's lines count as line 0: line numbers in a compile error are your
  file's own.
* Write `void main()` and set `gl_FragColor`. There is no vertex shader to
  write: the game draws one rectangle over the whole picture.
* Rules of GLSL 1.00 that bite: `1` is not `1.0` (no implicit int to float),
  loops need a constant count (`for (int i = 0; i < 8; i++)`), no recursion, no
  `%` (use `mod(x, y)`), no array initialisers. A shader that does too much makes the game slow: it
  runs for every pixel of the window, every frame.

### What the shader has

| Name | Type | Meaning |
| --- | --- | --- |
| `v_uv` | `varying vec2` | Where this pixel is on the picture: `(0, 0)` bottom left, `(1, 1)` top right |
| `scene(uv)` | `vec4 scene(vec2 uv)` | The 3D picture at `uv` (0 to 1; outside is clamped to the edge). **Always read the picture through this**, never with `texture2D`: it hides how the picture sits in its texture |
| `u_time` | `float` | Seconds since the game started, wrapped every hour (so it stays precise) |
| `u_resolution` | `vec2` | Size in pixels of what this pass draws: the window in the last pass (and in a one-pass shader). With a 3D DETAIL below 100% the picture is smaller than this, and `scene()` stretches it |
| `u_sun` | `vec3` | A unit vector towards the sun in **view space**: x to the right, y up, z towards you, so the sun is in front of you when `z` is below 0. The moon is the opposite way (`-u_sun`) |
| `u_daylight` | `float` | 0 at night, 1 by day, in between at dawn and dusk (the same number that dims the world) |
| `u_underwater` | `float` | 1 while the player's eyes are under water, otherwise 0 |
| `u_rain` | `float` | 0 (clear) to 1 (pouring) |
| `u_fov` | `float` | The vertical field of view in degrees (the FIELD OF VIEW setting) |
| `gl_FragCoord` | `vec4` | The pixel's position in the window, as always |

A uniform your shader does not use costs nothing: it need not be mentioned.

The picture has no depth channel: **depth is not available.** (It would need
a depth texture, which neither OpenGL 1.5 nor WebGL 1 gives without extensions,
and the game's renderer works on both.) Effects that need the distance of each
pixel (depth of field, fog by distance, outlines) cannot be made with this.

### Numbers from scripts

A shader may declare uniforms of its own, plain floats named `u_<name>`:

```glsl
uniform float u_glow;
```

A script (see [SCRIPTING.md](SCRIPTING.md)) sets them with
`set_shader_uniform("glow", 0.5)`. The name must be lower-case letters, digits
and `_`, starting with a letter, up to 24 long, and not one the game provides
(`time`, `resolution`, `sun`, `daylight`, `underwater`, `rain`, `fov`, `scene`,
`map`). The value is kept between -1000 and 1000, and at most 16 different names
can be set at once; the call returns `false` when it was refused. A uniform nobody
has set is **0**, so write the shader so that 0 is the usual (`1.0 + u_glow`).
The numbers belong to the running world and are cleared when the player leaves
it; a script that wants one to stay sets it again in `on_load`:

```rust
fn on_load() {
    set_shader_uniform("bloom", 0.5);
}

fn on_tick(dt: f32) {
    // Brighter at night.
    set_shader_uniform("bloom", 1.0 - time());
}
```

### A small shader

```glsl
// Slightly desaturate, more as it rains.
void main() {
  vec3 c = scene(v_uv).rgb;
  float grey = dot(c, vec3(0.3, 0.59, 0.11));
  gl_FragColor = vec4(mix(c, vec3(grey), 0.6 * u_rain), 1.0);
}
```

with `shader grey_rain shaders/grey_rain.glsl` in the pack.

## What happens when something goes wrong

The renderer never shows a black screen because of a shader.

* **The shader does not compile or link** (the driver's message is written to
  the log, with the pass number): that shader is switched off, the SHADER row shows
  `(FAILED)` after its name, and the game keeps drawing normally. It is not tried
  again until the packs are reloaded or another shader is chosen.
* **The chosen shader is not loaded** (its mod is off or deleted): the setting is kept,
  the row shows `(NOT LOADED)`, the picture is normal. It starts working when the mod is back.
* **The graphics driver cannot run shaders** (before OpenGL 2.0): the row says
  NOT SUPPORTED HERE and the setting is ignored.

## How it works

For the curious, and for anyone changing the renderer. The code is in
`src/client/shaders/`.

* The game's drawing is OpenGL 1.5 without framebuffer objects, in a browser
  WebGL 1 through `web/voxelrust.js`, so the chain works through the window's own
  back buffer. The existing 3D DETAIL path already draws the 3D view into the
  bottom-left corner and stretches it back; with a shader chosen that path is
  used even at 100%, and **only then**: with the SHADER set to OFF nothing is
  copied or drawn extra.
* After the 3D view (hand included) the corner is copied into a texture. Every pass
  but the last draws a rectangle over the same corner and copies the result back;
  the last draws over the whole window. Then the interface is drawn as usual.
  `plan.rs` has the arithmetic (and the tests).
* `source.rs` has the vertex shader (one triangle strip) and the header.
* The GL calls are the OpenGL 2.0 ones: `glCreateShader`, `glShaderSource`,
  `glCompileShader`, `glGetShaderiv`, `glGetShaderInfoLog`, `glDeleteShader`,
  `glCreateProgram`, `glAttachShader`, `glBindAttribLocation`, `glLinkProgram`,
  `glGetProgramiv`, `glGetProgramInfoLog`, `glUseProgram`, `glDeleteProgram`,
  `glGetUniformLocation`, `glUniform1i/1f/2f/3f/4f`, `glEnableVertexAttribArray`,
  `glDisableVertexAttribArray` and `glVertexAttribPointer`. They are in
  `src/sys/gl_shader.rs`, looked up apart from the fixed-function ones so a card
  without them still starts the game; `src/sys/web/gl.rs` imports them and
  `web/voxelrust.js` carries them out (a program the game makes takes the place
  of the shader that stands in for fixed function for the duration of a draw).
* Mods' shader files travel with a mod a server sends, like pictures and models.
  A downloaded shader runs only if the player picks it.

## Not done

* Depth (see above), and several texture inputs: a pass sees the picture only.
* A shader cannot add geometry or change the interface.
* The chosen shader is the player's setting; a server or a mod cannot switch one on.
