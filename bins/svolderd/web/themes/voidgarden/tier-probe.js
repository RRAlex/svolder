/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tier a browser draws the scene on: the faster of WebGPU and WebGL2 on this browser, this GPU
// and this size class, measured once and kept. On a visit with no kept pick for its key (the
// browser's own words with their version, the GPU, the size class) both tiers draw the scene for a
// moment each on a canvas laid over the scene's, at the budget's starting step and at the rig's
// fade of 0 (black, the same cost: the fade is each layer's last factor), and the one that drew
// more frames a second is kept. The `Renderer` row (Auto, WebGPU, WebGL2) or `?vg-gpu=` chooses
// instead. A browser driven by a check (`navigator.webdriver`) measures only when the address asks
// (`?vg-probe`): the checks' cells keep the tier their engine gives.
//
//   RENDERERS, PROBE                the row's values; the probe's numbers
//   rendererOf(prefs, search)       'auto', 'webgpu' or 'webgl2': the address's `vg-gpu`, else the pref
//   sizeClass(w, h, density)        the backing megapixels in half octaves
//   pickKey({ ua, gpu, w, h, density })   the key a kept pick carries
//   keptPick(kept, key)             the kept pick's tier when it is of this key, else null
//   keepPick(kept, pick)            the kept picks with `pick` in: the last four keys
//   fastest(fps)                    the faster tier of `{ webgpu, webgl2 }` (draws a second)
//   decide({ renderer, kept, key, hasAdapter, automated, asked })   `{ tier, probe }`
//   pickTier(parts)                 the whole: `{ tier, why, pick }` (pick: what was measured)

import { baseScale } from './budget.js';
import { createDirector } from './director.js';
import { Features } from './features.js';
import { createRig } from './motion.js';
import { palette as paletteOf } from './palette.js';

/** The `Renderer` row's values. */
export const RENDERERS = Object.freeze(['auto', 'webgpu', 'webgl2']);

/** The probe's numbers: the kept picks' format and how many keys are kept, the seconds each tier
 * draws before it is counted (its first frames) and while it is counted, the gap between two display
 * frames (ms) past which, and past three times the usual gap, is a stall (a compile, a collection, a
 * tab switch; one in a count disturbs it, more are the tier's own rhythm), the share by
 * which WebGL2 must be faster to win (a tie keeps WebGPU, the tier with the larger particle counts),
 * and how many display frames apart a tier the address slows (`vg-probe=slow-<tier>`, for the
 * checks) draws. */
export const PROBE = Object.freeze({
  version: 2, keep: 4, warmS: 0.5, countS: 1.5, gapMs: 250, stalls: 1, tie: 0.05, slowEvery: 6,
});

/** 'auto', 'webgpu' or 'webgl2': `?vg-gpu=` first, else the `renderer` pref. */
export function rendererOf(prefs, search = '') {
  const asked = new URLSearchParams(search).get('vg-gpu');
  if (RENDERERS.includes(asked)) return asked;
  const kept = prefs?.get?.('renderer', 'auto');
  return RENDERERS.includes(kept) ? kept : 'auto';
}

/** The backing megapixels of a canvas `w` by `h` CSS px at `density` (the budget's top step), in
 * half octaves: 960x540 is -2, 1728x972 1, 1080p 2, 4K (3.7 MP) 4. The size a tier's cost
 * follows. */
export function sizeClass(w, h, density) {
  const base = baseScale(w, h, density);
  const mp = Math.max(1 / 64, (w * h * base * base) / 1e6);
  return Math.round(2 * Math.log2(mp));
}

/** The key a kept pick carries: the browser's words (its version in them), the GPU, the size
 * class. */
export function pickKey({
  ua = '', gpu = '', w = 1, h = 1, density = 1,
}) {
  return `${ua}|${gpu}|${sizeClass(w, h, density)}`;
}

/** The tier of `kept` (`{ v, picks: [{ key, tier, fps }] }`) when it holds one of this key, else
 * null. */
export function keptPick(kept, key) {
  if (!kept || kept.v !== PROBE.version || !Array.isArray(kept.picks)) return null;
  const found = kept.picks.find((p) => p?.key === key);
  return found?.tier === 'webgpu' || found?.tier === 'webgl2' ? found.tier : null;
}

/** `kept` with `pick` (`{ key, tier, fps }`) in it, last: the picks of the last `PROBE.keep` keys
 * (a window that crosses a size class's edge does not measure again each time it flips). */
export function keepPick(kept, pick) {
  const old = kept?.v === PROBE.version && Array.isArray(kept.picks) ? kept.picks : [];
  const picks = [...old.filter((p) => p?.key !== pick.key), pick].slice(-PROBE.keep);
  return { v: PROBE.version, picks };
}

/** The faster of `{ webgpu, webgl2 }` (draws a second, null where the tier drew nothing): WebGL2
 * only when it is faster by more than the tie's share. Null when neither drew. */
export function fastest({ webgpu = null, webgl2 = null }) {
  const gpu = Number.isFinite(webgpu) && webgpu > 0 ? webgpu : null;
  const gl = Number.isFinite(webgl2) && webgl2 > 0 ? webgl2 : null;
  if (gpu === null && gl === null) return null;
  if (gpu === null) return 'webgl2';
  if (gl === null) return 'webgpu';
  return gl > gpu * (1 + PROBE.tie) ? 'webgl2' : 'webgpu';
}

/** What the visit draws on: `{ tier, probe }` (tier null while `probe` is to measure; 'webgpu' is
 * WebGPU where the browser gives an adapter, the renderer falling to WebGL2 by itself, with its
 * reason, where none). A chosen renderer wins; no adapter needs no measure; a kept pick of this key
 * stands; a check's browser that did not ask keeps WebGPU. */
export function decide({
  renderer = 'auto', kept = null, key = '', hasAdapter = true, automated = false, asked = false,
}) {
  if (renderer === 'webgl2' || renderer === 'webgpu') return { tier: renderer, probe: false };
  if (!hasAdapter) return { tier: 'webgpu', probe: false };
  const pick = keptPick(kept, key);
  if (pick) return { tier: pick, probe: false };
  if (automated && !asked) return { tier: 'webgpu', probe: false };
  return { tier: null, probe: true };
}

/** The GPU's words for the key: the WebGL renderer's (unmasked where the browser gives it) and the
 * WebGPU adapter's vendor and architecture; empty when nothing says. */
function gpuWords(doc, adapter) {
  let gl = '';
  try {
    const g = doc.createElement('canvas').getContext('webgl2');
    if (g) {
      const ext = g.getExtension('WEBGL_debug_renderer_info');
      gl = String(g.getParameter(ext ? ext.UNMASKED_RENDERER_WEBGL : g.RENDERER) ?? '');
      g.getExtension('WEBGL_lose_context')?.loseContext();
    }
  } catch {
    gl = '';
  }
  const info = adapter?.info;
  return [gl, info?.vendor, info?.architecture].filter(Boolean).join(' ');
}

/** What a frame of the probe hands the layers at `t` seconds: the seed's rig flown from rest, no
 * music, the budget's levels and the fade at 0 (the picture black, the cost whole). */
function probeFrames(seed, levels) {
  const rig = createRig({ seed });
  const director = createDirector();
  director.state.seed = seed;
  const values = new Features().values;
  const pal = paletteOf(null, { title: `seed ${seed}`, artist: '' });
  return (t, dt, aspect, terrain) => {
    rig.step(dt, {
      motion: 'full', features: values, cues: {}, director: director.state, events: [], aspect, terrain,
    });
    return {
      t,
      dt,
      cam: rig.cam,
      u: { ...rig.u, fade: 0 },
      budget: levels,
      palette: pal,
      seed,
      events: [],
      director: director.state,
      section: director.state.section,
      cues: {},
      flash: () => false,
    };
  };
}

/**
 * One tier's draws a second: a canvas laid over `host` (the scene's), the tier's renderer and
 * layers on it at `size()` (`{ w, h, css: { width, height } }`), compiled, `PROBE.warmS` of frames
 * then `PROBE.countS` counted, everything let go after. Null when the tier gives no renderer of
 * its kind or fails. `every` draws on every n-th display frame (a slowed tier, for the checks).
 * `state.clean` goes false when the page was hidden, or two display frames were over
 * `PROBE.gapMs` apart, during the draws: the number is then not the tier's.
 */
async function measure({
  tier, host, createGpu, makeLayers, load, size, levels, seed, every = 1, signal = null,
  state = { clean: true }, guard = true,
}) {
  const doc = host.ownerDocument;
  const win = doc.defaultView;
  const probe = doc.createElement('canvas');
  probe.id = 'vg-probe';
  probe.setAttribute('aria-hidden', 'true');
  Object.assign(probe.style, {
    position: 'absolute', inset: '0', width: '100%', height: '100%', display: 'block',
  });
  host.after(probe);
  let gpu = null;
  let layers = null;
  try {
    gpu = await createGpu({ canvas: probe, forceWebGL: tier === 'webgl2', load });
    if (gpu.tier !== tier) return null;
    const { w, h, css } = size();
    gpu.size(w, h);
    layers = makeLayers(gpu, { seed });
    const aspect = css.width / Math.max(1, css.height);
    const frameOf = probeFrames(seed, levels);
    layers.update(frameOf(0, 0, aspect, layers.ctx.terrain ?? undefined));
    await gpu.compile?.(layers.ctx.scene, layers.ctx.camera);
    const disturb = () => {
      if (doc.hidden) state.clean = false;
    };
    doc.addEventListener?.('visibilitychange', disturb);
    const fps = await new Promise((resolve) => {
      let first = null;
      let from = null;
      let prev = null;
      let stalls = 0;
      const gaps = [];
      let drawn = 0;
      let k = 0;
      let t = 0;
      const tick = (ms) => {
        if (signal?.aborted) {
          resolve(null);
          return;
        }
        disturb();
        if (prev !== null && from !== null && gaps.length >= 3 && ms - prev > PROBE.gapMs
          && ms - prev > 3 * [...gaps].sort((x, y) => x - y)[gaps.length >> 1]) stalls++;
        if (prev !== null) gaps.push(ms - prev);
        prev = ms;
        first ??= ms;
        const s = (ms - first) / 1000;
        if (k++ % every === 0) {
          const dt = Math.min(0.05, s - t);
          t = s;
          layers.update(frameOf(t, dt, aspect, layers.ctx.terrain ?? undefined));
          layers.draw();
          if (from !== null) drawn++;
        }
        if (from === null && s >= PROBE.warmS) from = ms;
        if (from !== null && ms - from >= PROBE.countS * 1000) {
          // one stall is a disturbance; a tier that stalls in rhythm (each heavy frame) is simply
          // slow, and measured as it is; the checks' slowed probe (`vg-probe=slow-`) does not
          // guard: software renderers stall on their own
          if (guard && stalls > 0 && stalls <= PROBE.stalls) state.clean = false;
          resolve((1000 * drawn) / (ms - from));
          return;
        }
        win.requestAnimationFrame(tick);
      };
      win.requestAnimationFrame(tick);
    });
    // three frames with no draw before the canvas goes, its last frame shown: a WebGPU canvas
    // removed right after its last draw left Firefox with no animation frame for the page ever
    // again (3.11: at 4K, the WebGL2 probe after it never drew)
    doc.removeEventListener?.('visibilitychange', disturb);
    for (let i = 0; i < 3; i++) await new Promise((resolve) => { win.requestAnimationFrame(resolve); });
    return fps;
  } catch {
    return null;
  } finally {
    layers?.dispose();
    // the WebGL2 probe's context is its own: three loses only a canvas it made itself
    try {
      if (tier === 'webgl2') gpu?.renderer?.backend?.gl?.getExtension?.('WEBGL_lose_context')?.loseContext?.();
    } catch {
      // already lost
    }
    gpu?.dispose?.();
    probe.remove();
  }
}

/**
 * The tier this visit draws on. `parts`: `prefs` (the `renderer` row and the kept pick), `canvas`
 * (the scene's: its size is the probe's), `createGpu`, `makeLayers`, `load` (the vendored files),
 * `levels` (the budget's starting levels), `seed`,
 * `signal` (the mount's: the probe stops when it ends).
 * Returns `{ tier, why, pick }`: `why` is what the readout says on WebGL2 ('chosen', 'faster', or
 * null for the engine's own reasons), `pick` the measurement when one was made. A tier whose count
 * was disturbed (see `measure`) is measured once more; one still disturbed, or neither tier drawing,
 * keeps nothing (the next visit measures again), and neither drawing keeps WebGPU, the engine's own
 * fallback, with no reason.
 */
export async function pickTier({
  prefs, canvas, createGpu, makeLayers, load, levels, seed = 0, signal = null,
}) {
  const doc = canvas.ownerDocument;
  const win = doc.defaultView;
  const search = win.location?.search ?? '';
  const renderer = rendererOf(prefs, search);
  if (renderer !== 'auto') return { tier: renderer, why: renderer === 'webgl2' ? 'chosen' : null, pick: null };
  let adapter = null;
  try {
    adapter = (await win.navigator?.gpu?.requestAdapter?.()) ?? null;
  } catch {
    adapter = null;
  }
  // the canvas's size and the backing pixels of the budget's top step
  const css = { width: canvas.clientWidth || 1, height: canvas.clientHeight || 1 };
  const density = win.devicePixelRatio || 1;
  const base = baseScale(css.width, css.height, density);
  const size = () => ({ w: css.width * base, h: css.height * base, css });
  const key = pickKey({
    ua: win.navigator?.userAgent ?? '', gpu: gpuWords(doc, adapter), w: css.width, h: css.height, density,
  });
  const asked = new URLSearchParams(search).get('vg-probe');
  const { tier, probe } = decide({
    renderer,
    kept: prefs?.get?.('renderer-pick', null),
    key,
    hasAdapter: Boolean(adapter),
    automated: Boolean(win.navigator?.webdriver),
    asked: asked !== null,
  });
  if (!probe) {
    const kept = keptPick(prefs?.get?.('renderer-pick', null), key);
    return { tier, why: kept === 'webgl2' ? 'faster' : null, pick: null };
  }
  const fps = {};
  let clean = true;
  for (const each of ['webgpu', 'webgl2']) {
    let state = { clean: true };
    for (let go = 0; go < 2; go++) {
      state = { clean: true };
      fps[each] = await measure({
        tier: each,
        host: canvas,
        createGpu,
        makeLayers,
        load,
        size,
        levels,
        seed,
        every: asked === `slow-${each}` ? PROBE.slowEvery : 1,
        signal,
        state,
        guard: !(asked ?? '').startsWith('slow-'),
      });
      if (signal?.aborted) return { tier: 'webgpu', why: null, pick: null };
      if (state.clean || fps[each] === null) break;
    }
    if (!state.clean) clean = false;
  }
  const won = fastest(fps);
  if (won === null) return { tier: 'webgpu', why: null, pick: null };
  const pick = { key, tier: won, fps };
  if (clean) prefs?.set?.('renderer-pick', keepPick(prefs?.get?.('renderer-pick', null), pick));
  return { tier: won, why: won === 'webgl2' ? 'faster' : null, pick };
}
