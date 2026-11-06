/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/lifecycle.js and core/analysis.js: a canvas loop that runs only while its
// canvas is on show, follows its size, holds a long gap to a tenth of a second and draws one still
// frame for reduced motion (or, with `reduced: 'theme'`, keeps running), and draws at a pace
// (a timer for 100 ms or more, `dt` held to twice the pace, one timer or frame outstanding even
// when the pace changes); and the analysis frames, one stream for every holder, opened by the
// first and closed by the last, a late holder told what the stream said, a gone stream asked for
// again with a doubling wait. The browser's frames, observers and streams are fakes.
// Run: node core-loop.mjs
import assert from 'node:assert/strict';
import { createAnalysis } from '../../../bins/svolderd/web/core/analysis.js';
import { canvasLoop } from '../../../bins/svolderd/web/core/lifecycle.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

// The browser's frames and timers, run when the test says.
let frames = new Map();
let frameId = 0;
let timers = [];
let now = 0;
let seq = 0;
globalThis.requestAnimationFrame = (fn) => { frameId++; frames.set(frameId, fn); return frameId; };
globalThis.cancelAnimationFrame = (id) => { frames.delete(id); };
globalThis.setTimeout = (fn, ms) => { seq++; timers.push({ id: seq, at: now + ms, fn }); return seq; };
globalThis.clearTimeout = (id) => { timers = timers.filter((t) => t.id !== id); };
const frame = (ms) => {
  const [id, fn] = [...frames][0] ?? [];
  if (id === undefined) return false;
  frames.delete(id);
  fn(ms);
  return true;
};
const advance = (ms) => {
  const end = now + ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= end).sort((a, b) => a.at - b.at)[0];
    if (!due) break;
    timers = timers.filter((t) => t !== due);
    now = due.at;
    due.fn();
  }
  now = end;
};

function loop({ reduced = false, width = 400, height = 100, mode, pace } = {}) {
  frames = new Map();
  const canvas = { clientWidth: width, clientHeight: height };
  const log = { draws: [], resizes: [] };
  let watch = null;
  const motion = {
    matches: reduced,
    listeners: new Set(),
    addEventListener(_, fn) { motion.listeners.add(fn); },
    removeEventListener(_, fn) { motion.listeners.delete(fn); },
  };
  const stops = { watch: 0 };
  const loopOf = canvasLoop({
    canvas,
    draw: (dt, at) => log.draws.push([dt, at]),
    resize: (w, h, dpr) => log.resizes.push([w, h, dpr]),
    watchVisible: (options) => { watch = options; return { stop() { stops.watch++; } }; },
    ...(mode === undefined ? {} : { reduced: mode }),
    ...(pace === undefined ? {} : { pace }),
    matchMedia: (query) => {
      assert.equal(query, '(prefers-reduced-motion: reduce)');
      return motion;
    },
  });
  return { loop: loopOf, canvas, log, watch: () => watch, motion, stops };
}

ok('a_loop_runs_only_while_its_canvas_is_on_show', () => {
  const { loop: l, log, watch } = loop();
  l.start();
  assert.equal(l.running, false, 'nothing runs before it is on show');
  watch().onVisible(true);
  assert.equal(l.running, true);
  frame(1000);
  frame(1016);
  assert.deepEqual(log.draws.map(([dt]) => Math.round(dt * 1000)), [0, 16], 'the first has no step');
  assert.deepEqual(log.draws[1][1], 1.016, 'now is in seconds');
  watch().onVisible(false);
  assert.equal(l.running, false);
  assert.equal(frames.size, 0, 'the frame asked for was taken back');
  watch().onVisible(true);
  frame(5000);
  assert.equal(log.draws.at(-1)[0], 0, 'a pause does not arrive as a jump');
  frame(9000);
  assert.equal(log.draws.at(-1)[0], 0.1, 'a long gap is held to a tenth of a second');
  l.stop();
  assert.equal(l.running, false);
  assert.equal(frames.size, 0);
});

ok('the_size_is_told_at_first_and_when_it_changes', () => {
  const { loop: l, log, watch, canvas } = loop({ width: 300, height: 80 });
  globalThis.devicePixelRatio = 2;
  l.start();
  watch().onVisible(true);
  frame(1000);
  frame(1016);
  assert.deepEqual(log.resizes, [[300, 80, 2]]);
  canvas.clientWidth = 320;
  frame(1032);
  assert.deepEqual(log.resizes, [[300, 80, 2], [320, 80, 2]]);
  assert.equal(log.draws.length, 3);
  delete globalThis.devicePixelRatio;
});

ok('reduced_motion_draws_one_still_frame_and_runs_no_loop', () => {
  const { loop: l, log, watch, motion } = loop({ reduced: true });
  assert.equal(l.reduced, true);
  l.start();
  watch().onVisible(true);
  assert.equal(l.running, false);
  assert.equal(log.draws.length, 1, 'one still frame');
  assert.equal(log.draws[0][0], 0);
  assert.equal(frames.size, 0);
  // The system's setting changes while it is on show: the loop begins.
  motion.matches = false;
  for (const fn of motion.listeners) fn();
  assert.equal(l.running, true);
  frame(2000);
  assert.equal(log.draws.length, 2);
  motion.matches = true;
  for (const fn of motion.listeners) fn();
  assert.equal(l.running, false);
  assert.equal(log.draws.length, 3, 'and a still frame again');
  l.stop();
  assert.equal(motion.listeners.size, 0);
});

ok('reduced_still_is_as_before', () => {
  const { loop: l, log, watch } = loop({ reduced: true, mode: 'still' });
  l.start();
  watch().onVisible(true);
  assert.equal(l.running, false);
  assert.equal(log.draws.length, 1, 'one still frame');
  assert.equal(frames.size, 0);
  l.stop();
});

ok('reduced_theme_keeps_the_loop_running_under_reduced_motion', () => {
  const { loop: l, log, watch, motion } = loop({ reduced: true, mode: 'theme' });
  assert.equal(l.reduced, true, 'the theme reads it from the loop');
  l.start();
  watch().onVisible(true);
  assert.equal(l.running, true);
  assert.equal(log.draws.length, 0, 'no still frame: the loop draws');
  frame(1000);
  frame(1016);
  assert.deepEqual(log.draws.map(([dt]) => Math.round(dt * 1000)), [0, 16]);
  motion.matches = false;
  for (const fn of motion.listeners) fn();
  assert.equal(l.running, true, 'and goes on when the setting changes');
  assert.equal(l.reduced, false);
  l.stop();
  assert.equal(l.running, false);
  assert.equal(frames.size, 0);
});

/** Frames the browser would give: one after another, 16 ms apart, until `ms` of fake time pass. */
function run(ms, step = 16) {
  const end = now + ms;
  while (now < end) {
    advance(Math.min(step, end - now));
    if (frames.size) frame(now);
  }
}

ok('a_pace_of_1000_draws_once_a_second', () => {
  timers = [];
  now = 0;
  const { loop: l, log, watch } = loop({ pace: () => 1000 });
  l.start();
  watch().onVisible(true);
  run(5000);
  assert.ok(Math.abs(log.draws.length - 5) <= 1, `${log.draws.length} draws in 5 s`);
  l.stop();
});

ok('a_pace_waits_on_a_timer_not_on_frames', () => {
  timers = [];
  now = 0;
  const { loop: l, log, watch } = loop({ pace: () => 500 });
  let asked = 0;
  const ask = globalThis.requestAnimationFrame;
  globalThis.requestAnimationFrame = (fn) => { asked++; return ask(fn); };
  l.start();
  watch().onVisible(true);
  assert.equal(asked, 1, 'one frame to begin');
  frame(now);
  assert.equal(log.draws.length, 1);
  asked = 0;
  assert.equal(frames.size, 0, 'no frame is waiting while the pace waits');
  advance(499);
  assert.equal(asked, 0);
  advance(1);
  assert.equal(asked, 1, 'the timer asks for one frame');
  frame(now);
  assert.equal(log.draws.length, 2);
  l.stop();
  assert.equal(timers.length, 0, 'stopping takes the timer back');
  globalThis.requestAnimationFrame = ask;
});

ok('a_pace_below_100_ms_draws_at_every_frame', () => {
  timers = [];
  now = 0;
  const { loop: l, log, watch } = loop({ pace: () => 50 });
  l.start();
  watch().onVisible(true);
  frame(1000);
  frame(1016);
  frame(1032);
  assert.equal(log.draws.length, 3);
  l.stop();
});

ok('a_pace_holds_dt_to_twice_the_pace', () => {
  timers = [];
  now = 0;
  const { loop: l, log, watch } = loop({ pace: () => 1000 });
  l.start();
  watch().onVisible(true);
  frame(1000);
  advance(1000);
  frame(3500);
  assert.equal(log.draws.at(-1)[0], 2, 'a gap of 2.5 s is held to 2 s');
  advance(1000);
  frame(4500);
  assert.equal(log.draws.at(-1)[0], 1, 'a step of one pace is told whole');
  l.stop();
});

ok('a_pace_change_between_draws_leaves_one_handle', () => {
  timers = [];
  now = 0;
  let wait = 1000;
  const { loop: l, log, watch } = loop({ pace: () => wait });
  const outstanding = () => timers.length + frames.size;
  l.start();
  watch().onVisible(true);
  frame(now);
  assert.equal(outstanding(), 1, 'one timer after a draw at 1000');
  wait = 0;
  advance(1000);
  frame(now);
  assert.equal(outstanding(), 1, 'one frame after the pace went to every frame');
  assert.equal(timers.length, 0);
  const drawn = log.draws.length;
  frame(now + 16);
  frame(now + 32);
  assert.equal(log.draws.length, drawn + 2, 'every frame again');
  assert.equal(outstanding(), 1);
  wait = 1000;
  frame(now + 48);
  assert.equal(outstanding(), 1, 'one timer after the pace went to 1000');
  assert.equal(frames.size, 0);
  const before = log.draws.length;
  advance(999);
  assert.equal(log.draws.length, before);
  advance(1);
  frame(now);
  assert.equal(log.draws.length, before + 1, 'draws resume at the new rate');
  l.stop();
  assert.equal(outstanding(), 0);
});

ok('a_hidden_canvas_draws_nothing_whatever_the_pace', () => {
  timers = [];
  now = 0;
  const { loop: l, log, watch } = loop({ pace: () => 300 });
  l.start();
  watch().onVisible(true);
  frame(1000);
  assert.equal(log.draws.length, 1);
  watch().onVisible(false);
  assert.equal(timers.length, 0, 'the timer is taken back');
  advance(5000);
  assert.equal(frames.size, 0);
  assert.equal(log.draws.length, 1);
  l.stop();
});

ok('stopping_stops_watching', () => {
  const { loop: l, watch, stops } = loop();
  l.start();
  watch().onVisible(true);
  l.stop();
  assert.equal(stops.watch, 1);
  l.start();
  l.stop();
  assert.equal(stops.watch, 2, 'it can start again');
});

class FakeStream {
  static all = [];

  constructor(url) {
    this.url = url;
    this.readyState = 1;
    this.listeners = {};
    FakeStream.all.push(this);
  }

  addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); }

  close() { this.readyState = FakeStream.CLOSED; }

  send(name, data) {
    for (const fn of this.listeners[name] ?? []) fn({ data });
  }
}
FakeStream.CLOSED = 2;

ok('one_stream_serves_every_holder_and_closes_with_the_last', () => {
  FakeStream.all = [];
  const analysis = createAnalysis({ EventSource: FakeStream });
  assert.equal(analysis.holders, 0);
  const a = { hello: [], capture: [], frame: [] };
  const b = { hello: [], capture: [], frame: [] };
  const heardBy = (log) => ({
    hello: (d) => log.hello.push(d.now_ms),
    capture: (d) => log.capture.push(d.capture),
    frame: (text) => log.frame.push(text),
  });
  const first = analysis.open(heardBy(a));
  assert.equal(FakeStream.all.length, 1);
  assert.equal(FakeStream.all[0].url, 'api/v1/host/frames');
  FakeStream.all[0].send('hello', JSON.stringify({ now_ms: 5, capture: 'starting' }));
  FakeStream.all[0].send('capture', JSON.stringify({ capture: 'running', sink: 'Out' }));
  const second = analysis.open(heardBy(b));
  assert.equal(FakeStream.all.length, 1, 'the same stream');
  assert.equal(analysis.holders, 2);
  assert.deepEqual(b.hello, [5], 'a late holder is told the last hello');
  assert.deepEqual(b.capture, ['running'], 'and the last capture state');
  FakeStream.all[0].send('frame', '[1,2,3]');
  assert.deepEqual(a.frame, ['[1,2,3]']);
  assert.deepEqual(b.frame, ['[1,2,3]'], 'a frame goes as it came');
  FakeStream.all[0].send('hello', 'not json');
  FakeStream.all[0].send('hello', JSON.stringify({ now_ms: 'x' }));
  assert.deepEqual(a.hello, [5], 'a hello with no clock says nothing');
  first.close();
  first.close();
  assert.equal(analysis.holders, 1);
  assert.equal(FakeStream.all[0].readyState, 1, 'still open for the one left');
  second.close();
  assert.equal(FakeStream.all[0].readyState, FakeStream.CLOSED);
  analysis.open({}).close();
  assert.equal(FakeStream.all.length, 2, 'a new first holder opens a new stream');
});

ok('a_holder_that_throws_does_not_stop_the_others', () => {
  FakeStream.all = [];
  const analysis = createAnalysis({ EventSource: FakeStream });
  const heard = [];
  const error = console.error;
  console.error = () => {};
  analysis.open({ frame: () => { throw new Error('a drawing that broke'); } });
  analysis.open({ frame: (text) => heard.push(text) });
  FakeStream.all[0].send('hello', JSON.stringify({ now_ms: 1 }));
  analysis.open({ hello: () => { throw new Error('late'); }, frame: (text) => heard.push(text) });
  FakeStream.all[0].send('frame', 'x');
  console.error = error;
  assert.deepEqual(heard, ['x', 'x']);
});

ok('a_stream_the_browser_gave_up_on_is_asked_for_again_with_a_doubling_wait', () => {
  FakeStream.all = [];
  timers = [];
  const analysis = createAnalysis({ EventSource: FakeStream });
  const held = analysis.open({});
  const gave = (stream) => { stream.readyState = FakeStream.CLOSED; stream.send('error'); };
  FakeStream.all[0].readyState = 0;
  FakeStream.all[0].send('error');
  advance(60000);
  assert.equal(FakeStream.all.length, 1, 'an interrupted stream is the browser\'s to retry');
  gave(FakeStream.all[0]);
  advance(2999);
  assert.equal(FakeStream.all.length, 1);
  advance(1);
  assert.equal(FakeStream.all.length, 2, 'after 3 s');
  gave(FakeStream.all[1]);
  advance(5999);
  assert.equal(FakeStream.all.length, 2);
  advance(1);
  assert.equal(FakeStream.all.length, 3, 'after 6 s');
  FakeStream.all[2].send('open');
  gave(FakeStream.all[2]);
  advance(3000);
  assert.equal(FakeStream.all.length, 4, 'a good open starts the wait over');
  held.close();
  gave(FakeStream.all[3]);
  advance(60000);
  assert.equal(FakeStream.all.length, 4, 'nobody holds it, nobody asks again');
});

console.log(`${n} core-loop checks passed`);
