/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The post chain, written in TSL on a `RenderPipeline`: the scene pass, a bright pass and two
// blur levels (the second is the halation), an anamorphic streak, and one composite with the
// chromatic split, barrel, heat haze, VHS bleed, datamosh, pixel sort, the 1-bit grade, the
// barcode strips, film grain and the anti-banding dither. Each effect has its own strength
// uniform, so the budget and the bisect switches drop one at a time.
//
// The pure half (no GPU, imported by the node checks): which effects there are, which ones the
// budget turns off first, the director's mix of a section, and the strengths a frame asks of each
// effect. The GPU half, `createPost`, is handed the vendored namespaces by the post layer.
//
//   EFFECTS, GRIT, BLOOM2       the effect names; the grit ones; the bloom's second octave's
//   mixOf(section)              the director's mix of a section: `{ bg, exposure, lift }`
//   eventsOf(ctx, frame, state)  the frame's events, a peak carrying whether it was granted a flash
//   createSurge()               the beacons' surge on a granted peak
//   flashOf(ctx, frame)         the flash gate the accents, the eruptions and the post layer share
//   grooveOf(frame)             the groove a frame carries (the director's), or the one its numbers
//                               give: `{ kick, snare, hat, energy, bar, down, beat, bpm, kn, sn,
//                               section }`
//   createMix()                 `step(dt, section)` -> the mix, eased by time constants
//   levelsOf(input)             the strength of each effect for a frame (see there)
//   createPost(parts)           `{ pipeline, strengths, update(), render(), setBloom2(on),
//                               setHistory(on), setBack(share), setPost(share), dispose() }`
//   BACK_LAYER, PRIME_LAYER     the camera layers of the backdrop and of the quad that lays it under
//                               the rest

import { FlashLimiter } from '../director.js';

/** The post effects, each with a strength uniform of its own. */
export const EFFECTS = Object.freeze([
  'bloom', 'halation', 'anamorphic', 'chroma', 'grain', 'vhs', 'mosh', 'sort', 'barcode', 'dither',
  'barrel', 'haze',
]);

/** What the budget turns off first (its `grit` step), and the bloom's second octave (its next). */
export const GRIT = Object.freeze(['vhs', 'mosh', 'sort', 'barcode']);
export const BLOOM2 = Object.freeze(['halation']);

/** The director's mix by section: the beacons' gain (the gain a section lets the lights run at),
 * the exposure and the bloom's lift. */
const MIX = Object.freeze({
  intro: { bg: 0.55, exposure: 0.92, lift: 0 },
  build: { bg: 0.8, exposure: 1, lift: 0.04 },
  drop: { bg: 1, exposure: 1.1, lift: 0.1 },
  break: { bg: 0.6, exposure: 0.9, lift: 0 },
});

/** The mix of `section` (an unknown one reads as a build). */
export const mixOf = (section) => MIX[section] ?? MIX.build;

/** The groove of a frame: the director's (`frame.director`) when the wire hands it in, else one
 * derived from the rig's numbers (the bass for the kick, the mids for the snare, the highs for the
 * hat, 120 beats a minute from the scene's clock). */
export function grooveOf(frame) {
  const d = frame.director;
  if (d?.groove) {
    const { groove } = d;
    return {
      kick: groove.kick, snare: groove.snare, hat: groove.hat, energy: groove.energy,
      bar: groove.bar, down: groove.down, beat: d.beat, bpm: d.bpm,
      kn: d.hits?.[0] ?? 0, sn: d.hits?.[1] ?? 0, section: frame.section ?? d.section ?? 'build',
    };
  }
  const { u } = frame;
  const beat = ((u.time ?? 0) * 120) / 60;
  const bar = (beat / 4) % 1;
  return {
    kick: clamp((u.bass ?? 0) * 1.2 - 0.3),
    snare: clamp((u.mid ?? 0) * 1.2 - 0.3),
    hat: clamp((u.high ?? 0) * 1.2 - 0.3),
    energy: clamp((u.loud ?? 0.35) * 1.6),
    bar,
    down: Math.exp(-bar * 14),
    beat,
    bpm: 120,
    kn: Math.floor(beat),
    sn: Math.floor(beat / 2),
    section: frame.section ?? 'build',
  };
}

/** The flash gate of a frame, `(t) => granted`: the wire's (`frame.flash`, the director's own
 * limiter) when it hands one in, else one limiter the layers of this scene share (`ctx.fx`), so
 * the accents and the eruptions together never pass the limiter's count. */
export function flashOf(ctx, frame) {
  if (typeof frame.flash === 'function') return frame.flash;
  ctx.fx ??= { limiter: new FlashLimiter() };
  return (t) => ctx.fx.limiter.grant(t);
}

/** The director's events of a frame, each peak carrying whether a flash was granted for it. With
 * the director's events (`frame.events`) the grant is the director's own (`e.flash`); without
 * them a peak is made from a rise of the rig's peak number and asks the frame's gate once (the
 * answer is kept for the other layers of the same frame). */
export function eventsOf(ctx, frame, state) {
  if (Array.isArray(frame.events)) return frame.events;
  const peak = frame.u?.peak ?? 0;
  const rose = peak > 0.6 && peak > state.peak + 0.2;
  state.peak = peak;
  if (!rose) return [];
  ctx.fx ??= { limiter: new FlashLimiter() };
  const memo = ctx.fx.peaks;
  if (memo && memo.t === frame.t && memo.peak === peak) return memo.events;
  const events = [{ type: 'peak', flash: flashOf(ctx, frame)(frame.t) }];
  ctx.fx.peaks = { t: frame.t, peak, events };
  return events;
}

/** The surge of every beacon on a peak that was granted a flash: 0.8 at the
 * peak, decaying 1.3 a second, never in Calm. `step(dt, events, calm)` returns its level. */
export function createSurge() {
  const state = { level: 0 };
  return {
    state,
    reset() {
      state.level = 0;
    },
    step(dt, events, calm) {
      state.level *= Math.exp(-dt * 1.3);
      if (calm) state.level = 0;
      else if (events.some((e) => e.type === 'peak' && e.flash === true)) state.level = 0.8;
      return state.level;
    },
  };
}

/** How fast the mix eases toward a section's, per second. */
const MIX_RATE = 0.8;

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const ease = (dt, rate) => 1 - Math.exp(-dt * rate);

/** The mix, eased: a section's change moves it over a second or two, never at once. */
export function createMix() {
  const state = { ...MIX.intro };
  return {
    state,
    /** Back to the first section's mix (a seed's start: a frame reads nothing carried over). */
    reset() {
      Object.assign(state, MIX.intro);
    },
    step(dt, section) {
      const to = mixOf(section);
      const k = ease(dt, MIX_RATE);
      for (const key of Object.keys(state)) state[key] += (to[key] - state[key]) * k;
      return state;
    },
  };
}

/**
 * The strength of each effect for a frame. `input`: `u` (the rig's numbers), `groove` (`kick`,
 * `energy`), `mix` (the director's, eased), `accents` (`barcode`, `mosh`, `vhs`, `sort`, `flare`,
 * `ca`: the accents' levels), `signal` (`vhs`, `block`, `sort`: the weak link's and the link drop's
 * slow levels), `budget` (`grit`, `bloom2`), `calm` (a level below Full) and `off` (the bisect set
 * of effect names). Returns `{ strengths, exposure }`. Every strength is 0 for an effect that is
 * off, and the accents' are 0 in Calm.
 */
export function levelsOf({
  u, groove, mix, accents, signal, budget, calm = false, off = new Set(),
}) {
  const g = mix.bg * (0.45 + 0.75 * groove.energy);
  const loud = u.loud ?? 0.35;
  const bass = u.bass ?? 0.3;
  const quiet = calm ? 0.4 : 1;
  const accent = calm ? 0 : 1;
  const strengths = {
    bloom: 0.8 + mix.lift + 0.22 * groove.kick * groove.energy * quiet,
    halation: budget.bloom2 ? (0.25 + 0.5 * g * bass) : 0,
    anamorphic: 0.25 + 0.45 * g + accents.flare * accent,
    chroma: (0.2 + 0.25 * g + 1.1 * accents.ca * accent) * quiet,
    grain: 1,
    vhs: Math.max(signal.vhs, accents.vhs) * accent,
    mosh: Math.max(signal.block, accents.mosh) * accent,
    sort: Math.max(signal.sort, accents.sort) * accent,
    barcode: accents.barcode * accent,
    dither: clamp((u.frost ?? 0) * 0.6, 0, 0.72),
    barrel: 0.07 * g * bass * quiet,
    haze: clamp(u.haze ?? 0),
  };
  for (const name of GRIT) if (!budget.grit) strengths[name] = 0;
  for (const name of off) if (name in strengths) strengths[name] = 0;
  const exposure = mix.exposure * (0.9 + 0.25 * loud)
    * (1 + (0.05 * (groove.down ?? 0) + 0.04 * groove.kick) * groove.energy * quiet);
  return { strengths, exposure };
}

/** The camera layer the backdrop's objects are on (the sky, the land, the river, the trees: drawn at
 * the budget's share of the canvas's resolution), and the one of the quad that lays the backdrop
 * (its colour and its depth) under the rest, which is drawn at the canvas's own (layer 0). */
export const BACK_LAYER = 1;
export const PRIME_LAYER = 2;

/** The scene's bright pass threshold (linear), the blur's spread and the anamorphic taps. */
const THRESHOLD = 0.9;
const SPREAD = 1.5;
const STREAK = [2.5, 6, 14];

/**
 * The chain on `renderer` (`gpu.renderer`), for `scene` seen by `camera`, reading the uniform
 * block `u`. `T` and `S` are the vendored namespaces and `lib` the shared shading functions.
 */
export function createPost({
  T, S, lib, u, scene, camera, renderer,
}) {
  const {
    vec2, vec3, vec4, float, uniform, mix, clamp: sclamp, max, abs, floor, fract, step, smoothstep,
    sin, dot, mod, select, If, Loop, uv, screenUV, screenCoordinate,
  } = S;
  const strengths = Object.fromEntries(EFFECTS.map((name) => [name, uniform(0)]));
  const exposure = uniform(1);
  const px = uniform(1);
  const barSeed = uniform(0);
  const res = uniform(new T.Vector2(1, 1));
  const texel = [0, 1, 2].map(() => uniform(new T.Vector2(1, 1)));
  const [t0, t1, t2] = texel;

  // the backdrop at its share of the resolution, then the rest at the canvas's over a quad that lays
  // the backdrop's colour and depth under it (the hero, the beacons and the particles crisp, hidden
  // by the land where it is in front); at a share of 1 one pass draws everything
  const backLayers = new T.Layers();
  backLayers.set(BACK_LAYER);
  const frontLayers = new T.Layers();
  frontLayers.set(0);
  frontLayers.enable(PRIME_LAYER);
  const allLayers = new T.Layers();
  allLayers.set(0);
  allLayers.enable(BACK_LAYER);
  // drawn before the chain (a pass a material of the scene reads would be drawn inside the scene's
  // own render)
  const backTarget = new T.RenderTarget(1, 1, { type: T.HalfFloatType });
  backTarget.depthTexture = new T.DepthTexture(1, 1);
  let backShare = 1;
  const scenePass = S.pass(scene, camera);
  scenePass.setLayers(allLayers);
  const sceneTex = scenePass.getTextureNode();
  const primeMaterial = new T.MeshBasicNodeMaterial();
  primeMaterial.vertexNode = vec4(S.positionGeometry.xy, 0, 1);
  primeMaterial.colorNode = vec4(S.texture(backTarget.texture, screenUV).level(0).rgb, 1);
  primeMaterial.depthNode = S.texture(backTarget.depthTexture, screenUV).level(0).x;
  primeMaterial.depthTest = true;
  primeMaterial.depthWrite = true;
  primeMaterial.depthFunc = T.AlwaysDepth;
  const prime = new T.Mesh(new T.PlaneGeometry(2, 2), primeMaterial);
  prime.layers.set(PRIME_LAYER);
  prime.frustumCulled = false;
  prime.renderOrder = -1000;
  prime.visible = false;
  scene.add(prime);
  let split = false;
  const at = (tex, p) => tex.sample(p).level(0);

  const rtt = (node, scale) => S.rtt(node, null, null, { resolutionScale: scale });
  const blur = (tex, dir) => {
    const p = uv();
    const side = (k) => at(tex, p.add(dir.mul(k))).rgb.add(at(tex, p.sub(dir.mul(k))).rgb);
    const c = at(tex, p).rgb.mul(0.227027027)
      .add(side(1.3846153846).mul(0.3162162162))
      .add(side(3.2307692308).mul(0.0702702703));
    return vec4(c, 1);
  };
  const bright = rtt(S.Fn(() => {
    const p = uv();
    const c = at(sceneTex, p.sub(t0)).rgb
      .add(at(sceneTex, p.add(t0.mul(vec2(1, -1)))).rgb)
      .add(at(sceneTex, p.add(t0.mul(vec2(-1, 1)))).rgb)
      .add(at(sceneTex, p.add(t0)).rgb).mul(0.25);
    const l = max(c.r, max(c.g, c.b));
    return vec4(c.mul(max(l.sub(THRESHOLD), 0).div(max(l, 1e-4))), 1);
  })(), 0.5);
  const dirX = (t, k = SPREAD) => vec2(t.x.mul(k), 0);
  const dirY = (t, k = SPREAD) => vec2(0, t.y.mul(k));
  const b1h = rtt(blur(bright, dirX(t1)), 0.5);
  const b1 = rtt(blur(b1h, dirY(t1)), 0.5);
  const copy = rtt(vec4(at(b1, uv()).rgb, 1), 0.25);
  const b2h = rtt(blur(copy, dirX(t2)), 0.25);
  const b2 = rtt(blur(b2h, dirY(t2)), 0.25);
  const a1 = rtt(blur(bright, dirX(t2, STREAK[0])), 0.25);
  const a2 = rtt(blur(a1, dirX(t2, STREAK[1])), 0.25);
  const an = rtt(blur(a2, dirX(t2, STREAK[2])), 0.25);
  const second = [copy, b2h, b2];
  // Each pass runs once per render of the chain. three runs a RENDER-kind node once per render id,
  // and every pass is itself a render (a new id), so left alone each pass would run again inside the
  // ones after it. A pass is let through once until `render()` clears the set: one draw of the chain
  // shows the scene once, and two draws in one task (a probe, a resize) each show it as it then stands.
  const ran = new Set();
  for (const node of [scenePass, bright, b1h, b1, ...second, a1, a2, an]) {
    node.updateBeforeType = T.NodeUpdateType.RENDER;
    const run = node.updateBefore.bind(node);
    node.updateBefore = (frame) => {
      if (ran.has(node)) return undefined;
      ran.add(node);
      return run(frame);
    };
  }

  // the previous scene, for the datamosh: a copy of the scene pass kept one frame back, at half size
  const prev = new T.RenderTarget(1, 1, { type: T.HalfFloatType });
  // the bloom's share of its resolution (the budget's post step: 1, or 0.5 for its levels drawn at
  // a quarter and an eighth of the frame)
  const half = [bright, b1h, b1];
  const quarter = [copy, b2h, b2, a1, a2, an];
  let bloomShare = 1;
  const prevTex = S.texture(prev.texture);
  const copyMaterial = new T.NodeMaterial();
  copyMaterial.fragmentNode = vec4(at(sceneTex, screenUV).rgb, 1);
  const copyQuad = new T.QuadMesh(copyMaterial);

  const lum = (c) => dot(c, vec3(0.2126, 0.7152, 0.0722));
  const aces = (x) => S.clamp(
    x.mul(x.mul(2.51).add(0.03)).div(x.mul(x.mul(2.43).add(0.59)).add(0.14)), 0, 1,
  );
  const tone = (c) => aces(c.mul(exposure));
  const encode = (c) => S.pow(c, vec3(0.4545));
  const bay2 = (p) => mod(p.x.mul(2).add(p.y.mul(3)), 4);
  const bayer4 = (p0) => {
    const p = mod(floor(p0), 4);
    return bay2(mod(p, 2)).mul(4).add(bay2(floor(p.div(2)))).div(16).add(1 / 32);
  };
  const st = strengths;

  const composite = S.Fn(() => {
    const asp = vec2(res.x.div(res.y), 1);
    const cc0 = screenUV.sub(0.5).mul(asp);
    const r2 = dot(cc0, cc0);
    const frag = screenCoordinate.xy;
    const gt = u.gt;
    // the warps of the picture's coordinates, one after the other; an effect at rest draws nothing
    // (each is a no-op at a strength of 0, so its taps and noise are not drawn then)
    const p = screenUV.sub(0.5).mul(float(1).sub(st.barrel.mul(r2))).add(0.5).toVar();
    If(st.haze.greaterThan(0), () => {
      const hz = vec2(
        lib.vn(vec3(p.mul(vec2(9, 22)), u.time.mul(1.4))),
        lib.vn(vec3(p.mul(vec2(9, 22)).add(5), u.time.mul(1.4))),
      );
      p.addAssign(hz.sub(0.5).mul(0.012).mul(st.haze));
    });
    If(st.vhs.greaterThan(0), () => {
      const wobble = lib.vn(vec3(p.y.mul(40), gt.mul(3), 1)).sub(0.5).mul(0.006).mul(st.vhs)
        .add(step(0.985, lib.h11(floor(p.y.mul(res.y).div(3)).add(floor(gt.mul(20)))))
          .mul(0.01).mul(st.vhs).mul(sin(p.y.mul(3).add(u.time.mul(1.3))).mul(0.5).add(0.5)));
      p.assign(vec2(p.x.add(wobble), p.y));
    });
    If(u.tear.greaterThan(0), () => {
      const band = float(1).sub(step(
        lib.h11(floor(gt.mul(9)).add(2)).mul(0.03).add(0.035),
        abs(p.y.sub(u.tearY.mul(0.5).add(0.5))),
      ));
      const shift = band.mul(u.tear).mul(lib.h11(floor(gt.mul(14))).sub(0.35)).mul(0.11);
      p.assign(vec2(p.x.add(shift), p.y));
    });
    const cell = px.mul(16);
    const bh = lib.h13(vec3(floor(frag.div(cell)), floor(gt.mul(12))));
    If(st.mosh.greaterThan(0), () => {
      const blockPx = px.mul(8);
      const snapped = floor(p.mul(res).div(blockPx)).add(0.5).mul(blockPx).div(res);
      p.assign(mix(p, snapped, st.mosh.mul(step(0.6, bh))));
    });
    const uvw = p;
    // the chromatic split
    const dir = cc0.div(r2.sqrt().add(1e-4)).div(asp);
    const ca = st.chroma.mul(r2.mul(1.6).add(0.15)).mul(0.006);
    const c = vec3(
      at(sceneTex, uvw.add(dir.mul(ca))).r,
      at(sceneTex, uvw).g,
      at(sceneTex, uvw.sub(dir.mul(ca))).b,
    ).toVar();
    // the pixel sort: bright pixels smear along a column
    If(st.sort.greaterThan(0.005), () => {
      const cx = floor(frag.x.div(px.mul(3)));
      const gx = floor(frag.x.div(px.mul(27)));
      const sd = floor(barSeed.mul(7));
      const y0 = lib.h11(gx.mul(1.37).add(sd).add(3));
      const act = step(float(1).sub(st.sort.mul(0.6)), lib.h11(gx.mul(0.731).add(sd)))
        .mul(step(0.25, lib.h11(cx.mul(0.37).add(sd).add(0.5))))
        .mul(step(y0, screenUV.y))
        .mul(step(screenUV.y, y0.add(0.25).add(lib.h11(gx.mul(2.1).add(sd)).mul(0.5))));
      If(act.greaterThan(0), () => {
        const best = c.toVar();
        const bestL = lum(c).toVar();
        const reach = st.sort.mul(0.22).mul(lib.h11(cx.add(sd)).mul(0.6).add(0.4));
        Loop(19, ({ i }) => {
          const f = float(i).add(1).div(19);
          const s = at(sceneTex, vec2(uvw.x, uvw.y.add(f.mul(reach)))).rgb;
          const sl = lum(s).mul(float(1).sub(f.mul(0.5)));
          If(sl.greaterThan(bestL), () => {
            bestL.assign(sl);
            best.assign(s.mul(float(1).sub(f.mul(0.4))));
          });
        });
        c.assign(mix(c, best, smoothstep(0.05, 0.25, bestL)));
      });
    });
    // the light: bloom, halation tinted toward warm, the anamorphic streak in the palette's light
    const anaC = mix(u.colA, vec3(1, 1, 1), 0.55);
    c.addAssign(at(b1, uvw).rgb.mul(st.bloom));
    If(st.halation.greaterThan(0), () => {
      c.addAssign(at(b2, uvw).rgb.mul(st.halation).mul(vec3(1, 0.5, 0.35)));
    });
    c.addAssign(at(an, uvw).rgb.mul(st.anamorphic).mul(anaC));
    // from here on the picture is display-referred: the tone's output is encoded, and the steps
    // that follow (the S-curve, scanlines, the grade, the grain and the dither) work on the
    // encoded values, one 8-bit step being what they are sized in
    const toned = encode(tone(c)).toVar();
    toned.assign(mix(toned, toned.mul(toned).mul(float(3).sub(toned.mul(2))), 0.3));
    // VHS: the colour bleeds sideways and the scanlines darken
    If(st.vhs.greaterThan(0), () => {
      const bleed = encode(tone(at(sceneTex, uvw.sub(vec2(st.vhs.mul(0.006), 0))).rgb));
      toned.assign(mix(
        toned, vec3(lum(toned)).add(bleed.sub(lum(bleed))), st.vhs.mul(0.7),
      ));
      toned.mulAssign(float(1).sub(st.vhs.mul(0.2).mul(step(0.5, fract(frag.y.div(px.mul(2)))))));
    });
    // the datamosh: blocks hold the previous picture, shifted
    If(st.mosh.greaterThan(0), () => {
      const mv = vec2(lib.h11(bh.mul(7.1)), lib.h11(bh.mul(13.3))).sub(0.5).mul(0.05);
      const held = encode(tone(at(prevTex, screenUV.add(mv)).rgb));
      toned.assign(mix(toned, held, st.mosh.mul(step(0.55, bh)).mul(0.85)));
      toned.assign(mix(
        toned,
        floor(toned.mul(5).add(0.5)).div(5),
        st.mosh.mul(step(0.6, bh)).mul(0.5),
      ));
    });
    // the 1-bit grade of a muted scene: ink and paper through a Bayer matrix
    If(st.dither.greaterThan(0), () => {
      const ink = u.colB.mul(0.06);
      const paper = mix(u.colA, vec3(1, 1, 1), 0.7);
      toned.assign(mix(
        toned,
        mix(ink, paper, step(bayer4(frag.div(px)), lum(toned).mul(1.1).add(0.02))),
        st.dither,
      ));
    });
    // the barcode strips at the two sides
    If(st.barcode.greaterThan(0), () => {
      const sw = floor(px.mul(46));
      const ex = S.min(frag.x, res.x.sub(frag.x));
      const sdb = floor(barSeed.mul(13));
      const blk = floor(frag.y.div(res.y.mul(0.06)));
      const w = float(1).add(floor(lib.h11(floor(frag.x.div(px.mul(8))).add(sdb)).mul(3)));
      const bit = step(0.5, lib.h11(floor(frag.x.div(w.mul(px))).mul(0.613).add(sdb.mul(1.7))
        .add(blk.mul(0.1))));
      const half = step(res.x.mul(0.5), frag.x).mul(5);
      const on = step(0.35, lib.h11(blk.mul(0.91).add(sdb).add(half)))
        .mul(0.85).mul(smoothstep(sw, sw.mul(0.7), ex)).mul(step(ex, sw));
      toned.assign(mix(toned, vec3(bit), st.barcode.mul(on)));
    });
    // the vignette, the film grain and the anti-banding dither of about one 8-bit step
    const q2 = screenUV.sub(0.5);
    toned.mulAssign(float(1).sub(dot(q2, q2).mul(0.5)));
    const grain = lib.h13(vec3(frag, fract(gt).mul(61))).sub(0.5)
      .mul(mix(float(0.026), float(0.018), u.grain)).mul(u.flat.mul(0.8).add(0.6)).mul(st.grain);
    const dq = frag.add(fract(gt.mul(7.31)).mul(vec2(47, 17)));
    const band8 = fract(fract(dot(dq, vec2(0.06711056, 0.00583715))).mul(52.9829189))
      .add(fract(fract(dot(dq.yx.add(5), vec2(0.06711056, 0.00583715))).mul(52.9829189)))
      .sub(1).mul(1.3 / 255);
    return vec4(sclamp(toned.add(grain).add(band8), 0, 1), 1);
  });

  const pipeline = new T.RenderPipeline(renderer, composite());
  // the chain encodes its own output (see the tone), so the pipeline adds no transform
  pipeline.outputColorTransform = false;
  const size = new T.Vector2();
  let history = true;

  /** The sizes the chain reads (the drawing buffer's), and the previous scene's target. */
  function resize() {
    renderer.getDrawingBufferSize(size);
    const w = Math.max(2, size.x);
    const h = Math.max(2, size.y);
    res.value.set(w, h);
    t0.value.set(1 / w, 1 / h);
    t1.value.set(2 / (w * bloomShare), 2 / (h * bloomShare));
    t2.value.set(4 / (w * bloomShare), 4 / (h * bloomShare));
    px.value = Math.max(0.7, h / 800);
    // the previous scene at half the size: the datamosh holds blocks of 8 px of it, shifted
    const pw = Math.ceil(w / 2);
    const ph = Math.ceil(h / 2);
    if (prev.width !== pw || prev.height !== ph) prev.setSize(pw, ph);
  }

  return {
    pipeline,
    strengths,
    uniforms: { exposure, barSeed, px, res },
    /** Move the uniforms to a frame's: `levels` from `levelsOf`, the barcode's seed. */
    update(levels, seed) {
      resize();
      for (const name of EFFECTS) strengths[name].value = levels.strengths[name];
      exposure.value = levels.exposure;
      barSeed.value = seed;
    },
    /** Draw the chain to the canvas and keep the scene for the datamosh when it is on. */
    render() {
      ran.clear();
      if (split) {
        // the backdrop at its share of the drawing buffer, into the target the quad reads
        const bw = Math.max(1, Math.floor(size.x * backShare));
        const bh = Math.max(1, Math.floor(size.y * backShare));
        if (backTarget.width !== bw || backTarget.height !== bh) backTarget.setSize(bw, bh);
        const mask = camera.layers.mask;
        camera.layers.mask = backLayers.mask;
        renderer.setRenderTarget(backTarget);
        renderer.render(scene, camera);
        renderer.setRenderTarget(null);
        camera.layers.mask = mask;
      }
      pipeline.render();
      if (history) {
        renderer.setRenderTarget(prev);
        copyQuad.render(renderer);
        renderer.setRenderTarget(null);
      }
    },
    /** Whether the previous picture is kept each frame (the datamosh may start on any frame; off
     * when the effect is off). */
    setHistory(on) {
      history = on;
    },
    /** The bloom's share of its resolution (1, or 0.5: its passes drawn at half their size). */
    setPost(share) {
      if (share === bloomShare) return;
      bloomShare = share;
      for (const node of half) node.setResolutionScale(0.5 * share);
      for (const node of quarter) node.setResolutionScale(0.25 * share);
    },
    /** The backdrop's share of the canvas's resolution (1: one pass draws everything). */
    setBack(share) {
      const next = share < 0.999;
      if (next) backShare = share;
      if (next === split) return;
      split = next;
      prime.visible = split;
      scenePass.setLayers(split ? frontLayers : allLayers);
    },
    /** The bloom's second octave: its passes stop drawing when it is off. */
    setBloom2(on) {
      for (const node of second) node.autoUpdate = on;
    },
    dispose() {
      pipeline.dispose?.();
      prev.dispose();
      copyMaterial.dispose();
      for (const node of [scenePass, bright, b1h, b1, ...second, a1, a2, an]) node.dispose?.();
      backTarget.depthTexture.dispose();
      backTarget.dispose();
      scene.remove(prime);
      prime.geometry.dispose();
      primeMaterial.dispose();
    },
  };
}
