/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Void Garden's scene wire and rig (themes/voidgarden/scene-wire.js,
// motion.js, gpu.js's pure parts, layers.js's statistics) over a fake core and a fake renderer:
// the analysis held only while the scene shows, a `step` sent after each draw, the daemon clock
// handed to the cues (a weak reading turns stale with no new facts), `changed` from the
// computer's changes alone (a DAC at -20 dB is not a changed path), the draws sampling the budget
// and a forced draw time stepping the ladder, the still and its quiet line with no GPU, the rig's
// time constants (the same easing at 30, 60 and 144 steps a second), the accents Full's, and the
// theme's files holding no `Points` object, `Math.random` or `Date.now`. A jsdom page (apt
// `node-jsdom`). Run: node voidgarden-scene.mjs
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { page as domPage } from './dom.mjs';
import { computerOf } from '../../../bins/svolderd/web/core/path-words.js';
import { daemonNow } from '../../../bins/svolderd/web/core/clock.js';
import {
  POLL, createGpu, disposeAfterFrames, flushAfterCompute, tierReason, tierWords,
} from '../../../bins/svolderd/web/themes/voidgarden/gpu.js';
import {
  PROBE, decide, fastest, keepPick, keptPick, pickKey, pickTier, rendererOf, sizeClass,
} from '../../../bins/svolderd/web/themes/voidgarden/tier-probe.js';
import { BUDGET } from '../../../bins/svolderd/web/themes/voidgarden/budget.js';
import { palette as paletteOf } from '../../../bins/svolderd/web/themes/voidgarden/palette.js';
import { frameStats } from '../../../bins/svolderd/web/themes/voidgarden/layers.js';
import { createRig, keepFrame, motionOf, paceOf } from '../../../bins/svolderd/web/themes/voidgarden/motion.js';
import {
  NO_GPU_WORDS, createFeed, spectrumOf, wireScene,
} from '../../../bins/svolderd/web/themes/voidgarden/scene-wire.js';
import { BANDS } from '../../../bins/svolderd/web/core/viz-model.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

const doc = JSON.parse(readFileSync(new URL('./fixtures/voidgarden-pages.json', import.meta.url)));
const PAGES = doc.pages;
const clone = (value) => JSON.parse(JSON.stringify(value));

/** A fake core over `model` (a function giving the page), a loop that stores what the wire gave
 * it, an analysis that counts its holders, and prefs held in a map. */
function rig(model, { prefs = {} } = {}) {
  const p = domPage('<div id="voidgarden"><canvas id="vg-scene"></canvas><div id="vg-hud"></div></div>');
  const seen = { holders: 0, opened: 0, loop: null, watch: null, set: [] };
  const control = new AbortController();
  const core = {
    prefs: {
      get: (key, fallback) => (key in prefs ? prefs[key] : fallback),
      set: (key, value) => { prefs[key] = value; seen.set.push([key, value]); },
    },
    model: { page: model },
    computer: { side: () => 'device' },
    lifecycle: {
      canvasLoop: (options) => {
        seen.loop = options;
        return { start() { options.watchVisible({ element: options.canvas, onVisible() {} }); }, stop() {}, running: false, reduced: false };
      },
      watchVisible: (options) => { seen.watch = options; return { stop() {} }; },
    },
    analysis: {
      open: (listeners) => {
        seen.holders++;
        seen.opened++;
        seen.listeners = listeners;
        return { close() { seen.holders--; } };
      },
    },
  };
  const events = [];
  const steps = new EventTarget();
  steps.addEventListener('step', (event) => events.push(event.detail));
  const vg = {
    page: p.document.getElementById('voidgarden'), core, signal: control.signal, prefs: core.prefs, steps,
  };
  return {
    ...p, core, seen, events, vg, control, prefs,
    /** Run the wire; the GPU is offered (a WebGL2 context) when `gpu` is given. */
    wire(seams) {
      if (seams?.createGpu) p.window.WebGL2RenderingContext = function WebGL2RenderingContext() {};
      return wireScene(vg, seams);
    },
    /** One draw at `seconds`, `dt` after the last. */
    draw: (dt, seconds) => seen.loop.draw(dt, seconds),
  };
}

/** A renderer and layers that note what they were asked, in place of the vendored three. `clock`
 * (`{ now }`, ms) is moved by `cost.ms` at each draw; `gate` holds the renderer back until it opens. */
function fakes({ tier = 'webgl2', fail = false, clock = null, gate = null } = {}) {
  const log = { sizes: [], updates: [], draws: 0, builds: [], disposed: 0, reads: 0, audio: [], made: [] };
  const cost = { ms: 0 };
  const gpu = {
    tier,
    caps: { tier, timestamps: false },
    size: (w, h) => log.sizes.push([w, h]),
    target: () => ({ width: 160, height: 100 }),
    dispose: () => { log.disposed++; },
  };
  const sum = (list) => (list ? Array.from(list).reduce((a, c) => a + c, 0) : 0);
  return {
    log,
    cost,
    gpu,
    seams: {
      createGpu: async () => {
        if (gate) await gate;
        if (fail) throw new Error('no context');
        return gpu;
      },
      makeLayers: (_, options = {}) => {
        const layers = {
          seed: options.seed ?? 0,
          ctx: { audio: { set: (audio) => log.audio.push(audio) } },
          update: (frame) => log.updates.push(frame),
          draw: () => { log.draws++; if (clock) clock.now += cost.ms; },
          refit: () => { log.refits = (log.refits ?? 0) + 1; },
          build: (seed) => { layers.seed = seed; log.builds.push(seed); },
          readStats: async () => { log.reads++; return { lit: 1, sum: sum(log.audio.at(-1)?.left) }; },
          dispose: () => { log.disposed++; },
        };
        log.made.push(options.seed ?? 0);
        return layers;
      },
    },
  };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

await ok('the_analysis_is_held_while_the_scene_shows_and_let_go_when_it_does_not_and_on_unmount', async () => {
  const r = rig(() => PAGES.base);
  r.wire();
  assert.equal(r.seen.holders, 0, 'nothing is held before the scene shows');
  r.seen.watch.onShow();
  assert.equal(r.seen.holders, 1);
  r.seen.watch.onShow();
  assert.equal(r.seen.holders, 1, 'one holder, not one per show');
  r.seen.watch.onHide();
  assert.equal(r.seen.holders, 0, 'off show for the core\'s grace: let go');
  r.seen.watch.onShow();
  assert.equal(r.seen.holders, 1);
  r.control.abort();
  assert.equal(r.seen.holders, 0, 'unmounting lets it go at once');
});

await ok('a_step_is_sent_after_each_draw_with_the_features_the_cues_and_the_history', async () => {
  const r = rig(() => PAGES.base);
  r.wire();
  await tick();
  r.draw(0.1, 1);
  r.draw(0.1, 2);
  assert.equal(r.events.length, 2);
  const d = r.events[1];
  for (const key of ['t', 'dt', 'features', 'cues', 'section', 'events', 'director', 'history', 'palette', 'seed']) {
    assert.ok(key in d, key);
  }
  assert.equal(d.section, 'intro');
  assert.equal(typeof d.features.bass, 'number');
  assert.equal(d.cues.mute, false);
  assert.ok(d.t > r.events[0].t, 'the clock is the sum of the dt handed in');
  near(d.t, 0.2, 1e-9, 'the director clock');
});

await ok('with_no_gpu_the_scene_is_the_still_and_the_quiet_line_and_the_feed_goes_on_at_still_pace', async () => {
  const r = rig(() => PAGES.base);
  const wired = r.wire();
  await tick();
  assert.equal(wired.driver.tier(), 'none');
  const still = r.document.getElementById('vg-still');
  assert.ok(still, 'the still is in the page');
  assert.ok(still.querySelector('svg'), 'a valley drawn as SVG');
  assert.equal(r.document.getElementById('vg-quiet').textContent, NO_GPU_WORDS);
  assert.equal(r.seen.loop.pace(), 1000, 'the Still pace');
  r.draw(1, 1);
  assert.equal(r.events.length, 1, 'a step still goes to the HUD and the panel');
  assert.equal(await wired.driver.pixels(), null);
  assert.equal(await wired.driver.frame(1, 0), null);
  r.control.abort();
  assert.equal(r.document.getElementById('vg-still'), null, 'unmount takes the still away');
});

await ok('a_renderer_that_fails_gives_the_still_too', async () => {
  const f = fakes({ fail: true });
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  assert.equal(wired.driver.tier(), 'none');
  assert.ok(r.document.getElementById('vg-still'));
});

await ok('the_driver_answers_the_tier_once_the_renderer_is_up_and_a_forced_draw_time_is_taken', async () => {
  const f = fakes({ tier: 'webgpu' });
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  assert.equal(wired.driver.tier(), null, 'pending until the renderer answers');
  await tick();
  assert.equal(wired.driver.tier(), 'webgpu');
  assert.equal(wired.driver.forceDrawMs(20), true);
  assert.equal(r.document.getElementById('vg-still'), null, 'a GPU scene has no still');
  r.seen.loop.resize(1280, 800, 2);
  assert.deepEqual(f.log.sizes.at(-1), [2240, 1400], 'the backing store at density 1.75');
});

await ok('each_draw_samples_the_budget_and_a_forced_draw_time_steps_it_down_then_back_up', async () => {
  const f = fakes();
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  r.seen.loop.resize(1280, 800, 2);
  wired.driver.forceDrawMs(28);
  let seconds = 0;
  const frames = () => {
    for (let i = 0; i < 60 * 9; i++) { seconds += 1 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); }
  };
  for (let i = 0; i < 6; i++) frames();
  const down = wired.driver.perf();
  assert.ok(down.step >= 14, `stepped down to ${down.step}`);
  assert.equal(down.grit, false);
  assert.equal(f.log.updates.at(-1).budget.grit, false, 'the layers are told');
  assert.ok(f.log.updates.at(-1).budget.q < 1);
  assert.ok(f.log.updates.at(-1).budget.rings < 1, 'the land is told its ring share');
  assert.deepEqual(r.prefs.tier, { v: BUDGET.version, key: 'webgl2||35', step: down.step, fps: 30 }, 'the tier that held is kept, keyed by the tier, the GPU and the size class');
  wired.driver.forceDrawMs(2);
  for (let i = 0; i < 60; i++) frames();
  assert.equal(wired.driver.perf().step, 0, 'and back up');
  assert.equal(wired.driver.perf().grit, true);
});

await ok('the_cues_get_the_daemons_clock_from_the_hello_so_a_weak_reading_turns_stale_with_no_new_facts', async () => {
  const page = clone(PAGES.weak_signal);
  const base = 9e11;
  page.device.facts.now_ms = base;
  page.device.facts.signal_age.heard_at_ms = base - 10_000;
  // another caller of the page-wide table saw this value first, on the wall clock
  assert.equal(daemonNow(base, 1.7e12), base);
  const r = rig(() => page);
  r.wire({ clock: () => 0 });
  r.seen.watch.onShow();
  r.seen.listeners.hello({ now_ms: base });
  r.draw(0.1, 1);
  assert.equal(r.events.at(-1).cues.weak, true, 'a 10 s old reading of -83 dBm is weak');
  r.draw(0.1, 60);
  assert.equal(r.events.at(-1).cues.weak, true, 'a minute on it is still fresh');
  r.draw(0.1, 125);
  assert.equal(r.events.at(-1).cues.weak, false, 'two minutes on, with no new facts, the reading is old');
  page.device.facts.now_ms = base + 126_000;
  page.device.facts.signal_age.heard_at_ms = base + 125_000;
  r.draw(0.1, 126);
  assert.equal(r.events.at(-1).cues.weak, true, 'a new reading is fresh again');
  assert.equal(daemonNow(base, 1.7e12 + 5000), base + 5000, 'the other caller\'s sight is as it was');
  assert.equal(daemonNow(base + 7, 40), base + 7, 'and the scene left none of its own');
});

await ok('the_budget_times_the_render_alone_not_the_feed_step_nor_the_step_listeners', async () => {
  const clk = { now: 0 };
  const f = fakes({ clock: clk });
  const r = rig(() => PAGES.base);
  const wired = r.wire({ ...f.seams, clock: () => clk.now });
  await tick();
  r.seen.loop.resize(1280, 800, 2);
  r.vg.steps.addEventListener('step', () => { clk.now += 20; });
  let seconds = 0;
  const frames = (count) => {
    for (let i = 0; i < count; i++) { seconds += 1 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); }
  };
  frames(60 * 12);
  assert.equal(wired.driver.perf().ms, 0, 'a listener that takes 20 ms a step is not the draw');
  assert.equal(wired.driver.perf().step, 0, 'the budget did not move');
  f.cost.ms = 20;
  frames(60 * 3);
  assert.equal(wired.driver.perf().ms, 20, 'a render that takes 20 ms is');
  assert.ok(wired.driver.perf().step >= 2, `and it steps the budget down (${wired.driver.perf().step})`);
});

await ok('the_gpu_timers_reading_is_the_budgets_and_a_scale_change_refits_the_layers', async () => {
  const f = fakes();
  const calls = [];
  let reading = 40;
  f.gpu.timer = {
    begin: () => calls.push('begin'),
    end: () => calls.push('end'),
    read: () => reading,
  };
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  r.seen.loop.resize(1920, 1080, 1);
  let seconds = 0;
  const frames = (count) => {
    for (let i = 0; i < count; i++) { seconds += 1 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); }
  };
  frames(2);
  assert.deepEqual(calls.slice(0, 2), ['begin', 'end'], 'each draw is timed, update to draw');
  assert.equal(wired.driver.perf().gpuMs, 40, 'the reading is the driver\'s');
  frames(60 * 2);
  assert.ok(wired.driver.perf().step >= 1, 'a 40 ms GPU frame steps the budget down with a fast CPU');
  const refits = f.log.refits ?? 0;
  wired.driver.hold(wired.driver.perf().rungs, 30);
  assert.ok(f.log.refits > refits, 'the frame\'s scale changed (its last resort): the layers take the new size');
  assert.equal(wired.driver.hold(0, 60), true);
  assert.equal(wired.driver.perf().step, 0, 'a held step');
  frames(60 * 4);
  assert.equal(wired.driver.perf().step, 0, 'stays held');
  reading = null;
  f.gpu.timer.read = () => null;
  frames(2);
  assert.equal(wired.driver.perf().gpuMs, 40, 'no new reading leaves the last');
});

await ok('a_webgpu_timestamp_reading_is_one_frames_time_not_a_share_of_the_draws_since_the_last', async () => {
  // three's timestamp pool answers the total of the last frame it timed (vendor three.webgpu.js,
  // WebGPUTimestampQueryPool): 20 ms is one frame's 20 ms, however many draws came since the last read
  const f = fakes({ tier: 'webgpu' });
  f.gpu.caps = { tier: 'webgpu', timestamps: true };
  let asked = 0;
  f.gpu.gpuMs = async () => { asked++; return 20; };
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  r.seen.loop.resize(1920, 1080, 1);
  let seconds = 0;
  for (let i = 0; i < 16; i++) { seconds += 1 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); await tick(); }
  assert.ok(asked >= 1, 'the timestamps were read');
  assert.equal(wired.driver.perf().gpuMs, 20, 'the frame\'s time, not 20 / 8');
});

await ok('the_first_frame_compiles_before_it_draws_and_a_failed_compile_still_draws', async () => {
  for (const outcome of ['resolve', 'reject']) {
    const f = fakes();
    let settle = null;
    let compiled = 0;
    f.gpu.compile = () => { compiled++; return new Promise((ok2, no) => { settle = outcome === 'resolve' ? ok2 : no; }); };
    const r = rig(() => PAGES.base);
    const wired = r.wire(f.seams);
    await tick();
    r.seen.loop.resize(1280, 800, 1);
    let seconds = 0;
    const frames = (n) => { for (let i = 0; i < n; i++) { seconds += 1 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); } };
    frames(5);
    assert.equal(compiled, 1, `${outcome}: one compile`);
    assert.equal(f.log.draws, 0, `${outcome}: no draw while it runs`);
    assert.equal(f.log.updates.length, 1, `${outcome}: the first update (the land cache's targets) came before it`);
    settle();
    await tick();
    await tick();
    frames(3);
    assert.equal(f.log.draws, 3, `${outcome}: the draws begin once it settled`);
    assert.ok(wired.driver.perf().load.compiled !== null, `${outcome}: the stamp says when`);
  }
});

await ok('a_gpu_reading_that_stopped_coming_expires_and_a_held_step_is_not_kept', async () => {
  const clk = { now: 0 };
  const f = fakes({ clock: clk });
  let reading = 40;
  f.gpu.timer = { begin() {}, end() {}, read: () => reading };
  const r = rig(() => PAGES.base);
  const wired = r.wire({ ...f.seams, clock: () => clk.now });
  await tick();
  r.seen.loop.resize(1920, 1080, 1);
  let seconds = 0;
  const frames = (count) => {
    for (let i = 0; i < count; i++) { seconds += 1 / 60; clk.now += 1000 / 60; r.draw(1 / 60, seconds); r.seen.loop.pace(); }
  };
  frames(2);
  assert.equal(wired.driver.perf().gpuMs, 40);
  reading = null;
  frames(60);
  assert.equal(wired.driver.perf().gpuMs, 40, 'a second on, the last reading stands');
  frames(90);
  assert.equal(wired.driver.perf().gpuMs, null, 'after about 2 s with none it is gone: no stale value walks the ladder');
  const before = r.prefs.tier;
  wired.driver.hold(5, 30);
  frames(5);
  assert.equal(wired.driver.perf().step, 5);
  assert.deepEqual(r.prefs.tier, before, 'a held step (the checks, the measure) is not written as the browser\'s tier');
});

await ok('the_kept_tier_applies_for_the_same_gpu_and_size_class_and_not_for_another', async () => {
  const tierFor = (device) => ({ v: BUDGET.version, key: `webgl2|${device}|35`, step: 6, fps: 30 });
  for (const [device, want] of [['amd', 6], ['nvidia', 0]]) {
    const f = fakes();
    f.gpu.caps = { tier: 'webgl2', timestamps: false, device: 'amd' };
    const r = rig(() => PAGES.base, { prefs: { tier: tierFor(device) } });
    const wired = r.wire(f.seams);
    await tick();
    r.seen.loop.resize(1280, 800, 2);
    assert.equal(wired.driver.perf().step, want, `${device}: ${want}`);
  }
});

await ok('the_webgl2_query_timer_reads_finished_frames_drops_disjoint_ones_and_keeps_8_in_flight', async () => {
  const { createQueryTimer } = await import('../../../bins/svolderd/web/themes/voidgarden/gpu.js');
  const ext = { TIME_ELAPSED_EXT: 1, GPU_DISJOINT_EXT: 2 };
  const done = new Set();
  const results = new Map();
  let disjoint = false;
  let made = 0;
  let open = 0;
  const gl = {
    QUERY_RESULT_AVAILABLE: 10,
    QUERY_RESULT: 11,
    getExtension: (name) => (name === 'EXT_disjoint_timer_query_webgl2' ? ext : null),
    createQuery: () => ({ id: ++made }),
    beginQuery: () => { open++; assert.equal(open, 1, 'one query open at a time'); },
    endQuery: () => { open--; },
    deleteQuery: () => {},
    getParameter: () => disjoint,
    getQueryParameter: (q, what) => (what === 10 ? done.has(q.id) : results.get(q.id)),
  };
  assert.equal(createQueryTimer({ getExtension: () => null }), null, 'no extension, no timer');
  const t = createQueryTimer(gl);
  for (let i = 0; i < 12; i++) { t.begin(); t.end(); }
  assert.equal(made, 8, 'at most 8 in flight');
  assert.equal(t.read(), null, 'nothing finished yet');
  results.set(1, 10e6);
  results.set(2, 20e6);
  done.add(1);
  done.add(2);
  assert.equal(t.read(), 15, 'the mean of what finished, in ms');
  assert.equal(t.read(), null, 'and read once');
  results.set(3, 99e6);
  done.add(3);
  disjoint = true;
  assert.equal(t.read(), null, 'a disjoint reading is dropped');
  disjoint = false;
  t.begin();
  t.end();
  assert.equal(made, 9, 'room again after the reads');
});

/** A frame event's text: both sides every band at `level`. */
const liveFrame = (level) => {
  const bands = btoa(String.fromCharCode(...new Array(BANDS).fill(level)));
  return JSON.stringify({ t: 1, l: bands, r: bands, lv: [100, 100, 100, 100] });
};

await ok('a_frame_of_a_seed_sets_its_own_audio_and_reads_the_same_after_the_live_loop_played', async () => {
  const f = fakes();
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  const before = await wired.driver.frame(7, 2);
  const again = await wired.driver.frame(7, 2);
  assert.deepEqual(before, again, 'the same seed and time, the same statistics');
  // the live loop plays bands
  r.seen.watch.onShow();
  r.seen.listeners.hello({ now_ms: 1000 });
  r.seen.listeners.capture({ capture: 'running', sink: 'hdmi', why: null });
  let at = 0;
  for (let i = 0; i < 40; i++) {
    at += 0.05;
    r.seen.listeners.frame(liveFrame(200));
    r.draw(0.05, at);
  }
  const live = f.log.audio.at(-1);
  assert.ok(live.left.some((v) => v > 0), 'the live loop sent bands');
  const after = await wired.driver.frame(7, 2);
  assert.deepEqual(after, before, 'a frame is not the live loop\'s last bands');
  assert.equal(f.log.audio.at(-1).left.some((v) => v > 0), true, 'the live audio is put back');
});

await ok('the_first_tracks_seed_reaches_the_rig_and_the_layers_whichever_comes_up_first', async () => {
  // the renderer first: the layers are built for seed 0 and told at the first step
  const f = fakes();
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  r.draw(0.1, 1);
  const seed = r.events.at(-1).seed;
  assert.notEqual(seed, 0);
  assert.deepEqual(wired.driver.perf().seeds, { rig: seed, layers: seed });
  // the first track first: the layers are built for it
  let open = null;
  const gate = new Promise((resolve) => { open = resolve; });
  const g = fakes({ gate });
  const q = rig(() => PAGES.base);
  const late = q.wire(g.seams);
  await tick();
  q.draw(0.1, 1);
  assert.equal(late.driver.perf().seeds.rig, seed, 'the rig has it before any renderer');
  open();
  await tick();
  assert.equal(late.driver.perf().seeds.layers, seed, 'the layers are built for it');
  assert.equal(g.log.made[0], seed);
});

/** The 32x32 bytes a decode of a cover gives. */
const coverOf = (colourAt) => {
  const px = new Uint8ClampedArray(32 * 32 * 4);
  for (let y = 0; y < 32; y++) {
    for (let x = 0; x < 32; x++) px.set([...colourAt(x, y), 255], (y * 32 + x) * 4);
  }
  return px;
};
const TWO = coverOf((x) => (x < 20 ? [210, 40, 50] : [40, 70, 220]));
const GREY = coverOf(() => [120, 120, 120]);

await ok('the_palette_comes_from_the_cover_and_falls_back_to_the_seeded_pair', async () => {
  const page = clone(PAGES.base);
  const asked = [];
  const covers = { abc: TWO, grey: GREY };
  const f = fakes();
  const r = rig(() => page);
  r.wire({ ...f.seams, decodeCover: async (url) => { asked.push(url); return covers[url.split('=')[1]] ?? null; } });
  await tick();
  const seeded = paletteOf(null, { title: 'Song', artist: 'Artist' });
  const cover = paletteOf(TWO, { title: 'Song', artist: 'Artist' });
  assert.notDeepEqual(cover, seeded, 'the fixture is not the seeded pair');
  r.draw(0.1, 1);
  assert.deepEqual(r.events.at(-1).palette, seeded, 'the seeded pair until the cover is drawn');
  await tick();
  r.draw(0.1, 2);
  r.draw(0.1, 3);
  assert.deepEqual(r.events.at(-1).palette, cover, 'the cover\'s two colours');
  assert.deepEqual(asked, ['api/v1/host/cover?v=abc'], 'one same-origin request, not one per step');
  page.host.track.cover = 'grey';
  r.draw(0.1, 4);
  await tick();
  r.draw(0.1, 5);
  assert.deepEqual(r.events.at(-1).palette, seeded, 'a grey cover: the seeded pair');
  page.host.track.cover = 'abc';
  r.draw(0.1, 6);
  await tick();
  r.draw(0.1, 7);
  assert.deepEqual(r.events.at(-1).palette, cover);
  page.host.track.cover = null;
  r.draw(0.1, 8);
  r.draw(0.1, 8.1);
  assert.deepEqual(r.events.at(-1).palette, seeded, 'the cover gone: the seeded pair, no request');
  assert.equal(asked.length, 3);
  page.host.track.cover = 'missing';
  r.draw(0.1, 9);
  await tick();
  r.draw(0.1, 10);
  assert.deepEqual(r.events.at(-1).palette, seeded, 'a cover that does not load');
});

await ok('a_cover_that_arrives_after_the_track_changed_is_not_used', async () => {
  const page = clone(PAGES.base);
  let release = null;
  const f = fakes();
  const r = rig(() => page);
  r.wire({ ...f.seams, decodeCover: () => new Promise((resolve) => { release = () => resolve(TWO); }) });
  await tick();
  r.draw(0.1, 1);
  page.host.track.headline = 'Other';
  page.host.track.cover = null;
  r.draw(0.1, 2);
  release();
  await tick();
  r.draw(0.1, 3);
  assert.deepEqual(r.events.at(-1).palette, paletteOf(null, { title: 'Other', artist: 'Artist' }));
});

await ok('the_tier_is_told_in_words_with_its_reason_and_the_vendor', async () => {
  assert.equal(tierReason({ forced: true }), 'chosen');
  assert.equal(tierReason({ forced: true, hasApi: false, secure: false }), 'chosen');
  assert.equal(tierReason({ hasApi: false, secure: false }), 'not a secure context');
  assert.equal(tierReason({ hasApi: false, secure: true }), 'no adapter');
  assert.equal(tierReason({ hasApi: true, secure: true }), 'no adapter');
  assert.equal(tierWords({ tier: 'webgpu', vendor: 'amd' }), 'WebGPU · amd');
  assert.equal(tierWords({ tier: 'webgpu', vendor: '' }), 'WebGPU');
  assert.equal(tierWords({ tier: 'webgl2', reason: 'no adapter' }), 'WebGL2 · no adapter');
  assert.equal(tierWords({ tier: 'webgl2', reason: 'chosen' }), 'WebGL2 · chosen');
  assert.equal(tierWords(null), null);
  const f = fakes({ tier: 'webgpu' });
  f.gpu.caps = { tier: 'webgpu', vendor: 'amd' };
  const r = rig(() => PAGES.base);
  const wired = r.wire(f.seams);
  await tick();
  assert.equal(wired.driver.perf().words, 'WebGPU · amd', 'the driver gives it to the debug readout');
});

await ok('the_detail_pref_reaches_the_budget_at_the_start_and_at_run_time', async () => {
  const f = fakes();
  const r = rig(() => PAGES.base, { prefs: { detail: 'low' } });
  const wired = r.wire(f.seams);
  await tick();
  r.seen.loop.resize(1280, 800, 2);
  assert.equal(wired.driver.perf().detail, 'low');
  assert.equal(wired.driver.perf().step, wired.driver.perf().rungs, 'Low is the whole ladder');
  r.prefs.detail = 'high';
  r.draw(0.1, 1);
  r.seen.loop.pace();
  r.draw(0.1, 2);
  assert.equal(wired.driver.perf().detail, 'high', 'read again at run time');
  assert.equal(wired.driver.perf().step, 0);
  assert.equal(r.prefs.tier, undefined, 'a pinned Detail is not kept as the held tier');
});

await ok('changed_is_the_computers_changes_and_a_dac_at_minus_20_db_is_not_changed', async () => {
  const page = clone(PAGES.base);
  page.device.playing.path.hops = [
    { label: 'computer', detail: 'bit-for-bit', changes: false },
    { label: 'DAC', detail: 'volume -20.0 dB', changes: true },
  ];
  page.device.playing.path.verdict = 'changed';
  assert.equal(computerOf(page).changes, false);
  const r = rig(() => page);
  r.wire();
  r.draw(0.1, 1);
  assert.equal(r.events.at(-1).cues.changed, false, 'nothing changed on the computer: no haze');
  page.device.playing.path.hops[0].changes = true;
  r.draw(0.1, 2);
  assert.equal(r.events.at(-1).cues.changed, true, 'the computer changes the sound: haze');
});

await ok('a_new_track_reseeds_through_the_fade_and_the_first_track_starts_at_once', async () => {
  const page = clone(PAGES.base);
  page.host.track.headline = 'First';
  const r = rig(() => page);
  r.wire();
  r.draw(0.1, 1);
  const first = r.events.at(-1);
  assert.notEqual(first.seed, 0, 'the first track seeds the scene with no fade');
  assert.equal(first.director.fadeDir, 0);
  page.host.track.headline = 'Second';
  r.draw(0.1, 2);
  assert.equal(r.events.at(-1).director.fadeDir, -1, 'fading out');
  let at = 2;
  while (!r.events.some((e) => e.events.some((x) => x.type === 'reseed')) && at < 10) { at += 0.1; r.draw(0.1, at); }
  const reseed = r.events.flatMap((e) => e.events).find((x) => x.type === 'reseed');
  assert.ok(reseed, 'the new seed is taken at black');
  assert.notEqual(reseed.seed, first.seed);
  assert.notDeepEqual(r.events.at(-1).palette, first.palette === undefined ? null : first.palette, 'a palette is chosen');
});

await ok('a_hidden_page_empties_the_feed_and_a_capture_that_is_not_running_is_silent', async () => {
  const feed = createFeed();
  assert.equal(feed.running, false);
  feed.capture({ capture: 'running', sink: 'hdmi', why: null });
  assert.equal(feed.running, true);
  feed.capture({ capture: 'unavailable', sink: null, why: 'no sound' });
  assert.equal(feed.running, false);
  const s = feed.step(0.1, 100, { page: PAGES.base });
  assert.equal(s.silent, true);
  assert.equal(s.cues.pause, false, 'two seconds of silence are needed for the drift');
  let last = s;
  for (let i = 0; i < 25; i++) last = feed.step(0.1, 200 + i * 100, { page: PAGES.base });
  assert.equal(last.cues.pause, true, 'silent frames for 2 s: the slow drift');
});

await ok('the_streams_own_hello_and_capture_events_let_its_frames_move_the_features', async () => {
  // the events as bins/svolderd/src/web/host/frames.rs writes them: `capture` names the state
  const feed = createFeed();
  const hello = { now_ms: 1000, capture: 'starting', sink: null, why: null };
  feed.hello(hello.now_ms, 0);
  feed.capture(hello);
  feed.capture({ capture: 'running', sink: 'Radeon (HDMI)', why: null });
  assert.equal(feed.running, true, 'the stream’s running capture is running');
  let s = null;
  for (let i = 1; i <= 30; i++) {
    feed.push(liveFrame(200), i * 33);
    s = feed.step(0.033, i * 33, { page: PAGES.base });
  }
  assert.equal(s.running, true);
  assert.equal(s.silent, false, 'the frames carry sound');
  assert.ok(s.raw.bass > 0.5 && s.raw.loud > 0, 'the frames reach the raw features');
  assert.ok(s.features.bass > 0.5, 'and the smoothed ones the panel reads');
});

await ok('a_recording_of_the_real_hosts_stream_moves_the_features_whatever_the_players_status', async () => {
  // fixtures/voidgarden-host-frames.sse: `hello`, `capture` and 60 frames of a real host daemon (its
  // own PipeWire, music into a null sink), as the wire sent them. A real host's player may be paused
  // or stale while the sink plays: the sound heard must still reach the features and keep the drift
  const events = [];
  let name = null;
  for (const line of readFileSync(new URL('./fixtures/voidgarden-host-frames.sse', import.meta.url), 'utf8').split('\n')) {
    if (line.startsWith('event: ')) name = line.slice(7);
    else if (line.startsWith('data: ')) events.push([name, line.slice(6)]);
  }
  assert.equal(events.length, 62);
  for (const key of ['base', 'paused_player', 'stopped_player', 'no_player']) {
    const feed = createFeed();
    let ms = 0;
    let s = null;
    for (const [type, data] of events) {
      if (type === 'hello') { feed.hello(JSON.parse(data).now_ms, 0); feed.capture(JSON.parse(data)); }
      else if (type === 'capture') feed.capture(JSON.parse(data));
      else {
        ms += 33;
        feed.push(data, ms);
        s = feed.step(0.033, ms, { page: PAGES[key] });
      }
    }
    assert.equal(s.running, true, key);
    assert.equal(s.cues.pause, false, `${key}: sound is heard, so no pause`);
    assert.ok(s.features.mid > 0.2 && s.features.high > 0.2 && s.features.loud > 0.1, `${key}: ${JSON.stringify(s.features)}`);
  }
});

await ok('the_spectrum_is_64_log_bins_from_the_bands_and_a_lit_band_lights_its_own_bin', async () => {
  const left = new Float32Array(BANDS);
  const right = new Float32Array(BANDS);
  left[120] = 1;
  const bins = spectrumOf(left, right);
  assert.equal(bins.length, 64);
  const lit = [...bins].map((v, i) => [v, i]).filter(([v]) => v > 0);
  assert.ok(lit.length >= 1 && lit.length <= 2, 'one or two bins');
  near(lit[0][1], 32, 2, 'band 120 is the middle of the log axis');
  for (const v of bins) assert.ok(v >= 0 && v <= 1);
});

await ok('the_motion_levels_pace_and_reduced_motion_start_on_calm', async () => {
  assert.equal(paceOf({ motion: 'full' }), 0);
  assert.equal(paceOf({ motion: 'calm' }), 100);
  assert.equal(paceOf({ motion: 'still' }), 1000);
  assert.equal(paceOf({ motion: 'full', silent: true, coarse: true }), 1000 / 30);
  assert.equal(paceOf({ motion: 'full', silent: true, coarse: false }), 0);
  const prefs = (v) => ({ get: (k, f) => (k === 'motion' && v ? v : f) });
  assert.equal(motionOf(prefs(null), true), 'calm');
  assert.equal(motionOf(prefs(null), false), 'full');
  assert.equal(motionOf(prefs('still'), true), 'still');
  assert.equal(motionOf(prefs('nonsense'), false), 'full');
  assert.equal(keepFrame({ sinceMs: 5, frameMs: 8, paceMs: 33 }), false);
  assert.equal(keepFrame({ sinceMs: 30, frameMs: 8, paceMs: 33 }), true);
  assert.equal(rendererOf({ get: () => 'webgl2' }, ''), 'webgl2');
  assert.equal(rendererOf({ get: () => 'webgpu' }, ''), 'webgpu');
  assert.equal(rendererOf({ get: (k, f) => f }, '?vg-gpu=webgl2'), 'webgl2');
  assert.equal(rendererOf({ get: () => 'webgl2' }, '?vg-gpu=webgpu'), 'webgpu');
  assert.equal(rendererOf({ get: (k, f) => f }, '?vg-gpu=nonsense'), 'auto');
  assert.equal(rendererOf(undefined, ''), 'auto');
});

/** Steps a rig for `seconds` at `rate` steps a second with `input`. */
function spin(rate, seconds, input, seed = 7) {
  const r = createRig({ seed });
  const steps = Math.round(seconds * rate);
  for (let i = 0; i < steps; i++) r.step(1 / rate, typeof input === 'function' ? input(i / rate) : input);
  return r;
}

await ok('the_rigs_cue_easings_are_time_constants_the_same_at_30_60_and_144_steps_a_second', async () => {
  const input = { cues: { mute: true, changed: true, cross: true }, features: { loud: 0.6 }, aspect: 1.6 };
  const [a, b, c] = [30, 60, 144].map((rate) => spin(rate, 1.5, input));
  for (const k of ['frost', 'haze', 'cross']) {
    near(b.u[k], a.u[k], 1e-9, k);
    near(c.u[k], a.u[k], 1e-9, k);
    assert.ok(a.u[k] > 0.9, `${k} ends at its target`);
  }
  const run = [30, 144].map((rate) => spin(rate, 6, { cues: {}, features: { loud: 0.6 }, aspect: 1.6 }));
  near(run[1].cam.z / run[0].cam.z, 1, 0.01, 'the flight is the same distance');
  for (const r of run) near(r.u.time, 6, 1e-6, 'the scene clock is the seconds handed in at full time');
});

await ok('a_muted_scene_stops_its_clock_a_paused_one_drifts_and_calm_runs_at_half_time', async () => {
  const full = spin(60, 4, { cues: {}, aspect: 1.6 });
  const muted = spin(60, 4, { cues: { mute: true }, aspect: 1.6 });
  const paused = spin(60, 4, { cues: { pause: true }, aspect: 1.6 });
  const calm = spin(60, 4, { cues: {}, motion: 'calm', aspect: 1.6 });
  assert.ok(muted.u.time < 0.3 * full.u.time, 'muted: the clock all but stops');
  assert.ok(paused.u.time < 0.5 * full.u.time && paused.u.time > muted.u.time, 'paused: slow drift');
  near(calm.u.time / full.u.time, 0.5, 0.05, 'Calm: half the time scale');
});

await ok('the_accents_are_fulls_the_tear_and_the_static_are_off_in_calm', async () => {
  const drop = (i) => ({ cues: { linkDrop: i < 1 / 60 + 1e-9, weak: true }, aspect: 1.6 });
  let sawTear = false;
  let sawStatic = false;
  const full = createRig({ seed: 3 });
  const calm = createRig({ seed: 3 });
  for (let i = 0; i < 90; i++) {
    full.step(1 / 60, { ...drop(i / 60), motion: 'full' });
    calm.step(1 / 60, { ...drop(i / 60), motion: 'calm' });
    if (full.u.tear > 0) sawTear = true;
    if (full.u.static > 0.05) sawStatic = true;
    assert.equal(calm.u.tear, 0, 'no tear in Calm');
    assert.equal(calm.u.static, 0, 'no static in Calm');
  }
  assert.ok(sawTear && sawStatic, 'Full tears and crackles');
  // the same seed gives the same accents
  const again = createRig({ seed: 3 });
  const first = createRig({ seed: 3 });
  for (let i = 0; i < 90; i++) {
    again.step(1 / 60, { ...drop(i / 60), motion: 'full' });
    first.step(1 / 60, { ...drop(i / 60), motion: 'full' });
    assert.equal(again.u.tear, first.u.tear);
    assert.equal(again.u.static, first.u.static);
  }
});

await ok('the_accents_flicker_is_a_function_of_time_the_same_events_at_30_60_and_144_steps_a_second', async () => {
  /** Each grid cell's value of the static (over its level), the tear and the tear's row. */
  const cells = (rate) => {
    const r = createRig({ seed: 5 });
    const got = { static: new Map(), tear: new Map(), tearY: new Map() };
    const note = (kind, cell, value) => {
      if (got[kind].has(cell)) near(value, got[kind].get(cell), 1e-9, `${kind} in cell ${cell} at ${rate} a second`);
      else got[kind].set(cell, value);
    };
    for (let i = 0; i < Math.round(0.9 * rate); i++) {
      r.step(1 / rate, { cues: { weak: true, linkDrop: i === 0 }, aspect: 1.6 });
      const gt = r.u.gt;
      if (gt < 0.2) continue;
      note('static', Math.floor(gt * 30 + 1e-9), r.u.static / (1 - Math.exp(-2 * gt)));
      note('tear', Math.floor(gt * 24 + 1e-9), r.u.tear);
      note('tearY', Math.floor((gt * 1) / 0.09 + 1e-9), r.u.tearY);
    }
    return got;
  };
  const [a, b, c] = [30, 60, 144].map(cells);
  for (const kind of ['static', 'tear', 'tearY']) {
    let shared = 0;
    for (const other of [b, c]) {
      for (const [cell, value] of a[kind]) {
        if (other[kind].has(cell)) {
          near(other[kind].get(cell), value, 1e-9, `${kind} cell ${cell}`);
          shared++;
        }
      }
    }
    assert.ok(shared >= (kind === 'tearY' ? 8 : 12), `${kind}: ${shared} cells in common`);
  }
  assert.ok(new Set([...a.static.values()].map((v) => v.toFixed(6))).size > 8, 'the static flickers');
  assert.deepEqual([...new Set(a.tear.values())].sort(), [0.2, 1], 'the tear is on and in between');
});

await ok('the_camera_comes_nearer_with_the_volume_and_the_rig_is_a_function_of_seed_and_input', async () => {
  const quiet = spin(60, 6, { cues: { volume: 0 }, aspect: 1.6 });
  const loud = spin(60, 6, { cues: { volume: 1 }, aspect: 1.6 });
  assert.ok(loud.cam.dist < quiet.cam.dist - 0.4, 'a loud volume is nearer');
  const tall = spin(60, 6, { cues: { volume: 0.5 }, aspect: 0.5 });
  const wide = spin(60, 6, { cues: { volume: 0.5 }, aspect: 1.6 });
  assert.ok(tall.cam.dist > wide.cam.dist && tall.cam.fov > wide.cam.fov, 'a tall window stands back');
  const a = spin(60, 5, { cues: { weak: true }, aspect: 1.6 }, 11);
  const b = spin(60, 5, { cues: { weak: true }, aspect: 1.6 }, 11);
  assert.deepEqual(a.u, b.u);
  assert.deepEqual(a.cam, b.cam);
  const c = spin(60, 5, { cues: { weak: true }, aspect: 1.6 }, 12);
  assert.notEqual(c.u.static, a.u.static, 'another seed, another static');
});

await ok('a_readbacks_statistics_count_lit_pixels_and_their_places', async () => {
  const px = new Uint8Array(4 * 4 * 2);
  assert.deepEqual(frameStats(px, 4, 2), { w: 4, h: 2, mean: [0, 0, 0], luma: 0, lit: 0, places: 0 });
  px.set([255, 255, 255, 255], 4);
  px.set([0, 200, 0, 255], 20);
  const s = frameStats(px, 4, 2);
  assert.equal(s.lit, 2);
  assert.notEqual(s.places, 0);
  const moved = new Uint8Array(px.length);
  moved.set([255, 255, 255, 255], 12);
  moved.set([0, 200, 0, 255], 20);
  assert.notEqual(frameStats(moved, 4, 2).places, s.places, 'the same pixels in other places differ');
});

/** The theme's own files, vendor left out. */
function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    if (name === 'vendor') return [];
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return files(path);
    return name.endsWith('.js') ? [path] : [];
  });
}

await ok('the_theme_never_makes_a_points_object_and_has_no_random_or_wall_clock', async () => {
  const theme = new URL('../../../bins/svolderd/web/themes/voidgarden/', import.meta.url).pathname;
  const all = files(theme);
  assert.ok(all.length > 30);
  for (const path of all) {
    const code = readFileSync(path, 'utf8').replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
    assert.ok(!/\bnew\s+(?:[\w$]+\.)?Points\s*\(/.test(code), `${path}: a Points object is 1 px on every cell`);
    assert.ok(!/\b(?:THREE|T)\.Points\b/.test(code), `${path}: THREE.Points`);
    assert.ok(!/Math\.random/.test(code), `${path}: no random`);
    assert.ok(!/Date\.now|new Date\(/.test(code), `${path}: no wall clock`);
    // the scene's wire alone reads the page's clock (behind its seam); every other file is handed
    // the time. The one exception is the mount's single line that makes the clock it hands to the
    // dash's hands (held keys, idle fade, card seconds) and the readout.
    const clocks = code.match(/\bperformance\./g)?.length ?? 0;
    const allowed = path.endsWith('scene-wire.js') ? Infinity : path.endsWith('/voidgarden.js') ? 1 : 0;
    assert.ok(clocks <= allowed, `${path}: no clock of its own`);
    assert.ok(!/\b(?:eval|Function)\s*\(|new Worker|createObjectURL/.test(code), `${path}: no eval, worker or blob`);
    assert.ok(!/from\s+['"][^./]/.test(code), `${path}: no bare specifier (no importmap)`);
  }
});

await ok('every_webgl2_compute_pass_is_followed_by_a_gl_flush_and_the_webgpu_tier_has_none', async () => {
  for (const tier of ['webgl2', 'webgpu']) {
    const order = [];
    class Renderer {
      constructor() {
        this.backend = tier === 'webgl2' ? { isWebGPUBackend: false, gl: { flush: () => order.push('flush') } }
          : { isWebGPUBackend: true };
      }
      async init() {}
      hasFeature() { return false; }
      setPixelRatio() {}
      compute(node) { order.push(`compute ${node}`); }
      async computeAsync(node) { order.push(`computeAsync ${node}`); }
    }
    const gpu = await createGpu({ canvas: {}, load: async () => ({ T: { WebGPURenderer: Renderer }, S: {} }) });
    assert.equal(gpu.tier, tier);
    gpu.renderer.compute('a');
    await gpu.renderer.computeAsync('b');
    if (tier === 'webgl2') {
      assert.deepEqual(order, ['compute a', 'flush', 'computeAsync b', 'flush'], 'a flush right after each pass');
    } else {
      assert.deepEqual(order, ['compute a', 'computeAsync b'], 'no GL to flush');
    }
  }
  assert.equal(typeof flushAfterCompute, 'function');
});

// Firefox notices finished GPU work (the canvas's copy, a map) only when the device is polled: at a
// submit or on its 100 ms timer; a frame whose GPU work outlasts a display frame then waits for the
// timer (3.11: 20 a second at 19.6 ms of GPU). Firefox's WebGPU tier submits an empty list while it
// draws; no other browser does (4.2 review), and a lost device ends it for good.
await ok('the_webgpu_tier_polls_its_device_with_empty_submits_while_it_draws_and_stops_when_idle', async () => {
  for (const [tier, firefox, lose] of [['webgpu', true, false], ['webgl2', true, false], ['webgpu', false, false], ['webgpu', true, true]]) {
    const submits = [];
    let loseDevice = null;
    const timers = {
      t: 0, fns: new Map(), id: 0,
      performance: { now: () => timers.t },
      setInterval(fn, ms) { this.id++; this.fns.set(this.id, { fn, ms, at: this.t + ms }); return this.id; },
      clearInterval(id) { this.fns.delete(id); },
      /** Run the clock `ms` on, firing each interval as it falls due. */
      run(ms) {
        const end = this.t + ms;
        for (;;) {
          const due = [...this.fns.entries()].filter(([, f]) => f.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
          if (!due) break;
          this.t = due[1].at;
          due[1].at += due[1].ms;
          due[1].fn();
        }
        this.t = end;
      },
    };
    class Renderer {
      constructor() {
        const device = { queue: { submit: (list) => submits.push(list.length) } };
        if (lose) device.lost = new Promise((resolve) => { loseDevice = resolve; });
        this.backend = tier === 'webgl2' ? { isWebGPUBackend: false, gl: { flush() {} } }
          : { isWebGPUBackend: true, device };
      }
      async init() {}
      hasFeature() { return false; }
      setPixelRatio() {}
      render() {}
      compute() {}
      dispose() {}
    }
    const gpu = await createGpu({
      canvas: {}, timers, firefox, load: async () => ({ T: { WebGPURenderer: Renderer }, S: {} }),
    });
    timers.run(100);
    assert.equal(submits.length, 0, `${tier}: nothing before the first draw`);
    // a frame of draws, then the wait for the next one (a display frame and a half)
    gpu.render({}, {});
    gpu.renderer.compute({});
    timers.run(25);
    if (tier === 'webgl2') {
      assert.equal(submits.length, 0, 'the WebGL2 tier has no device to poll');
      continue;
    }
    if (!firefox) {
      assert.equal(submits.length, 0, 'a WebGPU browser other than Firefox is not polled');
      continue;
    }
    if (lose) {
      assert.ok(submits.length > 0, 'polled until the loss');
      loseDevice({ reason: 'unknown', message: 'gone' });
      await null;
      await null;
      const atLoss = submits.length;
      gpu.render({}, {});
      timers.run(100);
      assert.equal(submits.length, atLoss, 'no submit to a lost device, even while it draws');
      assert.equal(timers.fns.size, 0, 'the interval is cleared at the loss');
      continue;
    }
    assert.ok(submits.length >= 5 && submits.every((n) => n === 0), `empty submits while it waits: ${submits}`);
    // the draws stop (the page hidden): the polling stops a quarter second on, and stays stopped
    timers.run(POLL.idleMs + 10);
    const stopped = submits.length;
    timers.run(1000);
    assert.equal(submits.length, stopped, 'no polling once the draws stopped');
    assert.equal(timers.fns.size, 0, 'the interval is cleared');
    // drawing again starts it again; dispose stops it at once
    gpu.render({}, {});
    timers.run(10);
    assert.ok(submits.length > stopped);
    gpu.dispose();
    const disposed = submits.length;
    timers.run(50);
    assert.equal(submits.length, disposed, 'dispose stops the polling');
  }
});

// 3.11: the tier is the faster of the two, measured once per browser (its version), GPU and size
// class and kept; the Renderer row chooses instead.
await ok('the_tier_pick_keys_compares_and_decides', async () => {
  assert.deepEqual([[960, 540], [1728, 972], [1920, 1080], [3840, 2160]].map(([w, h]) => sizeClass(w, h, 1)), [-2, 1, 2, 4]);
  assert.equal(sizeClass(390, 844, 3), sizeClass(390 * 1.75, 844 * 1.75, 1), 'a phone: its backing pixels');
  const key = pickKey({ ua: 'Firefox/160.0', gpu: 'radeon', w: 1728, h: 972, density: 1 });
  assert.notEqual(key, pickKey({ ua: 'Firefox/161.0', gpu: 'radeon', w: 1728, h: 972, density: 1 }), 'a browser update');
  assert.notEqual(key, pickKey({ ua: 'Firefox/160.0', gpu: 'intel', w: 1728, h: 972, density: 1 }), 'another GPU');
  assert.notEqual(key, pickKey({ ua: 'Firefox/160.0', gpu: 'radeon', w: 3840, h: 2160, density: 1 }), 'another size class');
  assert.equal(key, pickKey({ ua: 'Firefox/160.0', gpu: 'radeon', w: 1700, h: 960, density: 1 }), 'the same class');
  const kept = keepPick(null, { key, tier: 'webgl2', fps: {} });
  assert.equal(keptPick(kept, key), 'webgl2');
  assert.equal(keptPick({ ...kept, v: 0 }, key), null);
  assert.equal(keptPick(kept, `${key}x`), null);
  assert.equal(keptPick({ v: PROBE.version, picks: [{ key, tier: 'none' }] }, key), null);
  // the last four keys are kept; a key measured again moves to the end with its new tier
  let many = null;
  for (const k of ['a', 'b', 'c', 'd', 'e']) many = keepPick(many, { key: k, tier: 'webgpu', fps: {} });
  assert.deepEqual(many.picks.map((p) => p.key), ['b', 'c', 'd', 'e']);
  many = keepPick(many, { key: 'c', tier: 'webgl2', fps: {} });
  assert.deepEqual(many.picks.map((p) => p.key), ['b', 'd', 'e', 'c']);
  assert.equal(keptPick(many, 'c'), 'webgl2');
  assert.equal(keptPick(many, 'a'), null, 'the oldest is gone');
  assert.equal(fastest({ webgpu: 20, webgl2: 41 }), 'webgl2');
  assert.equal(fastest({ webgpu: 38.6, webgl2: 40.1 }), 'webgpu', 'within the tie WebGPU stays');
  assert.equal(fastest({ webgpu: 60, webgl2: 30 }), 'webgpu');
  assert.equal(fastest({ webgpu: null, webgl2: 30 }), 'webgl2');
  assert.equal(fastest({ webgpu: 30, webgl2: null }), 'webgpu');
  assert.equal(fastest({}), null);
  assert.deepEqual(decide({ renderer: 'webgl2', kept, key }), { tier: 'webgl2', probe: false });
  assert.deepEqual(decide({ renderer: 'webgpu', kept, key }), { tier: 'webgpu', probe: false });
  assert.deepEqual(decide({ kept, key }), { tier: 'webgl2', probe: false });
  assert.deepEqual(decide({ kept, key: 'other' }), { tier: null, probe: true });
  assert.deepEqual(decide({ key, hasAdapter: false }), { tier: 'webgpu', probe: false });
  assert.deepEqual(decide({ key, automated: true }), { tier: 'webgpu', probe: false });
  assert.deepEqual(decide({ key, automated: true, asked: true }), { tier: null, probe: true });
});

/** A page for pickTier: a canvas of `w` by `h`, a WebGPU adapter, a display at 60 a second, prefs
 * in a map. `slowTier` draws its frames 2 display frames apart (as a tier the GPU cannot keep up
 * with). */
function tierPage({
  w = 1728, h = 972, ua = 'Firefox/160.0', prefs = {}, search = '', slowTier = null,
  stallOn = () => false, hideOn = () => false, noTier = false,
} = {}) {
  const seen = { made: [], live: 0, measured: 0, lost: 0, destroyed: 0 };
  let now = 0;
  let drawing = null;
  let frames = 0;
  const listeners = [];
  const win = {
    location: { search },
    devicePixelRatio: 1,
    navigator: { userAgent: ua, gpu: { requestAdapter: async () => ({ info: { vendor: 'amd', architecture: 'rdna-2' } }) } },
    requestAnimationFrame(fn) {
      // the slow tier's frames take two display frames
      now += drawing === slowTier ? 1000 / 30 : 1000 / 60;
      frames++;
      // a stall (a compile, a collection) of 3 s in the count, on the probe canvas `stallOn` names
      if (frames === 40 && stallOn(seen.made.length)) now += 3000;
      // a tab switched away and back between two frames: only the event says so
      if (frames === 40 && hideOn(seen.made.length)) {
        doc.hidden = true;
        for (const f of listeners) f();
        doc.hidden = false;
      }
      setImmediate(() => fn(now));
      return 1;
    },
  };
  const doc = {
    defaultView: win,
    hidden: false,
    addEventListener: (name, fn) => { if (name === 'visibilitychange') listeners.push(fn); },
    removeEventListener: (name, fn) => { const i = listeners.indexOf(fn); if (i >= 0) listeners.splice(i, 1); },
    createElement: () => {
      const el = {
        ownerDocument: doc, style: {}, setAttribute() {}, getContext: () => null,
        remove() { seen.live--; },
      };
      return el;
    },
  };
  const canvas = {
    ownerDocument: doc, clientWidth: w, clientHeight: h,
    after(el) { seen.live++; seen.made.push(el); frames = 0; },
  };
  const createGpu = async ({ forceWebGL }) => {
    const tier = noTier ? 'none' : forceWebGL ? 'webgl2' : 'webgpu';
    const lose = { loseContext() { seen.lost++; } };
    return {
      tier,
      renderer: tier === 'webgl2' ? { backend: { gl: { getExtension: (n) => (n === 'WEBGL_lose_context' ? lose : null) } } } : null,
      size() {},
      compile: async () => {},
      dispose() { seen.destroyed++; },
    };
  };
  const makeLayers = (gpu) => ({
    ctx: { scene: {}, camera: {} },
    update() {},
    draw() { drawing = gpu.tier; seen.measured++; },
    dispose() {},
  });
  const store = { ...prefs };
  return {
    seen, win, store,
    parts: {
      prefs: { get: (k, f) => (k in store ? store[k] : f), set: (k, v) => { store[k] = v; } },
      canvas, createGpu, makeLayers, load: async () => ({}), levels: { back: 1 }, seed: 7,
    },
  };
}

await ok('the_tier_probe_measures_both_keeps_the_faster_and_reads_it_back', async () => {
  // WebGPU at 30 a second, WebGL2 at 60: WebGL2 is kept, its probe canvases gone
  const first = tierPage({ slowTier: 'webgpu' });
  const got = await pickTier(first.parts);
  assert.equal(got.tier, 'webgl2');
  assert.equal(got.why, 'faster');
  near(got.pick.fps.webgpu, 30, 1.5, 'webgpu draws a second');
  near(got.pick.fps.webgl2, 60, 1.5, 'webgl2 draws a second');
  assert.equal(first.seen.made.length, 2, 'one canvas per tier');
  assert.equal(first.seen.live, 0, 'both let go');
  assert.equal(first.store['renderer-pick'].picks[0].tier, 'webgl2');
  assert.equal(first.seen.lost, 1, 'the WebGL2 probe lost its context');
  assert.equal(first.seen.destroyed, 2, 'both gpus let go');
  // the next visit: the kept pick, no measure
  const again = tierPage({ prefs: first.store });
  assert.deepEqual(await pickTier(again.parts), { tier: 'webgl2', why: 'faster', pick: null });
  assert.equal(again.seen.measured, 0);
  // a browser update measures again; so does another size class
  const updated = tierPage({ prefs: first.store, ua: 'Firefox/161.0' });
  assert.equal((await pickTier(updated.parts)).tier, 'webgpu', 'both at 60: WebGPU');
  assert.ok(updated.seen.measured > 0);
  const big = tierPage({ prefs: first.store, w: 3840, h: 2160, slowTier: 'webgl2' });
  assert.equal((await pickTier(big.parts)).tier, 'webgpu');
  // the row chooses: no measure, whatever is kept
  const chosen = tierPage({ prefs: { ...first.store, renderer: 'webgpu' } });
  assert.deepEqual(await pickTier(chosen.parts), { tier: 'webgpu', why: null, pick: null });
  const floor = tierPage({ prefs: { renderer: 'webgl2' } });
  assert.deepEqual(await pickTier(floor.parts), { tier: 'webgl2', why: 'chosen', pick: null });
  assert.equal(chosen.seen.measured + floor.seen.measured, 0);
  // a check's browser measures only when the address asks
  const driven = tierPage({ slowTier: 'webgpu' });
  driven.win.navigator.webdriver = true;
  assert.deepEqual(await pickTier(driven.parts), { tier: 'webgpu', why: null, pick: null });
  const asked = tierPage({ search: '?vg-probe', slowTier: 'webgpu' });
  asked.win.navigator.webdriver = true;
  assert.equal((await pickTier(asked.parts)).tier, 'webgl2');
});

await ok('the_tier_probe_re_measures_a_disturbed_count_and_keeps_nothing_still_disturbed', async () => {
  // a clean run saves
  const clean = tierPage({ slowTier: 'webgpu' });
  await pickTier(clean.parts);
  assert.equal(clean.seen.made.length, 2);
  assert.equal(clean.store['renderer-pick'].picks.length, 1, 'a clean run keeps its pick');
  // a 3 s gap in the first count: that tier is measured once more, clean, and the pick is kept
  const gap = tierPage({ stallOn: (n) => n === 1 });
  const got = await pickTier(gap.parts);
  assert.equal(gap.seen.made.length, 3, 'WebGPU measured twice, WebGL2 once');
  assert.equal(got.tier, 'webgpu');
  near(got.pick.fps.webgpu, 60, 1.5, 'the second count is the one used');
  assert.equal(gap.store['renderer-pick'].picks.length, 1, 'kept');
  // the page hidden and shown within the count (the event; no frame saw it): the same
  const hid = tierPage({ hideOn: (n) => n === 2 });
  await pickTier(hid.parts);
  assert.equal(hid.seen.made.length, 3, 'WebGL2 measured twice');
  assert.equal(hid.store['renderer-pick'].picks.length, 1);
  // still disturbed on the second go: the pick stands for this visit, nothing is kept
  const bad = tierPage({ stallOn: (n) => n === 1 || n === 2, slowTier: 'webgpu' });
  const again = await pickTier(bad.parts);
  assert.equal(again.tier, 'webgl2');
  assert.equal(bad.seen.made.length, 3 + 0, 'WebGPU twice, WebGL2 once');
  assert.equal('renderer-pick' in bad.store, false, 'nothing kept: the next visit measures again');
  const hidden = tierPage({ hideOn: (n) => n === 1 || n === 2 });
  await pickTier(hidden.parts);
  assert.equal('renderer-pick' in hidden.store, false, 'hidden twice: nothing kept');
  assert.equal(hidden.seen.live, 0, 'every probe canvas let go');
  // neither tier drew: WebGPU, no reason, nothing kept
  const none = tierPage({ noTier: true });
  assert.deepEqual(await pickTier(none.parts), { tier: 'webgpu', why: null, pick: null });
  assert.equal('renderer-pick' in none.store, false);
});

await ok('the_tier_probe_aborted_lets_its_gpu_go_after_three_undrawn_frames', async () => {
  // aborted in the count: the canvas goes with the page, the gpu three frames later, never at once
  const run = tierPage();
  const ctl = new AbortController();
  const events = [];
  const rafOf = run.win.requestAnimationFrame;
  let drawn = 0;
  const makeLayers = run.parts.makeLayers;
  run.parts.makeLayers = (gpu) => {
    const l = makeLayers(gpu);
    const draw = l.draw;
    l.draw = () => { draw(); drawn++; if (drawn === 20) { ctl.abort(); events.push('abort'); } else events.push('draw'); };
    return l;
  };
  run.win.requestAnimationFrame = (fn) => { events.push('frame'); return rafOf(fn); };
  const orig = run.parts.createGpu;
  run.parts.createGpu = async (o) => {
    const g = await orig(o);
    const d = g.dispose;
    g.dispose = () => { events.push('dispose'); d(); };
    return g;
  };
  await pickTier({ ...run.parts, signal: ctl.signal });
  const at = events.indexOf('abort');
  const gone = events.indexOf('dispose');
  assert.ok(at >= 0 && gone > at, 'disposed after the abort');
  assert.ok(events.slice(at, gone).filter((e) => e === 'frame').length >= 3, 'three display frames between');
  assert.ok(!events.slice(at, gone).includes('draw'), 'none of them drawn');
});

await ok('a_scene_aborted_lets_its_gpu_go_after_three_undrawn_frames', async () => {
  let frame = null;
  const win = {
    requestAnimationFrame: (fn) => { frame = fn; return 1; },
    setTimeout: () => 1,
    clearTimeout() {},
  };
  const events = [];
  const gpu = { dispose() { events.push('dispose'); } };
  disposeAfterFrames(gpu, win);
  assert.deepEqual(events, [], 'not at once');
  for (let i = 0; i < 2; i++) { const f = frame; f(); assert.deepEqual(events, [], `after frame ${i + 1}`); }
  frame();
  assert.deepEqual(events, ['dispose'], 'on the third');
  // where frames do not come (a hidden page) the timer lets it go; a second call does nothing
  let timer = null;
  const hiddenWin = { requestAnimationFrame: () => 1, setTimeout: (f) => { timer = f; return 2; }, clearTimeout() {} };
  const g2 = { n: 0, dispose() { this.n++; } };
  disposeAfterFrames(g2, hiddenWin);
  timer();
  timer();
  assert.equal(g2.n, 1);
});

console.log(`voidgarden-scene: ${n} ok`);
process.exit(0);
