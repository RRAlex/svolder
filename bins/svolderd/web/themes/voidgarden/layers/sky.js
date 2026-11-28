/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's sky layer and the light every land layer shares: the mood (sun height and side, the
// haze, the fog, the cloud and the stars, each eased toward the next mood by a time constant), the
// palette programs that turn the two cover colours into the sky, the sun, the foliage and the
// water, the gradient sky with its sun, clouds and stars on a dome that follows the camera, and
// the shading functions the land, the river and the trees read the same light through (fog, mist,
// the sky's colour in a direction). The moods and palette are pure (`createMood`, `colourSpec`,
// node-tested); the layer writes them to uniforms in `ctx.env`.
//
//   createMood({ seed })       the mood's live numbers `M`, `step(sdt, section, loud)`
//   paletteProgram(name, hueA, hueB, t)   the program's hues and saturation
//   colourSpec(M, program)     the colours as [hue, saturation, lightness, scale]
//   MOODS, MOOD_FOR, PROGRAMS  the tables
//   shading(ctx)               the shared uniforms and TSL nodes (`ctx.env` holds the uniforms):
//                              `{ env, sst, hs2, nd2, skyBase, fogCol, mistI, fog, sync, reset }`;
//                              `sync(frame)` writes the light (every land layer's update calls it)
//   skyLayer(ctx)              the layer factory

import { hashStr, mulberry } from '../rng.js';

const TAU = Math.PI * 2;
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const lerp = (a, b, t) => a + (b - a) * t;
const fr = (x) => x - Math.floor(x);
const sstJs = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

/** The moods: the sun's height and side (from the heading), its strength and glow, the horizon's
 * lightness and saturation, the zenith's lightness, the fog, the mist, the cloud, the stars, a
 * storm's light (lit), the exposure, the ambient, and a moon (not a sun) when on. */
export const MOODS = Object.freeze([
  { name: 'dusk', el: 0.03, az: 0.12, sun: 1.2, glow: 1.2, hl: 0.34, hs: 0.55, zl: 0.1, fog: 0.004,
    mist: 0.3, cl: 0.25, st: 0.1, lit: 0, ex: 1.15, amb: 0.5, moon: 0 },
  { name: 'golden hour', el: 0.07, az: -0.05, sun: 1.5, glow: 1.4, hl: 0.4, hs: 0.6, zl: 0.16,
    fog: 0.0048, mist: 0.4, cl: 0.1, st: 0, lit: 0, ex: 1.05, amb: 0.6, moon: 0 },
  { name: 'night', el: 0.16, az: -0.4, sun: 0.3, glow: 0.5, hl: 0.1, hs: 0.45, zl: 0.025,
    fog: 0.0035, mist: 0.25, cl: 0.12, st: 1, lit: 0, ex: 1.6, amb: 0.35, moon: 1 },
  { name: 'storm', el: 0.1, az: 0.35, sun: 0.4, glow: 0.5, hl: 0.24, hs: 0.2, zl: 0.07, fog: 0.0065,
    mist: 0.5, cl: 0.95, st: 0, lit: 1, ex: 1.35, amb: 0.45, moon: 0 },
  { name: 'dawn', el: 0.015, az: -0.18, sun: 1, glow: 1.1, hl: 0.4, hs: 0.4, zl: 0.18, fog: 0.005,
    mist: 0.6, cl: 0.3, st: 0.05, lit: 0, ex: 1.15, amb: 0.6, moon: 0 },
  { name: 'overcast', el: 0.3, az: 0.05, sun: 0.5, glow: 0.4, hl: 0.36, hs: 0.18, zl: 0.26,
    fog: 0.0058, mist: 0.4, cl: 0.85, st: 0, lit: 0, ex: 1.2, amb: 0.75, moon: 0 },
]);

/** The moods a section can bring, by index into MOODS. */
export const MOOD_FOR = Object.freeze({
  intro: [0, 1, 4], build: [4, 5, 1, 0], drop: [1, 0, 3, 2], break: [2, 5, 3, 4],
});

/** The mood's numbers that ease between moods. */
const NUMERIC = Object.keys(MOODS[0]).filter((k) => typeof MOODS[0][k] === 'number');

/** How fast the mood eases (per second) and how long a new section type must hold before the
 * mood follows it when the director names none. */
const MOOD_RATE = 0.3;
const SECTION_HOLD_S = 3.5;

/** The section type a loudness gives when the director names none. */
const sectionOfLoud = (loud) => (loud > 0.5 ? 'drop' : loud < 0.22 ? 'break' : 'build');

/**
 * The mood of a seed: `M` the live numbers (the first mood at once), `index` the mood aimed at.
 * `step(sdt, section, loud)` moves `sdt` scene seconds: the section's change (the director's, or
 * the loudness's after 3.5 s) brings the next mood of its bag, and the numbers ease toward it by
 * a time constant, so the same seconds give the same numbers at any step rate.
 */
export function createMood({ seed = 0 } = {}) {
  const rng = mulberry(hashStr(`mood:${seed}`));
  const bags = {};
  let index = rng() < 0.5 ? 0 : 1;
  let section = null;
  let candidate = null;
  let held = 0;
  const M = { ...MOODS[index] };
  const nextMood = (type) => {
    const list = MOOD_FOR[type] ?? MOOD_FOR.build;
    const bag = (bags[type] ??= []);
    if (!bag.length) {
      const a = list.slice();
      for (let i = a.length - 1; i > 0; i--) {
        const j = Math.floor(rng() * (i + 1));
        [a[i], a[j]] = [a[j], a[i]];
      }
      bag.push(...a);
    }
    let m = bag.shift();
    if (m === index) m = list[(list.indexOf(m) + 1) % list.length];
    index = m;
  };
  return {
    M,
    get index() { return index; },
    get section() { return section; },
    step(sdt, named, loud) {
      let type = named;
      if (!type) {
        const asked = sectionOfLoud(loud ?? 0.35);
        if (asked !== candidate) {
          candidate = asked;
          held = 0;
        } else {
          held += sdt;
        }
        type = section === null || (candidate !== section && held >= SECTION_HOLD_S)
          ? candidate : section;
      }
      if (type !== section) {
        if (section !== null) nextMood(type);
        section = type;
      }
      const k = 1 - Math.exp(-sdt * MOOD_RATE);
      const target = MOODS[index];
      for (const key of NUMERIC) M[key] += (target[key] - M[key]) * k;
      M.name = target.name;
    },
  };
}

/** The palette programs, a seed picks one. */
export const PROGRAMS = Object.freeze([
  'complementary split', 'triadic neon', 'iridescent', 'spectral', 'warm/cool duotone',
  'night neon',
]);

const warmth = (h) => Math.cos(TAU * (h - 0.07));

/**
 * The hues of a palette program from the two palette colours' hues `a` and `b` (0-1) at scene
 * time `t`: the sky's horizon, middle, zenith and far side, the sun, the three band hues, the
 * accents, the art pair, a saturation and a dimming.
 */
export function paletteProgram(name, a, b, t = 0) {
  const w0 = warmth(a) >= warmth(b);
  const warm = w0 ? a : b;
  const c0 = w0 ? b : a;
  const cool = Math.abs(fr(c0 - warm + 0.5) - 0.5) < 0.25 ? warm + 0.5 : c0;
  let o;
  switch (name) {
    case 'triadic neon':
      o = { hor: a + 2 / 3, mid: a + 0.85, zen: a + 1 / 3, skyB: a, sun: a, band: [a, a + 1 / 3,
        a + 2 / 3], sat: 1 };
      break;
    case 'iridescent': {
      const s = t * 0.012;
      o = { hor: a + s, mid: a + s + 0.18, zen: a + s + 0.4, skyB: a + s + 0.62, sun: a + s + 0.1,
        band: [a + s, a + s + 0.22, a + s + 0.45], sat: 0.9, fixed: 1 };
      break;
    }
    case 'spectral':
      o = { hor: b, mid: b + 0.1, zen: a + 0.6, skyB: a, sun: 0.08, band: [0.02, 0.3, 0.62],
        sat: 0.95, fixed: 1 };
      break;
    case 'warm/cool duotone':
      o = { hor: cool, mid: cool + 0.04, zen: cool - 0.03, skyB: warm, sun: warm, band: [warm,
        warm + 0.08, cool], sat: 0.85, fixed: 1 };
      break;
    case 'night neon':
      o = { hor: 0.7, mid: 0.75, zen: 0.66, skyB: a, sun: a + 0.5, band: [a, b, b + 0.5], sat: 1,
        dim: 0.62 };
      break;
    default:
      o = { hor: a, mid: a + 0.22, zen: a + 0.5, skyB: a + 0.42, sun: a + 0.03, band: [a, a + 0.42,
        a + 0.58], sat: 0.85 };
  }
  if (!o.fixed) o.band.sort((x, y) => warmth(y) - warmth(x));
  return o;
}

/** Yellow-green hues (0.11 to 0.38) lose most of their saturation in the sky, the fog and the
 * leaves, so no program washes the frame lime. */
const yg = (h) => sstJs(0.09, 0.15, fr(h)) * sstJs(0.4, 0.33, fr(h));

/**
 * The colours of the light for mood numbers `M` and a palette program `p`, each as `[hue,
 * saturation, lightness, scale]` for `Color.setHSL(h, s, l, SRGBColorSpace)` times the scale.
 */
export function colourSpec(M, p) {
  const nn = p.dim || 1;
  const S = p.sat;
  const yk = (h) => 1 - 0.4 * Math.max(0, Math.cos(TAU * (h - 0.19)));
  const sL = (v, h) => Math.min(0.34, v) * nn * yk(h) * (1 - 0.42 * yg(h));
  const sS = (v, h) => Math.min(0.52, v) * (1 - 0.6 * yg(h));
  const sh = M.moon > 0.5 ? p.zen : p.sun;
  return {
    hor: [p.hor, sS(M.hs * 0.95, p.hor), sL(M.hl, p.hor), 1],
    mid: [p.mid, sS(M.hs * 1.2 + 0.1, p.mid), sL(M.hl * 0.75 + M.zl * 0.25, p.mid), 1],
    zen: [p.zen, sS(M.hs * 1.1 + 0.1, p.zen), sL(M.zl * 1.3, p.zen), 1],
    skyB: [p.skyB, sS(M.hs * 1.1, p.skyB), sL(M.hl * 0.8, p.skyB), 1],
    sun: M.moon > 0.5
      ? [sh, 0.2, 0.85, 1]
      : [sh, 0.8 * (1 - 0.55 * yg(sh)), 0.62 - 0.1 * yg(sh), 1],
    light: [p.band[0], S, 0.58, 2.4],
    light2: [p.band[2], 0.5 * S + 0.1, 0.72, 1.2],
    alb: [p.hor, 0.45, 0.6, 1],
    grass: [p.band[1] + 0.02, 0.32 * S, 0.22, 1],
    fol: [p.band[0] + 0.04, 0.4 * S, 0.28, 1],
    ws: [p.band[2], 0.45 * S, 0.2, 0.45],
    wd: [p.hor, 0.5, 0.06, 0.5],
  };
}

/** The sun's direction for the heading `yaw`, the mood's side `az` and height `el`. */
export function sunDirection(yaw, az, el) {
  const ya = yaw + az;
  return [Math.cos(el) * Math.sin(ya), Math.sin(el), -Math.cos(el) * Math.cos(ya)];
}

/** The scene's exposure follows the mood (the post chain's tone reads it). */
export const exposureOf = (M) => M.ex;

/** The depth the dome is drawn at, as a share of the far plane's: behind every fragment of the land
 * up to the far plane (a 24-bit depth at 400 units reads under 1 - 3e-7), in front of the clear. */
const SKY_DEPTH = 0.9999999;

/** The dome's radius (inside the camera's far plane) and its segments. */
const DOME = Object.freeze({ radius: 300, width: 40, height: 20 });

/**
 * The uniforms and TSL nodes every land layer shares, made once for `ctx` (`ctx.env` is the
 * uniforms; the layers read the light only through them and through these nodes).
 */
export function shading(ctx) {
  if (ctx.shading) return ctx.shading;
  const { T, S, lib, u } = ctx;
  const {
    Fn, float, vec2, vec3, uniform, floor, fract, mix, clamp: clampNode, dot, max, normalize, exp,
    pow, cameraPosition,
  } = S;
  const colour = () => uniform(new T.Color());
  const env = {
    sunDir: uniform(new T.Vector3(0, 0.05, -1)),
    sunC: colour(),
    horC: colour(),
    midC: colour(),
    zenC: colour(),
    skyB: colour(),
    ambC: colour(),
    lightC: colour(),
    light2C: colour(),
    albT: uniform(new T.Color(1, 1, 1)),
    grassC: colour(),
    folC: colour(),
    wsC: colour(),
    wdC: colour(),
    sunI: uniform(1),
    glowI: uniform(1),
    fogD: uniform(0.005),
    mist: uniform(0.3),
    mistY: uniform(4),
    cloud: uniform(0),
    stars: uniform(0),
    lit: uniform(0),
    exposure: uniform(1),
  };

  /** Smoothstep from `a` to `b` at `x`, either way round (WGSL leaves a reversed one undefined). */
  const sst = (a, b, x) => {
    const t = clampNode(x.sub(a).div(float(b).sub(a)), 0, 1);
    return t.mul(t).mul(float(3).sub(t.mul(2)));
  };
  const hs2 = Fn(([p]) => {
    const p3 = fract(vec3(p.x, p.y, p.x).mul(0.1031)).toVar();
    p3.addAssign(dot(p3, p3.yzx.add(33.33)));
    return fract(p3.x.add(p3.y).mul(p3.z));
  });
  /** Value noise with its derivatives: `vec3(value, d/dx, d/dy)`. */
  const nd2 = Fn(([x]) => {
    const i = floor(x).toVar();
    const f = fract(x).toVar();
    const q = f.mul(f).mul(f).mul(f.mul(f.mul(6).sub(15)).add(10)).toVar();
    const dq = f.mul(f).mul(30).mul(f.mul(f.sub(2)).add(1)).toVar();
    const a = hs2(i);
    const b = hs2(i.add(vec2(1, 0)));
    const c = hs2(i.add(vec2(0, 1)));
    const d = hs2(i.add(vec2(1, 1)));
    const k = a.sub(b).sub(c).add(d);
    const value = a.add(b.sub(a).mul(q.x)).add(c.sub(a).mul(q.y)).add(k.mul(q.x).mul(q.y));
    const grad = dq.mul(vec2(b.sub(a), c.sub(a)).add(k.mul(q.yx)));
    return vec3(value, grad.x, grad.y);
  });

  /** The colour of the sky in direction `d` (unit). */
  const skyBase = (d) => {
    const sd = max(dot(d, env.sunDir), 0);
    const side = normalize(vec3(env.sunDir.z.negate(), 0, env.sunDir.x).add(1e-4));
    const az = dot(normalize(vec3(d.x, 0, d.z).add(1e-4)), side);
    const hor = mix(env.horC, env.skyB, az.mul(0.35).add(0.35));
    let c = mix(hor, env.midC, sst(0.01, 0.1, d.y));
    c = mix(c, env.zenC, sst(0.07, 0.32, d.y));
    c = mix(c, hor.mul(0.5), sst(0, -0.2, d.y));
    const glow = pow(sd, 5).mul(0.3).add(pow(sd, 48).mul(0.6));
    return c.add(env.sunC.mul(env.sunI).mul(env.glowI).mul(glow));
  };
  const fogCol = (d) => skyBase(vec3(d.x, max(d.y, 0).mul(0.4).add(0.01), d.z)).mul(0.5);

  /** A height layer of mist, patchy, summed along the view ray `V` (length `dist`) in four
   * steps. */
  const mistI = (V, dist) => {
    let ms = float(0);
    for (let i = 0; i < 4; i++) {
      const p = cameraPosition.add(V.mul((i + 0.5) * 0.25));
      const band = exp(pow(p.y.sub(env.mistY).div(2.6), 2).negate());
      const patch = sst(0.3, 0.75, nd2(p.xz.mul(0.016).add(vec2(u.time.mul(0.015), 0))).x);
      ms = ms.add(band.mul(patch));
    }
    return clampNode(float(1).sub(exp(ms.mul(dist).mul(0.0066).mul(env.mist).negate())), 0, 0.8);
  };

  /** Fog, height mist and the sun's haze over `col` seen along `V` (the camera to the point, length
   * `dist`, at height `y`); a changed path thickens the fog (`u.haze`). `mist` is the height mist
   * when it was summed already (a vertex's, its noise being tens of units across). */
  const fog = (col, V, dist, y, mist = null) => {
    const vd = V.div(dist);
    const fh = exp(max(y, 0).mul(-0.04));
    const thick = float(1).add(u.haze.mul(1.5));
    let fa = float(1).sub(exp(dist.mul(env.fogD).mul(thick).mul(fh.mul(0.7).add(0.3)).negate()));
    fa = max(fa, sst(1, 12, dist).mul(0.07));
    const fc = fogCol(vd);
    let c = mix(col, fc, fa);
    const low = clampNode(
      env.mist.mul(exp(max(y, 0).mul(-0.6))).mul(float(1).sub(exp(dist.mul(-0.012)))), 0, 0.7,
    );
    c = mix(c, fc.mul(1.1), low);
    c = c.add(env.sunC.mul(env.sunI).mul(pow(max(dot(vd, env.sunDir), 0), 7)).mul(fa).mul(0.22));
    return mix(c, fc.mul(1.3).add(env.sunC.mul(env.sunI).mul(0.03)), mist ?? mistI(V, dist));
  };

  // the light's numbers are written by one function every land layer's update calls, so a hidden
  // sky leaves the others with the palette, the mood and the heading
  const light = { mood: createMood({ seed: 0 }), yaw: null, last: null, program: PROGRAMS[0] };
  const tint = new T.Color();
  const hsl = { h: 0, s: 0, l: 0 };
  const paint = (target, [h, s, l, k]) => {
    target.setHSL(fr(h), clamp(s), clamp(l), T.SRGBColorSpace);
    if (k !== 1) target.multiplyScalar(k);
  };
  /** Start the light over for `seed`: its first mood, a palette program, the mist's height. */
  const reset = (seed) => {
    light.mood = createMood({ seed });
    light.program = PROGRAMS[Math.floor(mulberry(hashStr(`program:${seed}`))() * PROGRAMS.length)];
    env.mistY.value = 1.8 + mulberry(hashStr(`mist:${seed}`))() * 5;
    light.yaw = null;
    light.last = null;
  };
  /** Write the light for `frame`: the mood by the scene's seconds, the palette's colours, the sun.
   * A second call at the same time changes nothing. */
  const sync = (frame) => {
    const { mood } = light;
    const { u: rig, cam } = frame;
    const sdt = light.last === null ? 0 : Math.max(0, rig.time - light.last);
    light.last = rig.time;
    mood.step(sdt, frame.section ?? null, rig.loud);
    const M = mood.M;
    // the colours: the palette pair's hues through the seed's program
    ctx.u.colA.value.getHSL(hsl, T.SRGBColorSpace);
    const a = hsl.h;
    ctx.u.colB.value.getHSL(hsl, T.SRGBColorSpace);
    const spec = colourSpec(M, paletteProgram(light.program, a, hsl.h, rig.time));
    paint(env.horC.value, spec.hor);
    paint(env.midC.value, spec.mid);
    paint(env.zenC.value, spec.zen);
    paint(env.skyB.value, spec.skyB);
    paint(env.sunC.value, spec.sun);
    paint(env.lightC.value, spec.light);
    paint(env.light2C.value, spec.light2);
    paint(env.grassC.value, spec.grass);
    paint(env.folC.value, spec.fol);
    paint(env.wsC.value, spec.ws);
    paint(env.wdC.value, spec.wd);
    env.ambC.value.copy(env.horC.value).lerp(env.zenC.value, 0.5).multiplyScalar(M.amb);
    paint(tint, spec.alb);
    const luma = 0.2126 * tint.r + 0.7152 * tint.g + 0.0722 * tint.b;
    env.albT.value.copy(tint).multiplyScalar(1 / Math.max(luma, 0.05)).lerp(WHITE(T), 0.55);
    env.sunI.value = Math.min(M.sun * (0.6 + 0.8 * rig.loud), 1.45);
    env.glowI.value = Math.min(M.glow * (0.7 + 0.7 * rig.loud + 0.4 * rig.mid), 1.35);
    env.fogD.value = M.fog;
    env.mist.value = M.mist;
    env.cloud.value = M.cl;
    env.stars.value = M.st;
    env.lit.value = M.lit;
    env.exposure.value = M.ex;
    // the sun follows the river's heading, slowly
    const t = ctx.terrain;
    const target = t ? Math.atan2(t.riverX(cam.z - 150) - t.riverX(cam.z), 150) : 0;
    const follow = 1 - Math.exp(-sdt * 0.1);
    light.yaw = light.yaw === null ? target : light.yaw + (target - light.yaw) * follow;
    const [x, y, z] = sunDirection(light.yaw, M.az, M.el);
    env.sunDir.value.set(x, y, z);
  };

  const shared = { env, sst, hs2, nd2, skyBase, fogCol, mistI, fog, lib, sync, reset };
  ctx.env = env;
  ctx.shading = shared;
  return shared;
}

/** The factory of the layer. */
export const skyLayer = (ctx) => {
  const { T, S, lib, u } = ctx;
  const sh = shading(ctx);
  const { env } = sh;
  const {
    Fn, If, float, vec2, vec3, vec4, normalize, dot, max, mix, sin, atan, asin, length, step, floor,
    fract, positionLocal, cameraProjectionMatrix, modelViewMatrix,
  } = S;

  const object = new T.Group();
  // the budget's sky detail (1, 0.5, 0): the clouds' noise takes four, three or two octaves, a
  // dropped octave standing in at its mean
  const detail = S.uniform(1);
  const clouds = (p0) => {
    const sum = float(0).toVar();
    let p = p0;
    let a = 0.5;
    for (let i = 0; i < 4; i++) {
      const at = p;
      const k = a;
      if (i < 2) sum.addAssign(lib.vn(at).mul(k));
      else {
        If(detail.greaterThan(i === 2 ? 0.25 : 0.75), () => {
          sum.addAssign(lib.vn(at).mul(k));
        }).Else(() => {
          sum.addAssign(k * 0.5);
        });
      }
      p = p.mul(2.02).add(vec3(17.3, 9.1, 4.7));
      a *= 0.5;
    }
    return sum.div(0.9375);
  };
  const material = new T.MeshBasicNodeMaterial();
  material.side = T.BackSide;
  // the sky is drawn after the opaque land, at the far plane, so only the pixels it shows are
  // shaded (a land fragment at the far plane's distance is still in front of it)
  material.depthTest = true;
  material.depthWrite = false;
  const clip = cameraProjectionMatrix.mul(modelViewMatrix.mul(vec4(positionLocal, 1)));
  material.vertexNode = vec4(clip.x, clip.y, clip.w.mul(SKY_DEPTH), clip.w);
  material.colorNode = Fn(() => {
    const d = normalize(positionLocal).toVar();
    const sd = dot(d, env.sunDir);
    const c = sh.skyBase(d).add(env.sunC.mul(env.sunI).mul(sh.sst(0.9993, 0.99965, sd)).mul(4))
      .toVar();
    // clouds drift over the sky, lit from the horizon and the sun (none below the horizon, none
    // in a clear mood: the noise is not drawn where it would be multiplied by nothing)
    const cloudy = float(0).toVar();
    If(d.y.greaterThan(0).and(env.cloud.greaterThan(0)), () => {
      const cp = d.xz.div(d.y.add(0.06)).mul(0.7).add(vec2(0, u.time.mul(-0.012)));
      const cn = clouds(vec3(cp.x, cp.y, u.time.mul(0.01)));
      cloudy.assign(sh.sst(float(0.62).sub(env.cloud.mul(0.3)), float(0.95), cn)
        .mul(env.cloud).mul(sh.sst(0, 0.12, d.y)));
      const cc = env.horC.mul(0.55)
        .add(env.sunC.mul(env.sunI).mul(S.pow(max(sd, 0), 3)).mul(0.5))
        .add(vec3(0.55, 0.62, 0.8).mul(env.lit).mul(0.9));
      c.assign(mix(c, cc, cloudy.mul(0.85)));
    });
    // stars on an angular grid, twinkling, hidden by cloud (only above 0.04 of the horizon, only in
    // a starry mood)
    If(d.y.greaterThan(0.04).and(env.stars.greaterThan(0)), () => {
      const grid = vec2(atan(d.z, d.x), asin(d.y)).mul(220);
      const cell = sh.hs2(floor(grid).add(7));
      const dot2 = sh.sst(0.3, 0.05, length(fract(grid).sub(0.5)));
      const twinkle = sin(u.time.mul(2).add(cell.mul(90))).mul(0.5).add(0.5);
      const star = env.stars.mul(step(0.996, cell)).mul(dot2).mul(twinkle)
        .mul(sh.sst(0.04, 0.25, d.y)).mul(float(1).sub(cloudy));
      c.addAssign(vec3(0.8, 0.86, 1).mul(star));
    });
    // the height mist low over the horizon (none from 0.25 up)
    If(d.y.lessThan(0.25), () => {
      c.assign(mix(c, sh.fogCol(d).mul(1.3).add(env.sunC.mul(env.sunI).mul(0.03)),
        sh.mistI(d.mul(500), float(500)).mul(sh.sst(0.25, 0, d.y))));
    });
    c.addAssign(vec3(0.5, 0.56, 0.75).mul(env.lit).mul(0.05));
    return vec4(lib.frost(c, u.frost).mul(u.fade), 1);
  })();
  const geometry = new T.SphereGeometry(DOME.radius, DOME.width, DOME.height);
  const dome = new T.Mesh(geometry, material);
  dome.frustumCulled = false;
  // after the land, the trees and the bodies (the opaque list's order), before anything blended
  dome.renderOrder = 10;
  object.add(dome);

  return {
    name: 'sky',
    group: 'base',
    object,
    build(seed) {
      sh.reset(seed);
    },
    update(frame) {
      sh.sync(frame);
      detail.value = frame.budget?.sky ?? 1;
      object.position.set(frame.cam.x, frame.cam.y, frame.cam.z);
    },
    dispose() {
      geometry.dispose();
      material.dispose();
    },
  };
};

/** A white colour (the albedo tint is pulled toward it). */
const WHITE = (T) => new T.Color(1, 1, 1);
