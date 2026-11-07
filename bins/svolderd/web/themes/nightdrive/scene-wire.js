/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's loop: the two canvases (the sky under, the road and the ridge over), the music read
// from the core's analysis through a `VizModel`, the floor's rows born from it, the ear's mood
// (mood.js, look.js) made of it and the frame drawn from all of it. The analysis is held only while
// the scene is on show: the loop's own watch of the canvas opens it when the scene shows and lets it
// go once the scene has been off show for the core's grace, and unmounting lets it go at once.

import { BANDS, VizModel } from '../../core/viz-model.js';
import { COLUMNS } from './axis.js';
import {
  FLOOR, crossings, drawRidge, drawRoad,
} from './draw/road.js';
import { drawEmbers, drawHaze, drawWet } from './draw/mood.js';
import { drawBubbles, drawDiamonds } from './draw/tune.js';
import { PALETTE } from './draw/palette.js';
import { drawSky } from './draw/sky.js';
import { createEmbers } from './embers.js';
import { createFloor } from './floor.js';
import { BUDGET, createBudget, paceUnder } from './budget.js';
import { CAMERA, horizonOf } from './camera.js';
import { createLook } from './look.js';
import { createEar } from './mood.js';
import { CALM_RIDGE, keepFrame, paceOf } from './motion.js';
import { bandUnits, bassOf, columnLevels, highsOf, rowRate, silent } from './music.js';
import { driftSwell } from './ridge.js';
import { createSky } from './sky.js';
import { inertOf } from './inert.js';
import { dullMood, flatMood, moodOf } from './states.js';
import { createStats } from './stats.js';
import { wireTune } from './tune-wire.js';

/** The windows where the dash is a panel over the foot of the scene (the stylesheet's phone held upright). */
const UPRIGHT = '(max-width: 719px) and (min-height: 500px)';

/** How long the sun's pulse lasts after a beat, ms. */
const KICK_MS = 150;

/** The shortest pace the loop waits on a timer for, ms (core/lifecycle.js). */
const SLOW_PACE = 100;

/** The least ms between draws while the scene is dull (out of reach, lost, reading again): nothing in it moves. */
const STILL_PACE = 500;

/** How long every column stays under 0.02 before the music counts as silent, seconds. */
const SILENT_FOR = 2;

/**
 * Wire the scene in `root` (its `#nd-sky` and `#nd-road`) to `core` and `view`; `live` is
 * `createLive`'s handle; `off` is the set of layers the address turned off (debug.js). Returns
 * `{ loop, floor, cols, model, ear, look, embers, tune, perf(), sky(), stop() }`: `tune` is the mountains' hands
 * (tune-wire.js), `ear` and `look` the mood's (the checks move them), `stop` ends the loop and lets the
 * analysis go.
 */
export function wireScene({
  root, core, view, live, signal, off = new Set(),
}) {
  const skyCanvas = root.querySelector('#nd-sky');
  const roadCanvas = root.querySelector('#nd-road');
  const sky = skyCanvas.getContext('2d');
  const road = roadCanvas.getContext('2d');
  const model = new VizModel();
  // the rows beyond the scene's bottom edge are what a lifted near edge is drawn on
  const floor = createFloor({ rows: CAMERA.rows + FLOOR.extra });
  const cols = new Float32Array(COLUMNS);
  const quiet = new Float32Array(COLUMNS).fill(0.05);
  const ridge = new Float32Array(BANDS);
  const slow = new Float32Array(BANDS);
  const events = createSky({});
  const ear = createEar();
  const look = createLook();
  const embers = createEmbers({});
  let mirror = null;
  let shown = { rays: 0, stars: [] };
  let quietSince = null;
  let drawnAt = 0;
  let offeredAt = 0;
  let swell = 0;
  const geo = { w: 1, h: 1, v: 1, H: 1, dpr: 1 };
  // A phone held upright has the dash panel over the foot of the scene: the horizon is placed in the part it leaves
  // uncovered. The panel's height is read when it or the window changes, not at every frame.
  let covered = 0;
  const dashBox = root.querySelector('#nd-dash');
  const readCovered = () => {
    const win = root.ownerDocument.defaultView;
    const over = dashBox && win?.matchMedia?.(UPRIGHT).matches;
    covered = over ? Math.max(0, win.innerHeight - dashBox.getBoundingClientRect().top) : 0;
  };
  if (dashBox && typeof globalThis.ResizeObserver === 'function') {
    const watching = new globalThis.ResizeObserver(readCovered);
    watching.observe(dashBox);
    // the window's own height moves without a resize event too (a phone's address bar, a keyboard)
    watching.observe(root.ownerDocument.documentElement);
    signal.addEventListener('abort', () => watching.disconnect(), { once: true });
  }
  root.ownerDocument.defaultView?.addEventListener('resize', readCovered, { signal });
  readCovered();
  const tune = wireTune({
    root, core, live, signal, geometry: () => geo,
  });
  const perf = { draws: 0, mirrored: 0, pace: 0, asked: 0 };
  const stats = createStats();
  // a finger for a pointer is a phone: it starts at the rate's cap
  const coarse = Boolean(globalThis.matchMedia?.('(pointer: coarse)').matches);
  // the tier that held on this device last time is where the budget starts; it measures and corrects from there
  const kept = core.prefs?.get('tier', null);
  const keptOk = Number.isInteger(kept?.step) && kept.step >= 0 && kept.step < BUDGET.maxStep && typeof kept.capped === 'boolean';
  const start = keptOk ? kept : null;
  const budget = createBudget({ coarse, start });
  let keptStep = budget.step;
  let keptCapped = budget.capped;
  let motionWas = view.motion;
  let owed = 0;
  let css = { width: 1, height: 1, density: 1 };
  let capture = null;
  let holder = null;
  let kickAt = -Infinity;

  const clock = () => globalThis.performance.now();

  /** How the page stands now (states.js): the scene is drawn dull in trouble, with the mountains flat or lost. */
  const kindNow = () => moodOf(core.model.page(), view.state, capture);

  function follow(data) {
    const was = capture?.state;
    capture = { state: data.capture, why: data.why };
    view.capture = capture;
    if (data.capture !== 'running') model.clear();
    // the card and the quiet line say what the music's analysis is now
    if (was !== data.capture) live.draw('scene');
  }

  function open() {
    if (holder) return;
    holder = core.analysis.open({
      hello: (data) => model.hello(data.now_ms, clock()),
      capture: follow,
      frame: (text) => model.push(text, clock()),
    });
  }

  function close() {
    holder?.close();
    holder = null;
    capture = null;
    view.capture = null;
    model.clear();
  }

  /** Size the canvases' backing stores for the CSS size and the budget's scale (the CSS size is not
   * touched; a canvas that is resized is blank until the next draw). */
  function fit() {
    const scale = budget.scale;
    for (const canvas of [skyCanvas, roadCanvas]) {
      canvas.width = Math.max(1, Math.round(css.width * scale));
      canvas.height = Math.max(1, Math.round(css.height * scale));
    }
    sky.setTransform(scale, 0, 0, scale, 0, 0);
    road.setTransform(scale, 0, 0, scale, 0, 0);
    Object.assign(geo, { w: css.width, h: css.height, dpr: scale, glow: !off.has('noglow') });
  }

  function resize(width, height, density) {
    css = { width, height, density };
    budget.size(width, height, off.has('dpr1') ? 1 : density);
    fit();
  }

  /** Mirror the sky and the mountains in the road, as the wet road does: the sky just drawn above the
   * horizon, with the mountains over it, is drawn again in slices below it, which sway together as one
   * wave. The copy is made at every draw, so the reflection never trails the sky; it is at the scene's own
   * density. */
  function reflect(frame) {
    mirror ??= root.ownerDocument.createElement('canvas');
    const rows = Math.min(skyCanvas.height, Math.ceil(geo.H * geo.dpr));
    if (mirror.width !== skyCanvas.width || mirror.height !== rows) {
      mirror.width = skyCanvas.width;
      mirror.height = rows;
    }
    perf.mirrored++;
    const copy = mirror.getContext('2d');
    copy.setTransform(1, 0, 0, 1, 0, 0);
    copy.clearRect(0, 0, mirror.width, mirror.height);
    copy.drawImage(skyCanvas, 0, 0, mirror.width, rows, 0, 0, mirror.width, rows);
    copy.setTransform(geo.dpr, 0, 0, geo.dpr, 0, 0);
    if (!off.has('noridge')) {
      drawRidge(copy, { ...geo, glow: false }, slow, { dull: false, flat: frame.flat, swell: frame.swell });
    }
    drawWet(sky, mirror, geo, frame.look.wet, frame.t);
  }

  /** Paint `frame` on both canvases. */
  function paint(frame) {
    if (!off.has('nosky')) {
      drawSky(sky, geo, frame);
      if (frame.look.wet > 0.01 && !frame.dull) reflect(frame);
    }
    road.clearRect(0, 0, geo.w, geo.h);
    if (!off.has('noridge')) drawRidge(road, geo, slow, frame);
    if (!off.has('nofloor')) drawRoad(road, geo, floor, frame);
    if (frame.look.haze > 0.01 && !frame.dull) drawHaze(road, geo, frame.look.haze, frame.look.mood);
    if (!frame.dull) drawEmbers(road, embers.list);
    drawDiamonds(road, geo, frame.tune);
    drawBubbles(road, geo, frame.tune);
    // a page that lost its daemon is frozen under a veil over both canvases
    if (frame.veil) {
      road.globalAlpha = 1;
      road.fillStyle = PALETTE.veil;
      road.fillRect(0, 0, geo.w, geo.h);
    }
  }

  function draw(step, seconds) {
    owed += step;
    const frameMs = Math.min(50, (seconds - offeredAt) * 1000);
    offeredAt = seconds;
    // the loop waits on a timer only for a pace of 100 ms or more: a shorter one (the 30 a second
    // cap) is kept here, by leaving the frames in between undrawn
    if (perf.pace < SLOW_PACE && !keepFrame({
      sinceMs: (seconds - drawnAt) * 1000, frameMs, paceMs: perf.pace, atMost: coarse && !budget.capped,
    })) return;
    const dt = owed;
    owed = 0;
    const began = clock();
    const ms = seconds * 1000;
    drawnAt = seconds;
    model.step(ms);
    if (model.beatDue(ms)) kickAt = ms;
    const running = capture?.state === 'running';
    if (running) {
      columnLevels(model.left, model.right, cols);
      bandUnits(model.left, model.right, ridge);
    } else {
      cols.set(quiet);
      ridge.fill(0);
    }
    const kind = kindNow();
    const dull = dullMood(kind);
    // Calm halves the rates of the ear's views; Still finds no moment, so spends none
    const heard = ear.step(dt, ridge, view.motion === 'calm' ? 2 : 1, view.motion !== 'still');
    const mood = look.step(dt, heard, view.motion, off);
    if (!budget.wet) mood.wet = 0;
    // Calm's mountains follow the music with a time constant, not a share of a frame
    const calm = view.motion === 'calm';
    const follow = calm ? 1 - Math.exp(-dt / CALM_RIDGE) : 1;
    for (let i = 0; i < BANDS; i++) slow[i] += (ridge[i] - slow[i]) * follow;
    if (running && silent(cols)) quietSince ??= seconds;
    else quietSince = null;
    const bass = bassOf(cols);
    shown = events.step(seconds, {
      bass,
      highs: highsOf(cols),
      on: view.motion === 'full' && running && !dull,
      spin: calm ? 0.5 : 1,
    });
    const period = model.beats.period;
    const live = period && ms - model.beats.seenAt <= 1000;
    floor.advance(dt, rowRate(live ? 60000 / period : null) * mood.cruise, cols, view.motion === 'full' && !dull);
    geo.v = geo.h - covered;
    geo.H = geo.v * horizonOf();
    embers.step(dt, {
      strength: dull ? 0 : mood.embers, crossings, w: geo.w, h: geo.h, H: geo.H,
    });
    swell = driftSwell(swell, dt, view.motion);
    tune.step(dt, ms);
    const kick = dull ? 0 : Math.max(0, 1 - (ms - kickAt) / KICK_MS);
    const frame = {
      t: seconds,
      kick,
      dull,
      flat: flatMood(kind),
      veil: kind === 'lost',
      // no music to follow: the mountains take the tone curve's shape and breathe (not in Still); a viewer who
      // may not write has them rest at the baseline, as they never turn into the curve for a guest
      nocap: kind === 'nocap' ? { curve: inertOf(core.model.page()) ? null : tune.curve(), breath: view.motion === 'still' ? 0 : 1 } : null,
      bass,
      rays: shown.rays,
      raysAngle: shown.raysAngle,
      stars: shown.stars,
      tune: tune.frame(), swell, look: mood,
    };
    paint(frame);
    perf.draws++;
    const took = clock() - began;
    stats.add(took, clock());
    const changed = budget.sample(perf.forcedMs ?? took, seconds, perf.asked === 0);
    if (budget.step !== keptStep || budget.capped !== keptCapped) {
      keptStep = budget.step;
      keptCapped = budget.capped;
      core.prefs?.set('tier', budget.tier);
    }
    if (changed) {
      // resizing blanks the canvases: paint again at once, not at the next draw
      fit();
      paint(frame);
    }
  }

  /** The least ms between draws now: the level's, and 30 a second at most while silent or not playing. */
  function pace() {
    const playing = core.model.page().host?.track?.status === 'playing';
    const idle = quietSince !== null && drawnAt - quietSince >= SILENT_FOR;
    // the mountains being tuned are drawn at the screen's rate, whatever the level
    const asked = tune.active() ? 0 : Math.max(dullMood(kindNow()) ? STILL_PACE : 0, paceOf({
      motion: view.motion, silent: idle, playing, coarse,
    }));
    // a scene that is slow at the least size is drawn at 30 a second at most, whatever is asked
    perf.asked = asked;
    budget.setIdle(idle || !playing);
    // choosing Full from a slower level: the scene's cost is found out in about a second, from the cap lifted
    if (view.motion === 'full' && motionWas !== 'full') budget.probe();
    motionWas = view.motion;
    perf.pace = paceUnder({ asked, capped: budget.capped, coarse });
    return perf.pace;
  }

  const loop = core.lifecycle.canvasLoop({
    canvas: roadCanvas,
    draw,
    resize,
    reduced: 'theme',
    pace,
    watchVisible: (options) => core.lifecycle.watchVisible({ ...options, onShow: open, onHide: close }),
    matchMedia: (query) => globalThis.matchMedia(query),
  });
  loop.start();
  // a mount that fails after this ends the loop and lets the analysis go, though no unmount follows
  signal.addEventListener('abort', () => { loop.stop(); close(); }, { once: true });

  view.tune = tune;

  return {
    loop,
    floor,
    /** For the checks: the draw time the budget measures, whatever the browser takes (null: the real one). */
    forceDrawMs(ms) {
      perf.forcedMs = ms;
    },
    /** For the checks: sixty slow draws, which step the budget down (the wet road goes first). */
    forceStepDown() {
      for (let i = 0; i < 60; i++) budget.sample(30, 1e6 + i / 60, true);
    },
    cols,
    model,
    mood: () => {
      const kind = kindNow();
      return { mood: kind, dull: dullMood(kind), flat: flatMood(kind) };
    },
    ear,
    look,
    embers,
    tune,
    perf: () => {
      const read = stats.read(clock());
      return {
        draws: perf.draws,
        mirrored: perf.mirrored,
        drawMs: read.mean,
        p95: read.p95,
        fps: read.fps,
        pace: perf.pace,
        scale: budget.scale,
        capped: budget.capped,
        step: budget.step,
        wet: budget.wet,
      };
    },
    sky: () => ({
      log: events.log,
      rays: shown.rays,
      stars: shown.stars,
      levels: { ...events.levels(), ear: { ...ear.read() } },
    }),
    stop() {
      loop.stop();
      close();
    },
  };
}
