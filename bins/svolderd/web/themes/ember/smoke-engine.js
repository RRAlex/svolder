/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The smoke's engine: the GPU side of the smoke, a small stable-fluids simulation (Stam 1999, run
// on the GPU as in GPU Gems 38, with vorticity confinement after Fedkiw, Stam and Jensen 2001)
// drawn on a canvas. It owns the WebGL2 context, the programs, the grids and their resizing, the
// passes, the notes' atlas, the readback the browser checks use and the context's loss. It
// decides nothing: it executes the plans smoke-director.js makes (see `FramePlan` there).
//
// The engine's interface, from `createEngine(canvas, tuning = TUNING)` (`null` when there is no
// WebGL2 or no half-float target):
//   fit(size)       size the canvas and the grids to `size`, CSS pixels `[width, height]`; the old
//                   content moves into new grids; `true` when anything changed.
//   ready           whether the grids exist.
//   hasNotes        whether the notes' atlas could be made.
//   shift(du, dv)   move both grids by (du, dv) in uv; the dye that leaves the view is held (half a
//                   viewport each way) and comes back on the reverse move; what comes in from
//                   beyond the hold is empty.
//   run(plan)       one simulation step of a `run` plan: its splats, note and band fade, advected.
//   draw(params)    the dye on the canvas at `{ bright, sat }`.
//   clear()         empty the velocity and dye grids.
//   readback()      the grids as floats (check-only), or `null`.
//   density()       one read of the dye as a `(x, y, half)` mean-amount function, or `null`.
//   stop()          free everything, once.
//   onLost          a function the engine calls when the browser takes the context.

import { TUNING, gridSize, splatBox } from './smoke-model.js';
import { drawAtlas } from './smoke-notes.js';

/** The context's attributes: transparent, premultiplied, nothing the smoke does not use. */
const ATTRS = {
  alpha: true, premultipliedAlpha: true, depth: false, stencil: false, antialias: false,
  powerPreference: 'low-power',
};

/** The vertex shader of every pass: one triangle over the target, with the neighbours' coordinates
 * (`uTexel` is one texel of the grid being written). */
const VERT = `#version 300 es
precision highp float;
uniform vec2 uTexel;
out vec2 vUv;
out vec2 vL;
out vec2 vR;
out vec2 vT;
out vec2 vB;
void main() {
  vec2 p = vec2(float((gl_VertexID & 1) * 2), float((gl_VertexID >> 1) * 2));
  vUv = p;
  vL = p - vec2(uTexel.x, 0.0);
  vR = p + vec2(uTexel.x, 0.0);
  vT = p + vec2(0.0, uTexel.y);
  vB = p - vec2(0.0, uTexel.y);
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

/** The head of every fragment shader. */
const HEAD = `#version 300 es
precision highp float;
precision highp sampler2D;
in vec2 vUv;
in vec2 vL;
in vec2 vR;
in vec2 vT;
in vec2 vB;
out vec4 o;
`;

/** The fragment shaders, by pass. Velocity is in cells per second, so one cell is one unit. */
const FRAGMENTS = {
  // Semi-Lagrangian advection of `uSrc` by `uVel` (read the source where the fluid came from),
  // lost at `uDiss` per second. `uVelTexel` is one velocity cell in uv, whatever grid is written.
  advect: `
uniform sampler2D uVel;
uniform sampler2D uSrc;
uniform vec2 uVelTexel;
uniform float uDt;
uniform float uDiss;
void main() {
  vec2 back = vUv - uDt * texture(uVel, vUv).xy * uVelTexel;
  o = texture(uSrc, back) / (1.0 + uDiss * uDt);
}`,
  // The curl of the velocity.
  curl: `
uniform sampler2D uVel;
void main() {
  float l = texture(uVel, vL).y;
  float r = texture(uVel, vR).y;
  float t = texture(uVel, vT).x;
  float b = texture(uVel, vB).x;
  o = vec4(0.5 * (r - l - t + b), 0.0, 0.0, 1.0);
}`,
  // Vorticity confinement: a force along N x w, N the direction of the curl's growth.
  vorticity: `
uniform sampler2D uVel;
uniform sampler2D uCurl;
uniform float uStrength;
uniform float uDt;
void main() {
  float l = texture(uCurl, vL).x;
  float r = texture(uCurl, vR).x;
  float t = texture(uCurl, vT).x;
  float b = texture(uCurl, vB).x;
  float c = texture(uCurl, vUv).x;
  vec2 grad = 0.5 * vec2(abs(r) - abs(l), abs(t) - abs(b));
  vec2 n = grad / (length(grad) + 1e-5);
  o = vec4(texture(uVel, vUv).xy + uStrength * c * vec2(n.y, -n.x) * uDt, 0.0, 1.0);
}`,
  // The divergence, the border's normal velocity mirrored so the edges are walls.
  divergence: `
uniform sampler2D uVel;
void main() {
  vec2 c = texture(uVel, vUv).xy;
  float l = texture(uVel, vL).x;
  float r = texture(uVel, vR).x;
  float t = texture(uVel, vT).y;
  float b = texture(uVel, vB).y;
  if (vL.x < 0.0) l = -c.x;
  if (vR.x > 1.0) r = -c.x;
  if (vT.y > 1.0) t = -c.y;
  if (vB.y < 0.0) b = -c.y;
  o = vec4(0.5 * (r - l + t - b), 0.0, 0.0, 1.0);
}`,
  // One Jacobi pass of the pressure's Poisson equation.
  pressure: `
uniform sampler2D uPressure;
uniform sampler2D uDivergence;
void main() {
  float l = texture(uPressure, vL).x;
  float r = texture(uPressure, vR).x;
  float t = texture(uPressure, vT).x;
  float b = texture(uPressure, vB).x;
  o = vec4(0.25 * (l + r + t + b - texture(uDivergence, vUv).x), 0.0, 0.0, 1.0);
}`,
  // The velocity less the pressure's gradient.
  gradient: `
uniform sampler2D uVel;
uniform sampler2D uPressure;
void main() {
  float l = texture(uPressure, vL).x;
  float r = texture(uPressure, vR).x;
  float t = texture(uPressure, vT).x;
  float b = texture(uPressure, vB).x;
  o = vec4(texture(uVel, vUv).xy - 0.5 * vec2(r - l, t - b), 0.0, 1.0);
}`,
  // `uScale` times the texture (the previous pressure's damping, the away fade's rest).
  scale: `
uniform sampler2D uSrc;
uniform float uScale;
void main() {
  o = uScale * texture(uSrc, vUv);
}`,
  // The texture moved by `uShift` (uv), what comes in from outside empty; a zero shift is a copy
  // into a grid of another size.
  shift: `
uniform sampler2D uSrc;
uniform vec2 uShift;
void main() {
  vec2 at = vUv - uShift;
  bool inside = at.x >= 0.0 && at.x <= 1.0 && at.y >= 0.0 && at.y <= 1.0;
  o = inside ? texture(uSrc, at) : vec4(0.0);
}`,
  // The dye grid `uSrc` placed in the middle of the hold texture `uHold` (the grid's rows start
  // `uOff` up it, in the hold's uv: the hold is twice as high as the grid); what the hold has
  // above and below is scaled by `uKeep`, the share of its dye left since it was last touched.
  hold: `
uniform sampler2D uSrc;
uniform sampler2D uHold;
uniform float uOff;
uniform float uKeep;
void main() {
  float g = (vUv.y - uOff) * 2.0;
  o = (g >= 0.0 && g < 1.0) ? texture(uSrc, vec2(vUv.x, g)) : uKeep * texture(uHold, vUv);
}`,
  // The middle of the hold texture `uSrc`, as a grid again (the inverse of `hold`).
  take: `
uniform sampler2D uSrc;
uniform float uOff;
void main() {
  o = texture(uSrc, vec2(vUv.x, uOff + vUv.y * 0.5));
}`,
  // The texture scaled by `uKeep` between `uLo` and `uHi` (uv, across), the edges `uSoft` wide.
  band: `
uniform sampler2D uSrc;
uniform float uLo;
uniform float uHi;
uniform float uSoft;
uniform float uKeep;
void main() {
  float m = smoothstep(uLo - uSoft, uLo, vUv.x) * (1.0 - smoothstep(uHi, uHi + uSoft, vUv.x));
  o = texture(uSrc, vUv) * mix(1.0, uKeep, m);
}`,
  // A Gaussian splat: `uAdd` added, then the value pulled toward `uTarget` at `uRate`. The
  // distance is in units of the canvas's height (`uAspect` is its width over its height).
  splat: `
uniform sampler2D uSrc;
uniform vec2 uPoint;
uniform float uRadius;
uniform float uAspect;
uniform vec4 uAdd;
uniform vec4 uTarget;
uniform float uRate;
void main() {
  vec2 d = vUv - uPoint;
  d.x *= uAspect;
  float g = exp(-dot(d, d) / (uRadius * uRadius));
  vec4 v = texture(uSrc, vUv) + uAdd * g;
  v = mix(v, uTarget, clamp(uRate * g, 0.0, 1.0));
  o = v;
}`,
  // A music note carved out of the dye: inside the glyph's mask (red) `uCarve` of the dye is
  // removed, so the glyph is a gap and the page shows through it, and what is left mixes toward
  // `uTint`, a lighter tint of the emission colour; around it the glyph's halo (green, outside the
  // mask) thins the dye a little, which keeps the shape's edge crisp. No dye is added. The cell is
  // `uSize` (units of the canvas's height) wide, centred on `uPoint`, turned by `uRot` (cosine,
  // sine); `uGlyph` picks the atlas's cell.
  stamp: `
uniform sampler2D uSrc;
uniform sampler2D uAtlas;
uniform vec2 uPoint;
uniform vec2 uRot;
uniform vec4 uTint;
uniform float uAspect;
uniform float uSize;
uniform float uGlyph;
uniform float uInk;
uniform float uHalo;
uniform float uCarve;
void main() {
  vec4 d = texture(uSrc, vUv);
  vec2 p = vUv - uPoint;
  p.x *= uAspect;
  p = vec2(uRot.x * p.x + uRot.y * p.y, -uRot.y * p.x + uRot.x * p.y) / uSize + 0.5;
  if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) {
    o = d;
    return;
  }
  vec2 m = texture(uAtlas, vec2((uGlyph + p.x) / 3.0, p.y)).rg;
  vec3 col = d.a > 1e-4 ? d.rgb / d.a : uTint.rgb;
  col = mix(col, uTint.rgb, m.r * uInk);
  float amount = d.a * (1.0 - uCarve * m.r) * (1.0 - uHalo * m.g * (1.0 - m.r) * (1.0 - m.r));
  o = vec4(col * amount, amount);
}`,
  // The dye over transparency: its mean colour, an opacity from its amount and the brightness,
  // premultiplied; `uSat` 0 is grey. A little noise hides 8-bit steps.
  draw: `
uniform sampler2D uDye;
uniform float uBright;
uniform float uSat;
uniform float uGain;
uniform float uPeak;
void main() {
  vec4 d = texture(uDye, vUv);
  vec3 col = d.rgb / max(d.a, 1e-4);
  float alpha = uPeak * uBright * (1.0 - exp(-max(d.a, 0.0) * uGain));
  col = mix(vec3(dot(col, vec3(0.299, 0.587, 0.114))), col, uSat);
  float n = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
  alpha = max(alpha + n / 255.0 * smoothstep(0.0, 2.0 / 255.0, alpha), 0.0);
  o = vec4(col * alpha, alpha);
}`,
};

/** A context on `canvas` with a half-float colour attachment that can be rendered to, or the
 * reason there is none: `{ gl }` or `{ why }`. */
function contextOn(canvas) {
  const gl = canvas.getContext('webgl2', ATTRS);
  if (!gl) return { why: 'This browser cannot draw the smoke (no WebGL2)' };
  const half = Boolean(gl.getExtension('EXT_color_buffer_float'))
    || Boolean(gl.getExtension('EXT_color_buffer_half_float'));
  let complete = false;
  if (half) {
    const tex = gl.createTexture();
    const fbo = gl.createFramebuffer();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, 4, 4, 0, gl.RGBA, gl.HALF_FLOAT, null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    complete = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
    if (complete) {
      // the splats copy a box of one half-float grid over another, from its framebuffer
      const into = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, into);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, 4, 4, 0, gl.RGBA, gl.HALF_FLOAT, null);
      while (gl.getError() !== gl.NO_ERROR); // an earlier error is not ours
      gl.copyTexSubImage2D(gl.TEXTURE_2D, 0, 0, 0, 0, 0, 2, 2);
      complete = gl.getError() === gl.NO_ERROR;
      gl.deleteTexture(into);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.deleteFramebuffer(fbo);
    gl.deleteTexture(tex);
  }
  if (!complete) {
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return { why: 'This browser cannot draw the smoke (no half-float rendering)' };
  }
  return { gl };
}

/** Whether the smoke can run here: `ok`, and when it cannot, `why` in one line. The test context
 * is lost again at once. */
export function supported() {
  const probe = contextOn(document.createElement('canvas'));
  if (!probe.gl) return { ok: false, why: probe.why };
  probe.gl.getExtension('WEBGL_lose_context')?.loseContext();
  return { ok: true, why: null };
}

/** A compiled program for the fragment source `frag`, with its uniforms' locations. */
function program(gl, frag) {
  const shader = (type, source) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, source);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      const log = gl.getShaderInfoLog(s);
      gl.deleteShader(s);
      throw new Error(`shader: ${log}`);
    }
    return s;
  };
  const vs = shader(gl.VERTEX_SHADER, VERT);
  const fs = shader(gl.FRAGMENT_SHADER, HEAD + frag);
  const prog = gl.createProgram();
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(prog);
    gl.deleteProgram(prog);
    throw new Error(`program: ${log}`);
  }
  const uniforms = {};
  const count = gl.getProgramParameter(prog, gl.ACTIVE_UNIFORMS);
  for (let i = 0; i < count; i++) {
    const { name } = gl.getActiveUniform(prog, i);
    uniforms[name] = gl.getUniformLocation(prog, name);
  }
  return { prog, uniforms };
}

/** The notes' atlas as one RG8 texture (red the glyph, green its halo, the cells side by side),
 * or `null` when it cannot be made (the smoke then runs without notes). */
function uploadAtlas(gl, tuning) {
  const cell = tuning.noteCell;
  let tex = null;
  try {
    for (let i = 0; i < 16; i++) { // an earlier error is not ours
      if (gl.getError() === gl.NO_ERROR) break;
    }
    const canvas = typeof OffscreenCanvas === 'undefined' ? document.createElement('canvas')
      : new OffscreenCanvas(3 * cell, cell);
    canvas.width = 3 * cell;
    canvas.height = cell;
    drawAtlas(canvas.getContext('2d'), cell, {
      blur: tuning.noteHaloBlur, width: tuning.noteHaloWidth,
    });
    tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RG8, 3 * cell, cell, 0, gl.RG, gl.UNSIGNED_BYTE, canvas);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    if (gl.getError() === gl.NO_ERROR) return { tex, w: 3 * cell, h: cell };
  } catch {
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
  }
  if (tex) gl.deleteTexture(tex);
  return null;
}

/**
 * The engine on `canvas`, or `null` when it cannot run (no WebGL2, no half-float target or a
 * program that does not build). `tuning` is a `TUNING`-shaped object.
 */
export function createEngine(canvas, tuning = TUNING) {
  const made = contextOn(canvas);
  if (!made.gl) return null;
  try {
    return build(canvas, made.gl, tuning);
  } catch (error) {
    // a throw after the context exists: the context goes now, not at garbage collection
    made.gl.getExtension('WEBGL_lose_context')?.loseContext();
    throw error;
  }
}

/** [`createEngine`]'s work on the context `gl` that `contextOn` made for `canvas`. */
function build(canvas, gl, tuning) {
  const lose = gl.getExtension('WEBGL_lose_context');
  const programs = {};
  const textures = [];
  let grids = null;
  /** The dye a scroll moved out of view: a texture of the grid's width and twice its height, the
   * grid's rows in its middle and half a viewport of dye above and below; never simulated, only
   * aged by the dye's own loss when it is next touched. */
  let hold = null;
  /** The simulated time (s) so far, and when the hold was last touched. */
  let simTime = 0;
  let heldAt = 0;
  try {
    for (const [name, frag] of Object.entries(FRAGMENTS)) programs[name] = program(gl, frag);
  } catch {
    for (const p of Object.values(programs)) gl.deleteProgram(p.prog);
    lose?.loseContext();
    return null;
  }
  const atlasTex = uploadAtlas(gl, tuning);
  let dead = false;
  let lost = false;
  /** The size last given to `fit`, CSS pixels. */
  let size = [0, 0];
  /** One texture of the grid size, with its framebuffer. */
  const make = (w, h) => {
    const tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, w, h, 0, gl.RGBA, gl.HALF_FLOAT, null);
    const fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const t = { tex, fbo, w, h };
    textures.push(t);
    return t;
  };
  /** A pair of grids, one read while the other is written. */
  const pair = (w, h) => {
    const p = { read: make(w, h), write: make(w, h) };
    p.swap = () => {
      [p.read, p.write] = [p.write, p.read];
    };
    return p;
  };
  /** Free textures and their framebuffers. */
  const free = (...ts) => {
    for (const t of ts) {
      gl.deleteTexture(t.tex);
      gl.deleteFramebuffer(t.fbo);
      textures.splice(textures.indexOf(t), 1);
    }
  };

  /** Run pass `name` into `out` (`null` is the canvas) with `bind` textures and `values`. */
  const pass = (name, out, bind, values) => {
    const { prog, uniforms } = programs[name];
    gl.useProgram(prog);
    let unit = 0;
    for (const [uniform, texture] of Object.entries(bind)) {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, texture.tex);
      gl.uniform1i(uniforms[uniform], unit++);
    }
    for (const [uniform, value] of Object.entries(values)) {
      const loc = uniforms[uniform];
      if (typeof value === 'number') gl.uniform1f(loc, value);
      else if (value.length === 2) gl.uniform2f(loc, ...value);
      else gl.uniform4f(loc, ...value);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, out ? out.fbo : null);
    gl.viewport(0, 0, out ? out.w : canvas.width, out ? out.h : canvas.height);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };
  /** One texel of grid `t` in uv. */
  const texel = (t) => [1 / t.w, 1 / t.h];

  /** Make the canvas and the grids fit `at`, the layer's size in CSS pixels; the old content moves
   * into new grids. `true` when something changed. */
  const fit = (at) => {
    const [cw, ch] = at;
    if (!(cw > 0 && ch > 0)) return false;
    size = [cw, ch];
    const scale = Math.min(window.devicePixelRatio || 1, tuning.dprCap);
    const bw = Math.max(1, Math.round(cw * scale));
    const bh = Math.max(1, Math.round(ch * scale));
    let changed = false;
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
      changed = true;
    }
    const v = gridSize(cw, ch, tuning.simRes);
    const d = gridSize(cw, ch, tuning.dyeRes);
    if (grids && grids.vel.read.w === v.w && grids.vel.read.h === v.h
      && grids.dye.read.w === d.w && grids.dye.read.h === d.h) return changed;
    gl.disable(gl.BLEND);
    const next = {
      vel: pair(v.w, v.h), dye: pair(d.w, d.h), pressure: pair(v.w, v.h),
      divergence: make(v.w, v.h), curl: make(v.w, v.h),
    };
    const nextHold = pair(d.w, 2 * d.h);
    if (grids) {
      for (const key of ['vel', 'dye']) {
        pass('shift', next[key].write, { uSrc: grids[key].read }, {
          uTexel: texel(next[key].write), uShift: [0, 0],
        });
        next[key].swap();
      }
      free(grids.vel.read, grids.vel.write, grids.dye.read, grids.dye.write,
        grids.pressure.read, grids.pressure.write, grids.divergence, grids.curl);
    }
    if (hold) free(hold.read, hold.write);
    hold = nextHold;
    heldAt = simTime;
    grids = next;
    return true;
  };

  /** Move both grids by (`du`, `dv`) in uv, the scroll's delta the director planned. The velocity
   * that comes in from outside is empty. The dye goes through the hold: the grid is written into
   * the middle of it, the whole is moved, and the middle is the grid again, so dye that leaves
   * the view stays in the hold (half a viewport each way) and returns on the reverse scroll;
   * beyond the hold what comes in is empty. */
  const shift = (du, dv) => {
    pass('shift', grids.vel.write, { uSrc: grids.vel.read }, {
      uTexel: texel(grids.vel.write), uShift: [du, dv],
    });
    grids.vel.swap();
    const rows = grids.dye.read.h;
    const off = Math.floor(rows / 2) / (2 * rows);
    const keep = Math.exp(-tuning.dyeDissipation * Math.max(0, simTime - heldAt));
    heldAt = simTime;
    pass('hold', hold.write, { uSrc: grids.dye.read, uHold: hold.read }, {
      uTexel: texel(hold.write), uOff: off, uKeep: keep,
    });
    hold.swap();
    pass('shift', hold.write, { uSrc: hold.read }, {
      uTexel: texel(hold.write), uShift: [du, dv / 2],
    });
    hold.swap();
    pass('take', grids.dye.write, { uSrc: hold.read }, {
      uTexel: texel(grids.dye.write), uOff: off,
    });
    grids.dye.swap();
  };

  /** Stamp the planned `note` into the dye. */
  const stamp = (note) => {
    const [cw, ch] = size;
    pass('stamp', grids.dye.write, { uSrc: grids.dye.read, uAtlas: atlasTex }, {
      uTexel: texel(grids.dye.read), uPoint: [note.uv.x, note.uv.y],
      uRot: [Math.cos(note.angle), Math.sin(note.angle)],
      uTint: [note.tint[0], note.tint[1], note.tint[2], 1], uAspect: note.aspect,
      uSize: note.cell / ch, uGlyph: note.glyph, uInk: tuning.noteInk, uHalo: tuning.noteHalo,
      uCarve: tuning.noteCarve,
    });
    grids.dye.swap();
  };

  /** Apply splat `values` of `s` to `target`'s current grid in place: drawn into the pair's other
   * grid inside the Gaussian's 3-radius box only, then copied back over the same box. */
  const splat = (target, s, values) => {
    const [cw, ch] = size;
    const box = splatBox({ x: s.x, y: s.y }, s.radius, cw / ch, [target.read.w, target.read.h]);
    if (!box) return;
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(box.x, box.y, box.w, box.h);
    pass('splat', target.write, { uSrc: target.read }, {
      uPoint: [s.x, s.y], uRadius: s.radius, uAspect: cw / ch, uTexel: texel(target.read),
      ...values,
    });
    gl.disable(gl.SCISSOR_TEST);
    gl.bindTexture(gl.TEXTURE_2D, target.read.tex);
    gl.copyTexSubImage2D(gl.TEXTURE_2D, 0, box.x, box.y, box.x, box.y, box.w, box.h);
  };

  /** One step of the simulation the director planned. */
  const step = (plan) => {
    const { dt } = plan;
    simTime += dt;
    gl.disable(gl.BLEND);
    const { vel, dye, pressure, divergence, curl } = grids;
    const vt = texel(vel.read);
    const loss = tuning.velocityDissipation + (plan.velocityLoss ?? 0);
    pass('advect', vel.write, { uVel: vel.read, uSrc: vel.read }, {
      uTexel: vt, uVelTexel: vt, uDt: dt, uDiss: loss,
    });
    vel.swap();
    if (plan.vorticity !== false) {
      pass('curl', curl, { uVel: vel.read }, { uTexel: vt });
      pass('vorticity', vel.write, { uVel: vel.read, uCurl: curl }, {
        uTexel: vt, uStrength: tuning.curl, uDt: dt,
      });
      vel.swap();
    }
    pass('divergence', divergence, { uVel: vel.read }, { uTexel: vt });
    pass('scale', pressure.write, { uSrc: pressure.read }, {
      uTexel: vt, uScale: tuning.pressureDamp,
    });
    pressure.swap();
    for (let i = 0; i < tuning.pressureIterations; i++) {
      pass('pressure', pressure.write, { uPressure: pressure.read, uDivergence: divergence }, {
        uTexel: vt,
      });
      pressure.swap();
    }
    pass('gradient', vel.write, { uVel: vel.read, uPressure: pressure.read }, { uTexel: vt });
    vel.swap();
    for (const s of plan.splats) {
      splat(vel, s, {
        uAdd: s.add ?? [0, 0, 0, 0], uTarget: s.pull ?? [0, 0, 0, 0], uRate: s.rate ?? 0,
      });
      if (s.dye) splat(dye, s, { uAdd: s.dye, uTarget: [0, 0, 0, 0], uRate: 0 });
    }
    if (plan.note) stamp(plan.note);
    if (plan.bandFade) {
      pass('band', dye.write, { uSrc: dye.read }, {
        uTexel: texel(dye.read), uLo: plan.bandFade.lo, uHi: plan.bandFade.hi,
        uSoft: tuning.hazeFadeSoft, uKeep: plan.bandFade.keep,
      });
      dye.swap();
    }
    pass('advect', dye.write, { uVel: vel.read, uSrc: dye.read }, {
      uTexel: texel(dye.read), uVelTexel: vt, uDt: dt, uDiss: tuning.dyeDissipation,
    });
    dye.swap();
  };

  /** Draw the dye on the canvas at the planned brightness and saturation. */
  const draw = ({ bright, sat }) => {
    pass('draw', null, { uDye: grids.dye.read }, {
      uTexel: [1, 1], uBright: bright, uSat: sat, uGain: tuning.drawGain, uPeak: tuning.peakAlpha,
    });
  };
  /** Empty the velocity and dye grids. */
  const clear = () => {
    if (!grids) return;
    for (const t of [grids.vel.read, grids.vel.write, grids.dye.read, grids.dye.write,
      hold.read, hold.write]) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
  };

  /** The grids as floats and the atlas as bytes, for the browser checks. */
  const readback = () => {
    if (!grids || dead || lost || !gl.getExtension('EXT_color_buffer_float')) return null;
    const grab = (t) => {
      gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
      const data = new Float32Array(t.w * t.h * 4);
      gl.readPixels(0, 0, t.w, t.h, gl.RGBA, gl.FLOAT, data);
      return { w: t.w, h: t.h, data };
    };
    const out = { vel: grab(grids.vel.read), dye: grab(grids.dye.read), atlas: null };
    if (atlasTex) {
      const fbo = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, atlasTex.tex,
        0);
      const data = new Uint8Array(atlasTex.w * atlasTex.h * 4);
      gl.readPixels(0, 0, atlasTex.w, atlasTex.h, gl.RGBA, gl.UNSIGNED_BYTE, data);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.deleteFramebuffer(fbo);
      out.atlas = { w: atlasTex.w, h: atlasTex.h, data };
    }
    return gl.getError() === gl.NO_ERROR ? out : null;
  };

  /** The dye's density for placing a note: a function `(x, y, half)` giving the mean dye amount in
   * the box of `half` CSS px around the viewport point `x`, `y`, from one read of the dye grid
   * (a note comes at most every few seconds, so this is no per-frame read); `null` when the grid
   * cannot be read. */
  let densityData = null;
  const density = () => {
    if (!grids || dead || lost || !gl.getExtension('EXT_color_buffer_float')) return null;
    const t = grids.dye.read;
    if (densityData?.length !== t.w * t.h * 4) densityData = new Float32Array(t.w * t.h * 4);
    const data = densityData;
    gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
    gl.readPixels(0, 0, t.w, t.h, gl.RGBA, gl.FLOAT, data);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (gl.getError() !== gl.NO_ERROR) return null;
    const [cw, ch] = size;
    return (x, y, half) => {
      const x0 = Math.max(0, Math.floor(((x - half) / cw) * t.w));
      const x1 = Math.min(t.w, Math.ceil(((x + half) / cw) * t.w));
      const y0 = Math.max(0, Math.floor((1 - (y + half) / ch) * t.h));
      const y1 = Math.min(t.h, Math.ceil((1 - (y - half) / ch) * t.h));
      let sum = 0;
      let n = 0;
      for (let gy = y0; gy < y1; gy++) {
        for (let gx = x0; gx < x1; gx++) {
          sum += data[(gy * t.w + gx) * 4 + 3];
          n++;
        }
      }
      return n > 0 ? sum / n : 0;
    };
  };

  /** The browser took the context: nothing more is drawn or freed. */
  const contextLost = () => {
    lost = true;
    engine.onLost?.();
  };
  canvas.addEventListener('webglcontextlost', contextLost);

  const engine = {
    onLost: null,
    hasNotes: Boolean(atlasTex),
    get ready() {
      return grids !== null;
    },
    fit,
    shift,
    run: step,
    draw,
    clear,
    readback,
    density,
    /** Free everything, once; a context the browser took is left as it is. */
    stop() {
      if (dead) return;
      dead = true;
      densityData = null;
      canvas.removeEventListener('webglcontextlost', contextLost);
      if (!lost && !gl.isContextLost()) {
        for (const t of [...textures]) free(t);
        if (atlasTex) gl.deleteTexture(atlasTex.tex);
        for (const p of Object.values(programs)) gl.deleteProgram(p.prog);
        lose?.loseContext();
      }
    },
  };
  return engine;
}
