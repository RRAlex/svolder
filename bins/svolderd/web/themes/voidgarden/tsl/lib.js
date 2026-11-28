/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The shading functions the layers share, written in TSL: the hashes and value noise, the frost,
// the heat haze, the glitch of the clip position, the sprite's falloff and its circle of
// confusion, the stars; the palette's colours; the scene's uniform block; the audio buffers; and
// the one way to read a buffer at another element's index. The module imports nothing: it is
// handed the two vendored namespaces (`T` three.webgpu.js, `S` three.tsl.js) by whoever loaded them
// (gpu.js), so evaluating it touches no GPU and the other themes load none of it.
//
//   createLib(T, S, tier)   `tier` 'webgpu' or 'webgl2' (gpu.js): where the audio lives
//                     -> { colour, uniformBlock, setUniforms, cpuBuffer, computeBuffer,
//                          upload, audio,
//                          neighbours, h11, h13, vn, side, frost, haze, glitch, spr, finish,
//                          stars, psz, coc, BANDS, SPECTRUM, WAVE }

/** The bands of one side of the analysis, the spectrum's log bins and the waveform's samples. */
export const BANDS = 240;
export const SPECTRUM = 64;
export const WAVE = 256;

/** The rig's numbers the layers read as uniforms (motion.js `u`). */
const NUMBERS = [
  'time', 'gt', 'bass', 'mid', 'high', 'l', 'r', 'width', 'loud', 'flux', 'cent', 'flat', 'peak',
  'shock', 'morph', 'frost', 'haze', 'cross', 'grain', 'static', 'tear', 'tearY', 'fade', 'seed',
  'calm', 'volume',
];

export function createLib(T, S, tier = 'webgpu') {
  const {
    float, vec2, vec3, vec4, fract, floor, dot, mix, smoothstep, step, max, min, abs, exp, clamp,
    select,
    length, uniform, instancedArray, screenCoordinate, uv,
  } = S;

  /**
   * A colour from a hex string, the way the scene's colours are all made: through three's colour
   * management (`new Color(hex)`), so the output reads as the hex did. Never `hex / 255` raw
   * floats: those read 73/255 too bright under the default output.
   */
  const colour = (hex) => S.color(new T.Color(hex));

  /** The scene's uniforms: one per number of the rig and the two palette colours. */
  function uniformBlock() {
    const u = {};
    for (const k of NUMBERS) u[k] = uniform(k === 'fade' ? 1 : 0);
    u.colA = uniform(new T.Color('#7fe0ff'));
    u.colB = uniform(new T.Color('#ff7a4d'));
    return u;
  }

  /** Move the block to the rig's numbers (`rig.u`) and ease the two colours toward the palette's
   * hexes by `rig.u.colRate`. */
  function setUniforms(u, rig, palette = null) {
    for (const k of NUMBERS) if (Number.isFinite(rig[k])) u[k].value = rig[k];
    if (palette) {
      const target = new T.Color();
      u.colA.value.lerp(target.set(palette[0]), rig.colRate ?? 1);
      u.colB.value.lerp(target.set(palette[1]), rig.colRate ?? 1);
    }
  }

  /** The buffers the CPU writes: `neighbours` refuses them (see there). */
  const written = new WeakSet();

  /** A storage buffer of `n` elements of `type` ('float', 'vec4': a vec3 would be padded) that
   * compute passes write and the shaders read. */
  const computeBuffer = (n, type = 'float') => instancedArray(n, type);

  /** The same, for the CPU to write (`upload`); the shaders read it at their own element's
   * index. */
  const cpuBuffer = (n, type = 'float') => {
    const buffer = computeBuffer(n, type);
    written.add(buffer);
    return buffer;
  };

  /** Write `values` into a CPU buffer and tell the renderer. */
  function upload(buffer, values) {
    buffer.value.array.set(values);
    buffer.value.needsUpdate = true;
  }

  /** `n` floats the CPU writes (`write`) and any shader stage reads at any index (`read(i)`, `i` a
   * node): a storage buffer on WebGPU; on WebGL2, which has no storage buffer to read at an index
   * in a render, a float texture of `n` by 1 read with a texel fetch, re-sent whole at each
   * write. */
  function table(n) {
    if (tier === 'webgpu') {
      const buffer = cpuBuffer(n);
      return { read: (i) => buffer.element(i), write: (values) => upload(buffer, values) };
    }
    const data = new Float32Array(n);
    const texture = new T.DataTexture(data, n, 1, T.RedFormat, T.FloatType);
    texture.minFilter = T.NearestFilter;
    texture.magFilter = T.NearestFilter;
    texture.generateMipmaps = false;
    texture.needsUpdate = true;
    return {
      read: (i) => S.textureLoad(texture, S.ivec2(S.int(i), 0)).x,
      write(values) {
        data.set(values);
        texture.needsUpdate = true;
      },
    };
  }

  /** The audio the shaders read, the same accessors on both tiers: both sides' 240 bands
   * (`bandL(i)`, `bandR(i)`), the spectrum's 64 log bins (`spectrum(i)`) and the synthesised
   * waveform's 256 samples (`wave(i)`), each written by `set`. */
  function audio() {
    const bands = table(BANDS * 2);
    const bins = table(SPECTRUM);
    const samples = table(WAVE);
    const both = new Float32Array(BANDS * 2);
    return {
      bandL: (i) => bands.read(i),
      bandR: (i) => bands.read(S.int(i).add(BANDS)),
      spectrum: (i) => bins.read(i),
      wave: (i) => samples.read(i),
      set({ left, right, bins: spectrumValues, samples: waveValues }) {
        if (left && right) {
          both.set(left, 0);
          both.set(right, BANDS);
          bands.write(both);
        }
        if (spectrumValues) bins.write(spectrumValues);
        if (waveValues) samples.write(waveValues);
      },
    };
  }

  /** A buffer a compute pass wrote, read at another element's index (a neighbour's, a previous
   * frame's): on the WebGL2 backend a plain read of another element returns the element's own value
   * without a word, so every such read goes through the buffer's texture path (`setPBO`). That
   * texture is refreshed after a compute pass and not by `upload`, so a buffer the CPU writes is
   * refused: copy it into a compute-written one first (a pass reading its own element's index).
   * The texture is made when the first pass that reads it is built, so a write that ran before that
   * is seen from the buffer's next write on: a pass that runs every frame has it from its second.
   * The audio is read through `audio()`, not through this. */
  const neighbours = (buffer) => {
    if (written.has(buffer)) throw new Error('neighbours: a CPU-written buffer is stale on WebGL2');
    // a view of its own: setPBO and toReadOnly change the node they are called on, and the pass
    // that writes the buffer needs its node as it was
    return S.storage(buffer.value, buffer.bufferType, buffer.bufferCount).setPBO(true).toReadOnly();
  };

  const h11 = (p) => {
    const a = fract(p.mul(0.1031));
    const b = a.mul(a.add(33.33));
    return fract(b.mul(b.add(b)));
  };
  const h13 = (p) => {
    const a = fract(p.mul(0.1031));
    const b = a.add(dot(a, a.zyx.add(31.32)));
    return fract(b.x.add(b.y).mul(b.z));
  };
  const vn = (p) => {
    const i = floor(p);
    const f0 = fract(p);
    const f = f0.mul(f0).mul(float(3).sub(f0.mul(2)));
    const at = (x, y, z) => h13(i.add(vec3(x, y, z)));
    return mix(
      mix(mix(at(0, 0, 0), at(1, 0, 0), f.x), mix(at(0, 1, 0), at(1, 1, 0), f.x), f.y),
      mix(mix(at(0, 0, 1), at(1, 0, 1), f.x), mix(at(0, 1, 1), at(1, 1, 1), f.x), f.y),
      f.z,
    );
  };
  const side = (x, w) => smoothstep(float(w).negate(), w, x);

  /** The frost's pale blue-grey: a colour from its hex, so it reads as the hex does. */
  const PALE = colour('#b8dbff');

  /** The frost of a muted scene: colour toward a pale blue-grey of its own lightness. */
  const frost = (c, amount) => {
    const l = dot(c, vec3(0.3, 0.59, 0.11));
    return mix(c, PALE.mul(l.mul(1.3).add(0.012)), amount.mul(0.85));
  };

  /** The heat haze of a changed path: a position pushed by three noises over time. */
  const haze = (p, a, amount, time) => {
    const q = p.mul(1.7);
    const t = time.mul(0.3);
    const n = vec3(vn(q.add(t)), vn(q.add(11).add(t)), vn(q.add(23).sub(t)));
    return p.add(n.sub(0.5).mul(a).mul(amount));
  };

  /** The glitch of a clip position `clip` (vec4) of element `id`: a jitter near the screen edge
   * under weak signal and a horizontal band torn sideways. Returns `{ clip, edge }`. */
  function glitch(clip, id, u) {
    const n = clip.xy.div(clip.w);
    const edge = smoothstep(0.5, 1, max(abs(n.x), abs(n.y)));
    const f = floor(u.gt.mul(18));
    const jitter = vec2(h11(id.mul(0.37).add(f)), h11(id.mul(0.91).add(f).add(7)))
      .sub(0.5).mul(0.06).mul(u.static).mul(edge);
    const m = n.add(jitter);
    const width = h11(floor(u.gt.mul(9)).add(2)).mul(0.06).add(0.07);
    const band = float(1).sub(step(width, abs(m.y.sub(u.tearY))));
    const x = m.x.add(band.mul(u.tear).mul(h11(floor(u.gt.mul(14))).sub(0.35)).mul(0.22));
    const out = vec4(x.mul(clip.w), m.y.mul(clip.w), clip.z, clip.w);
    const behind = clip.w.lessThanEqual(0);
    return { clip: select(behind, clip, out), edge: select(behind, float(0), edge) };
  }

  /** A sprite's falloff over its own square (`uv` 0-1): a soft disc with a bright core. */
  const spr = () => {
    const d = uv().sub(0.5);
    const r = dot(d, d).mul(4);
    return max(float(0), float(1).sub(r)).mul(exp(r.mul(-6)).mul(0.75).add(0.25));
  };

  /** The scene's last colour step: frost, the static's speckle at the edges, the track change's
   * fade. */
  function finish(c, u, edge = float(0)) {
    const speck = step(0.7, h13(vec3(screenCoordinate.xy, floor(u.gt.mul(30)))));
    return frost(c, u.frost).add(edge.mul(u.static).mul(speck).mul(0.18)).mul(u.fade);
  }

  /** Stars on a grid `q` (vec2) at density `dens`: a soft dot per lit cell. */
  function stars(q, dens, seed) {
    const i = floor(q);
    const f = fract(q);
    const h = h13(vec3(i, seed.mul(0.37).add(1)));
    const o = vec2(h13(vec3(i, 7.1)), h13(vec3(i, 3.3))).mul(0.7).add(0.15);
    const lit = smoothstep(0.3, 0, length(f.sub(o)))
      .mul(h.sub(1).add(dens).div(dens).mul(0.7).add(0.3));
    return select(h.lessThan(float(1).sub(dens)), float(0), lit);
  }

  /** A sprite's size in pixels at view depth `z`: nearer is bigger, never under 1.6 px. */
  const psz = (base, z, px, grain) => max(
    float(1.6),
    base.mul(px).mul(mix(float(1.45), float(1), grain)).div(max(float(0.1), z)),
  );

  /** The circle of confusion of depth `z` (depth of field): `maxC` its cap, `aper` the aperture,
   * `focusD` the focus distance. */
  const coc = (z, { maxC, aper, focusD, px }) => min(
    maxC,
    aper.mul(abs(float(1).div(focusD).sub(float(1).div(max(z, float(0.3)))))).mul(60).mul(px),
  );

  return {
    colour,
    uniformBlock,
    setUniforms,
    cpuBuffer,
    computeBuffer,
    upload,
    audio,
    neighbours,
    h11,
    h13,
    vn,
    side,
    frost,
    haze,
    glitch,
    spr,
    finish,
    stars,
    psz,
    coc,
    clamp,
    BANDS,
    SPECTRUM,
    WAVE,
  };
}
