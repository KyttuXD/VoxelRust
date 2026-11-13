// VoxelRust in a browser.
//
// The game is one WebAssembly file, voxelrust.wasm, built from the same Rust
// source as every other version. It asks this file for everything a browser
// has in place of an operating system (the functions in `env` below):
//
//   - OpenGL 1.5 calls (gl...), carried out with WebGL: a single shader
//     stands in for the fixed-function pipeline;
//   - the window, mouse and keyboard, delivered as SDL events;
//   - files, kept in localStorage;
//   - sound, played through WebAudio;
//   - the clock and the console.
//
// The Rust side of this is src/sys/web/. docs/WEB.md explains the whole.
"use strict";

(function () {
  const canvas = document.getElementById("screen");
  const startBox = document.getElementById("start");
  const startNote = document.getElementById("start-note");
  const errorBox = document.getElementById("error");
  const errorText = document.getElementById("error-text");
  const noticeBox = document.getElementById("notice");
  let noticeTimer = 0;
  const query = new URLSearchParams(location.search);

  let wasm = null; // the game's exports
  let gl = null;
  let running = false;
  let lastLog = "";

  // ------------------------------------------------------------- memory

  // The game's memory. It is replaced by a new buffer whenever it grows, so
  // views are made afresh when that has happened.
  let memU8 = new Uint8Array(0);
  function bytes() {
    if (memU8.buffer !== wasm.memory.buffer) memU8 = new Uint8Array(wasm.memory.buffer);
    return memU8;
  }
  function view() {
    return new DataView(wasm.memory.buffer);
  }
  const decoder = new TextDecoder();
  function text(ptr, len) {
    return decoder.decode(bytes().subarray(ptr, ptr + len));
  }

  // ------------------------------------------------------------- errors

  function showError(message) {
    errorText.textContent = message;
    errorBox.hidden = false;
    startBox.hidden = true;
  }

  // Something the game asked for that this file does not do. Never silent:
  // each kind is reported once, in the console.
  const reported = new Set();
  function unimplemented(what) {
    if (!reported.has(what)) {
      reported.add(what);
      console.error("voxelrust.js: NOT IMPLEMENTED: " + what);
    }
  }

  // ------------------------------------------------------------- OpenGL

  // The OpenGL constants the game uses (src/sys/gl.rs). Where WebGL has the
  // same thing it has the same number, so most pass straight through.
  const GL_POINTS = 0, GL_LINES = 1, GL_QUADS = 7;
  const GL_DEPTH_TEST = 0x0B71, GL_CULL_FACE = 0x0B44, GL_BLEND = 0x0BE2;
  const GL_ALPHA_TEST = 0x0BC0, GL_FOG = 0x0B60, GL_TEXTURE_2D = 0x0DE1;
  const GL_MODELVIEW = 0x1700, GL_PROJECTION = 0x1701, GL_TEXTURE = 0x1702;
  const GL_RGB = 0x1907, GL_RGBA = 0x1908;
  const GL_UNSIGNED_BYTE = 0x1401, GL_SHORT = 0x1402, GL_FLOAT = 0x1406;
  const GL_TEXTURE_MAG_FILTER = 0x2800, GL_TEXTURE_MIN_FILTER = 0x2801;
  const GL_TEXTURE_WRAP_S = 0x2802, GL_TEXTURE_WRAP_T = 0x2803, GL_TEXTURE_MAX_LEVEL = 0x813D;
  const GL_NEAREST = 0x2600, GL_LINEAR = 0x2601;
  const GL_VERTEX_ARRAY = 0x8074, GL_COLOR_ARRAY = 0x8076, GL_TEXTURE_COORD_ARRAY = 0x8078;
  const GL_ARRAY_BUFFER = 0x8892, GL_TEXTURE0 = 0x84C0;
  const GL_FOG_MODE = 0x0B65, GL_FOG_START = 0x0B63, GL_FOG_END = 0x0B64, GL_FOG_COLOR = 0x0B66;
  const GL_GREATER = 0x0204;
  const GL_VENDOR = 0x1F00, GL_RENDERER = 0x1F01, GL_VERSION = 0x1F02;
  const GL_PACK_ALIGNMENT = 0x0D05, GL_UNPACK_ALIGNMENT = 0x0CF5;
  const GL_PERSPECTIVE_CORRECTION_HINT = 0x0C50, GL_FOG_HINT = 0x0C54;

  const VERTEX_SHADER = `
    attribute vec4 aPosition;
    attribute vec4 aColor;
    attribute vec4 aTexCoord0;
    attribute vec4 aTexCoord1;
    uniform mat4 uModelView;
    uniform mat4 uProjection;
    uniform mat4 uTexture0;
    uniform mat4 uTexture1;
    uniform float uPointSize;
    varying vec4 vColor;
    varying vec2 vTexCoord0;
    varying vec2 vTexCoord1;
    varying float vFogDistance;
    void main() {
      vec4 eye = uModelView * aPosition;
      gl_Position = uProjection * eye;
      gl_PointSize = uPointSize;
      vColor = aColor;
      vTexCoord0 = (uTexture0 * aTexCoord0).xy;
      vTexCoord1 = (uTexture1 * aTexCoord1).xy;
      vFogDistance = abs(eye.z);
    }`;

  // Colour times the texture of each unit (a unit that is switched off has
  // a white texture bound, so it changes nothing), then linear fog, then the
  // alpha test.
  const FRAGMENT_SHADER = `
    #ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    #else
    precision mediump float;
    #endif
    uniform sampler2D uSampler0;
    uniform sampler2D uSampler1;
    uniform vec3 uFogColor;
    uniform vec3 uFog;        // end, 1 / (end - start), 1 when fog is off
    uniform float uAlphaRef;  // below 0 when the alpha test is off
    varying vec4 vColor;
    varying vec2 vTexCoord0;
    varying vec2 vTexCoord1;
    varying float vFogDistance;
    void main() {
      vec4 c = vColor * texture2D(uSampler0, vTexCoord0) * texture2D(uSampler1, vTexCoord1);
      if (c.a <= uAlphaRef) discard;
      float clear = max(clamp((uFog.x - vFogDistance) * uFog.y, 0.0, 1.0), uFog.z);
      gl_FragColor = vec4(mix(uFogColor, c.rgb, clear), c.a);
    }`;

  const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  const ATTRIBUTES = ["aPosition", "aColor", "aTexCoord0", "aTexCoord1"];
  // What an attribute is when its array is switched off.
  const CONSTANT = [[0, 0, 0, 1], [1, 1, 1, 1], [0, 0, 0, 1], [0, 0, 0, 1]];

  function newArray() {
    return { enabled: false, size: 0, type: 0, stride: 0, pointer: 0, buffer: 0 };
  }

  // Everything OpenGL remembers that WebGL does not.
  const state = {
    matrixMode: GL_MODELVIEW,
    modelView: new Float32Array(IDENTITY),
    projection: new Float32Array(IDENTITY),
    texture: [new Float32Array(IDENTITY), new Float32Array(IDENTITY)],
    activeTexture: 0,
    clientActiveTexture: 0,
    textureOn: [false, false],
    textureBound: [0, 0], // the game's texture numbers
    alphaTest: false,
    alphaRef: 0,
    fog: false,
    fogStart: 0,
    fogEnd: 1,
    fogColor: new Float32Array([0, 0, 0, 0]),
    pointSize: 1,
    arrayBuffer: 0,
    arrays: [newArray(), newArray(), newArray(), newArray()], // as ATTRIBUTES
  };

  const textures = [null]; // number -> { object, wantMin }
  const buffers = [null]; // number -> WebGLBuffer
  let webgl2 = false;
  let uniforms = {};
  let whiteTexture = null;
  let unitHolds = [null, null]; // the WebGLTexture each unit really has bound
  let scratchBuffer = null; // for vertices drawn straight from memory
  let quadIndices = null;
  let quadCapacity = 0;
  let indexType = 0;
  let glDebug = false;

  function compile(kind, source) {
    const shader = gl.createShader(kind);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error("shader: " + gl.getShaderInfoLog(shader));
    }
    return shader;
  }

  function setUpGl() {
    const options = { alpha: false, depth: true, stencil: false, antialias: false, preserveDrawingBuffer: true };
    if (!query.has("webgl1")) gl = canvas.getContext("webgl2", options);
    webgl2 = !!gl;
    if (!gl) gl = canvas.getContext("webgl", options) || canvas.getContext("experimental-webgl", options);
    if (!gl) return false;
    glDebug = query.has("gldebug");

    const program = gl.createProgram();
    gl.attachShader(program, compile(gl.VERTEX_SHADER, VERTEX_SHADER));
    gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAGMENT_SHADER));
    ATTRIBUTES.forEach((name, i) => gl.bindAttribLocation(program, i, name));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error("shader program: " + gl.getProgramInfoLog(program));
    }
    gl.useProgram(program);
    for (const name of ["uModelView", "uProjection", "uTexture0", "uTexture1", "uPointSize", "uSampler0", "uSampler1", "uFogColor", "uFog", "uAlphaRef"]) {
      uniforms[name] = gl.getUniformLocation(program, name);
    }
    gl.uniform1i(uniforms.uSampler0, 0);
    gl.uniform1i(uniforms.uSampler1, 1);

    whiteTexture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, whiteTexture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, whiteTexture);
    gl.activeTexture(gl.TEXTURE0);
    unitHolds = [whiteTexture, whiteTexture];

    scratchBuffer = gl.createBuffer();
    quadIndices = gl.createBuffer();
    // A chunk can have more than 65536 vertices; 32-bit indices reach them
    // in one draw call. (Without them the draw is done in parts.)
    indexType = webgl2 || gl.getExtension("OES_element_index_uint") ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT;
    return true;
  }

  // Bind `object` (or the white texture) to a unit, if it is not already.
  function hold(unit, object) {
    object = object || whiteTexture;
    if (unitHolds[unit] !== object) {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, object);
      unitHolds[unit] = object;
    }
  }

  // The texture the game has bound on the active unit, bound for real, for
  // calls that change a texture. Null when the game has bound none.
  function boundTexture(call) {
    const t = textures[state.textureBound[state.activeTexture]];
    if (!t) {
      unimplemented(call + " with no texture bound");
      return null;
    }
    hold(state.activeTexture, t.object);
    gl.activeTexture(gl.TEXTURE0 + state.activeTexture);
    return t;
  }

  function currentMatrix() {
    switch (state.matrixMode) {
      case GL_MODELVIEW: return state.modelView;
      case GL_PROJECTION: return state.projection;
      default: return state.texture[state.activeTexture];
    }
  }

  function pixelBytes(format) {
    return format === GL_RGBA ? 4 : 3;
  }

  function checkPixels(call, target, format, type) {
    if (target !== GL_TEXTURE_2D || (format !== GL_RGBA && format !== GL_RGB) || type !== GL_UNSIGNED_BYTE) {
      unimplemented(call + " target 0x" + target.toString(16) + " format 0x" + format.toString(16) + " type 0x" + type.toString(16));
      return false;
    }
    return true;
  }

  // Bytes per component, and whether integers mean 0..1 (colours do).
  function typeSize(type) {
    return type === GL_FLOAT ? 4 : type === GL_SHORT ? 2 : 1;
  }

  function setPointer(index, call, size, type, stride, pointer) {
    if (type !== GL_FLOAT && type !== GL_SHORT && type !== GL_UNSIGNED_BYTE) {
      unimplemented(call + " type 0x" + type.toString(16));
      return;
    }
    const a = state.arrays[index];
    a.size = size;
    a.type = type;
    a.stride = stride || size * typeSize(type);
    a.pointer = pointer;
    a.buffer = state.arrayBuffer;
  }

  function clientArray(call, array) {
    if (array === GL_VERTEX_ARRAY) return state.arrays[0];
    if (array === GL_COLOR_ARRAY) return state.arrays[1];
    if (array === GL_TEXTURE_COORD_ARRAY) return state.arrays[2 + state.clientActiveTexture];
    unimplemented(call + " 0x" + array.toString(16));
    return null;
  }

  // Send the state the shader needs. Called before every draw.
  function applyState() {
    gl.uniformMatrix4fv(uniforms.uModelView, false, state.modelView);
    gl.uniformMatrix4fv(uniforms.uProjection, false, state.projection);
    gl.uniformMatrix4fv(uniforms.uTexture0, false, state.texture[0]);
    gl.uniformMatrix4fv(uniforms.uTexture1, false, state.texture[1]);
    gl.uniform1f(uniforms.uPointSize, state.pointSize);
    gl.uniform1f(uniforms.uAlphaRef, state.alphaTest ? state.alphaRef : -1);
    const span = state.fogEnd - state.fogStart;
    gl.uniform3f(uniforms.uFog, state.fogEnd, span !== 0 ? 1 / span : 0, state.fog ? 0 : 1);
    gl.uniform3f(uniforms.uFogColor, state.fogColor[0], state.fogColor[1], state.fogColor[2]);
    for (let unit = 0; unit < 2; unit++) {
      const t = state.textureOn[unit] ? textures[state.textureBound[unit]] : null;
      hold(unit, t ? t.object : null);
    }
  }

  // Vertices that are not in a buffer object are read from the game's
  // memory: copy the part being drawn into a buffer of our own. Returns the
  // address the copy starts at.
  function uploadClientArrays(first, count) {
    let low = Infinity, high = 0;
    for (const a of state.arrays) {
      if (a.enabled && a.size && a.buffer === 0) {
        low = Math.min(low, a.pointer);
        high = Math.max(high, a.pointer + (first + count - 1) * a.stride + a.size * typeSize(a.type));
      }
    }
    if (low === Infinity) return 0;
    gl.bindBuffer(gl.ARRAY_BUFFER, scratchBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, bytes().subarray(low, high), gl.DYNAMIC_DRAW);
    return low;
  }

  // Point the shader's attributes at the arrays, starting `skip` vertices in.
  function pointAttributes(clientBase, skip) {
    for (let i = 0; i < 4; i++) {
      const a = state.arrays[i];
      if (!a.enabled || !a.size) {
        gl.disableVertexAttribArray(i);
        const c = CONSTANT[i];
        gl.vertexAttrib4f(i, c[0], c[1], c[2], c[3]);
        continue;
      }
      let offset = a.pointer + skip * a.stride;
      if (a.buffer === 0) {
        gl.bindBuffer(gl.ARRAY_BUFFER, scratchBuffer);
        offset -= clientBase;
      } else {
        gl.bindBuffer(gl.ARRAY_BUFFER, buffers[a.buffer]);
      }
      gl.enableVertexAttribArray(i);
      // Colours given as bytes mean 0..1; positions and texture
      // coordinates given as integers mean their own value.
      gl.vertexAttribPointer(i, a.size, a.type, i === 1 && a.type !== GL_FLOAT, a.stride, offset);
    }
  }

  // Indices that draw quads 0-1-2-3, 4-5-6-7... as two triangles each.
  function needQuadIndices(quads) {
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, quadIndices);
    if (quads <= quadCapacity) return;
    quadCapacity = 4096;
    while (quadCapacity < quads) quadCapacity *= 2;
    const data = indexType === gl.UNSIGNED_INT ? new Uint32Array(quadCapacity * 6) : new Uint16Array(quadCapacity * 6);
    for (let q = 0; q < quadCapacity; q++) {
      const v = q * 4, i = q * 6;
      data[i] = v;
      data[i + 1] = v + 1;
      data[i + 2] = v + 2;
      data[i + 3] = v;
      data[i + 4] = v + 2;
      data[i + 5] = v + 3;
    }
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, data, gl.STATIC_DRAW);
  }

  // Counted for tests and for anyone curious (window.voxelrust.stats).
  const stats = { frames: 0, draws: 0, vertices: 0, lastFrameDraws: 0, lastFrameVertices: 0 };

  function drawArrays(mode, first, count) {
    if (count <= 0) return;
    stats.draws++;
    stats.vertices += count;
    applyState();
    const clientBase = uploadClientArrays(first, count);
    if (mode === GL_QUADS) {
      const limit = indexType === gl.UNSIGNED_INT ? Infinity : 65536;
      for (let done = 0; done < count; done += limit) {
        const quads = Math.min(count - done, limit) >> 2;
        pointAttributes(clientBase, first + done);
        needQuadIndices(quads);
        gl.drawElements(gl.TRIANGLES, quads * 6, indexType, 0);
      }
    } else if (mode === GL_LINES || mode === GL_POINTS) {
      pointAttributes(clientBase, 0);
      gl.drawArrays(mode === GL_LINES ? gl.LINES : gl.POINTS, first, count);
    } else {
      unimplemented("glDrawArrays mode " + mode);
    }
  }

  const glFunctions = {
    glClear(mask) { gl.clear(mask); },
    glClearColor(r, g, b, a) { gl.clearColor(r, g, b, a); },
    glEnable(cap) { setCapability(cap, true); },
    glDisable(cap) { setCapability(cap, false); },
    glViewport(x, y, w, h) { gl.viewport(x, y, w, h); },
    glMatrixMode(mode) {
      if (mode !== GL_MODELVIEW && mode !== GL_PROJECTION && mode !== GL_TEXTURE) {
        unimplemented("glMatrixMode 0x" + mode.toString(16));
        return;
      }
      state.matrixMode = mode;
    },
    glLoadMatrixf(ptr) { currentMatrix().set(new Float32Array(wasm.memory.buffer, ptr, 16)); },
    glLoadIdentity() { currentMatrix().set(IDENTITY); },
    glGenTextures(n, ptr) {
      const out = view();
      for (let i = 0; i < n; i++) {
        textures.push({ object: gl.createTexture(), wantMin: 0 });
        out.setUint32(ptr + i * 4, textures.length - 1, true);
      }
    },
    glBindTexture(target, id) {
      if (target !== GL_TEXTURE_2D) {
        unimplemented("glBindTexture target 0x" + target.toString(16));
        return;
      }
      state.textureBound[state.activeTexture] = id;
    },
    glTexImage2D(target, level, internal, w, h, border, format, type, ptr) {
      if (!checkPixels("glTexImage2D", target, format, type)) return;
      if (internal !== format || border !== 0) {
        unimplemented("glTexImage2D internal format 0x" + internal.toString(16) + " for 0x" + format.toString(16) + ", border " + border);
        return;
      }
      if (!boundTexture("glTexImage2D")) return;
      const px = ptr ? bytes().subarray(ptr, ptr + w * h * pixelBytes(format)) : null;
      gl.texImage2D(gl.TEXTURE_2D, level, format, w, h, 0, format, gl.UNSIGNED_BYTE, px);
    },
    glTexParameteri(target, name, value) {
      const t = target === GL_TEXTURE_2D ? boundTexture("glTexParameteri") : null;
      if (!t) return;
      switch (name) {
        case GL_TEXTURE_MIN_FILTER:
          t.wantMin = value;
          if (t.noMipmaps && value !== GL_NEAREST && value !== GL_LINEAR) value = plainFilter(value);
          gl.texParameteri(gl.TEXTURE_2D, name, value);
          break;
        case GL_TEXTURE_MAG_FILTER:
        case GL_TEXTURE_WRAP_S:
        case GL_TEXTURE_WRAP_T:
          gl.texParameteri(gl.TEXTURE_2D, name, value);
          break;
        case GL_TEXTURE_MAX_LEVEL:
          if (webgl2) {
            gl.texParameteri(gl.TEXTURE_2D, name, value);
          } else {
            // WebGL 1 wants every level down to one pixel or none. The game
            // supplies only the first few (so that tiles of the atlas never
            // blend into each other), so there the texture goes without.
            t.noMipmaps = true;
            if (t.wantMin) gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, plainFilter(t.wantMin));
          }
          break;
        default:
          unimplemented("glTexParameteri 0x" + name.toString(16));
      }
    },
    glTexSubImage2D(target, level, x, y, w, h, format, type, ptr) {
      if (!checkPixels("glTexSubImage2D", target, format, type) || !boundTexture("glTexSubImage2D")) return;
      gl.texSubImage2D(gl.TEXTURE_2D, level, x, y, w, h, format, gl.UNSIGNED_BYTE, bytes().subarray(ptr, ptr + w * h * pixelBytes(format)));
    },
    glCopyTexSubImage2D(target, level, x, y, sx, sy, w, h) {
      if (target !== GL_TEXTURE_2D || !boundTexture("glCopyTexSubImage2D")) return;
      gl.copyTexSubImage2D(gl.TEXTURE_2D, level, x, y, sx, sy, w, h);
    },
    glEnableClientState(array) {
      const a = clientArray("glEnableClientState", array);
      if (a) a.enabled = true;
    },
    glDisableClientState(array) {
      const a = clientArray("glDisableClientState", array);
      if (a) a.enabled = false;
    },
    glVertexPointer(size, type, stride, ptr) { setPointer(0, "glVertexPointer", size, type, stride, ptr); },
    glColorPointer(size, type, stride, ptr) { setPointer(1, "glColorPointer", size, type, stride, ptr); },
    glTexCoordPointer(size, type, stride, ptr) { setPointer(2 + state.clientActiveTexture, "glTexCoordPointer", size, type, stride, ptr); },
    glDrawArrays(mode, first, count) { drawArrays(mode, first, count); },
    glFogf(name, value) {
      if (name === GL_FOG_START) state.fogStart = value;
      else if (name === GL_FOG_END) state.fogEnd = value;
      else unimplemented("glFogf 0x" + name.toString(16));
    },
    glFogfv(name, ptr) {
      if (name === GL_FOG_COLOR) state.fogColor.set(new Float32Array(wasm.memory.buffer, ptr, 4));
      else unimplemented("glFogfv 0x" + name.toString(16));
    },
    glFogi(name, value) {
      // Linear fog is the only kind the shader does.
      if (name !== GL_FOG_MODE || value !== GL_LINEAR) unimplemented("glFogi 0x" + name.toString(16) + " = 0x" + value.toString(16));
    },
    glBlendFunc(src, dst) { gl.blendFunc(src, dst); },
    glDepthFunc(func) { gl.depthFunc(func); },
    glDepthMask(on) { gl.depthMask(on !== 0); },
    glDepthRange(near, far) { gl.depthRange(near, far); },
    glAlphaFunc(func, ref) {
      if (func !== GL_GREATER) unimplemented("glAlphaFunc 0x" + func.toString(16));
      state.alphaRef = ref;
    },
    glCullFace(face) { gl.cullFace(face); },
    glReadPixels(x, y, w, h, format, type, ptr) {
      if ((format !== GL_RGB && format !== GL_RGBA) || type !== GL_UNSIGNED_BYTE) {
        unimplemented("glReadPixels format 0x" + format.toString(16) + " type 0x" + type.toString(16));
        return;
      }
      // WebGL only promises RGBA; drop the alpha for RGB.
      const px = new Uint8Array(w * h * 4);
      gl.readPixels(x, y, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
      const out = bytes();
      if (format === GL_RGBA) {
        out.set(px, ptr);
      } else {
        for (let i = 0, o = ptr; i < px.length; i += 4, o += 3) {
          out[o] = px[i];
          out[o + 1] = px[i + 1];
          out[o + 2] = px[i + 2];
        }
      }
    },
    glPixelStorei(name, value) {
      if (name === GL_PACK_ALIGNMENT || name === GL_UNPACK_ALIGNMENT) gl.pixelStorei(name, value);
      else unimplemented("glPixelStorei 0x" + name.toString(16));
    },
    glLineWidth(width) { gl.lineWidth(width); },
    glPointSize(size) { state.pointSize = size; },
    glHint(target, mode) {
      // Hints ask for speed over quality; the shader has only one way.
      if (target !== GL_FOG_HINT && target !== GL_PERSPECTIVE_CORRECTION_HINT) unimplemented("glHint 0x" + target.toString(16));
    },
    glFinish() { gl.finish(); },
    glGenBuffers(n, ptr) {
      const out = view();
      for (let i = 0; i < n; i++) {
        // Numbers of deleted buffers are used again, so the table stays small.
        let id = buffers.indexOf(undefined, 1);
        if (id < 0) id = buffers.length;
        buffers[id] = gl.createBuffer();
        out.setUint32(ptr + i * 4, id, true);
      }
    },
    glBindBuffer(target, id) {
      if (target !== GL_ARRAY_BUFFER) {
        unimplemented("glBindBuffer target 0x" + target.toString(16));
        return;
      }
      state.arrayBuffer = id;
    },
    glBufferData(target, size, ptr, usage) {
      if (target !== GL_ARRAY_BUFFER || !buffers[state.arrayBuffer]) {
        unimplemented("glBufferData target 0x" + target.toString(16) + " buffer " + state.arrayBuffer);
        return;
      }
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[state.arrayBuffer]);
      gl.bufferData(gl.ARRAY_BUFFER, bytes().subarray(ptr, ptr + size), gl.STATIC_DRAW);
    },
    glDeleteBuffers(n, ptr) {
      const ids = view();
      for (let i = 0; i < n; i++) {
        const id = ids.getUint32(ptr + i * 4, true);
        if (id && buffers[id]) {
          gl.deleteBuffer(buffers[id]);
          buffers[id] = undefined;
        }
      }
    },
    glActiveTexture(unit) { state.activeTexture = textureUnit("glActiveTexture", unit); },
    glClientActiveTexture(unit) { state.clientActiveTexture = textureUnit("glClientActiveTexture", unit); },
    js_gl_string(what, ptr, cap) {
      const name = what === GL_VENDOR ? gl.VENDOR : what === GL_RENDERER ? gl.RENDERER : what === GL_VERSION ? gl.VERSION : 0;
      const encoded = new TextEncoder().encode(name ? String(gl.getParameter(name)) : "").subarray(0, cap);
      bytes().set(encoded, ptr);
      return encoded.length;
    },
  };

  function plainFilter(filter) {
    // NEAREST_MIPMAP_* -> NEAREST, LINEAR_MIPMAP_* -> LINEAR.
    return filter === 0x2700 || filter === 0x2702 ? GL_NEAREST : GL_LINEAR;
  }

  function textureUnit(call, unit) {
    const n = unit - GL_TEXTURE0;
    if (n !== 0 && n !== 1) {
      unimplemented(call + " unit " + n);
      return 0;
    }
    return n;
  }

  function setCapability(cap, on) {
    switch (cap) {
      case GL_DEPTH_TEST:
      case GL_CULL_FACE:
      case GL_BLEND:
        if (on) gl.enable(cap);
        else gl.disable(cap);
        break;
      case GL_ALPHA_TEST: state.alphaTest = on; break;
      case GL_FOG: state.fog = on; break;
      case GL_TEXTURE_2D: state.textureOn[state.activeTexture] = on; break;
      default: unimplemented("glEnable/glDisable 0x" + cap.toString(16));
    }
  }

  // With ?gldebug in the address, ask WebGL after every call whether it
  // objected, and say which call it was. Slow; for finding mistakes.
  function checked(name, f) {
    return function (...args) {
      const result = f(...args);
      const error = gl.getError();
      if (error) console.error("voxelrust.js: WebGL error 0x" + error.toString(16) + " after " + name + "(" + args.join(", ") + ")");
      return result;
    };
  }

  // ------------------------------------------------------------- window

  // SDL events, as plain objects, waiting for the game to ask for them.
  const events = [];
  const EV_QUIT = 0x100, EV_WINDOW = 0x200, EV_KEYDOWN = 0x300, EV_KEYUP = 0x301, EV_TEXTINPUT = 0x303;
  const EV_MOUSEMOTION = 0x400, EV_MOUSEDOWN = 0x401, EV_MOUSEUP = 0x402, EV_MOUSEWHEEL = 0x403;
  const WINEV_SIZE_CHANGED = 6, WINEV_FOCUS_LOST = 13;
  const SCANCODE_ESCAPE = 41;

  // KeyboardEvent.code -> SDL scancode. Both name the key by where it is
  // on the keyboard, not by the letter printed on it.
  const SCANCODES = {
    Enter: 40, Escape: 41, Backspace: 42, Tab: 43, Space: 44, Minus: 45, Equal: 46, BracketLeft: 47,
    BracketRight: 48, Backslash: 49, Semicolon: 51, Quote: 52, Backquote: 53, Comma: 54, Period: 55,
    Slash: 56, CapsLock: 57, PrintScreen: 70, ScrollLock: 71, Pause: 72, Insert: 73, Home: 74,
    PageUp: 75, Delete: 76, End: 77, PageDown: 78, ArrowRight: 79, ArrowLeft: 80, ArrowDown: 81,
    ArrowUp: 82, NumLock: 83, NumpadDivide: 84, NumpadMultiply: 85, NumpadSubtract: 86, NumpadAdd: 87,
    NumpadEnter: 88, Numpad0: 98, NumpadDecimal: 99, IntlBackslash: 100, ContextMenu: 101,
    ControlLeft: 224, ShiftLeft: 225, AltLeft: 226, MetaLeft: 227,
    ControlRight: 228, ShiftRight: 229, AltRight: 230, MetaRight: 231,
  };
  for (let i = 0; i < 26; i++) SCANCODES["Key" + String.fromCharCode(65 + i)] = 4 + i;
  for (let i = 1; i <= 9; i++) {
    SCANCODES["Digit" + i] = 29 + i;
    SCANCODES["Numpad" + i] = 88 + i;
  }
  SCANCODES.Digit0 = 39;
  for (let i = 1; i <= 12; i++) SCANCODES["F" + i] = 57 + i;

  let mouseX = 0, mouseY = 0;
  let wantPointerLock = false;
  let unlockedAt = -1e9; // when the browser last took the mouse back
  let fullscreenRequest = null; // true / false: asked for by the game this frame
  let windowOpen = false;

  function pixelRatio() {
    return window.devicePixelRatio || 1;
  }

  // Keep the canvas as many pixels as it covers on the screen. Returns
  // true if that changed its size.
  function fitCanvas() {
    const w = Math.max(1, Math.round(canvas.clientWidth * pixelRatio()));
    const h = Math.max(1, Math.round(canvas.clientHeight * pixelRatio()));
    if (canvas.width === w && canvas.height === h) return false;
    canvas.width = w;
    canvas.height = h;
    return true;
  }

  function pointerLocked() {
    return document.pointerLockElement === canvas;
  }

  function lockPointer() {
    if (!wantPointerLock || pointerLocked() || !canvas.requestPointerLock) return;
    try {
      const promise = canvas.requestPointerLock();
      if (promise && promise.catch) promise.catch(() => {});
    } catch (e) {
      // Refused (no click yet, or too soon after Esc): the next click asks again.
    }
  }

  function mousePosition(e) {
    if (pointerLocked()) return;
    const box = canvas.getBoundingClientRect();
    mouseX = Math.round((e.clientX - box.left) * pixelRatio());
    mouseY = Math.round((e.clientY - box.top) * pixelRatio());
  }

  // Keys the browser keeps for itself even while the game runs.
  function browserKey(e) {
    if (e.code === "F12") return true;
    if ((e.ctrlKey || e.metaKey) && (e.code === "KeyR" || e.code === "Minus" || e.code === "Equal" || e.code === "Digit0")) return true;
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.code === "KeyI" || e.code === "KeyJ" || e.code === "KeyC")) return true;
    return false;
  }

  function listen() {
    window.addEventListener("keydown", (e) => {
      if (!running || browserKey(e)) return;
      const code = SCANCODES[e.code];
      if (code === undefined) return;
      e.preventDefault();
      wakeAudio();
      // Esc while the mouse is captured is the browser's: it frees the
      // mouse, and the game is told its window lost the keyboard (below),
      // which opens the pause menu. Do not also pass the key on, or the
      // menu would close again.
      if (code === SCANCODE_ESCAPE && performance.now() - unlockedAt < 300) return;
      events.push({ type: EV_KEYDOWN, code, repeat: e.repeat });
      // What SDL sends as a text event: the character the key types.
      if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && e.key.charCodeAt(0) < 128) {
        events.push({ type: EV_TEXTINPUT, text: e.key });
      }
    });
    window.addEventListener("keyup", (e) => {
      if (!running) return;
      const code = SCANCODES[e.code];
      if (code === undefined) return;
      e.preventDefault();
      events.push({ type: EV_KEYUP, code });
    });
    canvas.addEventListener("mousemove", (e) => {
      mousePosition(e);
      events.push({ type: EV_MOUSEMOTION, x: mouseX, y: mouseY, dx: e.movementX || 0, dy: e.movementY || 0 });
    });
    canvas.addEventListener("mousedown", (e) => {
      e.preventDefault();
      wakeAudio();
      mousePosition(e);
      lockPointer();
      if (e.button <= 2) events.push({ type: EV_MOUSEDOWN, button: e.button + 1, x: mouseX, y: mouseY });
    });
    window.addEventListener("mouseup", (e) => {
      if (e.button <= 2) events.push({ type: EV_MOUSEUP, button: e.button + 1, x: mouseX, y: mouseY });
    });
    canvas.addEventListener("wheel", (e) => {
      e.preventDefault();
      if (e.deltaY !== 0) events.push({ type: EV_MOUSEWHEEL, y: e.deltaY < 0 ? 1 : -1 });
    }, { passive: false });
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());

    document.addEventListener("pointerlockchange", () => {
      if (!pointerLocked() && wantPointerLock) {
        // Taken back by the browser (Esc, or another window came forward).
        unlockedAt = performance.now();
        events.push({ type: EV_WINDOW, event: WINEV_FOCUS_LOST });
      }
    });
    document.addEventListener("pointerlockerror", () => {
      // Browsers refuse for a moment after Esc; try once more shortly.
      setTimeout(lockPointer, 1200);
    });
    window.addEventListener("blur", () => {
      if (running) events.push({ type: EV_WINDOW, event: WINEV_FOCUS_LOST });
    });
    document.addEventListener("fullscreenchange", () => {
      // In full screen, ask (where the browser can) for every key, so that
      // Ctrl+W sprints forward instead of closing the tab.
      if (navigator.keyboard && navigator.keyboard.lock) {
        if (document.fullscreenElement) navigator.keyboard.lock().catch(() => {});
        else navigator.keyboard.unlock();
      }
    });
    // Leaving the page: let the game end properly, which saves the world
    // and the settings.
    window.addEventListener("pagehide", () => {
      if (running) {
        events.push({ type: EV_QUIT });
        runFrame();
      }
    });
    // While the mouse is captured the player is in the middle of the game:
    // ask before a stray Ctrl+W closes it.
    window.addEventListener("beforeunload", (e) => {
      if (running && wantPointerLock) {
        e.preventDefault();
        e.returnValue = "";
      }
    });
  }

  function applyFullscreen() {
    if (fullscreenRequest === null) return;
    const want = fullscreenRequest;
    fullscreenRequest = null;
    const root = document.documentElement;
    try {
      if (want && !document.fullscreenElement && root.requestFullscreen) {
        const promise = root.requestFullscreen();
        if (promise && promise.catch) promise.catch(() => {});
      } else if (!want && document.fullscreenElement) {
        document.exitFullscreen();
      }
    } catch (e) {
      // Not allowed right now; the window stays as it is.
    }
  }

  const windowFunctions = {
    js_window_open(fullscreen) {
      try {
        if (!setUpGl()) return 0;
      } catch (e) {
        console.error(e);
        return 0;
      }
      fitCanvas();
      listen();
      windowOpen = true;
      if (fullscreen) fullscreenRequest = true;
      return 1;
    },
    js_window_width() { return canvas.width; },
    js_window_height() { return canvas.height; },
    js_fullscreen(on) { fullscreenRequest = on !== 0; },
    js_pointer_lock(on) {
      wantPointerLock = on !== 0;
      if (wantPointerLock) lockPointer();
      else if (pointerLocked()) document.exitPointerLock();
    },
    // Lay the next event out as SDL does (see handle_events in app.rs for
    // the fields the game reads).
    js_poll_event(ptr) {
      const e = events.shift();
      if (!e) return 0;
      bytes().fill(0, ptr, ptr + 56);
      const out = view();
      out.setUint32(ptr, e.type, true);
      switch (e.type) {
        case EV_WINDOW:
          out.setUint8(ptr + 12, e.event);
          out.setInt32(ptr + 16, e.w || 0, true);
          out.setInt32(ptr + 20, e.h || 0, true);
          break;
        case EV_KEYDOWN:
        case EV_KEYUP:
          out.setUint8(ptr + 12, e.type === EV_KEYDOWN ? 1 : 0);
          out.setUint8(ptr + 13, e.repeat ? 1 : 0);
          out.setInt32(ptr + 16, e.code, true);
          break;
        case EV_TEXTINPUT:
          out.setUint8(ptr + 12, e.text.charCodeAt(0));
          break;
        case EV_MOUSEMOTION:
          out.setInt32(ptr + 20, e.x, true);
          out.setInt32(ptr + 24, e.y, true);
          out.setInt32(ptr + 28, e.dx, true);
          out.setInt32(ptr + 32, e.dy, true);
          break;
        case EV_MOUSEDOWN:
        case EV_MOUSEUP:
          out.setUint8(ptr + 16, e.button);
          out.setUint8(ptr + 17, e.type === EV_MOUSEDOWN ? 1 : 0);
          out.setInt32(ptr + 20, e.x, true);
          out.setInt32(ptr + 24, e.y, true);
          break;
        case EV_MOUSEWHEEL:
          out.setInt32(ptr + 20, e.y, true);
          break;
      }
      return 1;
    },
    js_message(ptr, len) { showError(text(ptr, len)); },
    js_notice(ptr, len) {
      noticeBox.textContent = text(ptr, len);
      noticeBox.hidden = false;
      clearTimeout(noticeTimer);
      noticeTimer = setTimeout(() => { noticeBox.hidden = true; }, 15000);
    },
    js_log(ptr, len) {
      lastLog = text(ptr, len);
      console.log(lastLog);
    },
    js_now() { return performance.now(); },
  };

  // -------------------------------------------------------------- files

  // Files are kept in localStorage, one entry each: the key is the file's
  // path after this prefix, the value its bytes in base64. Browsers allow
  // about 5 MB per site in all; a write that does not fit fails, and the
  // game reports it as it would a full disk.
  const FILE_PREFIX = "voxelrust:";
  let storage = null;
  try {
    storage = window.localStorage;
    storage.getItem(FILE_PREFIX);
  } catch (e) {
    storage = null;
  }
  if (!storage) {
    // Storage switched off (some private windows): keep files in memory so
    // the game can be played, though nothing outlives the page.
    console.warn("voxelrust.js: no localStorage; worlds and settings will not be kept");
    const kept = new Map();
    storage = {
      getItem: (k) => (kept.has(k) ? kept.get(k) : null),
      setItem: (k, v) => void kept.set(k, v),
    };
  }

  function toBase64(data) {
    let s = "";
    for (let i = 0; i < data.length; i += 0x8000) s += String.fromCharCode.apply(null, data.subarray(i, i + 0x8000));
    return btoa(s);
  }

  function fromBase64(value) {
    const s = atob(value);
    const data = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) data[i] = s.charCodeAt(i);
    return data;
  }

  // The game asks a file's size, then reads it: decode once for both.
  let lastRead = { key: null, data: null };
  function readFile(path) {
    const key = FILE_PREFIX + path;
    if (lastRead.key === key) return lastRead.data;
    let data = null;
    try {
      const value = storage.getItem(key);
      if (value !== null) data = fromBase64(value);
    } catch (e) {
      data = null;
    }
    lastRead = { key, data };
    return data;
  }

  // Screenshots are too big to store and no use there: hand them to the
  // browser to save as a download.
  function download(path, data) {
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([data], { type: "image/bmp" }));
    link.download = path.split("/").pop();
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(link.href), 10000);
  }

  const fileFunctions = {
    js_file_size(pathPtr, pathLen) {
      const data = readFile(text(pathPtr, pathLen));
      return data ? data.length : -1;
    },
    js_file_read(pathPtr, pathLen, dst, cap) {
      const data = readFile(text(pathPtr, pathLen));
      lastRead = { key: null, data: null };
      if (!data) return -1;
      const n = Math.min(cap, data.length);
      bytes().set(data.subarray(0, n), dst);
      return n;
    },
    js_file_write(pathPtr, pathLen, src, len, append) {
      const path = text(pathPtr, pathLen);
      let data = bytes().slice(src, src + len);
      lastRead = { key: null, data: null };
      if (/\.bmp$/i.test(path)) {
        download(path, data);
        return 1;
      }
      try {
        if (append) {
          const old = readFile(path);
          lastRead = { key: null, data: null };
          if (old) {
            const joined = new Uint8Array(old.length + data.length);
            joined.set(old);
            joined.set(data, old.length);
            data = joined;
          }
        }
        storage.setItem(FILE_PREFIX + path, toBase64(data));
        return 1;
      } catch (e) {
        console.error("voxelrust.js: could not store " + path + " (" + data.length + " bytes): " + e);
        return 0;
      }
    },
  };

  // -------------------------------------------------------------- sound

  // The game mixes its own sound. After each frame enough of it is fetched
  // to stay a little ahead of what is being played, and queued in pieces.
  const AUDIO_PIECE = 1024; // samples
  const AUDIO_AHEAD = 0.12; // seconds
  let audio = null;
  let audioRate = 22050;
  let audioPaused = true;
  let audioNext = 0;

  function wakeAudio() {
    if (audio && audio.state === "suspended") audio.resume().catch(() => {});
  }

  function pumpAudio() {
    if (!audio || audioPaused || audio.state !== "running") return;
    const now = audio.currentTime;
    if (audioNext < now) audioNext = now + 0.02;
    for (let i = 0; i < 8 && audioNext < now + AUDIO_AHEAD; i++) {
      const ptr = wasm.voxelrust_audio(AUDIO_PIECE);
      if (!ptr) return;
      const samples = new Int16Array(wasm.memory.buffer, ptr, AUDIO_PIECE);
      const piece = audio.createBuffer(1, AUDIO_PIECE, audioRate);
      const out = piece.getChannelData(0);
      for (let s = 0; s < AUDIO_PIECE; s++) out[s] = samples[s] / 32768;
      const source = audio.createBufferSource();
      source.buffer = piece;
      source.connect(audio.destination);
      source.start(audioNext);
      audioNext += AUDIO_PIECE / audioRate;
    }
  }

  const audioFunctions = {
    js_audio_open(rate) {
      const Context = window.AudioContext || window.webkitAudioContext;
      if (!Context) return 0;
      try {
        audio = new Context();
        audioRate = rate;
        audioNext = 0;
        wakeAudio();
        return 1;
      } catch (e) {
        audio = null;
        return 0;
      }
    },
    js_audio_pause(paused) { audioPaused = paused !== 0; },
    js_audio_close() {
      if (audio) audio.close().catch(() => {});
      audio = null;
    },
  };

  // ---------------------------------------------------------- main loop

  function stop(message) {
    running = false;
    if (pointerLocked()) document.exitPointerLock();
    if (message && errorBox.hidden) showError(message);
  }

  // One frame of the game. Returns false when there will be no more.
  function runFrame() {
    if (!running) return false;
    if (fitCanvas()) events.push({ type: EV_WINDOW, event: WINEV_SIZE_CHANGED, w: canvas.width, h: canvas.height });
    const drawsBefore = stats.draws, verticesBefore = stats.vertices;
    let more;
    try {
      more = wasm.voxelrust_frame();
      pumpAudio();
    } catch (e) {
      // A trap: the game hit an internal error and logged what it was.
      console.error(e);
      stop("VoxelRust has stopped because of an internal error.\n\n" + (lastLog || e) + "\n\nReload the page to start again. Worlds are saved as you play.");
      return false;
    }
    stats.frames++;
    stats.lastFrameDraws = stats.draws - drawsBefore;
    stats.lastFrameVertices = stats.vertices - verticesBefore;
    applyFullscreen();
    if (!more) {
      // The player chose QUIT (or the game showed why it cannot go on).
      stop("VoxelRust has ended. Reload the page to play again.");
      return false;
    }
    return true;
  }

  function loop() {
    if (runFrame()) requestAnimationFrame(loop);
  }

  const env = Object.assign({}, glFunctions, windowFunctions, fileFunctions, audioFunctions);

  async function load() {
    if (query.has("gldebug")) {
      for (const name of Object.keys(glFunctions)) env[name] = checked(name, glFunctions[name]);
    }
    const response = await fetch("voxelrust.wasm");
    if (!response.ok) throw new Error("voxelrust.wasm: " + response.status + " " + response.statusText);
    const module = await WebAssembly.compile(await response.arrayBuffer());
    // Every function the game imports must be here, or it cannot start;
    // say which are missing rather than leave a cryptic link error.
    const missing = WebAssembly.Module.imports(module).filter((i) => i.module !== "env" || typeof env[i.name] !== "function").map((i) => i.module + "." + i.name);
    if (missing.length) throw new Error("voxelrust.js does not provide: " + missing.join(", "));
    const instance = await WebAssembly.instantiate(module, { env });
    wasm = instance.exports;
  }

  function start() {
    startBox.hidden = true;
    canvas.focus();
    // Starting inside the click is what lets the page play sound and, if
    // the settings say so, go full screen.
    let code;
    try {
      code = wasm.voxelrust_start();
    } catch (e) {
      console.error(e);
      showError("VoxelRust could not start.\n\n" + (lastLog || e));
      return;
    }
    if (code !== 0) {
      if (errorBox.hidden) showError("VoxelRust could not start.\n\n" + lastLog);
      return;
    }
    running = true;
    applyFullscreen();
    requestAnimationFrame(loop);
  }

  // For tests and the curious: the state of the page's side of things.
  window.voxelrust = {
    stats,
    env,
    get running() { return running; },
    get exports() { return wasm; },
    pushEvent(e) { events.push(e); },
  };

  if (typeof WebAssembly !== "object") {
    showError("This browser cannot run WebAssembly, which VoxelRust needs.");
    return;
  }
  load().then(() => {
    startNote.textContent = "Click to play";
    startBox.classList.add("ready");
    startBox.addEventListener("click", start, { once: true });
  }, (e) => {
    console.error(e);
    showError("VoxelRust could not be loaded.\n\n" + e + (location.protocol === "file:" ? "\n\nOpen this page through a web server, not as a file: see docs/WEB.md." : ""));
  });
})();
