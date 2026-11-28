/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The loop that feeds the scene and sends a step to the HUD and the panel. The feed's runtime
// holds the core's analysis while the scene shows (the loop's own watch of the canvas opens it
// when the scene shows and lets it go once the scene has been off show for the core's grace;
// unmounting lets it go at once), steps the feed's modules at each draw (features, the conductor,
// the cues, the history, the waveform), sends the result as a `step` on `vg.steps`, moves the rig
// (the clock, the uniforms, the camera) and draws the layers under the budget. Without a GPU
// scene (neither WebGPU nor WebGL2 gives a context) the scene is a still of the valley with a
// quiet line, the feed goes on at the Still pace and the HUD and the dash work.
//
//   createFeed()          the feed's runtime without a page: `hello`, `push`, `capture`, `step`
//   wireScene(vg, seams)  the scene's wire; returns `{ driver }` (`tier`, `frame`, `pixels`,
//                         `perf`, `forceDrawMs`); `seams` hands the checks their own `createGpu`,
//                         `makeLayers`, `clock` (ms) and `decodeCover(url)` (32x32 RGBA bytes
//                         or null)

import { computerOf } from '../../core/path-words.js';
import { BANDS, VizModel, posBand } from '../../core/viz-model.js';
import { createBudget, paceUnder } from './budget.js';
import { CueTracker } from './cues.js';
import { createDirector, seedOf } from './director.js';
import { Features, isSilent } from './features.js';
import {
  createGpu as realCreateGpu, disposeAfterFrames, loadThree, tierWords,
} from './gpu.js';
import { History } from './history.js';
import { createLayers } from './layers.js';
import { createRig, keepFrame, motionOf, paceOf } from './motion.js';
import { PALETTES, palette as paletteOf } from './palette.js';
import { preview } from './preview.js';
import { pickTier } from './tier-probe.js';
import { SAMPLES, waveOf } from './wave.js';

/** The side of the square a cover is drawn into for its palette. */
const COVER_SIZE = 32;

/** The spectrum's log bins (the audio buffers' second block). */
const BINS = 64;

/** The shortest pace the loop waits on a timer for, ms (core/lifecycle.js). */
const SLOW_PACE = 100;

/** The pace of the feed with no GPU scene, ms: the Still pace. */
const STILL_PACE = 1000;

/** How long the music must be silent before the scene counts as idle, seconds. */
const SILENT_FOR = 2;

/** How often the Motion level is read from the prefs again, ms of the page's clock. */
const MOTION_READ_MS = 500;

/** Every how many draws the GPU time is asked for (the tier that has one). */
const GPU_MS_EVERY = 8;
/** How long a GPU time stands as the last reading, ms: a reading that stopped coming (every query
 * disjoint, a readback rejected) is not evidence. */
const GPU_MS_FRESH = 2000;

/** The small target a readback is made into: statistics, never a picture. */
const READ = Object.freeze({ w: 160, h: 100 });

/** The steps the readback frame of a seed runs from rest to its time, per second. */
const FRAME_STEPS_PER_S = 30;

/** The quiet line of a canvas that has no GPU scene. */
export const NO_GPU_WORDS = 'This theme needs WebGPU or WebGL2';

/** The 64 log bins of the spectrum from the two sides' 240 smoothed bands (the larger side, the
 * bin's mean over the bands it spans). */
export function spectrumOf(left, right, out = new Float32Array(BINS)) {
  for (let j = 0; j < BINS; j++) {
    const lo = posBand(j, BINS);
    const hi = Math.max(lo, posBand(j + 1, BINS) - (j + 1 < BINS ? 1 : 0));
    let sum = 0;
    for (let i = lo; i <= hi; i++) sum += Math.max(left[i], right[i]);
    out[j] = sum / (hi - lo + 1);
  }
  return out;
}

/** The feed's runtime: the modules of the feed over the analysis' frames, with no page. */
export function createFeed() {
  const model = new VizModel();
  const features = new Features();
  const director = createDirector();
  const tracker = new CueTracker();
  const history = new History();
  const bins = new Float32Array(BINS);
  let wave = new Float32Array(SAMPLES);
  let capture = null;
  let silent = true;
  let trackKey = null;
  let pair = PALETTES[0];
  let text = { title: '', artist: '' };

  return {
    model,
    features,
    director,
    tracker,
    history,

    /** The stream said it was `daemonNowMs` on the daemon's clock when the page's read `pageMs`. */
    hello(daemonNowMs, pageMs) {
      model.hello(daemonNowMs, pageMs);
    },

    /** A `frame` event's text read at page time `pageMs`. */
    push(text, pageMs) {
      model.push(text, pageMs);
    },

    /** A `hello` or `capture` event as the stream sends it (`{ capture, sink, why }`, `capture`
     * being `starting`, `running` or `unavailable`), kept as `{ state, sink, why }`; anything but
     * running empties the curves. */
    capture(data) {
      capture = data ? { state: data.capture, sink: data.sink ?? null, why: data.why ?? null } : null;
      if (capture?.state !== 'running') model.clear();
    },

    /** The cover of the track on show, as 32x32 RGBA bytes: the palette's pair from it, or the
     * track's seeded pair for none (no cover, one that is gone, or a grey one). */
    cover(pixels) {
      pair = paletteOf(pixels, text);
    },

    /** Back to nothing: the stream is let go. */
    release() {
      capture = null;
      model.clear();
    },

    get running() { return capture?.state === 'running'; },
    get captured() { return capture; },

    /**
     * One step of `dt` seconds at page time `pageMs` over `page` (the core's page model); `changed`
     * (the path changes the sound), `side` (whose volume is shown) and `nowMs` (the daemon's clock
     * for the signal's age) come from the wire. Returns what the layers, the HUD and the panel
     * read.
     */
    step(dt, pageMs, {
      page, changed = false, side = 'device', nowMs = null,
    }) {
      const running = capture?.state === 'running';
      const { frame } = model.step(pageMs);
      const seen = running ? frame : null;
      if (seen) {
        features.observe(seen);
        silent = isSilent(seen);
        spectrumOf(model.left, model.right, bins);
      } else if (!running) {
        silent = true;
      }
      const cues = tracker.update(page, {
        dt, silent, changed, side, nowMs,
      });
      features.step(dt, { gate: cues.pause ? 0 : 1, mute: cues.mute });

      // a new track reseeds through the fade (the first one is the scene's start: no fade)
      const track = page?.host?.track;
      const title = track?.headline ?? '';
      const key = track ? `${title}\u0000${track.artist ?? ''}` : '';
      if (key !== trackKey) {
        const first = trackKey === null;
        trackKey = key;
        text = { title, artist: track?.artist };
        pair = paletteOf(null, text);
        if (first) director.state.seed = seedOf(title, track?.artist);
        else director.track(title, track?.artist);
      }
      const events = director.step(dt, {
        frame: seen,
        raw: features.raw,
        values: features.values,
        now: pageMs,
        offsetMs: model.offsetMs ?? 0,
        mute: cues.mute,
        pause: cues.pause,
      });
      history.push(features.values, dt);
      if (seen) wave = waveOf(bins, director.state.seed, wave);
      return {
        t: director.state.time,
        dt,
        running,
        silent,
        features: { ...features.values },
        raw: { ...features.raw },
        cues,
        section: director.state.section,
        events,
        director: director.state,
        history,
        palette: pair,
        seed: director.state.seed,
        bands: { left: model.left, right: model.right },
        bins,
        wave,
      };
    },
  };
}

/** The bytes (RGBA) of the cover at `url` drawn into a 32x32 canvas, or null when it does not load
 * or the canvas cannot be read. */
function decodeImage(win, doc, url) {
  return new Promise((resolve) => {
    const image = new win.Image();
    image.onload = () => {
      try {
        const canvas = doc.createElement('canvas');
        canvas.width = COVER_SIZE;
        canvas.height = COVER_SIZE;
        const ctx = canvas.getContext('2d', { willReadFrequently: true });
        ctx.drawImage(image, 0, 0, COVER_SIZE, COVER_SIZE);
        resolve(ctx.getImageData(0, 0, COVER_SIZE, COVER_SIZE).data);
      } catch {
        resolve(null);
      }
    };
    image.onerror = () => resolve(null);
    image.src = url;
  });
}

/** What a draw hands the layers: the budget's levels (budget.js) and whether the draws are
 * capped, written into `into` (one object a scene, not one a draw). */
function budgetView(budget, into = {}) {
  Object.assign(into, budget.levels);
  into.capped = budget.capped;
  return into;
}

/**
 * Wire the scene in `vg.page` (its `#vg-scene`) to `vg.core`. Returns `{ driver, loop }`. `seams`
 * (the checks') replaces the renderer's and the layers' makers.
 */
export function wireScene(vg, {
  createGpu = realCreateGpu, makeLayers = createLayers, clock = () => globalThis.performance.now(),
  decodeCover = null,
} = {}) {
  const { core, page, signal } = vg;
  const doc = page.ownerDocument;
  const win = doc.defaultView;
  const canvas = page.querySelector('#vg-scene');
  const feed = createFeed();
  const rig = createRig({ seed: 0 });
  const coarse = Boolean(win?.matchMedia?.('(pointer: coarse)').matches);
  const reduced = () => Boolean(win?.matchMedia?.('(prefers-reduced-motion: reduce)').matches);
  const kept = core.prefs?.get?.('tier', null);
  const detailPref = () => core.prefs?.get?.('detail', 'auto');
  // the kept tier applies once the renderer says which GPU it is (`budget.device`)
  const budget = createBudget({
    coarse, start: kept, detail: detailPref(), device: null,
  });
  const decode = decodeCover ?? ((url) => decodeImage(win, doc, url));

  const run = {
    gpu: null,
    layers: null,
    tier: null,
    target: null,
    holder: null,
    css: { width: 1, height: 1, density: 1 },
    motion: 'full', motionAt: -Infinity, motionWas: 'full', gpuMs: null, still: null,
    // the scene's draw count and the steps sent
    draws: 0,
    sent: 0,
    lastMs: null,
    pace: 0,
    asked: 0,
    quietSince: null,
    drawnAt: 0,
    offeredAt: 0,
    owed: 0,
    keptStep: -1,
    keptFps: 0,
    gpuAt: 0, gpuReadAt: -Infinity, gpuEpoch: 0, gpuPending: false, rigSeed: 0, last: null, coverKey: null,
    // the load's stamps (ms of the page's clock): the wire's start, the vendored files in, the
    // renderer up, the layers built, the first update's time (the land cache's first fill), the
    // materials compiled, and the first two draws' update and draw times
    load: {
      start: clock(), three: null, gpu: null, layers: null, update: null, compiled: null, first: null,
      second: null,
    },
    // the first frame's compile: null before it, false while it runs, true once done
    compiled: null,
    // the tiers measured on this visit (tier-probe.js), or null
    pick: null,
    // the budget's levels as the layers read them, refilled each draw
    view: {},
  };

  vg.steps.addEventListener('explore', (e) => feed.director.explore(e.detail.kind, { flash: run.motion === 'full' }), { signal });

  /** The analysis, held while the scene shows: the one holder of the page. The capture's state and
   * the letting go are told on `vg.steps` (`capture`, `release`) to the parts that draw them. */
  function open() {
    if (run.holder) return;
    run.holder = core.analysis.open({
      hello: (data) => feed.hello(data.now_ms, clock()),
      capture: (data) => {
        feed.capture(data);
        vg.steps.dispatchEvent(new globalThis.CustomEvent('capture', { detail: data }));
      },
      frame: (text) => feed.push(text, clock()),
    });
  }

  function close() {
    run.holder?.close();
    run.holder = null;
    budget.pause();
    feed.release();
    vg.steps.dispatchEvent(new globalThis.Event('release'));
  }

  /** Size the canvas's backing store for the CSS size and the budget's density. */
  function fit() {
    if (!run.gpu) return;
    const { width, height } = run.css;
    run.gpu.size(width * budget.scale, height * budget.scale);
  }

  function resize(width, height, density) {
    run.css = { width, height, density };
    budget.size(width, height, density);
    fit();
  }

  /** No GPU scene: the still of the valley and the quiet line, the feed goes on at Still pace. */
  function fallback() {
    run.tier = 'none';
    const still = doc.createElement('div');
    still.id = 'vg-still';
    preview(still);
    const line = doc.createElement('p');
    line.id = 'vg-quiet';
    line.textContent = NO_GPU_WORDS;
    still.append(line);
    canvas.after(still);
    run.still = still;
  }

  /** Whether the browser offers a GPU canvas at all (checked before the vendored files load). */
  const offered = () => Boolean(win?.navigator?.gpu || win?.WebGL2RenderingContext);

  async function init() {
    // the mount is still building the page: the wire touches its children after it
    await null;
    if (signal.aborted) return;
    if (!offered()) {
      fallback();
      return;
    }
    let gpu = null;
    try {
      const load = async () => {
        const three = await loadThree();
        run.load.three ??= clock();
        return three;
      };
      // the tier: the Renderer row's, else the kept pick of this browser, GPU and size, else the
      // faster of the two measured now (tier-probe.js)
      const choice = await pickTier({
        prefs: core.prefs, canvas, createGpu, makeLayers, load, levels: budgetView(budget),
        seed: feed.director.state.seed, signal,
      });
      run.pick = choice.pick;
      if (signal.aborted) return;
      gpu = await createGpu({
        canvas, forceWebGL: choice.tier === 'webgl2', why: choice.why, load,
      });
      run.load.gpu = clock();
      if (signal.aborted) {
        gpu.dispose?.();
        return;
      }
      budget.device(`${gpu.caps?.tier ?? gpu.tier}|${gpu.caps?.device ?? ''}`);
      run.layers = makeLayers(gpu, { seed: feed.director.state.seed });
      run.load.layers = clock();
      run.gpu = gpu;
      run.target = gpu.target(READ.w, READ.h);
      fit();
      run.tier = gpu.tier;
    } catch (error) {
      gpu?.dispose?.();
      run.gpu = null;
      run.layers = null;
      if (!signal.aborted) {
        globalThis.console?.info?.('Void Garden: no GPU scene', error);
        fallback();
      }
    }
  }

  const motionNow = (ms) => {
    if (ms - run.motionAt >= MOTION_READ_MS) {
      run.motionAt = ms;
      run.motion = motionOf(core.prefs, reduced());
      if (budget.setDetail(detailPref())) fit();
    }
    return run.motion;
  };

  /** The least ms between draws now: the level's, and the cap's; the Still pace with no GPU
   * scene. */
  function pace() {
    const playing = core.model.page().host?.track?.status === 'playing';
    const idle = run.quietSince !== null && run.drawnAt - run.quietSince >= SILENT_FOR;
    const motion = motionNow(clock());
    run.asked = run.tier === 'none' ? STILL_PACE : paceOf({
      motion, silent: idle, playing, coarse,
    });
    // choosing Full from a slower level: the cost is found out in about a second
    if (motion === 'full' && run.motionWas !== 'full') budget.probe();
    run.motionWas = motion;
    run.pace = paceUnder({ asked: run.asked, capped: budget.capped });
    return run.pace;
  }

  /** The cover of `track` read for the palette: a new track, or a new cover, starts from its seeded
   * pair; the cover's pair follows once it is drawn (a cover that is gone leaves the seeded
   * one). */
  function coverOf(track) {
    const hash = track?.cover ?? null;
    const key = `${track?.headline ?? ''}\u0000${track?.artist ?? ''}\u0000${hash ?? ''}`;
    if (key === run.coverKey) return;
    run.coverKey = key;
    feed.cover(null);
    if (hash === null || run.tier === 'none') return;
    decode(`api/v1/host/cover?v=${encodeURIComponent(hash)}`).then((pixels) => {
      if (!signal.aborted && run.coverKey === key) feed.cover(pixels);
    }, () => {});
  }

  /** One step of the feed and the rig at page time `seconds`; sends the `step`. */
  function tick(dt, seconds) {
    const ms = seconds * 1000;
    const model = core.model.page();
    // the daemon's clock now, from the analysis hello's offset (none before it: the facts' own)
    const { offsetMs } = feed.model;
    const nowMs = Number.isFinite(offsetMs) ? ms + offsetMs : null;
    const state = feed.step(dt, ms, {
      page: model,
      changed: computerOf(model).changes,
      side: core.computer?.side?.() ?? 'device',
      nowMs,
    });
    rig.step(dt, {
      motion: run.motion,
      features: state.features,
      cues: state.cues,
      director: state.director,
      events: state.events,
      aspect: run.css.width / Math.max(1, run.css.height),
      terrain: run.layers?.ctx.terrain ?? undefined,
    });
    if (state.running && state.silent) run.quietSince ??= seconds;
    else run.quietSince = null;
    // a seed (the first track's, or a new track's at black) reaches the rig and the layers,
    // whichever came up first
    if (state.seed !== run.rigSeed) {
      run.rigSeed = state.seed;
      rig.reseed(state.seed);
    }
    if (run.layers && state.seed !== run.layers.seed) run.layers.build(state.seed);
    coverOf(model.host?.track);
    run.sent++;
    vg.steps.dispatchEvent(new globalThis.CustomEvent('step', { detail: state }));
    return state;
  }

  /** What the layers read of the director: this step's events, its state and section, the cues,
   * and its flash limiter (the one limiter of the scene: a layer that flashes asks it). */
  const directed = (state) => ({
    events: state.events,
    director: state.director,
    section: state.section,
    cues: state.cues,
    flash: () => feed.director.flash(),
  });

  /** The audio `state` carries, sent to the layers' buffers. */
  const sendAudio = (layers, state) => layers.ctx.audio.set({
    left: state.bands.left, right: state.bands.right, bins: state.bins, samples: state.wave,
  });

  /** Draw one frame of the layers for `state`; the budget is sampled with the draw's own time: from
   * the audio upload to the render's return, not the feed's step nor its listeners'. */
  function paint(state, seconds) {
    const { layers, gpu } = run;
    if (run.compiled === null && !gpu.compile) run.compiled = true;
    if (run.compiled !== true) {
      // the first frame: the layers are updated (the land cache makes its targets and draws what is
      // in view) and every material of the scene is compiled before anything is drawn, side by side
      // where the browser can (KHR_parallel_shader_compile, WebGPU's async pipelines), the page
      // live meanwhile; the draws begin once it is done
      if (run.compiled === false) return;
      run.compiled = false;
      run.last = state;
      const began = clock();
      sendAudio(layers, state);
      layers.update({
        t: state.t, dt: state.dt, cam: rig.cam, u: rig.u, budget: budgetView(budget),
        palette: state.palette, seed: state.seed, ...directed(state),
      });
      run.load.update = clock() - began;
      const done = () => {
        run.compiled = true;
        run.load.compiled = clock();
      };
      Promise.resolve(gpu.compile(layers.ctx.scene, layers.ctx.camera)).then(done, done);
      return;
    }
    const began = clock();
    run.last = state;
    gpu.timer?.begin();
    sendAudio(layers, state);
    layers.update({
      t: state.t, dt: state.dt, cam: rig.cam, u: rig.u, budget: budgetView(budget, run.view),
      palette: state.palette, seed: state.seed, ...directed(state),
    });
    const updated = clock();
    layers.draw();
    gpu.timer?.end();
    if (run.draws < 2) {
      // the first draws: the land cache's first fill (the update) and the shaders' compile (the draw)
      run.load[run.draws ? 'second' : 'first'] = { at: began, update: updated - began, draw: clock() - updated };
    }
    run.draws++;
    const took = clock() - began;
    run.lastMs = took;
    const timed = gpu.timer?.read();
    if (timed !== null && timed !== undefined) {
      run.gpuMs = timed;
      run.gpuReadAt = began;
    }
    if (gpu.caps.timestamps && !run.gpuPending && run.draws % GPU_MS_EVERY === 0) {
      // three's timestamp pool answers the total of the last frame it timed: one frame's time
      const epoch = run.gpuEpoch;
      run.gpuAt = run.draws;
      run.gpuPending = true;
      gpu.gpuMs().then((ms) => {
        // a reading that spans a change of step is not the step's
        if (epoch !== run.gpuEpoch) return;
        run.gpuMs = ms;
        run.gpuReadAt = clock();
      }, () => {}).finally(() => { run.gpuPending = false; });
    }
    // no stale value walks the ladder: a reading that stopped coming expires
    if (run.gpuMs !== null && began - run.gpuReadAt > GPU_MS_FRESH) run.gpuMs = null;
    const changed = budget.sample({
      ms: took, gpuMs: run.gpuMs, seconds, full: run.asked === 0,
    });
    if (budget.step !== run.keptStep || budget.target !== run.keptFps) {
      if (run.keptStep >= 0 && budget.step !== run.keptStep) { run.gpuEpoch++; run.gpuAt = run.draws; }
      run.keptStep = budget.step;
      run.keptFps = budget.target;
      // a pinned Detail is the viewer's choice, and a held step the checks' (neither is what this
      // screen settled on)
      if (budget.detail === 'auto' && !budget.held) core.prefs?.set?.('tier', budget.tier);
    }
    if (changed) {
      // resizing blanks the canvas: draw again at once
      fit();
      layers.refit?.();
      layers.draw();
    }
  }

  function draw(step, seconds) {
    run.owed += step;
    const frameMs = Math.min(50, (seconds - run.offeredAt) * 1000);
    run.offeredAt = seconds;
    budget.offer(frameMs);
    // the loop waits on a timer only for a pace of 100 ms or more: a shorter one (the 30 a second
    // cap) is kept here, by leaving the frames in between undrawn
    if (run.pace < SLOW_PACE && !keepFrame({
      sinceMs: (seconds - run.drawnAt) * 1000,
      frameMs,
      paceMs: run.pace,
      atMost: coarse && !budget.capped,
    })) return;
    const dt = run.owed;
    run.owed = 0;
    run.drawnAt = seconds;
    const state = tick(dt, seconds);
    if (run.gpu) paint(state, seconds);
  }

  const loop = core.lifecycle.canvasLoop({
    canvas,
    draw,
    resize,
    reduced: 'theme',
    pace,
    watchVisible: (options) => core.lifecycle.watchVisible({
      ...options, onShow: open, onHide: close,
    }),
    matchMedia: (query) => win.matchMedia(query),
  });
  loop.start();
  signal.addEventListener('abort', () => {
    loop.stop();
    close();
    run.layers?.dispose();
    // not at once: a device destroyed right after a draw hung the next theme's frames on Firefox
    disposeAfterFrames(run.gpu, win);
    run.gpu = null;
    run.layers = null;
    run.still?.remove();
  }, { once: true });
  init();

  /** One frame of the scene for `seed` at `t` seconds, from rest with no feed: the rig stepped
   * 30 a second, the layers built for the seed, the audio buffers silent, a small readback's
   * statistics. Same seed and time, same statistics whatever the live loop was playing; the live
   * seed and audio are put back after. With `onCanvas` the frame is drawn on the canvas instead,
   * under the live budget's levels (a held step), for a still: the same moment at any step. */
  async function frameFor(seed, t, onCanvas = false) {
    const { layers } = run;
    if (!layers) return null;
    const probe = createRig({ seed });
    const director = createDirector();
    director.state.seed = Number(seed);
    const values = new Features().values;
    const steps = Math.max(1, Math.round(t * FRAME_STEPS_PER_S));
    const was = layers.seed;
    // built first: the land layer sets the seed's terrain then, and the probe's camera flies
    // over it
    layers.build(Number(seed));
    for (let i = 0; i < steps; i++) {
      probe.step(1 / FRAME_STEPS_PER_S, {
        motion: 'full',
        features: values,
        cues: {},
        director: director.state,
        events: [],
        aspect: onCanvas ? run.css.width / Math.max(1, run.css.height) : READ.w / READ.h,
        terrain: layers.ctx.terrain ?? undefined,
      });
    }
    layers.ctx.audio.set({
      left: new Float32Array(BANDS), right: new Float32Array(BANDS), bins: new Float32Array(BINS),
      samples: new Float32Array(SAMPLES),
    });
    layers.update({
      t, dt: 0, cam: probe.cam, u: { ...probe.u, colRate: 1 }, budget: budgetView(onCanvas ? budget : createBudget()),
      palette: paletteOf(null, { title: `seed ${seed}`, artist: '' }), seed: Number(seed),
      events: [], director: director.state, section: director.state.section, cues: {},
    });
    if (onCanvas) {
      // twice: the post chain's second frame is the settled one
      layers.draw();
      layers.draw();
      return { seed, t };
    }
    const stats = await layers.readStats(run.target);
    layers.build(was);
    if (run.last) sendAudio(layers, run.last);
    return { seed, t, ...stats };
  }

  return {
    loop,
    driver: {
      tier: () => run.tier,
      frame: (seed, t) => frameFor(seed, t),
      // for the stills: the live loop stops, and one frame at (seed, t) is drawn on the canvas
      freeze: () => { loop.stop(); return true; },
      still: (seed, t) => frameFor(seed, t, true),
      pixels: async () => (run.layers ? run.layers.readStats(run.target) : null),
      // for the checks: the hero's centre on the canvas as of the last draw, in clip units
      // (`[x, y, inFront]`), or null
      heroAt: () => {
        const scope = run.layers?.list.find((l) => l.name === 'hero')?.probe?.().h.scope.value;
        return scope ? [scope.x, scope.y, scope.z] : null;
      },
      perf: () => ({
        tier: run.tier,
        words: tierWords(run.gpu?.caps),
        detail: budget.detail,
        palette: run.last?.palette ?? null,
        seeds: { rig: run.rigSeed, layers: run.layers?.seed ?? null },
        draws: run.draws,
        steps: run.sent,
        ms: run.lastMs,
        gpuMs: run.gpuMs,
        pace: run.pace,
        asked: run.asked,
        motion: run.motion,
        ...budget.levels,
        capped: budget.capped,
        step: budget.step,
        target: budget.target,
        rungs: budget.steps,
        load: { ...run.load },
        pick: run.pick,
      }),
      forceDrawMs: (ms) => { budget.forceDrawMs(ms); return true; },
      hold: (step, fps) => {
        if (budget.hold(step, fps)) {
          fit();
          run.layers?.refit?.();
        }
        return true;
      },
    },
  };
}
